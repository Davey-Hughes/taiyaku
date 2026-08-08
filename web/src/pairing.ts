import type { Position } from './resolver'
import type { Cell } from './viewport'

export type Mode = 'single' | 'two' | 'scroll'

/** One thing drawn at once: one slot, a pair of slots, or a joined spread. */
export interface Group {
  slots: number[]
  spread?: true
}

/**
 * Which slots are drawn together.
 *
 * **A joined spread is one group in every mode** — its two slots are one wide
 * image, so drawing them as two groups would show the same page twice in
 * `single` and `scroll`, and slice a wide image across a pair's two halves in
 * `two`. That much does not vary by mode.
 *
 * What does vary by mode is how many *non-spread* slots share a group.
 * `single` and `scroll` put one slot per group. `two` pairs `(0,1), (2,3), …`,
 * and **a joined spread interrupts that pairing**: it forms its own group and
 * pairing restarts after it. Without that, every page after the first spread
 * pairs one slot out of phase, which is the parity break the `regroup` module
 * lets a reader fix by hand in M5b — this handles only the case the format
 * already states, where `spreads` makes the break derivable.
 *
 * Reading direction does not appear in the grouping. It decides which slot of
 * a pair is drawn on the right, which is the renderer's business, and the slot
 * sequence itself already accounts for direction — `Edition::slots` orders a
 * spread's halves by it.
 *
 * `breaks` is the `regroup` module's per-edition, client-side list of slots a
 * reader has marked to restart pairing at — the case a sidecar's `spreads`
 * cannot state, because it is one missing page throwing off parity rather
 * than a fact about the work. Honoured only in `two` mode, where parity
 * exists to break: `single` and `scroll` already put one slot per group, so a
 * break has nothing to do there. The spread branch above still wins even
 * when a break falls on one of a spread's own slots — a spread is one image
 * and cannot be halved, so that break is absorbed rather than applied, and
 * pairing simply resumes where the spread already forces it to.
 *
 * `skipped` is the `credits` module's set of slots a reader has asked to keep
 * out of the reading flow. **It is honoured here rather than by filtering the
 * result**, and the difference is the whole of whether `skip` works in `two`
 * mode. A filter over finished groups can only drop a group whose every slot
 * is skipped, and on the primary corpus that is none of them: each of
 * `en-fan`'s four credit pages sits at a chapter's end and so pairs with an
 * ordinary page, measured at 122 tiles with `skip` on and 122 with it off.
 * Deciding *while pairing* lets the partner search step past a skipped slot,
 * so its neighbour pairs forward instead of being stranded beside a page the
 * reader asked to set aside.
 *
 * **Slot numbers are untouched either way.** `Group.slots` holds indices into
 * the unfiltered `slots` array, so skipping changes which slots are drawn and
 * never what any of them is called — `docs/status.md` §4's "skipping never
 * renumbers", and the reason the position readout still divides by the whole
 * edition. Empty by default, so every caller that does not skip is unaffected
 * and pays nothing.
 */
export function groups(
  slots: Position[],
  mode: Mode,
  _reading: 'rtl' | 'ltr',
  breaks: number[] = [],
  skipped: Set<number> = NOTHING_SKIPPED,
): Group[] {
  const paired = mode === 'two'

  const out: Group[] = []
  let index = 0
  while (index < slots.length) {
    const here = slots[index]
    const next = slots[index + 1]
    const joined =
      here.half !== undefined &&
      next?.half !== undefined &&
      next.page === here.page
    if (joined) {
      // A spread is one image, so it is set aside only when both halves are.
      // `creditSlotsIn` filters by page and both halves carry the same one, so
      // in practice the two always agree; the test is written out anyway
      // rather than assuming that, since this function is given a slot set and
      // has no way to check where it came from.
      if (!(skipped.has(index) && skipped.has(index + 1))) {
        out.push({ slots: [index, index + 1], spread: true })
      }
      index += 2
      continue
    }
    if (skipped.has(index)) {
      // Emit nothing and carry on. In `single` and `scroll` that is the whole
      // of skipping; in `two` it is half, and the partner search below is the
      // other half.
      index += 1
      continue
    }
    if (!paired) {
      out.push({ slots: [index] })
      index += 1
      continue
    }
    // **The partner is the next slot that is not set aside**, not simply
    // `index + 1`. Pairing around a skipped slot rather than over it is what
    // makes `skip` mean anything in `two` mode at all: measured on the primary
    // corpus, every one of `en-fan`'s four credit pages sits at a chapter's
    // end and so pairs with an ordinary page, so a rule that only dropped
    // wholly-skipped pairs dropped none of them — 122 tiles with `skip` on and
    // 122 with it off. Skipping past the slot re-pairs its neighbour forward
    // instead, which is what a reader who asked for those pages to be set
    // aside meant.
    let partner = index + 1
    while (partner < slots.length && skipped.has(partner)) partner += 1
    const at = slots[partner]
    // A spread starting at the *second* position of a would-be pair must not
    // be split across two groups, so the pair stops short and the spread
    // opens the next group. A hand-inserted break at that same position does
    // the same thing: the slot at `partner` must open a fresh group rather
    // than close out this one. Both tests are against `partner` rather than
    // `index + 1`, so a skipped slot in between does not hide a spread or
    // swallow a break.
    const nextIsSpreadStart =
      at?.half !== undefined && slots[partner + 1]?.page === at.page
    // The partner search can also land *inside* a spread, on its second half,
    // when the first half is skipped and the second is not. Pairing with it
    // would put half a wide image in an ordinary cell and drop the other half
    // — and `single` mode keeps that spread whole, so the two modes would
    // disagree about a case the spread branch above deliberately specifies.
    // Stopping short instead lets `index` reach the spread's own first half on
    // the next turn, where that branch handles it.
    const partnerIsSpreadTail =
      at?.half !== undefined && slots[partner - 1]?.page === at.page
    // Any break *in* the skipped run counts, not only one exactly at
    // `partner`. A break is a slot a reader marked by hand to restart pairing
    // at; a skipped slot in between must not swallow it, or switching `skip`
    // on would silently change the parity for the rest of the volume of a
    // reader who had fixed it themselves.
    const nextIsBreak =
      at !== undefined && breaks.some((b) => b > index && b <= partner)
    if (
      at === undefined ||
      nextIsSpreadStart ||
      partnerIsSpreadTail ||
      nextIsBreak
    ) {
      out.push({ slots: [index] })
      index += 1
      continue
    }
    out.push({ slots: [index, partner] })
    // Past the partner, not past `index + 1`: everything between them is
    // skipped and has already had its chance to be emitted.
    index = partner + 1
  }
  return out
}

/** One empty set, so the default argument does not allocate per call. */
const NOTHING_SKIPPED: Set<number> = new Set()

/**
 * One group per slot, always — ignoring `mode`, spreads, and breaks alike.
 *
 * `groups` above is deliberately mode- and format-aware: a joined spread is
 * always a two-slot group in every mode, and `two` mode pairs ordinary slots
 * two to a tile. Both are right for *reading* and wrong for *correcting* — a
 * reader picking a counterpart has to be able to name any one slot,
 * including one half of a joined spread or one slot of a two-page pair, and
 * a tile standing for more than one slot has no way to say which was meant.
 * The M5d review's I4 found this concretely: the filmstrip could only ever
 * assert `group.slots[0]`, so half of every spread — and, in `two` mode,
 * half of every ordinary pair — was simply unaddressable, and worse, if the
 * alignment already resolved to the *other* half, the spread's one tile was
 * the one marked "this is what it resolves to," so clicking it (the obvious
 * "yes, this one" gesture) silently asserted the wrong half.
 *
 * `App.tsx` uses this in place of `groups` for exactly the filmstrip's
 * `groups` prop while `correcting` is set, and only there — the page area
 * itself keeps rendering through `groups` above regardless, since a
 * correction never changes what is on screen until it is confirmed. One
 * tile per slot is also the simpler thing to explain to a reader: "while
 * correcting, every tile is one page" needs no caveat about spreads or the
 * current mode, where "click the half of this tile you mean" would.
 */
export function correctionTiles(slots: Position[]): Group[] {
  return slots.map((_, index) => ({ slots: [index] }))
}

/** Which two fixed screen positions a two-page group's slots render into — see `cellsFor`. */
export interface Cells {
  /** Whether `first` should render full width with `second` collapsed (a joined spread or a solo slot with no pairing partner) rather than the two rendering at equal, half width (an ordinary pair). */
  wide: boolean
  first: number
  second: number
}

/**
 * Two fixed positions, `first` and `second`, for `App.tsx`'s two-page layout
 * to render `group`'s slots into — always both, regardless of whether
 * `group` is a pair, a joined spread, or a solo slot with nothing to pair
 * with.
 *
 * This exists so a turn that changes group *shape* — a spread interrupting
 * or resuming ordinary pairing, a hand-inserted break doing the same — never
 * changes the *element type* React sees at either position. `App.tsx` used
 * to render a pair as a `<div>` wrapping two `<PageView>`s but a spread or
 * solo group as a single `<PageView>` directly: the position's element type
 * flipped between `div` and `PageView` on exactly the turns that mattered,
 * which forces React to unmount the outgoing subtree and mount a fresh one
 * rather than update props on what was already there. A freshly-mounted
 * `PageView` has nothing decoded yet — `decode.ts`'s whole gate only ever
 * applies to a `src` prop changing on an instance that survives, see
 * `presentWhenDecoded`'s doc — so that remount was the actual mechanism
 * behind a blank frame flashing on a two-page turn onto or off a spread,
 * the defect `prefetch`/`decode.ts` exist to remove. Two fixed positions,
 * always present, is what lets `App.tsx` render the *same* wrapping
 * `<div>` and the *same* two child `<PageView>`s across the change, so
 * React reconciles by position and only updates `src` props — exactly the
 * case the decode gate already handles.
 *
 * `second` mirrors `first` (`group.slots[1] ?? group.slots[0]`) whenever
 * `group` has no real second slot — every `wide` group, spread or solo
 * alike. A spread's own two slots already share one URL (`groups`' own
 * `joined` check requires `next.page === here.page`), so for a spread this
 * is simply its real second half; a solo group has no second slot at all,
 * so mirroring the first is a harmless, always-valid filler for a position
 * `App.tsx` renders collapsed and hidden regardless of what it holds. The
 * point either way is that the second cell's `PageView` always has a valid
 * `src`, so it stays mounted with something already decoded through every
 * `wide` group, ready to swap the moment pairing resumes and it becomes the
 * visible second half of a real pair — a plain `src` prop change on an
 * instance that never unmounted, not a fresh mount with nothing decoded.
 *
 * `reading` only reorders which physical slot lands in `first` vs. `second`
 * for an ordinary pair; a `wide` group has no left/right to reorder — its
 * one visible image is `first` regardless of reading direction, the same as
 * `App.tsx`'s original spread rendering did.
 */
export function cellsFor(group: Group, reading: 'rtl' | 'ltr'): Cells {
  const wide = group.spread === true || group.slots.length === 1
  const [a, b = a] = group.slots
  if (wide) return { wide: true, first: a, second: b }
  const [first, second] = reading === 'rtl' ? [b, a] : [a, b]
  return { wide: false, first, second }
}

/**
 * Which of `two` mode's two drawing positions holds `slot` — `viewport.ts`'s
 * `Cell`, for the group `cellsFor` has just laid out.
 *
 * `cellsFor`'s answer restated in the vocabulary the geometry uses, rather
 * than a second rule: `wide` is `'only'` because that is precisely how
 * `App.tsx` draws it — the second cell is collapsed to zero width, so the
 * first fills the group — and an ordinary pair is `'first'` or `'second'` by
 * which of `cellsFor`'s two positions the slot landed in, already
 * reading-direction-corrected. Doing that comparison at the call site would
 * put a `reading === 'rtl'` test back into `App.tsx`, which is the thing
 * `cellsFor` exists to keep out of it.
 *
 * `'second'` is the answer for a slot this group does not hold at all, which
 * no caller can reach: the only caller finds its group by
 * `slots.includes(slot)`, and a non-`wide` group holds exactly `first` and
 * `second`. Stated rather than thrown on, because a wrong cell is a swap that
 * lands half a page out, not a crash.
 *
 * `single` and `scroll` mode never call this. One image fills the group in
 * both, which is `'only'` without asking — see `Cell`'s own doc.
 */
export function cellOf(
  group: Group,
  reading: 'rtl' | 'ltr',
  slot: number,
): Cell {
  const cells = cellsFor(group, reading)
  if (cells.wide) return 'only'
  return cells.first === slot ? 'first' : 'second'
}

/**
 * The slot one arrow press away, stepping by what is *drawn* rather than by
 * slot.
 *
 * A group is one image on screen — that is the whole point of `groups` above
 * — so a reader pressing an arrow expects the next group, not the next slot.
 * Where the two differ, stepping by slot redraws what is already there and
 * the press appears to do nothing: a joined spread is two slots and one wide
 * image, so crossing one took two presses, and in `two` mode a press advanced
 * the pair by one, redrawing half of what was on screen. Reported from a
 * manual pass as "I have to right arrow twice to go by it".
 *
 * **It lands on the group's first slot, and that fixes a second thing.** Both
 * halves of a joined spread draw the same image but resolve *differently* —
 * `route.rs`'s `resolve_slot` exists for exactly that, and on the primary
 * corpus one half of `en-mono`'s page 4 pairs with raw 5 while the other
 * pairs with nothing. So a reader who could not see which half they were on
 * got one of two different answers on a swap, from a screen that looked
 * identical either way. Always standing on the group's first slot makes the
 * swap predictable from what is visible.
 *
 * **What it costs:** the second half of a joined spread is no longer
 * reachable with an arrow key, so neither is its own distinct swap answer.
 * That half is still addressable where addressing it is the point — the
 * correction filmstrip puts one tile per slot (`correctionTiles`), and `two`
 * mode draws the halves in separate cells. What is given up is reaching it by
 * arrow in a mode that cannot show you which one you are on.
 *
 * **An arrow with nowhere to go leaves the slot alone**, at both ends of the
 * volume. Where `here + by` falls outside the group range the slot comes back
 * unchanged. The older clamp — `Math.min(Math.max(here + by, 0), all.length -
 * 1)` — kept the reader in the group they were already in, and the first-slot
 * landing above then moved them to its start; standing on that group's
 * *second* slot, that is backward on a forward press, which was O28. Not a
 * `two`-mode quirk: measured with nothing skipped, `two` `[[0,1],[2,3],[4,5]]`
 * answered 4 going forward from slot 5, and any volume ending in a joined
 * spread did the same in `single` and `scroll`.
 *
 * **The first-slot rule governs where an arrow lands when it moves, not what
 * it does when it cannot**, and that is the distinction the clamp lost. None
 * of these presses changed the picture — a group is one image and both slots
 * were inside it — so what they silently changed was which slot a swap, a note
 * or a written position answers from.
 *
 * **The start of the volume held the mirror of it**, which O28's own entry got
 * wrong: it recorded that backward at the start "already returns the slot
 * unchanged", and that is true only from a group's *first* slot. Measured from
 * the second half of a leading joined spread, backward answered 0 — the same
 * invisible move, in the same direction the reader had pressed, which is why
 * it read as correct. One rule now covers both ends rather than two rules
 * meeting in the middle, which is how this was missed in the first place.
 *
 * An arrow still cannot *put* a reader on a second slot — that is the cost
 * named above — but a swap, a confirmed correction or a restored progress
 * record can, and an arrow now leaves them there rather than moving them off
 * it in whichever direction the nearest boundary happens to face.
 *
 * A slot no group claims goes to the nearest group in the direction of travel
 * (`nearestGroup` below) rather than staying put. **That branch is a backstop,
 * not a live path**: `modules/credits.ts`'s `skippedApartFrom` exempts the
 * page the reader is standing on from being skipped, so `groups` always emits
 * a group containing it and `findIndex` cannot answer `-1` for any position
 * the reader can hold. It is kept because the invariant lives in another
 * module and a caller that passed a skipped set without the exemption would
 * otherwise strand a reader with no way out but the panel that put them there.
 * *An earlier version of this paragraph said the opposite — that such a slot
 * stayed put — and then, when `skipped` arrived, that the state was ordinary.
 * Neither is true now.*
 */
export function stepToGroup(all: Group[], slot: number, by: 1 | -1): number {
  const here = all.findIndex((group) => group.slots.includes(slot))
  if (here === -1) return nearestGroup(all, slot, by)
  const next = here + by
  if (next < 0 || next >= all.length) return slot
  return all[next].slots[0]
}

/**
 * Where an arrow goes from a slot no group holds.
 *
 * **Unreachable as the reader is wired today, and deliberately kept.**
 * `modules/credits.ts`'s `skippedApartFrom` takes the page the reader stands
 * on out of the skipped set, so every position they can hold is in some group
 * and `stepToGroup`'s `findIndex` never answers `-1`. The three routes onto a
 * skipped page — taking one from the credits panel, clicking its filmstrip
 * tile before switching `skip` on, switching `skip` on where you already
 * were — are all closed by that exemption. This exists because the invariant
 * that closes them lives in a *different module*: a caller that built a
 * skipped set without it, or a future simplification of `skippedApartFrom`,
 * would strand a reader on a page nothing draws, and a silent backstop is
 * cheaper than that. `pairing.test.ts` pins its behaviour directly for the
 * same reason.
 *
 * **It needs no knowledge of which slots are skipped**, which is why it can
 * live here at all: `all` is already the grouping, so "the nearest group in
 * the direction of travel" is answerable from it alone, and this stays a pure
 * function of the grouping.
 *
 * Returns `slot` unchanged when nothing survives ahead — the same boundary
 * answer the reader gives at the first and last slot.
 */
function nearestGroup(all: Group[], slot: number, by: 1 | -1): number {
  // **Tested on the group's whole span, not on its first or last slot alone.**
  // A group used to be a contiguous run, so either end stood for all of it.
  // The `skipped` partner search broke that: `[2, 4]` is one tile with slot 3
  // set aside between its halves, and it *straddles* the slot a reader on 3 is
  // standing at. Asking only whether its first slot is past 3 (forward) or its
  // last is before 3 (backward) says no both ways, and the arrow passes over
  // the one tile adjacent in either direction — measured as `←` giving 0 and
  // `→` giving 5 where 2 was right.
  //
  // With slots 1 and 3 both set aside (`[[0, 2], [4, 5]]`, reader on 1) the
  // two directions failed differently, and only one was stuck: `←` returned 1
  // and did nothing, while `→` returned 4 — it moved, but over slot 2, which
  // *is* drawn, as half of the tile `[0, 2]`. The span test changes `→` to 0,
  // which is that same tile. So this is not only an unsticking: it reverses
  // the forward answer, and the case for it is that landing on the tile
  // holding slot 2 shows content that jumping to 4 skipped past.
  if (by === 1) {
    const ahead = all.find(
      (group) => group.slots[group.slots.length - 1] > slot,
    )
    return ahead ? ahead.slots[0] : slot
  }
  // Groups are in ascending order, so the last one *beginning* below `slot` is
  // the nearest behind it — including one that straddles it.
  let behind: Group | undefined
  for (const group of all) {
    if (group.slots[0] < slot) behind = group
    else break
  }
  return behind ? behind.slots[0] : slot
}
