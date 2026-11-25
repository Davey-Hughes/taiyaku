import { afterEach, describe, expect, it, onTestFinished, vi } from 'vitest'
import {
  anchorOf,
  bringIntoSight,
  keepsRead,
  kept,
  matchShows,
  movesAnchor,
  readingAt,
  readPlaced,
  resumeBlock,
  show,
  shows,
  topOf,
  whole,
  wordNodes,
} from '../src/bookPages'

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
  vi.unstubAllGlobals()
})

/** A window the phone layout's condition holds for, which jsdom has no `matchMedia` to say. */
function inThePhoneLayout() {
  vi.stubGlobal('matchMedia', () => ({ matches: false }))
}

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

/** A character's width in a compared column, twenty of which fill half a page. */
const NARROW = PAGE / 2 / PER

/**
 * A compared row: each column's blocks as `[block, characters]`. It starts on
 * the page after the last row's, or on page `on`, sharing the page that row
 * ends on.
 */
interface Row {
  left: [number, number][]
  right: [number, number][]
  on?: number
}

/**
 * Two editions compared and paged, each row named from its first left block.
 * A column runs on over the row's pages twenty characters to a page, the left
 * one down the page's left half and the right one down its right half, so a
 * page can hold the words of one column and none of the other.
 */
function compared(rows: Row[]) {
  const el = document.createElement('div')
  el.dataset.paged = ''
  el.dataset.compared = ''
  const spots = new WeakMap<Node, (offset: number) => DOMRect>()
  let next = 0
  for (const { left, right, on = next } of rows) {
    const row = document.createElement('div')
    row.dataset.block = String(left[0][0])
    const column = (side: string, blocks: [number, number][], lane: number) => {
      const cell = document.createElement('div')
      let slot = 0
      for (const [block, length] of blocks) {
        const words = document.createElement('p')
        words.dataset.pick = `${side}:${block}`
        const node = document.createTextNode('a'.repeat(length))
        const base = slot
        spots.set(node, (offset) => {
          const at = base + offset
          const page = on + Math.floor(at / PER)
          const x = page * PAGE + lane + (at % PER) * NARROW
          return new DOMRect(x - el.scrollLeft, 0, NARROW, 20)
        })
        words.append(node)
        cell.append(words)
        slot += length
      }
      row.append(cell)
      return slot
    }
    const read = column('left', left, 0)
    const other = column('right', right, PAGE / 2)
    const pages = Math.ceil(Math.max(read, other) / PER)
    row.getBoundingClientRect = () =>
      new DOMRect(on * PAGE - el.scrollLeft, 0, pages * PAGE, TALL)
    el.append(row)
    next = on + pages
  }
  el.getBoundingClientRect = () => new DOMRect(0, 0, PAGE, TALL)
  Object.defineProperty(el, 'clientWidth', { value: PAGE })
  el.scrollTo = ((to: ScrollToOptions) => {
    el.scrollLeft = to.left ?? el.scrollLeft
  }) as typeof el.scrollTo
  Range.prototype.getClientRects = function (this: Range) {
    const spot = spots.get(this.startContainer) as (at: number) => DOMRect
    return [spot(this.startOffset)] as unknown as DOMRectList
  }
  document.body.append(el)
  return el
}

/** One row of both columns over three pages. */
const THREE_PAGES: Row[] = [{ left: [[0, 45]], right: [[0, 45]] }]

/** A row whose left column ends on its first page and whose right runs onto a second. */
const LEFT_ENDS_FIRST: Row = { left: [[0, 10]], right: [[0, 30]] }

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
  it('placed whole, forgets the character read as it forgets the fraction', () => {
    const anchor = { block: 3, at: 0, into: 0.5, read: { block: 3, at: 40 } }
    expect(whole(anchor)).toEqual({ block: 3, at: 0 })
  })

  it('placed whole, forgets a character read taken with no fraction', () => {
    const anchor = { block: 3, at: 0, read: { block: 3, at: 40 } }
    expect(whole(anchor)).toEqual({ block: 3, at: 0 })
  })

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

  it('turns to the page holding the character read, whatever the fraction would name', () => {
    const el = compared(THREE_PAGES)
    el.scrollLeft = 2 * PAGE
    show(el, 0, 0, { into: 0.1, read: { block: 0, at: 25 } })
    expect(el.scrollLeft).toBe(PAGE)
    show(el, 0, 0, { into: 0.1, read: { block: 0, at: 44 } })
    expect(el.scrollLeft).toBe(2 * PAGE)
  })

  it('goes by the fraction where the character read is no longer drawn', () => {
    const el = compared(THREE_PAGES)
    show(el, 0, 0, { into: 0.5, read: { block: 7, at: 0 } })
    expect(el.scrollLeft).toBe(PAGE)
  })
})

describe('the character read on a page a compared row carries over onto', () => {
  it('is the first of the column read on that page', () => {
    const el = compared(THREE_PAGES)
    el.scrollLeft = PAGE
    expect(anchorOf(el)).toEqual({
      block: 0,
      at: 0,
      into: 1 / 3,
      read: { block: 0, at: 20 },
    })
    el.scrollLeft = 2 * PAGE
    expect(anchorOf(el)).toEqual({
      block: 0,
      at: 0,
      into: 2 / 3,
      read: { block: 0, at: 40 },
    })
  })

  it('is named in the block the page reads in, past a block that ended on an earlier page', () => {
    const el = compared([
      {
        left: [
          [0, 10],
          [1, 35],
        ],
        right: [[0, 45]],
      },
    ])
    el.scrollLeft = PAGE
    expect(anchorOf(el).read).toEqual({ block: 1, at: 10 })
  })

  it('is in the next row where the column read ended before this page and that row starts on it', () => {
    const el = compared([
      LEFT_ENDS_FIRST,
      { left: [[1, 5]], right: [[1, 5]], on: 1 },
    ])
    el.scrollLeft = PAGE
    expect(anchorOf(el)).toEqual({
      block: 0,
      at: 0,
      into: 0.5,
      read: { block: 1, at: 0 },
    })
  })

  it('is the row’s first on the page the row starts on', () => {
    const el = compared(THREE_PAGES)
    expect(anchorOf(el)).toEqual({
      block: 0,
      at: 0,
      into: 0,
      read: { block: 0, at: 0 },
    })
  })

  it('in the phone layout is taken on the page the row starts on too', () => {
    inThePhoneLayout()
    expect(anchorOf(compared(THREE_PAGES)).read).toEqual({ block: 0, at: 0 })
  })

  it('is none on a page holding only the other column’s words', () => {
    const el = compared([LEFT_ENDS_FIRST])
    el.scrollLeft = PAGE
    expect(anchorOf(el)).toEqual({ block: 0, at: 0, into: 0.5 })
  })

  it('is none where the next row’s words start on a later page', () => {
    const el = compared([LEFT_ENDS_FIRST, { left: [[1, 5]], right: [[1, 5]] }])
    el.scrollLeft = PAGE
    expect(anchorOf(el)).toEqual({ block: 0, at: 0, into: 0.5 })
  })
})

describe('the character kept after a placing, compared and paged', () => {
  it('is the one placed while that is on the page', () => {
    const el = compared(THREE_PAGES)
    el.scrollLeft = PAGE
    expect(readPlaced(el, { block: 0, at: 25 })).toEqual({ block: 0, at: 25 })
  })

  it('is the page’s first of the column read once the one placed is off it', () => {
    const el = compared(THREE_PAGES)
    el.scrollLeft = PAGE
    expect(readPlaced(el, { block: 0, at: 0 })).toEqual({ block: 0, at: 20 })
  })

  it('is none where the page holds only the other column’s words', () => {
    const el = compared([LEFT_ENDS_FIRST])
    el.scrollLeft = PAGE
    expect(readPlaced(el, { block: 0, at: 0 })).toBeUndefined()
  })

  it('is the one placed in the phone layout too', () => {
    inThePhoneLayout()
    expect(readPlaced(compared(THREE_PAGES), { block: 0, at: 5 })).toEqual({
      block: 0,
      at: 5,
    })
  })

  it('is none scrolled, outside compare, or with no text', () => {
    const scrolled = compared(THREE_PAGES)
    delete scrolled.dataset.paged
    expect(readPlaced(scrolled, { block: 0, at: 0 })).toBeUndefined()
    const single = text(true, ['short', LONG])
    expect(readPlaced(single, { block: 0, at: 0 })).toBeUndefined()
    expect(readPlaced(null, { block: 0, at: 0 })).toBeUndefined()
  })
})

describe('whether a re-place goes by the character read', () => {
  it('holds compared and paged in the sidebar layout', () => {
    expect(keepsRead(compared(THREE_PAGES))).toBe(true)
  })

  it('holds in the phone layout too', () => {
    inThePhoneLayout()
    expect(keepsRead(compared(THREE_PAGES))).toBe(true)
  })

  it('does not hold scrolled, outside compare, or with no text', () => {
    const scrolled = compared(THREE_PAGES)
    delete scrolled.dataset.paged
    expect(keepsRead(scrolled)).toBe(false)
    expect(keepsRead(text(true, ['short', LONG]))).toBe(false)
    expect(keepsRead(null)).toBe(false)
  })
})

describe('the block saved for the page showing', () => {
  /** Block 0 over pages 0 to 2, blocks 1 and 2 opening on page 2, block 3 on page 3. */
  const BLOCKS = [LONG, 'b'.repeat(5), 'c'.repeat(20), 'd'.repeat(5)]

  it('is the first block to open on the page, past one carried onto it', () => {
    const el = text(true, BLOCKS)
    el.scrollLeft = PAGE * 2
    expect(topOf(el).block, 'the block carried over').toBe(0)
    expect(resumeBlock(el)).toBe(1)
  })

  it('is the block carried onto a page none opens on', () => {
    const el = text(true, BLOCKS)
    el.scrollLeft = PAGE
    expect(resumeBlock(el)).toBe(0)
  })

  it('is the block carried onto the book’s last page', () => {
    const el = text(true, [LONG])
    el.scrollLeft = PAGE * 2
    expect(resumeBlock(el)).toBe(0)
  })

  it('passes over a figure at the top of the page', () => {
    const el = text(true, ['a'.repeat(20), { figure: 1 }, 'b'.repeat(5)])
    el.scrollLeft = PAGE
    expect(resumeBlock(el)).toBe(1)
  })

  it('goes by the pages running down, vertical', () => {
    const el = text(true, BLOCKS, true)
    el.scrollTop = TALL * 2
    expect(resumeBlock(el)).toBe(1)
    el.scrollTop = TALL
    expect(resumeBlock(el)).toBe(0)
  })

  it('is the top block scrolled, where a screen’s length on a block opens', () => {
    const el = text(false, ['a'.repeat(500), 'b'.repeat(20)])
    el.scrollTop = TALL + 20
    expect(topOf(el).block).toBe(0)
    expect(resumeBlock(el)).toBe(0)
  })

  it('is block 0 with no text', () => {
    expect(resumeBlock(null)).toBe(0)
  })

  describe('compared', () => {
    /** Blocks 4 to 6 of the column read in one row: pages 0, 1 and 3 are where they open. */
    const MERGED: Row[] = [
      {
        left: [
          [4, 30],
          [5, 30],
          [6, 30],
        ],
        right: [[4, 90]],
      },
    ]

    it('is the block opening on the page, inside the row', () => {
      const el = compared(MERGED)
      el.scrollLeft = PAGE * 3
      expect(topOf(el).block, 'the block the row is named from').toBe(4)
      expect(resumeBlock(el)).toBe(6)
    })

    it('is the first of the blocks opening on the page', () => {
      const el = compared([
        {
          left: [
            [0, 25],
            [1, 5],
            [2, 5],
          ],
          right: [[0, 35]],
        },
      ])
      el.scrollLeft = PAGE
      expect(resumeBlock(el)).toBe(1)
    })

    it('is the block carried onto a page none opens on, not the row’s first', () => {
      const el = compared(MERGED)
      el.scrollLeft = PAGE * 2
      expect(resumeBlock(el)).toBe(5)
    })

    it('is the row’s block where the page holds only the other column’s words', () => {
      const el = compared([
        { left: [[3, 10]], right: [[3, 30]] },
        { left: [[4, 10]], right: [[4, 10]] },
      ])
      el.scrollLeft = PAGE
      expect(resumeBlock(el)).toBe(3)
    })

    it('is the last block of the column read to open before a page of the row holding none of it', () => {
      const el = compared([
        {
          left: [
            [4, 30],
            [5, 10],
          ],
          right: [[4, 90]],
        },
      ])
      el.scrollLeft = PAGE * 3
      expect(topOf(el).block, 'the block the row is named from').toBe(4)
      expect(resumeBlock(el)).toBe(5)
    })
  })
})

describe('a match on the page', () => {
  it('in a single column is on it while its own character is', () => {
    const el = text(true, ['short', LONG])
    el.scrollLeft = PAGE
    expect(matchShows(el, { block: 1, start: 15 })).toBe(true)
    expect(matchShows(el, { block: 1, start: 14 })).toBe(false)
    expect(matchShows(el, { block: 1, start: 35 })).toBe(false)
  })

  it('compared is on it while the column read holds that character there', () => {
    const el = compared(THREE_PAGES)
    el.scrollLeft = PAGE
    expect(matchShows(el, { block: 0, start: 20 })).toBe(true)
    expect(matchShows(el, { block: 0, start: 39 })).toBe(true)
    expect(matchShows(el, { block: 0, start: 19 })).toBe(false)
    expect(matchShows(el, { block: 0, start: 40 })).toBe(false)
  })

  it('compared is looked for in the row holding its block', () => {
    const el = compared([
      { left: [[0, 25]], right: [[0, 25]] },
      { left: [[1, 10]], right: [[1, 10]] },
    ])
    el.scrollLeft = 2 * PAGE
    expect(matchShows(el, { block: 1, start: 2 })).toBe(true)
    el.scrollLeft = PAGE
    expect(matchShows(el, { block: 1, start: 2 })).toBe(false)
  })

  it('compared is not on it where only another column draws its block', () => {
    const el = compared([{ left: [[0, 45]], right: [[9, 45]] }])
    expect(matchShows(el, { block: 9, start: 0 })).toBe(false)
  })
})

describe('the character the reader is at, stepping through matches', () => {
  it('outside a compared text is the anchor’s own, with a character read on the page', () => {
    const el = text(true, ['short', LONG])
    el.scrollLeft = PAGE
    const anchor = { block: 1, at: 15, read: { block: 1, at: 20 } }
    expect(readingAt(el, anchor)).toBe(anchor)
  })

  it('compared is the character read while that is on the screen, ahead of the anchor’s own', () => {
    const el = compared(THREE_PAGES)
    el.scrollLeft = PAGE
    const anchor = { block: 0, at: 21, read: { block: 0, at: 30 } }
    expect(readingAt(el, anchor)).toBe(anchor.read)
  })

  it('compared is the anchor’s own once the character read is off the screen', () => {
    const el = compared(THREE_PAGES)
    el.scrollLeft = PAGE
    const anchor = { block: 0, at: 21, read: { block: 0, at: 5 } }
    expect(readingAt(el, anchor)).toBe(anchor)
  })

  it('compared is the first of the column read on the screen once both are off it', () => {
    const el = compared(THREE_PAGES)
    el.scrollLeft = 2 * PAGE
    const anchor = { block: 0, at: 0, read: { block: 0, at: 25 } }
    expect(readingAt(el, anchor)).toEqual({ block: 0, at: 40 })
  })

  it('compared is the anchor’s own where the screen holds none of the column read', () => {
    const el = compared([LEFT_ENDS_FIRST])
    el.scrollLeft = PAGE
    const anchor = { block: 0, at: 3 }
    expect(readingAt(el, anchor)).toBe(anchor)
  })
})

describe('an image loading in a compared text, paged', () => {
  /** Three one-page rows, blocks 0 to 2, with an image in row `at`. */
  const withImage = (at: number) => {
    const el = compared(
      [0, 1, 2].map((block) => ({
        left: [[block, 5]],
        right: [[block, 5]],
      })) as Row[],
    )
    const image = document.createElement('img')
    el.children[at].append(image)
    return { el, image }
  }

  it('moves what is read up to the row of the character read, later than the anchor’s own', () => {
    const anchor = { block: 0, at: 0, read: { block: 2, at: 3 } }
    const between = withImage(1)
    expect(movesAnchor(between.el, anchor, between.image)).toBe(true)
    const inside = withImage(2)
    expect(movesAnchor(inside.el, anchor, inside.image)).toBe(true)
  })

  it('does not move what is read from a row after the character read', () => {
    const anchor = { block: 0, at: 0, read: { block: 1, at: 3 } }
    const after = withImage(2)
    expect(movesAnchor(after.el, anchor, after.image)).toBe(false)
  })

  it('with no character read goes by the anchor’s own row', () => {
    const anchor = { block: 1, at: 0 }
    const before = withImage(0)
    expect(movesAnchor(before.el, anchor, before.image)).toBe(true)
    const after = withImage(2)
    expect(movesAnchor(after.el, anchor, after.image)).toBe(false)
  })
})

describe('the child holding a block, among thousands', () => {
  /** A paged text of `children`, each a block's number or `null` for a figure, which names none. */
  const laid = (children: (number | null)[]) => {
    const el = document.createElement('div')
    el.dataset.paged = ''
    for (const block of children) {
      if (block === null) {
        const f = document.createElement('figure')
        f.dataset.figure = ''
        f.append(document.createElement('img'))
        el.append(f)
      } else {
        const p = document.createElement('p')
        p.dataset.block = String(block)
        el.append(p)
      }
    }
    return el
  }
  /** A paged text of `count` blocks, a figure before every seventh and one after the last. */
  const long = (count: number) =>
    laid([
      ...Array.from({ length: count }, (_, block) =>
        block % 7 === 0 ? [null, block] : [block],
      ).flat(),
      null,
    ])
  /** `count` figures in a row. */
  const figures = (count: number) => Array<null>(count).fill(null)
  /** The image of the figure set before block `block`. */
  const imageBefore = (el: HTMLElement, block: number) =>
    el.querySelector(`[data-block="${block}"]`)?.previousElementSibling
      ?.firstElementChild as Element
  /** Counts the `data-block`s read until the test ends. */
  const counted = () => {
    const read = vi.spyOn(HTMLElement.prototype, 'dataset', 'get')
    onTestFinished(() => read.mockRestore())
    return () => read.mock.calls.length
  }

  it('is the block itself, past the figures before it and before those after it', () => {
    const el = long(300)
    for (const block of [0, 7, 70, 140, 294]) {
      const image = imageBefore(el, block)
      expect(movesAnchor(el, { block, at: 0 }, image)).toBe(true)
      expect(movesAnchor(el, { block: block - 1, at: 0 }, image)).toBe(
        block === 0,
      )
    }
    const last = el.lastElementChild?.firstElementChild as Element
    expect(movesAnchor(el, { block: 299, at: 0 }, last)).toBe(false)
  })

  it.each([
    ['figures between blocks', long(60)],
    [
      'long runs of figures before and after',
      laid([...figures(40), 0, 1, 2, ...figures(40)]),
    ],
    [
      'runs of figures between blocks',
      laid([0, ...figures(9), 1, 2, ...figures(30), 3, null]),
    ],
    ['figures alone', laid(figures(5))],
    ['blocks alone', laid([0, 1, 2])],
    // Compared, a row whose read column is empty names the block the row
    // after it names; the figures here are only how the test tells the last
    // such child from an earlier one.
    [
      'children naming one block, figures between them',
      laid([0, 1, null, 1, null, 1, 2]),
    ],
  ])(
    'is the last child to start at or before the block, as reading every child finds it, with %s',
    (_, el) => {
      const children = [...el.children] as HTMLElement[]
      const images = [...el.querySelectorAll('img')]
      const named = children.filter((c) => c.dataset.block !== undefined)
      for (let block = -1; block <= named.length; block++) {
        const held = children.findLast((c) => Number(c.dataset.block) <= block)
        for (const image of images) {
          expect(movesAnchor(el, { block, at: 0 }, image)).toBe(
            !held ||
              (held.compareDocumentPosition(image) &
                Node.DOCUMENT_POSITION_PRECEDING) !==
                0,
          )
        }
      }
    },
  )

  it('is found reading a few of the children, not all of them', () => {
    const el = long(3000)
    const image = imageBefore(el, 1400)
    const reads = counted()
    expect(movesAnchor(el, { block: 1400, at: 0 }, image)).toBe(true)
    expect(reads()).toBeLessThan(100)
  })

  it('reads a long run of figures after the block once, not at every halving', () => {
    const el = laid([...figures(400), 0, 1, 2, ...figures(400)])
    const last = el.lastElementChild?.firstElementChild as Element
    const reads = counted()
    expect(movesAnchor(el, { block: 2, at: 0 }, last)).toBe(false)
    expect(reads()).toBeLessThan(el.children.length)
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

describe('a word brought into sight', () => {
  /** How long the text runs along its reading axis. */
  const LENGTH = 5000
  const START = 1000

  /** A screen `start` pixels into the text, along its reading axis. */
  function screenOf({ paged = false, vertical = false } = {}, start = START) {
    const el = document.createElement('div')
    if (paged) el.dataset.paged = ''
    if (vertical) el.dataset.vertical = ''
    el.getBoundingClientRect = () => new DOMRect(0, 0, PAGE, TALL)
    Object.defineProperties(el, {
      clientWidth: { value: PAGE },
      clientHeight: { value: TALL },
      scrollWidth: { value: LENGTH },
      scrollHeight: { value: LENGTH },
    })
    el.scrollTo = ((to: ScrollToOptions) => {
      if (to.left !== undefined) el.scrollLeft = to.left
      if (to.top !== undefined) el.scrollTop = to.top
    }) as typeof el.scrollTo
    el.scrollBy = ((by: ScrollToOptions) => {
      el.scrollLeft += by.left ?? 0
      el.scrollTop += by.top ?? 0
    }) as typeof el.scrollBy
    if (vertical && !paged) el.scrollLeft = 0 - start
    else if (paged && !vertical) el.scrollLeft = start
    else el.scrollTop = start
    return el
  }

  const wordOf = (...boxes: DOMRect[]) =>
    ({ getClientRects: () => boxes }) as unknown as Range
  /** A word's box, `top` down the screen and `left` across it. */
  const wordAt = (top: number, left = 100) => new DOMRect(left, top, 60, 20)
  /** A cover across the whole screen, from `top` down to `bottom`. */
  const band = (top: number, bottom: number) =>
    new DOMRect(0, top, PAGE, bottom - top)
  /** A cover down the whole screen, from `left` across to `right`. */
  const strip = (left: number, right: number) =>
    new DOMRect(left, 0, right - left, TALL)

  describe('scrolled', () => {
    it('scrolls on by the least that shows a word past the end of the screen', () => {
      const el = screenOf()
      bringIntoSight(el, wordOf(wordAt(500)), [])
      expect(el.scrollTop).toBe(START + 120)
    })

    it('scrolls back by the least that shows a word before the screen', () => {
      const el = screenOf()
      bringIntoSight(el, wordOf(wordAt(-50)), [])
      expect(el.scrollTop).toBe(START - 50)
    })

    it('lands on a whole pixel with all of the word showing, going on', () => {
      const el = screenOf()
      bringIntoSight(el, wordOf(wordAt(500.5)), [])
      expect(el.scrollTop).toBe(START + 121)
    })

    it('lands on a whole pixel with all of the word showing, going back', () => {
      const el = screenOf()
      bringIntoSight(el, wordOf(wordAt(-50.5)), [])
      expect(el.scrollTop).toBe(START - 51)
    })

    it('shows the last line of a word running on past the screen', () => {
      const el = screenOf()
      bringIntoSight(el, wordOf(wordAt(390), wordAt(410)), [])
      expect(el.scrollTop).toBe(START + 30)
    })

    it('shows the first line of a word running back before the screen', () => {
      const el = screenOf()
      bringIntoSight(el, wordOf(wordAt(-40), wordAt(-20)), [])
      expect(el.scrollTop).toBe(START - 40)
    })

    it('leaves a word in sight where it is', () => {
      const el = screenOf()
      bringIntoSight(el, wordOf(wordAt(100)), [])
      expect(el.scrollTop).toBe(START)
    })

    it.each([-0.4, 380.4])(
      'counts a word at %f, a hair over the edge, as in sight',
      (top) => {
        const el = screenOf()
        bringIntoSight(el, wordOf(wordAt(top)), [])
        expect(el.scrollTop).toBe(START)
      },
    )

    it('clears a cover the word lies under', () => {
      const el = screenOf()
      bringIntoSight(el, wordOf(wordAt(350)), [band(300, TALL)])
      expect(el.scrollTop).toBe(START + 70)
    })

    it('takes the shorter way back past a cover in the middle', () => {
      const el = screenOf()
      bringIntoSight(el, wordOf(wordAt(230)), [band(150, 250)])
      expect(el.scrollTop).toBe(START - 20)
    })

    it('takes the shorter way on past a cover in the middle', () => {
      const el = screenOf()
      bringIntoSight(el, wordOf(wordAt(150)), [band(150, 250)])
      expect(el.scrollTop).toBe(START + 20)
    })

    it('takes the longer way where the book does not reach back the shorter', () => {
      const el = screenOf({}, 0)
      bringIntoSight(el, wordOf(wordAt(230)), [band(150, 250)])
      expect(el.scrollTop).toBe(100)
    })

    it('passes over a cover to one side of the word', () => {
      const el = screenOf()
      bringIntoSight(el, wordOf(wordAt(500)), [strip(300, PAGE)])
      expect(el.scrollTop).toBe(START + 120)
    })

    it('stays where a cover hides the whole screen', () => {
      const el = screenOf()
      bringIntoSight(el, wordOf(wordAt(100)), [band(0, TALL)])
      expect(el.scrollTop).toBe(START)
    })

    it('stays where the word is longer than the room a cover leaves', () => {
      const el = screenOf()
      const word = new DOMRect(100, 100, 60, 60)
      bringIntoSight(el, wordOf(word), [band(50, TALL)])
      expect(el.scrollTop).toBe(START)
    })

    it('stays at the start of the book where only going back would clear a cover', () => {
      const el = screenOf({}, 0)
      bringIntoSight(el, wordOf(wordAt(50)), [band(0, 100)])
      expect(el.scrollTop).toBe(0)
    })

    it('stays at the end of the book where only going on would clear a cover', () => {
      const end = LENGTH - TALL
      const el = screenOf({}, end)
      bringIntoSight(el, wordOf(wordAt(350)), [band(300, TALL)])
      expect(el.scrollTop).toBe(end)
    })

    it('stays for a cover level with one piece of a word and across from the other', () => {
      const el = screenOf()
      const word = wordOf(wordAt(100, 500), wordAt(120, 0))
      bringIntoSight(el, word, [new DOMRect(0, 90, 200, 25)])
      expect(el.scrollTop).toBe(START)
    })

    it('moves for a cover over one piece of a word on two lines', () => {
      const el = screenOf()
      const word = wordOf(wordAt(100, 500), wordAt(120, 0))
      bringIntoSight(el, word, [new DOMRect(0, 110, 200, 40)])
      expect(el.scrollTop).toBe(START + 30)
    })
  })

  describe('scrolled, in vertical text', () => {
    const vertical = { vertical: true }
    const columnAt = (left: number, top = 100) => new DOMRect(left, top, 30, 60)

    it('scrolls on, leftward, to a column past the left of the screen', () => {
      const el = screenOf(vertical)
      bringIntoSight(el, wordOf(columnAt(-100)), [])
      expect(el.scrollLeft).toBe(-START - 100)
    })

    it('scrolls back, rightward, to a column past the right of the screen', () => {
      const el = screenOf(vertical)
      bringIntoSight(el, wordOf(columnAt(650)), [])
      expect(el.scrollLeft).toBe(-START + 80)
    })

    it('clears a cover narrower than the text', () => {
      const el = screenOf(vertical)
      const cover = new DOMRect(0, 300, 200, 100)
      bringIntoSight(el, wordOf(columnAt(100, 320)), [cover])
      expect(el.scrollLeft).toBe(-START - 100)
    })

    it('passes over a cover above the word’s own lines', () => {
      const el = screenOf(vertical)
      bringIntoSight(el, wordOf(columnAt(-100, 200)), [band(0, 100)])
      expect(el.scrollLeft).toBe(-START - 100)
    })

    it('stays where a cover lies as wide as the text', () => {
      const el = screenOf(vertical)
      bringIntoSight(el, wordOf(columnAt(100, 320)), [band(300, TALL)])
      expect(el.scrollLeft).toBe(-START)
    })

    it('stays at the start of the book where only going back would clear a cover', () => {
      const el = screenOf(vertical, 0)
      const cover = new DOMRect(400, 300, 200, 100)
      bringIntoSight(el, wordOf(columnAt(500, 320)), [cover])
      expect(el.scrollLeft).toBe(0)
    })
  })

  describe('paged', () => {
    const paged = { paged: true }
    const ON_PAGE_TWO = 2 * PAGE

    it('turns on to the page a word starts on', () => {
      const el = screenOf(paged, ON_PAGE_TWO)
      bringIntoSight(el, wordOf(wordAt(100, 700)), [])
      expect(el.scrollLeft).toBe(3 * PAGE)
    })

    it('turns back to the page a word starts on', () => {
      const el = screenOf(paged, ON_PAGE_TWO)
      bringIntoSight(el, wordOf(wordAt(100, -700)), [])
      expect(el.scrollLeft).toBe(0)
    })

    it('stays on the page a word is on', () => {
      const el = screenOf(paged, ON_PAGE_TWO)
      bringIntoSight(el, wordOf(wordAt(100, 100)), [])
      expect(el.scrollLeft).toBe(ON_PAGE_TWO)
    })

    it('goes by the first piece of a word broken over a turn', () => {
      const el = screenOf(paged, ON_PAGE_TWO)
      bringIntoSight(el, wordOf(wordAt(100, 500), wordAt(0, 700)), [])
      expect(el.scrollLeft).toBe(ON_PAGE_TWO)
    })

    it('stays on the word’s page whatever is drawn over it', () => {
      const el = screenOf(paged, ON_PAGE_TWO)
      bringIntoSight(el, wordOf(wordAt(100, 100)), [strip(0, 200)])
      expect(el.scrollLeft).toBe(ON_PAGE_TWO)
    })

    it('moves nothing for a word with no boxes', () => {
      const el = screenOf(paged, ON_PAGE_TWO)
      bringIntoSight(el, wordOf(), [])
      expect(el.scrollLeft).toBe(ON_PAGE_TWO)
    })

    it('turns down the screen in vertical text', () => {
      const el = screenOf({ paged: true, vertical: true }, 2 * TALL)
      bringIntoSight(el, wordOf(wordAt(500)), [])
      expect(el.scrollTop).toBe(3 * TALL)
    })

    it('turns back up the screen in vertical text', () => {
      const el = screenOf({ paged: true, vertical: true }, 2 * TALL)
      bringIntoSight(el, wordOf(wordAt(-100)), [])
      expect(el.scrollTop).toBe(TALL)
    })
  })
})
