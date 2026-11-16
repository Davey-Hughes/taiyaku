import { afterEach, describe, expect, it } from 'vitest'
import { kept, show, shows, topOf, wordNodes } from '../src/bookPages'

/**
 * `bookPages.ts`'s places, on a laid-out text jsdom cannot lay out: twenty
 * characters to a page 600 px wide, or to a line 20 px high when scrolled,
 * each character 30 px wide, counted through the book's words and past any
 * aside set among them. `bookPages.browser.test.tsx` checks the same against
 * a real layout.
 */

const PER = 20
const WIDE = 30
const PAGE = 600

const realRects = Range.prototype.getClientRects
afterEach(() => {
  Range.prototype.getClientRects = realRects
  document.body.replaceChildren()
})

/** Where each text node's first character falls in the book. */
let bases = new WeakMap<Node, number>()

/**
 * A text of `blocks`, each its words and, where given, an aside set after the
 * first `[n, text]` characters; `paged` lays it in pages, else in lines.
 */
function text(paged: boolean, blocks: (string | [string, number, string])[]) {
  bases = new WeakMap()
  const el = document.createElement('div')
  if (paged) el.dataset.paged = ''
  let at = 0
  const box = (g: number) => {
    const line = Math.floor(g / PER)
    const x = paged
      ? line * PAGE + (g % PER) * WIDE - el.scrollLeft
      : (g % PER) * WIDE
    const y = paged ? 0 : line * 20 - el.scrollTop
    return new DOMRect(x, y, WIDE, 20)
  }
  for (const [i, given] of blocks.entries()) {
    const [words, cut, aside] =
      typeof given === 'string' ? [given, given.length, undefined] : given
    const p = document.createElement('p')
    p.dataset.block = String(i)
    const own = document.createElement('span')
    own.dataset.text = ''
    const first = document.createTextNode(words.slice(0, cut))
    bases.set(first, at)
    own.append(first)
    if (aside !== undefined) {
      const peek = document.createElement('span')
      peek.dataset.aside = ''
      peek.append(aside)
      const rest = document.createTextNode(words.slice(cut))
      bases.set(rest, at + cut)
      own.append(peek, rest)
    }
    p.append(own)
    const [start, end] = [at, at + words.length - 1]
    p.getBoundingClientRect = () => {
      const [a, b] = [box(start), box(end)]
      return new DOMRect(a.left, a.top, b.right - a.left, b.bottom - a.top)
    }
    p.getClientRects = () => [box(start)] as unknown as DOMRectList
    el.append(p)
    at += words.length
  }
  el.getBoundingClientRect = () => new DOMRect(0, 0, PAGE, 400)
  Object.defineProperty(el, 'clientWidth', { value: PAGE })
  el.scrollTo = ((to: ScrollToOptions) => {
    if (to.left !== undefined) el.scrollLeft = to.left
    if (to.top !== undefined) el.scrollTop = to.top
  }) as typeof el.scrollTo
  Range.prototype.getClientRects = function (this: Range) {
    const base = bases.get(this.startContainer) ?? 0
    return [box(base + this.startOffset)] as unknown as DOMRectList
  }
  document.body.append(el)
  return el
}

/** Forty characters, a page's worth and more, then a short block. */
const LONG = 'a'.repeat(45)

describe('a place finer than a block', () => {
  it('is the first character on the page, inside the block the page starts in', () => {
    const el = text(true, ['short', LONG])
    el.scrollLeft = PAGE
    expect(topOf(el)).toEqual({ block: 1, at: 15 })
    el.scrollLeft = 2 * PAGE
    expect(topOf(el)).toEqual({ block: 1, at: 35 })
  })

  it('is on the page only while its character is, by more than a pixel', () => {
    const el = text(true, ['short', LONG])
    el.scrollLeft = PAGE
    expect(shows(el, { block: 1, at: 15 })).toBe(true)
    expect(shows(el, { block: 1, at: 14 })).toBe(false)
    expect(shows(el, { block: 1, at: 35 })).toBe(false)
  })

  it('is kept while shown, and is the top of the page once it is not', () => {
    const el = text(true, ['short', LONG])
    el.scrollLeft = PAGE
    const anchor = { block: 1, at: 20 }
    expect(kept(el, anchor)).toBe(anchor)
    el.scrollLeft = 2 * PAGE
    expect(kept(el, anchor)).toEqual({ block: 1, at: 35 })
  })

  it('turns to the page holding its character, and without one to the block’s first line', () => {
    const el = text(true, ['short', LONG])
    show(el, 1, 30)
    expect(el.scrollLeft).toBe(PAGE)
    show(el, 1, 40)
    expect(el.scrollLeft).toBe(2 * PAGE)
    show(el, 1)
    expect(el.scrollLeft).toBe(0)
  })

  it('scrolled, is the first character on the screen, and scrolls its line to the top', () => {
    const el = text(false, ['short', LONG])
    el.scrollTop = 20
    expect(topOf(el)).toEqual({ block: 1, at: 15 })
    expect(shows(el, { block: 1, at: 15 })).toBe(true)
    show(el, 1, 40)
    expect(el.scrollTop).toBe(40)
  })

  it('counts a block’s own words past a peek set among them', () => {
    const el = text(true, [
      'short',
      [LONG, 10, 'a peek of thirty letters, long'],
    ])
    const words = el.querySelector('[data-block="1"] [data-text]') as Element
    expect(wordNodes(words).map((n) => n.length)).toEqual([10, 35])
    el.scrollLeft = PAGE
    expect(topOf(el)).toEqual({ block: 1, at: 15 })
  })

  it('is the block alone where nothing can be measured', () => {
    const el = text(true, ['short', LONG])
    Range.prototype.getClientRects =
      undefined as unknown as Range['getClientRects']
    el.scrollLeft = PAGE
    expect(topOf(el)).toEqual({ block: 1, at: 0 })
    expect(shows(el, { block: 1, at: 15 })).toBe(false)
  })
})
