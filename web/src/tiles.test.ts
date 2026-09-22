import { describe, expect, it } from 'vitest'
import { bSlots, whole } from './fixtures'
import { correctionTiles, groups } from './pairing'
import {
  focusAtFraction,
  medianTileWidth,
  normalizedTileWidth,
  refinedStripUnit,
  spacerWidth,
  stripWindow,
  tileMinWidth,
  tilePages,
  tileUnits,
  unitsBetween,
} from './tiles'

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

describe('tileUnits', () => {
  it('is one for a singleton group', () => {
    const slots = whole(3)
    const [group] = groups(slots, 'single', 'rtl')
    expect(tileUnits(group)).toBe(1)
  })

  it('is two for a non-spread pair — two images side by side', () => {
    const slots = whole(4)
    const [pair] = groups(slots, 'two', 'rtl')
    expect(tileUnits(pair)).toBe(2)
  })

  // The case that makes this function necessary rather than derivable.
  // `tilePages` collapses a joined spread to ONE entry, because its two
  // slots share a page — so a width taken from `tilePages(group).length`
  // would call a spread one unit wide. It is one image, but that image is a
  // double-page scan and draws twice as wide as a page.
  it('is two for a joined spread, though tilePages gives it one entry', () => {
    const spread = groups(bSlots, 'single', 'rtl').find((g) => g.spread)!
    expect(tilePages(spread, bSlots)).toHaveLength(1)
    expect(tileUnits(spread)).toBe(2)
  })

  it('is one for a spread half addressed on its own while correcting', () => {
    const [rTile, lTile] = correctionTiles(bSlots).slice(2, 4)
    expect(tileUnits(rTile)).toBe(1)
    expect(tileUnits(lTile)).toBe(1)
  })
})

describe('tileMinWidth', () => {
  it('is the one-unit floor calc() the runtime measurement matches tiles against', () => {
    expect(tileMinWidth(1)).toBe(
      'calc(var(--strip-unit) * 1 + var(--strip-tile-border) * 2)',
    )
  })

  it('doubles the unit term for a two-unit tile, leaving the border term alone', () => {
    expect(tileMinWidth(2)).toBe(
      'calc(var(--strip-unit) * 2 + var(--strip-tile-border) * 2)',
    )
  })
})

describe('normalizedTileWidth', () => {
  // Identity for a one-unit tile: dividing by 1 after subtracting the
  // border and adding it straight back on again is the same number.
  it('is the identity for a one-unit tile', () => {
    expect(normalizedTileWidth(68.19, 2, 1)).toBeCloseTo(68.19, 5)
  })

  // fix-round review, finding A: a two-unit tile's own outer width rescales
  // to the outer width a one-unit tile of the same per-unit figure would
  // have measured. unit = (132.38 - 2*2) / 2 = 64.19 — the same unit a
  // one-unit tile at 68.19 implies (68.19 - 2*2 = 64.19) — so both tiles'
  // normalised widths agree, and `medianTileWidth` can take their median
  // together without either one distorting it.
  it('rescales a two-unit tile to what a one-unit tile of the same per-unit width would measure', () => {
    expect(normalizedTileWidth(132.38, 2, 2)).toBeCloseTo(68.19, 5)
  })
})

describe('refinedStripUnit', () => {
  // O140's measured case: under mobile emulation at 390×844 the rail
  // subtracts no height of its own, and a loaded one-unit tile draws wider
  // than the CSS constant claims — 68.19 px outer width, against
  // `--strip-tile-border: 2px` and a nominal `--strip-unit` of 54. (*Why*
  // the contexts differ is not established — see `refinedStripUnit`'s doc.)
  // 68.19 - 2*2 = 64.19, which is more than 0.5 px away from 54, so the
  // measurement is not noise and the corrected value comes back.
  it('returns the measured unit when it differs from the current one past tolerance', () => {
    expect(refinedStripUnit(68.19, 2, 54)).toBeCloseTo(64.19, 5)
  })

  // The same machine's other case, measured under desktop emulation at
  // dpr 1, where the rail does subtract 15 px of its own height: a loaded
  // tile there is 58.16 px against the same 58 px nominal (54 unit + 2*2
  // border) — 58.16 - 4 = 54.16, only 0.16 px from the current 54, well
  // inside tolerance, and must not cause a write: a constant already this
  // close is sub-pixel rounding, not a platform difference.
  it('returns undefined when the measured unit is within tolerance of the current one', () => {
    expect(refinedStripUnit(58.16, 2, 54)).toBeUndefined()
  })

  it('returns undefined exactly at the tolerance boundary, not just below it', () => {
    // unit = 10 - 2*2 = 6; current 5.5; difference exactly 0.5.
    expect(refinedStripUnit(10, 2, 5.5)).toBeUndefined()
  })

  it('returns the measured unit just past the tolerance boundary', () => {
    // unit = 10 - 2*2 = 6; current 5.49; difference 0.51.
    expect(refinedStripUnit(10, 2, 5.49)).toBeCloseTo(6, 5)
  })

  it('honours a caller-supplied tolerance instead of the 0.5px default', () => {
    // unit = 20 - 2*1 = 18; current 17; difference 1, inside a tolerance of 2.
    expect(refinedStripUnit(20, 1, 17, 2)).toBeUndefined()
  })
})

describe('medianTileWidth', () => {
  // The default floor is 3 (fix-round review finding A, lowered from 5 now
  // that `normalizedTileWidth` lets every loaded tile contribute rather than
  // only one-unit ones) — two values is one short of it.
  it('is undefined below the minimum sample size', () => {
    expect(medianTileWidth([60, 60])).toBeUndefined()
  })

  it('is the middle value once the sample reaches the minimum', () => {
    expect(medianTileWidth([60, 61, 59])).toBe(60)
  })

  it('averages the two middle values for an even-sized sample', () => {
    // sorted: 58, 59, 60, 61, 62, 63 — the two middle values are 60 and 61.
    expect(medianTileWidth([63, 59, 61, 58, 60, 62])).toBe(60.5)
  })

  // The case `medianTileWidth` exists for (C2, whole-branch review): a
  // double-page scan the sidecar never marked `spread: true` still carries
  // the one-unit floor and would previously corrupt `--strip-unit` outright
  // if it happened to be the first tile to load — see `Filmstrip.tsx`'s
  // measurement effect. With an odd sample, the outlier sorts to one end and
  // never reaches the middle position, whatever its own magnitude is.
  it('is not moved by an outlier roughly twice the ordinary width', () => {
    const ordinary = [58, 59, 60, 61]
    expect(medianTileWidth([...ordinary, 120])).toBe(60)
    // Twenty times the ordinary width moves it no further than twice did —
    // the outlier's own magnitude is irrelevant once it sorts to one end.
    expect(medianTileWidth([...ordinary, 1200])).toBe(60)
  })

  it('honours a caller-supplied minimum sample size', () => {
    expect(medianTileWidth([60, 61], 2)).toBe(60.5)
    expect(medianTileWidth([60], 2)).toBeUndefined()
  })
})

describe('unitsBetween', () => {
  it('sums to the tile count when no group in the range is multi-unit', () => {
    const slots = whole(5)
    const grouped = groups(slots, 'single', 'rtl')
    expect(unitsBetween(grouped, 0, 3)).toBe(3)
  })

  // The case a mutant that counts groups instead of summing `tileUnits`
  // cannot be told apart from `unitsBetween` on any of this file's other
  // fixtures — every one of them is one-unit-per-group. `bSlots` has a
  // genuine `spread: true` group (slots 2 and 3, both page 2), grouped as a
  // single two-unit entry: `groups(bSlots, 'single', 'rtl')` is
  // `[0],[1],[2,3]spread,[4],[5],[6],[7],[8]` — 8 groups covering the 9
  // slots. Counting groups over the first three would give 3; summing units
  // gives 4, because the spread alone is worth 2.
  it('sums two units for a joined spread inside the range, not one per group', () => {
    const grouped = groups(bSlots, 'single', 'rtl')
    expect(unitsBetween(grouped, 0, 3)).toBe(4)
    // The whole rail: 7 singleton groups plus one 2-unit spread over 9 slots.
    expect(unitsBetween(grouped, 0, grouped.length)).toBe(9)
  })

  it('is zero for an empty range', () => {
    const grouped = groups(bSlots, 'single', 'rtl')
    expect(unitsBetween(grouped, 3, 3)).toBe(0)
  })
})

describe('stripWindow', () => {
  it('spans a margin either side of the focus', () => {
    expect(stripWindow(50, 237, 12)).toEqual({ lo: 38, hi: 63 })
  })

  it('clamps at the start rather than running negative', () => {
    expect(stripWindow(2, 237, 12)).toEqual({ lo: 0, hi: 15 })
  })

  it('clamps at the end rather than running past the last tile', () => {
    expect(stripWindow(235, 237, 12)).toEqual({ lo: 223, hi: 237 })
  })

  // `Math.min(focus, count - 1)` — without it, a focus past the last tile
  // (e.g. a stale index after the edition shrinks) would leave `at` above
  // `count - 1`, so `lo = at - margin` could exceed `hi = count`, and the
  // slice would come back empty: a rail with no tiles, one full-width
  // leading spacer and no trailing one.
  it('clamps a focus past the last tile rather than emptying the rail', () => {
    expect(stripWindow(300, 237, 12)).toEqual({ lo: 224, hi: 237 })
  })

  it('is the whole rail when the margin covers it', () => {
    expect(stripWindow(1, 4, 12)).toEqual({ lo: 0, hi: 4 })
  })

  // `currentIndex` is -1 in a gap and while correcting before a counterpart
  // is picked. A window built around -1 must not start below zero.
  it('treats a focus below zero as the first tile', () => {
    expect(stripWindow(-1, 237, 12)).toEqual({ lo: 0, hi: 13 })
  })

  it('is empty for an empty rail', () => {
    expect(stripWindow(0, 0, 12)).toEqual({ lo: 0, hi: 0 })
  })
})

describe('spacerWidth', () => {
  // A spacer replaces N tiles AND the gaps between them, but not the gap
  // either side of itself — the rail still lays one out, so N tiles
  // contribute N-1 internal gaps.
  //
  // The border term is `tiles * 2`, not `units * 2`: every tile has two
  // borders whatever its width. A tile's nominal OUTER width is therefore
  // `units * --strip-unit + 2 * --strip-tile-border`, which measures 101 px
  // at 1440x900 against the 100.97 px a loaded one-page tile occupied before
  // O140 touched anything. Measured on the running app: a 237-tile rail
  // whose tiles are all at their nominal width is
  // `237*101 + 236*6 + 16 = 25369`, and the live rail read 25465 — the 96 px
  // difference being the one tile on that volume whose image is wider than
  // its nominal and is allowed to exceed it (see `tileStyle`'s `minWidth`).
  it('sums the units, the borders and the internal gaps', () => {
    expect(spacerWidth(5, 4)).toBe(
      'calc(var(--strip-unit) * 5 + var(--strip-tile-border) * 8 + var(--strip-gap) * 3)',
    )
  })

  it('has no internal gap for a single tile', () => {
    expect(spacerWidth(2, 1)).toBe(
      'calc(var(--strip-unit) * 2 + var(--strip-tile-border) * 2 + var(--strip-gap) * 0)',
    )
  })

  it('is zero for no tiles, so a rail at either end renders no spacer', () => {
    expect(spacerWidth(0, 0)).toBe('0px')
  })
})

describe('focusAtFraction', () => {
  // Four tiles of one unit each: the rail is four units long, so each tile
  // owns a quarter of it.
  const even = [1, 1, 1, 1]

  it('is the first tile at the start of the rail', () => {
    expect(focusAtFraction(even, 0)).toBe(0)
  })

  it('is the last tile at the end of the rail', () => {
    expect(focusAtFraction(even, 1)).toBe(3)
  })

  it('is the tile whose span contains the fraction', () => {
    expect(focusAtFraction(even, 0.3)).toBe(1)
    expect(focusAtFraction(even, 0.6)).toBe(2)
  })

  // A wide tile owns proportionally more of the rail, which is the whole
  // reason this reads units rather than counting tiles.
  it('gives a two-unit tile twice the span of a one-unit tile', () => {
    const mixed = [1, 2, 1] // spans 0-0.25, 0.25-0.75, 0.75-1
    expect(focusAtFraction(mixed, 0.1)).toBe(0)
    expect(focusAtFraction(mixed, 0.5)).toBe(1)
    expect(focusAtFraction(mixed, 0.9)).toBe(2)
  })

  it('clamps a fraction outside the rail rather than running off it', () => {
    expect(focusAtFraction(even, -0.5)).toBe(0)
    expect(focusAtFraction(even, 1.5)).toBe(3)
  })

  it('is zero for an empty rail', () => {
    expect(focusAtFraction([], 0.5)).toBe(0)
  })
})
