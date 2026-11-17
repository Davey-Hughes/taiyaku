import { afterEach, describe, expect, it, vi } from 'vitest'
import { type Geometry, geometryOf, moveOf } from '../src/reseat'
import {
  frameTime,
  mayReseat,
  onRowLaid,
  onScrollPutBack,
  reseatEased,
  rowLaid,
  scrollPutBack,
  TURN_RESEATS,
  turnSnaps,
  uprightNow,
} from '../src/reseatTiming'

/**
 * The arithmetic the covers glide by (spec O304): where a list lays each
 * cover, and the move each cover makes when the column count changes. The
 * motion itself is drawn only in a browser.
 */

afterEach(() => {
  vi.restoreAllMocks()
  vi.unstubAllGlobals()
})

/** The style a list's tokens and resolved rows give, in px. */
const style = (fill: boolean, rows: number[] | undefined) => ({
  track: 144,
  gap: 16,
  fill,
  rows,
})

describe('geometryOf', () => {
  it('lays Smooth covers at the track, spread across the width', () => {
    // Six 144 px columns across 1008 px, the 144 px left over spread into
    // five gaps of 28.8; fourteen covers make three rows.
    expect(geometryOf(1008, style(false, [250, 250, 120]), 14)).toEqual({
      count: 14,
      columns: 6,
      column: 144,
      pitch: 172.8,
      tops: [0, 266, 532],
      heights: [250, 250, 120],
    })
  })

  it('lays filled covers across the width at the least gap', () => {
    // Six columns share 1008 px less five 16 px gaps: 928 / 6 each.
    const filled = geometryOf(1008, style(true, [260, 260, 260]), 14)
    expect(filled?.columns).toBe(6)
    expect(filled?.column).toBeCloseTo(928 / 6, 9)
    expect(filled?.pitch).toBeCloseTo(928 / 6 + 16, 9)
    expect(filled?.tops).toEqual([0, 276, 552])
  })

  it('lays one column at the track in Smooth and at the whole width filled', () => {
    // 150 px holds one 144 px track and not a second.
    expect(geometryOf(150, style(false, [250, 250]), 2)).toMatchObject({
      columns: 1,
      column: 144,
      pitch: 0,
      tops: [0, 266],
    })
    expect(geometryOf(150, style(true, [260, 260]), 2)).toMatchObject({
      columns: 1,
      column: 150,
      tops: [0, 276],
    })
  })

  it('describes no list whose rows the grid did not lay as it counts them', () => {
    // Fourteen covers in six columns are three rows, not two or four.
    expect(geometryOf(1008, style(false, [250, 250]), 14)).toBeUndefined()
    expect(
      geometryOf(1008, style(false, [250, 250, 250, 250]), 14),
    ).toBeUndefined()
    expect(geometryOf(1008, style(false, undefined), 14)).toBeUndefined()
  })

  it('describes no list without a width or the tokens in px', () => {
    expect(geometryOf(0, style(false, []), 0)).toBeUndefined()
    expect(
      geometryOf(1008, { ...style(false, [250]), track: Number.NaN }, 6),
    ).toBeUndefined()
    expect(
      geometryOf(1008, { ...style(false, [250]), gap: Number.NaN }, 6),
    ).toBeUndefined()
  })
})

describe('moveOf', () => {
  /** Six Smooth columns 172.8 px apart, rows 250 px tall 16 px apart. */
  const six: Geometry = {
    count: 14,
    columns: 6,
    column: 144,
    pitch: 172.8,
    tops: [0, 266, 532],
    heights: [250, 250, 250],
  }
  /** Five columns 189 px apart: the same covers in a narrower list. */
  const five: Geometry = {
    count: 14,
    columns: 5,
    column: 144,
    pitch: 189,
    tops: [0, 266, 532],
    heights: [250, 250, 250],
  }
  const view = 800

  it('moves a cover from its place in the old columns to its place in the new', () => {
    // The sixth cover ends the first row of six and starts the second of
    // five: it was drawn 864 px right of its new box and 266 px above it.
    expect(
      moveOf(
        5,
        { geometry: six, left: 40, top: 100 },
        { geometry: five, left: 40, top: 100 },
        view,
      ),
    ).toEqual({ dx: 864, dy: -266, scale: 1 })
  })

  it('makes no move of a cover under 2 px from its new box, in place and in width', () => {
    const at = (left: number, top: number, column = 144) =>
      moveOf(
        0,
        { geometry: six, left: 40, top: 100 },
        { geometry: { ...five, column }, left, top },
        view,
      )
    expect(at(41.9, 98.1, 145.9)).toBeUndefined()
    // THE CONTROL: 2 px in either direction or in width is a move.
    expect(at(42, 100)).toEqual({ dx: -2, dy: 0, scale: 1 })
    expect(at(40, 102)).toEqual({ dx: 0, dy: -2, scale: 1 })
    expect(at(40, 100, 146)).toEqual({ dx: 0, dy: 0, scale: 144 / 146 })
  })

  it('makes no move of a cover beyond 200 px of the view before and after', () => {
    // The sixth cover's row is 266 px lower in five columns.
    const from = (top: number) =>
      moveOf(
        5,
        { geometry: six, left: 40, top },
        { geometry: five, left: 40, top },
        view,
      )
    // Below: drawn from 1000 px, 200 px under the view's 800, and to 1266.
    expect(from(1000)).toBeUndefined()
    // Above: a 250 px cover drawn from -716 and going to -450 ends 200 px
    // over the view's top at the nearest.
    expect(from(-716)).toBeUndefined()
    // THE CONTROL: a pixel nearer, in either direction, and it moves.
    expect(from(999)).toEqual({ dx: 864, dy: -266, scale: 1 })
    expect(from(-715)).toEqual({ dx: 864, dy: -266, scale: 1 })
  })

  it('moves a cover whose new box alone is near the view', () => {
    // From 1000 px down, out of reach, to 966 (its list now at 700).
    expect(
      moveOf(
        5,
        { geometry: six, left: 40, top: 1000 },
        { geometry: five, left: 40, top: 700 },
        view,
      ),
    ).toEqual({ dx: 864, dy: 34, scale: 1 })
  })

  it('starts a cover still moving from where that move draws it now', () => {
    // A quarter of the way through a move from 100 px right, 40 up, at
    // twice the size: three quarters of the offset left, 1.75 times the
    // size, and the new move starts there.
    const running = { dx: 100, dy: -40, scale: 2, progress: 0.25 }
    const move = moveOf(
      5,
      { geometry: six, left: 40, top: 100 },
      { geometry: five, left: 40, top: 100 },
      view,
      running,
    )
    expect(move?.dx).toBeCloseTo(864 + 75, 9)
    expect(move?.dy).toBeCloseTo(-266 - 30, 9)
    expect(move?.scale).toBeCloseTo(1.75, 9)
    // THE CONTROL: the same move run to its end starts from the cover's box.
    expect(
      moveOf(
        5,
        { geometry: six, left: 40, top: 100 },
        { geometry: five, left: 40, top: 100 },
        view,
        { ...running, progress: 1 },
      ),
    ).toEqual({ dx: 864, dy: -266, scale: 1 })
  })
})

describe('reseatEased', () => {
  it('runs from 0 to 1, fast out', () => {
    expect([
      reseatEased(-1),
      reseatEased(0),
      reseatEased(1),
      reseatEased(2),
    ]).toEqual([0, 0, 1, 1])
    // Nine tenths of the way in under half the time.
    expect(reseatEased(0.45)).toBeGreaterThan(0.9)
    let before = 0
    for (let step = 1; step < 100; step++) {
      const now = reseatEased(step / 100)
      expect(now).toBeGreaterThan(before)
      before = now
    }
  })
})

describe('mayReseat', () => {
  const asked = (reduce: boolean) =>
    vi.fn((query: string) => ({
      matches: query === '(prefers-reduced-motion: reduce)' && reduce,
    }))

  it('lets covers move where the system does not ask for less motion', () => {
    vi.stubGlobal('matchMedia', asked(false))
    expect(mayReseat()).toBe(true)
  })

  it('moves nothing where it asks for less, or where it cannot be asked', () => {
    vi.stubGlobal('matchMedia', asked(true))
    expect(mayReseat()).toBe(false)
    vi.stubGlobal('matchMedia', undefined)
    expect(mayReseat()).toBe(false)
  })
})

describe('uprightNow', () => {
  it('is whether the view stands taller than wide, and nothing where it cannot be asked', () => {
    const asked = (portrait: boolean) =>
      vi.fn((query: string) => ({
        matches: query === '(orientation: portrait)' && portrait,
      }))
    vi.stubGlobal('matchMedia', asked(true))
    expect(uprightNow()).toBe(true)
    vi.stubGlobal('matchMedia', asked(false))
    expect(uprightNow()).toBe(false)
    vi.stubGlobal('matchMedia', undefined)
    expect(uprightNow()).toBeUndefined()
  })
})

describe('turnSnaps', () => {
  it('snaps a change laid in the other orientation, and none laid in the same', () => {
    expect({
      reseats: TURN_RESEATS,
      turned: [turnSnaps(true, false), turnSnaps(false, true)],
      held: [turnSnaps(true, true), turnSnaps(false, false)],
      unasked: turnSnaps(undefined, undefined),
    }).toEqual({
      reseats: false,
      turned: [true, true],
      held: [false, false],
      unasked: false,
    })
  })
})

describe('frameTime', () => {
  it('is the document timeline’s time, and the clock’s where there is none', () => {
    vi.stubGlobal('document', { timeline: { currentTime: 1234.5 } })
    expect(frameTime()).toBe(1234.5)
    vi.stubGlobal('document', { timeline: { currentTime: null } })
    vi.spyOn(performance, 'now').mockReturnValue(99)
    expect(frameTime()).toBe(99)
    vi.stubGlobal('document', undefined)
    expect(frameTime()).toBe(99)
  })
})

describe.each([
  ['onRowLaid', onRowLaid, rowLaid, scrollPutBack],
  ['onScrollPutBack', onScrollPutBack, scrollPutBack, rowLaid],
] as const)('%s', (_, on, say, other) => {
  it('calls every listener at each signal, until each is removed, and at no other', () => {
    const first = vi.fn()
    const second = vi.fn()
    const offFirst = on(first)
    const offSecond = on(second)
    say()
    expect([first.mock.calls.length, second.mock.calls.length]).toEqual([1, 1])
    other()
    expect([first.mock.calls.length, second.mock.calls.length]).toEqual([1, 1])
    offFirst()
    say()
    expect([first.mock.calls.length, second.mock.calls.length]).toEqual([1, 2])
    offSecond()
    say()
    expect([first.mock.calls.length, second.mock.calls.length]).toEqual([1, 2])
  })
})
