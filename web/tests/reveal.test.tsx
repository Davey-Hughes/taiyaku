import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { bringIntoView, inView, reveal } from '../src/reveal'

/**
 * `reveal.ts` decides from geometry, which only a browser lays out, so the
 * browser tests reach it and the coverage run does not. These stub the two
 * reads it makes, `getBoundingClientRect` and the computed
 * `scroll-margin-top`, and watch what it asks of `scrollIntoView`.
 */

const WINDOW = 800

function at(top: number, height: number, margin = '0px') {
  const el = document.createElement('p')
  el.getBoundingClientRect = () => ({ top, height }) as DOMRect
  vi.spyOn(window, 'getComputedStyle').mockReturnValue({
    scrollMarginTop: margin,
  } as CSSStyleDeclaration)
  return el
}

beforeEach(() => {
  vi.stubGlobal('innerHeight', WINDOW)
})
afterEach(() => {
  vi.restoreAllMocks()
  vi.unstubAllGlobals()
})

describe('inView', () => {
  it('is true for an element wholly inside the window', () => {
    expect(inView(at(100, 20))).toBe(true)
  })

  it('is false when the head runs past the foot of the window', () => {
    expect(inView(at(790, 100))).toBe(false)
  })

  it('judges a tall element by its first 48px, not its height', () => {
    expect(inView(at(740, 400))).toBe(true)
    expect(inView(at(753, 400))).toBe(false)
  })

  it('is false above the sticky margin, true from a pixel under it', () => {
    expect(inView(at(10, 20, '60px'))).toBe(false)
    expect(inView(at(58, 20, '60px'))).toBe(false)
    expect(inView(at(59, 20, '60px'))).toBe(true)
  })

  it('judges the whole element where asked, as far as the window holds', () => {
    expect(inView(at(740, 40), true)).toBe(true)
    expect(inView(at(770, 40), true)).toBe(false)
    expect(inView(at(0, 900), true)).toBe(true)
    expect(inView(at(1, 900), true)).toBe(false)
  })

  it('is false outside the box of a panel that scrolls it, though in the window', () => {
    const panel = document.createElement('div')
    panel.getBoundingClientRect = () => ({ top: 100 }) as DOMRect
    Object.defineProperty(panel, 'clientHeight', { value: 500 })
    const el = document.createElement('p')
    panel.append(el)
    vi.spyOn(window, 'getComputedStyle').mockImplementation(
      (of) =>
        ({
          scrollMarginTop: '0px',
          overflowY: of === panel ? 'auto' : 'visible',
        }) as CSSStyleDeclaration,
    )
    const placed = (top: number) => {
      el.getBoundingClientRect = () => ({ top, height: 20 }) as DOMRect
      return el
    }
    expect(inView(placed(120))).toBe(true)
    expect(inView(placed(590))).toBe(false)
    expect(inView(placed(90))).toBe(false)
  })

  it('reads a margin that is not a length as none', () => {
    expect(inView(at(0, 20, ''))).toBe(true)
    expect(inView(at(-5, 20, 'auto'))).toBe(false)
  })
})

describe('reveal', () => {
  it('leaves an element in view where it is', () => {
    const el = at(100, 20)
    el.scrollIntoView = vi.fn()
    reveal(el)
    expect(el.scrollIntoView).not.toHaveBeenCalled()
  })

  it('brings one below the window to the top, not the nearest edge', () => {
    const el = at(900, 20)
    el.scrollIntoView = vi.fn()
    reveal(el)
    expect(el.scrollIntoView).toHaveBeenCalledWith({ block: 'start' })
  })

  it('brings one above the sticky margin to the top', () => {
    const el = at(10, 20, '60px')
    el.scrollIntoView = vi.fn()
    reveal(el)
    expect(el.scrollIntoView).toHaveBeenCalledWith({ block: 'start' })
  })

  it('does not throw where there is no scrollIntoView', () => {
    const el = at(900, 20)
    expect(el.scrollIntoView).toBeUndefined()
    expect(() => reveal(el)).not.toThrow()
  })
})

describe('bringIntoView', () => {
  it('leaves an element in view where it is', () => {
    const el = at(100, 20)
    el.scrollIntoView = vi.fn()
    bringIntoView(el)
    expect(el.scrollIntoView).not.toHaveBeenCalled()
  })

  it('scrolls one below the window just far enough, not to the top', () => {
    const el = at(900, 20)
    el.scrollIntoView = vi.fn()
    bringIntoView(el)
    expect(el.scrollIntoView).toHaveBeenCalledWith({ block: 'nearest' })
  })

  it('scrolls one whose head is in view and whose foot is not, so that it is seen whole', () => {
    const el = at(700, 120)
    el.scrollIntoView = vi.fn()
    bringIntoView(el)
    expect(el.scrollIntoView).toHaveBeenCalledWith({ block: 'nearest' })
  })

  it('scrolls one above the sticky margin just far enough', () => {
    const el = at(10, 20, '60px')
    el.scrollIntoView = vi.fn()
    bringIntoView(el)
    expect(el.scrollIntoView).toHaveBeenCalledWith({ block: 'nearest' })
  })

  it('does nothing for no element', () => {
    expect(() => bringIntoView(null)).not.toThrow()
  })

  it('does not throw where there is no scrollIntoView', () => {
    const el = at(900, 20)
    expect(el.scrollIntoView).toBeUndefined()
    expect(() => bringIntoView(el)).not.toThrow()
  })
})
