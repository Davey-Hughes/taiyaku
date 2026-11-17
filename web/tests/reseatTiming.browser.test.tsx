import { afterEach, describe, expect, it } from 'vitest'
import { RESEAT_EASING, RESEAT_MS, reseatEased } from '../src/reseatTiming'

/**
 * The covers glide by a Web Animation on `RESEAT_EASING`, and the Continue
 * row above them is laid frame by frame by `reseatEased` (spec O304): the
 * two keep step only while they are the same curve, which only the
 * browser's own easing can show.
 */

afterEach(() => {
  document.body.replaceChildren()
})

describe('the glide’s curve', () => {
  it('the animation’s curve and reseatEased agree', () => {
    const el = document.createElement('div')
    document.body.append(el)
    const animation = el.animate(
      [{ transform: 'translateX(100px)' }, { transform: 'none' }],
      { duration: RESEAT_MS, easing: RESEAT_EASING },
    )
    animation.pause()
    for (const at of [9, 18, 36, 54, 72, 90, 117, 144, 171]) {
      animation.currentTime = at
      const progress = animation.effect?.getComputedTiming().progress
      expect(typeof progress, `${at} ms in`).toBe('number')
      expect(
        Math.abs((progress as number) - reseatEased(at / RESEAT_MS)),
        `${at} ms in`,
      ).toBeLessThan(0.001)
    }
  })
})
