import { afterEach, describe, expect, it } from 'vitest'
import { anchorOf, kept, show, shows, topOf, wordNodes } from '../src/bookPages'

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
const TALL = 400

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
 * `vertical` stands the lines up, 30 px wide, running leftward from the
 * right edge, or paged, a page 400 px high each down the screen. A
 * `{ figure: n }` is a figure between blocks, `n` lines tall from the start
 * of a line (paged, `n` pages), naming no block.
 */
function text(
  paged: boolean,
  blocks: (string | [string, number, string] | { figure: number })[],
  vertical = false,
) {
  bases = new WeakMap()
  const el = document.createElement('div')
  if (paged) el.dataset.paged = ''
  if (vertical) el.dataset.vertical = ''
  let at = 0
  const box = (g: number) => {
    const line = Math.floor(g / PER)
    if (vertical) {
      const x = paged ? PAGE - WIDE : PAGE - (line + 1) * WIDE - el.scrollLeft
      const y = (paged ? line * TALL : 0) + (g % PER) * 20 - el.scrollTop
      return new DOMRect(x, y, WIDE, 20)
    }
    const x = paged
      ? line * PAGE + (g % PER) * WIDE - el.scrollLeft
      : (g % PER) * WIDE
    const y = paged ? 0 : line * 20 - el.scrollTop
    return new DOMRect(x, y, WIDE, 20)
  }
  /** The box from character slot `start` to `end`, as `getBoundingClientRect` gives a box over several lines. */
  const spanning = (start: number, end: number) => () => {
    const [a, b] = [box(start), box(end)]
    return new DOMRect(a.left, a.top, b.right - a.left, b.bottom - a.top)
  }
  let block = 0
  for (const given of blocks) {
    if (!Array.isArray(given) && typeof given === 'object') {
      at = Math.ceil(at / PER) * PER
      const figure = document.createElement('figure')
      figure.dataset.figure = ''
      figure.getBoundingClientRect = spanning(at, at + given.figure * PER - 1)
      el.append(figure)
      at += given.figure * PER
      continue
    }
    const [words, cut, aside] =
      typeof given === 'string' ? [given, given.length, undefined] : given
    const p = document.createElement('p')
    p.dataset.block = String(block++)
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
    const start = at
    p.getBoundingClientRect = spanning(start, at + words.length - 1)
    p.getClientRects = () => [box(start)] as unknown as DOMRectList
    el.append(p)
    at += words.length
  }
  el.getBoundingClientRect = () => new DOMRect(0, 0, PAGE, TALL)
  Object.defineProperty(el, 'clientWidth', { value: PAGE })
  Object.defineProperty(el, 'clientHeight', { value: TALL })
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

describe('a place in vertical text', () => {
  it('scrolled, is the first character in from the right edge, scrollLeft run negative', () => {
    const el = text(false, ['short', LONG], true)
    el.scrollLeft = -WIDE
    expect(topOf(el)).toEqual({ block: 1, at: 15 })
    expect(shows(el, { block: 1, at: 15 })).toBe(true)
    expect(shows(el, { block: 1, at: 14 })).toBe(false)
    show(el, 1, 40)
    expect(el.scrollLeft).toBe(-2 * WIDE)
  })

  it('paged, is the first character on the page down the screen', () => {
    const el = text(true, ['short', LONG], true)
    el.scrollTop = TALL
    expect(topOf(el)).toEqual({ block: 1, at: 15 })
    expect(shows(el, { block: 1, at: 14 })).toBe(false)
    show(el, 1, 40)
    expect(el.scrollTop).toBe(2 * TALL)
    show(el, 1)
    expect(el.scrollTop).toBe(0)
  })
})

describe('a place in a compared row, paged', () => {
  it('goes back to the page it was taken on, whatever its fraction rounds to', () => {
    // A row from 32 px, 1521.75 px long over two 1000 px pages: taken at the
    // second page's start, its fraction times its length falls a hair short.
    const el = document.createElement('div')
    el.dataset.paged = ''
    el.dataset.compared = ''
    const row = document.createElement('div')
    row.dataset.block = '0'
    row.getBoundingClientRect = () =>
      new DOMRect(32 - el.scrollLeft, 0, 1521.75, TALL)
    el.append(row)
    el.getBoundingClientRect = () => new DOMRect(0, 0, 1000, TALL)
    Object.defineProperty(el, 'clientWidth', { value: 1000 })
    el.scrollTo = ((to: ScrollToOptions) => {
      el.scrollLeft = to.left ?? el.scrollLeft
    }) as typeof el.scrollTo
    document.body.append(el)
    el.scrollLeft = 1000
    const anchor = anchorOf(el)
    expect(anchor.into).toBeGreaterThan(0)
    el.scrollLeft = 0
    show(el, anchor.block, anchor.at, anchor)
    expect(el.scrollLeft).toBe(1000)
  })
})

describe('a place at a figure', () => {
  it('scrolled, is the figure the screen starts in, named from the block after it, and how far into it', () => {
    // Block 0 on line 0, the figure on lines 1 to 3, block 1 from line 4.
    const el = text(false, ['short', { figure: 3 }, LONG])
    el.scrollTop = 35
    const anchor = anchorOf(el)
    expect(anchor).toEqual({ block: 1, at: 0, figure: -1, into: 0.25 })
    expect(shows(el, anchor)).toBe(true)
    el.scrollTop = 200
    expect(shows(el, anchor)).toBe(false)
    expect(kept(el, anchor)).not.toEqual(anchor)
    show(el, anchor.block, anchor.at, anchor)
    expect(el.scrollTop).toBe(35)
  })

  it('paged, is a figure a page starts with, and turns to its page rather than to the words after it', () => {
    // Block 0 on page 0, the figure filling page 1, block 1 from page 2.
    const el = text(true, ['short', { figure: 1 }, LONG])
    el.scrollLeft = PAGE
    const anchor = anchorOf(el)
    expect(anchor).toEqual({ block: 1, at: 0, figure: -1 })
    expect(kept(el, anchor)).toBe(anchor)
    show(el, 0)
    show(el, anchor.block, anchor.at, anchor)
    expect(el.scrollLeft).toBe(PAGE)
    show(el, anchor.block)
    expect(el.scrollLeft).toBe(2 * PAGE)
  })

  it('is named from the block before it where a figure ends the book', () => {
    // Blocks 0 and 1 on lines 0 to 2, the figure on lines 3 to 5.
    const el = text(false, ['short', LONG, { figure: 3 }])
    el.scrollTop = 70
    const anchor = anchorOf(el)
    expect(anchor).toMatchObject({ block: 1, at: 0, figure: 1 })
    expect(anchor.into).toBeCloseTo(1 / 6)
    el.scrollTop = 0
    show(el, anchor.block, anchor.at, anchor)
    expect(el.scrollTop).toBeCloseTo(70)
  })

  it('is placed by its block where no figure is drawn where it names, or no child is its block’s own', () => {
    const el = text(true, ['short', { figure: 1 }, LONG])
    // Two children back from block 1 is block 0, not a figure.
    const words = { block: 1, at: 0, figure: -2 }
    expect(shows(el, words)).toBe(false)
    show(el, words.block, words.at, words)
    expect(el.scrollLeft).toBe(2 * PAGE)
    // No child is block 2's, so the figure before block 1 is not its.
    const none = { block: 2, at: 0, figure: -1 }
    expect(shows(el, none)).toBe(false)
    show(el, none.block, none.at, none)
    expect(el.scrollLeft).toBe(2 * PAGE)
  })
})
