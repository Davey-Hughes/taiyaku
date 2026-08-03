import type { Position, Resolver } from './resolver'

/**
 * A fake resolver over an explicit slot table.
 *
 * `slots` gives each edition's slot sequence in order; `pairs` maps a
 * `from|to|index` key to the counterpart index, and any key absent from it is
 * a gap. Explicit rather than derived, so a test states the alignment it means
 * instead of depending on run arithmetic that is already tested in Rust.
 */
export function fakeResolver(
  slots: Record<string, Position[]>,
  pairs: Record<string, number>,
): Resolver {
  const key = (from: string, to: string, index: number) => `${from}|${to}|${index}`
  return {
    editions: () => Object.keys(slots),
    slotCount: (edition) => slots[edition]?.length,
    slotAt: (edition, index) => slots[edition]?.[index],
    indexOf: (edition, at) => {
      const found = slots[edition]?.findIndex((s) => s.page === at.page && s.half === at.half)
      return found === undefined || found < 0 ? undefined : found
    },
    resolve: (from, to, at) => {
      const index = slots[from]?.findIndex((s) => s.page === at.page && s.half === at.half)
      if (index === undefined || index < 0) return undefined
      if (from === to) return { ...at, via: 'identity' }
      const landedIndex = pairs[key(from, to, index)]
      if (landedIndex === undefined) return undefined
      const landed = slots[to]?.[landedIndex]
      return landed && { ...landed, via: 'direct' }
    },
  }
}

/** Two editions; `b`'s page 2 is a joined spread, so `b` has 9 slots for 8 pages. */
export const whole = (n: number): Position[] =>
  Array.from({ length: n }, (_, page) => ({ page }))

export const bSlots: Position[] = [
  { page: 0 }, { page: 1 },
  { page: 2, half: 'R' }, { page: 2, half: 'L' },
  { page: 3 }, { page: 4 }, { page: 5 }, { page: 6 }, { page: 7 },
]
