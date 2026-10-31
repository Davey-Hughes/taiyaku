import { describe, expect, it } from 'vitest'
import {
  carrySwap,
  frameOf,
  swapStep,
  type CarriedSwap,
  type DrawnPage,
  type Measurable,
} from '../src/swapView'
import type { Drawn } from '../src/viewport'

const img = (
  src: string,
  naturalWidth = 800,
  naturalHeight = 1200,
): Measurable => ({ src, naturalWidth, naturalHeight })

const drawn: DrawnPage = {
  url: '/api/works/w/en/pages/4',
  cell: 'first',
  half: 'R',
  hug: 'right-edge',
}

const from: Drawn = {
  frame: { w: 800, h: 1200 },
  cell: 'first',
  half: 'R',
  hug: 'right-edge',
}

const carried: CarriedSwap = { from, edition: 'raw' }
const box = { width: 1400, height: 700 }

describe('frameOf', () => {
  // `src` on a live `<img>` is absolute; the URL asked about is the path
  // `pageAt` built. Suffix, for the same reason `draftImage` matches by one.
  it('matches by URL suffix, not by equality', () => {
    const images = [img('http://host/api/works/w/en/pages/4', 700, 1000)]
    expect(frameOf(images, '/api/works/w/en/pages/4')).toEqual({
      w: 700,
      h: 1000,
    })
  })

  it('takes the first image drawing that page, of however many are on screen', () => {
    const images = [
      img('http://host/pages/3', 100, 100),
      img('http://host/pages/4', 200, 300),
      img('http://host/pages/4', 900, 900),
    ]
    expect(frameOf(images, '/pages/4')).toEqual({ w: 200, h: 300 })
  })

  // The three states that are one state for a caller: not on screen, on
  // screen but not decoded, decoded to nothing.
  it('finds nothing for a page that is not on screen', () => {
    expect(frameOf([img('http://host/pages/3')], '/pages/4')).toBeUndefined()
    expect(frameOf([], '/pages/4')).toBeUndefined()
  })

  it('finds nothing for a page that has not decoded', () => {
    expect(frameOf([img('/pages/4', 0, 1200)], '/pages/4')).toBeUndefined()
    expect(frameOf([img('/pages/4', 800, 0)], '/pages/4')).toBeUndefined()
  })
})

describe('carrySwap', () => {
  it('carries the frame, the cell, the half and the hug', () => {
    expect(carrySwap(drawn, { w: 800, h: 1200 }, 'raw', true)).toEqual({
      from: {
        frame: { w: 800, h: 1200 },
        cell: 'first',
        half: 'R',
        hug: 'right-edge',
      },
      edition: 'raw',
    })
  })

  // The hug is the part a swap can get wrong: the artwork is pulled to the
  // seam in two-page mode, and measuring it as centred would put the mapped
  // rect where a centred page would have been.
  it('carries a centred hug as faithfully as a hugged one', () => {
    const centred: DrawnPage = { url: '/p/1', cell: 'only', hug: 'centre' }
    const swap = carrySwap(centred, { w: 1, h: 2 }, 'en', true)
    expect(swap?.from.hug).toBe('centre')
    expect(swap?.from.half).toBeUndefined()
  })

  // Each of these is a state the view is simply left alone in, which is the
  // design's §9 answer reached four different ways.
  it('carries nothing while the module is off', () => {
    expect(carrySwap(drawn, { w: 8, h: 1 }, 'raw', false)).toBeUndefined()
  })

  it('carries nothing with no page drawn — scroll mode, or a gap', () => {
    expect(carrySwap(undefined, { w: 8, h: 1 }, 'raw', true)).toBeUndefined()
  })

  it('carries nothing with no edition being left', () => {
    expect(carrySwap(drawn, { w: 8, h: 1 }, undefined, true)).toBeUndefined()
  })

  // A swap pressed faster than a page decodes: there is nothing to measure,
  // so nothing is carried and the view stays where it was.
  it('carries nothing when the outgoing page has no frame', () => {
    expect(carrySwap(drawn, undefined, 'raw', true)).toBeUndefined()
  })
})

describe('swapStep', () => {
  const to: DrawnPage = {
    url: '/p/9',
    cell: 'second',
    half: 'L',
    hug: 'left-edge',
  }
  const frame = { w: 600, h: 900 }

  it('applies, handing mapView both drawn pages and the area aspect', () => {
    const step = swapStep(carried, true, to, frame, box, 'en', 'pillarbox')
    expect(step).toEqual({
      kind: 'apply',
      from,
      to: { frame, cell: 'second', half: 'L', hug: 'left-edge' },
      areaAspect: 2,
      mapKind: 'pillarbox',
    })
  })

  it('passes the map kind straight through rather than deciding it', () => {
    const step = swapStep(carried, true, to, frame, box, 'en', 'reset')
    expect(step.kind === 'apply' && step.mapKind).toBe('reset')
  })

  // **The distinction the fourth kind exists for.** A later swap replaced
  // this one while it waited; that newer swap has its own effect with its own
  // listener, and clearing the carry here would throw its measurement away and
  // leave the view where it was — the displacement this module prevents.
  it('is done, not dropped, once a newer swap has replaced this one', () => {
    expect(swapStep(carried, false, to, frame, box, 'en', 'reset')).toEqual({
      kind: 'done',
    })
  })

  // `done` is answered before anything else is even looked at: a superseded
  // swap whose incoming page also has not decoded must not come back as a
  // `wait` that keeps the old listener alive.
  it('is done whatever else is missing', () => {
    expect(
      swapStep(carried, false, undefined, undefined, undefined, 'raw', 'reset'),
    ).toEqual({ kind: 'done' })
  })

  // A digit pressed for the edition already on screen. `swap` builds a fresh
  // `ReadingState` either way, so this comparison is the only thing that tells
  // a no-op swap from a real one.
  it('drops a swap that changed nothing', () => {
    expect(
      swapStep(carried, true, to, frame, box, carried.edition, 'pillarbox'),
    ).toEqual({ kind: 'drop' })
  })

  // A mode switch to scroll, or the reader moved onto a gap: there is no
  // drawn page to carry the view onto.
  it('drops when nothing is drawn to land on', () => {
    expect(
      swapStep(carried, true, undefined, frame, box, 'en', 'pillarbox'),
    ).toEqual({ kind: 'drop' })
  })

  it('drops when there is no page area to measure at all', () => {
    expect(
      swapStep(carried, true, to, frame, undefined, 'en', 'pillarbox'),
    ).toEqual({ kind: 'drop' })
  })

  // The wait a `load` event actually ends, which is why the listener is
  // attached at all.
  it('waits, undecoded, for a page on screen that has not decoded', () => {
    expect(
      swapStep(carried, true, to, undefined, box, 'en', 'pillarbox'),
    ).toEqual({ kind: 'wait', why: 'undecoded' })
  })

  // **No `load` will end this one**, and that is why the two waits are not
  // the same wait: nothing about the image is what is missing. It ends on the
  // next render that gives the page area a box.
  it('waits, no-box, for a page area with no layout', () => {
    for (const empty of [
      { width: 0, height: 700 },
      { width: 1400, height: 0 },
    ]) {
      expect(
        swapStep(carried, true, to, frame, empty, 'en', 'pillarbox'),
      ).toEqual({ kind: 'wait', why: 'no-box' })
    }
  })

  // A drop is final and a wait is not, so where both could apply the drop has
  // to win — otherwise a swap onto a gap would hold a listener open forever.
  it('drops rather than waits where both would apply', () => {
    expect(
      swapStep(carried, true, undefined, undefined, box, 'en', 'pillarbox'),
    ).toEqual({ kind: 'drop' })
  })
})
