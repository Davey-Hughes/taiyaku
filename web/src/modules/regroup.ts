import type { ReaderModule } from '../module'
import type { Group, Mode } from '../pairing'

export interface RegroupPrefs {
  /**
   * Break positions per volume (its slug), then per edition key: a break
   * fixes one volume's parity, and another volume's edition of the same key
   * has its own.
   */
  breaks: Record<string, Record<string, number[]>>
}

/**
 * Insert or remove a parity break at a slot.
 *
 * Kept sorted so the same set of breaks always produces the same layout,
 * whatever order a reader inserted them in. A break at slot 0 is refused
 * because pairing already restarts there — accepting it would store a
 * preference that cannot change anything.
 */
export function toggleBreak(breaks: number[], slot: number): number[] {
  if (slot <= 0) return breaks
  return breaks.includes(slot)
    ? breaks.filter((b) => b !== slot)
    : [...breaks, slot].sort((a, b) => a - b)
}

/**
 * The reader's own breaks after `b` on `slot`, or `undefined` when no pair
 * would change: a break stored inert wakes to split a pair the reader never
 * chose once pairing before it shifts. `layout` is the viewed edition's
 * two-page grouping for a list of own breaks.
 *
 * In `two` mode a pair breaks at its second slot, and a lone page at the slot
 * that opens the next group (not `slot + 1`, which can be a skipped page), so
 * a second press undoes the first. Elsewhere the target is `slot`.
 */
export function realign(
  layout: (breaks: number[]) => Group[],
  own: number[],
  slot: number,
  mode: Mode,
): number[] | undefined {
  const before = layout(own)
  const target = mode === 'two' ? pairFrom(before, slot) : slot
  if (target === undefined) return undefined
  const after = toggleBreak(own, target)
  return sameGroups(before, layout(after)) ? undefined : after
}

/** Where `b` breaks in `two` mode: see `realign`. */
function pairFrom(all: Group[], slot: number): number | undefined {
  const here = all.findIndex((group) => group.slots.includes(slot))
  if (here === -1) return undefined
  const { slots, spread } = all[here]
  if (slots.length === 2 && !spread) return slots[1]
  return all[here + 1]?.slots[0]
}

/**
 * The first slot of the two-page pair `slot` is the second slot of, or
 * `undefined` where `slot` opens its group or its group is no pair. `b`
 * puts the reader there: its break stands that page alone, so the reader
 * stays on the pair it was looking at, and a second press undoes the first.
 */
export function pairStartBefore(
  all: Group[],
  slot: number,
): number | undefined {
  const group = all.find((one) => one.slots.includes(slot))
  if (group === undefined || group.slots.length !== 2 || group.spread)
    return undefined
  return group.slots[0] === slot ? undefined : group.slots[0]
}

function sameGroups(a: Group[], b: Group[]): boolean {
  return (
    a.length === b.length &&
    a.every(
      (group, i) =>
        group.spread === b[i].spread &&
        group.slots.length === b[i].slots.length &&
        group.slots.every((slot, j) => slot === b[i].slots[j]),
    )
  )
}

/** `regroupModule`'s own declared intent string — read here by `App.tsx` rather than duplicated as a literal. */
export const TOGGLE_INTENT = 'regroup:toggle'

export const regroupModule: ReaderModule<RegroupPrefs> = {
  id: 'regroup',
  label: 'Regroup pages',
  defaults: { breaks: {} },
  keys: [
    {
      key: 'b',
      intent: TOGGLE_INTENT,
      label:
        'two pages: stand this page alone and pair from the next (again to undo)',
    },
  ],
}

/** One volume's edition's own breaks; none when nothing is stored there. */
export function breaksFor(
  prefs: RegroupPrefs,
  work: string,
  edition: string,
): number[] {
  return prefs.breaks[work]?.[edition] ?? []
}

/**
 * One volume's edition's break list replaced, every other list left alone.
 *
 * Mirrors `direction.ts`'s `setOverride`: the same merge shape, where a
 * forgotten spread would silently wipe every other volume's or edition's
 * saved breaks.
 */
export function withBreaks(
  prefs: RegroupPrefs,
  work: string,
  edition: string,
  breaks: number[],
): RegroupPrefs {
  return {
    ...prefs,
    breaks: {
      ...prefs.breaks,
      [work]: { ...prefs.breaks[work], [edition]: breaks },
    },
  }
}
