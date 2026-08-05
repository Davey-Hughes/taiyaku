import type { Group } from './pairing'
import type { Position } from './resolver'

/** One tile's worth of pages, in reading order within the tile. */
export interface Tile {
  /** The slot each entry navigates to, in `groups`' order. */
  slot: number
  page: number
  /**
   * Which half of a joined spread's shared page this entry is, when it is
   * one. Only ever set for a genuine one-slot group — never for the
   * two-slot `spread: true` group `groups()` itself produces, which stands
   * for *both* halves as a single wide tile and has no one half to name.
   *
   * This is what lets two singleton tiles that share a page — a joined
   * spread's `R` and `L` slots, addressed individually via `pairing.ts`'s
   * `correctionTiles` while correcting — read as anything other than two
   * identical, unlabelled thumbnails: `Filmstrip` folds this into each
   * tile's `alt` text, the same `page N half` convention `App.tsx`'s `altAt`
   * uses for the page view itself.
   */
  half?: 'L' | 'R'
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
  if (group.spread) {
    const [slot] = group.slots
    return [{ slot, page: slots[slot].page }]
  }
  if (group.slots.length === 1) {
    const [slot] = group.slots
    const { page, half } = slots[slot]
    return [{ slot, page, half }]
  }
  return group.slots.map((slot) => ({ slot, page: slots[slot].page }))
}
