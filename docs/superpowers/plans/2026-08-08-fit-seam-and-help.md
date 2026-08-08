# Fit, the seam, and the keybind help — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Pages fill their box whatever the scan resolution, the two pages in two-page mode meet at the seam, and `?` opens a keybind list that cannot drift from the bindings.

**Architecture:** Three independent changes to the browser reader, sequenced so each is easier to see once the one before it lands. A new `fit` `ReaderModule` holds the three geometry preferences; a pure `pageStyle` turns them into CSS so the rule is testable without a DOM. The seam is a layout change in `renderGroup` plus a side for the annotation tabs. The help is a refactor of `keys.ts` into one ordered table that `intentOf` dispatches from and the overlay lists from.

**Tech Stack:** TypeScript + React 19 + Vite, tested with Vitest. No Rust changes.

**Spec:** `docs/superpowers/specs/2026-08-08-fit-seam-and-help-design.md`. Read it before Task 1.

## Global Constraints

- **No new dependencies** in `web/`. The dependency list in `docs/status.md` must not grow.
- **No Rust, sidecar, aligner or server changes.** This is entirely `web/`.
- **Pure logic lives apart from components.** A pure function sharing a file with a component defeats React Fast Refresh and biome's `style/useComponentExportOnlyModules` flags it. Follow `pairing.ts` / `viewport.ts` / `keys.ts`.
- **`web/` has no DOM harness.** Vitest cannot construct elements — `document` is undefined. Test pure functions that return values; anything that is a rendered pixel is deferred to the controller's browser pass.
- **House documentation style, enforced strictly:** every decision records the alternative it beat and what it cost; a figure that is a measurement says where it came from; **a figure that is a guess says it is a guess.** This branch's predecessor spent nine review rounds on comments asserting things the code does not do — check every claim against the code before writing it, including claims copied from this plan.
- **Steps marked `[deferred: controller]`** are browser checks. Do not start a dev server (`npm run dev` blocks); run the automated checks named beside them and report the by-hand step as not run.
- **Run `scripts/check-all.sh` before the final commit.** It mirrors all four CI jobs.

## File Structure

| File | Responsibility |
| ---- | -------------- |
| `web/src/modules/fit.ts` (create) | The `fit` module: `upscaleToFit`, `upscaleFilter`, `gap`, plus the pure `pageStyle` and `seamStyle` that turn them into CSS. |
| `web/src/modules/fit.test.ts` (create) | Vitest for both pure functions. |
| `web/src/modules/help.ts` (create) | The `help` module and its two claimed keys. |
| `web/src/Help.tsx` (create) | The overlay component, modelled on `CreditsPanel.tsx`. |
| `web/src/keys.ts` (modify) | `intentOf` dispatches from a new exported ordered table; the table is also what the help lists. |
| `web/src/PageView.tsx` (modify) | Applies `pageStyle` instead of a fixed `fitWhole`. |
| `web/src/Annotations.tsx` (modify) | Tabs gain a side. |
| `web/src/App.tsx` (modify) | Threads fit prefs, seam layout, tab side, and the help overlay. |
| `web/src/modules/registry.ts` (+ `.test.ts`) (modify) | Two more modules in the one array. |

---

### Task 1: The `fit` module and its two pure rules

**Files:**
- Create: `web/src/modules/fit.ts`
- Create: `web/src/modules/fit.test.ts`
- Modify: `web/src/modules/registry.ts`, `web/src/modules/registry.test.ts`

**Interfaces:**
- Consumes: `ReaderModule` from `../module`.
- Produces: `interface FitPrefs { upscaleToFit: boolean; upscaleFilter: 'smooth' | 'crisp'; gap: number }`; `const fitModule: ReaderModule<FitPrefs>`; `function pageStyle(prefs: FitPrefs): CSSProperties`; `function seamStyle(prefs: FitPrefs, cell: 'first' | 'second'): CSSProperties`.

**Read first:** `web/src/modules/zoom.ts` (the closest model — a module with a boolean, numbers and a `select`), `web/src/modules/registry.ts`'s doc comment, and `web/src/PageView.tsx`'s `fitWhole` block, whose comment names `upscaleToFit` as unbuilt and is the thing this task starts making false.

- [ ] **Step 1: Write the failing tests**

Create `web/src/modules/fit.test.ts`:

```ts
import { describe, expect, it } from 'vitest'
import { fitModule, pageStyle, seamStyle } from './fit'
import { registry } from './registry'

describe('fitModule', () => {
  it('is registered', () => {
    expect(registry.ids).toContain('fit')
  })

  it('claims no keys — it is tuned in settings, not pressed mid-read', () => {
    expect(fitModule.keys ?? []).toEqual([])
  })

  it('upscales by default, which reverses the M5 design deliberately', () => {
    expect(fitModule.defaults.upscaleToFit).toBe(true)
  })

  it('offers the filter as a choice, because manga is line art', () => {
    expect(fitModule.options?.upscaleFilter).toEqual(['smooth', 'crisp'])
    expect(fitModule.defaults.upscaleFilter).toBe('smooth')
  })

  it('closes the seam by default', () => {
    expect(fitModule.defaults.gap).toBe(0)
  })
})

describe('pageStyle', () => {
  const prefs = { ...fitModule.defaults }

  it('lets a small page grow when upscaling, by sizing to the box', () => {
    const s = pageStyle({ ...prefs, upscaleToFit: true })
    expect(s.width).toBe('100%')
    expect(s.height).toBe('100%')
    expect(s.objectFit).toBe('contain')
  })

  // The exact declarations today's reader draws. Off must be byte-for-byte the
  // old behaviour, not merely similar, or turning the setting off is a new
  // third rendering nobody asked for.
  it('is the old fit-whole exactly when upscaling is off', () => {
    const s = pageStyle({ ...prefs, upscaleToFit: false })
    expect(s.width).toBe('auto')
    expect(s.height).toBe('auto')
    expect(s.maxWidth).toBe('100%')
    expect(s.maxHeight).toBe('100%')
    expect(s.objectFit).toBe('contain')
  })

  it('keeps lines crisp on request and leaves the browser alone otherwise', () => {
    expect(pageStyle({ ...prefs, upscaleFilter: 'crisp' }).imageRendering).toBe('pixelated')
    expect(pageStyle({ ...prefs, upscaleFilter: 'smooth' }).imageRendering).toBeUndefined()
  })

  // A filter is only meaningful while something is being enlarged.
  it('does not ask for pixelation while upscaling is off', () => {
    expect(
      pageStyle({ ...prefs, upscaleToFit: false, upscaleFilter: 'crisp' }).imageRendering,
    ).toBeUndefined()
  })
})

describe('seamStyle', () => {
  const prefs = { ...fitModule.defaults }

  // Screen position, not reading direction: `cellsFor` has already applied
  // direction by the time these cells are drawn.
  it('pushes the first cell right and the second left, toward the seam', () => {
    expect(seamStyle(prefs, 'first').justifyContent).toBe('flex-end')
    expect(seamStyle(prefs, 'second').justifyContent).toBe('flex-start')
  })

  it('splits the gap between the two cells so the pair stays centred', () => {
    const g = { ...prefs, gap: 24 }
    expect(seamStyle(g, 'first').paddingRight).toBe('12px')
    expect(seamStyle(g, 'second').paddingLeft).toBe('12px')
  })

  it('adds no padding at all when the gap is zero', () => {
    expect(seamStyle(prefs, 'first').paddingRight).toBe('0px')
    expect(seamStyle(prefs, 'second').paddingLeft).toBe('0px')
  })

  it('refuses a negative gap rather than overlapping the pages', () => {
    expect(seamStyle({ ...prefs, gap: -10 }, 'first').paddingRight).toBe('0px')
  })
})
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd /home/davey/projects/taiyaku/web && npx vitest run src/modules/fit.test.ts`
Expected: FAIL — `Failed to resolve import "./fit"`.

- [ ] **Step 3: Write the module**

Create `web/src/modules/fit.ts`:

```ts
import type { CSSProperties } from 'react'
import type { ReaderModule } from '../module'

/**
 * Page geometry: how large a page is drawn in its own box, and how far apart
 * the two pages of a spread sit.
 *
 * `docs/superpowers/specs/2026-08-02-reader-m5-design.md` specified this module
 * two milestones ago with four settings and built none of them. Two ship here
 * — the two a reader asked for — and `trim` and `fillWidth`/`fillHeight` stay
 * unbuilt with their reasoning intact in that document. `gap` is new and joins
 * them because a seam is geometry, which is the line that design draws
 * between `fit` and `filters`.
 *
 * **`upscaleToFit` defaults on, and that reverses the M5 design.** That
 * document chose off and argued it well: a low-resolution scan on a large
 * monitor is "a small page marooned in a large letterbox", so the enlargement
 * should be asked for. What it could not know is how the default would read.
 * Measured on this corpus: `gotoubun-v01`'s `raw-mono` is 685x1024 and its
 * `en-mono` 2147x3056, so in a 1400x1263 page area the English fills the
 * height and the Japanese sits at 1:1 with 239px unused. The first reader to
 * meet that reported a bug in the images, not a setting they had not found.
 *
 * `upscaleFilter` ships with it rather than after it. Enlarging line art with
 * a smoothing filter reads as mush; nearest-neighbour keeps the line crisp and
 * pays in visible stair-stepping. Which is better is a per-scan judgement, so
 * turning the default on without exposing the filter would trade one
 * complaint for another.
 */
export interface FitPrefs {
  upscaleToFit: boolean
  upscaleFilter: 'smooth' | 'crisp'
  /** Pixels between the two pages in `two` mode. */
  gap: number
}

export const fitModule: ReaderModule<FitPrefs> = {
  id: 'fit',
  label: 'Fit',
  defaults: { upscaleToFit: true, upscaleFilter: 'smooth', gap: 0 },
  options: { upscaleFilter: ['smooth', 'crisp'] },
}

/**
 * How one page sits in its own box.
 *
 * The whole of the upscale switch is `width`/`height`: `auto` on a replaced
 * element means "your intrinsic size, capped by `max-*`", which downscales a
 * large scan and leaves a small one alone; `100%` means "your box", which is
 * what lets a small one grow. `objectFit: contain` is what keeps the aspect
 * either way — with `auto` it has nothing to reconcile, since box and content
 * coincide, and with `100%` it is doing the work.
 *
 * `imageRendering` is omitted rather than set to a default keyword when it is
 * not wanted, so the browser's own choice stands and this never has an opinion
 * about downscaling — a filter is only meaningful while something is being
 * enlarged, which is why `crisp` is ignored while `upscaleToFit` is off.
 */
export function pageStyle(prefs: FitPrefs): CSSProperties {
  const filling = prefs.upscaleToFit
  return {
    maxWidth: '100%',
    maxHeight: '100%',
    width: filling ? '100%' : 'auto',
    height: filling ? '100%' : 'auto',
    objectFit: 'contain',
    display: 'block',
    ...(filling && prefs.upscaleFilter === 'crisp'
      ? { imageRendering: 'pixelated' as const }
      : {}),
  }
}

/**
 * How one cell of a two-page group holds its page.
 *
 * The pages are pushed **toward each other** so the grey collects on the outer
 * edges instead of down the middle, and `gap` reopens a measured amount of it
 * deliberately. Half the gap goes on each cell so the pair stays centred in
 * the page area; putting the whole gap on one side would shift the spread.
 *
 * `cell` is screen position and not reading order. `cellsFor` has already
 * applied the work's direction by the time these cells exist, so the first is
 * always the one on the left; a rule written in terms of `rtl` here would
 * apply that correction a second time.
 *
 * A negative gap is treated as zero. Nothing in the settings panel produces
 * one — a `number` field is a `number` field — but overlapping the two pages
 * is not a thing this can be asked for by accident.
 */
export function seamStyle(
  prefs: FitPrefs,
  cell: 'first' | 'second',
): CSSProperties {
  const half = `${Math.max(prefs.gap, 0) / 2}px`
  return cell === 'first'
    ? { display: 'flex', justifyContent: 'flex-end', paddingRight: half }
    : { display: 'flex', justifyContent: 'flex-start', paddingLeft: half }
}
```

- [ ] **Step 4: Register it**

In `web/src/modules/registry.ts`, add the import beside the others and `fitModule` to the `modules` array. Extend that file's doc comment by a sentence or two in its own voice — it narrates each module's arrival and what registration did and did not wire up. `fitModule` claims no keys, which is the `prefetchModule`/`readoutsModule` shape, and unlike `creditsModule`'s arrival its behaviour lands in the same milestone.

`web/src/modules/registry.test.ts` hardcodes the module-id list and its count. Update both — a tenth module fails that test until you do, and the test exists so a registry mistake fails loudly.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cd /home/davey/projects/taiyaku/web && npx vitest run`
Expected: PASS. Report the real total — do not repeat a predicted one; two briefs on the previous milestone predicted counts and both were wrong.

- [ ] **Step 6: Lint and typecheck**

Run: `cd /home/davey/projects/taiyaku/web && npm run lint && npm run typecheck`
Expected: no diagnostics.

- [ ] **Step 7: Commit**

```bash
git add web/src/modules/fit.ts web/src/modules/fit.test.ts web/src/modules/registry.ts web/src/modules/registry.test.ts
git commit -m "fit: a page smaller than its box now has a setting that grows it

The M5 design specified this module two milestones ago, chose upscale off,
and argued it well. Measured on this corpus, gotoubun-v01's raw is 685x1024
against en-mono's 2147x3056, so in a 1400x1263 area the English fills the
height and the Japanese sits at 1:1 with 239px unused. The first reader to
meet that reported a bug in the images rather than a setting they had not
found, which is the argument for reversing the default and not for
reversing the reasoning.

upscaleFilter ships with it rather than after it: enlarging line art
smoothly reads as mush, and turning the default on without exposing the
choice would trade one complaint for another."
```

---

### Task 2: `PageView` draws through `pageStyle`

**Files:**
- Modify: `web/src/PageView.tsx`
- Modify: `web/src/App.tsx`

**Interfaces:**
- Consumes: `FitPrefs`, `pageStyle` from `./modules/fit`.
- Produces: `PageViewProps` gains `fit?: FitPrefs`.

- [ ] **Step 1: Replace the fixed style with the computed one**

In `web/src/PageView.tsx`, delete the `fitWhole` constant and take the style from `pageStyle(fit)` instead, with `fitModule.defaults` as the fallback so a caller that passes nothing draws what the module's defaults say. Add to `PageViewProps`:

```tsx
  /**
   * Page geometry. Defaults to `fitModule.defaults` so this component still
   * works standalone — the same reason `decodeQueue` defaults to an uncapped
   * queue — and so a caller that has not been threaded yet draws what the
   * settings panel says rather than a third behaviour of its own.
   */
  fit?: FitPrefs
```

**Rewrite the `fitWhole` doc comment rather than deleting it.** It is long, it is accurate about two mechanisms, and one of its claims is what this task falsifies: it says "Inside that box nothing below upscales: `max-width`/`max-height` at 100% is the ceiling and this milestone did not raise it. `upscaleToFit` is still an unbuilt `fit` module setting". Move what stays true onto `pageStyle` in `fit.ts` (it is the rule now), and leave `PageView` with the part that is about *this component* — that `Viewport` scales the group above it and this styles one page inside its cell. Date the correction, as that file's other corrections are.

- [ ] **Step 2: Thread the preference in `App.tsx`**

`App.tsx` builds every module's preferences into one map from `registry.all()`, so `fit`'s slice is already there — read it with `sliceFor(map, fitModule)` exactly as `zoomPrefs` is read, and pass `fit={fitPrefs}` to all three `PageView` call sites: single/two mode's `renderGroup` (two of them) and scroll mode's.

Do **not** add a ref for it. `zoomPrefs` needs one only because the keydown listener subscribes once and cannot see current state; `fit` is read during render.

- [ ] **Step 3: Verify nothing else moved**

Run: `cd /home/davey/projects/taiyaku/web && npm run lint && npm run typecheck && npx vitest run`
Expected: PASS, and the total unchanged from Task 1 — this task adds no test, because what it changes is a rendered pixel and `pageStyle` is already covered.

- [ ] **Step 4: [deferred: controller] The browser pass**

The controller confirms against `gotoubun-v01`: `raw-mono` (685×1024) fills the page area's height instead of sitting at 1:1; swapping to `en-mono` no longer changes how large the art is drawn; `upscaleFilter: crisp` visibly hardens the enlargement; and turning `upscaleToFit` off restores exactly today's behaviour.

- [ ] **Step 5: Commit**

```bash
git add web/src/PageView.tsx web/src/App.tsx
git commit -m "fit: PageView draws through the rule instead of a constant

fitWhole's comment claimed the 100% ceiling was unraised and upscaleToFit
unbuilt. Both stop being true here, so the comment moves rather than
going: the part about how one page sits in its cell belongs on pageStyle,
which is now the rule, and PageView keeps the part about Viewport scaling
the group above it."
```

---

### Task 3: The seam

**Files:**
- Modify: `web/src/App.tsx`

**Interfaces:**
- Consumes: `seamStyle`, `FitPrefs` from `./modules/fit`.
- Produces: nothing new.

- [ ] **Step 1: Find out what centres a page in its cell today**

Before changing any CSS, establish the current mechanism — this plan deliberately does not assert it, because the markup alone does not settle it. `renderGroup`'s two-mode branch renders a flex row whose cells are `{ flex: 1, minWidth: 0, height: '100%' }`, and `PageView` returns a bare `<img>`; a measurement recorded in the spec found the image inset 8px in a 700px cell, i.e. centred, so *something* centres it.

Read the page-area wrapper's style in `App.tsx` and `PageView`'s own, and write one sentence in your report naming the mechanism. If the cell is already a flex container, `seamStyle`'s `display: flex` is redundant and should be dropped rather than restated; if it is not, `seamStyle` is what makes it one and the `justifyContent` needs it.

- [ ] **Step 2: Apply the seam**

In `renderGroup`'s two-mode branch, spread `seamStyle(fitPrefs, 'first')` into the first cell's style and `seamStyle(fitPrefs, 'second')` into the second's, keeping `flex: 1`, `minWidth: 0` and `height: '100%'`.

Leave the `wide` branch's collapsed second cell exactly as it is. A joined spread or a solo slot draws one image and has no seam; that cell is `flex: '0 0 0'` with `overflow: hidden` and must stay so, for the decode-gate reason its own comment gives.

- [ ] **Step 3: Verify**

Run: `cd /home/davey/projects/taiyaku/web && npm run lint && npm run typecheck && npx vitest run`
Expected: PASS, total unchanged — `seamStyle` is tested in Task 1 and this is its wiring.

- [ ] **Step 4: [deferred: controller] The browser pass**

The controller confirms the two pages meet with `gap: 0`, that a non-zero gap opens symmetrically and leaves the pair centred, and that a joined spread still draws as one wide image with no seam.

- [ ] **Step 5: Commit**

```bash
git add web/src/App.tsx
git commit -m "fit: the two pages meet at the seam

Each cell centred its own page, so the letterbox collected down the middle
— measured at 15px with 700px cells and 685px images, and widening with
the window, since the cell grows and an un-upscaled image does not. The
pages are pushed toward each other now and gap reopens a measured amount
deliberately, half on each cell so the pair stays centred."
```

---

### Task 4: The tabs move to the outer margin

**Files:**
- Modify: `web/src/Annotations.tsx`
- Modify: `web/src/App.tsx`

**Interfaces:**
- Consumes: nothing new.
- Produces: `AnnotationsProps` and `PageAnnotations`'s props gain `side?: 'left' | 'right'`, defaulting `'right'`.

**Read first:** `Annotations.tsx`'s tab style block, whose comment quotes the design's "fit-whole guarantees the side margins the tabs live in on every portrait page" — the guarantee Task 3 just stopped relying on in `two` mode.

- [ ] **Step 1: Give the tab a side**

The tab is positioned `left: '100%'` with `marginLeft: 4`, which places it just past the page's right edge. Add the mirror — `right: '100%'` with `marginRight: 4` — selected by a `side` prop that defaults to `'right'`, so single mode, scroll mode and every existing test keep today's behaviour without changing a call site.

Rewrite the surrounding comment to say what is now true: the tab lives in the margin **away from the seam**, which is the right margin everywhere except the first cell of a two-page pair. Record what it cost — the design's guarantee was written about a centred page and Task 3's seam-aligned pages break it — and that the alternatives were holding a gap open whenever the left page carries a note (a gap that appears and disappears as pages turn, reading as a layout bug) and letting tabs overlap (a tab over the facing page's art asserts a note is on a page it is not on).

- [ ] **Step 2: Pass the side from `App.tsx`**

Single and scroll mode pass nothing and keep `'right'`. The two-mode overlay passes `'left'` for the first cell and `'right'` for the second — the same `'first'`/`'second'` distinction `seamStyle` uses, and screen position for the same reason.

- [ ] **Step 3: Verify**

Run: `cd /home/davey/projects/taiyaku/web && npm run lint && npm run typecheck && npx vitest run`
Expected: PASS, total unchanged.

- [ ] **Step 4: [deferred: controller] The browser pass**

The controller writes a note on each page of a two-page pair and confirms both tabs sit in the outer letterbox with neither over artwork, and that single mode is unchanged.

- [ ] **Step 5: Commit**

```bash
git add web/src/Annotations.tsx web/src/App.tsx
git commit -m "notes: the tab lives in the margin away from the seam

Closing the seam invalidated the guarantee this comment quoted — fit-whole
guarantees the side margins the tabs live in — which was true of a centred
page and is not of a seam-aligned one. In two mode the first cell's tabs
move to its left margin; everywhere else is unchanged, which the default
keeps free."
```

---

### Task 5: One ordered key table

**Files:**
- Modify: `web/src/keys.ts`
- Modify: `web/src/keys.test.ts`

**Interfaces:**
- Consumes: `Intent`, `ReaderModule`.
- Produces: `interface ReaderBinding { keys: string[]; label: string; resolve: (key: string, editions: string[]) => Intent | undefined }`; `const READER_KEYS: ReaderBinding[]`.

**This is a refactor, not a rewrite.** `intentOf` is the reader's only input path and `keys.test.ts` is the whole safety net. **Every existing test must pass untouched.** If one has to change, that is evidence the table changed behaviour and the change is wrong — stop and report it rather than editing the expectation.

- [ ] **Step 1: Write the failing tests for the table**

Append to `web/src/keys.test.ts`:

```ts
describe('READER_KEYS', () => {
  it('describes every built-in the reader answers before modules', () => {
    const claimed = READER_KEYS.flatMap((b) => b.keys)
    for (const key of [' ', 'ArrowRight', 'ArrowLeft', 'Tab', 'c'])
      expect(claimed).toContain(key)
  })

  it('covers the digits as one row rather than nine', () => {
    const digits = READER_KEYS.filter((b) => b.keys.includes('1'))
    expect(digits).toHaveLength(1)
    expect(digits[0].keys).toEqual(['1', '2', '3', '4', '5', '6', '7', '8', '9'])
  })

  // `0` staying unclaimed is what makes it safe for zoom to take — see
  // `modules/zoom.ts`, which says so in its own doc.
  it('leaves 0 unclaimed', () => {
    expect(READER_KEYS.flatMap((b) => b.keys)).not.toContain('0')
  })

  it('gives every row a label for the help to show', () => {
    for (const b of READER_KEYS) {
      expect(b.label.length).toBeGreaterThan(0)
      expect(b.keys.length).toBeGreaterThan(0)
    }
  })

  // The property the ordering carries: a module cannot take a reader key,
  // which `intentOf`'s own comment calls "not so much a priority choice as
  // those keys not being a module's to claim".
  it('answers a built-in even when a module claims the same key', () => {
    const greedy = mod('greedy', [' ', 'c', '1'])
    expect(intentOf(press(' '), editions, [greedy])).toEqual({ kind: 'toggle' })
    expect(intentOf(press('c'), editions, [greedy])).toEqual({ kind: 'correct' })
    expect(intentOf(press('1'), editions, [greedy])).toEqual({
      kind: 'view',
      edition: editions[0],
    })
  })
})
```

`mod` and `press` are the helpers already at the top of that file; `editions` is the constant already defined there. Add `READER_KEYS` to the existing `./keys` import rather than a second import statement.

- [ ] **Step 2: Run to verify they fail**

Run: `cd /home/davey/projects/taiyaku/web && npx vitest run src/keys.test.ts`
Expected: FAIL — `READER_KEYS` is not exported.

- [ ] **Step 3: Build the table and dispatch from it**

Add to `web/src/keys.ts`, above `intentOf`:

```ts
/**
 * One of the reader's own keys: what it is called, what it asks for, and how
 * to say so to a person.
 *
 * `label` exists so the help screen can be *generated* rather than written.
 * A second list of what the keys do is a copy of a rule, and this project has
 * repeatedly found copies of rules drifting from the rules — a comment true
 * when written and false four commits later is the same failure at a smaller
 * scale. One table, dispatched from and listed from.
 *
 * `keys` is every key the row answers, so the digits are one row rather than
 * nine: `1`..`9` is a single rule — select the nth edition — and nine help
 * lines saying the same thing would be worse than one saying it once.
 *
 * `resolve` takes the pressed key because a row can answer several, and takes
 * `editions` because the digit row is the only rule here that depends on
 * anything outside the keyboard.
 */
export interface ReaderBinding {
  keys: string[]
  label: string
  resolve: (key: string, editions: string[]) => Intent | undefined
}

/**
 * The reader's own keys, **in the order they are answered**.
 *
 * The order is the rule that was previously carried by the sequence of `if`
 * statements in `intentOf`: these are checked before any module, because a
 * module claiming one of them would never fire regardless. Keeping them first
 * is therefore not a priority choice so much as those keys not being a
 * module's to claim.
 */
export const READER_KEYS: ReaderBinding[] = [
  { keys: [' '], label: 'swap to the last edition and back', resolve: () => ({ kind: 'toggle' }) },
  { keys: ['ArrowRight'], label: 'next page', resolve: () => ({ kind: 'step', by: 1 }) },
  { keys: ['ArrowLeft'], label: 'previous page', resolve: () => ({ kind: 'step', by: -1 }) },
  { keys: ['Tab'], label: 'show or hide the chrome', resolve: () => ({ kind: 'chrome' }) },
  { keys: ['c'], label: 'correct this pairing', resolve: () => ({ kind: 'correct' }) },
  {
    keys: ['1', '2', '3', '4', '5', '6', '7', '8', '9'],
    label: 'jump to the nth edition',
    resolve: (key, editions) => {
      const edition = editions[Number(key) - 1]
      return edition === undefined ? undefined : { kind: 'view', edition }
    },
  },
]
```

Then replace `intentOf`'s six `if` statements with a walk of the table, leaving the modifier guard and the module loop exactly as they are:

```ts
  if (event.ctrlKey || event.metaKey || event.altKey) return undefined
  for (const binding of READER_KEYS) {
    if (binding.keys.includes(event.key)) return binding.resolve(event.key, editions)
  }
```

Note the digit behaviour this preserves: a digit with no edition behind it returns `undefined` from `resolve` and the walk **stops** — it does not fall through to the modules. That matches today, where the `if` returned `undefined` rather than continuing, and a test above pins it.

Update `intentOf`'s doc comment: the paragraph explaining that built-ins are checked first now describes the table's order rather than a sequence of statements.

- [ ] **Step 4: Run the whole file to verify nothing moved**

Run: `cd /home/davey/projects/taiyaku/web && npx vitest run src/keys.test.ts`
Expected: PASS, including every pre-existing test unedited.

- [ ] **Step 5: Full check**

Run: `cd /home/davey/projects/taiyaku/web && npm run lint && npm run typecheck && npx vitest run`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add web/src/keys.ts web/src/keys.test.ts
git commit -m "keys: one table, dispatched from and listed from

The built-ins were a sequence of if statements with nothing enumerable,
so a help screen listing them would have been a second copy of a rule.
This project has spent nine review rounds on copies of rules drifting
from the rules; a table that intentOf walks and the help reads cannot.

Ordering is preserved and is the point: a module claiming a reader key
would never fire, so the built-ins going first is not a priority choice
so much as those keys not being a module's to claim."
```

---

### Task 6: The help overlay

**Files:**
- Create: `web/src/modules/help.ts`
- Create: `web/src/modules/help.test.ts`
- Create: `web/src/Help.tsx`
- Modify: `web/src/modules/registry.ts`, `web/src/modules/registry.test.ts`, `web/src/App.tsx`

**Interfaces:**
- Consumes: `READER_KEYS` from `../keys`; `registry` from `./registry`.
- Produces: `interface HelpPrefs { enabled: boolean }`; `const helpModule: ReaderModule<HelpPrefs>`; `const TOGGLE_INTENT: string`; `function Help(props: { visible: boolean; modules: ReaderModule<unknown>[]; onClose: () => void }): JSX.Element | null`.

**Read first:** `web/src/CreditsPanel.tsx` — the panel precedent, including how it returns `null` when not visible and how `App.tsx` renders it.

- [ ] **Step 1: Write the failing tests**

Create `web/src/modules/help.test.ts`:

```ts
import { describe, expect, it } from 'vitest'
import { READER_KEYS } from '../keys'
import { helpModule } from './help'
import { registry } from './registry'

describe('helpModule', () => {
  it('is registered', () => {
    expect(registry.ids).toContain('help')
  })

  it('opens on both ? and h', () => {
    expect((helpModule.keys ?? []).map((k) => k.key).sort()).toEqual(['?', 'h'])
  })

  it('gives both keys the same intent, so either opens the one panel', () => {
    const intents = new Set((helpModule.keys ?? []).map((k) => k.intent))
    expect(intents.size).toBe(1)
  })

  it('claims nothing the reader answers first', () => {
    expect(READER_KEYS.flatMap((b) => b.keys)).not.toContain('h')
    expect(READER_KEYS.flatMap((b) => b.keys)).not.toContain('?')
  })

  it('claims nothing another module already has', () => {
    const mine = new Set((helpModule.keys ?? []).map((k) => k.key))
    const others = registry
      .all()
      .filter((m) => m.id !== 'help')
      .flatMap((m) => m.keys ?? [])
      .map((k) => k.key)
    for (const key of others) expect(mine.has(key)).toBe(false)
  })

  it('is on by default — a help nobody can find helps nobody', () => {
    expect(helpModule.defaults.enabled).toBe(true)
  })
})
```

- [ ] **Step 2: Run to verify it fails**

Run: `cd /home/davey/projects/taiyaku/web && npx vitest run src/modules/help.test.ts`
Expected: FAIL — `Failed to resolve import "./help"`.

- [ ] **Step 3: Write the module**

Create `web/src/modules/help.ts`:

```ts
import type { ReaderModule } from '../module'

export const TOGGLE_INTENT = 'help:toggle'

/**
 * The keybind list.
 *
 * Two keys, one intent: `?` is what a reader tries and `h` is what a reader
 * who has used a pager tries, and making them two intents would mean two
 * cases in `App.tsx` doing the same thing.
 *
 * On by default, unlike most modules' more cautious defaults, because a help
 * screen that has to be found in settings before it can be found by keypress
 * has the problem backwards.
 */
export interface HelpPrefs {
  enabled: boolean
}

export const helpModule: ReaderModule<HelpPrefs> = {
  id: 'help',
  label: 'Keyboard help',
  defaults: { enabled: true },
  keys: [
    { key: '?', intent: TOGGLE_INTENT },
    { key: 'h', intent: TOGGLE_INTENT },
  ],
}
```

- [ ] **Step 4: Write the overlay**

Create `web/src/Help.tsx`, following `CreditsPanel.tsx`'s shape — `visible` false returns `null`, a positioned panel, a close button, and no state of its own. It renders two groups:

- **Reading** — one row per `READER_KEYS` entry, its `keys` joined for display and its `label` as the description. The digit row shows as a range.
- **One group per module** that declares keys, taking the module's `label` as the heading and each binding's `intent` as the row description.

Nothing is hardcoded: both groups are derived from the same declarations the reader dispatches on. Say so in the component's doc comment, and say why — a written list would be a second copy of a rule.

Note that this makes the `intent` strings user-facing for the first time. Read the ones the registry holds (`zoom:in`, `annotations:new`, and so on) and, if a string reads as an internal identifier rather than a description, add a `label` alongside `intent` on `KeyBinding` in `module.ts` and prefer it. **Report which you did.** If you add the field, every existing binding keeps working without it — the row falls back to `intent`.

- [ ] **Step 5: Wire it into `App.tsx`**

Add a `case TOGGLE_INTENT` to the module-intent switch that flips a `helpVisible` state, and render `<Help …>` beside `<CreditsPanel …>`. Gate on the module's `enabled` preference through a render-assigned ref, the way `creditsPrefsRef` does, because the keydown listener subscribes once and cannot see current state.

Register `helpModule` in `web/src/modules/registry.ts` and update `registry.test.ts`'s hardcoded list and count.

- [ ] **Step 6: Verify**

Run: `cd /home/davey/projects/taiyaku/web && npm run lint && npm run typecheck && npx vitest run`
Expected: PASS. Report the real total.

- [ ] **Step 7: [deferred: controller] The browser pass**

The controller confirms `?` and `h` both open it, Escape and the close button both dismiss it, every key it lists does what it says, and turning the module off in settings makes both keys inert.

- [ ] **Step 8: Commit**

```bash
git add web/src/modules/help.ts web/src/modules/help.test.ts web/src/Help.tsx web/src/modules/registry.ts web/src/modules/registry.test.ts web/src/App.tsx
git commit -m "help: the keybind list is generated, not written

Both groups come from the declarations the reader already dispatches on —
READER_KEYS for the built-ins and the registry for module keys — so a
binding that changes behaviour cannot keep an old description. A written
list would have been the same class of defect as a stale comment, which
this project has now corrected nine times."
```

---

### Task 7: Documentation

**Files:**
- Modify: `docs/superpowers/specs/2026-08-02-reader-m5-design.md`, `docs/status.md`

- [ ] **Step 1: Correct the M5 design in place**

Its `fit` table gives `upscaleToFit` a default of `off`. Change it to `on` and record the reversal beneath the table in that file's own dated-correction style — the reasoning it gives is sound and stays; what changed is the evidence about how the default reads to someone who did not write it.

Also correct its module table row, which lists `fit` as *(later)* with four settings; two ship and two do not.

- [ ] **Step 2: Update `docs/status.md`**

The reader's capabilities, the module count (`fit` and `help` make eleven), and the test counts. **Count them, do not trust this plan:** run `cargo test --workspace` and `cd web && npx vitest run` and read the numbers off.

- [ ] **Step 3: Run the full gate**

Run: `scripts/check-all.sh`
Expected: all four jobs green, the corpus checks skipping with `TAIYAKU_CORPUS` unset.

- [ ] **Step 4: Commit**

```bash
git add docs/superpowers/specs/2026-08-02-reader-m5-design.md docs/status.md
git commit -m "docs: fit ships two of its four settings, and upscale defaults on

The M5 design's table said off. Corrected in place with the reason rather
than rewritten: its argument was sound and the thing that changed is
evidence about how the default reads to a reader who did not write it."
```

---

## Self-Review

**Spec coverage.** §1's `fit` module and its default reversal → Tasks 1, 2, 7. §1's "what is not built" → stated in Task 1's module doc. §2's seam → Tasks 1 (`seamStyle`), 3. §2's tab move → Task 4. §3's declarative table → Task 5. §3's overlay and module → Task 6. Testing section → the Vitest steps throughout plus five `[deferred: controller]` browser passes. Documentation-goes-stale list → Tasks 2 (`PageView`), 4 (`Annotations`), 7 (M5 design, `status.md`).

**Sequencing** matches the spec: fit, then seam, then help.

**Known soft spots, named rather than hidden.**

- **Task 3 Step 1 asks the implementer to determine something instead of telling them.** The markup does not settle what centres a page in its cell, and I would rather a step say "find out and report it" than assert CSS I have not verified. Every other step gives the code.
- **Task 6 Step 4 is described rather than given as code.** The overlay's markup depends on what the existing `intent` strings read like, which is a judgement about text I have not made for them; the *structure* (two groups, both derived, nothing hardcoded) is fixed and is the part that must not drift.
- **Task 6 may add a `label` to `KeyBinding`.** That widens the module contract, which `module.ts`'s doc says should be a signal to revisit rather than done quietly — hence the instruction to report it.

## Execution Handoff

Two execution options:

**1. Subagent-Driven (recommended)** — a fresh subagent per task, review between tasks.

**2. Inline Execution** — tasks executed in this session using `superpowers:executing-plans`, with checkpoints.
