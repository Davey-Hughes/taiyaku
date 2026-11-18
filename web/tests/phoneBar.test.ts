import { describe, expect, it } from 'vitest'
import { shownShare, trackEnd } from '../src/phoneBar'

/**
 * The band rule the phone bar's track moves by, with the bar 52 px tall
 * and a page that scrolls 1000 px. The hook that applies it on each
 * scroll is held in jsdom (`phoneBar.hook.test.tsx`).
 */

const H = 52
const MAX = 1000

describe('trackEnd', () => {
  it('follows the scroll up while the bar is shown', () => {
    expect(trackEnd(152, 80, H, MAX)).toBe(132)
  })

  it('holds while the bar slides out or in', () => {
    expect(trackEnd(152, 120, H, MAX)).toBe(152)
    expect(trackEnd(400, 380, H, MAX)).toBe(400)
    for (const end of [152, 400]) {
      for (let y = end - H; y <= end; y += 13) {
        expect({ y, end: trackEnd(end, y, H, MAX) }).toEqual({ y, end })
      }
    }
  })

  it('follows the scroll down once the bar is out', () => {
    expect(trackEnd(152, 400, H, MAX)).toBe(400)
  })

  it('counts a rubber band past either end as the end', () => {
    expect(trackEnd(400, -60, H, MAX)).toBe(52)
    expect(trackEnd(1000, 1080, H, MAX)).toBe(1000)
  })

  it("never puts the end above the bar's own height", () => {
    expect(trackEnd(10, 0, H, MAX)).toBe(52)
    expect(trackEnd(30, 20, H, 0)).toBe(52)
  })
})

describe('shownShare', () => {
  it('gives the share of the bar in view, from none to whole', () => {
    expect(shownShare(152, 100, H)).toBe(1)
    expect(shownShare(152, 126, H)).toBe(0.5)
    expect(shownShare(152, 200, H)).toBe(0)
    expect(shownShare(152, 90, H)).toBe(1)
  })
})
