# Continuous integration

Workflows live in `.forgejo/workflows/`. Forgejo looks there first and falls back
to `.github/workflows/`; using the Forgejo path keeps the intent unambiguous.

## Jobs

| Job | Runs |
| --- | ---- |
| `lint` | `cargo fmt --check`, `cargo clippy -D warnings` |
| `test` | `cargo test --workspace` |
| `wasm` | builds `taiyaku-core` and `taiyaku-wasm` for `wasm32-unknown-unknown`, builds the browser package with `wasm-pack`, and runs `scripts/smoke-wasm.mjs` against the compiled module |
| `web` | `pnpm install --frozen-lockfile`, `biome check` (formatting **and** lint), `tsc -b` typecheck, `vitest run` in `web/` |

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
  - the web job (web/node_modules absent — run `pnpm -C web install`)
```

The corpus checks are the one thing the script measures that the forge cannot,
since CI has no corpus. Everything else runs in both places — with one
exception.

## Two profiles, and the gate that took an hour

**Setting `TAIYAKU_CORPUS` used to cost far more than the corpus job itself.**
The script exports it before the `test` job, and that job is an *unfiltered*
`cargo test --workspace --all-features` — the `corpus::` filter belongs to the
separate corpus job below it. So a local gate run with a corpus present ran all
fifteen standing checks twice, both times in **debug**, and ran the two one-off
measurement modules as well. `geometry_experiment` fingerprints roughly 1.4
million Lanczos3 crops: about a minute under `--release`, about forty without.
Measured rather than estimated — one gate run was killed at **1h09m** still
inside that single test, and another at over 25 minutes without reaching the
second corpus pass.

**Fixed by giving the two jobs the profiles they each want**, rather than by
documenting a workaround:

- The `test` job runs `env -u TAIYAKU_CORPUS cargo test --workspace
  --all-features`. Removing the variable drops the corpus checks and both
  experiment modules, and makes the job a *truer* mirror of CI, which has no
  corpus and never ran any of them. **4.4 seconds.**
- The corpus job runs `--release`. It is not a CI job, so the mirror above does
  not depend on its profile. **6.9 seconds** warm; about 1:13 the first time,
  since `[profile.release]` is `lto = "thin"` with `codegen-units = 1`.

The whole gate — all four jobs plus the corpus — now takes about **15 seconds**
warm.

**What that gives up, and how to get it back.** Integer overflow panics and
`debug_assert!` are compiled only under `debug_assertions`, and this repository
has two of the latter that matter: `PageRange::new`'s "page range must not be
inverted" and `Run::paired`'s equal-length check, both in
`taiyaku-core/src/lib.rs`. The debug `test` job exercises both through the unit
tests; the release corpus job cannot fail on either over real material.
`scripts/check-all.sh --corpus-debug` runs that pass in debug instead — worth
doing when touching `PageRange`, `Run::paired`, or arithmetic on page and slot
indices, and not otherwise. The script says which profile the corpus job used,
right above the run, so a reader does not have to remember this page.

The two experiment modules are still not filtered out of the test target: a
one-off measurement that silently stops being *compiled* is the failure their own
assertions exist to prevent. What changed is the gate's environment, not their
gating — each module's header names the release-profile command it is meant to
be run with.

**CI stays in debug, and that was measured rather than assumed.** The obvious
next step after the split above is to make CI match — the local corpus job got
much faster under `--release`, so why not the forge. It is the wrong move here,
for three reasons that are properties of *this* CI rather than of release builds
in general:

- **There is no cache.** Every job above clones with `git init` and a `--depth 1`
  fetch into a fresh container, so every run compiles the workspace from
  nothing. There is no warm second run for a release profile to pay off across.
- **There is no corpus.** CI's `test` job runs the unit suite alone — 4.4
  seconds in debug. The tests release actually speeds up are the fifteen
  standing corpus checks, and CI skips every one.
- **`[profile.release]` is tuned for the aligner, not for compiling.**
  `lto = "thin"` with `codegen-units = 1` is the slowest configuration to build,
  across an `axum`/`tokio` tree.

Measured cold, with an empty `CARGO_TARGET_DIR`, which is CI's condition on
every run:

| Profile | `cargo test --workspace --all-features --no-run` |
| ------- | ----------------------------------------------: |
| debug   | 22.5 s |
| release | 1 m 26.3 s |

So release would cost CI about 64 extra seconds per run to save a 4.4-second
test suite, and give up overflow checks and both `debug_assert!`s while doing
it. The local gate and CI therefore run the same `test` job in the same profile,
and the mirror in `scripts/check-all.sh` holds exactly — the release profile
appears only in the corpus job, which is the one thing CI does not have.

**The lockfile check *is* mirrored, and until 2026-08-09 it was not.** This
document used to carry that as a permanent gap: CI's install builds
`node_modules` from nothing, while running the same command locally would
delete a working install and rebuild it on every gate run, so the one property
it checks — that `package.json` and the lockfile agree — went unchecked, and a
drifted lockfile passed `check-all.sh` to fail the forge.

pnpm closes it, because it has a command that is that check and nothing else:
`pnpm -C web install --frozen-lockfile --lockfile-only` verifies the lockfile
satisfies `package.json` without touching `node_modules`. **Measured across
three consecutive runs: 249ms, 239ms and 237ms.** Verified by adding a
dependency the lockfile had never seen — the gate was green before the step
existed and failed after it, naming the package:

    specifiers in the lockfile don't match specifiers in package.json:
    * 1 dependencies were added: is-odd@^3.0.1

It is run directly rather than piped, because a pipe masks the exit code — which
was caught while writing the probe, and is exactly how a gate stops gating.

## Formatting, and the tool that checks it

Until 2026-08-07 the TypeScript half had **no formatting check at all**, which
was measured rather than assumed: a deliberately mangled exported function
(`(   a:number,b:number ):number{`) passed `oxlint`, `tsc -b` and all 414 tests.
The Rust half has always been checked — `cargo fmt --all -- --check` exits 1 on
mangled Rust, also measured — so the gap was one-sided and nothing said so.

`biome` now covers both halves of the `web` job's first step: `pnpm -C web run
lint` is `biome check ./src`, which is formatting *and* lint in one command, so
the CI step needed no change. `oxlint` is gone, along with `.oxlintrc.json`.

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

`web` runs on `node:26-bookworm`, not one of the dual-toolchain images below,
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
That risk is carried by `pnpm -C web run build` after `./scripts/build-wasm.sh`
locally — the one discipline this project still asks of a human rather than
a gate. Revisit if that discipline slips.

**Node 26 is Current, not LTS, and that was chosen rather than defaulted into.**
Per `nodejs/Release`'s own schedule, 26 goes LTS on 2026-10-28 and reaches EOL
on 2029-04-30; 24 has been Active LTS since 2025-10-28. Taking 26 means some
weeks on a Current release, bought against local and CI staying close — this
development machine runs 26.7.0, and skew between a developer's node and the
gate's is the kind of gap that hides a defect until the forge finds it. That
agreement is not exact for this job, though: `node:26-bookworm` (above) is a
floating tag, so the image can pick up 26.8.0 or later without this document
or `ci.yml` changing, and nothing pins the `web` job to 26.7.0 the way the
`wasm` job's own node install does. Exact agreement holds only there — see
that job's section below.

**Three places name a Node version, and nothing keeps them in step.**
`.node-version` at the repository root says `26`; the `web` job's image
floats at `node:26-bookworm`; the `wasm` job hardcodes `26.7.0` twice, in the
download URL and the symlink target (see `ci.yml`'s comment on that step).
`.node-version` is read by nothing in this repository — no document, script
or workflow references it — so it is not a single stated source for the pin,
only a fourth place that can drift from the other three. Bump all of them
together if any one moves, the same discipline `ci.yml` already asks of
`wasm-pack`'s version.

**The `wasm` job was on Node 18 and nothing said so.** It installed node with
`apt-get install -y nodejs`, and Debian bookworm's `nodejs` package is
`18.20.4+dfsg-1~deb12u2` — confirmed against Debian's own package index. So the
only thing anywhere that *executes* the compiled module ran on a different major
from every other job. That was not a decision; it is what `apt-get` happened to
ship. It now fetches a checksummed 26.7.0 tarball, the same prebuilt-binary
approach this job already takes for `wasm-pack`. **Guess, not measurement:** the
larger download should be roughly a wash against `apt-get`'s recorded 20–30s,
but no runner was reachable to time it. Confirm against a real run.

## The typecheck, and the compiler that stopped being JavaScript

`"typecheck": "tsc -b"` is unchanged from the TypeScript 6 era and that is the
point: in TypeScript 7, `tsc` **is** the native compiler. Verified rather than
taken on trust — `typescript@7.0.2` declares exactly one bin, `tsc`, which
dispatches through `optionalDependencies` to a platform package that is a
statically linked Go ELF binary. There is no `tsgo` bin in the package; that
name belonged to `@typescript/native-preview`, the preview channel that carried
the port before 7.0 shipped. The package size says the same from the other
side: 24.3 MB unpacked at 6.0.2 against 2.5 MB at 7.0.2.

Measured against this repository's own `tsconfig.app.json` and
`tsconfig.node.json`, cold each time with `node_modules/.tmp` removed between
runs, TS 7 twice rather than once:

| | wall | CPU |
| --- | ---- | --- |
| TS 6.0.3 | 1.511s | 3.19s |
| TS 7.0.2 (run 1) | 0.164s | 0.93s |
| TS 7.0.2 (run 2) | 0.166s | 0.99s |

A full typecheck in under two tenths of a second invites the suspicion that
nothing ran, so that was checked too: injecting `export const probe: number =
"not a number"` into `web/src` made both compilers fail at the identical
position with the identical code, `TS2322`. Project references, `tsc -b`
build mode, `moduleResolution: bundler`, `verbatimModuleSyntax`,
`erasableSyntaxOnly` and `allowImportingTsExtensions` all work on the native
port.

**One behavioural difference, recorded rather than glossed:** on type errors
TS 6 exits `2` and TS 7 exits `1`. Both are non-zero so no gate changes, but
anything that ever tests for a specific code would break.

**One thing this migration found, unasked.** `pnpm -C web outdated` reported
nothing behind, but `pnpm -C web update --latest` bumped four packages anyway —
`vite`, `@vitejs/plugin-react`, `@types/react` and `@types/react-dom`, each one
patch release ahead of what `outdated` had just called current, all still
inside their existing caret ranges. `typescript`, `@types/node` and the
`taiyaku-wasm` `link:` protocol were checked afterward and confirmed
untouched, since `update --latest` does not distinguish a dependency pinned on
purpose from one merely unpinned. A reader who runs `pnpm outdated` alone to
decide whether an update is needed should not trust a clean result on its own.

## Running the compiled module in CI

Until M5b's close, nothing in CI ever executed the compiled `.wasm` module —
`cargo test -p taiyaku-wasm` exercises `binding.rs` as plain Rust, which
catches conversion drift (a transposed `from`/`to`, a dropped half) but not a
codegen break, and `scripts/smoke-wasm.mjs` was a one-line command nobody but
a human ran. The `wasm` job now closes that: after the two `cargo build`
steps above (unchanged, and still the fast fail-fast check that the core and
the binding compile for `wasm32-unknown-unknown` at all), three more steps
fetch and unpack a checksummed `node` tarball, fetch a prebuilt `wasm-pack`
release binary, build the real npm package with `./scripts/build-wasm.sh`,
and run `node scripts/smoke-wasm.mjs` against it — the same two commands a
developer runs locally, unmodified for CI.

**Why a tarball fetch, not a dual-toolchain image.** Reusing the `wasm` job's
existing `rust:1-bookworm` container and dropping a `node` binary into it is
cheaper than switching to `cimg/rust:1.97-node` or `catthehacker/ubuntu:rust-latest`
(see the image table below): those cost 200 MB–500 MB more and, per that
table's own probe, five minutes of cold image pull. Measured directly on this
development machine, not the runner (whose network this task could not
reach): fetching `node-v26.7.0-linux-x64.tar.xz` (33.5 MB), verifying its
checksum against the value pinned in `ci.yml`, and extracting it took
**1.4–1.6 seconds** across three runs. That is not a runner figure, but it is
well under the 200 MB–500 MB / five-minute cost of the alternative, so the
conclusion — stay in this container — does not turn on closing that gap
precisely.

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

**What was measured versus inferred.** The node tarball fetch (curl,
checksum, extract), the `wasm-pack` download, and a full
`./scripts/build-wasm.sh` run with that download's binary and a cleared
`wasm-pack` tool cache, were all measured directly on this development
machine — fast on its network (1.4–1.6s for the node tarball, above; well
under a second for the `wasm-pack` binary; a few seconds for the full build
including a fresh `wasm-bindgen-cli` fetch) but not representative of the
Forgejo runner's hardware or network, which this task could not reach to
measure directly (no local `docker` access to replicate `rust:1-bookworm`
here, and pushing a probe branch was out of scope).

**The previous total here is retracted, not adjusted.** This section used to
put the job's addition at roughly 30–60 seconds by taking `apt-get install
nodejs`'s recorded ~20–30s as the dominant term and adding the smaller,
measured pieces to it. `apt-get` was removed from this job, not merely slowed
down or sped up, so there is no dominant-cost figure left to build that
total on — and quietly substituting this development machine's 1.4–1.6s
would smuggle a non-representative number into what read as a runner
estimate. So: the small pieces above are measured, the node fetch's shape (one
~33.5 MB download, checksummed, then extracted) is known, and the `wasm`
job's total wall-clock addition on the actual runner is **unmeasured**.
Confirm against an actual run before writing a new figure here; nothing here
has watched this workflow execute on the real runner.

**The `link:` dependency and a fresh clone.** `web/package.json` depends on
`taiyaku-wasm` via `"link:../crates/taiyaku-wasm/pkg"`, and `pkg/` is
gitignored — it does not exist in a clone that has only ever run `git fetch`.
This is verified rather than assumed, and it was re-verified when the package
manager changed rather than carried over: cloning the repository fresh (so
`crates/taiyaku-wasm/pkg/` is absent entirely) and running
`pnpm -C web install --frozen-lockfile` succeeds anyway, creating a dangling
symlink at `node_modules/taiyaku-wasm` without reading or validating anything
at the far end. `pnpm -C web run build`, `typecheck` and `test` then all pass,
because nothing reachable from any of them *requires* that symlink to resolve.
No `optionalDependencies` fallback was needed.

**Why `link:` and not `file:`, which is what npm used.** Two measurements, both
against pnpm 11.3.0. First, `file:` does not survive the fresh clone at all —
it fails with `ERR_PNPM_LINKED_PKG_DIR_NOT_FOUND` where `npm ci` was content to
dangle. Second, and worse, `file:` *with* the target present resolves to a
hard-linked **copy** in pnpm's store rather than a symlink. Shared inodes make
an in-place edit show through, which makes it look harmless; it is not, because
`scripts/build-wasm.sh` deletes and regenerates `pkg/`, and new files get new
inodes. Simulating exactly that, `web/node_modules/taiyaku-wasm` still served
the **pre-rebuild** file. A developer who rebuilt the binding and reran the gate
would have been testing against the previous build with nothing announcing it.
`link:` is a live symlink and has neither problem. It is pnpm-specific and not
resolvable by other package managers, which costs nothing here: `web` is
`"private": true` and is never published.

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
then, and the `wasm` job has grown three steps — the node tarball fetch, the
`wasm-pack` fetch and the package build plus smoke run — with no total
estimate above any more (the figure that used to stand there leaned on
`apt-get`'s cost, which this job no longer pays; see above). So the 17s
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
- **Installing a throwaway node as the first step**, via the container's own
  package manager rather than a pinned tarball — this path needs *a* node, not
  a specific version, so the fastest available install is fine. JavaScript
  actions then work for the rest of the job, since the runner only needs a
  node when it reaches the action. That install costs most of what caching
  would save.

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
