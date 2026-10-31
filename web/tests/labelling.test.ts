import { describe, expect, it } from 'vitest'
import {
  newBox,
  boxesAt,
  cycleAt,
  removeBox,
  setAxis,
  setKind,
  toggleIgnore,
  type LabelBox,
} from '../src/labelling'

const box = (id: string, x: number, y: number, w = 0.1, h = 0.1): LabelBox => ({
  id,
  x,
  y,
  w,
  h,
  kind: 'dialogue',
  axis: 'v',
  ignore: false,
})

describe('newBox', () => {
  it('appends a box with a fresh id and the default kind', () => {
    const after = newBox({ x: 0.1, y: 0.2, w: 0.3, h: 0.4 })
    expect(after).toMatchObject({
      x: 0.1,
      y: 0.2,
      w: 0.3,
      h: 0.4,
      kind: 'dialogue',
      ignore: false,
    })
    expect(after?.id).toBeTruthy()
  })

  it('normalises a rectangle dragged up and to the left', () => {
    // Dragging from bottom-right to top-left gives a negative width. A box
    // with a negative extent would fail every containment test downstream.
    const after = newBox({ x: 0.5, y: 0.6, w: -0.2, h: -0.3 })
    expect(after).toMatchObject({ x: 0.3, y: 0.3, w: 0.2, h: 0.3 })
  })

  it('refuses a degenerate drag rather than storing a zero-area box', () => {
    expect(newBox({ x: 0.5, y: 0.5, w: 0, h: 0.2 })).toBeUndefined()
  })

  it('clamps a drag that ended in the letterbox to the page', () => {
    // `PageView` letterboxes the page, so a drag that leaves the image reads
    // as a fraction outside 0..1 — natural when boxing a caption that bleeds
    // to the page edge. Stored unclamped, the box carries area that is not on
    // the page, and `score.rs`'s `classify_single_cover` divides by exactly
    // that area: the truth side of the ratio is diluted, a genuine 1:1
    // demotes to `Fragment` or `Missed`, and its prediction turns `spurious`.
    // Both terms of the promotion rule move, in the punishing direction, with
    // nothing anywhere saying so.
    const after = newBox({ x: 0.8, y: 0.9, w: 0.5, h: 0.4 })
    expect(after!.x).toBe(0.8)
    expect(after!.y).toBe(0.9)
    expect(after!.x + after!.w).toBeLessThanOrEqual(1)
    expect(after!.y + after!.h).toBeLessThanOrEqual(1)
    expect(after!.w).toBeCloseTo(0.2, 12)
    expect(after!.h).toBeCloseTo(0.1, 12)
  })

  it('clamps a drag that started in the letterbox to the page', () => {
    // The same failure at the near edge, and it needs its own case: a drag
    // starting above and left of the image clamps the *origin*, which is the
    // half that `Math.min`/`Math.abs` normalisation left alone entirely.
    const after = newBox({ x: -0.3, y: -0.2, w: 0.5, h: 0.4 })
    expect(after!.x).toBe(0)
    expect(after!.y).toBe(0)
    expect(after!.w).toBeCloseTo(0.2, 12)
    expect(after!.h).toBeCloseTo(0.2, 12)
  })

  it('stores a box drawn wholly on the page exactly as drawn', () => {
    // The clamp must not become a quiet rewrite. Recomputing the extent as a
    // difference of two clamped fractions — which is what the reader's
    // `regionFromDrag` does, correctly, for a drag expressed as two points —
    // would turn this `0.3` into `0.30000000000000004` on a box that never
    // left the page.
    const after = newBox({ x: 0.1, y: 0.2, w: 0.3, h: 0.4 })
    expect(after).toMatchObject({ x: 0.1, y: 0.2, w: 0.3, h: 0.4 })
  })

  it('drops a drag that never touched the page at all', () => {
    // Wholly in the letterbox. Clamping takes it to zero area, and a
    // zero-area box is a click that missed — it names nothing on the page.
    expect(newBox({ x: -0.4, y: -0.4, w: 0.2, h: 0.2 })).toBeUndefined()
  })
})

describe('newBox takes carried defaults', () => {
  it('takes the kind it is given rather than always dialogue', () => {
    expect(
      newBox({ x: 0.1, y: 0.1, w: 0.2, h: 0.2 }, { kind: 'sfx', axis: 'v' })
        ?.kind,
    ).toBe('sfx')
  })

  it('still defaults to dialogue reading vertically when none are named', () => {
    const box = newBox({ x: 0.1, y: 0.1, w: 0.2, h: 0.2 })
    expect(box?.kind).toBe('dialogue')
    expect(box?.axis).toBe('v')
  })

  it('takes the axis it is given, not-applicable included', () => {
    const box = newBox(
      { x: 0.1, y: 0.1, w: 0.2, h: 0.2 },
      { kind: 'sfx', axis: null },
    )
    expect(box?.axis).toBeNull()
  })

  it('never carries ignore, which would drop boxes out of scoring unseen', () => {
    expect(
      newBox({ x: 0.1, y: 0.1, w: 0.2, h: 0.2 }, { kind: 'sfx', axis: null })
        ?.ignore,
    ).toBe(false)
  })
})

describe('editing', () => {
  it('sets a kind on one box and leaves the others', () => {
    const before = [box('a', 0, 0), box('b', 0.5, 0.5)]
    const after = setKind(before, 'a', 'sfx')
    expect(after[0].kind).toBe('sfx')
    expect(after[1].kind).toBe('dialogue')
  })

  it('sets an axis, including not-applicable', () => {
    expect(setAxis([box('a', 0, 0)], 'a', null)[0].axis).toBeNull()
    expect(setAxis([box('a', 0, 0)], 'a', 'h')[0].axis).toBe('h')
  })

  it('toggles ignore both ways', () => {
    const once = toggleIgnore([box('a', 0, 0)], 'a')
    expect(once[0].ignore).toBe(true)
    expect(toggleIgnore(once, 'a')[0].ignore).toBe(false)
  })

  it('removes by id', () => {
    expect(removeBox([box('a', 0, 0), box('b', 0.5, 0.5)], 'a')).toHaveLength(1)
  })

  it('does not mutate its input', () => {
    const before = [box('a', 0, 0)]
    setKind(before, 'a', 'logo')
    expect(before[0].kind).toBe('dialogue')
  })
})

describe('cycleAt', () => {
  // Sound effects routinely sit on top of balloons. Without a way through the
  // stack the box underneath cannot be selected at all, which is why there is
  // no sidebar list to fall back on.
  const stack = [
    box('under', 0.1, 0.1, 0.4, 0.4),
    box('over', 0.2, 0.2, 0.1, 0.1),
  ]

  it('finds every box containing the point, topmost first', () => {
    expect(boxesAt(stack, { x: 0.25, y: 0.25 }).map((b) => b.id)).toEqual([
      'over',
      'under',
    ])
  })

  it('starts at the topmost when nothing is selected', () => {
    expect(cycleAt(stack, { x: 0.25, y: 0.25 }, undefined)).toBe('over')
  })

  it('steps to the next box down, then wraps', () => {
    expect(cycleAt(stack, { x: 0.25, y: 0.25 }, 'over')).toBe('under')
    expect(cycleAt(stack, { x: 0.25, y: 0.25 }, 'under')).toBe('over')
  })

  it('answers undefined where no box holds the point', () => {
    expect(cycleAt(stack, { x: 0.9, y: 0.9 }, undefined)).toBeUndefined()
  })

  it('starts over when the selected box does not hold the point', () => {
    expect(cycleAt(stack, { x: 0.15, y: 0.15 }, 'over')).toBe('under')
  })
})
