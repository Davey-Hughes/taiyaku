import { act } from 'react'
import { describe, expect, it } from 'vitest'
import { Labeller } from './Labeller'
import { mount } from './testing/mount'

// This is what the `browser` project exists for: `jsdom` has no layout
// engine, `getBoundingClientRect` returns zeroes there, and a box drawn at
// the wrong place is not observable without one — the argument
// `vite.config.ts`'s own comment already makes.
describe('drag to draw, with a layout engine', () => {
  it('lands the box where the pointer went, in page fractions', async () => {
    const host = mount(<Labeller initialBoxes={[]} />)
    const page = host.querySelector('[data-testid="label-page"]') as HTMLElement
    const box = page.getBoundingClientRect()
    // A quarter of the way in, to half way across and down.
    await drag(
      page,
      box.left + box.width * 0.25,
      box.top + box.height * 0.25,
      box.left + box.width * 0.5,
      box.top + box.height * 0.5,
    )
    const drawn = host.querySelector('.region-box') as HTMLElement
    expect(Number.parseFloat(drawn.style.left)).toBeCloseTo(25, 0)
    expect(Number.parseFloat(drawn.style.width)).toBeCloseTo(25, 0)
  })

  it('measures against the page element, not the window', async () => {
    // The page is letterboxed inside the viewport. Converting against the
    // window puts every box off by the letterbox margin, which looks correct
    // at one aspect ratio and wrong at every other.
    const host = mount(<Labeller initialBoxes={[]} />)
    const page = host.querySelector('[data-testid="label-page"]') as HTMLElement
    const box = page.getBoundingClientRect()
    expect(box.left).toBeGreaterThan(0)
    await drag(
      page,
      box.left,
      box.top,
      box.left + box.width,
      box.top + box.height,
    )
    const drawn = host.querySelector('.region-box') as HTMLElement
    expect(Number.parseFloat(drawn.style.left)).toBeCloseTo(0, 0)
    expect(Number.parseFloat(drawn.style.width)).toBeCloseTo(100, 0)
  })
})

async function drag(
  el: Element,
  x0: number,
  y0: number,
  x1: number,
  y1: number,
) {
  act(() => {
    el.dispatchEvent(
      new PointerEvent('pointerdown', {
        clientX: x0,
        clientY: y0,
        bubbles: true,
      }),
    )
  })
  act(() => {
    el.dispatchEvent(
      new PointerEvent('pointermove', {
        clientX: x1,
        clientY: y1,
        bubbles: true,
      }),
    )
  })
  act(() => {
    el.dispatchEvent(
      new PointerEvent('pointerup', {
        clientX: x1,
        clientY: y1,
        bubbles: true,
      }),
    )
  })
}
