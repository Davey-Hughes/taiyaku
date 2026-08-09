# Continuous integration

Workflows live in `.forgejo/workflows/`. Forgejo looks there first and falls back
to `.github/workflows/`; using the Forgejo path keeps the intent unambiguous.

## Jobs

| Job | Runs |
| --- | ---- |
| `lint` | `cargo fmt --check`, `cargo clippy -D warnings` |
| `test` | `cargo test --workspace` |
| `wasm` | builds `taiyaku-core` and `taiyaku-wasm` for `wasm32-unknown-unknown`, builds the browser package with `wasm-pack`, and runs `scripts/smoke-wasm.mjs` against the compiled module |
| `web` | `npm ci`, `biome check` (formatting **and** lint), `tsc -b` typecheck, `vitest run` in `web/` |

The `wasm` job is the one worth keeping even when it feels redundant. The design
depends on the core being shareable between the browser reader and a future
native shell, and that property is easy to destroy accidentally — a single
dependency that assumes threads or links a native library will do it. Without a
gate, the breakage stays invisible until the reader is being built, by which
point the offending dependency is load-bearing.

## The local mirror, and the one thing it cannot mirror

`scripts/check-all.sh` reproduces all four jobs above, in the same order with
the same flags, so the local gate and the CI gate cannot drift. That claim was
false between M5a and 2026-08-07: the script said "three jobs" and reproduced
three while CI had grown a fourth, so **a TypeScript change could take the local
gate green having had nothing run against it at all**. The `wasm` job had
drifted the narrower way over the same period — its two `cargo build`s were
mirrored, but not the `wasm-pack` package build or the smoke run, which is the
only thing anywhere that *executes* the generated module.

Skips are named twice, once where they happen and again in a closing summary
that refuses to print a bare `all green` when anything was skipped:

```
all green, except NOT MEASURED:
  - standing corpus checks (TAIYAKU_CORPUS unset)
  - the wasm job (--no-wasm)
  - the web job (web/node_modules absent — run `npm --prefix web ci`)
```

The corpus checks are the one thing the script measures that the forge cannot,
since CI has no corpus. Everything else runs in both places — with one
exception.

**Setting `TAIYAKU_CORPUS` costs far more than the corpus job itself, and two
source files claimed otherwise until 2026-08-08.** The script exports
`TAIYAKU_CORPUS` before its `test` job, and that job is an *unfiltered*
`cargo test --workspace --all-features` — the `corpus::` filter belongs to the
separate corpus job below it. So a local gate run with a corpus present also
runs the two one-off measurement modules, `geometry_experiment` and
`region_experiment`, in the **debug** profile. `geometry_experiment`
fingerprints roughly 1.4 million Lanczos3 crops: about a minute under
`--release` and about forty without. Measured rather than estimated — a gate
run was killed at **1h09m** still inside that one test.
`region_experiment` costs about a minute there and is not the problem. Both
modules' headers said `check-all.sh` "does not run this", reaching for the
`corpus::` filter and missing the job above it; both are corrected.
CI is unaffected, having no corpus. For a fast pass that mirrors CI exactly,
run the script *without* the variable and take the corpus job separately:

```sh
env -u TAIYAKU_CORPUS scripts/check-all.sh
TAIYAKU_CORPUS="$PWD/corpus" cargo test --workspace --all-features corpus:: \
  -- --nocapture --test-threads=1
```

The two modules are deliberately not filtered out of the `test` job: a one-off
measurement that silently stops being compiled and run is a worse failure than
a slow gate, and the release-profile invocation each module's header names is
how they are meant to be run on purpose.

**`npm ci` is not mirrored, and that is a real gap rather than an oversight.**
CI runs it to build the job's `node_modules` from nothing; locally it would
delete a working install and rebuild it on every gate run. What that costs is
the one property `npm ci` checks that `lint`, `typecheck` and `test` do not:
that `package.json` and `package-lock.json` agree. A lockfile that has drifted
out of sync passes `check-all.sh` and fails the forge. Run
`npm --prefix web ci` yourself after changing dependencies.

## Formatting, and the tool that checks it

Until 2026-08-07 the TypeScript half had **no formatting check at all**, which
was measured rather than assumed: a deliberately mangled exported function
(`(   a:number,b:number ):number{`) passed `oxlint`, `tsc -b` and all 414 tests.
The Rust half has always been checked — `cargo fmt --all -- --check` exits 1 on
mangled Rust, also measured — so the gap was one-sided and nothing said so.

`biome` now covers both halves of the `web` job's first step: `npm run lint` is
`biome check ./src`, which is formatting *and* lint in one command, so the CI
step needed no change. `oxlint` is gone, along with `.oxlintrc.json`.

**The config file is `biome.jsonc`, and the extension is load-bearing.** A
`biome.json` containing comments is silently discarded — biome falls back to its
own defaults (tabs, double quotes, semicolons) and emits **no warning and no
error**. That was measured here the expensive way: the first version of this
config was `biome.json` with its rationale in comments, and every run used
defaults while appearing to work. The A/B is clean — same file, comments
stripped, config applied. Since the rationale is the point of the file, the file
is `.jsonc`.

**Four rules are off**, listed with their sites in `biome.jsonc` itself. The
branch that adopted biome swapped one tool for another and deliberately did not
change what is enforced; each of the four is a rule `oxlint` never applied, so
turning it on would have been a second claim inside a tooling commit. Three of
them stay open questions about taste, and the fourth does not.

**`noArrayIndexKey` is the one that must stay off**, and it is worth knowing
why, because the branch that turned it off originally recorded the opposite —
"the one worth returning to", a possible defect wanting investigation. It was
investigated, and the index key at `App.tsx`'s scroll mode is the *mechanism*
rather than an oversight. Reconciling by position is what keeps `decode.ts`'s
gate alive across a page turn; a key derived from the slot remounts `PageView`
whenever slots shift, and a freshly-mounted `PageView` has nothing decoded.
`renderGroup` says so where it happens, and "The flicker, before and after" in
`notes.md` measured it: **3 of 79 animation-frame samples landed mid-load
without the gate, 0 of 83 with it.** So the rule's fix would undo a measured
improvement.

The rule's own premise fails here too. Its bug needs state that survives a
shift onto the wrong item, and nothing under that key has any: the wrapper
`<div>` holds none, `PageView` resets from `[src, buffered]`, and
`PageAnnotations` re-measures from `[match]` — and that component's own doc
comment already reasons about this key by name. The one visible consequence of
a `skip` toggle in scroll mode is a reused `PageView` showing the previous page
until the new one decodes, which is the designed no-blank-frame behaviour.

Read from the code and the measurements it cites; nobody has driven a browser,
toggled `skip` in scroll mode and watched. That is the limit of the claim.

**One rule is on by name because biome does not enable it and `oxlint` did.**
`.oxlintrc.json` named two rules: `react/rules-of-hooks`, whose biome
counterpart `correctness/useHookAtTopLevel` is on by default, and
`react/only-export-components`, whose counterpart
`style/useComponentExportOnlyModules` is **not** in the recommended set. Both
were checked by probe rather than by reading a list — a file exporting a
component beside a pure function drew nothing until the rule was named. Left at
biome's default it would have dropped coverage on the branch whose whole claim
was that coverage did not change. `tiles.ts` exists because of this rule.

**Two rules are on with inline suppressions carrying their reasons.**
`correctness/useExhaustiveDependencies` is the descendant of the rule `oxlint`
was enforcing, and its four suppressions all say the same thing: the flagged
dependencies are *triggers* rather than closure reads, and biome's own
suggestion — remove them — would leave scroll mode never following the reader,
the filmstrip centring once, and a reopened note never refocusing.
`suspicious/noTsIgnore` is suppressed at file level in `resolver.ts`, where
biome's suggested fix (`@ts-expect-error`) would break the build in exactly the
state the section below describes.

## The `web` job

`web` runs on `node:22-bookworm`, not one of the dual-toolchain images below,
and that is the point. Everything the job tests is pure TypeScript: the
reading model sits behind a `Resolver` interface, and only `resolver.ts`
imports `taiyaku-wasm` **as a value** — no test imports `resolver.ts` at all,
so `vitest` never loads it. `reading.ts` and `fixtures.ts` import only
`resolver.ts`'s *types* (`import type`), which is enough for `typecheck` to
need the file but not enough to need the value it wraps. Building the binding
here would still mean adding a dual-toolchain image or a `cargo install
wasm-pack` compile to a job that otherwise needs neither — the `wasm` job
below does that work instead, once, in a container that already has Rust.

**What that leaves ungated, stated rather than glossed:** because
`resolver.ts`'s `taiyaku-wasm` import cannot resolve in this job (see below),
`typecheck` checks that file against no real types at all here, so a call in
`wasmResolver` that no longer matches the generated `.d.ts` (a renamed method,
a changed argument) goes green in `web` regardless of what `wasm` catches.
That risk is carried by `npm run build` after `./scripts/build-wasm.sh`
locally — the one discipline this project still asks of a human rather than
a gate. Revisit if that discipline slips.

## Running the compiled module in CI

Until M5b's close, nothing in CI ever executed the compiled `.wasm` module —
`cargo test -p taiyaku-wasm` exercises `binding.rs` as plain Rust, which
catches conversion drift (a transposed `from`/`to`, a dropped half) but not a
codegen break, and `scripts/smoke-wasm.mjs` was a one-line command nobody but
a human ran. The `wasm` job now closes that: after the two `cargo build`
steps above (unchanged, and still the fast fail-fast check that the core and
the binding compile for `wasm32-unknown-unknown` at all), three more steps
install `node` via `apt-get`, fetch a prebuilt `wasm-pack` release binary,
build the real npm package with `./scripts/build-wasm.sh`, and run
`node scripts/smoke-wasm.mjs` against it — the same two commands a developer
runs locally, unmodified for CI.

**Why `apt-get`, not a dual-toolchain image.** Reusing the `wasm` job's
existing `rust:1-bookworm` container and installing `node` into it is cheaper
than switching to `cimg/rust:1.97-node` or `catthehacker/ubuntu:rust-latest`
(see the image table below): those cost 200 MB–500 MB more and, per that
table's own probe, five minutes of cold image pull, against `apt-get
install`'s cost of roughly the same 20–30 seconds the caching section below
already charges to the identical command.

**Why a curl'd binary, not `cargo install wasm-pack`.** Measured directly:
`cargo install wasm-pack --locked` is a multi-minute compile of a tool this
job only *runs*, never links against. A prebuilt release binary is a better
fit — `wasm-pack-v0.15.0-x86_64-unknown-linux-musl.tar.gz` from the project's
GitHub releases is 2.9 MB and was confirmed to resolve and extract to a
working `wasm-pack --version` before being wired into the workflow, rather
than guessed at. It is a static-PIE `musl` binary, so it runs on the
`bookworm` (glibc) container without needing a matching libc. `wasm-pack
build`'s default mode also downloads `wasm-bindgen-cli` (pinned to the
version `Cargo.lock` names, `0.2.126`) and `wasm-opt` on first use if neither
is already on `PATH`. The `wasm-bindgen-cli` fetch was tested locally with its
tool cache cleared, forcing a real download rather than a cache hit, and it
still built in under five seconds; the `wasm-opt` fetch was not, since this
machine has a system `wasm-opt` that could not be removed without root to
force it. Upstream (`wasm-pack`'s own source and documentation) confirms the
same download-on-first-use behaviour applies to `wasm-opt`, but that half is
inferred rather than measured.

**What was measured versus inferred.** The `wasm-pack` download, and a full
`./scripts/build-wasm.sh` run with that download's binary and a cleared
`wasm-pack` tool cache, were both measured directly on this development
machine — fast on its network (well under a second for the binary; a few
seconds for the full build including a fresh `wasm-bindgen-cli` fetch) but
not representative of the Forgejo runner's hardware or network, which this
task could not reach to measure directly (no local `docker` access to
replicate `rust:1-bookworm` here, and pushing a probe branch was out of
scope). The `apt-get install nodejs` cost is carried over from this same
document's existing, already-recorded figure for the identical command
(below) rather than re-measured. Taking the dominant cost (`apt-get`, ~20–30s)
plus the small ones measured here (binary fetches, low single-digit seconds;
the smoke script itself, well under a second) puts the `wasm` job's total
addition at roughly **30–60 seconds** — a real but modest fraction of a
pipeline whose slowest job, before this change, was well under a minute.
Confirm against an actual run once one exists; nothing here has watched this
workflow execute on the real runner.

**The `file:` dependency and a fresh clone.** `web/package.json` depends on
`taiyaku-wasm` via `"file:../crates/taiyaku-wasm/pkg"`, and `pkg/` is
gitignored — it does not exist in a clone that has only ever run `git fetch`.
This was verified rather than assumed: cloning the repository fresh (so
`crates/taiyaku-wasm/pkg/` is absent entirely) and running `npm ci` inside
`web/` succeeds anyway. `npm ci` resolves a `file:` entry by creating a symlink
at `node_modules/taiyaku-wasm` pointing at the target path — dangling, in this
case — without reading or validating anything at the far end. `npm run build`,
`npm run typecheck` and `npm run test` then all pass, because nothing
reachable from any of them *requires* that symlink to resolve. No
`optionalDependencies` fallback was needed.

**`typecheck` walks into `resolver.ts` without needing it to resolve.**
`tsc -b` type-checks every file `tsconfig.app.json` includes, and `resolver.ts`
is one of them — `reading.ts`'s `import type` from it means `tsc` opens the
file regardless of whether anything imports it as a value, so excluding
`resolver.ts` from the project would not stop it being checked. The
`taiyaku-wasm` import at its top is preceded by a single `@ts-ignore`, not a
`@ts-expect-error`: in a fresh clone the module can't resolve and the ignore
suppresses that one diagnostic, but after `./scripts/build-wasm.sh` has run
locally the import resolves fine and there is no error to suppress — an
`@ts-expect-error` would itself fail in that state, whereas `@ts-ignore` is
silent either way. This was verified in both states, not assumed: with `pkg/`
present, `tsc -b` still catches a real type error introduced elsewhere in
`wasmResolver` (confirming the ignore does not blanket-suppress the file); with
`pkg/` absent, `tsc -b` passes cleanly on the same source. This is the same
fact the `file:` dependency section above rests on, extended one file further:
`resolver.ts` degrades to typechecking against nothing, rather than failing
the job, the day its one import can't be resolved.

## Runner requirements

A registered `forgejo-runner` with:

- a label matching `runs-on: docker`
- Docker support, since every job specifies a `container:`

The label was `ubuntu-latest` until it was measured. git.daveynet.xyz's runner
advertises `docker`, so every job asked for a label nothing provided and sat in
`waiting` until a later push superseded and cancelled it — sixteen runs, none
of which ever recorded a start. Nothing in the workflow reports this: a job
matching no runner is queued, not failed.

If the runner advertises a different label, change `runs-on:` in `ci.yml` to
match. Check the instance's registered runners under repository settings if
unsure; note that an instance-wide runner shows in neither the repository's nor
the user's `actions/runners` API listing, so an empty list there is not evidence
that no runner exists.

## Why there are no actions at all

Every job clones with `git` in a `run:` step. No `uses:` anywhere, which is
unusual enough to explain.

`actions/checkout` is a JavaScript action, and a JS action needs a node inside
the job's container to execute it. `rust:1-bookworm` carries no node —
`command -v node` exits non-zero — and this runner does not inject one. A job
combining `container:` with any JS action therefore dies before its first step.

That was measured rather than assumed, because the symptom names none of it:
all three jobs failed inside twenty-five seconds with no output worth reading.
Probe jobs settled it — a container with no action passes, an action with no
container passes, and both together is the only failing combination.

The consequence to keep in mind: **no action can be added to these workflows
while the image is `rust:1-bookworm`**. Caching, artifact upload and the rest
are all JavaScript actions. The escape hatch, when one is wanted, is an image
carrying both toolchains; two are measured working below.

## Caching, and why there is none

Every job compiles from cold. `Swatinem/rust-cache` was in the first version of
this workflow and was removed with the rest of the actions, and no replacement
was added, because there is no such thing as a cache action that is not
JavaScript — `actions/cache` and every Rust-specific wrapper alike.

The measurement says not to care yet, but it is older than the pipeline it
describes. On a green run the **three** jobs of the time took 51s, 39s and 17s,
and they run in parallel, so the pipeline was under a minute wall-clock.
Caching would save perhaps twenty or thirty seconds of dependency compilation
against real added complexity. Revisit when a single job passes a couple of
minutes, or when artifact upload is wanted for the `verify` reports.

**Two things have been added since those figures were taken, and neither has
been timed on the runner.** The `web` job is a fourth job that did not exist
then, and the `wasm` job has grown three steps — the `apt-get` node install, the
`wasm-pack` fetch and the package build plus smoke run — estimated above at
roughly 30–60 seconds and never confirmed against an actual run. So the 17s
figure is certainly low and the "under a minute" conclusion is no longer
measured, only inherited. It is left standing because the *decision* it supports
— do not add caching yet — does not turn on the difference, and replacing a
stale number with a guess would be worse than marking it stale. Take fresh
figures before using this paragraph to argue anything.

Two options exist that do not need an action at all, neither yet tested:

- **Named Docker volumes** on the job container, mounting `/usr/local/cargo` and
  the target directory across runs. Pure workflow YAML. The risk is that
  `act_runner` gates mounts behind `container.valid_volumes`, which denies by
  default, so it may need a runner config change.
- **`apt-get install -y nodejs` as the first step**, after which JavaScript
  actions work for the rest of the job, since the runner only needs a node when
  it reaches the action. The apt install costs most of what caching would save.

## Images carrying both Rust and node

Measured on this runner, each by running `actions/checkout` — the step that
fails on `rust:1-bookworm` — followed by `node --version`, `cargo --version` and
a real `cargo fmt --check`:

| Image | Compressed | Result |
| ----- | ---------- | ------ |
| `cimg/rust:1.97-node` | 0.77 GB | works |
| `catthehacker/ubuntu:rust-latest` | 1.05 GB | works |
| `mcr.microsoft.com/devcontainers/rust:1-bookworm` | — | no node |
| `rust:1-bookworm` (control) | 0.57 GB | fails, as predicted |

The control is the point of the table: it failed the same way the real jobs did,
which is what makes the two passes mean something rather than being a guess that
happened to work.

**Not switched, deliberately.** The only thing an image change buys today is the
caching described above, and that is not worth paying for yet. The costs are
real: `cimg/rust` adds 200 MB and runs as a non-root user, and the probe run
spent five minutes pulling these images before any job started, which is what a
cold layer cache costs on this runner. `catthehacker/ubuntu` is the closer match
to a GitHub-style runner if the deciding factor ever becomes action
compatibility rather than size.

Note that neither image removes the toolchain download described below, since
`rust-toolchain.toml` pins `stable` regardless of what the image ships.

## Why the clone is authenticated

The instance requires sign-in for all content, so an anonymous fetch is refused
even though this repository is public — the same setting that makes the REST API
reject unauthenticated calls. Without credentials the clone fails with
`could not read Username for 'http://forgejo:3000'`.

The runner supplies `FORGEJO_TOKEN` for this. Note that `FORGEJO_SERVER_URL` is
the runner's own view of the forge, an address on its container network rather
than the public hostname — an unauthenticated clone fails against
`http://forgejo:3000`, not against `git.daveynet.xyz`. The workflow derives the
clone URL from the variable instead of hard-coding either name.

## Why the variables say Forgejo and not GitHub

Forgejo Actions implements GitHub's interface, so its runner exports every value
twice: once under `GITHUB_*`, which is what lets actions written for GitHub work
unchanged, and once under `FORGEJO_*`. Both were measured working on this
instance, and the values are identical — `GITHUB_SERVER_URL` here is
`http://forgejo:3000`, not anything on github.com.

The `FORGEJO_*` set is used because nothing in this project is hosted on or
fetched from GitHub, and a workflow that reads as though it were invites the
question every time someone opens it.

## Toolchain note

`rust-toolchain.toml` pins `channel = "stable"` and requests the `wasm32`
target plus `rustfmt` and `clippy`, which makes a fresh local checkout work with
no setup.

The tradeoff is in CI: the `rust:1-bookworm` image installs its toolchain under a
version-specific name rather than as `stable`, so rustup fetches `stable`
separately on each job. It works, but costs a download per job. If that becomes
annoying, either pin `channel` to the image's exact version and bump both
together, or delete `rust-toolchain.toml` and let CI use the image toolchain as-is
at the cost of local convenience.
