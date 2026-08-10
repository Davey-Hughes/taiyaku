# Test Coverage Gate Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Bring the untested parts of both languages under test and gate every PR on measured coverage thresholds, with the deliberately-untested periphery excluded by name rather than by wildcard.

**Architecture:** Infrastructure first on both sides (Tasks 1–2) so every later task can see its own effect. Then the web component work smallest-first (Tasks 3–9), then `App.tsx`'s extraction, which is read before it is designed (Tasks 10–14), then Rust (Tasks 15–16). Thresholds are set last, at the numbers actually achieved (Task 17).

**Tech Stack:** vitest 4.1.10 with `@vitest/coverage-v8`, jsdom, React 19 via `react-dom/client`, `cargo-llvm-cov` 0.8.7, Forgejo Actions.

**Design doc:** `docs/superpowers/specs/2026-08-09-coverage-gate-design.md`

## Global Constraints

- **`coverage.include` must always be set explicitly.** Without it v8 reports only files a test already imported — 98.43% over 15 files, where 50 belong in the gate. This is the defect the whole branch exists to prevent.
- **Read coverage from `coverage-summary.json`, never from the `text` table.** Measured while writing this plan: the text reporter prints 28 files while the JSON summary holds 50 — it omits files that are at 100% on every metric. Twenty-two fully-covered files are invisible in the table and *are* counted in the totals. A task that reads the table as its file list will conclude those files are unmeasured and go write tests that already exist.
- **`--coverage.include` cannot be repeated on the command line** — the last flag wins, silently. Set it in `web/vite.config.ts` and run the plain script.
- **Exclusions are a named list with a reason each, never a wildcard.** The seven entries are fixed by the design's §3 and no task may add an eighth without recording why in `docs/ci.md`.
- **`resolver.ts` is not excluded.** It is 0% because nothing loads it, which is a testability problem with a fix, not a property deserving amnesty.
- **No `@testing-library/react`.** `jsdom` is the only new web dependency. Components mount through the shared `mount(ui)` helper extracted from `PageView.browser.test.tsx`. Adding a second mounting API needs a stated reason, per this repository's rule.
- **Thresholds are set in Task 17 only**, at measured numbers minus a small margin. No task before it may add a threshold.
- Command form is `pnpm -C web run <script>` from the repository root, bare `pnpm run <script>` from inside `web/`.
- Node 26.7.0, pnpm 11.3.0, TypeScript `~7.0.2`. `taiyaku-wasm` stays `link:`.
- No commit message may contain an attribution trailer (`Co-Authored-By`, `Generated with`, or similar).
- Every figure written into a comment, commit message or document must be measured by the task that writes it.

## A deliberate deviation from "exact code in every step"

Tasks 4–9 and 15–16 do not contain literal test bodies. Writing invented assertions for files nobody has read would be fiction the implementer discards, and there are roughly 6,200 lines of components and 2,100 missed Rust regions in scope.

Instead those tasks carry: the file, its current measured coverage, the behaviours visible from its exports, and **an objective acceptance criterion — a per-file coverage number, measured.** Task 3 contains one fully-worked component test, written against a file that was read, as the pattern every later component task follows.

Tasks 1–3, 10 and 17 do carry exact content, because their content is knowable in advance.

---

### Task 1: Web coverage infrastructure

**Files:**
- Modify: `web/package.json`, `web/pnpm-lock.yaml`, `web/vite.config.ts`

**Interfaces:**
- Produces: `pnpm -C web run test:coverage`, and a `coverage` block every later task measures against.

- [ ] **Step 1: Install the provider**

```bash
pnpm -C web add -D "@vitest/coverage-v8@^4.1.10"
```

- [ ] **Step 2: Show the default number is wrong**

```bash
pnpm -C web exec vitest run --project node --coverage --coverage.reporter=text
```

Expected: about **98.43%** statements, and a table listing about **15 files**. Count them. `web/src` holds 52 source files, 50 of which belong in the gate. This is the failure the next step fixes — record both numbers in your report.

- [ ] **Step 3: Add the coverage block**

In `web/vite.config.ts`, inside `test:`, alongside `projects:`:

```ts
    // **`include` is set explicitly and that is the whole point.** v8 coverage
    // instruments what the run loaded, so a file no test imports is absent
    // from the report rather than sitting at 0%. Measured on this tree: the
    // default reports 98.43% over 15 files, while 50 are in the gate and the
    // real figure is 48.13%. A gate built on the default would enforce
    // nothing while reading beautifully.
    //
    // Exclusions are named individually with a reason, never a wildcard: a
    // wildcard silently swallows the next file that matches it, which is the
    // exact failure this gate exists to prevent.
    coverage: {
      provider: 'v8',
      include: ['src/**/*.ts', 'src/**/*.tsx'],
      exclude: [
        'src/**/*.test.ts',
        'src/**/*.test.tsx',
        'src/**/*.d.ts',
        // Entry point: ten lines that call createRoot. Nothing to assert.
        'src/main.tsx',
        // The vitest browser harness, not application code.
        'src/browser-setup.ts',
      ],
      reporter: ['text', 'json-summary'],
    },
```

- [ ] **Step 4: Add the script**

In `web/package.json`, after `"test:browser"`:

```json
    "test:coverage": "vitest run --project node --coverage",
```

- [ ] **Step 5: Fix the stale `file:` comment**

`web/vite.config.ts:10` still says `taiyaku-wasm` is a `file:` dependency. It became `link:` on the pnpm branch, and every sweep there missed it because the verification greps targeted `.md`, `.sh` and `.yml`. Change that line to say `link:`, and check the rest of the comment block still reads true.

- [ ] **Step 6: Measure the real baseline**

```bash
pnpm -C web run test:coverage
python3 -c "
import json
d = json.load(open('web/coverage/coverage-summary.json'))
print('files:', len(d) - 1)
print(d['total']['statements'])
"
```

Expected: **48.13%** statements (942 of 1957), **45.21%** branches, and **50 files** in the JSON.

**The text table will show only 28 files. That is not a bug and not a smaller include.** The reporter omits files at 100% on every metric; 22 fully-covered files are absent from the table and present in the totals. Read the JSON, not the table — this was measured while writing this plan, and a task that trusts the table will go write tests for files that already have them.

Record the exact figures. Task 17 needs them and every component task measures its delta against them.

- [ ] **Step 7: Confirm the gate still passes**

```bash
./scripts/check-all.sh --no-wasm
```

Expected: exit 0.

- [ ] **Step 8: Commit**

```bash
git add web/package.json web/pnpm-lock.yaml web/vite.config.ts
git commit -m "Web coverage, with the include that stops it lying

vitest --coverage with no include reports 98.43% over sixteen files while
src/ holds sixty-five: v8 instruments what the run loaded, so a file no test
imports is absent rather than zero, and the headline answers 'is the code we
test tested?'. Setting include explicitly gives 48.08%.

Exclusions are named with a reason each rather than globbed, because a
wildcard swallows the next file that matches it.

Also corrects vite.config.ts's comment calling taiyaku-wasm a file:
dependency. It became link: on the pnpm branch and every sweep there missed
it, because the greps targeted .md, .sh and .yml and this is a .ts."
```

---

### Task 2: Rust coverage, in CI and in the local gate

**Files:**
- Modify: `.forgejo/workflows/ci.yml` (the `test` job), `scripts/check-all.sh`

**Interfaces:**
- Produces: `cargo llvm-cov` running the workspace tests in both gates, with the four designed exclusions applied.

- [ ] **Step 1: Confirm the toolchain locally**

```bash
cargo llvm-cov --version
rustup component list --installed | grep llvm
```

Expected: `cargo-llvm-cov 0.8.7` and `llvm-tools-x86_64-unknown-linux-gnu`. Both are already present on this machine.

- [ ] **Step 2: Measure the unexcluded baseline**

```bash
env -u TAIYAKU_CORPUS cargo llvm-cov --workspace --all-features --summary-only
```

Expected: about **81.41%** regions, **82.73%** lines. Record it.

- [ ] **Step 3: Measure with the four exclusions**

`cargo-llvm-cov` takes a regex of paths to ignore:

```bash
env -u TAIYAKU_CORPUS cargo llvm-cov --workspace --all-features --summary-only \
  --ignore-filename-regex '(geometry_experiment|region_experiment|/corpus)\.rs$'
```

Expected: about **89.34%** regions. If it differs by more than a point from that, stop and report — the design's arithmetic (20,838 covered of 23,324) is what Task 17's threshold rests on.

Confirm the regex excludes **exactly four files** and no others, by diffing the file list against the unexcluded run.

- [ ] **Step 4: Replace `cargo test` in the CI `test` job**

`cargo llvm-cov` runs the tests, so this replaces the existing step rather than adding a job — the cost is instrumentation on a build CI already pays for, not a second cold compile. In `.forgejo/workflows/ci.yml`, in the `test` job, after the checkout:

```yaml
      - name: Add llvm-tools
        run: rustup component add llvm-tools-preview
      - name: Install cargo-llvm-cov
        # A prebuilt release binary rather than `cargo install`, the precedent
        # the `wasm` job already sets for `wasm-pack`: this job only runs the
        # tool, never links against it. The musl build is static, so it runs on
        # the glibc bookworm container without a matching libc.
        run: |
          curl -fsSL -o /tmp/llvm-cov.tar.gz \
            https://github.com/taiki-e/cargo-llvm-cov/releases/download/v0.8.7/cargo-llvm-cov-x86_64-unknown-linux-musl.tar.gz
          tar -xzf /tmp/llvm-cov.tar.gz -C /usr/local/bin
          cargo llvm-cov --version
      - name: Tests, with coverage
        # `cargo llvm-cov` runs the tests — this is the `cargo test` step, not
        # an addition to it. The four ignored files are named in docs/ci.md:
        # two gated one-off measurement modules, and two that cannot execute
        # without TAIYAKU_CORPUS, which CI does not have.
        run: |
          cargo llvm-cov --workspace --all-features \
            --ignore-filename-regex '(geometry_experiment|region_experiment|/corpus)\.rs$'
```

Remove the old `Tests` step that ran `cargo test --workspace --all-features`.

- [ ] **Step 5: Mirror it in the local gate**

In `scripts/check-all.sh`, replace the `test` job's `cargo test` invocation with the same `cargo llvm-cov` command, guarded so a machine without the tool names the skip rather than failing:

```bash
if ! command -v cargo-llvm-cov >/dev/null 2>&1; then
  note_skip "coverage on the test job (cargo-llvm-cov not on PATH — tests still ran)"
  run env -u TAIYAKU_CORPUS cargo test --workspace --all-features
else
  run env -u TAIYAKU_CORPUS cargo llvm-cov --workspace --all-features \
    --ignore-filename-regex '(geometry_experiment|region_experiment|/corpus)\.rs$'
fi
```

Keep the existing `env -u TAIYAKU_CORPUS` treatment — `docs/ci.md` records that the `test` job must not inherit the corpus, and that this is what took a gate run past an hour before it was fixed.

- [ ] **Step 6: Run the full gate**

```bash
./scripts/check-all.sh
```

Expected: exit 0, corpus skip only, and a coverage summary printed by the `test` job.

- [ ] **Step 7: Commit**

```bash
git add .forgejo/workflows/ci.yml scripts/check-all.sh
git commit -m "Rust coverage, replacing cargo test rather than joining it

cargo llvm-cov runs the tests, so it takes the test job's existing step
instead of adding a job: the cost is instrumentation on a build CI already
pays for, not a second cold compile. The binary is a 1.8 MB prebuilt musl
tarball, the approach the wasm job already takes for wasm-pack.

Four files are ignored by name. Two are gated one-off measurement modules
and two cannot execute without TAIYAKU_CORPUS, which CI never has. They hold
2,443 of the 4,929 missed regions, so excluding them moves the measured
figure from 81.41% to 89.34% without any work — the earlier number was
averaging in code nobody intends to test."
```

---

### Task 3: The jsdom project, the shared mount helper, and the first component

**Files:**
- Create: `web/src/testing/mount.ts`, `web/src/Viewport.test.tsx`
- Modify: `web/package.json`, `web/pnpm-lock.yaml`, `web/vite.config.ts`, `web/src/PageView.browser.test.tsx`

**Interfaces:**
- Produces: `mount(ui): HTMLElement` and a `jsdom` vitest project. Tasks 4–9 and 14 use both.

- [ ] **Step 1: Install jsdom**

```bash
pnpm -C web add -D jsdom
```

- [ ] **Step 2: Extract the mount helper**

`web/src/PageView.browser.test.tsx` already carries this pattern. Move it to `web/src/testing/mount.ts` so both projects share one mounting API — and so no second one gets added:

```ts
import { act } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { afterEach } from 'vitest'

/**
 * Mount a component and hand back its host element.
 *
 * Extracted from `PageView.browser.test.tsx`, which had it inline, so the
 * `jsdom` and `browser` projects mount identically. Deliberately not
 * `@testing-library/react`: this repository's rule is that a dev-only
 * dependency needs a stated reason, and a second mounting API earns nothing
 * over the nine lines already working here.
 */
let root: Root | undefined
let host: HTMLElement | undefined

afterEach(() => {
  act(() => root?.unmount())
  host?.remove()
  root = undefined
  host = undefined
})

export function mount(ui: React.ReactNode): HTMLElement {
  host = document.createElement('div')
  document.body.append(host)
  root = createRoot(host)
  act(() => root?.render(ui))
  return host
}
```

- [ ] **Step 3: Point the browser test at the shared helper**

In `web/src/PageView.browser.test.tsx`, delete the inline `root`/`host`/`afterEach`/`mount` block and import instead:

```ts
import { mount } from './testing/mount'
```

- [ ] **Step 4: Verify the browser tests still pass**

```bash
pnpm -C web run test:browser
```

Expected: 6/6 passing. If they fail, the extraction is wrong — fix it before going further, because Tasks 4–9 all build on this helper.

- [ ] **Step 5: Add the jsdom project**

In `web/vite.config.ts`, in the `projects` array between `node` and `browser`:

```ts
      {
        extends: true,
        test: {
          // **Components, in jsdom rather than in the browser project.** The
          // browser project needs a downloaded Chromium that CI deliberately
          // does not carry, so anything measured there is invisible to the
          // gate and a developer's local number would exceed CI's. A gate has
          // to measure what CI can reproduce. What jsdom structurally cannot
          // see — real layout, real HTMLImageElement.decode() — stays in the
          // browser project, which is why both exist.
          name: 'jsdom',
          environment: 'jsdom',
          include: ['src/**/*.test.tsx'],
          exclude: ['src/**/*.browser.test.tsx'],
        },
      },
```

The `node` project's `include` is `src/**/*.test.ts`, which does not match `.tsx`, so the two do not overlap.

- [ ] **Step 6: Write the failing test**

`Viewport.tsx` is 70 lines, pure, and its docstring names the exact property worth pinning: a zero-sized rect would divide by zero, and it guards. Create `web/src/Viewport.test.tsx`:

```tsx
import { describe, expect, it } from 'vitest'
import { mount } from './testing/mount'
import { Viewport } from './Viewport'

const transformOf = (host: HTMLElement) =>
  (host.firstElementChild as HTMLElement).style.transform

describe('Viewport', () => {
  it('scales by the reciprocal of the rect so it fills the box', () => {
    const host = mount(
      <Viewport rect={{ x: 0, y: 0, w: 0.5, h: 0.5 }}>
        <span />
      </Viewport>,
    )
    expect(transformOf(host)).toContain('scale(2, 2)')
  })

  it('translates in the untransformed box’s own percent units', () => {
    const host = mount(
      <Viewport rect={{ x: 0.25, y: 0.1, w: 1, h: 1 }}>
        <span />
      </Viewport>,
    )
    expect(transformOf(host)).toContain('translate(-25%, -10%)')
  })

  it('guards a zero width rather than dividing by zero', () => {
    const host = mount(
      <Viewport rect={{ x: 0, y: 0, w: 0, h: 0.5 }}>
        <span />
      </Viewport>,
    )
    expect(transformOf(host)).toContain('scale(1, 2)')
  })

  it('guards a zero height rather than dividing by zero', () => {
    const host = mount(
      <Viewport rect={{ x: 0, y: 0, w: 0.5, h: 0 }}>
        <span />
      </Viewport>,
    )
    expect(transformOf(host)).toContain('scale(2, 1)')
  })

  it('sets transform-origin to the top left so the terms compose in order', () => {
    const host = mount(
      <Viewport rect={{ x: 0, y: 0, w: 1, h: 1 }}>
        <span />
      </Viewport>,
    )
    expect((host.firstElementChild as HTMLElement).style.transformOrigin).toBe('0 0')
  })

  it('renders its children inside the transformed wrapper', () => {
    const host = mount(
      <Viewport rect={{ x: 0, y: 0, w: 1, h: 1 }}>
        <span data-testid="child" />
      </Viewport>,
    )
    expect(host.querySelector('[data-testid="child"]')).not.toBeNull()
  })
})
```

- [ ] **Step 7: Run it and watch it fail for the right reason**

```bash
pnpm -C web exec vitest run --project jsdom
```

Expected on first run: the project resolves and the tests execute. If instead the project is not found, Step 5 is wrong; if `document is not defined`, `environment: 'jsdom'` did not take.

- [ ] **Step 8: Make it pass**

`Viewport.tsx` needs no change — these tests describe what it already does. Any failure here is a test bug, not a component bug. Fix the test.

- [ ] **Step 9: Add the script and wire the gate**

In `web/package.json`:

```json
    "test:jsdom": "vitest run --project jsdom",
```

Change `test:coverage` to cover both non-browser projects:

```json
    "test:coverage": "vitest run --project node --project jsdom --coverage",
```

In `scripts/check-all.sh`, add `run pnpm -C web run test:jsdom` beside the existing `run pnpm -C web run test`, and in `.forgejo/workflows/ci.yml`'s `web` job add a matching step. The jsdom project needs no Chromium, so unlike the browser tests it belongs in CI.

- [ ] **Step 10: Measure**

```bash
pnpm -C web run test:coverage
```

Expected: `Viewport.tsx` at or near 100%, total up from 48.08% by a fraction of a point. Record both.

- [ ] **Step 11: Commit**

```bash
git add web/package.json web/pnpm-lock.yaml web/vite.config.ts web/src/testing/mount.ts web/src/Viewport.test.tsx web/src/PageView.browser.test.tsx scripts/check-all.sh .forgejo/workflows/ci.yml
git commit -m "A jsdom project, one shared mount helper, and Viewport under test

Components go in a third vitest project rather than the browser one, because
the browser project needs a Chromium CI deliberately does not carry — so
anything measured there is invisible to the gate and a developer's number
would exceed CI's. A gate has to measure what CI can reproduce.

The mount helper is PageView.browser.test.tsx's own, extracted rather than
replaced. No @testing-library/react: a dev-only dependency needs a stated
reason here, and a second mounting API earns nothing over nine working lines.

Viewport is first because it is 70 lines and its docstring names the property
worth pinning — a zero-sized rect divides by zero, and it guards. Both guards
now have a test."
```

---

### Tasks 4–9: The remaining components

Each task follows Task 3's pattern exactly: read the component, write `<Name>.test.tsx` against the `jsdom` project using `mount` from `./testing/mount`, run `pnpm -C web run test:coverage`, and commit.

**The acceptance criterion for each file is measured, not enumerated: ≥90% statements and ≥85% branches for that file in the coverage table.** Where a component cannot reach that — a branch reachable only through real layout, say — say so in the report with the specific lines and why, rather than lowering the bar silently.

Every task ends with `./scripts/check-all.sh --no-wasm` green and a commit whose message states the file's measured before and after.

**Sizes below are statements, not lines, because statements are what the gate counts.** Measured from `coverage-summary.json`: the ten remaining components plus `resolver.ts` come to **263 statements between them**, against `App.tsx`'s 737. Line counts make `Filmstrip.tsx` look like a third of `Annotations.tsx`; in statements it is half. Do not size this work by `wc -l`.

- [ ] **Task 4: `Gap.tsx` (4) and `NoteList.tsx` (10)** — both 0%. The two smallest; expect these to be quick.
- [ ] **Task 5: `Help.tsx` (10) and `CreditsPanel.tsx` (8)** — both 0%. `Help.tsx` pairs with `modules/help.ts`, already at 100%, so check what is genuinely uncovered before writing.
- [ ] **Task 6: `Settings.tsx` (31) and `Chrome.tsx` (20)** — both 0%.
- [ ] **Task 7: `NotePanel.tsx` (27) and `PageView.tsx` (24)** — both 0% **in the gate**. `PageView.tsx` reads 92.3% locally via the browser project, but `test:coverage` runs `node` and `jsdom` only, so the gate sees 0% and the threshold would be dragged down by work that already exists. Cover in jsdom whatever does not need real layout; leave the decode and geometry properties to the existing browser test and say in your report which lines those are. `NotePanel.tsx` pairs with `notes.ts` at 99.13% — test the component, not the module again.
- [ ] **Task 8: `Filmstrip.tsx` (32) and `resolver.ts` (27)** — both 0%. `Filmstrip.tsx` imports `Position` from `resolver` as a type only, so it needs no wasm. `resolver.ts` does import `taiyaku-wasm` as a value, which is why nothing loads it; mock the module with `vi.mock('taiyaku-wasm', …)` and test `wasmResolver`'s conversion surface. It is explicitly **not** excluded — the design's §3 records that this is a testability problem with a fix, not a property deserving amnesty.
- [ ] **Task 9: `Annotations.tsx` (67)** — 0%, and the largest before `App.tsx`. If it proves to hold logic that wants extracting rather than testing in place, do that and say so — the same argument as `App.tsx`, at a size where it is still a judgement call.

---

### Task 10: Read `App.tsx` and write the extraction map

**Files:**
- Create: `docs/superpowers/specs/2026-08-09-app-extraction-map.md`

This task writes no code. Its deliverable is the design that Tasks 11–14 execute, and it exists because the design doc deliberately refused to invent module boundaries from a `grep` of hook counts.

**This is the task that decides whether the branch succeeds.** Measured from `coverage-summary.json`: `App.tsx` holds **737 uncovered statements of the 1,015 in the whole web tree — 73%.** The ten other components and `resolver.ts` together are 263. Tasks 4–9 are worth doing and will move the total by roughly 13 points; this one moves 36.

- [ ] **Step 1: Read the whole file**

`web/src/App.tsx`, all 3,360 lines. `App()` spans 334–3359.

**Do not trust a `grep -c` for the hook counts.** This file writes at length about its own hooks, so `grep -c useState` returns 33 where real call sites number in the low-to-mid twenties, and `useMemo` returns 2 where there are none. Strip comments before counting anything here. Roughly half of `App()` is comment — 47.6% by one measure, 50.8% by another — which is also why line counts overstate how much logic is present.

- [ ] **Step 2: Inventory the state**

For each `useState` and `useRef`, record: what it holds, which effects read and write it, and which other pieces of state it moves with. State that always changes together is one unit.

- [ ] **Step 3: Inventory the effects**

For each `useEffect`, record its dependencies, what it subscribes to or schedules, and its cleanup.

- [ ] **Step 4: Propose the modules**

Group into pure modules that sit beside `pairing.ts` and `viewport.ts` — no React import where possible, a custom hook only where React state genuinely must live. The design's starting hypothesis, to confirm or reject against what you actually read: correction and history; decode and prefetch orchestration; session and progress persistence; notes.

For each proposed module state: its name and path, its exported surface with types, which `App.tsx` lines move into it, and what stays behind.

- [ ] **Step 5: Sequence them**

Order the extractions so each is independently landable and testable, with the least coupled first. Name the ordering constraints.

- [ ] **Step 6: Sanity-check the residue**

State what `App()` is left holding and roughly how many lines. If that is still over about 500, the grouping is too coarse — go back to Step 4.

- [ ] **Step 7: Commit the map**

```bash
git add docs/superpowers/specs/2026-08-09-app-extraction-map.md
git commit -m "The extraction map for App.tsx, written from reading it

The coverage design refused to name module boundaries from a grep of hook
counts, which would have been exactly the kind of guess it forbids. This is
that decision made properly: every useState, useRef and useEffect inventoried,
grouped by what actually moves together, and sequenced so each extraction
lands on its own."
```

---

### Tasks 11–13: Extract the modules

One task per module group from Task 10's map, in the order it sequences. Each follows the same shape, which is TDD in the direction extraction allows:

- [ ] Write the new module's tests first, against the exported surface the map specifies.
- [ ] Run them and watch them fail — the module does not exist yet.
- [ ] Create the module by moving code out of `App.tsx`, changing behaviour nowhere.
- [ ] Run the new tests until they pass.
- [ ] Run `pnpm -C web run test` and `test:browser` — **654 and 6 must both still pass.** An extraction that changes behaviour is a defect, and this is how it is caught.
- [ ] Run `pnpm -C web run test:coverage` and record `App.tsx`'s new figure and the new module's.
- [ ] Commit.

**The invariant across all three: 654 node tests and 6 browser tests pass before and after every single extraction.** If a count changes, stop and report — moving code is not supposed to change what the reader does.

---

### Task 14: What `App.tsx` still holds

**Files:**
- Create: `web/src/App.test.tsx`

After Tasks 11–13, `App()` should be wiring. Cover it with a small number of integration tests through `mount`, with `./api` and `./resolver` mocked via `vi.mock`.

- [ ] Test that it renders without crashing against a mocked resolver and empty API responses.
- [ ] Test the two or three flows that cross module boundaries and therefore no unit test can see. Name them in the report.
- [ ] **Acceptance: `App.tsx` ≥70% statements.** Lower than the component bar deliberately — this is a shell, and the logic that matters now lives in tested modules. If it comes in far above 70%, say so; that suggests the extraction left more behind than intended.
- [ ] `pnpm -C web run test` and `test:browser` still 654 and 6.
- [ ] Commit.

---

### Task 15: `taiyaku-cli/src/main.rs`

**Files:**
- Modify: `crates/taiyaku-cli/src/main.rs` and its tests

Currently **43.79%**, **1,159 missed regions** — 55% of all remaining Rust work in one file. It is argument parsing and dispatch for nine commands.

- [ ] **Step 1: See exactly what is uncovered**

```bash
env -u TAIYAKU_CORPUS cargo llvm-cov --package taiyaku-cli --all-features --html
```

Open the report and list the uncovered regions by kind: argument parsing, dispatch arms, error paths, output formatting.

- [ ] **Step 2: Write tests for the largest uncovered group first**

Follow the crate's existing test conventions — read a covered neighbour like `detect.rs` (98.86%) or `duplicates.rs` (94.46%) for the pattern before inventing one.

- [ ] **Step 3: Measure after each group**

```bash
env -u TAIYAKU_CORPUS cargo llvm-cov --package taiyaku-cli --all-features --summary-only
```

- [ ] **Acceptance: `main.rs` ≥85% regions.** If a region is genuinely unreachable — a match arm the type system already excludes — say which and why rather than contorting a test to reach it.

- [ ] **Step 4: Full gate, then commit.**

---

### Task 16: The remaining Rust files

Same shape as Task 15, one commit per file or per coherent group. Current figures and targets:

| file | now | target |
| ---- | --- | ------ |
| `taiyaku-cli/src/report.rs` | 32.56% | ≥85% |
| `taiyaku-cli/src/cache.rs` | 12.50% | ≥85% |
| `taiyaku-cli/src/inventory.rs` | 72.94% | ≥85% |
| `taiyaku-cli/src/fingerprint.rs` | 77.49% | ≥85% |
| `taiyaku-serve/src/stump/http.rs` | 66.80% | ≥80% |
| `taiyaku-wasm/src/lib.rs` | 73.05% | ≥85% |
| `taiyaku-serve/src/lib.rs` | 84.46% | ≥90% |

`stump/http.rs` gets a lower bar because it is network code whose error paths need a fake server; if the existing `stump/` tests already have that harness, use it and raise the bar to 85%.

Each file: measure, test, re-measure, full gate, commit.

---

### Task 17: Thresholds, and proving the gate actually gates

**Files:**
- Modify: `web/vite.config.ts`, `.forgejo/workflows/ci.yml`, `scripts/check-all.sh`, `docs/ci.md`, `docs/status.md`

- [ ] **Step 1: Measure the achieved numbers**

```bash
pnpm -C web run test:coverage
env -u TAIYAKU_CORPUS cargo llvm-cov --workspace --all-features --summary-only \
  --ignore-filename-regex '(geometry_experiment|region_experiment|/corpus)\.rs$'
```

Record statements, branches, functions and lines for web; regions and lines for Rust.

- [ ] **Step 2: Set the thresholds just under them**

In `web/vite.config.ts`'s `coverage` block, add `thresholds` with each value **2 points below** what Step 1 measured, rounded down to a whole number. The margin absorbs the ordinary jitter of v8 counting, and is small enough that a real regression still trips it. Write the measured figure beside each threshold in a comment so a later reader can see the headroom.

- [ ] **Step 3: Set the Rust threshold**

Add `--fail-under-lines <N>` to the `cargo llvm-cov` invocation in both `ci.yml` and `check-all.sh`, again 2 points under measured.

- [ ] **Step 4: Prove a regression fails — web**

An unproven gate is indistinguishable from no gate.

```bash
# Skip a covered suite and confirm the threshold catches it
sed -i 's/^describe(/describe.skip(/' web/src/pairing.test.ts
pnpm -C web run test:coverage; echo "exit=$?"
git checkout web/src/pairing.test.ts
```

Expected: **non-zero exit**, naming the threshold that was not met. Then exit 0 again after the restore.

- [ ] **Step 5: Prove a regression fails — Rust**

Do the same on the Rust side: `#[ignore]` a covered test module in a file that is inside the gate, confirm `cargo llvm-cov --fail-under-lines` exits non-zero, restore, confirm it passes. Record both outputs.

- [ ] **Step 6: Confirm the exclusion list excludes only what it names**

```bash
env -u TAIYAKU_CORPUS cargo llvm-cov --workspace --all-features --summary-only > /tmp/all.txt
env -u TAIYAKU_CORPUS cargo llvm-cov --workspace --all-features --summary-only \
  --ignore-filename-regex '(geometry_experiment|region_experiment|/corpus)\.rs$' > /tmp/gated.txt
diff <(awk '{print $1}' /tmp/all.txt) <(awk '{print $1}' /tmp/gated.txt)
```

Expected: exactly four files missing — `geometry_experiment.rs`, `region_experiment.rs`, and both `corpus.rs`. Any fifth is a wildcard swallowing something, which the design forbids.

- [ ] **Step 7: Document**

In `docs/ci.md`, a new section covering: what each gate measures, the thresholds and the measured numbers they sit under, the exclusion list with a reason per entry, why `coverage.include` is set explicitly and what the default reported instead, and why component tests live in a `jsdom` project rather than the browser one.

In `docs/status.md`, one entry in the established style. Do not open a numbered O-item and do not change the open/closed counts.

- [ ] **Step 8: Full gate, then commit.**

```bash
./scripts/check-all.sh
```

Expected: exit 0, corpus skip only, both thresholds enforced and met.

---

## Verification of the whole branch

```bash
./scripts/check-all.sh
pnpm -C web run test:browser
./scripts/build-wasm.sh && pnpm -C web run build
git status --porcelain
```

All four succeed, the last printing nothing. Plus the two regression proofs from Task 17 steps 4 and 5, which are the only evidence that any of this is a gate rather than a report.
