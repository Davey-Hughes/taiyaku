import { describe, expect, it } from 'vitest'
import { bSlots, whole } from './support/fixtures'
import { correctionTiles, groups } from '../src/pairing'
import {
  focusAtFraction,
  type GlideSample,
  glideSpeed,
  glideTravel,
  medianTileWidth,
  normalizedTileWidth,
  refinedStripUnit,
  releaseSpeed,
  shownShare,
  spacerWidth,
  stripWheel,
  stripWindow,
  tileMinWidth,
  tilePages,
  tileUnits,
  unitsBetween,
} from '../src/tiles'

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
  // A loaded one-unit tile 68.19 px wide, against `--strip-tile-border: 2px`
  // and a current `--strip-unit` of 54: 68.19 - 2*2 = 64.19, which is more
  // than 0.5 px away from 54, so the measurement is not noise and the
  // corrected value comes back.
  it('returns the measured unit when it differs from the current one past tolerance', () => {
    expect(refinedStripUnit(68.19, 2, 54)).toBeCloseTo(64.19, 5)
  })

  // A loaded tile 58.16 px wide against the same 58 px nominal (54 unit +
  // 2*2 border): 58.16 - 4 = 54.16, only 0.16 px from the current 54, well
  // inside tolerance, and must not cause a write.
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
  // `units * --strip-unit + 2 * --strip-tile-border`.
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

describe('stripWheel', () => {
  const wheel = (
    over: Partial<{
      deltaX: number
      deltaY: number
      deltaMode: number
      ctrlKey: boolean
      shiftKey: boolean
    }> = {},
  ) => ({
    deltaX: 0,
    deltaY: 0,
    deltaMode: 0,
    ctrlKey: false,
    shiftKey: false,
    ...over,
  })

  it('moves toward later pages for a notch down, and earlier for a notch up', () => {
    expect(stripWheel(wheel({ deltaY: 100 }))).toBe(100)
    expect(stripWheel(wheel({ deltaY: -100 }))).toBe(-100)
  })

  it('moves the rail by a pixel-mode wheel’s own delta', () => {
    expect(stripWheel(wheel({ deltaY: 40 }))).toBeCloseTo(40, 10)
    expect(stripWheel(wheel({ deltaY: -7 }))).toBeCloseTo(-7, 10)
  })

  // One detent, reported three ways: `viewport.ts`'s `wheelNotches`.
  it('reads one detent alike in pixels, lines and pages', () => {
    expect(stripWheel(wheel({ deltaY: 3, deltaMode: 1 }))).toBe(100)
    expect(stripWheel(wheel({ deltaY: 1, deltaMode: 2 }))).toBe(100)
  })

  it('caps one event at four notches', () => {
    expect(stripWheel(wheel({ deltaY: 100_000 }))).toBe(400)
    expect(stripWheel(wheel({ deltaY: -100_000 }))).toBe(-400)
  })

  it('leaves Ctrl and Shift to the browser', () => {
    expect(stripWheel(wheel({ deltaY: 100, ctrlKey: true }))).toBeUndefined()
    expect(stripWheel(wheel({ deltaY: 100, shiftKey: true }))).toBeUndefined()
  })

  it('leaves a sideways delta to the browser, a tie included', () => {
    expect(stripWheel(wheel({ deltaX: 100 }))).toBeUndefined()
    expect(stripWheel(wheel({ deltaX: 100, deltaY: 30 }))).toBeUndefined()
    expect(stripWheel(wheel({ deltaX: -100, deltaY: 30 }))).toBeUndefined()
    expect(stripWheel(wheel({ deltaX: 100, deltaY: 100 }))).toBeUndefined()
  })

  it('takes a vertical wheel that drifts a little sideways', () => {
    expect(stripWheel(wheel({ deltaX: 30, deltaY: 100 }))).toBe(100)
    expect(stripWheel(wheel({ deltaX: -30, deltaY: -100 }))).toBe(-100)
  })

  it('takes nothing from a wheel it cannot read', () => {
    expect(stripWheel(wheel())).toBeUndefined()
    expect(stripWheel(wheel({ deltaY: Number.NaN }))).toBeUndefined()
  })
})

// Each share asked for below is a count of sixteenths, which a float holds
// exactly, so `toEqual` asks for the number itself.
describe('shownShare', () => {
  /** A rail four times as long as it is wide, so it shows a quarter of itself. */
  const rail = (scrollLeft: number) => ({
    scrollLeft,
    clientWidth: 400,
    scrollWidth: 1600,
  })

  it('is the first quarter of the rail at its start', () => {
    expect(shownShare(rail(0))).toEqual({ start: 0, end: 0.25 })
  })

  it('is the stretch the rail has been scrolled to, in the middle', () => {
    expect(shownShare(rail(600))).toEqual({ start: 0.375, end: 0.625 })
  })

  it('reaches the end of the rail at the end of its travel', () => {
    expect(shownShare(rail(1200))).toEqual({ start: 0.75, end: 1 })
  })

  // A reversed row scrolls negative, and its start is still the start of the
  // volume: each reading is the one its positive twin gives.
  it('reads a reversed row as it reads its twin', () => {
    expect(shownShare(rail(-600))).toEqual(shownShare(rail(600)))
    expect(shownShare(rail(-600))).toEqual({ start: 0.375, end: 0.625 })
    expect(shownShare(rail(-1200))).toEqual({ start: 0.75, end: 1 })
  })

  it('stops at the end of the travel for a rail scrolled past it, either way round', () => {
    expect(shownShare(rail(1300))).toEqual({ start: 0.75, end: 1 })
    expect(shownShare(rail(-1300))).toEqual({ start: 0.75, end: 1 })
  })

  it('is the whole rail where nothing overflows', () => {
    const whole = { start: 0, end: 1 }
    expect(
      shownShare({ scrollLeft: 0, clientWidth: 400, scrollWidth: 400 }),
    ).toEqual(whole)
    expect(
      shownShare({ scrollLeft: 0, clientWidth: 400, scrollWidth: 300 }),
    ).toEqual(whole)
    // What jsdom reports for every element, and a browser for a rail that has
    // not been laid out.
    expect(
      shownShare({ scrollLeft: 0, clientWidth: 0, scrollWidth: 0 }),
    ).toEqual(whole)
  })
})

// O216. In figures, and not in the constants' names: these are the numbers
// the glide was tried and chosen at, and a case written in the constants'
// own terms would pass whatever they were changed to.
describe('releaseSpeed', () => {
  /** `count` moves `step` px apart, one every `every` ms, from x 500 at 1000 ms. */
  const steady = (count: number, every: number, step: number): GlideSample[] =>
    Array.from({ length: count }, (_, i) => ({
      at: 1000 + i * every,
      x: 500 + i * step,
    }))
  /** When the last move of `trail` was made. */
  const lastAt = (trail: GlideSample[]) => trail[trail.length - 1].at

  it('is the speed of a steady drag let go as it moves, either way', () => {
    const right = steady(11, 10, 10)
    expect(releaseSpeed(right, lastAt(right))).toBe(1)
    const left = steady(11, 10, -25)
    expect(releaseSpeed(left, lastAt(left))).toBe(-2.5)
  })

  it('reads the moves of the last 100 ms before the last one, and none older', () => {
    // Slow for most of a second, and then 100 px in the last 100 ms.
    const trail = [
      { at: 0, x: 300 },
      { at: 899.5, x: 400 },
      { at: 900, x: 400 },
      { at: 1000, x: 500 },
    ]
    expect(releaseSpeed(trail, 1000)).toBe(1)
  })

  it('is 0 where the pointer had rested 60 ms when it was let go', () => {
    const trail = steady(11, 10, 10)
    const last = lastAt(trail)
    expect(releaseSpeed(trail, last + 60)).toBe(0)
    expect(releaseSpeed(trail, last + 500)).toBe(0)
    // THE CONTROL: a rest short of it, and none.
    expect(releaseSpeed(trail, last + 59.9)).toBe(1)
    expect(releaseSpeed(trail, last)).toBe(1)
  })

  it('is 0 for a drag with one move or none in the time it is read over', () => {
    expect(releaseSpeed([], 1000)).toBe(0)
    expect(releaseSpeed([{ at: 1000, x: 600 }], 1000)).toBe(0)
    // Three moves, and only the last of them is within 100 ms of the last.
    const spaced = [
      { at: 0, x: 0 },
      { at: 500, x: 300 },
      { at: 1000, x: 600 },
    ]
    expect(releaseSpeed(spaced, 1000)).toBe(0)
  })

  it('is 0 under 0.1 px a millisecond, either way, and the speed from 0.1 up', () => {
    const over100ms = (step: number) => steady(11, 10, step)
    expect(releaseSpeed(over100ms(0.99), 1100)).toBe(0)
    expect(releaseSpeed(over100ms(-0.99), 1100)).toBe(0)
    expect(releaseSpeed(over100ms(1), 1100)).toBe(0.1)
    expect(releaseSpeed(over100ms(-1), 1100)).toBe(-0.1)
  })

  // A press that creeps 6 px in 202 ms, a pixel every 40 ms, its last two
  // moves 2 ms apart. With the press in the trail, the 100 ms before the
  // last move hold 3 px in 82 ms, which is under 0.1 px a millisecond.
  it('is 0 for a creep read from its press on, whose last two moves alone would read as fast', () => {
    const creep = [
      { at: 0, x: 500 },
      { at: 40, x: 501 },
      { at: 80, x: 502 },
      { at: 120, x: 503 },
      { at: 160, x: 504 },
      { at: 200, x: 505 },
      { at: 202, x: 506 },
    ]
    expect(releaseSpeed(creep, 202)).toBe(0)
    // THE CONTROL: what its last two moves read by themselves.
    expect(releaseSpeed(creep.slice(-2), 202)).toBe(0.5)
  })

  // Two places are a speed: a press, and one move 6 px on 30 ms later.
  it('reads a press and one move after it as the travel over the time between them', () => {
    const nudge = [
      { at: 1000, x: 500 },
      { at: 1030, x: 506 },
    ]
    expect(releaseSpeed(nudge, 1030)).toBe(0.2)
  })

  it('is 0 where the first and the last move it reads share a time', () => {
    const trail = [
      { at: 1000, x: 500 },
      { at: 1000, x: 560 },
    ]
    expect(releaseSpeed(trail, 1000)).toBe(0)
  })
})

describe('glideTravel', () => {
  it('has carried the rail nowhere at the release, nor for a frame stamped before it', () => {
    expect(glideTravel(2, 0)).toBe(0)
    expect(glideTravel(2, -16)).toBe(0)
  })

  it('slows exponentially, with a time constant of 325 ms', () => {
    // At one time constant, all of the way but a share of 1/e; at two, all
    // but 1/e².
    expect(glideTravel(1, 325)).toBeCloseTo(325 * (1 - Math.exp(-1)), 9)
    expect(glideTravel(1, 650)).toBeCloseTo(325 * (1 - Math.exp(-2)), 9)
    expect(glideTravel(2, 16)).toBeCloseTo(650 * (1 - Math.exp(-16 / 325)), 9)
  })

  it('goes the way the pointer was going', () => {
    for (const elapsed of [16, 325, 2000]) {
      expect(glideTravel(-1.5, elapsed)).toBeLessThan(0)
      expect(glideTravel(-1.5, elapsed)).toBe(-glideTravel(1.5, elapsed))
    }
  })

  it('comes as near as it likes to the speed times 325 ms, and never passes it', () => {
    expect(glideTravel(1, 5000)).toBeCloseTo(325, 3)
    expect(glideTravel(1, 5000)).toBeLessThanOrEqual(325)
    expect(glideTravel(-2, 5000)).toBeCloseTo(-650, 3)
    expect(glideTravel(-2, 5000)).toBeGreaterThanOrEqual(-650)
    expect(glideTravel(1, 1e9)).toBe(325)
    expect(glideTravel(3, Number.POSITIVE_INFINITY)).toBe(975)
  })

  it('never goes back as time goes on', () => {
    let before = glideTravel(1, -50)
    for (let elapsed = -40; elapsed <= 3000; elapsed += 10) {
      const now = glideTravel(1, elapsed)
      expect(now).toBeGreaterThanOrEqual(before)
      before = now
    }
  })
})

describe('glideSpeed', () => {
  it('is the speed the glide began at as it begins, and for a time before it', () => {
    expect(glideSpeed(2, 0)).toBe(2)
    expect(glideSpeed(-0.5, 0)).toBe(-0.5)
    expect(glideSpeed(2, -16)).toBe(2)
  })

  it('falls to 1/e of that in 325 ms, and to 1/e² in 650', () => {
    expect(glideSpeed(2, 325)).toBeCloseTo(2 * Math.exp(-1), 12)
    expect(glideSpeed(2, 650)).toBeCloseTo(2 * Math.exp(-2), 12)
  })

  it('goes the way the pointer was going, and never faster as time goes on', () => {
    let before = glideSpeed(1.5, 0)
    for (let elapsed = 10; elapsed <= 3000; elapsed += 10) {
      const now = glideSpeed(1.5, elapsed)
      expect(now).toBeGreaterThan(0)
      expect(now).toBeLessThan(before)
      expect(glideSpeed(-1.5, elapsed)).toBe(-now)
      before = now
    }
  })

  // The two functions are one glide. Its speed is how fast its travel is
  // growing, read here from the travel a thousandth of a millisecond either
  // side; and the travel so far and the travel still to come, which is the
  // speed times 325 ms, make the whole of the glide's way.
  it('is the slope of `glideTravel`, and what is left of the way over 325 ms', () => {
    for (const speed of [1, -2.5, 0.1]) {
      for (let elapsed = 1; elapsed <= 3000; elapsed += 37) {
        const slope =
          (glideTravel(speed, elapsed + 0.001) -
            glideTravel(speed, elapsed - 0.001)) /
          0.002
        expect(glideSpeed(speed, elapsed)).toBeCloseTo(slope, 6)
        expect(
          glideTravel(speed, elapsed) + 325 * glideSpeed(speed, elapsed),
        ).toBeCloseTo(325 * speed, 9)
      }
    }
  })

  // 325 × ln 10 is 748.3.
  it('is under 0.1 px a millisecond from 749 ms into a glide begun at 1, and not at 748', () => {
    expect(glideSpeed(1, 748)).toBeGreaterThan(0.1)
    expect(glideSpeed(1, 749)).toBeLessThan(0.1)
    expect(glideSpeed(-1, 748)).toBeLessThan(-0.1)
    expect(glideSpeed(-1, 749)).toBeGreaterThan(-0.1)
  })
})
