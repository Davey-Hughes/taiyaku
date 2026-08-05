import { describe, expect, it } from 'vitest'
import { bSlots, whole } from './fixtures'
import { correctionTiles, groups } from './pairing'
import { tilePages } from './tiles'

describe('tilePages', () => {
  it('is one entry, at the slot itself, for a singleton group', () => {
    const slots = whole(3)
    const [group] = groups(slots, 'single', 'rtl')
    expect(tilePages(group, slots)).toEqual([{ slot: 0, page: 0 }])
  })

  it('is two entries, one per slot, for a non-spread pair', () => {
    const slots = whole(4)
    const [pair] = groups(slots, 'two', 'rtl')
    expect(tilePages(pair, slots)).toEqual([
      { slot: 0, page: 0 },
      { slot: 1, page: 1 },
    ])
  })

  it('is one entry for a joined spread, not two — both its slots share a page', () => {
    // bSlots: 0, 1, 2R, 2L, 3, 4, 5, 6, 7 — slots 2 and 3 are one page (page 2).
    const spread = groups(bSlots, 'single', 'rtl').find((g) => g.spread)!
    expect(tilePages(spread, bSlots)).toEqual([{ slot: 2, page: 2 }])
  })

  it('converts through the slot table past a spread, where slot and page diverge', () => {
    // After bSlots' one spread, slot 4 is page 3, slot 8 is page 7 — a tile
    // that read `slot` back as `page` here would silently point one page
    // early, which is exactly the mistake a filmstrip is easy to make.
    const grouped = groups(bSlots, 'single', 'rtl')
    const lastGroup = grouped[grouped.length - 1]
    expect(tilePages(lastGroup, bSlots)).toEqual([{ slot: 8, page: 7 }])
  })

  // I4 (M5d review): while correcting, `App.tsx` builds the strip from
  // `pairing.ts`'s `correctionTiles` instead of `groups`, so a joined
  // spread's two halves arrive here as two separate one-slot groups rather
  // than one `spread: true` group. Both share a page, so without `half` the
  // two tiles would show the same thumbnail with the same "page N" alt text
  // and a reader would have no way to tell which is which.
  it('carries the half for a genuine singleton slot, so two tiles sharing a page can be told apart', () => {
    // bSlots' slot 2 (R) and slot 3 (L) share page 2.
    const [rTile, lTile] = correctionTiles(bSlots).slice(2, 4)
    expect(tilePages(rTile, bSlots)).toEqual([{ slot: 2, page: 2, half: 'R' }])
    expect(tilePages(lTile, bSlots)).toEqual([{ slot: 3, page: 2, half: 'L' }])
  })

  it('does not carry a half for the two-slot spread group groups() itself produces — it names both halves at once, not one', () => {
    const spread = groups(bSlots, 'single', 'rtl').find((g) => g.spread)!
    expect(tilePages(spread, bSlots)).toEqual([{ slot: 2, page: 2 }])
  })

  it('has no half for an ordinary singleton slot, which was never one', () => {
    expect(tilePages({ slots: [1] }, whole(3))).toEqual([{ slot: 1, page: 1 }])
  })
})
