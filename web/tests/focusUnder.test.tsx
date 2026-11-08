import { afterEach, describe, expect, it } from 'vitest'
import { focusUnder } from '../src/focusUnder'

/** `el` drawn at `left`, `top`, `width` × `height`, which jsdom does not lay out. */
function at(
  el: Element,
  left: number,
  top: number,
  width: number,
  height: number,
) {
  el.getBoundingClientRect = () =>
    DOMRect.fromRect({ x: left, y: top, width, height })
}

describe('focusUnder', () => {
  afterEach(() => {
    document.body.replaceChildren()
  })

  /**
   * A cover at 0,0 100×40, and a box that takes focus, at `left`, `top`: a
   * text box, which is marked as focused however focus came to it.
   */
  function drawn(left: number, top: number) {
    const cover = document.createElement('p')
    const box = document.createElement('input')
    document.body.append(box, cover)
    at(cover, 0, 0, 100, 40)
    at(box, left, top, 20, 10)
    return { cover, box }
  }

  it('says a focused control whose centre is under the cover is under it', () => {
    const { cover, box } = drawn(10, 10)
    box.focus()
    expect(box.matches(':focus-visible')).toBe(true)
    expect(focusUnder(cover)).toBe(true)
  })

  it('says a focused control whose centre is outside the cover is not', () => {
    const { cover, box } = drawn(10, 36)
    box.focus()
    expect(box.matches(':focus-visible')).toBe(true)
    expect(focusUnder(cover)).toBe(false)
  })

  it('says a focused control under the cover that is not marked as focused, as one a pointer pressed, is not under it', () => {
    const { cover } = drawn(60, 60)
    const pressed = document.createElement('button')
    document.body.append(pressed)
    at(pressed, 10, 10, 20, 10)
    pressed.focus()
    expect(document.activeElement).toBe(pressed)
    expect(pressed.matches(':focus-visible')).toBe(false)
    expect(focusUnder(cover)).toBe(false)
  })
})
