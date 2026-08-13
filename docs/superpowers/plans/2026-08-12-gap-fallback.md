# Gap Fallback Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** When the edition being read has no page for the reader's slot, draw
another edition's page in its place, visibly marked, instead of the `Gap` view.

**Architecture:** A pure rule (`lenderFor`/`withBorrow`) picks a lending
edition and maps `Display`'s `gap` arm to a new `borrowed` arm. A new
`Borrowed` component draws the lender's page with a mark that toggles to the
existing `<Gap>`. A `borrow` module supplies the on/off switch through the
generated settings panel. `display()` itself is not changed.

**Tech Stack:** TypeScript, React 19, Vitest (three projects: `node`, `jsdom`,
`browser`), Biome, pnpm.

**Spec:** `docs/superpowers/specs/2026-08-12-gap-fallback-design.md`. Read it
before starting. Where this plan and the spec disagree, the spec wins and the
disagreement is a plan bug worth reporting.

## Global Constraints

- **Package manager is pnpm, invoked as `pnpm --prefix web <script>`.** Never
  `npm`. See `docs/ci.md`.
- **No new runtime or dev dependencies.** This repository's rule is that a
  dependency needs a stated reason; nothing here needs one.
- **`display()` in `web/src/reading.ts` keeps its current signature and
  behaviour.** Only its return *type* gains an arm. `nearest.ts`'s `landingAt`,
  `session.ts`'s `applyIntent` and `resolveOffered` all call it and all test
  `kind !== 'at'`; none of them may change.
- **`Display.edition` means "the edition being viewed" in all three arms.**
  `App.tsx`'s `allSlots(resolver, shown.edition)` and `alignedBreaksCached(…,
  shown.edition, …)` depend on it.
- **Page numbers crossing into anything a person reads are 1-based; every
  internal page and slot value stays 0-based.** Convert at the prop boundary,
  the convention `altAt`, `Gap`'s `page` props and `toMove` already follow.
- **Comments explain *why*, not *what*.** Match the density of the file being
  edited — this codebase's comments carry rejected alternatives and
  measurements, and a bare restatement of the code is not in keeping.
- **No git attribution trailers** (`Co-Authored-By`, `Generated with`).
- **The gate is `scripts/check-all.sh`, run without `TAIYAKU_CORPUS` set.**
  Run it as `env -u TAIYAKU_CORPUS scripts/check-all.sh`. With that variable
  set it runs a 40+ minute debug-profile experiment.

## File Structure

| File | Responsibility |
| --- | --- |
| `web/src/modules/borrow.ts` | **New.** The `borrow` module declaration, `BorrowPrefs`, and the two pure functions `lenderFor` and `withBorrow`. Matches `modules/zoom.ts`, which holds both a module and its pure geometry. |
| `web/src/modules/borrow.test.ts` | **New.** Node-project tests for both functions. No DOM. |
| `web/src/reading.ts` | **Modify.** `Display` gains a `borrowed` arm. Nothing else. |
| `web/src/Borrowed.tsx` | **New.** Draws the lender's page with the mark; toggles to a `ReactNode` of offers it is handed. |
| `web/src/Borrowed.test.tsx` | **New.** jsdom-project tests for the component. |
| `web/src/modules/registry.ts` | **Modify.** One import, one array entry. |
| `web/src/App.tsx` | **Modify.** Three edits: read the prefs slice, wrap `display` at the `shown` binding, and render `Borrowed` in the page area. |
| `docs/status.md` | **Modify.** Close the "Later" entry; update the three web test counts. |

---

### Task 1: The rule — `lenderFor`, `withBorrow`, and the `borrow` module

**Files:**
- Create: `web/src/modules/borrow.ts`
- Create: `web/src/modules/borrow.test.ts`
- Modify: `web/src/reading.ts` (the `Display` type, currently at line 76)

**Interfaces:**
- Consumes: `ReadingState` and `Display` from `../reading`; `Resolver` from
  `../resolver`; `ReaderModule` from `../module`; `fakeResolver` and `whole`
  from `../fixtures` (tests only).
- Produces:
  - `export interface BorrowPrefs { enabled: boolean }`
  - `export const borrowModule: ReaderModule<BorrowPrefs>` with `id: 'borrow'`
  - `export function lenderFor(state: ReadingState, resolver: Resolver, order: string[]): { edition: string; slot: number } | undefined`
  - `export function withBorrow(shown: Display, state: ReadingState, resolver: Resolver, order: string[], prefs: BorrowPrefs): Display`
  - The `borrowed` arm of `Display` (see Step 3).

---

- [ ] **Step 1: Write the failing test**

Create `web/src/modules/borrow.test.ts`:

```ts
import { describe, expect, it } from 'vitest'
import { borrowModule, lenderFor, withBorrow } from './borrow'
import { fakeResolver, whole } from '../fixtures'
import { display, type ReadingState } from '../reading'

// Four editions of four pages each, `a` the hub and therefore first in the
// order `App.tsx` passes. Slot 2 is the interesting one: `b` has no
// counterpart for it, so a reader viewing `b` from authority `a` is in a gap
// there. `c` has no counterpart either; `d` does. That shape is what lets one
// fixture exercise the order, the skip and the fallback rather than three.
const slots = { a: whole(4), b: whole(4), c: whole(4), d: whole(4) }
const order = ['a', 'b', 'c', 'd']

// Every slot of `a` pairs into `b` except slot 2. `d` pairs slot 2; `c` never
// does.
const pairs = { 'a|b|0': 0, 'a|b|1': 1, 'a|b|3': 3, 'a|d|2': 2 }

const resolver = fakeResolver(slots, pairs)
const inGap: ReadingState = { authority: 'a', slot: 2, viewing: 'b' }
const on = { enabled: true }

describe('lenderFor', () => {
  it('prefers an edition that is neither viewed nor the authority', () => {
    expect(lenderFor(inGap, resolver, order)).toEqual({ edition: 'd', slot: 2 })
  })

  it('skips a candidate that does not resolve, in the order given', () => {
    // `c` comes before `d` and has no pairing for slot 2, so a rule that
    // returned the first *candidate* rather than the first that *resolves*
    // would answer `c` here.
    expect(lenderFor(inGap, resolver, order)?.edition).not.toBe('c')
  })

  it('falls back to the authority when no third edition resolves', () => {
    const twoEditions = fakeResolver({ a: whole(4), b: whole(4) }, pairs)
    expect(lenderFor(inGap, twoEditions, ['a', 'b'])).toEqual({
      edition: 'a',
      slot: 2,
    })
  })

  it('never lends the edition being viewed', () => {
    // `b` resolves to itself trivially, so a rule that forgot to exclude the
    // viewed edition would answer `b` — and draw the page that is missing.
    const fromD: ReadingState = { authority: 'a', slot: 2, viewing: 'd' }
    expect(lenderFor(fromD, resolver, order)?.edition).not.toBe('d')
  })

  it('has nothing to lend when the authority slot does not exist', () => {
    const offEnd: ReadingState = { authority: 'a', slot: 99, viewing: 'b' }
    expect(lenderFor(offEnd, resolver, order)).toBeUndefined()
  })
})

describe('withBorrow', () => {
  const gap = display(inGap, resolver)

  it('maps a gap to a borrowed page, keeping `edition` the viewed one', () => {
    expect(withBorrow(gap, inGap, resolver, order, on)).toEqual({
      kind: 'borrowed',
      edition: 'b',
      from: { edition: 'a', slot: 2 },
      lender: { edition: 'd', slot: 2 },
    })
  })

  it('leaves a gap alone when the module is switched off', () => {
    expect(withBorrow(gap, inGap, resolver, order, { enabled: false })).toBe(gap)
  })

  it('leaves a gap alone when nothing can lend', () => {
    const offEnd: ReadingState = { authority: 'a', slot: 99, viewing: 'b' }
    const shown = display(offEnd, resolver)
    expect(withBorrow(shown, offEnd, resolver, order, on)).toBe(shown)
  })

  it('leaves a drawn page alone', () => {
    const at: ReadingState = { authority: 'a', slot: 0, viewing: 'b' }
    const shown = display(at, resolver)
    expect(shown.kind).toBe('at')
    expect(withBorrow(shown, at, resolver, order, on)).toBe(shown)
  })
})

describe('borrowModule', () => {
  it('starts enabled and claims no keys', () => {
    expect(borrowModule.defaults).toEqual({ enabled: true })
    expect(borrowModule.keys).toBeUndefined()
  })
})
```

- [ ] **Step 2: Run the test to verify it fails**

```
pnpm --prefix web exec vitest run --project node src/modules/borrow.test.ts
```

Expected: FAIL — `Failed to resolve import "./borrow"`.

- [ ] **Step 3: Add the `borrowed` arm to `Display`**

In `web/src/reading.ts`, replace the `Display` type (currently at line 76):

```ts
export type Display =
  | { kind: 'at'; edition: string; slot: number; via: Landed['via'] }
  /**
   * The viewed edition has no page here and another edition's is drawn in
   * its place — `modules/borrow.ts` decides whose, and `Borrowed.tsx` draws
   * it marked.
   *
   * `edition` is the edition being *viewed*, exactly as in the other two
   * arms, and not the lender. `App.tsx` passes `shown.edition` to `allSlots`
   * and to `alignedBreaksCached`, so naming the lender here would group and
   * index against the wrong edition — invisibly, since nothing is drawn from
   * that grouping while this arm is showing.
   */
  | {
      kind: 'borrowed'
      edition: string
      from: { edition: string; slot: number }
      /** Whose page is drawn, and which slot of it. */
      lender: { edition: string; slot: number }
    }
  | { kind: 'gap'; edition: string; from: { edition: string; slot: number } }
```

Leave `display()` itself untouched.

- [ ] **Step 4: Write the implementation**

Create `web/src/modules/borrow.ts`:

```ts
import type { ReaderModule } from '../module'
import type { Display, ReadingState } from '../reading'
import type { Resolver } from '../resolver'

/** Whether a gap is filled from another edition, or left as a gap. */
export interface BorrowPrefs {
  enabled: boolean
}

/**
 * Drawing another edition's page where the viewed edition has none.
 *
 * Claims no keys: a reader decides once whether they want gaps filled, rather
 * than toggling it mid-read — `readouts`' and `prefetch`'s reasoning, against
 * `direction`'s `d` and `regroup`'s `b`.
 *
 * Defaults on, matching `zoom`, `prefetch` and `help`. The switch exists
 * because a reader comparing translations may want to *see* that a page is
 * missing; that is served by it existing, not by it defaulting off.
 */
export const borrowModule: ReaderModule<BorrowPrefs> = {
  id: 'borrow',
  label: 'Borrowed pages',
  defaults: { enabled: true },
}

/**
 * Which edition's page to draw where the viewed one has none, and which slot
 * of it.
 *
 * Candidates are `order` — `App.tsx` passes `Session.editions`, which
 * `startup.ts` builds with `orderedEditions`, so the hub is first — minus the
 * viewed edition and minus the authority. The first that resolves wins; if
 * none does, the authority lends.
 *
 * **The authority can always lend**, which is why the fallback needs no
 * resolution: `from` below is its own slot, so there is nothing to route. The
 * one case with no lender at all is `from` being undefined, meaning the
 * authority slot does not exist — and that is the only gap that survives this
 * module being enabled.
 *
 * **The authority is tried last rather than first, and that rule is
 * imperfect.** It is right when the authority is the raw and a third edition
 * is another English release, which is the case the feature exists for. It is
 * wrong in the mirror case — authority `en-fan`, viewed `en-official`, third
 * `raw-mono` — where it draws the raw over the other English edition. Nothing
 * available distinguishes them: the sidecar's `editions` block carries a
 * free-text `label` and no language or kind field. See the design's §2.
 */
export function lenderFor(
  state: ReadingState,
  resolver: Resolver,
  order: string[],
): { edition: string; slot: number } | undefined {
  const from = resolver.slotAt(state.authority, state.slot)
  if (from === undefined) return undefined
  for (const edition of order) {
    if (edition === state.viewing || edition === state.authority) continue
    const landed = resolver.resolve(state.authority, edition, from)
    const slot = landed && resolver.indexOf(edition, landed)
    if (slot !== undefined) return { edition, slot }
  }
  return { edition: state.authority, slot: state.slot }
}

/**
 * A `gap` with a lender becomes `borrowed`; everything else passes through
 * unchanged, by identity, so a caller can compare against what it passed in.
 *
 * **Layered on top of `display` rather than built into it.** `nearest.ts`'s
 * `landingAt` calls `display` and treats anything that is not `at` as
 * not-landing, which is exactly right for the offers either side of a gap:
 * they must land on a *real* page in the edition the reader chose. A `display`
 * that returned `borrowed` would silently turn each of those offers into a
 * jump to a third edition's page instead — the opposite of what they are for.
 * Keeping the decision here means `nearest.ts` needs no change at all.
 */
export function withBorrow(
  shown: Display,
  state: ReadingState,
  resolver: Resolver,
  order: string[],
  prefs: BorrowPrefs,
): Display {
  if (!prefs.enabled || shown.kind !== 'gap') return shown
  const lender = lenderFor(state, resolver, order)
  if (lender === undefined) return shown
  return {
    kind: 'borrowed',
    edition: shown.edition,
    from: shown.from,
    lender,
  }
}
```

- [ ] **Step 5: Run the test to verify it passes**

```
pnpm --prefix web exec vitest run --project node src/modules/borrow.test.ts
```

Expected: PASS, 10 tests.

- [ ] **Step 6: Typecheck and lint**

```
pnpm --prefix web typecheck && pnpm --prefix web lint
```

Expected: both clean. If `lint` reports formatting, run `pnpm --prefix web
format` and re-run.

- [ ] **Step 7: Commit**

```bash
git add web/src/modules/borrow.ts web/src/modules/borrow.test.ts web/src/reading.ts
git commit -m "Pick an edition to lend a page where the viewed one has none

lenderFor prefers an edition that is neither viewed nor the authority,
falling back to the authority — which can always lend, since its own slot
is the position. Display gains a borrowed arm; display() is unchanged, so
nearest.ts's offers keep landing only on real pages."
```

---

### Task 2: The `Borrowed` component

**Files:**
- Create: `web/src/Borrowed.tsx`
- Create: `web/src/Borrowed.test.tsx`

**Interfaces:**
- Consumes: `PageView` from `./PageView`; `FitPrefs` from `./modules/fit`;
  `mount` from `./testing/mount` (tests only).
- Produces: `export interface BorrowedProps` and
  `export function Borrowed(props: BorrowedProps)`.

**Note on the `offers` prop.** `Borrowed` takes the Gap view as an opaque
`ReactNode` rather than re-declaring `Gap`'s seven props and passing them
through. That keeps this component's surface at four props, keeps its tests
free of `Gap` entirely, and leaves exactly one place that knows how to build a
`<Gap>` — `App.tsx`, which already did.

---

- [ ] **Step 1: Write the failing test**

Create `web/src/Borrowed.test.tsx`:

```tsx
import { describe, expect, it } from 'vitest'
import { Borrowed } from './Borrowed'
import { fitModule } from './modules/fit'
import { mount } from './testing/mount'

const lender = { edition: 'raw-mono', page: 47 }
const props = {
  lender,
  src: '/api/works/w/raw-mono/pages/46',
  fit: fitModule.defaults,
  offers: <p>the offers</p>,
}

describe('Borrowed', () => {
  it('names the lender and its page on the mark', () => {
    const host = mount(<Borrowed {...props} />)
    expect(host.textContent).toContain('raw-mono')
    expect(host.textContent).toContain('47')
  })

  it('says so in the alt text, which is not a style choice', () => {
    const host = mount(<Borrowed {...props} />)
    expect(host.querySelector('img')?.alt).toBe(
      'page 47, borrowed from raw-mono',
    )
  })

  it('draws the page, not the offers, until the mark is pressed', () => {
    const host = mount(<Borrowed {...props} />)
    expect(host.textContent).not.toContain('the offers')
    expect(host.querySelector('img')).not.toBeNull()
  })

  it('reveals the offers when the mark is pressed', () => {
    const host = mount(<Borrowed {...props} />)
    const mark = host.querySelector('button')
    expect(mark).not.toBeNull()
    mark?.click()
    expect(host.textContent).toContain('the offers')
  })

  it('puts the mark in the tab order, since no key claims it', () => {
    const host = mount(<Borrowed {...props} />)
    const mark = host.querySelector('button')
    expect(mark?.tagName).toBe('BUTTON')
    expect(mark?.getAttribute('type')).toBe('button')
  })
})
```

- [ ] **Step 2: Run the test to verify it fails**

```
pnpm --prefix web exec vitest run --project jsdom src/Borrowed.test.tsx
```

Expected: FAIL — `Failed to resolve import "./Borrowed"`.

- [ ] **Step 3: Write the implementation**

Create `web/src/Borrowed.tsx`:

```tsx
import { type ReactNode, useState } from 'react'
import type { FitPrefs } from './modules/fit'
import { PageView } from './PageView'

export interface BorrowedProps {
  /**
   * Whose page is drawn, and its page number in that edition. `page` is
   * 1-based for display — `App.tsx` converts at the point it builds this
   * prop, the same boundary convention `Gap`'s `page` props follow.
   */
  lender: { edition: string; page: number }
  src: string
  fit: FitPrefs
  /**
   * The `Gap` view for this slot, revealed when the mark is pressed.
   *
   * An opaque node rather than `Gap`'s own seven props re-declared here: it
   * keeps this component's surface small, keeps its tests free of `Gap`, and
   * leaves `App.tsx` the one place that knows how to build one.
   */
  offers: ReactNode
}

/**
 * A page borrowed from another edition, where the viewed edition has none.
 *
 * Drawn solo, in the page area, in every mode. That is not a simplification:
 * `App.tsx` tests this arm before it consults `mode`, and scroll and two-page
 * mode both draw `grouped`, built from the viewed edition's own slots — every
 * one of which exists. There is no column with a hole for this to sit in.
 *
 * **The mark is the control.** The one thing a reader might want instead is a
 * real page in the edition they chose, which is exactly what `Gap`'s
 * before/after offers are — so reaching them from the thing that says the page
 * is borrowed puts them where the question arises. A keybinding would be a
 * fifth thing to learn for a screen most readers meet rarely, and `borrow`
 * claims no keys.
 *
 * `useState` rather than a prop lifted into `App.tsx`: the toggle is reset by
 * a `key` on this component that includes the position, so a page turn
 * remounts it and there is no stale state for an effect to clear.
 */
export function Borrowed({ lender, src, fit, offers }: BorrowedProps) {
  const [showingOffers, setShowingOffers] = useState(false)
  if (showingOffers) return <>{offers}</>
  return (
    <div
      style={{
        position: 'relative',
        height: '100%',
        // The tinted leading edge. Inset rather than a border on the page
        // itself, so it cannot alter the geometry `fit` computes.
        boxShadow: 'inset 4px 0 0 0 rgba(180, 140, 60, 0.9)',
      }}
    >
      <button
        type="button"
        onClick={() => setShowingOffers(true)}
        style={{
          position: 'absolute',
          top: 0,
          left: 0,
          zIndex: 1,
          border: 'none',
          padding: '4px 10px',
          font: 'inherit',
          fontSize: '0.85em',
          cursor: 'pointer',
          background: 'rgba(180, 140, 60, 0.9)',
          color: '#1a1a1a',
        }}
      >
        borrowed from {lender.edition} page {lender.page}
      </button>
      <PageView
        src={src}
        alt={`page ${lender.page}, borrowed from ${lender.edition}`}
        // This component is remounted per position (see the `key` in
        // `App.tsx`), so no mounted instance ever sees its `src` change —
        // which is the only state buffering exists for.
        buffered={false}
        fit={fit}
      />
    </div>
  )
}
```

- [ ] **Step 4: Run the test to verify it passes**

```
pnpm --prefix web exec vitest run --project jsdom src/Borrowed.test.tsx
```

Expected: PASS, 5 tests.

**If `PageView` misbehaves under jsdom** (it is currently only exercised in the
`browser` project, by `PageView.browser.test.tsx`), do not stub it. Move this
file to `web/src/Borrowed.browser.test.tsx` unchanged — `mount` is shared by
both projects for exactly this reason — and run it with
`pnpm --prefix web exec vitest run --project browser src/Borrowed.browser.test.tsx`.
Record which project it landed in; Task 4's test counts depend on it.

- [ ] **Step 5: Typecheck and lint**

```
pnpm --prefix web typecheck && pnpm --prefix web lint
```

Expected: both clean.

- [ ] **Step 6: Commit**

```bash
git add web/src/Borrowed.tsx web/src/Borrowed.test.tsx
git commit -m "Draw the borrowed page, marked, with the mark as the way back

The mark is a button that reveals the Gap offers, so the one thing a reader
might want instead is reachable from the thing telling them the page is
borrowed. Alt text says borrowed unconditionally — the substitution must not
be a purely visual signal."
```

---

### Task 3: Wire it into the reader

**Files:**
- Modify: `web/src/modules/registry.ts`
- Modify: `web/src/App.tsx` (four edits, listed below)

**Interfaces:**
- Consumes: `borrowModule`, `withBorrow` (Task 1); `Borrowed` (Task 2).
- Produces: nothing further tasks depend on.

**Why there is no unit test in this task.** `App.tsx` value-imports
`resolver.ts` and is therefore permanently unreachable from Vitest — see that
file's own comment. Step 6 is the verification, in a browser against the
corpus, which is this repository's existing practice for every `App.tsx`
change.

---

- [ ] **Step 1: Register the module**

In `web/src/modules/registry.ts`, add the import beside the others in
alphabetical order:

```ts
import { borrowModule } from './borrow'
```

and add `borrowModule` to the array passed to `buildRegistry`. Per that file's
own doc, this is the whole registration: `App.tsx`'s preference state is one
map built from `registry.all()`, and `Settings.tsx` generates the checkbox
from `fieldKind` reporting `'boolean'`.

- [ ] **Step 2: Read the preference slice**

In `web/src/App.tsx`, add the import beside the other module imports:

```ts
import { borrowModule, withBorrow } from './modules/borrow'
import { Borrowed } from './Borrowed'
```

and add the slice beside `fitPrefs` (currently line 445):

```ts
const borrowPrefs = sliceFor(prefsMap, borrowModule)
```

- [ ] **Step 3: Wrap the `shown` binding**

Replace line 1264, `const shown = display(state, resolver)`, with:

```ts
  // `editions` is already `orderedEditions`' output — `startup.ts` builds it
  // that way — so the hub is first and `withBorrow` needs no second ordering.
  const shown = withBorrow(
    display(state, resolver),
    state,
    resolver,
    editions,
    borrowPrefs,
  )
```

**Do not touch the four other `display` calls** (lines ~742, ~780, ~805,
~1076). Each guards on `kind !== 'at'`, which is already the right answer for a
borrowed page: there is no slot of the viewed edition to correct, to annotate,
or to prefetch. Wrapping them would change nothing and would suggest they had
been considered and needed to.

- [ ] **Step 4: Compute the offers in a borrowed state too**

At the `gapMoves` binding (currently line 2092), widen the guard:

```ts
  const gapMoves =
    shown.kind === 'gap' || shown.kind === 'borrowed'
      ? nearestPaired(
          state,
          resolver,
          resolver.slotCount(state.authority) ?? state.slot + 1,
        )
      : undefined
```

Amend the comment above it: the offers are computed in a borrowed state as
well, because the mark reveals them and the filmstrip still anchors on
`stripAnchor` — a reader looking at a borrowed page is standing in the same
place a gap would have put them.

- [ ] **Step 5: Render it**

Replace the opening of the page-area ternary (currently line 2243,
`{shown.kind === 'gap' ? (`) so that a `<Gap>` is built once and either
rendered directly or handed to `Borrowed`. Insert immediately above the
returned JSX's page area, beside `toMove` (line 2107):

```ts
  // One `<Gap>`, built from whichever arm is showing: rendered directly in a
  // gap, and handed to `Borrowed` as the thing its mark reveals. Two call
  // sites constructing the same seven props is the shape that lets them
  // drift.
  const gapView = (edition: string, from: { edition: string; slot: number }) => (
    <Gap
      reading={effectiveReading}
      edition={edition}
      from={{
        edition: from.edition,
        page: resolver.slotAt(from.edition, from.slot)!.page + 1,
      }}
      verdict={verdictFor(alignments, from.edition, edition)}
      before={toMove(gapMoves?.before)}
      after={toMove(gapMoves?.after)}
      onTake={take}
    />
  )
```

Then replace the `shown.kind === 'gap' ? ( <Gap …/> ) :` branch — the whole
`<Gap>` element currently spanning lines 2243–2256 — with:

```tsx
        {shown.kind === 'gap' ? (
          gapView(shown.edition, shown.from)
        ) : shown.kind === 'borrowed' ? (
          <Borrowed
            // Remounts on any real move, which is what resets the mark's
            // toggle without an effect to clear it.
            key={`${state.authority}:${state.slot}:${shown.edition}`}
            lender={{
              edition: shown.lender.edition,
              page:
                resolver.slotAt(shown.lender.edition, shown.lender.slot)!.page +
                1,
            }}
            src={pageUrl(
              work,
              shown.lender.edition,
              resolver.slotAt(shown.lender.edition, shown.lender.slot)!.page,
              pageLongEdge,
            )}
            fit={fitPrefs}
            offers={gapView(shown.edition, shown.from)}
          />
        ) : mode === 'scroll' ? (
```

leaving the rest of the ternary chain as it is.

- [ ] **Step 6: Verify in the browser against the corpus**

`gotoubun-v01` is the only corpus volume with three editions (`raw-mono`,
`en-mono`, `en-fan`) and therefore the only material where Task 1's rule does
anything the rejected "authority always" rule would not.

Serve a **scratch copy** of the sidecar with absolute member paths, so nothing
this session does can write into the gitignored corpus tree. Build the reader
and start the server, then check, in order:

1. Navigate in `raw-mono` to a slot `en-mono` lacks, swap to `en-mono`. A page
   draws, marked `borrowed from …`, rather than the Gap view.
2. The lender is `en-fan` where `en-fan` has the page — not `raw-mono`. This is
   the behaviour the whole rule exists for; if it shows `raw-mono` while
   `en-fan` has the slot, Task 1 is wrong.
3. Press the mark. The Gap view appears with its before/after offers, pointing
   the way the pages go for an RTL work.
4. Take one of the offers. It lands on a real `en-mono` page, and authority
   moves there.
5. Turn a page away and back. The mark is showing again, not the Gap view —
   the `key` reset works.
6. Open settings (`s`), switch **Borrowed pages** off. The same position now
   shows the Gap view. Switch it on; the page returns.
7. Repeat 1 and 6 in two-page mode and in continuous scroll. In both, the
   borrowed page draws solo and fills the page area.
8. Confirm the filmstrip still marks the nearest real page rather than nothing.

Record what you actually saw, including anything that disagreed with this list.
A step that could not be reached is a finding, not a step to skip quietly.

- [ ] **Step 7: Typecheck, lint, and run both TypeScript projects**

```
pnpm --prefix web typecheck && pnpm --prefix web lint && pnpm --prefix web test && pnpm --prefix web test:jsdom
```

Expected: all clean, with the new tests included and no existing test changed.
**If any existing test now fails, stop and report it** — this task is supposed
to add a branch, not alter one.

- [ ] **Step 8: Commit**

```bash
git add web/src/App.tsx web/src/modules/registry.ts
git commit -m "Draw a borrowed page where the reader would have seen a gap

One binding wraps display with withBorrow; the gapMoves guard widens so the
mark's offers and the filmstrip anchor are computed in a borrowed state too;
and one gapView helper builds the Gap element for both the gap arm and the
mark. The four other display calls are deliberately untouched — each guards
on kind !== 'at', which is already right for a borrowed page."
```

---

### Task 4: Close the "Later" entry and run the gate

**Files:**
- Modify: `docs/status.md` (the "Later" entry at line 3798; the test counts at
  lines 78–79)

**Interfaces:** none.

**The ritual that applies.** This is a **"Later" entry, not an O-number**, so
closing it is an edit to that entry — not the five-part open-items edit. Follow
`serve-cache-root`'s precedent: the entry is **kept and marked shipped**, not
deleted, because a list of future work is also a record of what the project
expected of that work.

---

- [ ] **Step 1: Get the real test counts**

```
pnpm --prefix web test && pnpm --prefix web test:jsdom && pnpm --prefix web test:browser
```

Write down the three totals. Do not compute them by adding to the numbers in
`status.md` — count what the runner prints. If Task 2's tests landed in the
`browser` project rather than `jsdom`, that is where the increase shows.

- [ ] **Step 2: Update the counts**

`docs/status.md` lines 78–79 currently read:

```
Five crates, 741 tests, plus 1,306 in `web/`'s TypeScript — 963 in the `node`
project and 343 in the `jsdom` one — and sixteen more in a
```

Update the total, the `node` figure and the `jsdom` figure to what Step 1
printed, and the `sixteen` if the browser project grew. The three must agree
with each other: node + jsdom must equal the total.

- [ ] **Step 3: Rewrite the "Later" entry**

At `docs/status.md:3798`, rewrite the entry to open with the shipped marker and
keep the original reasoning, in the shape `serve-cache-root`'s entry uses.
Cover, in prose rather than a list:

- **Shipped 2026-08-12** on `gap-fallback`, with
  `docs/superpowers/specs/2026-08-12-gap-fallback-design.md` as the record, and
  the entry kept as the opportunity that opened it.
- **What shipped:** the lender rule, the mark, the mark-as-control, and the
  `borrow` module's switch, defaulting on.
- **The two findings from §1 of the design that the entry did not know**, which
  are the part worth keeping: the authority can always lend, so the only gap
  that survives is an invalid authority slot; and a gap replaces the whole page
  area in every mode, so a borrowed page is drawn solo rather than woven into a
  scroll column. The entry was written expecting the second to be otherwise.
- **What the rule gets wrong**, from the design's §2: preferring a third
  edition over the authority is wrong when the authority is an English edition
  and the third is the raw, and no field in the sidecar distinguishes them.
- **The two-edition degeneration**, stated rather than hidden: on a work with
  two editions the rule draws the page the reader just swapped away from,
  marked, and whether that is useful or irritating is unmeasured.
- **What a borrowed page still is not** — no annotations, no zoom, not a
  prefetch target — with a pointer to `docs/notes.md`'s "A borrowed page is not
  a reading position, and might one day want to be". Do **not** file this as an
  open item or a new "Later" entry; it is a note by decision.

- [ ] **Step 4: Sweep for stale prose the entry edit cannot predict**

```
grep -rn 'Fall back to another edition\|no counterpart for the slot' docs/ web/src/
grep -n 'Gap' docs/status.md
```

Read every hit and correct any that still describes the gap as the only answer
when an edition lacks a page. Prose *outside* the entry describes this
behaviour in wording no sweep can predict — that is the failure mode the
open-items ritual records, and it applies to "Later" entries too.

- [ ] **Step 5: Run the full gate**

```
env -u TAIYAKU_CORPUS scripts/check-all.sh
```

Expected: all four CI jobs pass. **The `env -u` is not optional** — with
`TAIYAKU_CORPUS` set, the script exports it into an unfiltered
`cargo test --workspace` that runs a debug-profile experiment for 40+ minutes.

- [ ] **Step 6: Commit**

```bash
git add docs/status.md
git commit -m "Close the gap-fallback entry under Later, and correct what it expected

Kept rather than deleted: the entry expected a borrowed page woven into the
page sequence, and the reader turns out to replace the whole page area in
every mode, so it is drawn solo. Records what the lender rule gets wrong and
the two-edition degeneration, neither of which is measured."
```

- [ ] **Step 7: Open the PR**

The remote is Gitea, not GitHub — `gh` is authenticated against the wrong host.
Use the gitea MCP tools. The remote is squash-only, so **the PR body becomes
the squash commit message verbatim, once**; local commit messages are never
read. Write the body as the commit this branch should become, and **do not hard
wrap it**.

---

## Self-Review

**Spec coverage.** §1's two findings → Task 1 (the `Display` doc comment) and
Task 2 (the component doc). §2's rule and its stated flaw → Task 1, Steps 1
and 4. §3's type and the `edition`-means-viewed decision → Task 1, Step 3. §4's
"`display` is unchanged" → Task 1 Step 3, plus Task 3 Step 3's explicit
instruction not to touch the other four call sites. §5's module and default →
Task 1 Step 4 and Task 3 Step 1. §6's view, mark, alt text and `key` → Task 2
and Task 3 Step 5. §7's non-goals → nothing to build; the `notes.md` entry is
already committed. §8's testing → Tasks 1, 2 and 3 Step 6. §9's five
"deliberately not done" items → none are built. §10's two unmeasured claims →
carried into `status.md` by Task 4 Step 3 rather than asserted away.

**Placeholder scan.** No TBD, no "add error handling", no "similar to Task N".
Every code step carries its code. Task 4 Step 3 is prose-by-bullet rather than
literal text, deliberately: the surrounding entry must be read and matched in
voice, and a canned paragraph would read as pasted.

**Type consistency.** `BorrowPrefs`, `borrowModule`, `lenderFor`, `withBorrow`
and the `borrowed` arm's four fields (`kind`, `edition`, `from`, `lender`) are
spelled identically in Tasks 1 and 3. `BorrowedProps`' four props (`lender`,
`src`, `fit`, `offers`) match between Task 2's implementation, its tests and
Task 3's call site. `lender.page` is 1-based at every boundary that shows it
and 0-based in the `pageUrl` call, which is the convention the constraints
state.

**One risk carried rather than resolved.** Task 2 Step 4 does not know whether
`PageView` mounts cleanly under jsdom; it names the fallback and the cost
(which project the tests land in, which Task 4 Step 1 then counts) instead of
guessing.
