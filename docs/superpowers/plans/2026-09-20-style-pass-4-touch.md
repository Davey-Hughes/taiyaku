# Style pass ④ — touch: Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A tap on the left or right third of the reader's page turns it, a tap
in the middle shows or hides the chrome, a horizontal swipe turns — taking no
gesture from region drawing, the wheel, pinch zoom or panning — and the note
editor gains an exit a finger can reach (O137).

**Architecture:** One new pure module, `web/src/gestures.ts`, answers what a
finished press means, in the shape `notes.ts`'s `gestureFor` established: the
rule lives where it can be tested, and `App.tsx` reads the DOM for it and
carries out the answer. `App.tsx` records each press on `pointerdown` and
classifies it on `pointerup`; nothing about a press while it is still down
changes. The turn is handed to the existing `readerIntent`, which already puts
every `step` through `modules/direction.ts`'s `stepFor`, so right-to-left
mirroring is inherited rather than rewritten.

**Tech Stack:** TypeScript, React 19.2, Vitest 4.1 (three projects: `node`,
`jsdom`, `browser` with the Playwright provider), Biome 2.5.7, pnpm.

## Global Constraints

- **Branch:** `style-pass-4-touch`, off `main` at `333b3ab`. The spec section
  is already committed there as `54d9674`.
- **The spec is `docs/superpowers/specs/2026-09-17-style-pass-design.md` §9.**
  Read §9 before Task 1 and re-read the subsection a task names before starting
  that task. Where this plan and §9 disagree, stop and ask — do not pick one.
- **No attribution lines anywhere** — not in commit messages, not in code
  comments, not in documentation.
- **Every command runs from `/home/davey/projects/taiyaku`** unless a step says
  otherwise. Web commands are `pnpm --dir web …`, never a `cd` (this shell's
  `cd` is zoxide and has landed in another repository).
- **Constants:** `TAP_SLOP = 10` CSS px, `TAP_HOLD = 500` ms, `SWIPE_MIN = 40`
  CSS px, `SWIPE_RATIO = 2`. These are chosen, not measured (§9.2); Task 5's
  device pass is what confirms or changes them, and if it changes one, both the
  module and §9.2 are updated in the same commit.
- **`--reporter=default`** on every vitest run whose console output matters.
  Under Claude Code vitest otherwise picks its `agent` reporter, which hides
  every passing test's console output even with `--silent=false`.
- **Every new test is watched failing** with its defect put back by hand, and
  the failure's text is recorded in the commit or in `docs/notes.md`. A test
  that passes against its own defect is this repository's most-recorded
  failure. Putting the defect back means editing the production code, running
  the one test, reading the failure, and restoring the code — not reasoning
  about what would happen.
- **Nothing in ④ changes what a screen does** beyond the gestures and O137's
  button (spec §6). A defect found on the way is filed in `docs/status.md`, not
  fixed, unless it blocks ④'s own browser pass.

---

## Decided while planning

Probes and readings taken from `main` at `333b3ab` before the tasks were
drawn. Each one is a fact a task depends on.

1. **`setChromeVisible` has exactly two call sites** — `App.tsx:799`
   (`keyPlan`'s `toggle-chrome`) and `App.tsx:849` (the correction path). A
   finger can reach neither, which is why ③'s device pass never saw the bars.
2. **`readerIntent` is declared inside the keydown effect** (`App.tsx`
   761–1054), so a pointer handler cannot call it directly. The file already
   has the idiom for crossing that boundary: `closeEditorRef` (declared at 663,
   assigned at 1680, called at 795). Task 2 adds `readerIntentRef` the same
   way.
3. **`view === WHOLE` is not a usable test for fit-whole.** `clampRect`
   (`viewport.ts:116`) allocates a fresh rect on every call and `readerIntent`
   runs `setView(clampRect)` on every `step`, so identity with the frozen
   `WHOLE` is lost after the first page turn. A swipe gated on identity would
   work once and never again. Task 1 adds `isWhole`, which compares by value.
4. **The browser project can emulate touch and ④ does not ask it to.**
   `@vitest/browser-playwright`'s `dist/index.d.ts:28` exposes
   `contextOptions?: Omit<BrowserContextOptions, …>`, and Playwright's
   `hasTouch` lives there. But `vite.config.ts` gives the project a single
   instance, so enabling it would put all fifty-eight existing browser tests in
   a touch context. Because §9.1 chose "every pointer", a mouse press exercises
   every rule, so Task 4 drives a mouse and leaves the config alone. The double
   tap and the pinch are Task 5's device pass.
5. **`App.test.tsx` already has the harness** Task 2 needs: `pointer(el, type,
   {x, y}, shiftKey?)` at 1986, `measure(el, {w, h})` at 467, `pageArea(host)`,
   `press(key)` at 363, `click(el)` at 380, and a `describe('the pointer on the
   page')` block at 1985 where the new cases belong.
6. **A slot is readable in jsdom** from `.reader-progress-fill`'s inline
   `width`, which is `(slot + 1) / 6` for the six-page fixture: slot 1 is
   `33.333…%`, slot 2 `50%`, slot 3 `66.666…%`. The element exists only while
   the chrome is shown, so a test that reads it presses `Tab` first.
7. **The note editor has no Close button on purpose.** `NotePanel.tsx`
   216–228 carries a comment arguing against one, and its status line reads
   `saved · esc or click away when done`. Both name exits a finger does not
   have, which is O137. §9.3 records the reversal; Task 3 rewrites the comment
   rather than leaving it contradicting the code.
8. **`.reader-button` already takes 44 px** under `@media (pointer: coarse)`
   (`styles/reader.css` 390–394), so O137's button is 44 px by wearing the
   class the review panel's Close already wears.
9. **The web coverage gate** (`scripts/coverage-floor.mjs:221`) holds
   `minFiles: 108` and a per-file floor on four metrics. A new file raises the
   count, which is safe; the per-file floor applies to `gestures.ts` like
   everything else, and a pure module with a dedicated test clears it easily.
10. **`docs/status.md:120–121`** states the web test counts in prose —
    "2,266 in `web/`'s TypeScript — 1,226 in the `node` project and 1,040 in
    the `jsdom` one — and fifty-eight more in a **browser** project". Nothing
    derives them (only the Rust count is machine-checked, by
    `scripts/status-test-count.mjs`), so Task 5 updates them by hand from a
    real run.
11. **Corrected during Task 2, 2026-09-20 — this plan's test arithmetic was
    wrong twice.** `App.test.tsx`'s fixture sets `work: { reading: 'rtl' }`
    (line 156), not `ltr`, so every direction this plan's Task 2 test code
    asserts is mirrored from what it says; and the progress line's width is
    the raw float string — `'66.66666666666666%'`, not the rounded
    `'66.6667%'` written below. The committed tests carry the measured values.
    Task 2's step 3 note — read the string the app produces rather than
    trusting this plan — is what caught both.
12. **React's `onPointerLeave` is derived from native `pointerout`/
    `pointerover`**, found while watching Task 2's defects fail: a raw
    `pointerleave` dispatch never reaches the handler, so a test written that
    way passes whichever function is bound and proves nothing.


---

### Task 1: The rule — `gestures.ts`, and a fit-whole test that survives a turn

**Files:**
- Create: `web/src/gestures.ts`
- Create: `web/src/gestures.test.ts`
- Modify: `web/src/viewport.ts` (add `isWhole` beside `WHOLE`, near line 80)
- Modify: `web/src/viewport.test.ts` (add the `isWhole` cases)

**Interfaces:**
- Consumes: `Rect` from `./viewport`.
- Produces, for Task 2: `gestureOfPress(press: PressReading): PageGesture`;
  `interface Press { x: number; y: number; t: number }`;
  `interface PressReading { down: Press; up: Press; width: number; onControl:
  boolean; consumed: boolean; editing: boolean; panelOpen: boolean; scrolling:
  boolean; atWhole: boolean }`;
  `type PageGesture = { kind: 'none' } | { kind: 'turn'; by: 1 | -1 } | { kind:
  'toggle-chrome' } | { kind: 'close-panels' }`; the four constants
  `TAP_SLOP`, `TAP_HOLD`, `SWIPE_MIN`, `SWIPE_RATIO`; and
  `isWhole(rect: Rect): boolean` from `./viewport`.

- [ ] **Step 1: Write the failing tests**

Create `web/src/gestures.test.ts`:

```ts
import { describe, expect, it } from 'vitest'
import {
  SWIPE_MIN,
  TAP_HOLD,
  TAP_SLOP,
  gestureOfPress,
  type PressReading,
} from './gestures'

/**
 * A press that is a tap in the middle of a 900 px page area, with nothing
 * open and nothing in the way. Each test overrides only what it is about,
 * so a case reads as its own difference from "an ordinary tap".
 */
function reading(over: Partial<PressReading> = {}): PressReading {
  return {
    down: { x: 450, y: 400, t: 1_000 },
    up: { x: 450, y: 400, t: 1_100 },
    width: 900,
    onControl: false,
    consumed: false,
    editing: false,
    panelOpen: false,
    scrolling: false,
    atWhole: true,
    ...over,
  }
}

/** The same press, moved to one x and given a matching release. */
function tapAt(x: number, over: Partial<PressReading> = {}): PressReading {
  return reading({
    down: { x, y: 400, t: 1_000 },
    up: { x, y: 400, t: 1_100 },
    ...over,
  })
}

/** A drag from the middle by `dx`, `dy`, over `ms`. */
function drag(
  dx: number,
  dy: number,
  ms: number,
  over: Partial<PressReading> = {},
): PressReading {
  return reading({
    down: { x: 450, y: 400, t: 1_000 },
    up: { x: 450 + dx, y: 400 + dy, t: 1_000 + ms },
    ...over,
  })
}

describe('a tap', () => {
  it('turns back on the left third and forward on the right', () => {
    // Physical, before `stepFor`: the left third is `-1` whatever the work
    // reads like, and `App.tsx`'s `readerIntent` is what mirrors it.
    expect(gestureOfPress(tapAt(10))).toEqual({ kind: 'turn', by: -1 })
    expect(gestureOfPress(tapAt(890))).toEqual({ kind: 'turn', by: 1 })
  })

  it('toggles the chrome in the middle third', () => {
    expect(gestureOfPress(tapAt(450))).toEqual({ kind: 'toggle-chrome' })
  })

  it('puts each boundary on the side away from the middle', () => {
    // 900/3 = 300. Just inside 300 turns; 300 itself is the middle, because
    // a reader aiming at the centre band should never turn by a pixel.
    expect(gestureOfPress(tapAt(299.9))).toEqual({ kind: 'turn', by: -1 })
    expect(gestureOfPress(tapAt(300))).toEqual({ kind: 'toggle-chrome' })
    expect(gestureOfPress(tapAt(599.9))).toEqual({ kind: 'toggle-chrome' })
    expect(gestureOfPress(tapAt(600))).toEqual({ kind: 'turn', by: 1 })
  })

  it('survives travel up to the slop and no further', () => {
    expect(gestureOfPress(drag(TAP_SLOP, 0, 100))).toEqual({
      kind: 'toggle-chrome',
    })
    expect(gestureOfPress(drag(0, TAP_SLOP, 100))).toEqual({
      kind: 'toggle-chrome',
    })
    // One pixel past it is a drag, and a drag of 11 px is not a swipe either.
    expect(gestureOfPress(drag(TAP_SLOP + 1, 0, 100))).toEqual({ kind: 'none' })
  })

  it('survives a hold up to the limit and no further', () => {
    expect(gestureOfPress(drag(0, 0, TAP_HOLD))).toEqual({
      kind: 'toggle-chrome',
    })
    // A long press is the selection callout's, not a turn's.
    expect(gestureOfPress(drag(0, 0, TAP_HOLD + 1))).toEqual({ kind: 'none' })
  })

  it('is nothing when it began on a control, or was already spent', () => {
    expect(gestureOfPress(tapAt(10, { onControl: true }))).toEqual({
      kind: 'none',
    })
    // The press that closed the note editor on `pointerdown`: its release
    // must not also turn a page.
    expect(gestureOfPress(tapAt(10, { consumed: true }))).toEqual({
      kind: 'none',
    })
  })

  it('leaves the press to region drawing while the editor is open', () => {
    expect(gestureOfPress(tapAt(10, { editing: true }))).toEqual({
      kind: 'none',
    })
    expect(gestureOfPress(tapAt(450, { editing: true }))).toEqual({
      kind: 'none',
    })
  })

  it('closes every open panel instead of turning', () => {
    expect(gestureOfPress(tapAt(10, { panelOpen: true }))).toEqual({
      kind: 'close-panels',
    })
    // It beats the middle third and scroll mode too: one tap returns to the
    // page whatever is over it, and nothing else happens.
    expect(gestureOfPress(tapAt(450, { panelOpen: true }))).toEqual({
      kind: 'close-panels',
    })
    expect(
      gestureOfPress(tapAt(890, { panelOpen: true, scrolling: true })),
    ).toEqual({ kind: 'close-panels' })
  })

  it('only toggles the chrome in scroll mode, wherever it lands', () => {
    expect(gestureOfPress(tapAt(10, { scrolling: true }))).toEqual({
      kind: 'toggle-chrome',
    })
    expect(gestureOfPress(tapAt(890, { scrolling: true }))).toEqual({
      kind: 'toggle-chrome',
    })
  })

  it('does nothing against a page area with no width', () => {
    // The pan's own zero-box guard, for the same reason: a box measured
    // mid-teardown divides the thirds into nothing.
    expect(gestureOfPress(tapAt(10, { width: 0 }))).toEqual({ kind: 'none' })
    expect(gestureOfPress(tapAt(10, { width: Number.NaN }))).toEqual({
      kind: 'none',
    })
  })
})

describe('a swipe', () => {
  it('turns forward when it travels left, and back when it travels right', () => {
    // Dragging the page leftwards brings in what is on the right, which is
    // the physical `→`.
    expect(gestureOfPress(drag(-SWIPE_MIN, 0, 200))).toEqual({
      kind: 'turn',
      by: 1,
    })
    expect(gestureOfPress(drag(SWIPE_MIN, 0, 200))).toEqual({
      kind: 'turn',
      by: -1,
    })
  })

  it('needs the distance', () => {
    expect(gestureOfPress(drag(-(SWIPE_MIN - 1), 0, 200))).toEqual({
      kind: 'none',
    })
  })

  it('needs to be more across than down', () => {
    // 60 across, 40 down: 60 < 2 × 40, so it is a diagonal and turns nothing.
    expect(gestureOfPress(drag(-60, 40, 200))).toEqual({ kind: 'none' })
    expect(gestureOfPress(drag(-100, 40, 200))).toEqual({ kind: 'turn', by: 1 })
  })

  it('has no time limit of its own', () => {
    // A slow drag at fit-whole moved nothing, so reading it as a turn takes
    // nothing from the pan.
    expect(gestureOfPress(drag(-120, 0, 4_000))).toEqual({
      kind: 'turn',
      by: 1,
    })
  })

  it('is nothing once the reader has zoomed in', () => {
    expect(gestureOfPress(drag(-120, 0, 200, { atWhole: false }))).toEqual({
      kind: 'none',
    })
  })

  it('is nothing in scroll mode, while editing, or under a panel', () => {
    expect(gestureOfPress(drag(-120, 0, 200, { scrolling: true }))).toEqual({
      kind: 'none',
    })
    expect(gestureOfPress(drag(-120, 0, 200, { editing: true }))).toEqual({
      kind: 'none',
    })
    expect(gestureOfPress(drag(-120, 0, 200, { panelOpen: true }))).toEqual({
      kind: 'none',
    })
  })

  it('is nothing when it began on a control, or was already spent', () => {
    expect(gestureOfPress(drag(-120, 0, 200, { onControl: true }))).toEqual({
      kind: 'none',
    })
    expect(gestureOfPress(drag(-120, 0, 200, { consumed: true }))).toEqual({
      kind: 'none',
    })
  })
})
```

Add to `web/src/viewport.test.ts` (import `isWhole` alongside the existing
imports from `./viewport`):

```ts
describe('isWhole', () => {
  it('is true of WHOLE and of an equal rect that is not it', () => {
    expect(isWhole(WHOLE)).toBe(true)
    // The case the identity test would fail: `clampRect` allocates, and
    // `readerIntent` runs it on every turn.
    expect(isWhole(clampRect(WHOLE))).toBe(true)
    expect(clampRect(WHOLE)).not.toBe(WHOLE)
  })

  it('is false of anything zoomed or panned', () => {
    expect(isWhole({ x: 0, y: 0, w: 0.5, h: 0.5 })).toBe(false)
    expect(isWhole({ x: 0.1, y: 0, w: 1, h: 1 })).toBe(false)
  })
})
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `pnpm --dir web exec vitest run --project node gestures viewport --reporter=default`

Expected: FAIL — `Failed to resolve import "./gestures"` for the first file,
and `isWhole is not a function` (or an import error) for the second.

- [ ] **Step 3: Write `isWhole`**

In `web/src/viewport.ts`, immediately after the `WHOLE` declaration (line 80):

```ts
/**
 * Whether a rect is the whole group — **by value, not by identity**.
 *
 * `WHOLE` is frozen and `zoomStep('whole')` hands the object itself back, so
 * `rect === WHOLE` looks like it would do. It does not survive a page turn:
 * `App.tsx`'s `step` intent runs `setView(clampRect)`, and `clampRect`
 * allocates a fresh rect every time — equal to `WHOLE`, and not it. The swipe
 * (`gestures.ts`) is gated on being at fit-whole, so on identity it would have
 * worked until the reader's first turn and never again.
 */
export function isWhole(rect: Rect): boolean {
  return rect.x === 0 && rect.y === 0 && rect.w === 1 && rect.h === 1
}
```

- [ ] **Step 4: Write `gestures.ts`**

Create `web/src/gestures.ts`:

```ts
/**
 * What a *finished* press on the reader's page area means.
 *
 * The rule lives here rather than in `App.tsx` for the reason `notes.ts`'s
 * `gestureFor` gives for the same split: this is where it can be tested.
 * `App.tsx` reads the DOM for it — the page area's box, where the press went
 * down and came up, which panels are open — and carries out the answer.
 *
 * Named for gestures rather than for touch because a mouse answers these too
 * (spec 2026-09-17 §9.1): a click is a press that moved nowhere, and the
 * browser project drives every rule below without emulating a finger.
 *
 * **What this does not decide** is anything a press does while it is still
 * down. Panning is `pressTakesPan`'s; drawing a region, moving one and
 * finishing a note are `gestureFor`'s; the wheel and the trackpad pinch it
 * carries are `App.tsx`'s own listener's. None of them changes, and a press
 * one of them has already spent arrives here marked `consumed`.
 */
import type { Rect } from './viewport'

/** Where and when a press was, in the page area's own pixels and in milliseconds. */
export interface Press {
  /** Pixels from the page area's left edge, not the viewport's. */
  x: number
  /** Pixels from the page area's top edge. */
  y: number
  /** `event.timeStamp`, which jsdom and a real browser both fill. */
  t: number
}

/** Everything the rule reads. `App.tsx` supplies all of it from the DOM and from state. */
export interface PressReading {
  down: Press
  up: Press
  /** The page area's own width. Thirds are of this, not of a page cell. */
  width: number
  /** The press began on a control — `keys.ts`'s ancestor-aware `isInteractivePress`. */
  onControl: boolean
  /** Something already acted on this press while it was down. */
  consumed: boolean
  /** The note editor is open: `panelMode === 'edit'`. */
  editing: boolean
  /** Help, Settings, credits or the note review panel is open. */
  panelOpen: boolean
  /** Scroll mode, where scrolling is the turn. */
  scrolling: boolean
  /** The view is the whole group — `viewport.ts`'s `isWhole`, by value. */
  atWhole: boolean
}

/** What the press turned out to mean. `by` is *physical*, before `stepFor`. */
export type PageGesture =
  | { kind: 'none' }
  | { kind: 'turn'; by: 1 | -1 }
  | { kind: 'toggle-chrome' }
  | { kind: 'close-panels' }

/**
 * How far a press may travel and still be a tap, in CSS pixels.
 *
 * These four numbers are **chosen, not measured** (spec §9.2). A thumb on a
 * phone is what says whether 10 px is too tight to hold still for and whether
 * 40 px is far enough to be deliberate; ④'s device pass is where that is
 * asked, and a figure it changes is changed here and in §9.2 together.
 */
export const TAP_SLOP = 10

/** How long a press may last and still be a tap, in milliseconds. */
export const TAP_HOLD = 500

/** How far a drag must travel across the page to turn it, in CSS pixels. */
export const SWIPE_MIN = 40

/** How much more across than down a swipe must be. */
export const SWIPE_RATIO = 2

const NONE: PageGesture = { kind: 'none' }

export function gestureOfPress(press: PressReading): PageGesture {
  // A press that began on a control was never this rule's, and one already
  // spent must not act twice: `gestureFor`'s `close` fires on `pointerdown`,
  // so without this the release of the tap that shut the note editor would
  // fall through below and turn a page as well.
  if (press.onControl || press.consumed) return NONE

  const dx = press.up.x - press.down.x
  const dy = press.up.y - press.down.y
  const tapped =
    Math.abs(dx) <= TAP_SLOP &&
    Math.abs(dy) <= TAP_SLOP &&
    press.up.t - press.down.t <= TAP_HOLD
  if (!tapped) return swipe(press, dx, dy)

  // Region drawing's gesture, which §5 ④ forbids taking. The editor's own
  // exits are `Escape`, its Close button, and `gestureFor`'s letterbox rule.
  if (press.editing) return NONE
  // One tap returns to the page whatever is over it, and turns nothing — so
  // there is no precedence between panels to invent.
  if (press.panelOpen) return { kind: 'close-panels' }
  // Scrolling is already the turn in scroll mode, so the whole page area is
  // the middle third there.
  if (press.scrolling) return { kind: 'toggle-chrome' }
  // The pan's zero-box guard, for the same reason: a box measured mid-teardown
  // divides the thirds into nothing. `NaN` fails this comparison too.
  if (!(press.width > 0)) return NONE

  const third = press.width / 3
  // Where the press went *down* decides, not where it came up: a tap has
  // barely moved, and the finger's landing is what the reader aimed.
  if (press.down.x < third) return { kind: 'turn', by: -1 }
  if (press.down.x >= press.width - third) return { kind: 'turn', by: 1 }
  return { kind: 'toggle-chrome' }
}

/**
 * A drag that was not a tap.
 *
 * Only at fit-whole, where `panStep` hands the same rect back and `panBy`'s
 * own doc calls the drag "a no-op instead of a jitter" — so the swipe is
 * carved out of a press the pan already discards rather than taken from it.
 * Zoomed in, every drag stays the pan's.
 *
 * No time limit: a slow drag at fit-whole moved nothing either, and reading it
 * as a turn costs nothing. The axis test is what keeps scroll mode's vertical
 * drag and a diagonal out.
 */
function swipe(press: PressReading, dx: number, dy: number): PageGesture {
  if (press.editing || press.panelOpen || press.scrolling || !press.atWhole)
    return NONE
  if (Math.abs(dx) < SWIPE_MIN) return NONE
  if (Math.abs(dx) < SWIPE_RATIO * Math.abs(dy)) return NONE
  // Dragging leftwards brings in what is on the right: the physical `→`.
  return { kind: 'turn', by: dx < 0 ? 1 : -1 }
}
```

`Rect` is imported for the doc's sake only if TypeScript complains about an
unused import — if it does, drop the import line; `gestures.ts` takes
`atWhole` as a boolean and never touches a `Rect`.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `pnpm --dir web exec vitest run --project node gestures viewport --reporter=default`

Expected: PASS, with the new `gestures.test.ts` cases and the two `isWhole`
cases green.

- [ ] **Step 6: Watch the new tests fail with their defects put back**

Do each of these by hand, one at a time: edit the file, run the single test,
read the failure, restore the file. Record each failure line in the commit
message.

1. In `gestures.ts`, change `if (press.down.x >= press.width - third)` to
   `if (press.down.x > press.width)`. Expected: the left/right test fails on
   the right third, reporting `toggle-chrome` where `{ kind: 'turn', by: 1 }`
   was expected.
2. Change `dx < 0 ? 1 : -1` to `dx < 0 ? -1 : 1`. Expected: the swipe direction
   test fails both ways.
3. Change `if (press.onControl || press.consumed)` to `if (press.onControl)`.
   Expected: both `consumed` cases fail.
4. Change `Math.abs(dx) < SWIPE_RATIO * Math.abs(dy)` to `Math.abs(dx) <
   Math.abs(dy)`. Expected: the diagonal case fails, reporting a turn.
5. In `viewport.ts`, change `isWhole` to `return rect === WHOLE`. Expected: the
   `clampRect(WHOLE)` case fails — which is the defect this function exists to
   prevent, so watch this one especially.

- [ ] **Step 7: Typecheck, lint, commit**

```bash
pnpm --dir web exec tsc --noEmit -p tsconfig.app.json
pnpm --dir web exec biome check --write src/gestures.ts src/gestures.test.ts src/viewport.ts src/viewport.test.ts
pnpm --dir web exec vitest run --project node gestures viewport --reporter=default
git add web/src/gestures.ts web/src/gestures.test.ts web/src/viewport.ts web/src/viewport.test.ts
git commit
```

Commit message: what the rule is, that `by` is physical and `stepFor` is
applied by the caller, and the five defects watched failing with their
messages.

---

### Task 2: The wiring — `App.tsx` classifies a press on release

**Files:**
- Modify: `web/src/App.tsx` — the press record and its ref (near 663 and 2113),
  `beginRegion` (2113), `endRegion` (2205), the handler props (2436–2446), the
  page area's style (2400), and `readerIntentRef` (near 663, 984 and 1680)
- Modify: `web/src/App.test.tsx` — the `describe('the pointer on the page')`
  block at 1985

**Interfaces:**
- Consumes: `gestureOfPress`, `PressReading` from `./gestures`; `isWhole` from
  `./viewport`; the existing `readerIntent`, `setChromeVisible`,
  `setHelpVisible`, `setSettingsVisible`, `setCreditsVisible`, `setEditing`,
  `toggleReview`, `isInteractivePress`, `panelMode`, `mode`, `view`.
- Produces: nothing new for later tasks. Task 3 and Task 4 read this task's
  behaviour, not its names.

- [ ] **Step 1: Write the failing tests**

Add to `web/src/App.test.tsx`, inside `describe('the pointer on the page')`
(after the existing `a shift-press pans…` case). The `pointer`, `measure`,
`pageArea`, `press`, `click`, `dialog` and `button` helpers are already in the
file.

```ts
  /** The slot the progress line is reporting, as its percentage string. */
  function progress(host: HTMLElement): string {
    return (host.querySelector('.reader-progress-fill') as HTMLElement).style
      .width
  }

  /** A press that goes down and comes up in the same place: a tap. */
  function tap(el: Element, at: { x: number; y: number }) {
    pointer(el, 'pointerdown', at)
    pointer(el, 'pointerup', at)
  }

  it('a tap on the right third turns forward and on the left third back', async () => {
    const host = await open()
    const area = pageArea(host)
    measure(area, { w: 900, h: 600 })
    press('Tab')
    // Stored progress is `raw` slot 2 of six: (2 + 1) / 6.
    expect(progress(host)).toBe('50%')
    tap(area, { x: 880, y: 300 })
    // Slot 3: (3 + 1) / 6.
    expect(progress(host)).toBe('66.6667%')
    tap(area, { x: 20, y: 300 })
    expect(progress(host)).toBe('50%')
  })

  it('mirrors the thirds for a right-to-left work', async () => {
    const host = await open()
    const area = pageArea(host)
    measure(area, { w: 900, h: 600 })
    press('Tab')
    expect(progress(host)).toBe('50%')
    // `d` flips the viewed edition's direction. The tap is physical either
    // way — `readerIntent` is what puts it through `stepFor`, so the same
    // right third now goes back rather than forward.
    press('d')
    tap(area, { x: 880, y: 300 })
    expect(progress(host)).toBe('33.3333%')
  })

  it('a tap in the middle third shows the chrome and hides it again', async () => {
    const host = await open()
    const area = pageArea(host)
    measure(area, { w: 900, h: 600 })
    expect(host.querySelector('.reader-progress-fill')).toBeNull()
    tap(area, { x: 450, y: 300 })
    expect(host.querySelector('.reader-progress-fill')).not.toBeNull()
    tap(area, { x: 450, y: 300 })
    expect(host.querySelector('.reader-progress-fill')).toBeNull()
  })

  it('a tap closes an open panel and turns nothing', async () => {
    const host = await open()
    const area = pageArea(host)
    measure(area, { w: 900, h: 600 })
    press('Tab')
    expect(progress(host)).toBe('50%')
    press('s')
    expect(dialog(host, 'Settings')).not.toBeNull()
    // The right third, which would otherwise turn: the tap is spent on the
    // panel instead, which is also the pointer exit Settings had none of.
    tap(area, { x: 880, y: 300 })
    expect(dialog(host, 'Settings')).toBeNull()
    expect(progress(host)).toBe('50%')
  })

  it('the tap that closes the note editor does not also turn the page', async () => {
    const host = await open()
    const area = pageArea(host)
    measure(area, { w: 900, h: 600 })
    press('Tab')
    press('n')
    expect(dialog(host, 'Note')).not.toBeNull()
    // The right third, where `gestureFor` answers `close` because the press
    // is not on the draft's page. The close happens on `pointerdown`; the
    // `pointerup` must find the press already spent.
    tap(area, { x: 880, y: 300 })
    expect(dialog(host, 'Note')).toBeNull()
    expect(progress(host)).toBe('50%')
  })

  it('a swipe turns at fit-whole, and pans once zoomed in', async () => {
    const host = await open()
    const area = pageArea(host)
    measure(area, { w: 900, h: 600 })
    press('Tab')
    expect(progress(host)).toBe('50%')
    pointer(area, 'pointerdown', { x: 600, y: 300 })
    pointer(area, 'pointermove', { x: 400, y: 300 })
    pointer(area, 'pointerup', { x: 400, y: 300 })
    // Dragged leftwards: the physical `→`.
    expect(progress(host)).toBe('66.6667%')
    // Zoomed in, the same drag is the pan and turns nothing.
    press('=')
    pointer(area, 'pointerdown', { x: 600, y: 300 })
    pointer(area, 'pointermove', { x: 400, y: 300 })
    pointer(area, 'pointerup', { x: 400, y: 300 })
    expect(progress(host)).toBe('66.6667%')
    await settle()
  })

  it('a press that began on a control turns nothing', async () => {
    const host = await open()
    const area = pageArea(host)
    measure(area, { w: 900, h: 600 })
    press('Tab')
    expect(progress(host)).toBe('50%')
    // The note list draws inside the page area and takes pointer events
    // deliberately; so does anything else added there later. A press on one
    // is excluded by ancestor, not by tag.
    const control = host.querySelector('button') as HTMLElement
    pointer(control, 'pointerdown', { x: 880, y: 300 })
    pointer(control, 'pointerup', { x: 880, y: 300 })
    expect(progress(host)).toBe('50%')
  })

  it('in scroll mode a tap anywhere only toggles the chrome', async () => {
    const host = await open()
    press('Tab')
    click(button(host, 'scroll'))
    const area = pageArea(host)
    measure(area, { w: 900, h: 600 })
    expect(progress(host)).toBe('50%')
    // The right third, which turns in paged mode and must not here.
    tap(area, { x: 880, y: 300 })
    expect(host.querySelector('.reader-progress-fill')).toBeNull()
    tap(area, { x: 880, y: 300 })
    expect(progress(host)).toBe('50%')
  })

  it('leaves the chrome up when a tap turns the page', async () => {
    const host = await open()
    const area = pageArea(host)
    measure(area, { w: 900, h: 600 })
    press('Tab')
    tap(area, { x: 880, y: 300 })
    // Nothing in ④ is on a timer: a turn made with the bars up leaves them
    // up, exactly as a keyed turn does.
    expect(progress(host)).toBe('66.6667%')
  })

  it('gives the page area touch-action manipulation', async () => {
    const host = await open()
    // Without it a double tap on a phone turns two pages *and* zooms the
    // document. With it, two taps turn two pages.
    expect(pageArea(host).style.touchAction).toBe('manipulation')
  })
```

**One more case, and it goes in a different block.** `describe`'s gap section
at `App.test.tsx:1439` has a `beforeEach` that turns the borrow module off and
a `swapOntoUnpaired()` helper that is the only way to reach `Gap` at all
(`corpus()`'s one missing pair, explained at 164). §9.2 claims the thirds do
not require a drawn page, unlike the pan — so the case belongs there, not in
the pointer block:

```ts
  it('leaves a gap on a tap in the outer third', async () => {
    const host = await swapOntoUnpaired()
    expect(pages(host)).toEqual([])
    const area = pageArea(host)
    measure(area, { w: 900, h: 600 })
    // Nothing is drawn over a gap and `zoomable` is false, so the pan is
    // inert here — but a reader who wants away from one taps to leave, and
    // the thirds do not ask whether anything is drawn.
    pointer(area, 'pointerdown', { x: 20, y: 300 })
    pointer(area, 'pointerup', { x: 20, y: 300 })
    expect(pages(host)).not.toEqual([])
  })
```

`pointer`, `measure` and `pageArea` are file-level helpers, so they are in
scope there; `tap` is defined inside the pointer block, so either lift it to
file level or write the two calls out as above.

**What is deliberately not unit-tested here.** §9.2 says the page area's tap is
unchanged while a correction is pending. `gestureOfPress` has no term for a
correction, and a test that a rule ignores an input it never reads asserts
nothing about the rule — it only pins that nobody has added the term. Correcting
on a screen is Task 5's device pass (§9.4), which is where it is seen at all.

**A note for the implementer on two of these.** The `scroll` button's exact
accessible name and the way the layout is chosen after style pass ③ must be
checked in `TopBar.tsx` before writing the scroll-mode case — if `click(button(host,
'scroll'))` does not exist, open Settings and use its Layout section, which
`App.test.tsx` already does elsewhere. And `progress(host)`'s percentages come
from React's own formatting of `(slot + 1) / 6`; run the first test and read
the actual string before pinning `'66.6667%'`, rather than trusting this plan's
arithmetic.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `pnpm --dir web exec vitest run --project jsdom App --reporter=default`

Expected: FAIL. The turn, chrome, panel, swipe and scroll-mode cases fail with
the progress string unchanged or the element still absent; the `touch-action`
case fails with `''`.

- [ ] **Step 3: Reach `readerIntent` from a pointer handler**

`readerIntent` is declared inside the keydown effect, so a pointer handler
cannot call it. Add a ref beside `closeEditorRef` (`App.tsx` near 663):

```ts
  /**
   * The reader's own intents — a turn, a swap — reachable from a pointer
   * handler. `readerIntent` is declared inside the keydown effect, where its
   * closure over `resolver` and the grouped slots is built; the same problem
   * `closeEditorRef` above solves, solved the same way rather than by lifting
   * the function out of the effect and giving it a dependency list of its own.
   */
  const readerIntentRef = useRef<(intent: ReadingIntent) => void>(() => undefined)
```

Inside the keydown effect, immediately after `readerIntent`'s declaration ends
(after its closing brace, around line 1018) and before the effect's other
statements that use it:

```ts
    readerIntentRef.current = readerIntent
```

- [ ] **Step 4: Record the press on the way down**

At the top of `beginRegion` (`App.tsx:2113`), before the `pressTakesPan` call:

```ts
    // The press, for `gestures.ts` to read when it comes up. Recorded here
    // rather than measured again on release: where a press went *down* is
    // what a reader aimed, and the page area's box can change under a
    // gesture (the chrome does not resize it, but an orientation change
    // does).
    const box = event.currentTarget.getBoundingClientRect()
    pressRef.current = {
      x: event.clientX - box.left,
      y: event.clientY - box.top,
      t: event.timeStamp,
      // Read from the *down* target: a press that began on a control was
      // never a page gesture, whatever it is over when it ends.
      onControl: isInteractivePress(event.target),
      consumed: false,
    }
```

Declare the ref **beside `dragRef` and `panRef` (`App.tsx` 669–675)**, not
next to `beginRegion`. Everything below `if (!session) return null`
(`App.tsx:1397`) is past an early return, and a `useRef` there would be a hook
that does not run on every render:

```ts
  /**
   * The press in progress, as `gestures.ts` reads it on release. Beside
   * `dragRef` and `panRef` because it is the third thing one press can be
   * carrying, and above `if (!session) return null` because it is a hook.
   */
  const pressRef = useRef<
    (Press & { onControl: boolean; consumed: boolean }) | null
  >(null)
```

In `beginRegion`'s `gesture.kind === 'close'` branch (around 2147), mark it:

```ts
    if (gesture.kind === 'close') {
      closeEditor()
      // Spent: this press has done its work, and its release must not also
      // turn a page (`gestures.ts`'s `consumed`).
      if (pressRef.current) pressRef.current.consumed = true
      return
    }
```

- [ ] **Step 5: Classify the press on the way up**

`endRegion` is bound to both `onPointerUp` and `onPointerLeave`, and a pointer
that *left* the area finished no gesture. So the classification goes in a new
handler that wraps it, and `onPointerLeave` keeps calling `endRegion` alone.

After `endRegion` (`App.tsx`, after line 2230):

```ts
  /**
   * A press released on the page area: finish whatever it was doing, then ask
   * `gestures.ts` what it meant.
   *
   * Bound to `onPointerUp` only. `onPointerLeave` keeps calling `endRegion`
   * directly, because a pointer that left the area finished no gesture — and
   * `onPointerCancel` clears the record for the same reason.
   */
  function endPress(event: React.PointerEvent) {
    endRegion(event)
    const started = pressRef.current
    pressRef.current = null
    if (!started) return
    const box = event.currentTarget.getBoundingClientRect()
    const gesture = gestureOfPress({
      down: started,
      up: {
        x: event.clientX - box.left,
        y: event.clientY - box.top,
        t: event.timeStamp,
      },
      width: box.width,
      onControl: started.onControl,
      consumed: started.consumed,
      editing: panelMode === 'edit',
      panelOpen:
        helpVisible ||
        settingsVisible ||
        creditsVisible ||
        panelMode === 'review',
      scrolling: mode === 'scroll',
      // By value: `clampRect` allocates on every turn, so identity with
      // `WHOLE` would be lost after the first one (`viewport.ts`'s `isWhole`).
      atWhole: isWhole(view),
    })
    switch (gesture.kind) {
      case 'none':
        return
      case 'toggle-chrome':
        return setChromeVisible((visible) => !visible)
      case 'close-panels':
        // Every one that is open, so no precedence between them has to be
        // invented. `toggleReview` only has a `'review'` arm to close here,
        // because `'edit'` was answered by `gestures.ts` before this.
        setHelpVisible(false)
        setSettingsVisible(false)
        setCreditsVisible(false)
        if (panelMode === 'review') setEditing(toggleReview)
        return
      case 'turn':
        // Physical, as the keys are. `readerIntent` is the one caller that
        // puts a step through `stepFor`, so the mirroring is inherited.
        return readerIntentRef.current({ kind: 'step', by: gesture.by })
    }
  }
```

Change the page area's props (2436–2446):

```tsx
        onPointerDown={beginRegion}
        onPointerMove={trackRegion}
        onPointerUp={endPress}
```

leaving `onPointerLeave={endRegion}` as it is, and add the record's clearing to
the existing `onPointerCancel`:

```tsx
        onPointerCancel={() => {
          panRef.current = null
          dragRef.current = null
          pressRef.current = null
          setDragging(undefined)
        }}
```

- [ ] **Step 6: Give the page area `touch-action: manipulation`**

In the page area's inline style object (`App.tsx:2400`), beside `overflow`:

```ts
          // The browser's double-tap zoom, and nothing else. Without this a
          // double tap on a phone turns two pages *and* zooms the document;
          // with it, two taps turn two pages, which is what they say. Pinch
          // and scrolling are untouched — `touch-action: none` would take
          // both, and §9.2 keeps the pinch the browser's.
          touchAction: 'manipulation',
```

Add the imports at the top of `App.tsx`: `gestureOfPress` and `type Press` from
`./gestures`, and `isWhole` from `./viewport` (beside the existing `WHOLE`,
`clampRect`, `mapView`, `type Rect` import at line 193).

- [ ] **Step 7: Run the tests to verify they pass**

Run: `pnpm --dir web exec vitest run --project jsdom App --reporter=default`

Expected: PASS, including every pre-existing case in the file — especially
`a click on the page closes the editor`, `a press on a note grabs it`, `a drag
on the draft page anchors a region` and `a shift-press pans`, none of which may
change.

- [ ] **Step 8: Watch the new tests fail with their defects put back**

One at a time, by hand, reading each failure:

1. Bind `onPointerUp={endRegion}` again. Expected: every new case fails.
2. Bind `onPointerLeave={endPress}`. Expected: nothing fails — which is the
   point. Then add a case that a `pointerleave` after a press on the right
   third turns nothing, watch *it* fail with this defect in, and restore.
3. In `endPress`, drop `started.consumed` (pass `consumed: false`). Expected:
   the note-editor case fails, reporting `'66.6667%'`.
4. In `endPress`, change `atWhole: isWhole(view)` to `atWhole: view === WHOLE`.
   Expected: the swipe case fails on its *first* swipe only if a turn preceded
   it — so extend that test with a tap-turn before the swipe if it does not
   already fail, because this is the defect `isWhole` exists for.
5. In `endPress`, swap `panelOpen`'s `settingsVisible` for `false`. Expected:
   the panel case fails, reporting the Settings dialog still present.
6. Remove `touchAction`. Expected: the `touch-action` case fails.

- [ ] **Step 9: Typecheck, lint, commit**

```bash
pnpm --dir web exec tsc --noEmit -p tsconfig.app.json
pnpm --dir web exec biome check --write src/App.tsx src/App.test.tsx
pnpm --dir web exec vitest run --project jsdom --reporter=default
git add web/src/App.tsx web/src/App.test.tsx
git commit
```

---

### Task 3: O137 — an exit the finger can reach

**Files:**
- Modify: `web/src/NotePanel.tsx` — the edit-mode footer (210–243) and the
  comment above it
- Modify: `web/src/NotePanel.test.tsx` — the edit-mode cases
- Modify: `web/src/App.test.tsx` — one case that the button reaches the same
  close the key does

**Interfaces:**
- Consumes: `NotePanelProps.onClose`, which already exists and is already what
  the review panel's Close calls.
- Produces: nothing new. The button calls the existing `onClose`.

- [ ] **Step 1: Write the failing tests**

In `web/src/NotePanel.test.tsx`, in the edit-mode describe block:

```ts
  it('offers a Close button in the editor, which is O137s exit for a finger', () => {
    const onClose = vi.fn()
    const host = mount(<NotePanel {...editProps()} onClose={onClose} />)
    click(button(host, 'Close'))
    expect(onClose).toHaveBeenCalledTimes(1)
  })

  it('disables Close while a write is on the wire, like every other exit', () => {
    // The same rule Delete follows: every exit is disabled while `busy`,
    // because a close racing a write is the ordering that loses a note.
    const host = mount(<NotePanel {...editProps()} busy={true} />)
    expect((button(host, 'Close') as HTMLButtonElement).disabled).toBe(true)
  })

  it('no longer tells a reader to press a key they may not have', () => {
    const host = mount(<NotePanel {...editProps()} />)
    // O137: `esc` and `click away` are both absent on a phone, and the
    // status line named them as the way out.
    expect(host.textContent).not.toContain('esc')
    expect(host.textContent).toContain('saved')
  })
```

`editProps()` is whatever the file's existing edit-mode cases build their props
with — reuse it rather than writing a second builder; if the file inlines the
props instead, follow that shape.

In `web/src/App.test.tsx`, in `describe('the note editor')`:

```ts
  it('the editors Close button finishes the note, like Escape does', async () => {
    const host = await open()
    press('n')
    expect(dialog(host, 'Note')).not.toBeNull()
    const panel = dialog(host, 'Note') as HTMLElement
    click(button(panel, 'Close'))
    expect(dialog(host, 'Note')).toBeNull()
  })
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `pnpm --dir web exec vitest run --project jsdom NotePanel App --reporter=default`

Expected: FAIL — `Close` not found in the editor, and the `esc` assertion
failing on the current status line.

- [ ] **Step 3: Replace the comment and add the button**

In `web/src/NotePanel.tsx`, replace the comment at 216–228 (the block beginning
"No Save, and no Done either") with:

```tsx
          {/*
            No Save and no Done: changes reach the server on their own, and
            being finished is not an act a reader performs.

            **There is a Close, since O137.** This comment used to argue
            against one — "`Escape`, or a click on the background around the
            page, is what closes this. Buttons for both would be asking the
            reader to tell the reader something it can see." That holds for a
            keyboard and a mouse and fails on a phone, where both of the exits
            it named are missing: a tap on the page while the editor is open
            is region drawing's gesture, so the user's iPhone found Delete as
            the only way out of a note (spec 2026-09-17 §9.3). The two old
            exits still work and are unchanged; this is the third, and the
            only one a finger can always reach.

            Every exit is disabled while the write is on the wire, the same
            rule the correction banner follows — Delete especially, since it
            is the control that most looks like it undoes the save it is
            racing, and now Close, since a close racing a write is the same
            ordering.
          */}
          <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
            <button
              type="button"
              className="reader-button"
              disabled={busy}
              onClick={() => onDelete(draft.id)}
            >
              Delete
            </button>
            <span className="reader-meta" style={{ marginLeft: 'auto' }}>
              {busy ? 'saving…' : 'saved'}
            </span>
            <button
              type="button"
              className="reader-button"
              disabled={busy}
              onClick={onClose}
            >
              Close
            </button>
          </div>
```

`.reader-button` is what carries the 44 px minimum under `@media (pointer:
coarse)` (`styles/reader.css` 390–394), so the button is a finger's size by
wearing the same class the review panel's Close wears. Check that `onClose` is
already destructured in this component's parameter list — it is, for the
review arm — and do not add a second prop for it.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `pnpm --dir web exec vitest run --project jsdom NotePanel App --reporter=default`

Expected: PASS.

- [ ] **Step 5: Watch the new tests fail with their defects put back**

1. Remove `disabled={busy}` from Close. Expected: the busy case fails.
2. Change the status line back to `'saved · esc or click away when done'`.
   Expected: the `esc` case fails.
3. Change Close's `onClick` to `() => undefined`. Expected: both the NotePanel
   case and the `App.test.tsx` case fail.

- [ ] **Step 6: Typecheck, lint, commit**

```bash
pnpm --dir web exec tsc --noEmit -p tsconfig.app.json
pnpm --dir web exec biome check --write src/NotePanel.tsx src/NotePanel.test.tsx src/App.test.tsx
pnpm --dir web exec vitest run --project jsdom --reporter=default
git add web/src/NotePanel.tsx web/src/NotePanel.test.tsx web/src/App.test.tsx
git commit
```

---

### Task 4: The gestures in a real browser

**Files:**
- Modify: `web/src/App.browser.test.tsx`

**Interfaces:**
- Consumes: Task 2's wiring. Nothing is imported from `gestures.ts` here — the
  browser project asserts what a reader sees, not what the rule returns.

- [ ] **Step 1: Read the file's existing shape**

Open `web/src/App.browser.test.tsx` and find how it mounts the reader, how it
drives a pointer (the project has real layout and a real Chromium, so
`getBoundingClientRect` answers honestly and `measure` is neither needed nor
wanted), and how it asserts geometry. Follow that shape exactly; do not import
jsdom helpers into it.

- [ ] **Step 2: Write the failing tests**

Add to `web/src/App.browser.test.tsx`, after the existing describe blocks. The
file already has `reader`, `readerWithChrome`, `press`, `until`, `button`,
`box` and `SIZES`; reuse them and add only the two helpers below.

```tsx
// ---- the gestures, laid out -------------------------------------------------

/**
 * What jsdom cannot answer about ④'s gestures: the thirds are thirds of the
 * page area's *laid-out* width, and a swipe is measured against a real box.
 * `App.test.tsx` pins the routing against a stubbed rect; this asks Chromium
 * where the boundaries actually fall, and whether a bar is sitting on one.
 *
 * **Driven with a mouse, deliberately.** Spec 2026-09-17 §9.1 put every
 * pointer in scope, so a mouse press exercises every rule here.
 * `@vitest/browser-playwright` would allow a touch context through
 * `contextOptions.hasTouch`, but `vite.config.ts` gives this project a single
 * instance — enabling it would put all fifty-eight existing cases into a touch
 * context to reach the two things only a finger has, the double tap and the
 * pinch. Both are ④'s device pass's.
 */
function pointerAt(type: string, at: { x: number; y: number }) {
  // Dispatched on whatever is actually under the point, so a bar drawn over
  // the page area intercepts the press here exactly as it would for a reader.
  const target = document.elementFromPoint(at.x, at.y) ?? document.body
  act(() => {
    target.dispatchEvent(
      new PointerEvent(type, {
        bubbles: true,
        cancelable: true,
        clientX: at.x,
        clientY: at.y,
      }),
    )
  })
}

const tapAt = (at: { x: number; y: number }) => {
  pointerAt('pointerdown', at)
  pointerAt('pointerup', at)
}

/** Where the reader stands, as the progress line draws it. */
const fill = () =>
  (document.querySelector('.reader-progress-fill') as HTMLElement | null)?.style
    .width ?? null

describe('the gestures on the page, laid out', () => {
  it.each(SIZES)(
    'at %i×%i, the outer thirds of the real page area turn and the middle toggles',
    async (width, height) => {
      await readerWithChrome(width, height)
      const area = box('.page-area')
      const y = area.top + area.height / 2
      const third = area.width / 3
      const start = fill()
      // Just inside the right third, against the measured boundary rather
      // than an assumed one.
      tapAt({ x: area.right - third + 1, y })
      await until(() => fill() !== start)
      const forward = fill()
      tapAt({ x: area.left + third - 1, y })
      await until(() => fill() === start)
      expect(forward).not.toBe(start)
      // The middle third takes the bars down, so the fill leaves the DOM.
      tapAt({ x: area.left + area.width / 2, y })
      await until(() => fill() === null)
    },
  )

  it('reads a drag across the real page area as a swipe', async () => {
    await readerWithChrome(1440, 900)
    const area = box('.page-area')
    const y = area.top + area.height / 2
    const from = area.left + area.width / 2
    const start = fill()
    pointerAt('pointerdown', { x: from, y })
    pointerAt('pointermove', { x: from - 120, y })
    pointerAt('pointerup', { x: from - 120, y })
    // Dragged leftwards: the physical `→`.
    await until(() => fill() !== start)
  })

  it('does not turn on a drag too short to be a swipe', async () => {
    await readerWithChrome(1440, 900)
    const area = box('.page-area')
    const y = area.top + area.height / 2
    const from = area.left + area.width / 2
    const start = fill()
    // 30 px: past the tap's slop, short of the swipe's minimum. It is
    // neither, and it is the pan it always was.
    pointerAt('pointerdown', { x: from, y })
    pointerAt('pointermove', { x: from - 30, y })
    pointerAt('pointerup', { x: from - 30, y })
    expect(fill()).toBe(start)
  })

  it('does not turn once the reader has zoomed in', async () => {
    await readerWithChrome(1440, 900)
    const area = box('.page-area')
    const y = area.top + area.height / 2
    const from = area.left + area.width / 2
    press('=')
    const start = fill()
    pointerAt('pointerdown', { x: from, y })
    pointerAt('pointermove', { x: from - 120, y })
    pointerAt('pointerup', { x: from - 120, y })
    // Above fit-whole every drag is the pan's, and `isWhole` is what says so
    // — by value, because `clampRect` has already replaced `WHOLE` by now.
    expect(fill()).toBe(start)
  })
})
```

**Two things to check against the running file rather than against this plan.**
`press('=')` must actually reach the zoom — the file's `press` dispatches on
`document.body`, and `keyplan.ts` stands the handler down on an interactive
target, so if the last click left focus on a button the key does nothing.
And `until(() => fill() !== start)` throws `condition never held` after 2 s,
which is the failure to expect in Step 3 rather than an assertion message.

- [ ] **Step 3: Run the tests to verify they fail**

Run: `pnpm --dir web exec vitest run --project browser --reporter=default`

Expected: FAIL on the new cases only. If Chromium is not downloaded, run
`pnpm --dir web exec playwright install chromium` first.

- [ ] **Step 4: Make them pass**

No production change should be needed — Task 2 did the work. If one is needed,
that is a defect Task 2's tests did not catch: fix it, and add the jsdom case
that would have caught it.

- [ ] **Step 5: Watch them fail with their defects put back**

1. In `gestures.ts`, change `const third = press.width / 3` to `const third =
   press.width / 2`. Expected: the thirds case fails in the browser.
2. In `App.tsx`'s `endPress`, change `width: box.width` to `width: 900`.
   Expected: the thirds case fails at a viewport that is not 900 wide — run it
   at whatever size the file's other cases use, and if that size *is* 900,
   change the hard-coded number to something else so the defect is visible.

- [ ] **Step 6: Lint and commit**

```bash
pnpm --dir web exec biome check --write src/App.browser.test.tsx
pnpm --dir web exec vitest run --project browser --reporter=default
git add web/src/App.browser.test.tsx
git commit
```

---

### Task 5: Measure it, put it on a phone, record it, run the gate

**Files:**
- Modify: `docs/notes.md` — a new section, "Style pass ④: touch"
- Modify: `docs/status.md` — the test counts (120–121), O137's row, O134's and
  O138's rows, the open-items preamble's counts
- Modify: `docs/superpowers/specs/2026-09-17-style-pass-design.md` §9.2 — only
  if the device pass changes one of the four constants

- [ ] **Step 1: Serve a scratch copy of the real library**

Follow `docs/notes.md`'s "Style pass ③" section, which records the exact
commands: a scratch copy of the NAS library under `target/style-pass/`, `taiyaku
serve` against it, and vite in front. Use a new scratch directory,
`target/style-pass/four/`, for this pass's request log. **Never serve the real
library** — the copy is what every measurement is taken against.

- [ ] **Step 2: Hand-seed a work with two editions**

③ could not show correcting, a gap or a borrowed page on a screen, because
every work in the scratch library is single-edition and the screen that accepts
a proposal is ⑤'s (spec §9.4). Build one by hand in the scratch copy's
database: a second edition for one work, aligned to the first, with at least
one slot missing so a gap and a borrowed page can both be reached.

Work out the exact rows from `taiyaku-library`'s migrations and the sidecar
shape before writing any, and write them only to
`target/style-pass/four/…/library.db`. If it cannot be made to hold up, stop:
file a row in `docs/status.md` saying these three screens still have no device
pass and pass them to ⑤, and carry on with Step 3.

- [ ] **Step 3: Measure in Chromium at 1440×900 and 390×844, in Paper and in Ink**

Drive the running app with Playwright. Record, as figures read off the app:

1. The page area's width at both sizes, and the three thirds' boundaries in
   real pixels.
2. A tap in each third, with the slot before and after.
3. A tap in the middle with the chrome hidden, then shown.
4. A tap with Settings open: the panel gone, the slot unchanged.
5. A 40 px drag and a 39 px drag at fit-whole.
6. The same drag after `=`: the rect moved, the slot unchanged.
7. The note editor's Close, and the letterbox band's height at both sizes —
   the figure §9.3 says is computed and not measured.

- [ ] **Step 4: The iPhone, over the tailnet**

Serve over TLS on the tailnet (`docs/notes.md` records how). **Read the
user-agent out of `target/style-pass/four/requests.log` before believing any
result** — the log's byte count is always 0 for a download, and a result from
the wrong device is worse than no result.

Ask the user, and record their answers verbatim:

1. Each gesture in both orientations: the thirds, the middle, the swipe.
2. A pan that ends where it began — does it ever turn a page by accident?
3. Is 10 px too tight to hold still for? Is 40 px far enough to be deliberate?
4. Pinch: does it still zoom the document? Double tap: two pages and no zoom?
5. The bars with the chrome shown — their 44 px targets, and their safe areas
   against the notch and the home indicator, in both orientations.
6. Safari's resolution of the reader's `light-dark()` tokens inside `.reader`,
   in Paper and in Ink. ③ measured this in Chromium 151 and Firefox 153 only.
7. The note editor's Close button, and whether the letterbox tap is findable.
8. **O134**: does the strip's softness bother them on the phone?
9. **O138**: hide the rail's scrollbar, grow the strip, or leave it?
10. Correcting, a gap and a borrowed page on a screen — if Step 2 held up.

- [ ] **Step 5: Record it in `docs/notes.md`**

A new section, "Style pass ④: touch — 2026-09-20", in the shape ③'s has: what
was served, at which commit, driven how; then the figures, each one marked
measured or computed; then the iPhone paragraph with the user-agent quoted;
then what the device pass found that no test could.

- [ ] **Step 6: Update `docs/status.md`**

1. **Close O137** — the five-part edit the ritual requires (the row's checkbox,
   the row's text rewritten to say what closed it and what remains, the
   preamble's running count, the closed/open totals, and the "What is built"
   sentence if it names the item).
2. **O134 and O138**: record the user's answer in each row. If an answer closes
   one, close it properly; if it turns into work, say whose PR it is.
3. **O135**: amend the row to say ④ gave Settings a pointer exit, and that the
   keyboard half remains the accessibility pass's.
4. **The test counts at 120–121**, from a real run — Rust from the gate's own
   output, web from `vitest run --reporter=default` per project. The Rust count
   is machine-checked by `scripts/status-test-count.mjs`; the web counts are
   not, so they are the ones that go stale.

- [ ] **Step 7: Run the gate**

```bash
NEXTEST_TEST_THREADS=8 RUST_TEST_THREADS=8 scripts/check-all.sh
```

Do not pipe it through `tail` or append `; echo` — either masks the exit
status. Quote the closing `all green` line verbatim in the commit message, and
if it carries a `NOT MEASURED:` clause, quote that too.

- [ ] **Step 8: Commit**

```bash
git add docs/notes.md docs/status.md
git commit
```

---

---

### Task 6: Two exits the browser pass found unreachable

**Added 2026-09-20**, after Task 5's browser pass measured two of ④'s own
promises failing in a real browser. The full brief, with the measurements, the
code for both fixes, the tests and the four defects to watch fail, is
`.superpowers/sdd/task-6-brief.md`; the spec's §9.2 and §9.3 carry the
corrected design.

**Files:** `web/src/Settings.tsx`, `web/src/App.tsx`, `web/src/Settings.test.tsx`,
`web/src/App.test.tsx`, `web/src/App.browser.test.tsx`.

- **Fix A.** `.reader-dialog`'s scrim is the whole viewport, so no tap can
  reach the page area while Settings is open and rule 3b never fires for it.
  Settings dismisses on a press that goes down and comes up on its own scrim.
- **Fix B.** The page `<img>` fills the page area and letterboxes inside
  itself, so `onAnImage` is true of every press in the reader and
  `gestureFor`'s rule 1 could never fire. A press is on the page when it is on
  an image *and* inside that image's painted box — `pointInSlot` is unclamped
  for exactly this.

**Why this is a task and not a filed row:** both are ④'s own promises, and
§5's own list makes a defect that blocks this PR's browser pass one to fix
rather than file. Both were put to the user with the measurements on
2026-09-20 and both fixes were chosen.

**The lesson for the rest of this plan:** the two jsdom tests covering these
behaviours passed throughout, because each dispatches its event directly on an
element the real pointer can never reach. A jsdom case over a pointer rule
proves the routing and nothing about whether the press arrives.

## Before the PR is opened

The five checks §5 requires of every PR in this pass, in order. None is
optional and none is this plan's to skip:

1. `scripts/check-all.sh`, with its `all green` line quoted.
2. The browser pass of Task 5, at both sizes and in both themes.
3. Every new test watched failing with its defect put back by hand — Tasks 1,
   2, 3 and 4 each have that step, and the evidence is in their commits.
4. **A whole-branch review**, and a review of each fix round until one finds
   nothing. Per-task review passes on branches whose whole-branch review then
   finds Criticals; the fix rounds are where the later ones live.
5. **The pre-merge claim check** over the PR body, `docs/status.md` and
   `docs/notes.md` — every factual claim in each, checked against the thing it
   claims about. Run it twice, separately: on recent branches the second pass
   found findings inside the first pass's own fixes.
6. Anything found and not fixed is filed as its own open item.
