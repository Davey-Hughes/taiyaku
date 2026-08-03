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
 */
export function groups(slots: Position[], mode: Mode, _reading: 'rtl' | 'ltr'): Group[] {
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
    // opens the next group.
    const nextIsSpreadStart =
      next?.half !== undefined && slots[index + 2]?.page === next.page
    if (next === undefined || nextIsSpreadStart) {
      out.push({ slots: [index] })
      index += 1
      continue
    }
    out.push({ slots: [index, index + 1] })
    index += 2
  }
  return out
}
