# pnpm, Node 26, and the TypeScript that is no longer JavaScript — design

**Status: settled in brainstorming on 2026-08-09. Nothing built.**

Written in the style of the rest of `docs/`: every decision records the
alternative it beat and what it cost, a figure asserted here was measured on
this machine rather than recalled, and a guess says it is one.

A toolchain branch, not a feature branch. It touches no Rust, no reader
behaviour and no corpus. What it changes is the four commands the `web` job
runs and the versions underneath them.

## Where it came from

Asked directly: switch the web package manager to pnpm, say whether bun would
be the better choice, and get the dependencies — Node in particular — onto
current versions. Three questions, and the second one turned out to have an
answer that could be measured rather than argued.

## 1. Why pnpm and not bun

### What bun would actually buy

Bun's runtime advantages are real and they are documented mechanisms, not
marketing: a JavaScriptCore engine that starts in roughly 10ms against node's
40ms, filesystem and HTTP primitives implemented in Zig, and a built-in
transpiler that removes the `tsx`/`ts-node` layer.

Every one of them is a win for work performed *in JavaScript*. Taiyaku's
heavy work is not. The server is Rust and axum, the core and the solver are
Rust, the alignment is Rust compiled to WebAssembly. The JavaScript is a React
reader and a handful of pure-TypeScript modules.

**Measured, the whole JavaScript gate on this machine.** These are the
`npm run` invocations as the gate actually calls them, so each includes npm's
own startup; §6 times the `tsc` binary directly, which is why its figure for
the same work is fractionally lower.

| step | tool | wall |
| ---- | ---- | ---- |
| `npm run lint` | biome (Rust) | 0.14s |
| `npm run typecheck` | `tsc -b`, cold, `tsbuildinfo` cleared | 1.59s |
| `npm run test` | vitest, node project | 0.74s |

**About 2.5 seconds in total.** There is no runtime budget here to recover. A
faster engine applied to 0.74s of vitest would be a rounding error, and it is
not even available: the browser tests are vitest's browser mode driving
Playwright, which `bun test` cannot run, and the `wasm` job's
`scripts/smoke-wasm.mjs` wants a real node regardless. Bun would add a second
toolchain to CI to buy nothing this repository spends time on.

**Not measured, and said so:** bun itself was never installed or benchmarked
here. The mechanisms above are how it is documented to work; the 2.5s is what
this machine actually reported. The claim being made is about where those
mechanisms land in *this* repository, not about bun's numbers in general.

### What pnpm buys instead

Not install speed — on twelve dependencies that is not the bottleneck either.
Two things:

1. **A strict `node_modules`.** A phantom dependency — an import of a package
   that is present only because something else hoisted it — passes the `web`
   job today and would fail under pnpm. That is a class of defect this gate
   can currently ship green.
2. **It closes a gap `docs/ci.md` documents as permanent.** See §3.

## 2. The `file:` dependency, which is the whole difficulty

`web/package.json` depends on the WebAssembly package by path:

```json
"taiyaku-wasm": "file:../crates/taiyaku-wasm/pkg"
```

`pkg/` is gitignored. It does not exist in a clone that has only ever run
`git fetch`, and `docs/ci.md` records — as a verified property, not an
assumption — that `npm ci` tolerates this by creating a dangling symlink and
validating nothing at the far end. The `web` job depends on that being true.

**A straight package-manager swap breaks it.** Measured against pnpm 11.3.0
with the target absent:

```
[ERR_PNPM_LINKED_PKG_DIR_NOT_FOUND] Could not install from ".../pkg"
as it does not exist.
```

So the protocol has to change. Two candidates were measured.

### `file:` under pnpm is worse than the error

With the target present, pnpm resolves `file:` to a **hard-linked copy** in
its store, not a symlink. Confirmed by inode:

```
5048737 crates/taiyaku-wasm/pkg/i.js
5048737 web/node_modules/taiyaku-wasm/i.js
```

Shared inodes mean an in-place edit shows through, which makes this look
harmless. It is not. `scripts/build-wasm.sh` **deletes and regenerates**
`pkg/`, and new files get new inodes. Simulating exactly that — `rm -rf pkg`,
recreate, write new content — `web/node_modules/taiyaku-wasm` still served the
**pre-rebuild** file. A developer who rebuilds the binding and reruns the gate
would be testing against the previous build with nothing announcing it.

This is a silent-staleness trap that npm does not have, and it is the reason
`file:` is rejected rather than merely passed over.

### `link:` reproduces npm's behaviour exactly

```
lrwxrwxrwx taiyaku-wasm -> ../../crates/taiyaku-wasm/pkg
```

A plain symlink, live rather than copied, so a rebuild shows through
immediately. And with the target **absent** it still succeeds — measured under
both `pnpm install` and `pnpm install --frozen-lockfile`, the fresh-clone case
the `web` job needs.

`link:` is pnpm-specific and not resolvable by other package managers. `web` is
`"private": true` and is never published, so that costs nothing.

### What ships

```diff
- "taiyaku-wasm": "file:../crates/taiyaku-wasm/pkg"
+ "taiyaku-wasm": "link:../crates/taiyaku-wasm/pkg"
+ "packageManager": "pnpm@11.3.0"
```

`web/package-lock.json` is deleted and `web/pnpm-lock.yaml` replaces it.

**No `.npmrc`, unless a measurement demands one.** A strict `node_modules` is
the reason for the switch; adding `public-hoist-pattern` pre-emptively would
throw that away before knowing whether anything needs it. If a vite or vitest
plugin turns out to require a hoisted package, the escape gets added then,
narrowly, with the failure recorded next to it.

## 3. Closing a gap that `docs/ci.md` calls permanent

`scripts/check-all.sh` deliberately does not run `npm ci`, and `docs/ci.md`
line 121 states the consequence plainly: **"`npm ci` is not mirrored, and that
is a real gap rather than an oversight."** Running it locally would delete a
working install and rebuild it on every gate run, so the one property it
checks — that `package.json` and the lockfile agree — goes unchecked, and a
drifted lockfile passes the script and fails the forge.

pnpm has a command for exactly that check. Measured:

```
pnpm -C web install --frozen-lockfile --lockfile-only     → 214ms
```

`node_modules` untouched, verified by listing it before and after. On drift it
fails and says why:

```
specifiers in the lockfile don't match specifiers in package.json:
* 1 dependencies were added: is-odd@^3.0.1
```

So `check-all.sh` gains this step and `docs/ci.md` loses a paragraph. The gap
is closed rather than relocated.

**One implementation note carried from the measurement:** the step must be run
directly, not piped. Piping through `tail` masked the non-zero exit while the
probe was being written, which is precisely how a gate stops gating.

## 4. Command rewrites

`pnpm` has no `--prefix`; the equivalent is `-C`.

| Where | From | To |
| ----- | ---- | -- |
| `ci.yml`, `web` job | `npm --prefix web ci` | `pnpm -C web install --frozen-lockfile` |
| `check-all.sh` (×3) | `npm --prefix web run X` | `pnpm -C web X` |
| `check-all.sh` skip hint | `npx --prefix web playwright install chromium` | `pnpm -C web exec playwright install chromium` |
| `check-all.sh` skip hint | `npm --prefix web ci` | `pnpm -C web install` |

## 5. Node 22 → 26

Node 26 is **Current**, not LTS. From `nodejs/Release`'s own `schedule.json`,
fetched rather than recalled:

| line | status on 2026-08-09 | LTS from | EOL |
| ---- | -------------------- | -------- | --- |
| v22 — what CI runs today | maintenance since 2025-10-21 | 2024-10-29 | 2027-04-30 |
| v24 | **Active LTS** | 2025-10-28 | 2028-04-30 |
| v26 | Current (v26.7.0, 2026-08-05) | **2026-10-28** | 2029-04-30 |

26 was chosen over 24 deliberately and with the trade-off stated: it means
about eleven weeks on a Current release, and it buys local and CI agreeing
exactly, since this development machine runs 26.7.0. Version skew between a
developer's node and the gate's is the kind of gap that hides a defect until
the forge finds it, and eleven weeks of Current is the smaller risk.

### The `wasm` job is on Node 18 and nobody wrote that down

`ci.yml` installs node into the `rust:1-bookworm` container with `apt-get
install -y nodejs`. Debian bookworm's `nodejs` package is **Node 18** —
`18.20.4+dfsg-1~deb12u2`, confirmed against Debian's own package index rather
than assumed from the release date. So
`scripts/smoke-wasm.mjs` — the only thing anywhere that executes the compiled
module — runs on 18 while the `web` job runs on 22. This was not a decision;
it is what `apt-get` happened to yield.

It gets replaced with a curl'd official Node 26 tarball, following the
precedent the same job already sets for `wasm-pack`: fetch a prebuilt binary
rather than take whatever the distribution ships. The cost is a larger download
against `apt-get`'s already-recorded 20–30s. **Guess, not measurement:** this
should be roughly a wash on the runner, but no runner was reachable to time it.
Confirm against a real run.

### A version file

New `.node-version` at the repository root containing `26`. This machine's node
is a system install at `/usr/sbin/node` with no version manager in front of it,
so the file is documentation of the pin rather than enforcement of it — worth
having so that CI's version has a single stated source.

## 6. TypeScript 6 → 7, and the `tsgo` that does not exist

The request was to move to TypeScript 7 and use `tsgo` for the typecheck. The
first half ships; the second half is already true and needs no change.

**Measured:** `typescript@7.0.2` declares exactly one bin, `tsc`, which
dispatches to a platform package resolved through `optionalDependencies`. On
this machine that is `@typescript/typescript-linux-x64/lib/tsc`:

```
ELF 64-bit LSB executable, x86-64, statically linked, Go BuildID=...
```

A statically linked Go binary. **In TypeScript 7, `tsc` *is* tsgo.** The
`tsgo` name belonged to `@typescript/native-preview`, the preview channel that
carried the native port before 7.0 shipped; no such bin exists in 7.0.2. The
package size tells the same story from the other side — 24.3 MB unpacked at
6.0.2 against 2.5 MB at 7.0.2, because `tsc` is no longer a large `tsc.js`
shipped as JavaScript.

So `"typecheck": "tsc -b"` is unchanged and gets the native compiler for free.

### Measured against this repository's real tsconfigs

Cold each time, `node_modules/.tmp` removed between runs, two runs each:

| | wall | CPU |
| --- | ---- | --- |
| TS 6.0.3 | 1.56s, 1.56s | 3.2s |
| TS 7.0.2 | **0.16s, 0.16s** | 0.8s |

**About 9.5× on wall clock,** on the slowest JavaScript step in the gate.

### Verified that it is checking, not no-opping

A 0.16s full typecheck invites the suspicion that nothing happened. So a
deliberate error was injected into `web/src`:

```ts
export const probe: number = "not a number";
```

Both compilers rejected it at the identical position with the identical code:

```
src/__ts7probe.ts(1,14): error TS2322: Type 'string' is not assignable to type 'number'.
```

The probe file was then deleted and `git status --porcelain` confirmed empty.

This retires the risk the branch was most exposed to: project references,
`tsc -b` build mode, `moduleResolution: bundler`, `verbatimModuleSyntax`,
`erasableSyntaxOnly` and `allowImportingTsExtensions` all work on the native
port, against these exact `tsconfig.app.json` and `tsconfig.node.json` files.

**One behavioural difference recorded rather than glossed:** on type errors
TS 6 exits `2` and TS 7 exits `1`. Both are non-zero, so no gate changes — but
anything that ever tests for a specific code would break, and this document is
where that is written down.

## 7. The rest of the dependency refresh

Checked against the registry on 2026-08-09. Only two need an explicit major:

| package | at | latest | action |
| ------- | -- | ------ | ------ |
| `typescript` | `~6.0.2` | 7.0.2 | major — §6 |
| `@types/node` | `^24.13.3` | 26.2.0 | major, tracks the runtime pin |
| `vite` | `^8.2.0` | 8.2.1 | within caret |
| `@vitejs/plugin-react` | `^6.0.4` | 6.0.5 | within caret |
| `vitest`, `@vitest/browser`, `@vitest/browser-playwright` | `^4.1.10` | 4.1.10 | current |
| `@biomejs/biome` | `^2.5.7` | 2.5.7 | current |
| `react`, `react-dom` | `^19.2.8` | 19.2.8 | current |
| `playwright` | `^1.62.1` | 1.62.1 | current |

`pnpm update --latest` plus a regenerated lockfile covers the caret cases.

## 8. Documentation

`docs/ci.md` needs three edits, one of them load-bearing:

1. **The fresh-clone paragraph** (around line 283) states a *verified* property
   of `npm ci` resolving `file:` to a dangling symlink. That claim becomes
   false and must be rewritten with the pnpm measurements from §2 — including
   the hard-linked-copy finding, so that a later reader knows `link:` was
   chosen against evidence rather than by preference.
2. **The `npm ci` gap paragraph** (line 121) describes a gap that §3 closes.
3. **The `web` job section and the image table** — Node 22 → 26, and the
   `wasm` job's Node 18 correction from §5.

`docs/status.md` gets a short entry. No open item is opened; the `docs/ci.md`
gap closed in §3 was prose rather than a numbered O-item, so no O-number
changes.

## 9. Verification

In order, and none of it inherited from the npm era:

1. `./scripts/check-all.sh` green.
2. **Fresh-clone probe.** Clone to a scratch path so `crates/taiyaku-wasm/pkg`
   is genuinely absent, run `pnpm -C web install --frozen-lockfile`, then
   `lint`, `typecheck` and `test`. This is exactly the property `docs/ci.md`
   verified for `npm ci`; it is re-measured for pnpm rather than assumed to
   carry over.
3. **Rebuild probe.** `./scripts/build-wasm.sh`, then confirm
   `web/node_modules/taiyaku-wasm` resolves to the *new* content — the
   property `link:` has and `file:` does not.
4. Browser tests under Playwright: `pnpm -C web test:browser`.
5. `pnpm -C web build` after a `build-wasm.sh`, which is still the only thing
   that type-checks `resolver.ts` against the binding's real generated types.

## 10. What could not be measured

**How pnpm gets into the `web` CI job.** `pnpm/action-setup` is a JavaScript
action, and `ci.yml`'s opening comment records the measured finding that a job
combining `container:` with any JS action dies before its first step on this
runner. So the plan is `npm i -g pnpm@11.3.0` as the job's first step, which is
unconditional and needs nothing from the image.

Whether `node:26-bookworm` still ships `corepack` — which would allow honouring
the `packageManager` field directly — could not be checked: no local docker, the
same limitation `docs/ci.md` already records against its image table. If it
turns out to be present, the global install can be dropped later; the branch
does not depend on the answer.

## Out of scope

A test-coverage gate on the `web` job. Requested in the same conversation and
deliberately deferred to its own branch stacked on this one, so that a coverage
threshold argument cannot be confused with a package-manager regression.
