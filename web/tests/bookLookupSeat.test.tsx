import { afterEach, describe, expect, it } from 'vitest'
import { seatLookup, unseatLookup } from '../src/bookLookupSeat'

/**
 * Where a look-up's panel sits at phone size, on a screen jsdom cannot lay
 * out: the reader, the text, the bars and the panel each answer a stubbed box,
 * and the panel's edges follow its `data-side` and the bars it is told to
 * clear, as the stylesheet resolves them. The height the stylesheet caps the
 * panel at needs a real layout: `bookLookupSeat.browser.test.tsx` holds that.
 */

const SCREEN = new DOMRect(0, 0, 390, 844)
/** The text's box, short of the screen at both ends. */
const SHOWN = new DOMRect(0, 40, 390, 764)
/** What the stylesheet keeps between the panel and the edge it is set against. */
const MARGIN = 8
const TOP_EDGE = MARGIN
const BOTTOM_EDGE = SCREEN.bottom - MARGIN
const HEAD = 40
const PADDING = 12
const BORDER_TOP = 1
const BORDER_BOTTOM = 2
/** The least a panel can be and still show its Close. */
const LEAST = HEAD + PADDING + BORDER_TOP + BORDER_BOTTOM
/** The clear space the panel keeps from its word. */
const GAP = 8
const WHOLE_WIDTH: [number, number] = [8, 382]
const SEAT = ['--lookup-above', '--lookup-below', '--lookup-room']

afterEach(() => {
  document.body.replaceChildren()
})

function panelAcross([left, right]: [number, number]) {
  const panel = document.createElement('div')
  panel.style.paddingBottom = `${PADDING}px`
  panel.style.borderTopWidth = `${BORDER_TOP}px`
  panel.style.borderBottomWidth = `${BORDER_BOTTOM}px`
  const head = document.createElement('div')
  head.className = 'reader-panel-head'
  Object.defineProperty(head, 'clientHeight', { value: HEAD })
  panel.append(head)
  const inset = (name: string) =>
    Number.parseFloat(panel.style.getPropertyValue(name) || '0')
  panel.getBoundingClientRect = () => {
    const width = right - left
    if (panel.dataset.side === 'top') {
      return new DOMRect(left, inset('--lookup-above') + MARGIN, width, 100)
    }
    const bottom = SCREEN.bottom - inset('--lookup-below') - MARGIN
    return new DOMRect(left, bottom - 100, width, 100)
  }
  return panel
}

interface Layout {
  screen?: DOMRect
  across?: [number, number]
  bars?: [name: string, box: DOMRect][]
  text?: boolean
}

/** A panel in a reader laid out as `layout` says. */
function seated({
  screen = SCREEN,
  across = WHOLE_WIDTH,
  bars = [],
  text = true,
}: Layout = {}) {
  const reader = document.createElement('div')
  reader.className = 'book-reader'
  reader.getBoundingClientRect = () => screen
  const boxed = (name: string, box: DOMRect) => {
    const el = document.createElement('div')
    el.className = name
    el.getBoundingClientRect = () => box
    reader.append(el)
  }
  if (text) boxed('book-text', SHOWN)
  for (const [name, box] of bars) boxed(name, box)
  const panel = panelAcross(across)
  reader.append(panel)
  document.body.append(reader)
  return panel
}

const wordOf = (...boxes: DOMRect[]) =>
  ({ getClientRects: () => boxes }) as unknown as Range
/** A character's box, `top` down the screen and `left` across it. */
const charAt = (top: number, left = 100) => new DOMRect(left, top, 30, 20)
/** A bar along the top of the screen, down to `bottom`. */
const barDownTo = (bottom: number) => new DOMRect(0, 0, 390, bottom)
/** A bar along the bottom of the screen, up to `top`. */
const barUpTo = (top: number) => new DOMRect(0, top, 390, SCREEN.bottom - top)
const seatOf = (panel: HTMLElement, name: string) =>
  panel.style.getPropertyValue(name)

describe('the side a look-up panel takes', () => {
  it.each([
    [200, 'bottom'],
    [600, 'top'],
  ])('for a word %i down the screen is the %s', (top, side) => {
    const panel = seated()
    expect(seatLookup(panel, wordOf(charAt(top)))).toBe(side)
    expect(panel.dataset.side).toBe(side)
  })

  it('is the bottom where the word has the same room each way', () => {
    const top = (TOP_EDGE + BOTTOM_EDGE - 20) / 2
    expect(seatLookup(seated(), wordOf(charAt(top)))).toBe('bottom')
  })

  it('is the side it is told to keep, whatever the word would choose', () => {
    const panel = seated()
    expect(seatLookup(panel, wordOf(charAt(200)), 'top')).toBe('top')
    expect(panel.dataset.side).toBe('top')
  })

  it('counts the room from a bar over the top of the screen', () => {
    const word = wordOf(charAt(500))
    expect(seatLookup(seated(), word)).toBe('top')
    const bars: Layout['bars'] = [['reader-top-bar', barDownTo(300)]]
    expect(seatLookup(seated({ bars }), word)).toBe('bottom')
  })

  it('counts the room from a bar over the bottom of the screen', () => {
    const word = wordOf(charAt(300))
    expect(seatLookup(seated(), word)).toBe('bottom')
    const bars: Layout['bars'] = [['reader-bottom-bar', barUpTo(544)]]
    expect(seatLookup(seated({ bars }), word)).toBe('top')
  })

  it('measures the room above from the highest piece of a word on two lines', () => {
    const word = wordOf(charAt(300), charAt(500))
    expect(seatLookup(seated(), word)).toBe('bottom')
  })

  it('measures the room below from the lowest piece of a word on two lines', () => {
    const word = wordOf(charAt(330), charAt(540))
    expect(seatLookup(seated(), word)).toBe('top')
  })

  it('goes by the piece across from a panel narrower than the screen', () => {
    const panel = seated({ across: [8, 200] })
    const word = wordOf(charAt(100, 300), charAt(600, 50))
    expect(seatLookup(panel, word)).toBe('top')
  })

  it('goes by the whole word where no piece lies across from the panel', () => {
    const panel = seated({ across: [8, 200] })
    expect(seatLookup(panel, wordOf(charAt(600, 300)))).toBe('top')
    expect(panel.dataset.side).toBe('top')
  })
})

describe('what a look-up panel stays clear of', () => {
  it.each(['reader-top-bar', 'book-columns', 'book-pinning'])(
    'stays under .%s',
    (name) => {
      const panel = seated({ bars: [[name, barDownTo(140)]] })
      seatLookup(panel, wordOf(charAt(300)))
      expect(seatOf(panel, '--lookup-above')).toBe('140px')
    },
  )

  it('stays over the bottom bar', () => {
    const panel = seated({ bars: [['reader-bottom-bar', barUpTo(744)]] })
    seatLookup(panel, wordOf(charAt(300)))
    expect(seatOf(panel, '--lookup-below')).toBe('100px')
  })

  it('stays under the bar that reaches furthest', () => {
    const panel = seated({
      bars: [
        ['reader-top-bar', barDownTo(60)],
        ['book-columns', barDownTo(140)],
        ['book-pinning', barDownTo(100)],
      ],
    })
    seatLookup(panel, wordOf(charAt(300)))
    expect(seatOf(panel, '--lookup-above')).toBe('140px')
  })

  it('counts a bar slid off the screen for nothing', () => {
    const panel = seated({
      bars: [
        ['reader-top-bar', new DOMRect(0, -80, 390, 60)],
        ['reader-bottom-bar', new DOMRect(0, 900, 390, 60)],
      ],
    })
    seatLookup(panel, wordOf(charAt(300)))
    expect(seatOf(panel, '--lookup-above')).toBe('0px')
    expect(seatOf(panel, '--lookup-below')).toBe('0px')
  })

  it('measures a bar from the edges of the reader, not of the window', () => {
    const panel = seated({
      screen: new DOMRect(0, 100, 390, 844),
      bars: [
        ['reader-top-bar', new DOMRect(0, 100, 390, 60)],
        ['reader-bottom-bar', new DOMRect(0, 900, 390, 44)],
      ],
    })
    seatLookup(panel, wordOf(charAt(300)))
    expect(seatOf(panel, '--lookup-above')).toBe('60px')
    expect(seatOf(panel, '--lookup-below')).toBe('44px')
  })
})

describe('the room a look-up panel is held to', () => {
  it('is what lies below the word, less the gap, at the bottom', () => {
    const panel = seated()
    seatLookup(panel, wordOf(charAt(300)))
    expect(seatOf(panel, '--lookup-room')).toBe(`${BOTTOM_EDGE - 320 - GAP}px`)
  })

  it('is what lies above the word, less the gap, at the top', () => {
    const panel = seated()
    seatLookup(panel, wordOf(charAt(600)))
    expect(seatOf(panel, '--lookup-room')).toBe(`${600 - TOP_EDGE - GAP}px`)
  })

  it('never leaves the panel too short to show its Close', () => {
    const panel = seated()
    seatLookup(panel, wordOf(new DOMRect(100, 10, 30, 820)))
    expect(seatOf(panel, '--lookup-room')).toBe(`${LEAST}px`)
  })

  it.each([
    ['right', [8, 200] as [number, number], 200],
    ['left', [100, 382] as [number, number], 70],
  ])(
    'is lifted where the word runs clear on the panel’s %s',
    (_, across, left) => {
      const panel = seated({ across })
      seatLookup(panel, wordOf(charAt(300, 120)))
      expect(seatOf(panel, '--lookup-room')).not.toBe('')
      seatLookup(panel, wordOf(charAt(300, left)))
      expect(seatOf(panel, '--lookup-room')).toBe('')
    },
  )
})

describe('a look-up panel with no word on the screen', () => {
  it.each([
    ['above the text', new DOMRect(100, 20, 30, 20)],
    ['below the text', new DOMRect(100, 804, 30, 20)],
    ['left of the text', new DOMRect(-30, 300, 30, 20)],
    ['right of the text', new DOMRect(390, 300, 30, 20)],
    ['with no width', new DOMRect(100, 300, 0, 20)],
    ['with no height', new DOMRect(100, 300, 30, 0)],
  ])('goes back to where it opens for a box %s', (_, box) => {
    const panel = seated()
    seatLookup(panel, wordOf(charAt(300)))
    expect(seatLookup(panel, wordOf(box))).toBeUndefined()
    expect(panel.dataset.side).toBeUndefined()
    for (const name of SEAT) expect(seatOf(panel, name)).toBe('')
  })

  it('holds the side it is told to keep, with no room to answer to', () => {
    const panel = seated()
    seatLookup(panel, wordOf(charAt(300)))
    expect(seatLookup(panel, undefined, 'top')).toBe('top')
    expect(panel.dataset.side).toBe('top')
    expect(seatOf(panel, '--lookup-room')).toBe('')
  })

  it('takes no side where the reader has no text to measure against', () => {
    const panel = seated({ text: false })
    expect(seatLookup(panel, wordOf(charAt(300)))).toBeUndefined()
    expect(panel.dataset.side).toBeUndefined()
  })

  it('takes no side outside a reader, even one it is told to keep', () => {
    const panel = panelAcross(WHOLE_WIDTH)
    document.body.append(panel)
    panel.dataset.side = 'top'
    expect(seatLookup(panel, wordOf(charAt(300)), 'top')).toBeUndefined()
    expect(panel.dataset.side).toBeUndefined()
  })
})

describe('a kept side once the answer has lengthened the word', () => {
  it('gives way at the bottom where no room is left there', () => {
    const panel = seated()
    expect(seatLookup(panel, wordOf(charAt(780)), 'bottom', true)).toBe('top')
    expect(panel.dataset.side).toBe('top')
    expect(seatOf(panel, '--lookup-room')).toBe(`${780 - TOP_EDGE - GAP}px`)
  })

  it('gives way at the top where no room is left there', () => {
    const panel = seated()
    expect(seatLookup(panel, wordOf(charAt(44)), 'top', true)).toBe('bottom')
    expect(panel.dataset.side).toBe('bottom')
    expect(seatOf(panel, '--lookup-room')).toBe(`${BOTTOM_EDGE - 64 - GAP}px`)
  })

  it('holds where the word has not grown, however little room is left', () => {
    const panel = seated()
    expect(seatLookup(panel, wordOf(charAt(780)), 'bottom')).toBe('bottom')
    expect(seatOf(panel, '--lookup-room')).toBe(`${LEAST}px`)
  })

  it('holds while the room left is the least the panel can be', () => {
    const top = BOTTOM_EDGE - GAP - LEAST - 20
    const panel = seated()
    expect(seatLookup(panel, wordOf(charAt(top)), 'bottom', true)).toBe(
      'bottom',
    )
    expect(seatOf(panel, '--lookup-room')).toBe(`${LEAST}px`)
  })

  it('holds where the other side has less room still', () => {
    const word = wordOf(new DOMRect(100, 10, 30, 820))
    expect(seatLookup(seated(), word, 'bottom', true)).toBe('bottom')
  })

  it('holds where the other side has no more room', () => {
    const word = wordOf(new DOMRect(100, 14, 30, 816))
    expect(seatLookup(seated(), word, 'bottom', true)).toBe('bottom')
  })
})

describe('unseating a look-up panel', () => {
  it('clears its side and everything the seat wrote', () => {
    const panel = seated({ bars: [['reader-top-bar', barDownTo(60)]] })
    seatLookup(panel, wordOf(charAt(300)))
    for (const name of SEAT) expect(seatOf(panel, name)).not.toBe('')
    unseatLookup(panel)
    expect(panel.dataset.side).toBeUndefined()
    for (const name of SEAT) expect(seatOf(panel, name)).toBe('')
  })

  it('leaves a panel docked in the sidebar layout as it found it', () => {
    const panel = document.createElement('div')
    panel.setAttribute('style', 'width: 300px;')
    panel.dataset.state = 'open'
    unseatLookup(panel)
    expect(panel.getAttribute('style')).toBe('width: 300px;')
    expect(panel.dataset.state).toBe('open')
  })
})
