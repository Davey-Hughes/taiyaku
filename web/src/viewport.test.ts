import { describe, expect, it } from 'vitest'
import {
  WHOLE,
  type Cell,
  clampRect,
  type Drawn,
  fitRect,
  type Frame,
  groupToImage,
  imageToGroup,
  into,
  mapRect,
  mapView,
  outOf,
  paintedIn,
  panBy,
  wheelBy,
  zoomAt,
} from './viewport'

describe('clampRect', () => {
  it('leaves a rect already inside the group alone', () => {
    expect(clampRect({ x: 0.2, y: 0.3, w: 0.5, h: 0.4 })).toEqual({
      x: 0.2,
      y: 0.3,
      w: 0.5,
      h: 0.4,
    })
  })

  it('slides a rect that overhangs back inside, keeping its size', () => {
    expect(clampRect({ x: 0.8, y: 0.9, w: 0.5, h: 0.4 })).toEqual({
      x: 0.5,
      y: 0.6,
      w: 0.5,
      h: 0.4,
    })
  })

  it('slides a rect that underhangs back inside, keeping its size', () => {
    expect(clampRect({ x: -0.3, y: -0.1, w: 0.5, h: 0.4 })).toEqual({
      x: 0,
      y: 0,
      w: 0.5,
      h: 0.4,
    })
  })

  // The page-turn case the spec calls forced: a rect carried from a two-slot
  // group onto a one-slot group is wider than the group it landed in.
  it('reduces a rect larger than the group to the whole group', () => {
    expect(clampRect({ x: -0.5, y: 0.1, w: 2, h: 0.4 })).toEqual({
      x: 0,
      y: 0.1,
      w: 1,
      h: 0.4,
    })
  })

  it('is the whole group for the whole group', () => {
    expect(clampRect(WHOLE)).toEqual(WHOLE)
  })
})

describe('fitRect', () => {
  // "The content, fitted": expand to the viewport's aspect, never shrink, so a
  // swap never hides something that was visible before it.
  it('leaves a rect that already matches the viewport aspect alone', () => {
    // group 0.5 wide x 0.5 tall of a 1:2 group is 0.5:1 in real terms;
    // a viewport of the same 0.5 aspect needs no adjustment.
    expect(fitRect({ x: 0.25, y: 0.25, w: 0.5, h: 0.5 }, 0.5, 0.5)).toEqual({
      x: 0.25,
      y: 0.25,
      w: 0.5,
      h: 0.5,
    })
  })

  it('widens a rect too tall for the viewport, about its centre', () => {
    // rect is 0.5 x 0.5 of a square group => real aspect 1. Viewport aspect 2
    // is wider, so width doubles and the centre holds.
    expect(fitRect({ x: 0.25, y: 0.25, w: 0.5, h: 0.5 }, 2, 1)).toEqual({
      x: 0,
      y: 0.25,
      w: 1,
      h: 0.5,
    })
  })

  it('heightens a rect too wide for the viewport, about its centre', () => {
    expect(fitRect({ x: 0.25, y: 0.25, w: 0.5, h: 0.5 }, 0.5, 1)).toEqual({
      x: 0.25,
      y: 0,
      w: 0.5,
      h: 1,
    })
  })

  it('never shrinks — a fitted rect contains the rect it came from', () => {
    const before = { x: 0.25, y: 0.25, w: 0.5, h: 0.5 }
    const after = fitRect(before, 2, 1)
    expect(after.x).toBeLessThanOrEqual(before.x)
    expect(after.y).toBeLessThanOrEqual(before.y)
    expect(after.x + after.w).toBeGreaterThanOrEqual(before.x + before.w)
    expect(after.y + after.h).toBeGreaterThanOrEqual(before.y + before.h)
  })
})

describe('zoomAt', () => {
  it('halves the rect when zooming in by two', () => {
    const after = zoomAt(WHOLE, { x: 0.5, y: 0.5 }, 2, 8)
    expect(after).toEqual({ x: 0.25, y: 0.25, w: 0.5, h: 0.5 })
  })

  // The anchor is the whole point: the pixel under the pointer must not move.
  it('keeps the anchored point where it was', () => {
    const at = { x: 0.25, y: 0.75 }
    const after = zoomAt(WHOLE, at, 2, 8)
    expect((at.x - after.x) / after.w).toBeCloseTo(0.25, 10)
    expect((at.y - after.y) / after.h).toBeCloseTo(0.75, 10)
  })

  it('will not zoom in past maxScale', () => {
    const after = zoomAt(WHOLE, { x: 0.5, y: 0.5 }, 100, 4)
    expect(after.w).toBeCloseTo(0.25, 10)
    expect(after.h).toBeCloseTo(0.25, 10)
  })

  it('will not zoom out past the whole group', () => {
    const after = zoomAt(
      { x: 0.25, y: 0.25, w: 0.5, h: 0.5 },
      { x: 0.5, y: 0.5 },
      0.01,
      8,
    )
    expect(after).toEqual(WHOLE)
  })

  // Zooming out near an edge has to end up inside the group, not overhanging
  // it — which is the clamp doing its job on the way out, not a second rule.
  // Anchor at (1,1) against a rect already touching that corner is a no-op
  // for the clamp's slide, so this alone does not exercise it.
  it('lands inside the group when zooming out from a corner', () => {
    const after = zoomAt(
      { x: 0.75, y: 0.75, w: 0.25, h: 0.25 },
      { x: 1, y: 1 },
      0.5,
      8,
    )
    expect(after.x).toBeGreaterThanOrEqual(0)
    expect(after.y).toBeGreaterThanOrEqual(0)
    expect(after.x + after.w).toBeLessThanOrEqual(1 + 1e-9)
    expect(after.y + after.h).toBeLessThanOrEqual(1 + 1e-9)
  })

  // Off a corner, the clamp's slide actually moves the rect, and the anchor
  // gives way with it: the point at fraction 0.5 of the old rect on both
  // axes is at fraction 0.75 of the new one, not 0.5. Pins the caveat the
  // doc comment now states, rather than leaving it only described.
  it('lets the anchor slide when a zoom-out forces the clamp to move the rect', () => {
    const rect = { x: 0.7, y: 0.7, w: 0.3, h: 0.3 }
    const at = { x: 0.85, y: 0.85 }
    const before = {
      fx: (at.x - rect.x) / rect.w,
      fy: (at.y - rect.y) / rect.h,
    }
    const after = zoomAt(rect, at, 0.5, 8)
    expect(after).toEqual({ x: 0.4, y: 0.4, w: 0.6, h: 0.6 })
    const afterFraction = {
      fx: (at.x - after.x) / after.w,
      fy: (at.y - after.y) / after.h,
    }
    expect(afterFraction.fx).not.toBeCloseTo(before.fx, 5)
    expect(afterFraction.fy).not.toBeCloseTo(before.fy, 5)
    expect(afterFraction.fx).toBeCloseTo(0.75, 10)
    expect(afterFraction.fy).toBeCloseTo(0.75, 10)
  })

  // The ceiling is a third, distinct behaviour from the floor case above and
  // the edge-slide case below: reaching the whole group's extent on an axis
  // resets that axis to {start: 0, size: 1} unconditionally (`clampRect`),
  // discarding the solved origin rather than sliding it. An ordinary
  // zoom-out, not a corner: rect.w / by = 0.9 / 0.5 = 1.8, capped to 1.
  it('gives way for the anchor when a zoom-out hits the whole-group ceiling', () => {
    const rect = { x: 0.1, y: 0.1, w: 0.9, h: 0.9 }
    const at = { x: 0.15, y: 0.15 }
    const before = {
      fx: (at.x - rect.x) / rect.w,
      fy: (at.y - rect.y) / rect.h,
    }
    const after = zoomAt(rect, at, 0.5, 8)
    expect(after).toEqual(WHOLE)
    const afterFraction = {
      fx: (at.x - after.x) / after.w,
      fy: (at.y - after.y) / after.h,
    }
    expect(before.fx).toBeCloseTo(0.0556, 4)
    expect(before.fy).toBeCloseTo(0.0556, 4)
    expect(afterFraction.fx).toBeCloseTo(0.15, 10)
    expect(afterFraction.fy).toBeCloseTo(0.15, 10)
    expect(afterFraction.fx).not.toBeCloseTo(before.fx, 2)
    expect(afterFraction.fy).not.toBeCloseTo(before.fy, 2)
  })

  // `by <= 0` has no zoom direction; a zero-size incoming rect divides by
  // zero. Either alone reaches NaN throughout the result and, downstream, a
  // CSS `scale(NaN)` — the guard leaves the view alone instead.
  it('leaves the rect unchanged for a non-positive by', () => {
    expect(zoomAt(WHOLE, { x: 0.5, y: 0.5 }, 0, 8)).toEqual(WHOLE)
    expect(zoomAt(WHOLE, { x: 0.5, y: 0.5 }, -2, 8)).toEqual(WHOLE)
  })

  it('leaves the rect unchanged for a degenerate zero-size rect', () => {
    const rect = { x: 0.2, y: 0.3, w: 0, h: 0 }
    expect(zoomAt(rect, { x: 0.5, y: 0.5 }, 2, 8)).toEqual(rect)
  })
})

describe('panBy', () => {
  it('moves the rect and keeps its size', () => {
    expect(panBy({ x: 0.25, y: 0.25, w: 0.5, h: 0.5 }, 0.1, -0.1)).toEqual({
      x: 0.35,
      y: 0.15,
      w: 0.5,
      h: 0.5,
    })
  })

  it('stops at the group edge rather than leaving it', () => {
    expect(panBy({ x: 0.25, y: 0.25, w: 0.5, h: 0.5 }, 5, 5)).toEqual({
      x: 0.5,
      y: 0.5,
      w: 0.5,
      h: 0.5,
    })
  })

  it('has nowhere to go at fit-whole', () => {
    expect(panBy(WHOLE, 0.3, 0.3)).toEqual(WHOLE)
  })
})

describe('wheelBy', () => {
  const STEP = 1.15
  const wheel = (deltaY: number, deltaMode = 0, ctrlKey = false) => ({
    deltaY,
    deltaMode,
    ctrlKey,
  })

  it('is exactly one step for one detent, in each deltaMode', () => {
    expect(wheelBy(wheel(-100, 0), STEP)).toBeCloseTo(STEP, 12)
    expect(wheelBy(wheel(-3, 1), STEP)).toBeCloseTo(STEP, 12)
    expect(wheelBy(wheel(-1, 2), STEP)).toBeCloseTo(STEP, 12)
  })

  it('inverts for a downward wheel', () => {
    expect(wheelBy(wheel(100, 0), STEP)).toBeCloseTo(1 / STEP, 12)
    expect(wheelBy(wheel(3, 1), STEP)).toBeCloseTo(1 / STEP, 12)
  })

  // The defect this replaced: `deltaY < 0 ? step : 1 / step` gave a full step
  // to each of these, so the fifteen a trackpad flick emits multiplied to
  // 1.15 ** 15 ≈ 8.14 and pinned the view at the default `maxScale` of 8. The
  // whole gesture is one notch's worth of pixels, so it is one step.
  it('spreads a gesture over its events rather than giving each a full step', () => {
    const flick = Array.from({ length: 15 }, () =>
      wheelBy(wheel(-100 / 15), STEP),
    )
    expect(flick.reduce((a, b) => a * b, 1)).toBeCloseTo(STEP, 10)
    // Each event on its own is nowhere near a step.
    expect(flick[0]).toBeLessThan(1.01)
  })

  // The same physical detent, reported two ways. A single divisor would have
  // made line mode about thirty times faster.
  it('makes pixels and lines agree for the same detent', () => {
    expect(wheelBy(wheel(-100, 0), STEP)).toBeCloseTo(
      wheelBy(wheel(-3, 1), STEP),
      12,
    )
  })

  it('caps one pathological event at four notches', () => {
    expect(wheelBy(wheel(-100_000, 0), STEP)).toBeCloseTo(STEP ** 4, 10)
    expect(wheelBy(wheel(100_000, 0), STEP)).toBeCloseTo(STEP ** -4, 10)
  })

  // A pinch's deltas are an order of magnitude smaller, so it gets an order of
  // magnitude smaller divisor — 10 px to the notch rather than 100.
  it('reads a pinch faster than a scroll of the same size', () => {
    expect(wheelBy(wheel(-10, 0, true), STEP)).toBeCloseTo(STEP, 12)
    expect(wheelBy(wheel(-10, 0, false), STEP)).toBeCloseTo(STEP ** 0.1, 12)
  })

  // ctrl held on a real mouse is indistinguishable from a pinch except by
  // size. Capping a pinch at one notch is what keeps that detent worth the
  // same as the one without ctrl, instead of ten times it.
  it('gives ctrl+wheel from a mouse the same detent as a plain wheel', () => {
    expect(wheelBy(wheel(-100, 0, true), STEP)).toBeCloseTo(
      wheelBy(wheel(-100, 0, false), STEP),
      12,
    )
  })

  // ctrl in line or page mode is not a pinch — no trackpad reports one that
  // way — so it takes the ordinary rate for its mode.
  it('treats ctrl outside pixel mode as an ordinary wheel', () => {
    expect(wheelBy(wheel(-3, 1, true), STEP)).toBeCloseTo(STEP, 12)
  })

  it('asks for no zoom at all from a gesture it cannot read', () => {
    expect(wheelBy(wheel(0), STEP)).toBe(1)
    expect(wheelBy(wheel(Number.NaN), STEP)).toBe(1)
    expect(wheelBy(wheel(-100), Number.NaN)).toBe(1)
    expect(wheelBy(wheel(-100), 0)).toBe(1)
    expect(wheelBy(wheel(-100), -1)).toBe(1)
  })

  // `step` of 1 is a reader who has turned the zoom rate down to nothing, not
  // an error: every event asks for no change rather than for NaN.
  it('is inert for a step of one', () => {
    expect(wheelBy(wheel(-543, 0), 1)).toBe(1)
  })

  // The composition this exists for: the factor is `zoomAt`'s `by`, and an
  // unreadable gesture has to leave the rect exactly alone rather than move it
  // a little.
  it('leaves the rect untouched through zoomAt when it declines', () => {
    const rect = { x: 0.25, y: 0.25, w: 0.5, h: 0.5 }
    expect(
      zoomAt(rect, { x: 0.5, y: 0.5 }, wheelBy(wheel(0), STEP), 8),
    ).toEqual(rect)
  })
})

describe('imageToGroup / groupToImage', () => {
  const rect = { x: 0.2, y: 0.3, w: 0.4, h: 0.5 }

  it('is identity when one image fills the group', () => {
    expect(imageToGroup(rect, 'only')).toEqual(rect)
    expect(groupToImage(rect, 'only')).toEqual(rect)
  })

  it('puts the first cell in the left half of the group', () => {
    expect(imageToGroup(rect, 'first')).toEqual({
      x: 0.1,
      y: 0.3,
      w: 0.2,
      h: 0.5,
    })
  })

  it('puts the second cell in the right half of the group', () => {
    expect(imageToGroup(rect, 'second')).toEqual({
      x: 0.6,
      y: 0.3,
      w: 0.2,
      h: 0.5,
    })
  })

  it('round-trips through both cells', () => {
    for (const cell of ['only', 'first', 'second'] as const) {
      const there = imageToGroup(rect, cell)
      const back = groupToImage(there, cell)
      expect(back.x).toBeCloseTo(rect.x, 10)
      expect(back.y).toBeCloseTo(rect.y, 10)
      expect(back.w).toBeCloseTo(rect.w, 10)
      expect(back.h).toBeCloseTo(rect.h, 10)
    }
  })

  it('leaves the vertical axis alone — cells split the width only', () => {
    expect(imageToGroup(rect, 'second').y).toBe(rect.y)
    expect(imageToGroup(rect, 'second').h).toBe(rect.h)
  })
})

describe('paintedIn', () => {
  it('fills the box when the aspects match', () => {
    expect(paintedIn(1.5, 1.5)).toEqual({ x: 0, y: 0, w: 1, h: 1 })
  })

  it('insets a portrait page in a landscape box, centred, down both sides', () => {
    // page 0.5 (tall) in a box of 1.0: the page is half as wide as the box.
    expect(paintedIn(0.5, 1)).toEqual({ x: 0.25, y: 0, w: 0.5, h: 1 })
  })

  it('insets a landscape page in a portrait box, centred, top and bottom', () => {
    expect(paintedIn(1, 0.5)).toEqual({ x: 0, y: 0.25, w: 1, h: 0.5 })
  })

  it('is the whole box when an aspect has not been measured yet', () => {
    expect(paintedIn(0, 1)).toEqual({ x: 0, y: 0, w: 1, h: 1 })
    expect(paintedIn(1, Number.NaN)).toEqual({ x: 0, y: 0, w: 1, h: 1 })
  })
})

describe('into / outOf', () => {
  const inner = { x: 0.25, y: 0, w: 0.5, h: 1 }
  const rect = { x: 0.2, y: 0.4, w: 0.4, h: 0.2 }

  it('places an inner-box rect in the outer box', () => {
    expect(into(rect, inner)).toEqual({ x: 0.35, y: 0.4, w: 0.2, h: 0.2 })
  })

  it('round-trips', () => {
    const back = outOf(into(rect, inner), inner)
    expect(back.x).toBeCloseTo(rect.x, 10)
    expect(back.y).toBeCloseTo(rect.y, 10)
    expect(back.w).toBeCloseTo(rect.w, 10)
    expect(back.h).toBeCloseTo(rect.h, 10)
  })

  it('is identity for the whole box', () => {
    expect(into(rect, WHOLE)).toEqual(rect)
    expect(outOf(rect, WHOLE)).toEqual(rect)
  })

  it('leaves a rect alone rather than dividing by a collapsed box', () => {
    expect(outOf(rect, { x: 0, y: 0, w: 0, h: 1 })).toEqual(rect)
  })
})

describe('mapRect', () => {
  const rect = { x: 0.25, y: 0.25, w: 0.5, h: 0.5 }
  const square = { w: 1000, h: 1000 }
  const tall = { w: 700, h: 1000 }

  it('identity means the same fractions mean the same content', () => {
    expect(mapRect(rect, square, tall, 'identity')).toEqual(rect)
  })

  it('reset throws the view away and shows the whole page', () => {
    expect(mapRect(rect, square, tall, 'reset')).toEqual(WHOLE)
  })

  it('pillarbox is identity when both frames share an aspect', () => {
    const after = mapRect(
      rect,
      { w: 700, h: 1000 },
      { w: 1400, h: 2000 },
      'pillarbox',
    )
    expect(after.x).toBeCloseTo(rect.x, 10)
    expect(after.y).toBeCloseTo(rect.y, 10)
    expect(after.w).toBeCloseTo(rect.w, 10)
    expect(after.h).toBeCloseTo(rect.h, 10)
  })

  // The target is the wider-aspect frame, so the shared content occupies a
  // narrower band of its *width*, centred. A rect at the source's centre stays
  // centred.
  it('pillarbox keeps a centred rect centred', () => {
    const centred = { x: 0.25, y: 0.25, w: 0.5, h: 0.5 }
    const after = mapRect(centred, tall, square, 'pillarbox')
    expect(after.x + after.w / 2).toBeCloseTo(0.5, 10)
    expect(after.y + after.h / 2).toBeCloseTo(0.5, 10)
  })

  // The whole reason the candidate exists. The wider-aspect (square) frame is
  // the one carrying margin, so the shared content occupies less of its
  // *width*; coming out of it into a narrower frame, the rect grows
  // horizontally and the vertical axis is left alone.
  it('pillarbox widens a rect leaving the frame that carries the margin', () => {
    const after = mapRect(rect, square, tall, 'pillarbox')
    expect(after.w).toBeGreaterThan(rect.w)
    expect(after.y).toBe(rect.y)
    expect(after.h).toBe(rect.h)
  })

  // The other direction from the test above: source is the narrower (tall)
  // frame, target is the wider (square) one, so the rect now enters the
  // frame that carries the margin and shrinks horizontally instead of growing.
  // Only the round-trip test below exercised this direction before — a
  // symmetric error there would cancel, which is precisely how an earlier
  // transposition bug survived its own round-trip test.
  it('pillarbox shrinks a rect entering the frame that carries the margin', () => {
    const after = mapRect(rect, tall, square, 'pillarbox')
    expect(after).toEqual({ x: 0.325, y: 0.25, w: 0.35, h: 0.5 })
    expect(after.w).toBeLessThan(rect.w)
    expect(after.y).toBe(rect.y)
    expect(after.h).toBe(rect.h)
  })

  // An involution, and this is the test that pins it: without it a swap and a
  // swap back would walk the view a little further off the page each time.
  it('pillarbox round-trips back to where it started', () => {
    const there = mapRect(rect, tall, square, 'pillarbox')
    const back = mapRect(there, square, tall, 'pillarbox')
    expect(back.x).toBeCloseTo(rect.x, 10)
    expect(back.y).toBeCloseTo(rect.y, 10)
    expect(back.w).toBeCloseTo(rect.w, 10)
    expect(back.h).toBeCloseTo(rect.h, 10)
  })

  // Which axis moves, pinned on its own and swept both ways round. The three
  // tests above name a direction each, and the round-trip above pins none: an
  // involution cancels a transposed axis exactly as cleanly as a correct one,
  // which is how this function shipped `Math.max` for `Math.min` with a green
  // suite. So the vertical is asserted *identical* rather than close —
  // `content` is the narrower of the two aspects, so `ch` is 1 for both frames
  // and `axis` hands `y` and `h` straight back — and the horizontal scale is
  // asserted to be `aFrom / aTo`, which is the relation `docs/notes.md` found
  // in twelve corpus volumes of twelve.
  it('pillarbox scales the horizontal by the aspect ratio and the vertical not at all', () => {
    // Two hand-picked shapes, then Kaguya's own pair: `docs/notes.md` measures
    // its raw at 0.703 against the English 0.667. Off-centre on both axes,
    // because a centred rect maps to itself under a centred scaling and so
    // cannot tell an offset apart from no offset at all.
    const off = { x: 0.1, y: 0.2, w: 0.3, h: 0.4 }
    const frames = [tall, square, { w: 703, h: 1000 }, { w: 667, h: 1000 }]
    for (const from of frames) {
      for (const to of frames) {
        const after = mapRect(off, from, to, 'pillarbox')
        expect(after.y).toBe(off.y)
        expect(after.h).toBe(off.h)
        const ratio = from.w / from.h / (to.w / to.h)
        expect(after.w).toBeCloseTo(off.w * ratio, 12)
        // The offset from the page's centre scales by the same ratio, which is
        // what "split evenly between the two edges" comes to arithmetically.
        expect(after.x + after.w / 2 - 0.5).toBeCloseTo(
          (off.x + off.w / 2 - 0.5) * ratio,
          12,
        )
      }
    }
  })

  it('leaves the rect alone when a frame has not decoded yet', () => {
    expect(mapRect(rect, { w: 0, h: 0 }, tall, 'pillarbox')).toEqual(rect)
    expect(mapRect(rect, tall, { w: 0, h: 0 }, 'identity')).toEqual(rect)
  })

  it('leaves the rect alone for a negative frame dimension', () => {
    expect(mapRect(rect, { w: -700, h: 1000 }, tall, 'pillarbox')).toEqual(rect)
    expect(mapRect(rect, tall, { w: 700, h: -1000 }, 'pillarbox')).toEqual(rect)
  })

  // `NaN` and `Infinity` both pass `w <= 0`/`h <= 0` — that comparison is
  // false for either — so the zero/negative guard alone lets them through to
  // a division that produces an all-`NaN` rect and, downstream, a CSS
  // `scale(NaN)` that vanishes the page with no error anywhere. Same failure
  // `outOf`'s doc warns about; `fitRect` and `paintedIn` already guard it.
  it('leaves the rect alone for a non-finite frame dimension', () => {
    expect(
      mapRect(rect, { w: Number.NaN, h: Number.NaN }, tall, 'pillarbox'),
    ).toEqual(rect)
    expect(
      mapRect(
        rect,
        { w: Number.POSITIVE_INFINITY, h: 1000 },
        tall,
        'pillarbox',
      ),
    ).toEqual(rect)
    expect(
      mapRect(rect, tall, { w: Number.NaN, h: Number.NaN }, 'pillarbox'),
    ).toEqual(rect)
  })
})

describe('mapRect frames whose aspect is not finite', () => {
  const rect = { x: 0.25, y: 0.25, w: 0.5, h: 0.5 }
  const tall = { w: 700, h: 1000 }

  // Both dimensions are finite and positive, so a guard that checked only
  // them lets these through; the *derived* aspect then overflows or
  // underflows and divides its way back to NaN. Not reachable from a real
  // naturalWidth/naturalHeight, which are bounded integers — pinned because
  // the doc comment claims `ch` is identically 1, and a claim that absolute
  // deserves its boundary written down. Which of the two hides behind a
  // healthy partner and which poisons `content` directly swapped over when
  // `content` became the `min` of the two aspects; both still arrive here.
  it('leaves the rect alone when a frame overflows to an infinite aspect', () => {
    expect(mapRect(rect, { w: 1e308, h: 1e-10 }, tall, 'pillarbox')).toEqual(
      rect,
    )
  })

  it('leaves the rect alone when a frame underflows to a zero aspect', () => {
    expect(mapRect(rect, { w: 1e-310, h: 1e308 }, tall, 'pillarbox')).toEqual(
      rect,
    )
  })
})

describe('mapView', () => {
  // A landscape page area holding portrait pages, which is the shape every
  // measurement in `docs/notes.md` describes: Kaguya's raw is 0.703 and the
  // English 0.667, both well under any reader window's own aspect.
  const AREA = 2
  const tall: Frame = { w: 700, h: 1000 }
  const square: Frame = { w: 1000, h: 1000 }
  const page = (frame: Frame, cell: Cell): Drawn => ({ frame, cell })

  it('leaves a rect where it was when both sides are the same page', () => {
    const rect = { x: 0.4, y: 0.4, w: 0.2, h: 0.2 }
    const after = mapView(
      rect,
      page(tall, 'only'),
      page(tall, 'only'),
      AREA,
      'identity',
    )
    expect(after.x).toBeCloseTo(rect.x, 12)
    expect(after.y).toBeCloseTo(rect.y, 12)
    expect(after.w).toBeCloseTo(rect.w, 12)
    expect(after.h).toBeCloseTo(rect.h, 12)
  })

  // The cell conversion doing its job: in `two` mode the outgoing page fills
  // the left half of the group, so its own centre sits at group `x = 0.25`.
  // Swapping onto an edition that draws that slot alone must put the reader
  // back over the middle of the page, which is now the middle of the group.
  // Without `imageToGroup`/`groupToImage` here the view would land half a
  // page out — the failure this test exists to catch.
  it('carries the page centre across a swap out of a pair onto a whole page', () => {
    const after = mapView(
      { x: 0.2, y: 0.45, w: 0.1, h: 0.1 },
      page(tall, 'first'),
      page(tall, 'only'),
      AREA,
      'identity',
    )
    expect(after.x + after.w / 2).toBeCloseTo(0.5, 12)
    expect(after.y + after.h / 2).toBeCloseTo(0.5, 12)
  })

  // Throwing the view away is fit-whole of the *group*. Falling through the
  // composition instead would land on the incoming page's own box, which in
  // `two` mode is half the group — a "reset" that leaves the reader at 2x.
  it('is the whole group for reset, whatever the cells and frames say', () => {
    expect(
      mapView(
        { x: 0.2, y: 0.45, w: 0.1, h: 0.1 },
        page(tall, 'first'),
        page(square, 'second'),
        AREA,
        'reset',
      ),
    ).toEqual(WHOLE)
  })

  // `Rect`'s invariant, which `Viewport` turns into a uniform `scale` — and
  // which every step of this composition but the last two is entitled to
  // break. Swept rather than spot-checked because the axis `fitRect` grows
  // depends on which side of the viewport's aspect the mapped rect lands.
  it('always comes back with w === h, which is what makes it fit to hold', () => {
    for (const area of [0.75, 1, 1.5, 2, 3]) {
      for (const from of [
        page(tall, 'only'),
        page(square, 'first'),
        page(tall, 'second'),
      ]) {
        for (const to of [page(square, 'only'), page(tall, 'first')]) {
          for (const kind of ['identity', 'pillarbox'] as const) {
            const after = mapView(
              { x: 0.1, y: 0.7, w: 0.2, h: 0.2 },
              from,
              to,
              area,
              kind,
            )
            expect(after.w).toBeCloseTo(after.h, 12)
          }
        }
      }
    }
  })

  // The two candidates part company on the horizontal axis, which is
  // `docs/notes.md`'s measurement stated in group space: `pillarbox` makes the
  // *narrower* of the two aspects the shared content's, so the vertical axis is
  // never rescaled and the wider frame carries its margin at left and right.
  // The rect is deliberately left of centre and inside the drawn page — a
  // centred one maps to itself under a centred scaling, so it cannot tell the
  // two apart, and one straddling the page's edge would be partly over
  // background.
  //
  // Centres rather than corners, and only the vertical one is compared
  // directly: `fitRect` grows whichever axis is short of the page area's
  // aspect, and here it grows identity's vertical and neither of pillarbox's,
  // so `w` and `h` legitimately differ between the two. It grows about the
  // rect's own centre, which is why the centres are the like-for-like figures.
  it('separates identity from pillarbox horizontally and only horizontally', () => {
    const rect = { x: 0.35, y: 0.4, w: 0.1, h: 0.1 }
    const from = page(tall, 'only')
    const to = page(square, 'only')
    const plain = mapView(rect, from, to, AREA, 'identity')
    const boxed = mapView(rect, from, to, AREA, 'pillarbox')
    expect(boxed.y + boxed.h / 2).toBeCloseTo(plain.y + plain.h / 2, 12)
    expect(boxed.y + boxed.h / 2).toBeCloseTo(0.45, 10)
    // The incoming frame is the wider-aspect one, so it is the one carrying
    // left and right margin; mapping into it shrinks the rect and pulls a
    // point left of centre back toward centre.
    expect(boxed.x + boxed.w / 2).toBeGreaterThan(plain.x + plain.w / 2)
    expect(boxed.x + boxed.w / 2).toBeCloseTo(0.4, 10)
    expect(plain.x + plain.w / 2).toBeCloseTo(0.3571428571428, 10)
  })

  it('leaves the rect alone when the incoming page has not decoded', () => {
    const rect = { x: 0.4, y: 0.4, w: 0.2, h: 0.2 }
    expect(
      mapView(
        rect,
        page(tall, 'only'),
        page({ w: 0, h: 0 }, 'only'),
        AREA,
        'pillarbox',
      ),
    ).toEqual(rect)
    expect(
      mapView(
        rect,
        page({ w: 0, h: 0 }, 'only'),
        page(tall, 'only'),
        AREA,
        'identity',
      ),
    ).toEqual(rect)
  })

  // A page area mid-teardown, or one measured before layout. `fitRect` would
  // return the rect unfitted here, and an unfitted rect is one `Viewport` may
  // draw stretched — so the whole composition stands down instead.
  it('leaves the rect alone when the page area has no aspect', () => {
    const rect = { x: 0.4, y: 0.4, w: 0.2, h: 0.2 }
    expect(
      mapView(rect, page(tall, 'only'), page(tall, 'only'), 0, 'pillarbox'),
    ).toEqual(rect)
    expect(
      mapView(
        rect,
        page(tall, 'only'),
        page(tall, 'only'),
        Number.NaN,
        'identity',
      ),
    ).toEqual(rect)
  })
})

// Where one edition joins a spread and the other splits it. The slot model
// gets the reader to the right *page* and stops there — both halves of a
// joined spread are drawn as one image and share one coordinate space
// (`notes.ts`'s `Region`) — so landing on the right half is the transform's
// job, which is what `Drawn.half` is for.
//
// Every figure below is derived from `paintedIn` and `halfBox`, and the
// derivation is written beside each so a reader can check it rather than trust
// it. In a 2:1 page area: the 1.4-wide spread paints at `{x: 0.15, w: 0.7}`,
// so its left half's centre is group `0.15 + 0.25 * 0.7 = 0.325` and its right
// half's is `0.675`; the 0.7-wide single page paints at `{x: 0.325, w: 0.35}`
// and its own centre is group `0.5`; the 2-wide spread paints edge to edge, so
// its half centres are simply `0.25` and `0.75`.
describe('mapView across a joined spread', () => {
  const AREA = 2
  // Two 700×1000 pages printed as one image. Twice the width of its
  // neighbours, which is not a convenience: `docs/notes.md` records every one
  // of the corpus's 28 spreads at exactly that, and it is why a half is a flag
  // rather than a stored geometry.
  const SPREAD: Frame = { w: 1400, h: 1000 }
  // One of those pages, as the edition that splits the spread prints it.
  const PAGE: Frame = { w: 700, h: 1000 }
  // A second edition's spread, at a different aspect — and 2:1, so it paints
  // edge to edge in a 2:1 page area and keeps a test about the half rather
  // than about the letterbox around it.
  const WIDE: Frame = { w: 2000, h: 1000 }
  const SQUARE: Frame = { w: 1000, h: 1000 }
  // One image filling its group, which is what `pairing.ts` makes a joined
  // spread in every mode: it forms its own group, and `cellsFor` renders that
  // group `wide`.
  const only = (frame: Frame, half?: 'L' | 'R'): Drawn => ({
    frame,
    cell: 'only',
    half,
  })

  // The requirement that made the two half steps unconditional in `mapView`
  // rather than guarded by an `if`: `halfBox(undefined)` is `WHOLE`, and
  // `into`/`outOf` against `WHOLE` reduce to `x + f * 1` and `(x - 0) / 1`,
  // which are identities in floating point rather than approximations of one.
  // So this is `toEqual` and not `toBeCloseTo` — drift too small for the rest
  // of this file to see would still be drift on every ordinary swap, which is
  // the only thing an `if` would have bought.
  //
  // Deliberately all-dyadic, so the assertion is about the new steps rather
  // than about binary fractions: a square page paints at `{x: 0.25, w: 0.5}`
  // in a 2:1 area, and 0.375 and 0.25 divide by 0.5 without remainder.
  it('leaves a slot with no half exactly where it was, to the bit', () => {
    const rect = { x: 0.375, y: 0.25, w: 0.25, h: 0.25 }
    for (const kind of ['identity', 'pillarbox'] as const) {
      expect(mapView(rect, only(SQUARE), only(SQUARE), AREA, kind)).toEqual(
        rect,
      )
    }
  })

  // The defect this closes, in the direction the corpus reaches it: `en-mono`
  // joins Go-Toubun v01's page 4 where the raw prints two pages. A reader
  // zoomed into a balloon on one half of the joined image presses the swap and
  // must land on the page that half *is*.
  //
  // The last assertion is what makes this a test of which half rather than of
  // whether the rect landed on the page at all — mapping onto the wrong one is
  // precisely the bug, and a rect merely inside the page would pass either way.
  // The same visible rect read as the other half lands a third of the group
  // out, at `0.15`.
  it('carries one half of a joined spread onto the page the other edition prints it as', () => {
    // Over the left half's centre, `w === h` as real view state always is.
    const onLeft = { x: 0.275, y: 0.45, w: 0.1, h: 0.1 }
    const left = mapView(
      onLeft,
      only(SPREAD, 'L'),
      only(PAGE),
      AREA,
      'identity',
    )
    expect(left.x + left.w / 2).toBeCloseTo(0.5, 12)
    expect(left.y + left.h / 2).toBeCloseTo(0.5, 12)

    // A different rect, over the right half's centre, and the same answer.
    const onRight = { x: 0.625, y: 0.45, w: 0.1, h: 0.1 }
    const right = mapView(
      onRight,
      only(SPREAD, 'R'),
      only(PAGE),
      AREA,
      'identity',
    )
    expect(right.x + right.w / 2).toBeCloseTo(0.5, 12)

    const asOther = mapView(
      onLeft,
      only(SPREAD, 'R'),
      only(PAGE),
      AREA,
      'identity',
    )
    expect(asOther.x + asOther.w / 2).toBeCloseTo(0.15, 12)
  })

  // The other direction, which the reader reaches by swapping back. The rect
  // is the whole of the single page's centre; the answer is the centre of one
  // half of the spread, and the two halves are a third of the group apart.
  //
  // The third assertion is the defect itself, kept as a test rather than as a
  // memory: with no half the spread's extra width reads as margin at both
  // edges, and the page's centre maps to the *spread's* centre — the fold
  // between the two pages rather than the middle of either.
  it('lands on the half that holds the content, not on the fold', () => {
    const centred = { x: 0.45, y: 0.45, w: 0.1, h: 0.1 }
    const right = mapView(
      centred,
      only(PAGE),
      only(SPREAD, 'R'),
      AREA,
      'identity',
    )
    expect(right.x + right.w / 2).toBeCloseTo(0.675, 12)
    expect(right.y + right.h / 2).toBeCloseTo(0.5, 12)

    const left = mapView(
      centred,
      only(PAGE),
      only(SPREAD, 'L'),
      AREA,
      'identity',
    )
    expect(left.x + left.w / 2).toBeCloseTo(0.325, 12)

    const onFold = mapView(centred, only(PAGE), only(SPREAD), AREA, 'identity')
    expect(onFold.x + onFold.w / 2).toBeCloseTo(0.5, 12)
  })

  // Both editions join the spread, at different widths — the case where the
  // half boxes have to come out and go back in the *same* space, image
  // fractions, rather than cancelling. The reader is over the centre of the
  // 2:1 spread's left half (group 0.25, since that one paints edge to edge),
  // and lands on the centre of whichever half of the 1.4-wide spread the slot
  // names: 0.675 for the right, 0.325 for the left.
  it('carries a half onto the matching half of the other edition', () => {
    const onLeft = { x: 0.2, y: 0.45, w: 0.1, h: 0.1 }
    const right = mapView(
      onLeft,
      only(WIDE, 'L'),
      only(SPREAD, 'R'),
      AREA,
      'identity',
    )
    expect(right.x + right.w / 2).toBeCloseTo(0.675, 12)
    expect(right.y + right.h / 2).toBeCloseTo(0.5, 12)

    const left = mapView(
      onLeft,
      only(WIDE, 'L'),
      only(SPREAD, 'L'),
      AREA,
      'identity',
    )
    expect(left.x + left.w / 2).toBeCloseTo(0.325, 12)
  })

  // The second half of the fix, and the one the half boxes alone do not give.
  // `mapRect` reconciles two *aspects*, and a joined spread's is twice its
  // half's — so handed the whole image it would read that doubling as margin
  // the wider side carries at its edges and correct for something that is not
  // there.
  //
  // Here the outgoing half is 1000×1000 and the incoming page is 1000×1000, so
  // there is nothing for `pillarbox` to correct and it must agree with
  // `identity` to the bit. Handed the spread's own 2000×1000 it would rescale
  // the horizontal by two about the half's own centre — the half box is
  // already applied, so the centre is right and everything else is not — and
  // put the answer at 0.2 instead of 0.35. Not a rounding difference, which is
  // what makes `toEqual` safe here.
  it('gives mapRect the frame of the half rather than of the whole spread', () => {
    const onRight = { x: 0.55, y: 0.4, w: 0.1, h: 0.1 }
    const boxed = mapView(
      onRight,
      only(WIDE, 'R'),
      only(SQUARE),
      AREA,
      'pillarbox',
    )
    const plain = mapView(
      onRight,
      only(WIDE, 'R'),
      only(SQUARE),
      AREA,
      'identity',
    )
    expect(boxed).toEqual(plain)
    expect(boxed.x + boxed.w / 2).toBeCloseTo(0.35, 12)
    expect(boxed.y + boxed.h / 2).toBeCloseTo(0.45, 12)
  })

  // `Rect`'s invariant again, over combinations the sweep in the block above
  // does not reach. The half steps are two more that may break `w === h` —
  // `outOf` against a box half as wide doubles one axis and leaves the other —
  // and `fitRect` against the page area's aspect twice has to stay the last
  // thing that happens.
  //
  // A `half` beside `'first'`/`'second'` is in the sweep although `pairing.ts`
  // draws a joined spread as one wide image in every mode, so that combination
  // does not arise today. It is here because `Drawn`'s doc claims the two are
  // independent boxes in different spaces, and an untested claim about
  // independence is the kind this file has been wrong about before.
  it('still comes back with w === h when a half is involved', () => {
    for (const area of [0.75, 1, 2, 3]) {
      for (const from of [
        only(SPREAD, 'L'),
        only(WIDE, 'R'),
        only(PAGE),
        { frame: SPREAD, cell: 'second', half: 'R' } as Drawn,
      ]) {
        for (const to of [only(SPREAD, 'R'), only(SQUARE), only(PAGE, 'L')]) {
          for (const kind of ['identity', 'pillarbox'] as const) {
            const after = mapView(
              { x: 0.1, y: 0.7, w: 0.2, h: 0.2 },
              from,
              to,
              area,
              kind,
            )
            expect(after.w).toBeCloseTo(after.h, 12)
          }
        }
      }
    }
  })
})
