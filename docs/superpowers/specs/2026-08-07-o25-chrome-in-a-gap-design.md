# O25 — the chrome, and every panel, reachable from a gap

**Status: built and driven, 2026-08-07.** Every section below shipped as
designed, with one correction made in place — the worked example in §3 was
labelled "from the corpus" and was invented; see the block quote there. The
browser pass that verified it is in [`../../notes.md`](../../notes.md), "O25:
the chrome in a gap, driven against the corpus"; O25's own entry in
[`../../status.md`](../../status.md) is closed. Settled in brainstorming on
2026-08-07. This
closes O25 of [`../../status.md`](../../status.md)'s open-items table — "The
settings panel and the whole chrome are unreachable while showing a gap" —
turned up on 2026-08-05 by driving M5c's annotation work against the corpus.

O25 was left open deliberately rather than half-fixed. Its own entry says why:
the note panel was mounted in the gap branch during M5c because a reader in a
gap is precisely who asks where their notes are, and the other two were not,
because the settings panel is a one-line fix and the filmstrip is not — "a gap
has no slot in the viewed edition, so what the strip should show there is a
design question". This document settles that question, and two more the same
branch turns up.

**The design question is smaller than the item feared, and the reason is worth
recording**: `App.tsx`'s `goToSlot` is `reading.ts`'s `navigate(state, slot)`
against `state.viewing`, which is exactly the commit `session.ts`'s
`takeOffered` already makes for `Gap`'s two offered moves. A filmstrip click in
a gap therefore needs no new semantics at all — it is the same move `Gap`'s
buttons make, from a wider choice of destinations. What is genuinely open is
only what the strip *marks* and where it *scrolls to*, and both are answered
below.

## The defect, stated exactly

`App.tsx`'s render returns early on `shown.kind === 'gap'`, handing back
`<Gap>` plus `<NotePanel>` **instead of** the reader's own tree. Everything
mounted below that branch — `Chrome` (the filmstrip and the readout row),
`Settings`, the mode toolbar, the correction banner — is not rendered. The keys
still fire and the state still flips; nothing appears. Pressing `s` on a gap
opens a settings panel that is never drawn.

It is not a missing feature but a structural hazard, and it has already claimed
a third victim: `web/src/modules/credits.ts` is registered like every other
module, its preferences load and save, and its panel is **not mounted**, with
its own doc comment naming O25 as the reason. §4 of `status.md` says the same.
A fix that leaves two mount sites leaves the hazard.

## What is already written and cannot be reached

Two pieces of the answer exist in the codebase today and are dead from the
reader's side, both because `Chrome` never mounts in a gap.

**`readouts.ts`'s `healthLine` already renders the gap case.** Its second
argument is `hasCounterpart`, and `false` produces `` `${verdict} pair · no
counterpart here` ``. Its only caller, `healthReadout`, passes `true`
unconditionally, and that function's own doc explains: "`App.tsx` only ever
reaches `Chrome` once `display()` has resolved `'at'` rather than `'gap'` — a
`'gap'` is exactly 'no counterpart', and it is handled by `Gap.tsx` before
`Chrome` ever mounts". The string was written for the state O25 makes
unreachable.

**`Filmstrip`'s `currentIndex: -1` already means "mark nothing".** Correction
mode passes it (`App.tsx`, the `Chrome` call's `currentIndex={pending ? -1 :
currentIndex}`) with the comment "No group of `pending.to`'s own is 'current'
while correcting … so nothing should draw the solid 'where you are' border
there." A gap wants the same thing for the same reason.

## 1. Structure — remove the early return

One tree, with `Gap` substituted into the page area. Narrow once:

```ts
const at = shown.kind === 'at' ? shown : undefined
```

`slots`, `effectiveReading` and `grouped` are already derived from
`shown.edition`, which a gap has — only the *slot* is missing, never the
edition — so all three are computed unchanged and are correct in both states.
Five values gain a gap form:

| value | `at` | gap |
| --- | --- | --- |
| `currentIndex` | `Math.max(grouped.findIndex(…), 0)` | `-1` |
| `current` | `grouped[currentIndex]` | `undefined` |
| `here` | `notesHere(…, at.slot, …)` | `[]` |
| `chapter`, `positionText`, `slotsLeft` | viewed edition | authority — §3 |
| the page area's body | `renderGroup` / the scroll column | `<Gap>` |

`PageAnnotations`, the side `NoteList` and the region-drag cursor render only
under `at`. The shape:

```tsx
<div root>
  <toolbar/>
  <div page-area>
    {at ? <…page content…/> : <Gap …/>}
  </div>
  <Chrome …/>
  {pending && <banner/>}
  <NotePanel …/>
  <Settings …/>
</div>
```

**This is chosen over adding two mounts to the gap fragment, and over
extracting a shared tail component, for one reason**: after it there is exactly
one place in this component a panel can be mounted, so a panel added later is
reachable in a gap because there is nowhere else to put it. The other two
options both leave two return statements, which is the shape that let a third
panel be forgotten in the first place.

### What this changes that is not the defect

Three consequences follow from the restructure and are named here rather than
discovered later.

**The mode toolbar becomes visible in a gap.** It was not. The single/two/scroll
buttons take effect on leaving the gap, which is a reasonable thing to set while
standing in one, and O25 is an item about chrome being reachable.

**The gap branch's stubbed `NotePanel` disappears.** It passes `onBody`,
`onColour` and `onDelete` as `() => undefined`; the one real `NotePanel` now
serves both states. That existing branch's comment already carries the argument
for why this is safe — `n` is inert in a gap, there is no slot to anchor to, so
`panelMode` can only be `'review'` by the time a reader gets there — and that
reasoning moves to where the guard now lives rather than being deleted. The
comment's own hedge (passing `draft` through anyway rather than hard-coding
`undefined`, so this cannot drift if the key ever does something else in a gap)
is satisfied by construction once there is one panel instead of two.

**The page area's pointer handlers stay attached over the `Gap` view, and are
inert.** `beginRegion` calls `notes.ts`'s `gestureFor`, whose first line is
`if (!press.editing) return { kind: 'ignore' }`, and `editing` is
`panelMode === 'edit'`, which a gap cannot reach. No new guard is added: a
redundant one would imply the existing gate is insufficient, and it is not.

### What this does not change

**Arrows stay inert in a gap.** `session.ts`'s `applyIntent` returns the state
unchanged on `shown.kind !== 'at'`, with a doc that calls this deliberate:
"there is no slot to add `by` to while showing a gap, so arrows are deliberately
inert there. `Gap`'s offered moves … are the way out of one, not a repurposed
arrow key." The restructure does mean `groupedRef.current` holds the viewed
edition's groups in a gap rather than a stale leftover from the last non-gap
render, but `applyIntent` returns before reading it, so nothing observable
moves.

**No effect's behaviour changes.** Every `useEffect` in `App.tsx` is declared
above the `if (!session) return null` guard and reads `sessionRef` or state
dependencies, never a binding computed after the gap branch — several say so in
their own doc comments, because hooks must run in the same order on every
render. The prefetch effect already re-derives `display()` and returns on
`shownNow.kind !== 'at'`. The scroll-mode centring effect reads
`currentGroupRef`, which is attached to a rendered group and so is simply
unattached in a gap.

## 2. The filmstrip — the viewed edition, marked nowhere, centred on the neighbourhood

**Tiles.** `shown.edition`'s, as always. The reader swapped to this edition and
is looking at it; a strip that switched to the authority's tiles would show one
edition while the reader is in another, which no other state does, and clicking
a tile would then silently undo the swap.

**Marking.** Nothing. `currentIndex: -1`, the convention correction mode already
established. The two neighbours `Gap` offers as buttons are deliberately *not*
given `resolvedTileStyle`'s dashed border: dashed already means "where the
alignment says this lands", and in a gap it lands nowhere. Reusing it would put
a false claim on screen in the one state that exists to say the claim cannot be
made.

**Centring.** On the tile holding the nearest paired landing — the same page
`Gap`'s `←` button already names. Without this the strip opens at tile 0;
`en-fan` is 235 pages and 240 slots, so a reader in a gap near the middle of the
volume would find the strip scrolled some hundred tiles from where they are.

Two small additions carry it:

- **`NearestOffer` gains `at: number`** — the viewed-edition slot that
  `nearestPaired`'s `landingAt` already computes as `landed.slot` and discards.
  This is the same argument that interface's own doc makes about carrying
  `page`: "what lets a caller skip resolving the same slot a second time", with
  `status.md`'s O14 entry holding the measured cost of the redundant resolve.
  No new `Binding::resolve` call is introduced.
- **`FilmstripProps` gains `anchor?: number`** — a viewed-edition slot to centre
  on when nothing is current. A third ref joins the existing chain,
  `currentRef ?? pairedRef ?? anchorRef`, and `anchor` joins that effect's
  dependency array. `Chrome` already spreads `...filmstrip`, so it needs no
  change beyond the type flowing through.

`App.tsx` passes `anchor={before?.at ?? after?.at}` in a gap and nothing
otherwise. At most one of the three refs is ever attached, which the effect's
existing comment already relies on.

## 3. The readouts — authority-anchored, with the edition named

A gap is a real position in the work; it is only the *viewed edition* that has
no slot for it. `Gap.tsx`'s own sentence says exactly this in words — "No
**en-fan** page for **raw-mono** page 87" — so the row reports the same fact in
numbers rather than going quiet.

- **position.** `PositionInput` gains an optional `edition?: string`, prefixed
  when set. Only the gap sets it, and it is what keeps the switch from being
  silent: `positionLine`'s percentage is already allowed to change on a swap
  ("the percentage changing on a swap is the honest answer — hiding it would
  imply the two editions are the same length, which they are not"), but a
  percentage that changed *scale* with no marker would be a different and
  worse claim. The gap reads
  `{ slot: shown.from.slot, total: resolver.slotCount(shown.from.edition) ?? shown.from.slot + 1, page: resolver.slotAt(shown.from.edition, shown.from.slot)!.page + 1, edition: shown.from.edition }`
  — the same `?? slot + 1` fallback the existing gap branch already uses for
  `nearestPaired`'s `within`, so an edition the resolver cannot count still
  renders a line rather than `NaN%`.
- **chapter.** `resolver.chapterAt` against `shown.from.edition` and
  `shown.from.slot`. `chapterAt` resolves through the hub for any edition that
  is not the chapter-organised one anyway, so asking it from the authority is
  not a weaker answer than asking it from the viewed edition would have been.
  `chapterReadout`'s `~` marking is untouched.
- **time.** `slotsLeft` from the authority's slot count. Elapsed was never
  slot-dependent.
- **health.** A new pure `gapHealth(alignments, from, viewing)` beside
  `healthReadout`, returning `healthLine(verdictFor(alignments, from, viewing),
  false)`. **This is the one deliberate departure from `healthReadout`'s hub
  rule, and it exists to stop a contradiction.** `healthReadout` reports the
  hub↔viewed pair and returns `undefined` when `viewing` *is* the hub;
  `Gap.tsx` reports the authority↔viewed pair. In a gap those disagree, and a
  reader whose authority is a spoke and who swaps to the hub would get a verdict
  sentence from the view and silence from the row directly beneath it. The gap's
  health line reports the pair the gap is actually about, which is the pair
  `Gap` already displays.
- **notes.** Falls out of `here = []` with no change: `noteCountLine(0, n)`
  renders `no notes here of 7`. A gap genuinely has no notes on it, and the
  total is counted against the whole work by design.

Worked example. **Corrected 2026-08-07 after this design was built and driven.**
The version below is what the reader actually printed, transcribed from the
browser pass in `notes.md`, "O25: the chrome in a gap, driven against the
corpus":

```
Gap:  No en-fan page for raw-mono page 194.

Row:  raw-mono slot 194 / 195 · p. 194 · 99%
      · fragmented pair · no counterpart here
      · 3m read · no chapter edition
```

Both page numbers are the same value — `slotAt('raw-mono', 193).page + 1` — and
they have to be, since the view and the row describe one position. Slot index
193 renders as `slot 194 / 195` and as `99%` (`round(194 / 195 × 100)`), and
slot index equals page index here because **`raw-mono` carries no spreads at
all** (`"spreads": []` in the sidecar).

> **What this example said before, and why it was wrong.** It read `raw-mono
> slot 89 / 193 · p. 87 · 46%`, labelled "from the corpus", and explained that
> the slot "sits two ahead of the page index because `raw-mono` carries joined
> spreads before it". Every part of that is false for this edition: `raw-mono`
> has 195 slots and zero spreads, so slot and page indices are equal and the
> divergence being explained does not exist; `193` is the `paired` count on the
> `raw-mono → en-mono` edge, not a slot count; and `raw-mono` slot 88 is not a
> gap in `en-fan` at all — run `{"a": "58-131", "b": "96-167"}` pairs it to
> `en-fan` page 126, which is slot 128. (This correction itself first said
> "slot 126", repeating the page-for-slot confusion it was written to fix:
> `en-fan` has spreads at 45 and 91 below that page. The sentence above is only
> true of `raw-mono`, which has none.) The numbers were invented to illustrate a format and then
> labelled as a measurement, which is the one thing this project's documents
> are not allowed to do. Caught by the whole-branch review, which re-derived
> them from the sidecar instead of trusting them.

`readoutRow`'s fixed segment order is unchanged, so nothing reflows on entering
or leaving a gap — a segment only removes or adds itself at its own position.

## Testing

**Unit, in `vitest`, against the pure functions:**

- `positionLine` with and without `edition`, so the prefix is asserted rather
  than assumed.
- `gapHealth` across `sound`, `fragmented` and `refused`, and the `undefined`
  verdict — which must read `verdict unknown`, per `healthLine`'s standing rule
  that silence must never be mistaken for `sound`.
- `nearestPaired` carrying `at`, and `at` addressing the viewed edition rather
  than the authority — the one place these two spaces could be crossed.

**A browser pass against the corpus**, because no component tests exist in this
repo and the anchor, the mounts and the toolbar's new visibility are all things
only a real render shows. `corpus/` is gitignored and `cargo test` does not
rebuild the `serve` binary, so the pass rebuilds it first. What it confirms:

1. Reach a gap — `raw-mono` slot 1 is one M5c's own pass already used.
2. `Tab` draws the strip, showing the viewed edition, with no solid border on
   any tile and the rail scrolled to the neighbourhood rather than to tile 0.
3. The readout row reads as the worked example above, with the edition prefix
   present.
4. `s` draws the settings panel. This is the keypress that found the item.
5. `a` still draws the review list, and every entry still carries its "not
   reachable from here" marking — the M5c behaviour this must not regress.
6. Clicking a strip tile leaves the gap, landing on that page of the viewed
   edition with authority committed there.
7. Arrows still do nothing.

`status.md`'s own rule applies to the write-up: measurements go in
`notes.md`, and the open-items table's five-part close (checkbox, annotation,
prose bullet, test counts, and the "N closed — leaving M" sentences) is the
edit that actually closes O25.

## What this does not do

**It does not mount the credits panel.** That is §4's remaining half, and this
unblocks it rather than delivering it: once there is one mount site, the credits
panel is an ordinary addition to it, and `credits.ts`'s doc comment naming O25
as the blocker can be replaced with the panel itself. `creditSlots` in the same
file is a separate problem its own doc already states — it re-derives slot
expansion in TypeScript where `taiyaku_core::slot::expand` owns it.

**It does not make arrows or regroup work in a gap.** Both are deliberately
inert, with `applyIntent`'s doc carrying the reasoning.

**It does not resolve `healthReadout`'s hub-vs-authority question for the
non-gap case.** Only the gap's own line is changed. Whether a settled reader on
a spoke should hear about the hub pair or the authority pair is a separate
question with no evidence behind it yet, and this document does not invent one.
