import { describe, expect, it } from 'vitest'
import { PAGE_STOP, ringStep, type RingSizes, type RingStop } from './ring'

const bars = (index: number): RingStop => ({ region: 'bars', index })
const controls = (index: number): RingStop => ({ region: 'controls', index })

/** Walk `steps` presses from `from`, collecting every stop reached. */
function walk(
  from: RingStop,
  sizes: RingSizes,
  steps: number,
  backwards = false,
): RingStop[] {
  const reached: RingStop[] = []
  let at = from
  for (let i = 0; i < steps; i++) {
    at = ringStep(at, sizes, backwards)
    reached.push(at)
  }
  return reached
}

describe('ringStep (spec 2026-09-26 §3.1)', () => {
  const plain: RingSizes = { bars: 3, controls: 0 }
  const withControls: RingSizes = { bars: 3, controls: 2 }

  it('walks the page, the bars and back to the page', () => {
    expect(walk(PAGE_STOP, plain, 4)).toEqual([
      bars(0),
      bars(1),
      bars(2),
      PAGE_STOP,
    ])
  })

  it('puts what the page offers between the bars and the page', () => {
    expect(walk(PAGE_STOP, withControls, 6)).toEqual([
      bars(0),
      bars(1),
      bars(2),
      controls(0),
      controls(1),
      PAGE_STOP,
    ])
  })

  it('walks the same ring backwards', () => {
    expect(walk(PAGE_STOP, withControls, 6, true)).toEqual([
      controls(1),
      controls(0),
      bars(2),
      bars(1),
      bars(0),
      PAGE_STOP,
    ])
  })

  /** The bars are not mounted while hidden, so their last stop is not known yet. */
  it('answers the bars’ last stop as -1 while they are not mounted', () => {
    expect(ringStep(PAGE_STOP, { bars: undefined, controls: 0 }, true)).toEqual(
      bars(-1),
    )
    expect(
      ringStep(controls(0), { bars: undefined, controls: 1 }, true),
    ).toEqual(bars(-1))
    expect(
      ringStep(PAGE_STOP, { bars: undefined, controls: 0 }, false),
    ).toEqual(bars(0))
  })

  /** Asked again once the bars have gone, a stop in them steps on past them. */
  it('steps on from a bar stop once the bars have gone', () => {
    expect(ringStep(bars(2), { bars: undefined, controls: 1 }, false)).toEqual(
      controls(0),
    )
    expect(ringStep(bars(2), { bars: undefined, controls: 0 }, false)).toEqual(
      PAGE_STOP,
    )
  })

  it('skips a region with nothing in it', () => {
    expect(ringStep(PAGE_STOP, { bars: 0, controls: 1 }, false)).toEqual(
      controls(0),
    )
    expect(ringStep(PAGE_STOP, { bars: 0, controls: 0 }, false)).toEqual(
      PAGE_STOP,
    )
  })
})
