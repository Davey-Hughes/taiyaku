import type { Chapter, Position, Resolver } from './resolver'

/**
 * A fake resolver over an explicit slot table.
 *
 * `slots` gives each edition's slot sequence in order; `pairs` maps a
 * `from|to|index` key to the counterpart index, and any key absent from it is
 * a gap. `chapters` maps an `edition|index` key to a `Chapter`, and any key
 * absent from it has none. `credits` maps an edition to its credit pages, and
 * any key absent from it has none — the same non-event `Resolver.credits`
 * gives an unknown edition. Explicit rather than derived, so a test states the
 * alignment it means instead of depending on arithmetic that is already
 * tested in Rust.
 */
export function fakeResolver(
  slots: Record<string, Position[]>,
  pairs: Record<string, number>,
  chapters: Record<string, Chapter> = {},
  credits: Record<string, number[]> = {},
): Resolver {
  const key = (from: string, to: string, index: number) =>
    `${from}|${to}|${index}`
  const chapterKey = (edition: string, index: number) => `${edition}|${index}`
  return {
    editions: () => Object.keys(slots),
    slotCount: (edition) => slots[edition]?.length,
    slotAt: (edition, index) => slots[edition]?.[index],
    indexOf: (edition, at) => {
      const found = slots[edition]?.findIndex(
        (s) => s.page === at.page && s.half === at.half,
      )
      return found === undefined || found < 0 ? undefined : found
    },
    resolve: (from, to, at) => {
      const index = slots[from]?.findIndex(
        (s) => s.page === at.page && s.half === at.half,
      )
      if (index === undefined || index < 0) return undefined
      if (from === to) return { ...at, via: 'identity' }
      const landedIndex = pairs[key(from, to, index)]
      if (landedIndex === undefined) return undefined
      const landed = slots[to]?.[landedIndex]
      return landed && { ...landed, via: 'direct' }
    },
    chapterAt: (edition, index) => chapters[chapterKey(edition, index)],
    credits: (edition) => credits[edition] ?? [],
  }
}

/** Two editions; `b`'s page 2 is a joined spread, so `b` has 9 slots for 8 pages. */
export const whole = (n: number): Position[] =>
  Array.from({ length: n }, (_, page) => ({ page }))

export const bSlots: Position[] = [
  { page: 0 },
  { page: 1 },
  { page: 2, half: 'R' },
  { page: 2, half: 'L' },
  { page: 3 },
  { page: 4 },
  { page: 5 },
  { page: 6 },
  { page: 7 },
]

/** A spread that begins at slot 1, the second half of the first would-be pair. */
export const odd: Position[] = [
  { page: 0 },
  { page: 1, half: 'R' },
  { page: 1, half: 'L' },
  { page: 2 },
]
