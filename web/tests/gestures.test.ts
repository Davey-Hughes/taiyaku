import { describe, expect, it } from 'vitest'
import {
  SWIPE_MIN,
  TAP_HOLD,
  TAP_SLOP,
  gestureOfPress,
  type PressReading,
} from '../src/gestures'

/**
 * A press that is a tap in the middle of a 900 px page area, with nothing
 * open and nothing in the way. Each test overrides only what it is about,
 * so a case reads as its own difference from "an ordinary tap".
 */
function reading(over: Partial<PressReading> = {}): PressReading {
  return {
    down: { x: 450, y: 400, t: 1_000 },
    up: { x: 450, y: 400, t: 1_100 },
    width: 900,
    onControl: false,
    consumed: false,
    editing: false,
    panelOpen: false,
    scrolling: false,
    atWhole: true,
    ...over,
  }
}

/** The same press, moved to one x and given a matching release. */
function tapAt(x: number, over: Partial<PressReading> = {}): PressReading {
  return reading({
    down: { x, y: 400, t: 1_000 },
    up: { x, y: 400, t: 1_100 },
    ...over,
  })
}

/** A drag from the middle by `dx`, `dy`, over `ms`. */
function drag(
  dx: number,
  dy: number,
  ms: number,
  over: Partial<PressReading> = {},
): PressReading {
  return reading({
    down: { x: 450, y: 400, t: 1_000 },
    up: { x: 450 + dx, y: 400 + dy, t: 1_000 + ms },
    ...over,
  })
}

describe('a tap', () => {
  it('turns back on the left third and forward on the right', () => {
    // Physical, before `stepFor`: the left third is `-1` whatever the work
    // reads like, and `App.tsx`'s `readerIntent` is what mirrors it.
    expect(gestureOfPress(tapAt(10))).toEqual({ kind: 'turn', by: -1 })
    expect(gestureOfPress(tapAt(890))).toEqual({ kind: 'turn', by: 1 })
  })

  it('toggles the chrome in the middle third', () => {
    expect(gestureOfPress(tapAt(450))).toEqual({ kind: 'toggle-chrome' })
  })

  it('puts each boundary on the side away from the middle', () => {
    // 900/3 = 300. Just inside 300 turns; 300 itself is the middle, because
    // a reader aiming at the centre band should never turn by a pixel.
    expect(gestureOfPress(tapAt(299.9))).toEqual({ kind: 'turn', by: -1 })
    expect(gestureOfPress(tapAt(300))).toEqual({ kind: 'toggle-chrome' })
    expect(gestureOfPress(tapAt(599.9))).toEqual({ kind: 'toggle-chrome' })
    expect(gestureOfPress(tapAt(600))).toEqual({ kind: 'turn', by: 1 })
  })

  it('survives travel up to the slop and no further', () => {
    expect(gestureOfPress(drag(TAP_SLOP, 0, 100))).toEqual({
      kind: 'toggle-chrome',
    })
    expect(gestureOfPress(drag(0, TAP_SLOP, 100))).toEqual({
      kind: 'toggle-chrome',
    })
    // One pixel past it is a drag, and a drag of 11 px is not a swipe either.
    expect(gestureOfPress(drag(TAP_SLOP + 1, 0, 100))).toEqual({ kind: 'none' })
  })

  it('survives a hold up to the limit and no further', () => {
    expect(gestureOfPress(drag(0, 0, TAP_HOLD))).toEqual({
      kind: 'toggle-chrome',
    })
    // A long press is the selection callout's, not a turn's.
    expect(gestureOfPress(drag(0, 0, TAP_HOLD + 1))).toEqual({ kind: 'none' })
  })

  it('is nothing when it began on a control, or was already spent', () => {
    expect(gestureOfPress(tapAt(10, { onControl: true }))).toEqual({
      kind: 'none',
    })
    // The press that closed the note editor on `pointerdown`: its release
    // must not also turn a page.
    expect(gestureOfPress(tapAt(10, { consumed: true }))).toEqual({
      kind: 'none',
    })
  })

  it('leaves the press to region drawing while the editor is open', () => {
    expect(gestureOfPress(tapAt(10, { editing: true }))).toEqual({
      kind: 'none',
    })
    expect(gestureOfPress(tapAt(450, { editing: true }))).toEqual({
      kind: 'none',
    })
  })

  it('closes every open panel instead of turning', () => {
    expect(gestureOfPress(tapAt(10, { panelOpen: true }))).toEqual({
      kind: 'close-panels',
    })
    // It beats the middle third and scroll mode too: one tap returns to the
    // page whatever is over it, and nothing else happens.
    expect(gestureOfPress(tapAt(450, { panelOpen: true }))).toEqual({
      kind: 'close-panels',
    })
    expect(
      gestureOfPress(tapAt(890, { panelOpen: true, scrolling: true })),
    ).toEqual({ kind: 'close-panels' })
  })

  it('only toggles the chrome in scroll mode, wherever it lands', () => {
    expect(gestureOfPress(tapAt(10, { scrolling: true }))).toEqual({
      kind: 'toggle-chrome',
    })
    expect(gestureOfPress(tapAt(890, { scrolling: true }))).toEqual({
      kind: 'toggle-chrome',
    })
  })

  it('does nothing against a page area with no width', () => {
    // The pan's own zero-box guard, for the same reason: a box measured
    // mid-teardown divides the thirds into nothing.
    expect(gestureOfPress(tapAt(10, { width: 0 }))).toEqual({ kind: 'none' })
    expect(gestureOfPress(tapAt(10, { width: Number.NaN }))).toEqual({
      kind: 'none',
    })
  })
})

describe('a swipe', () => {
  it('turns forward when it travels left, and back when it travels right', () => {
    // Dragging the page leftwards brings in what is on the right, which is
    // the physical `→`.
    expect(gestureOfPress(drag(-SWIPE_MIN, 0, 200))).toEqual({
      kind: 'turn',
      by: 1,
    })
    expect(gestureOfPress(drag(SWIPE_MIN, 0, 200))).toEqual({
      kind: 'turn',
      by: -1,
    })
  })

  it('needs the distance', () => {
    expect(gestureOfPress(drag(-(SWIPE_MIN - 1), 0, 200))).toEqual({
      kind: 'none',
    })
  })

  it('needs to be more across than down', () => {
    // 60 across, 40 down: 60 < 2 × 40, so it is a diagonal and turns nothing.
    expect(gestureOfPress(drag(-60, 40, 200))).toEqual({ kind: 'none' })
    expect(gestureOfPress(drag(-100, 40, 200))).toEqual({ kind: 'turn', by: 1 })
  })

  it('has no time limit of its own', () => {
    // A slow drag at fit-whole moved nothing, so reading it as a turn takes
    // nothing from the pan.
    expect(gestureOfPress(drag(-120, 0, 4_000))).toEqual({
      kind: 'turn',
      by: 1,
    })
  })

  it('is nothing once the reader has zoomed in', () => {
    expect(gestureOfPress(drag(-120, 0, 200, { atWhole: false }))).toEqual({
      kind: 'none',
    })
  })

  it('is nothing in scroll mode, while editing, or under a panel', () => {
    expect(gestureOfPress(drag(-120, 0, 200, { scrolling: true }))).toEqual({
      kind: 'none',
    })
    expect(gestureOfPress(drag(-120, 0, 200, { editing: true }))).toEqual({
      kind: 'none',
    })
    expect(gestureOfPress(drag(-120, 0, 200, { panelOpen: true }))).toEqual({
      kind: 'none',
    })
  })

  it('is nothing when it began on a control, or was already spent', () => {
    expect(gestureOfPress(drag(-120, 0, 200, { onControl: true }))).toEqual({
      kind: 'none',
    })
    expect(gestureOfPress(drag(-120, 0, 200, { consumed: true }))).toEqual({
      kind: 'none',
    })
  })
})
