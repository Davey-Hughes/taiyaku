import { describe, expect, it } from 'vitest'
import { SIZE_LADDER, pageBoxLongEdge, requestedLongEdge } from './pageSize'

describe('requestedLongEdge', () => {
  it('rounds up to a rung, so a page is never asked for smaller than it is drawn', () => {
    expect(requestedLongEdge(1025, 1, 1)).toBe(1536)
    expect(requestedLongEdge(1024, 1, 1)).toBe(1024)
    expect(requestedLongEdge(1, 1, 1)).toBe(512)
  })

  // The measured case: a 1385x512 window in `two` mode draws a page into
  // 693x475, which wants 768 rather than the 2147 the archive holds.
  it('asks for a rung near the drawn size rather than the full page', () => {
    const box = pageBoxLongEdge(1385, 512, 'two')
    expect(requestedLongEdge(box, 1, 1)).toBe(768)
  })

  it('accounts for a high-density display rather than serving it soft', () => {
    expect(requestedLongEdge(700, 2, 1)).toBe(1536)
    expect(requestedLongEdge(700, 3, 1)).toBe(3072)
  })

  it('asks for the original once magnification wants more than the ladder offers', () => {
    // L1 zooms to 8x; nothing on the ladder satisfies that from a reading-size
    // page, and a soft page is exactly what zoom must not produce.
    expect(requestedLongEdge(700, 1, 8)).toBeUndefined()
    expect(requestedLongEdge(700, 2, 4)).toBeUndefined()
  })

  it('climbs the ladder as magnification grows, rather than jumping straight to the original', () => {
    const box = 700
    expect(requestedLongEdge(box, 1, 1)).toBe(768)
    expect(requestedLongEdge(box, 1, 2)).toBe(1536)
    expect(requestedLongEdge(box, 1, 3)).toBe(3072)
    expect(requestedLongEdge(box, 1, 5)).toBeUndefined()
  })

  // A wrong size here is invisible — the page still draws, only soft — so
  // every unusable input has to fall toward the original rather than a guess.
  it('asks for the original on any input that is not a positive finite number', () => {
    for (const bad of [0, -1, Number.NaN, Number.POSITIVE_INFINITY]) {
      expect(requestedLongEdge(bad, 1, 1)).toBeUndefined()
      expect(requestedLongEdge(700, bad, 1)).toBeUndefined()
      expect(requestedLongEdge(700, 1, bad)).toBeUndefined()
    }
  })

  it('never answers with a size that is not on the ladder', () => {
    for (const box of [1, 300, 700, 1100, 1900, 2500, 3000]) {
      const asked = requestedLongEdge(box, 1, 1)
      if (asked !== undefined) expect(SIZE_LADDER).toContain(asked)
    }
  })
})

describe('pageBoxLongEdge', () => {
  it('halves the width in two-page mode, where each cell holds one page', () => {
    expect(pageBoxLongEdge(1385, 512, 'two')).toBe(692.5)
  })

  it('gives a page the whole width in single and scroll mode', () => {
    expect(pageBoxLongEdge(1385, 512, 'single')).toBe(1385)
    expect(pageBoxLongEdge(1385, 512, 'scroll')).toBe(1385)
  })

  it('takes the taller edge when the window is tall, which is the ordinary reading shape', () => {
    // A portrait window: height governs how large a page is drawn, not width.
    expect(pageBoxLongEdge(900, 1600, 'two')).toBe(1600)
    expect(pageBoxLongEdge(900, 1600, 'single')).toBe(1600)
  })
})
