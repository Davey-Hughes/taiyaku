import type { Group } from './pairing'
import type { Position } from './resolver'

/** One tile's worth of pages, in reading order within the tile. */
export interface Tile {
  /** The slot each entry navigates to, in `groups`' order. */
  slot: number
  page: number
}

/**
 * The pages a tile shows for one `groups()` entry.
 *
 * A joined spread is two slots but **one page** — `pairing.ts`'s `groups`
 * already guarantees `here.page === next.page` for a `spread: true` group,
 * so rendering one `<img>` per slot would fetch the same thumbnail twice.
 * This collapses it to the single entry the tile actually shows, and is
 * exactly the seam a slot/page mixup would show up on: `slots` is a
 * *slot*-indexed table, `page` the number `thumbUrl` needs, and the two
 * only coincide up to the edition's first joined spread.
 *
 * A non-spread pair (`two` mode) keeps both slots — each is its own page —
 * and a singleton group is the one-slot case either way.
 *
 * Kept in its own module, separate from `Filmstrip.tsx`'s component, rather
 * than exported alongside it: a pure function sharing a file with a
 * component is exactly what defeats React Fast Refresh (oxlint's
 * `react/only-export-components` flags this), and this codebase already
 * keeps pure logic and components apart everywhere else (`pairing.ts`,
 * `reading.ts`, `nearest.ts` beside `App.tsx`, `Chrome.tsx`, `Gap.tsx`).
 * `tilePages` is also directly unit-tested (`tiles.test.ts`), which a plain
 * module suits better than a component file.
 */
export function tilePages(group: Group, slots: Position[]): Tile[] {
  if (group.spread || group.slots.length === 1) {
    const [slot] = group.slots
    return [{ slot, page: slots[slot].page }]
  }
  return group.slots.map((slot) => ({ slot, page: slots[slot].page }))
}
