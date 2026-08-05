import type { Position } from './resolver'

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
 */
export function groups(
  slots: Position[],
  mode: Mode,
  _reading: 'rtl' | 'ltr',
  breaks: number[] = [],
): Group[] {
  const paired = mode === 'two'

  const out: Group[] = []
  let index = 0
  while (index < slots.length) {
    const here = slots[index]
    const next = slots[index + 1]
    const joined = here.half !== undefined && next?.half !== undefined && next.page === here.page
    if (joined) {
      out.push({ slots: [index, index + 1], spread: true })
      index += 2
      continue
    }
    if (!paired) {
      out.push({ slots: [index] })
      index += 1
      continue
    }
    // A spread starting at the *second* position of a would-be pair must not
    // be split across two groups, so the pair stops short and the spread
    // opens the next group. A hand-inserted break at that same position does
    // the same thing: the slot at `index + 1` must open a fresh group rather
    // than close out this one.
    const nextIsSpreadStart =
      next?.half !== undefined && slots[index + 2]?.page === next.page
    const nextIsBreak = next !== undefined && breaks.includes(index + 1)
    if (next === undefined || nextIsSpreadStart || nextIsBreak) {
      out.push({ slots: [index] })
      index += 1
      continue
    }
    out.push({ slots: [index, index + 1] })
    index += 2
  }
  return out
}

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
