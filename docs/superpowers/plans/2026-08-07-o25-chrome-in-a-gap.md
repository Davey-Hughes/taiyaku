# O25 — the chrome, and every panel, reachable from a gap

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close O25 — mount the filmstrip, the readout row and the settings
panel while the reader is showing a gap, by removing the early return that made
them unreachable.

**Architecture:** `App.tsx` returns early on `shown.kind === 'gap'`, so
everything below that branch is never rendered. This narrows the `Display`
union once (`const at = shown.kind === 'at' ? shown : undefined`), gives five
values a gap form, and substitutes `<Gap>` into the page area instead of
short-circuiting the tree. Two pure modules gain the values the gap needs:
`nearestPaired` carries the viewed-edition slot it already computes, and
`readouts.ts` gains an edition prefix and a gap-specific health line.

**Tech Stack:** React 19, TypeScript, Vite, `vitest`. No new dependencies.

Design: [`../specs/2026-08-07-o25-chrome-in-a-gap-design.md`](../specs/2026-08-07-o25-chrome-in-a-gap-design.md).

## Global Constraints

- **Every command runs from `web/`.** `npm test`, `npm run typecheck`,
  `npm run lint`.
- **`npm run lint` is `oxlint --deny-warnings`** — a warning fails it.
- **No new dependencies.** `package.json` must not change.
- **Zero-based internally, 1-based only at a boundary a person reads.** Slots
  and page indices stay zero-based everywhere except immediately before they
  reach display text or `alt` text. `App.tsx`'s `altAt` is the standing example.
- **Every branch carries one checkable claim.** The remote is squash-only, so a
  branch becomes one commit. Do not fold O27 (`pageUrlRef`, dead) into this
  branch — it is tabled separately and stays that way.
  > **Overruled after the five tasks ran.** The owner chose to close O27 on
  > this branch after all, in a tenth commit. This constraint stood for the
  > whole of the execution above — Task 4 Step 5 was written under it and
  > deliberately kept the ref compiling — so it is left as written rather than
  > edited to match the outcome. See O27's own entry in `status.md`.
- **Comments explain why, not what.** This file's existing comments are the
  house style: they name the alternative that was rejected and the measurement
  or bug that decided it. Match that density.
- **Work on a branch.** `git switch -c o25-chrome-in-a-gap` before Task 1.

---

### Task 1: `nearestPaired` carries the viewed-edition slot

`nearest.ts`'s `landingAt` already computes `landed.slot` — the slot in the
edition being viewed — and throws it away, returning only the authority-space
`slot` and the display `page`. The filmstrip needs that discarded value to know
which tile to centre on. Adding it introduces no new `resolve` call, which is
the same argument `NearestOffer`'s own doc already makes about carrying `page`.

**Files:**
- Modify: `web/src/nearest.ts`
- Test: `web/src/nearest.test.ts`

**Interfaces:**
- Consumes: nothing from earlier tasks.
- Produces: `NearestOffer` gains `at: number`, a slot index in
  `state.viewing`. Task 4 reads `before?.at ?? after?.at`.

- [ ] **Step 1: Write the failing tests**

In `web/src/nearest.test.ts`, replace the three existing expectations that
assert a whole offer object. The fixture is unchanged: editions `a` (8 whole
slots) and `b` (9 slots, page 2 being a joined spread), with `a`'s slots 1, 2
and 3 paired to `b`'s 0, 1 and 2.

```ts
describe('nearestPaired', () => {
  it('offers the paired slots either side of a gap, already resolved', () => {
    // `a` slot 4 is unpaired; 3 is the nearest paired before it and there is
    // nothing paired after it. `b`'s slot 2 is `{ page: 2, half: 'R' }` — the
    // resolved page and the slot holding it both travel with the offer, so a
    // caller has neither to resolve again.
    expect(nearestPaired({ authority: 'a', slot: 4, viewing: 'b' }, r, 8))
      .toEqual({ before: { slot: 3, page: 2, at: 2 } })
  })

  it('offers only what exists at the start of a volume', () => {
    expect(nearestPaired({ authority: 'a', slot: 0, viewing: 'b' }, r, 8))
      .toEqual({ after: { slot: 1, page: 0, at: 0 } })
  })
```

Leave the third test (`offers nothing where the whole edition is unpaired`)
exactly as it is — it asserts `{}` and has nothing to carry.

In the fourth test, update its one expectation:

```ts
    expect(calls).toBe(4)
    expect(found).toEqual({ before: { slot: 3, page: 2, at: 2 } })
```

Then add a fifth test, which is the one that pins the property the whole change
exists for — that `at` addresses the viewed edition, not the authority:

```ts
  it('reports `at` in the viewed edition, not in the authority', () => {
    // `a` slot 3 pairs to `b` slot 2. The two numbers differ, so a
    // confusion between the spaces would be visible here and nowhere in the
    // tests above, where `slot` and `at` happen to be close together.
    // `slot` is what `takeOffered` navigates by (authority-space); `at` is
    // what the filmstrip centres on (a tile of `b`).
    const found = nearestPaired({ authority: 'a', slot: 4, viewing: 'b' }, r, 8)
    expect(found.before?.slot).toBe(3)
    expect(found.before?.at).toBe(2)
    expect(r.slotAt('b', found.before!.at)).toEqual({ page: 2, half: 'R' })
  })
})
```

- [ ] **Step 2: Run the tests to verify they fail**

```sh
cd web && npm test -- nearest
```

Expected: FAIL. The first, second and fourth report an object missing `at`; the
fifth reports `found.before?.at` is `undefined`.

- [ ] **Step 3: Write the implementation**

In `web/src/nearest.ts`, add the field to the interface:

```ts
export interface NearestOffer {
  /** Authority-space — what `onTake`/`takeOffered` expects. */
  slot: number
  /** Zero-based page in `state.viewing`. Display converts at its own boundary, same as `resolveOffered`'s. */
  page: number
  /**
   * Zero-based *slot* in `state.viewing` — not the authority-space `slot`
   * above, and not derivable from `page` where the edition holds joined
   * spreads (one page, two slots).
   *
   * Carried for the same reason `page` is: `landingAt` below already has it
   * as `landed.slot` and would otherwise discard it, leaving a caller to
   * resolve the same offer a second time. `App.tsx` centres the filmstrip on
   * this, since a gap has no slot of its own in the viewed edition and the
   * nearest one that resolves is the closest thing to "where the reader is".
   */
  at: number
}
```

and return it from `landingAt`:

```ts
  const landingAt = (slot: number): NearestOffer | undefined => {
    const landed = display({ ...state, slot }, resolver)
    if (landed.kind !== 'at') return undefined
    return { slot, page: resolver.slotAt(landed.edition, landed.slot)!.page, at: landed.slot }
  }
```

- [ ] **Step 4: Run the tests to verify they pass**

```sh
cd web && npm test -- nearest && npm run typecheck && npm run lint
```

Expected: all `nearest` tests PASS, typecheck clean, lint clean.

`npm run typecheck` will report **one unrelated error** if run against the
whole project before Task 4: none expected here, because adding a field to an
interface breaks no consumer. If typecheck does report an error in `App.tsx`,
stop and read it — it means a consumer was constructing a `NearestOffer`
literal, which nothing should be doing outside `nearest.ts`.

- [ ] **Step 5: Commit**

```sh
git add web/src/nearest.ts web/src/nearest.test.ts
git commit -m "nearest: carry the viewed-edition slot an offer already resolved"
```

---

### Task 2: `positionLine` names an edition, and `gapHealth` reports the gap's own pair

Two pure additions to `readouts.ts`.

The **edition prefix** is what keeps the position line honest when it switches
scale. `positionLine`'s own doc already argues the percentage should change on
a swap ("hiding it would imply the two editions are the same length, which they
are not") — but in a gap the line reports the *authority's* numbers while the
reader is looking at another edition, and a percentage that changed scale with
no marker would be a worse claim than the one that doc rejects.

**`gapHealth`** exists to stop a contradiction. `healthReadout` reports the
hub↔viewed pair and goes silent when `viewing` *is* the hub; `Gap.tsx` reports
the authority↔viewed pair. In a gap those disagree — a reader whose authority
is a spoke and who swaps to the hub gets a verdict sentence from the view and
silence from the row directly beneath it.

**Files:**
- Modify: `web/src/modules/readouts.ts`
- Test: `web/src/modules/readouts.test.ts`

**Interfaces:**
- Consumes: nothing from earlier tasks.
- Produces:
  - `PositionInput` gains `edition?: string`.
  - `gapHealth(alignments: Alignment[], from: string, viewing: string): string`
    — note the return type is `string`, not `string | undefined`.

- [ ] **Step 1: Write the failing tests**

In `web/src/modules/readouts.test.ts`, add `gapHealth` to the import list from
`./readouts` (keep the list alphabetical — it goes after `chapterReadout`).

Add two cases to the existing `positionLine` describe block:

```ts
  it('names the edition when one is given, so a change of scale is never silent', () => {
    // Only a gap sets this. The line then reports the authority's position
    // while the reader is looking at another edition, and the percentage is
    // against the authority's slot count — so the row has to say whose
    // numbers these are.
    expect(positionLine({ slot: 88, total: 193, page: 87, edition: 'raw-mono' }))
      .toBe('raw-mono slot 89 / 193 · p. 87 · 46%')
  })

  it('is unchanged where no edition is given', () => {
    // The reading case, which is every case but the gap. No prefix, no
    // leading space.
    expect(positionLine({ slot: 87, total: 195, page: 84, edition: undefined }))
      .toBe('slot 88 / 195 · p. 84 · 45%')
  })
```

Add a new describe block after the existing `healthReadout` one:

```ts
describe('gapHealth', () => {
  const alignments: Alignment[] = [
    { from: 'raw-mono', to: 'en-mono', quality: { verdict: 'sound' } },
    { from: 'raw-mono', to: 'en-fan', quality: { verdict: 'fragmented' } },
  ]

  it('says a sound pairing still has no counterpart here', () => {
    // The case `healthReadout` cannot produce: it is silent on a sound edge,
    // because a sound pairing with a counterpart has nothing to report. A
    // gap is a slot-level absence inside an otherwise sound pairing, which
    // is exactly what `Gap.tsx`'s own sentence for `sound` says in words.
    expect(gapHealth(alignments, 'raw-mono', 'en-mono')).toBe('sound pair · no counterpart here')
  })

  it('names a fragmented pairing and the absence together', () => {
    expect(gapHealth(alignments, 'raw-mono', 'en-fan')).toBe('fragmented pair · no counterpart here')
  })

  it('reports unknown rather than nothing where no edge is stored', () => {
    // `healthLine`'s standing rule: silence must never be mistaken for
    // `sound`. A spoke-to-spoke pair has no stored edge in a hub-and-spoke
    // sidecar, and guessing at a composed verdict is the one thing
    // `verdictFor` refuses to do.
    expect(gapHealth(alignments, 'en-mono', 'en-fan')).toBe('verdict unknown')
  })

  it('speaks even where the viewed edition is the hub', () => {
    // `healthReadout` returns `undefined` here by design — reading the hub
    // unswapped has no cross-edition pairing to judge. But a gap reached
    // *from* a spoke is a real pairing between two named editions, and
    // `Gap.tsx` already prints a verdict sentence for it. Silence in the row
    // directly beneath that sentence is the contradiction this exists to
    // prevent.
    expect(healthReadout(alignments, 'raw-mono', 'raw-mono')).toBeUndefined()
    expect(gapHealth(alignments, 'en-fan', 'raw-mono')).toBe('fragmented pair · no counterpart here')
  })
})
```

- [ ] **Step 2: Run the tests to verify they fail**

```sh
cd web && npm test -- readouts
```

Expected: FAIL. The `positionLine` cases fail on the missing prefix (and
`edition` is not a known property, so `npm run typecheck` would fail too); the
`gapHealth` block fails on `gapHealth is not a function`.

- [ ] **Step 3: Write the implementation**

In `web/src/modules/readouts.ts`, extend `PositionInput`:

```ts
/** `positionLine`'s inputs. `slot` and `total` are zero/whole-count as everywhere else internal; `page` is already display-ready (1-based) — the same boundary convention `App.tsx`'s `altAt` already applies before handing a page number to something a person reads. */
export interface PositionInput {
  slot: number
  total: number
  page: number
  /**
   * Whose position this is, prefixed to the line when set.
   *
   * Set by exactly one caller: `App.tsx` while showing a gap, where the line
   * reports the *authority's* slot and the authority's slot count while the
   * reader is looking at a different edition. The percentage below is
   * already allowed to change on a swap — see this function's own doc for
   * why hiding that would be the dishonest option — but changing the *scale*
   * it is measured against with nothing on screen to say so is a different
   * and worse claim, and this is what says so.
   *
   * Absent while reading, which is every other case: the edition is the one
   * on screen, and naming it in every frame would be noise.
   */
  edition?: string
}
```

and the function:

```ts
export function positionLine({ slot, total, page, edition }: PositionInput): string {
  const percent = Math.round(((slot + 1) / total) * 100)
  const line = `slot ${slot + 1} / ${total} · p. ${page} · ${percent}%`
  return edition ? `${edition} ${line}` : line
}
```

Add `gapHealth` immediately after `healthReadout`:

```ts
/**
 * `healthLine` for a gap — the pairing the gap is actually about, and the
 * absence, together.
 *
 * **Deliberately not `healthReadout`, and the difference is the point.** That
 * function reports the `hub`↔`viewing` pair and answers `undefined` while
 * `viewing` is the hub itself; `Gap.tsx` reports the `from`↔`viewing` pair,
 * where `from` is the authority the gap was found at. In a gap those disagree:
 * a reader whose authority is a spoke and who swaps to the hub would see
 * `Gap`'s verdict sentence with silence in the readout row directly beneath
 * it. This reports the same pair `Gap` does, so the view and the row cannot
 * contradict each other.
 *
 * `hasCounterpart` is `false` unconditionally, which is what a gap *is* — and
 * it is why this returns `string` rather than `string | undefined`:
 * `healthLine`'s only silent branch requires a counterpart, so with `false`
 * there is always something to say. That is the intended behaviour, not an
 * accident of the current implementation — silence here would let a reader
 * take a gap for an ordinary page.
 *
 * Unlike `healthReadout` this takes the authority explicitly rather than
 * closing over `hub`, because in a gap the authority is the edition the
 * position is recorded against and the hub may be neither side of the pair.
 */
export function gapHealth(alignments: Alignment[], from: string, viewing: string): string {
  return healthLine(verdictFor(alignments, from, viewing), false)!
}
```

Note the `!`: `healthLine` is declared `string | undefined`, and narrowing it
here on the `false` argument is beyond what TypeScript can do. The doc comment
above states why the assertion holds.

- [ ] **Step 4: Run the tests to verify they pass**

```sh
cd web && npm test -- readouts && npm run typecheck && npm run lint
```

Expected: all `readouts` tests PASS, typecheck clean, lint clean.

- [ ] **Step 5: Commit**

```sh
git add web/src/modules/readouts.ts web/src/modules/readouts.test.ts
git commit -m "readouts: an edition-named position line, and health for a gap's own pair"
```

---

### Task 3: `Filmstrip` centres on an anchor when nothing is current

The strip's scroll-into-view effect centres `currentRef ?? pairedRef`. In a gap
neither is attached — `currentIndex` is `-1` and nothing is `correcting` — so
the rail stays where it opened, at tile 0. `en-fan` is 235 pages and 240 slots,
so a reader in a gap near the middle of a volume would find the strip scrolled
a hundred tiles from where they are.

**This task has no unit test**, because no component tests exist in this repo —
every `*.test.ts` here covers a pure module and there is no DOM harness. Its
gate is `typecheck` and `lint`; the behaviour is confirmed in Task 5's browser
pass. Do not add a test framework for it.

**Files:**
- Modify: `web/src/Filmstrip.tsx`

**Interfaces:**
- Consumes: `NearestOffer.at` from Task 1 (indirectly — `App.tsx` passes it in
  Task 4).
- Produces: `FilmstripProps` gains `anchor?: number`, a slot index in
  `edition`. `ChromeProps extends FilmstripProps`, so `Chrome.tsx` needs no
  edit — it already spreads `...filmstrip` into `<Filmstrip>`.

- [ ] **Step 1: Add the prop**

In `web/src/Filmstrip.tsx`, add to `FilmstripProps`, immediately after
`currentIndex`:

```ts
  /**
   * A slot of `edition` to centre the rail on when nothing is current.
   *
   * Set only while showing a gap, where there is no current tile to centre on
   * — a gap has no slot in the viewed edition at all — and the nearest slot
   * that does resolve is the closest thing to "where the reader is".
   * `App.tsx` passes `nearestPaired`'s `before?.at ?? after?.at`, which is the
   * same page `Gap`'s own `←` button offers.
   *
   * Centring only. The anchor tile gets **no border**: the solid one means
   * "where you are" and the dashed one means "where the alignment says this
   * lands", and in a gap neither is true — it lands nowhere, which is what
   * the gap is. Marking it either way would put a claim on screen in the one
   * state that exists to say the claim cannot be made.
   */
  anchor?: number
```

- [ ] **Step 2: Attach the ref and extend the effect**

Destructure `anchor` in the parameter list, after `currentIndex`:

```ts
export function Filmstrip({
  work,
  edition,
  slots,
  groups,
  currentIndex,
  anchor,
  onNavigate,
  visible,
  correcting,
  onPickCounterpart,
  onPreview,
}: FilmstripProps) {
```

Add the ref beside the other two and extend the chain and the deps:

```ts
  const currentRef = useRef<HTMLButtonElement>(null)
  const pairedRef = useRef<HTMLButtonElement>(null)
  // Third and last in the chain below. At most one of the three is ever
  // attached: `currentIndex` is `-1` whenever `correcting` or `anchor` is
  // set (see `App.tsx`), and `anchor` is only set in a gap, where nothing is
  // correcting. Centring on "whichever is attached" therefore always names
  // the one tile a reader needs on screen for the state they are in.
  const anchorRef = useRef<HTMLButtonElement>(null)
  const railRef = useRef<HTMLDivElement>(null)
  useEffect(() => {
    const recenter = () =>
      (currentRef.current ?? pairedRef.current ?? anchorRef.current)
        ?.scrollIntoView({ inline: 'center', block: 'nearest' })
    recenter()
```

and the dependency array at the end of that effect:

```ts
  }, [currentIndex, visible, correcting?.pairedIndex, anchor])
```

- [ ] **Step 3: Attach it to the right tile**

Inside `groups.map`, add the third case. `isAnchor` is guarded on the other two
being false so the chain's "at most one attached" claim holds even if a future
caller sets `anchor` alongside a real `currentIndex`:

```ts
      {groups.map((group, index) => {
        const isCurrent = index === currentIndex
        const isResolved = correcting?.pairedIndex !== undefined && group.slots.includes(correcting.pairedIndex)
        const isAnchor =
          !isCurrent && !isResolved && anchor !== undefined && group.slots.includes(anchor)
        return (
          <button
            key={group.slots[0]}
            ref={isCurrent ? currentRef : isResolved ? pairedRef : isAnchor ? anchorRef : undefined}
            type="button"
            style={isCurrent ? currentTileStyle : isResolved ? resolvedTileStyle : tileStyle}
```

Note the `style` line is **unchanged** — the anchor deliberately falls through
to plain `tileStyle`.

- [ ] **Step 4: Update the component's own doc comment**

The block comment above `export function Filmstrip` ends with a paragraph
explaining `currentRef` and `pairedRef`. Replace its last sentence — "`pairedRef`
plays the same role for the dashed tile — `currentIndex` never matches anything
while `correcting` is set (see `App.tsx`), so at most one of the two refs is
ever attached at a time, and centring on 'whichever is attached' is exactly the
tile a reader needs on screen to disagree with it." — with:

```
 * `pairedRef` plays the same role for the dashed tile, and `anchorRef` for a
 * gap's neighbourhood — `currentIndex` never matches anything while
 * `correcting` or `anchor` is set (see `App.tsx`), so at most one of the three
 * refs is ever attached at a time, and centring on "whichever is attached" is
 * exactly the tile a reader needs on screen for the state they are in: the one
 * to disagree with while correcting, and the nearest page that exists while
 * standing in a gap.
```

- [ ] **Step 5: Verify and commit**

```sh
cd web && npm run typecheck && npm run lint && npm test
```

Expected: typecheck clean, lint clean, every existing test still PASS (no test
covers this file; the run confirms nothing else broke).

```sh
git add web/src/Filmstrip.tsx
git commit -m "filmstrip: centre on an anchor tile where nothing is current"
```

---

### Task 4: Remove the early return

The defect itself. After this there is exactly one place in `App.tsx` a panel
can be mounted.

**Files:**
- Modify: `web/src/App.tsx`

**Interfaces:**
- Consumes: `NearestOffer.at` (Task 1); `PositionInput.edition` and `gapHealth`
  (Task 2); `FilmstripProps.anchor` (Task 3).
- Produces: nothing later tasks call.

- [ ] **Step 1: Add the import**

`gapHealth` joins the existing import from `./modules/readouts`. Find that
import statement and add `gapHealth` to it, keeping the existing ordering.

- [ ] **Step 2: Narrow the union once**

Immediately after `const shown = display(state, resolver)` (currently
`App.tsx:1020`), add:

```ts
  // The page-drawing half of `Display`, or `undefined` while showing a gap.
  //
  // Narrowed here rather than by a branch that returns early. The early return
  // was O25: it handed back `Gap` *instead of* the reader's own tree, so
  // `Chrome`, `Settings` and the mode toolbar were never rendered while a gap
  // was showing — the keys fired, the state flipped, and nothing appeared. The
  // note panel was mounted inside that branch during M5c as a second copy,
  // which is the shape that let the credits panel be forgotten as well. After
  // this there is one mount site, so a panel added later is reachable in a gap
  // because there is nowhere else to put it.
  const at = shown.kind === 'at' ? shown : undefined
```

- [ ] **Step 3: Delete the gap branch, keeping what it computed**

Delete the whole `if (shown.kind === 'gap') { … }` block (currently
`App.tsx:1293–1359`), including the `<Gap>` element, the second `<NotePanel>`,
`within`, the `nearestPaired` call and the local `toMove`.

Its comment above `allNotes`/`entries` (currently 1276–1280, "Above the gap
branch, deliberately…") describes a branch that no longer exists. Replace those
five comment lines with:

```ts
  // The review list is built whatever `display` resolved to. A gap has no slot
  // to draw notes on, but the work still has notes and a reader in a gap is
  // exactly who asks where they are — found by driving this against the real
  // corpus, where pressing `a` on a gap opened nothing at all. Every row in
  // the list already carries its own "not reachable from here" marking, which
  // is the marking this state exists for.
```

Leave the two lines it introduces (`allNotes`, `entries`) exactly as they are.

- [ ] **Step 4: Give the five values their gap form**

Replace the `currentIndex`/`current` pair (currently 1378–1382):

```ts
  // `-1` in a gap: there is no group of the viewed edition the reader is
  // standing on, and `-1` is already `Filmstrip`'s "mark nothing" — the same
  // value correction mode passes, for the same reason.
  const currentIndex = at
    ? Math.max(grouped.findIndex((g) => g.slots.includes(at.slot)), 0)
    : -1
  const current = at ? grouped[currentIndex] : undefined
```

Replace the `here` line (currently 1444):

```ts
  // Empty in a gap rather than resolved against a slot that does not exist.
  // The note *count* readout still reports the work's total beside it, so a
  // reader sees "no notes here of 7" rather than the readout vanishing.
  const here = at ? notesHere(notesForHere, at.edition, at.slot, resolver) : []
```

Replace the readout block — `chapter`, `positionText`, `chapterText`,
`healthText` and `slotsLeft`, currently `App.tsx:1487–1491` — with:

```ts
  // Where the readouts report from. In a gap that is the authority slot the
  // gap was found at — a real position in the work, which only the *viewed*
  // edition has no page for — and `name` is what puts the edition in front of
  // the position line so the change of scale is never silent. `Gap.tsx` says
  // the same thing in words directly above the row.
  const reportFrom =
    shown.kind === 'at'
      ? { edition: shown.edition, slot: shown.slot, name: undefined }
      : { edition: shown.from.edition, slot: shown.from.slot, name: shown.from.edition }
  // Same `?? slot + 1` fallback the deleted gap branch used for
  // `nearestPaired`'s bound: an edition the resolver cannot count still
  // renders a line rather than `NaN%`.
  const reportTotal = resolver.slotCount(reportFrom.edition) ?? reportFrom.slot + 1
  const chapter: Chapter | undefined = resolver.chapterAt(reportFrom.edition, reportFrom.slot)
  const positionText = positionLine({
    slot: reportFrom.slot,
    total: reportTotal,
    page: resolver.slotAt(reportFrom.edition, reportFrom.slot)!.page + 1,
    edition: reportFrom.name,
  })
  const chapterText = chapterReadout(chapter, alignments, hub)
  // `gapHealth` rather than `healthReadout` in a gap: it reports the pair
  // `Gap.tsx` already prints a verdict sentence for, so the view and the row
  // beneath it cannot contradict each other. See `gapHealth`'s own doc.
  const healthText =
    shown.kind === 'at'
      ? healthReadout(alignments, hub, shown.edition)
      : gapHealth(alignments, shown.from.edition, shown.edition)
  const slotsLeft = Math.max(reportTotal - (reportFrom.slot + 1), 0)
```

Leave `timeText` and `notesText` below it untouched — both already read only
values defined above.

- [ ] **Step 5: Guard the one line that dereferences a slot for a URL**

Replace `pageUrlRef.current = pageAt(shown.slot)` (currently 1510) with:

```ts
  pageUrlRef.current = at ? pageAt(at.slot) : undefined
```

**Do not delete this ref, even though it has no reader.** That is O27, tabled
separately on 2026-08-07, and folding it in would put a second claim on a
branch meant to carry one.

- [ ] **Step 6: Compute the gap's own values above the return**

Immediately before the **component's own** final `return (` — the one at the
end of `App`, currently `App.tsx:1750`, not `renderGroup`'s at 1747 — add:

```ts
  // The gap's offered moves, and where the strip should sit. Computed only in
  // a gap; the scan is bounded by the authority's slot count so a wholly
  // unpaired edition costs one scan rather than one per render.
  const gapMoves =
    shown.kind === 'gap'
      ? nearestPaired(state, resolver, resolver.slotCount(state.authority) ?? state.slot + 1)
      : undefined
  // `nearestPaired` already resolved each offer against the edition being
  // looked at while finding it (see `nearest.ts`'s `NearestOffer`), so this is
  // only the display conversion — not a second `resolveOffered` call repeating
  // a resolution the scan already did. `Gap`'s `page` props are display-only
  // (see their doc there), so the +1 happens right here, at the boundary
  // handing a value to something a person reads — the same convention `altAt`
  // follows.
  const toMove = (offer: NearestOffer | undefined) =>
    offer && { slot: offer.slot, page: offer.page + 1 }
  // The tile the strip centres on while showing a gap: the nearest page that
  // exists, which is the same page `Gap`'s own button offers. `before` first —
  // a reader who has just walked forward into a gap came from behind it.
  const stripAnchor = gapMoves && (gapMoves.before?.at ?? gapMoves.after?.at)
```

- [ ] **Step 7: Substitute `Gap` into the page area**

In the page area's body (currently 1838–1880), make the scroll/single choice a
three-way:

```tsx
        {shown.kind === 'gap' ? (
          <Gap
            edition={shown.edition}
            from={{
              edition: shown.from.edition,
              page: resolver.slotAt(shown.from.edition, shown.from.slot)!.page + 1,
            }}
            verdict={verdictFor(alignments, shown.from.edition, shown.edition)}
            before={toMove(gapMoves?.before)}
            after={toMove(gapMoves?.after)}
            onTake={take}
          />
        ) : mode === 'scroll' ? (
```

with the rest of the existing scroll branch and the `current && renderGroup(current, 0)`
branch unchanged below it. Only the opening of the expression changes — one
`{cond ? (` becomes `{cond ? (…) : cond ? (`, so the closing `)}` and every
paren between stay exactly as they are.

Gate the two page-area overlays on `at`. The single/two-mode `PageAnnotations`
(currently 1803) becomes `{at && mode !== 'scroll' && (`, and inside it
`match={pageAt(shown.slot)}` becomes `match={pageAt(at.slot)}` and
`draft.slot === shown.slot` becomes `draft.slot === at.slot`.

The side `NoteList` (currently 1835) becomes:

```tsx
        {at && annotationsPrefs.reveal && notesPlacement === 'side' && mode !== 'scroll' && (
          <NoteList here={here} placement="side" onOpen={openNote} />
        )}
```

and the below-placement one just after the page area (currently 1882):

```tsx
      {at && annotationsPrefs.reveal && notesPlacement === 'below' && (
        <NoteList here={here} placement="below" onOpen={openNote} />
      )}
```

The page area's own `onPointerDown`/`Move`/`Up`/`Leave` handlers stay attached
and are **not** guarded. `notes.ts`'s `gestureFor` opens with
`if (!press.editing) return { kind: 'ignore' }`, and `editing` is
`panelMode === 'edit'`, which a gap cannot reach — `n` is inert there, since
there is no slot to anchor a note to. A redundant guard would imply the
existing gate is insufficient, and it is not.

- [ ] **Step 8: Pass the anchor to `Chrome`**

In the `<Chrome>` element, add one prop beside `currentIndex`:

```tsx
        currentIndex={pending ? -1 : currentIndex}
        anchor={stripAnchor}
```

`ChromeProps extends FilmstripProps` and `Chrome` spreads `...filmstrip`, so
nothing in `Chrome.tsx` changes.

- [ ] **Step 9: Verify**

```sh
cd web && npm run typecheck && npm run lint && npm test
```

Expected: typecheck clean, lint clean, every test PASS.

If typecheck reports `shown.slot` or `shown.from` on a line not listed above,
that line was missed — narrow it the same way (`at` for the page case, an
explicit `shown.kind === 'at' ? … : …` where both arms are needed).

- [ ] **Step 10: Commit**

```sh
git add web/src/App.tsx
git commit -m "the chrome and every panel are reachable from a gap"
```

---

### Task 5: Confirm it in a browser, then close O25 in `status.md`

No component tests exist in this repo, so the mounts, the anchor and the
toolbar's new visibility are only visible in a real render. `corpus/` is
gitignored and `cargo test` does not rebuild the `serve` binary — build it
explicitly.

**Files:**
- Modify: `docs/status.md`
- Modify: `docs/notes.md`

- [ ] **Step 1: Build and serve against the corpus**

The `taiyaku` binary is `taiyaku-cli`'s, **not** `taiyaku-serve`'s —
`taiyaku-serve` is a library crate and `-p taiyaku-serve` produces no binary at
all:

```sh
cargo build --release -p taiyaku-cli
./target/release/taiyaku serve corpus/gotoubun-v01/taiyaku.json --addr 127.0.0.1:8080
```

The synopsis is `docs/status.md`'s "Eight commands" block, line 100; the other
flags (`--stump-url`, `--stump-key`, `--log-requests`, `--refresh-stump`) are
not needed here. Then `cd web && npm run dev` and open the reader against that
server.

- [ ] **Step 2: Reach the two gaps this pass needs**

Both are read straight out of `corpus/gotoubun-v01/taiyaku.json`, whose hub is
`raw-mono`. A run with no `b` is a gap:

**Gap A — `raw-mono` slot 1, viewed in `en-mono`.** The `raw-mono → en-mono`
alignment's second run is `{"a": "1"}`, no `b`. This is the gap M5c's own
browser pass used — see `notes.md`, "M5c: a note written on the raw, read on
the English". **Its edge's verdict is `sound`**, which makes it the one check
that proves Task 2's whole chain: `healthReadout` is silent on a sound edge and
can never emit this string, so `sound pair · no counterpart here` on screen can
only have come from `gapHealth`.

**Gap B — `raw-mono` slot 193, viewed in `en-fan`.** The `raw-mono → en-fan`
alignment's last run is `{"a": "193-194"}`, no `b`, and the nearest paired run
before it lands at `en-fan` slot **234** of ~240. This is the gap that tests the
anchor: without it the rail sits at tile 0, with it the rail sits at ~234. Gap A
cannot tell those apart — its neighbours are slots 0 and 2, so the rail is near
tile 0 either way.

To reach either: navigate `raw-mono` to the slot, then swap to the named
edition. Slot 1 is a gap in `en-mono` and **not** in `en-fan` (where it pairs to
slot 43), which is itself worth confirming — it shows the gap is a fact about
the pair, not about the slot.

- [ ] **Step 3: Walk the eight checks**

Record the result of each. All eight must hold. Checks 1–2 and 4–8 run at
**Gap A**; check 3 runs at **Gap B**.

1. `Tab` draws the strip. It shows the **viewed** edition's tiles.
2. **No tile has a border** — neither solid nor dashed.
3. *(Gap B)* The rail is **scrolled to ~tile 234**, not to tile 0. The centred
   tile is the page `Gap`'s `←` button names.
4. The readout row carries the **edition prefix**: it begins `raw-mono slot 2 /
   195 · p. …` while the reader is looking at `en-mono`.
5. The health segment reads **`sound pair · no counterpart here`**. See Step 2
   for why this one string proves the most.
6. The page number in the readout row and the page number in `Gap`'s sentence
   are **the same number** — both are `slotAt(authority, slot).page + 1`.
7. `s` draws the settings panel. **This is the keypress that found the item.**
   `a` still draws the review list, and every entry still carries its "not
   reachable from here" marking — the M5c behaviour this must not regress.
8. Clicking a strip tile **leaves the gap**, landing on that page of the viewed
   edition with authority committed there. Arrow keys still do nothing.

If any check fails, stop and fix it before touching the documents. A failed
check is the plan being wrong, not the document being behind.

- [ ] **Step 4: Record the measurement in `notes.md`**

`status.md`'s own preamble promises every claim there traces to a measurement
recorded in `notes.md`. Add a section there — the surrounding sections are the
template — naming what was driven, in which browser, against which volume, and
what each of the seven checks produced. Name anything that came out differently
from what this plan expected, including in the two behaviour changes the design
predicted (the mode toolbar becoming visible in a gap, and the stubbed
`NotePanel` handlers going away).

- [ ] **Step 5: Close O25 in `status.md` — all five parts**

Missing any one makes the document contradict itself. There is no test for
this; re-reading it against itself is the only check.

1. **The table row's checkbox.** `| O25 | [ ] |` becomes `| O25 | [x] |`.
2. **The row's annotation**, only if the item narrowed rather than closed. It
   should close outright — if it did not, say what remains in the row.
3. **The prose bullet**, rewritten to say what was done. Its current
   "**What would close it:** deciding what the filmstrip shows in a gap, and
   landing that together with the one-line `Settings` fix" is the line to
   replace. Closed items' prose is written in the present tense of when it
   closed and left as written afterwards.
4. **The test counts, which appear twice.** Only the `web/` figure moves — this
   branch adds no Rust tests, so "Five crates, 410 tests" and "Twelve of the
   410 are standing checks" both stand. Read the new `web/` count off the
   actual run rather than adding up by hand:

   ```sh
   cd web && npm test 2>&1 | tail -5
   ```

   and update "plus 390 in `web`'s TypeScript" to match.
5. **The counts of what is closed and what remains.** `Eighteen are now closed
   … leaving nine` becomes `Nineteen are now closed — … and O25 — leaving
   eight`, with `O25` added to that list. Then the "What remains" sentence near
   the bottom: `the nine open items tabled above` becomes `the eight open items
   tabled above`, and the clause after it — which says how many came from a
   whole-branch review and names O23, O25 and O27 as the exceptions — has to
   drop O25 and stay arithmetically true.

- [ ] **Step 6: Update the two places that describe O25 as a blocker**

Both are prose elsewhere in `status.md` and neither is part of the five-part
ritual, so they are easy to miss:

- The `web/` row of the "What is built" table says the credits panel "waits on
  O25 and says so in its own doc comment".
- §4's heading, "Skipping scanlation credit pages — shipped to the sidecar, and
  the reader's half waits on O25", and its closing paragraph, "mounting it waits
  on O25, since every panel below `App.tsx`'s `shown.kind === 'gap'` branch goes
  unrendered while a gap is showing".

Both become "the mount site O25 was blocking now exists" — the credits panel is
still unmounted, but no longer blocked. `web/src/modules/credits.ts`'s own doc
comment says the same thing and should be corrected with them.

- [ ] **Step 7: Re-run the consistency sweep**

```sh
cd /home/davey/projects/taiyaku
echo "rows: $(grep -c '^| O[0-9]' docs/status.md)  closed: $(grep -c '^| O[0-9]* | \[x\]' docs/status.md)"
grep -n "leaving eight\|Nineteen are now closed" docs/status.md
grep -n -A2 'What remains:' docs/status.md
grep -n "waits on O25" docs/status.md
```

Expected: `rows: 27  closed: 19`; the count words agree with 27 − 19 = 8; the
"What remains" sentence (whose count word wraps across a line break, which is
why this greps the opener and reads the next line) says eight; and `waits on
O25` returns nothing.

- [ ] **Step 8: Commit**

```sh
git add docs/status.md docs/notes.md web/src/modules/credits.ts
git commit -m "docs: O25 closes — the chrome is reachable from a gap"
```

- [ ] **Step 9: Open the pull request**

The remote is Gitea, not GitHub, and `gh` is authenticated against the wrong
host — use the `gitea` MCP tools. The remote is squash-only, so this branch
becomes exactly one commit on `main`.

---

## What this plan does not do

- **Mount the credits panel.** That is §4's remaining half. This unblocks it:
  once there is one mount site, the panel is an ordinary addition to it.
  `creditSlots` in the same file is a separate problem its own doc already
  states — it re-derives slot expansion in TypeScript where
  `taiyaku_core::slot::expand` owns it.
- **Delete `pageUrlRef`.** O27, tabled 2026-08-07. Task 4 Step 5 keeps it
  compiling and nothing more.
- **Make arrows or regroup work in a gap.** Both are deliberately inert;
  `session.ts`'s `applyIntent` carries the reasoning.
- **Resolve `healthReadout`'s hub-vs-authority question for the reading case.**
  Only the gap's own line changes.
