# pnpm, Node 26, TypeScript 7 — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move `web/` from npm to pnpm, pin Node 26 across both jobs that use it, and adopt TypeScript 7's native compiler — without breaking the fresh-clone property the `web` CI job depends on.

**Architecture:** Six independent tasks, each ending in its own commit and its own verification. Task 1 carries the only real risk (the gitignored `file:` dependency); Tasks 2–5 are mechanical once Task 1 lands; Task 6 rewrites the documentation whose measured claims Task 1 invalidates.

**Tech Stack:** pnpm 11.3.0, Node 26.7.0, TypeScript 7.0.2 (native Go compiler), vite 8, vitest 4, biome 2.5, Forgejo Actions.

**Design doc:** `docs/superpowers/specs/2026-08-09-pnpm-node26-typescript7-design.md`

## Global Constraints

- Package manager is **pnpm 11.3.0**, declared as `"packageManager": "pnpm@11.3.0"` in `web/package.json`.
- The `taiyaku-wasm` dependency uses **`link:`**, never `file:`. `file:` under pnpm is a hard-linked copy that serves stale content after `scripts/build-wasm.sh` regenerates `pkg/`.
- Node is pinned to **26.7.0** everywhere — the `web` job image, the `wasm` job's installed node, and `.node-version`.
- TypeScript is **`~7.0.2`**. The `typecheck` script stays `tsc -b` verbatim: in TS 7 `tsc` already *is* the native Go binary. There is no `tsgo` bin to call.
- `@types/node` is **`^26.2.0`**, tracking the runtime pin.
- **No `web/.npmrc`** is created unless a specific measured failure requires it. Strict `node_modules` is the reason for the switch; record any escape hatch next to the failure that forced it.
- Script invocations use the explicit **`pnpm -C web run <script>`** form, never the bare `pnpm -C web <script>` shorthand, so a future script name can never collide with a pnpm builtin.
- The lockfile-drift check must be **run directly, never piped**. Piping through `tail` masks its non-zero exit, which is how a gate stops gating.
- Every figure written into `docs/` must be measured on the machine running this plan, not copied from the design doc.

---

### Task 1: pnpm, and the `link:` protocol

**Files:**
- Modify: `web/package.json`
- Delete: `web/package-lock.json`
- Create: `web/pnpm-lock.yaml` (generated, not hand-written)

**Interfaces:**
- Consumes: nothing — this is the first task.
- Produces: a `web/` tree installed by pnpm, with `web/node_modules/taiyaku-wasm` a live symlink to `../../crates/taiyaku-wasm/pkg`. Tasks 2–5 assume `pnpm -C web run <script>` works.

- [ ] **Step 1: Record the npm baseline**

Run from the repository root:

```bash
npm --prefix web run lint && npm --prefix web run typecheck && npm --prefix web run test
```

Expected: all three exit 0. Write down the vitest test count from the last line — Task 5 checks it has not changed.

- [ ] **Step 2: Write the failing probe — prove pnpm rejects `file:` on a fresh clone**

This is the risk the whole task exists to retire. Build a scratch clone where `crates/taiyaku-wasm/pkg/` is genuinely absent:

```bash
SCRATCH="$(mktemp -d)"
git clone --no-local . "${SCRATCH:?}/taiyaku"
test ! -e "${SCRATCH:?}/taiyaku/crates/taiyaku-wasm/pkg" && echo "pkg absent — good"
pnpm -C "${SCRATCH:?}/taiyaku/web" install --no-frozen-lockfile
```

- [ ] **Step 3: Run it and confirm it fails**

Expected: non-zero exit with

```
[ERR_PNPM_LINKED_PKG_DIR_NOT_FOUND] Could not install from ".../crates/taiyaku-wasm/pkg" as it does not exist.
```

If this *succeeds*, stop and re-read the design doc §2 — the premise has changed and the protocol choice needs re-deciding.

- [ ] **Step 4: Change the protocol and declare the package manager**

Edit `web/package.json`. Change the dependency:

```json
    "taiyaku-wasm": "link:../crates/taiyaku-wasm/pkg"
```

and add a top-level field after `"type": "module",`:

```json
  "packageManager": "pnpm@11.3.0",
```

- [ ] **Step 5: Re-run the fresh-clone probe against the change**

```bash
cp web/package.json "${SCRATCH:?}/taiyaku/web/package.json"
rm -f "${SCRATCH:?}/taiyaku/web/package-lock.json"
pnpm -C "${SCRATCH:?}/taiyaku/web" install --no-frozen-lockfile
ls -la "${SCRATCH:?}/taiyaku/web/node_modules/taiyaku-wasm"
```

Expected: exit 0, and the listing shows a **dangling symlink**:

```
taiyaku-wasm -> ../../crates/taiyaku-wasm/pkg
```

This is the exact property `docs/ci.md:279` records for `npm ci`, now re-measured for pnpm rather than assumed to carry over.

- [ ] **Step 6: Migrate the real tree**

```bash
rm -f web/package-lock.json
rm -rf web/node_modules
pnpm -C web install
```

Expected: exit 0, and `web/pnpm-lock.yaml` now exists.

- [ ] **Step 7: Verify the gate still passes**

```bash
pnpm -C web run lint && pnpm -C web run typecheck && pnpm -C web run test
```

Expected: all three exit 0, with the same test count recorded in Step 1.

If any step fails with a module-resolution error naming a package that is not a direct dependency, that is pnpm's strict `node_modules` catching a phantom dependency — a real defect, not a migration problem. Add the package to `web/package.json` as an explicit dependency. Do **not** reach for `.npmrc` hoisting first.

- [ ] **Step 8: Verify `link:` stays live across a wasm rebuild**

This is the property `file:` does not have.

```bash
head -c 120 web/node_modules/taiyaku-wasm/package.json > /tmp/before.txt
./scripts/build-wasm.sh
head -c 120 web/node_modules/taiyaku-wasm/package.json > /tmp/after.txt
readlink web/node_modules/taiyaku-wasm
test -f web/node_modules/taiyaku-wasm/taiyaku_wasm.js && echo "resolves after rebuild — good"
```

Expected: `readlink` prints `../../crates/taiyaku-wasm/pkg`, and the file test succeeds. `build-wasm.sh` does `rm -rf` on `pkg/`, so a hard-linked copy would have left a broken directory here.

- [ ] **Step 9: Verify the browser tests**

```bash
pnpm -C web run test:browser
```

Expected: exit 0. (Chromium is already present in `~/.cache/ms-playwright` on this machine. If it is not, run `pnpm -C web exec playwright install chromium` first.)

- [ ] **Step 10: Clean up the scratch clone**

```bash
rm -rf "${SCRATCH:?}"
```

- [ ] **Step 11: Commit**

`git add -A` is used because `web/package-lock.json` is a deletion, and a bare `git add <path>` does not stage one. It is safe here: `web/node_modules`, `web/dist` and `web/.taiyaku-cache` are all gitignored, verified with `git status --ignored`.

```bash
git add -A web/
git commit -m "web: pnpm, and the link: protocol the gitignored pkg needs

pnpm hard-fails where npm ci tolerated a dangling symlink, measured:
ERR_PNPM_LINKED_PKG_DIR_NOT_FOUND on a fresh clone with pkg/ absent. So
the protocol changes with the package manager.

link: rather than file: because pnpm resolves file: to a hard-linked copy,
which serves pre-rebuild content after build-wasm.sh deletes and regenerates
pkg/. link: is a live symlink and tolerates a missing target, which is
exactly what npm ci did. Both properties re-measured here rather than
assumed to carry over."
```

---

### Task 2: The commands, and a gap `docs/ci.md` calls permanent

**Files:**
- Modify: `.forgejo/workflows/ci.yml:149-156` (the `web` job's four steps)
- Modify: `scripts/check-all.sh:184-215` (the `web` job section)

**Interfaces:**
- Consumes: Task 1's `web/pnpm-lock.yaml` and `pnpm -C web run <script>`.
- Produces: `scripts/check-all.sh` gains a lockfile-drift check. Task 6 documents it.

- [ ] **Step 1: Write the failing test — prove drift is undetectable today**

Introduce a deliberate lockfile drift and confirm the current gate ignores it:

```bash
python3 - <<'EOF'
import json
p = "web/package.json"
d = json.load(open(p))
d["dependencies"]["is-odd"] = "^3.0.1"
json.dump(d, open(p, "w"), indent=2)
open(p, "a").write("\n")
EOF
./scripts/check-all.sh --no-wasm
```

- [ ] **Step 2: Run it and confirm the gap**

Expected: `check-all.sh` prints `all green` (or `all green, except NOT MEASURED:` with only corpus/wasm skips). `package.json` now names a dependency the lockfile has never heard of and nothing objects. That is the gap `docs/ci.md:121` describes.

Leave the drift in place — Step 5 uses it.

- [ ] **Step 3: Rewrite the CI job**

In `.forgejo/workflows/ci.yml`, replace the `web` job's `Install` step and the three that follow:

```yaml
      - name: Install pnpm
        # `pnpm/action-setup` is a JavaScript action, and this workflow's
        # opening comment records the measured finding that `container:` plus
        # any JS action dies before its first step on this runner. A global
        # npm install needs nothing from the image and cannot hit that.
        run: npm i -g pnpm@11.3.0
      - name: Install
        run: pnpm -C web install --frozen-lockfile
      - name: Lint
        run: pnpm -C web run lint
      - name: Typecheck
        run: pnpm -C web run typecheck
      - name: Test
        run: pnpm -C web run test
```

- [ ] **Step 4: Rewrite the local mirror**

In `scripts/check-all.sh`, replace the comment paragraph beginning `**\`npm ci\` is deliberately not run` (line 187) with:

```bash
# **The lockfile *is* checked here, and it did not used to be.** `docs/ci.md` carried this as a
# permanent gap: CI's install builds `node_modules` from nothing, while running the same command
# locally would delete a working install and rebuild it on every gate run — so the one property it
# checks, that `package.json` and the lockfile agree, went unchecked and a drifted lockfile passed
# this script and failed the forge. pnpm has a command for exactly that check and nothing else:
# `--lockfile-only` verifies the lockfile satisfies `package.json` without touching `node_modules`.
# Measured at 214ms. It is run directly rather than piped, because a pipe would mask its exit code.
```

Then replace lines 195–200 in full — the `elif` line through the last of the three `run npm` lines:

```bash
elif [ ! -d web/node_modules ]; then
  note_skip "the web job (web/node_modules absent — run \`pnpm -C web install\`)"
else
  run pnpm -C web install --frozen-lockfile --lockfile-only
  run pnpm -C web run lint
  run pnpm -C web run typecheck
  run pnpm -C web run test
```

And in the browser-test skip message further down, replace the hint:

```bash
    note_skip "the browser tests (no Chromium — run \`pnpm -C web exec playwright install chromium\`)"
```

- [ ] **Step 5: Run the gate and confirm it now catches the drift**

```bash
./scripts/check-all.sh --no-wasm
```

Expected: **non-zero exit**, with

```
specifiers in the lockfile don't match specifiers in package.json:
* 1 dependencies were added: is-odd@^3.0.1
```

- [ ] **Step 6: Revert the drift and confirm green**

```bash
git checkout web/package.json
./scripts/check-all.sh --no-wasm
```

Expected: exit 0. `git status --porcelain web/package.json` prints nothing.

- [ ] **Step 7: Commit**

```bash
git add .forgejo/workflows/ci.yml scripts/check-all.sh
git commit -m "The web gate runs pnpm, and now checks the lockfile it never checked

docs/ci.md carried the npm ci gap as permanent: mirroring it locally would
delete a working install every run, so package.json and the lockfile agreeing
went unverified and drift passed this script to fail the forge.

pnpm --lockfile-only is that check and nothing else — 214ms, node_modules
untouched. Verified by adding a dependency the lockfile had never seen: the
gate was green before this change and fails after it, naming the package.

CI installs pnpm with npm i -g rather than pnpm/action-setup, because this
workflow already records that container plus any JS action dies on this
runner."
```

---

### Task 3: Node 26, including the Node 18 nobody wrote down

**Files:**
- Modify: `.forgejo/workflows/ci.yml:109-110` (the `wasm` job's node install)
- Modify: `.forgejo/workflows/ci.yml:137` (the `web` job's image)
- Modify: `web/package.json` (`@types/node`)
- Create: `.node-version`

**Interfaces:**
- Consumes: Task 2's rewritten `web` job.
- Produces: `.node-version` as the single stated source for the pin. Task 6 documents it.

- [ ] **Step 1: Write the failing test — show what the `wasm` job actually runs**

The claim being fixed is that `apt-get install nodejs` on `rust:1-bookworm` yields Node 18, not 22 or 26. Verify it against Debian's own index rather than trusting the workflow:

```bash
curl -s "https://sources.debian.org/api/src/nodejs/" \
  | python3 -c "import json,sys; [print(v['version'], v['suites']) for v in json.load(sys.stdin)['versions'] if 'bookworm' in v.get('suites',[])]"
```

Expected: `18.20.4+dfsg-1~deb12u2 ['bookworm', 'bookworm-proposed-updates']`. So `scripts/smoke-wasm.mjs` runs on **Node 18** while the `web` job runs on 22 — a skew nobody chose.

- [ ] **Step 2: Bump the `web` job's image**

In `.forgejo/workflows/ci.yml`, in the `web` job:

```yaml
    container:
      image: node:26-bookworm
```

- [ ] **Step 3: Replace the `wasm` job's node install**

Replace the `Install node` step (lines 109–110) with:

```yaml
      - name: Install node
        # Debian bookworm's `nodejs` package is 18.20.4, so this step used to
        # run the smoke test on Node 18 while the `web` job ran 22 — not a
        # decision, just what `apt-get` happened to ship. A prebuilt tarball
        # pins it to the same 26.7.0 everything else uses, following the
        # precedent this job already sets for `wasm-pack`. The checksum is
        # from nodejs.org's own SHASUMS256.txt for this file.
        run: |
          curl -fsSL -o /tmp/node.tar.xz \
            https://nodejs.org/dist/v26.7.0/node-v26.7.0-linux-x64.tar.xz
          echo "982aa24dd8be4c889c6a8ab337ddff3b0896645b20f4239356e80552c16277ee  /tmp/node.tar.xz" | sha256sum -c -
          tar -xJf /tmp/node.tar.xz -C /opt
          ln -s /opt/node-v26.7.0-linux-x64/bin/node /usr/local/bin/node
          node --version
```

- [ ] **Step 4: Pin the version at the repository root**

Create `.node-version`:

```
26
```

- [ ] **Step 5: Bump `@types/node`**

The specifier is quoted because `^` and `~` are glob metacharacters in zsh, and this repository's shell is fish while agent tooling runs zsh — an unquoted range silently becomes a different one or a "no matches" error.

```bash
pnpm -C web add -D "@types/node@^26.2.0"
```

- [ ] **Step 6: Verify the typecheck still passes**

`@types/node` is a major bump and `tsconfig.node.json` sets `"types": ["node"]`, so this is where a breakage would surface.

```bash
pnpm -C web run typecheck
```

Expected: exit 0.

- [ ] **Step 7: Run the full gate**

```bash
./scripts/check-all.sh
```

Expected: `all green` apart from the corpus checks, which skip because `TAIYAKU_CORPUS` is unset. The `wasm` job runs locally here — its node install step is CI-only, so this confirms the surrounding job still passes.

- [ ] **Step 8: Commit**

```bash
git add .forgejo/workflows/ci.yml .node-version web/package.json web/pnpm-lock.yaml
git commit -m "Node 26 in both jobs that have a node, and one of them was on 18

The web job moves from node:22-bookworm to node:26-bookworm. 26 is Current
rather than LTS until 2026-10-28, taken deliberately so local and CI agree
exactly — version skew between a developer's node and the gate's is what
hides a defect until the forge finds it.

The wasm job was quietly on Node 18: Debian bookworm's nodejs package is
18.20.4, confirmed against Debian's index, so the only thing anywhere that
executes the compiled module ran on a different major from everything else.
It now fetches a checksummed 26.7.0 tarball, the same prebuilt-binary
approach the job already uses for wasm-pack.

.node-version states the pin once. @types/node follows the runtime to ^26."
```

---

### Task 4: TypeScript 7 — the compiler that is no longer JavaScript

**Files:**
- Modify: `web/package.json` (`typescript`)

**Interfaces:**
- Consumes: Task 1's pnpm install.
- Produces: nothing new. `"typecheck": "tsc -b"` is deliberately unchanged.

- [ ] **Step 1: Record the TypeScript 6 baseline**

```bash
rm -rf web/node_modules/.tmp
time web/node_modules/.bin/tsc -b web/tsconfig.json
```

Expected: exit 0. Note the wall time — roughly 1.5s on this machine.

- [ ] **Step 2: Install TypeScript 7**

```bash
pnpm -C web add -D "typescript@~7.0.2"
```

- [ ] **Step 3: Confirm `tsc` is now the native binary, and that no `tsgo` exists**

The request was to "use tsgo". This step is what proves no script change is needed.

```bash
file web/node_modules/@typescript/typescript-linux-x64/lib/tsc
find web/node_modules -name "tsgo*" | head
python3 -c "import json;print(json.load(open('web/node_modules/typescript/package.json'))['bin'])"
```

Expected: the `file` output contains `ELF 64-bit LSB executable` and `Go BuildID=`; the `find` prints nothing; the bin field is exactly `{'tsc': './bin/tsc'}`. In TypeScript 7, `tsc` **is** tsgo — `tsgo` was the bin name in `@typescript/native-preview`, the preview channel.

- [ ] **Step 4: Measure the native compiler on the real tsconfigs**

```bash
rm -rf web/node_modules/.tmp
time web/node_modules/.bin/tsc -b web/tsconfig.json
rm -rf web/node_modules/.tmp
time web/node_modules/.bin/tsc -b web/tsconfig.json
```

Expected: exit 0 both times, at roughly 0.16s — about 9.5× faster than Step 1. Record the actual numbers; Task 6 writes them into `docs/`.

- [ ] **Step 5: Verify it is checking, not no-opping**

A full typecheck in 0.16s invites the suspicion that nothing ran. Prove otherwise:

```bash
printf 'export const probe: number = "not a number";\n' > web/src/__ts7probe.ts
web/node_modules/.bin/tsc -b web/tsconfig.json; echo "exit=$?"
```

Expected: **exit 1**, with

```
src/__ts7probe.ts(1,14): error TS2322: Type 'string' is not assignable to type 'number'.
```

Note the exit code: TypeScript 6 exits `2` on type errors and 7 exits `1`. Both non-zero, so no gate changes — but it is a real behavioural difference and Task 6 records it.

- [ ] **Step 6: Remove the probe and confirm the tree is clean**

```bash
rm -f web/src/__ts7probe.ts
git status --porcelain web/src/
```

Expected: no output.

- [ ] **Step 7: Run the full gate**

```bash
./scripts/check-all.sh
```

Expected: `all green` apart from the corpus skips.

- [ ] **Step 8: Commit**

```bash
git add web/package.json web/pnpm-lock.yaml
git commit -m "TypeScript 7, and the typecheck script that needed no change

tsc in 7.0.2 dispatches to a statically linked Go binary — verified here as
an ELF with a Go BuildID, with no tsgo bin present anywhere in the package.
tsgo was the name in @typescript/native-preview, the preview channel. So
'typecheck': 'tsc -b' already gets the native compiler and stays verbatim.

Measured against this repository's real tsconfigs, cold, twice each:
1.56s -> 0.16s wall, 3.2s -> 0.8s CPU. Verified as a real check rather than
a no-op by injecting a deliberate type error, which both compilers caught at
the identical position with the identical code.

Project references, tsc -b build mode, moduleResolution bundler,
verbatimModuleSyntax and erasableSyntaxOnly all work on the native port.
One difference recorded: type errors exit 1 under 7 where 6 exited 2."
```

---

### Task 5: The rest of the dependency refresh

**Files:**
- Modify: `web/package.json`, `web/pnpm-lock.yaml`

**Interfaces:**
- Consumes: Tasks 1, 3 and 4.
- Produces: nothing structural. This is the caret-range sweep.

- [ ] **Step 1: See what is actually behind**

```bash
pnpm -C web outdated || true
```

Expected: at most `vite` (8.2.0 → 8.2.1) and `@vitejs/plugin-react` (6.0.4 → 6.0.5), both already inside their carets. `pnpm outdated` exits non-zero when anything is listed, hence the `|| true`.

- [ ] **Step 2: Update**

```bash
pnpm -C web update --latest
```

- [ ] **Step 3: Confirm the two majors were not disturbed**

```bash
python3 -c "
import json
d = json.load(open('web/package.json'))
print('typescript      ', d['devDependencies']['typescript'])
print('@types/node     ', d['devDependencies']['@types/node'])
print('taiyaku-wasm    ', d['dependencies']['taiyaku-wasm'])
print('packageManager  ', d['packageManager'])
"
```

Expected exactly:

```
typescript       ~7.0.2
@types/node      ^26.2.0
taiyaku-wasm     link:../crates/taiyaku-wasm/pkg
packageManager   pnpm@11.3.0
```

If `taiyaku-wasm` has reverted to `file:`, restore `link:` and re-run `pnpm -C web install` — the design's §2 measurements are the reason.

- [ ] **Step 4: Run the full gate plus the browser tests**

```bash
./scripts/check-all.sh
pnpm -C web run test:browser
```

Expected: `all green` apart from corpus skips, and the browser project exits 0. The vitest test count must match the number recorded in Task 1 Step 1.

- [ ] **Step 5: Verify a production build**

This is the one command that type-checks `resolver.ts` against the binding's real generated types, so it must run after `build-wasm.sh`:

```bash
./scripts/build-wasm.sh
pnpm -C web run build
```

Expected: both exit 0.

- [ ] **Step 6: Commit**

```bash
git add web/package.json web/pnpm-lock.yaml
git commit -m "Refresh the caret ranges

vite and @vitejs/plugin-react were the only packages behind, both inside
their existing carets. vitest, biome, react, react-dom and playwright were
already current. The two majors from earlier tasks are unchanged, checked
explicitly because pnpm update --latest would happily rewrite the
taiyaku-wasm protocol back to something that does not work here."
```

---

### Task 6: The documentation whose measurements are now false

**Files:**
- Modify: `docs/ci.md:13` (jobs table), `:40` (skip example), `:121-127` (the gap), `:196-201` (web job image), `:279-292` (the `file:` paragraph)
- Modify: `docs/status.md` (one entry)

**Interfaces:**
- Consumes: the measurements recorded in Tasks 1–5.
- Produces: nothing. Terminal task.

- [ ] **Step 1: Update the jobs table**

`docs/ci.md:13`:

```markdown
| `web` | `pnpm install --frozen-lockfile`, `biome check` (formatting **and** lint), `tsc -b` typecheck, `vitest run` in `web/` |
```

- [ ] **Step 2: Update the skip example**

`docs/ci.md:40`:

```markdown
  - the web job (web/node_modules absent — run `pnpm -C web install`)
```

- [ ] **Step 3: Replace the gap paragraph**

Replace `docs/ci.md:121-127` — the paragraph beginning `**\`npm ci\` is not mirrored` — with:

```markdown
**The lockfile check *is* mirrored, and until 2026-08-09 it was not.** This
document used to carry that as a permanent gap: CI's install builds
`node_modules` from nothing, while running the same command locally would
delete a working install and rebuild it on every gate run, so the one property
it checks — that `package.json` and the lockfile agree — went unchecked, and a
drifted lockfile passed `check-all.sh` to fail the forge.

pnpm closes it, because it has a command that is that check and nothing else:
`pnpm -C web install --frozen-lockfile --lockfile-only` verifies the lockfile
satisfies `package.json` without touching `node_modules`. **Measured at 214ms.**
Verified by adding a dependency the lockfile had never seen — the gate was green
before the step existed and failed after it, naming the package:

    specifiers in the lockfile don't match specifiers in package.json:
    * 1 dependencies were added: is-odd@^3.0.1

It is run directly rather than piped, because a pipe masks the exit code — which
was caught while writing the probe, and is exactly how a gate stops gating.
```

- [ ] **Step 4: Update the `web` job section**

`docs/ci.md:201`, change `node:22-bookworm` to `node:26-bookworm`, and append this paragraph to that section:

```markdown
**Node 26 is Current, not LTS, and that was chosen rather than defaulted into.**
Per `nodejs/Release`'s own schedule, 26 goes LTS on 2026-10-28 and reaches EOL
on 2029-04-30; 24 has been Active LTS since 2025-10-28. Taking 26 means some
weeks on a Current release, bought against local and CI agreeing exactly — this
development machine runs 26.7.0, and skew between a developer's node and the
gate's is the kind of gap that hides a defect until the forge finds it.

**The `wasm` job was on Node 18 and nothing said so.** It installed node with
`apt-get install -y nodejs`, and Debian bookworm's `nodejs` package is
`18.20.4+dfsg-1~deb12u2` — confirmed against Debian's own package index. So the
only thing anywhere that *executes* the compiled module ran on a different major
from every other job. That was not a decision; it is what `apt-get` happened to
ship. It now fetches a checksummed 26.7.0 tarball, the same prebuilt-binary
approach this job already takes for `wasm-pack`. **Guess, not measurement:** the
larger download should be roughly a wash against `apt-get`'s recorded 20–30s,
but no runner was reachable to time it. Confirm against a real run.
```

- [ ] **Step 5: Rewrite the `file:` paragraph — the load-bearing one**

Replace `docs/ci.md:279-292`, the paragraph beginning `**The \`file:\` dependency and a fresh clone.**`, with:

```markdown
**The `link:` dependency and a fresh clone.** `web/package.json` depends on
`taiyaku-wasm` via `"link:../crates/taiyaku-wasm/pkg"`, and `pkg/` is
gitignored — it does not exist in a clone that has only ever run `git fetch`.
This is verified rather than assumed, and it was re-verified when the package
manager changed rather than carried over: cloning the repository fresh (so
`crates/taiyaku-wasm/pkg/` is absent entirely) and running
`pnpm -C web install --frozen-lockfile` succeeds anyway, creating a dangling
symlink at `node_modules/taiyaku-wasm` without reading or validating anything
at the far end. `pnpm run build`, `typecheck` and `test` then all pass, because
nothing reachable from any of them *requires* that symlink to resolve. No
`optionalDependencies` fallback was needed.

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
```

- [ ] **Step 6: Add the TypeScript section**

Append a new section to `docs/ci.md`, after the `web` job section:

```markdown
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
runs:

| | wall | CPU |
| --- | ---- | --- |
| TS 6.0.3 | 1.56s | 3.2s |
| TS 7.0.2 | **0.16s** | 0.8s |

A full typecheck in 0.16s invites the suspicion that nothing ran, so that was
checked too: injecting `export const probe: number = "not a number"` into
`web/src` made both compilers fail at the identical position with the identical
code, `TS2322`. Project references, `tsc -b` build mode, `moduleResolution:
bundler`, `verbatimModuleSyntax`, `erasableSyntaxOnly` and
`allowImportingTsExtensions` all work on the native port.

**One behavioural difference, recorded rather than glossed:** on type errors
TS 6 exits `2` and TS 7 exits `1`. Both are non-zero so no gate changes, but
anything that ever tests for a specific code would break.
```

- [ ] **Step 7: Add the `docs/status.md` entry**

Add to the "What is built" section, matching the surrounding entries' style. No open item is opened or closed — the gap closed in Task 2 was prose in `docs/ci.md`, not a numbered O-item, so the counts in the "Open items" preamble stay exactly as they are.

```markdown
- **The web toolchain is pnpm, Node 26 and TypeScript 7** (2026-08-09). A
  toolchain branch that touched no Rust and no reader behaviour. Three things
  came out of it worth keeping: pnpm rejects the gitignored `file:` dependency
  outright and resolves it to a stale hard-linked copy when it does not, so the
  protocol is `link:`; the `wasm` job had been running the compiled module on
  Node 18 because that is what Debian bookworm ships, against 22 everywhere
  else; and `check-all.sh` now checks that `package.json` and the lockfile
  agree — 214ms, a gap `docs/ci.md` had carried as permanent. TypeScript 7's
  `tsc` is a Go binary, which took the typecheck from 1.56s to 0.16s with no
  script change. bun was measured against, not merely dismissed: the whole
  JavaScript gate is about 2.5 seconds, so there is no runtime budget to
  recover, and the one place a faster engine could apply is vitest, which bun
  cannot run in browser mode.
```

- [ ] **Step 8: Check every changed claim against the tree**

```bash
grep -rn "npm ci\|npm --prefix\|npx --prefix\|node:22\|package-lock" docs/ scripts/ .forgejo/ README.md web/README.md
```

Expected: no hits outside historical narrative that is explicitly dated to the past. Any live instruction still naming npm is a miss — fix it.

- [ ] **Step 9: Run the full gate one last time**

```bash
./scripts/check-all.sh
pnpm -C web run test:browser
```

Expected: `all green` apart from the corpus skips, and the browser project exits 0.

- [ ] **Step 10: Commit**

```bash
git add docs/ci.md docs/status.md
git commit -m "Document the toolchain, including the claims that stopped being true

docs/ci.md verified, in the npm era, that npm ci resolves a file: entry to a
dangling symlink on a fresh clone. That claim is false under pnpm and the
paragraph is rewritten with what was measured instead — including why file:
is rejected on two counts rather than one, so a later reader knows link: was
chosen against evidence rather than by preference.

The npm ci gap paragraph goes, because the gap is closed. The web job section
gains the Node 26 trade-off and the Node 18 the wasm job had been running on.
A new section records the TypeScript 7 measurements and the tsgo that does
not exist."
```

---

## Verification of the whole branch

After Task 6, from a clean tree:

```bash
./scripts/check-all.sh
pnpm -C web run test:browser
./scripts/build-wasm.sh && pnpm -C web run build
git status --porcelain
```

All four must succeed, with the last printing nothing.

## What this branch does not do

A test-coverage gate on the `web` job. Requested in the same conversation and deliberately deferred to its own branch stacked on this one, so that an argument about coverage thresholds cannot be confused with a package-manager regression.
