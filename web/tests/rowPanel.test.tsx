import { afterEach, describe, expect, it, vi } from 'vitest'
import { inRowView, layUnderRow } from '../src/tilePanel'

/**
 * `layUnderRow` places the Continue row's panel from the layout, which only
 * a browser draws, and `inRowView` asks of it whether the panel's card is in
 * the row's view; these stub the boxes they read, as `tilePanel.test.tsx`
 * stubs `layPanel`'s. The section is the page area, 976 px wide from 264;
 * the row runs 4 px past it on each side and keeps 8 px under its cards.
 */

afterEach(() => {
  vi.restoreAllMocks()
})

type Box = { left: number; right: number; width: number }

const SECTION = { left: 264, right: 1240, width: 976, top: 120, bottom: 420 }
const ROW = { left: 260, right: 1244, width: 984, top: 160, bottom: 380 }
const CARD_TOP = 164
const CARD_FOOT = 372

/** The row's second card, wholly inside the area. */
const INSIDE = { left: 431, right: 575, width: 144 }

/**
 * `card` in the row and the panel after the row in its section, under
 * `.library > main` unless `inArea` is false. The row's laid gap (23 px)
 * is not `--tile-gap` (16 px), as where `useContinueRow` spreads the cards.
 */
function drawn(at: Box, inArea = true) {
  const library = document.createElement('div')
  library.className = 'library'
  library.style.setProperty('--tile-gap', '16px')
  const main = document.createElement('main')
  const section = document.createElement('section')
  section.className = 'continue'
  const row = document.createElement('ul')
  row.style.columnGap = '23px'
  row.style.paddingBottom = '8px'
  const card = document.createElement('li')
  const panel = document.createElement('div')
  row.append(card)
  section.append(row, panel)
  if (inArea) {
    main.append(section)
    library.append(main)
  } else {
    library.append(section)
  }
  document.body.append(library)
  vi.spyOn(section, 'getBoundingClientRect').mockReturnValue(SECTION as DOMRect)
  vi.spyOn(row, 'getBoundingClientRect').mockReturnValue(ROW as DOMRect)
  vi.spyOn(card, 'getBoundingClientRect').mockReturnValue({
    ...at,
    top: CARD_TOP,
    bottom: CARD_FOOT,
  } as DOMRect)
  return { library, section, row, card, panel }
}

function laid(panel: HTMLElement) {
  return { left: panel.style.left, width: panel.style.width }
}

describe('layUnderRow', () => {
  it('lies from the card’s left edge, two cards and the row’s own gap wide', () => {
    const { library, section, row, card, panel } = drawn(INSIDE)
    try {
      const style = getComputedStyle(row)
      expect({
        gap: style.columnGap,
        tileGap: style.getPropertyValue('--tile-gap'),
      }).toEqual({ gap: '23px', tileGap: '16px' })
      layUnderRow(panel, card, row, section)
      expect(laid(panel)).toEqual({ left: '167px', width: '311px' })
    } finally {
      library.remove()
    }
  })

  it('starts at the area’s start, full width, for a card half past it', () => {
    const { library, section, row, card, panel } = drawn({
      left: 192,
      right: 336,
      width: 144,
    })
    try {
      layUnderRow(panel, card, row, section)
      expect(laid(panel)).toEqual({ left: '0px', width: '311px' })
    } finally {
      library.remove()
    }
  })

  it('ends at the area’s end, full width, for a card half past it', () => {
    const { library, section, row, card, panel } = drawn({
      left: 1168,
      right: 1312,
      width: 144,
    })
    try {
      layUnderRow(panel, card, row, section)
      // 665 + 311 is the section's 976.
      expect(laid(panel)).toEqual({ left: '665px', width: '311px' })
    } finally {
      library.remove()
    }
  })

  it('lies from the row’s foot, under the room the row keeps below its cards', () => {
    const { library, section, row, card, panel } = drawn(INSIDE)
    try {
      expect({
        row: row.getBoundingClientRect().bottom,
        card: card.getBoundingClientRect().bottom,
      }).toEqual({ row: 380, card: 372 })
      layUnderRow(panel, card, row, section)
      expect(panel.style.top).toBe('260px')
    } finally {
      library.remove()
    }
  })

  it('lays nothing outside a page area', () => {
    const { library, section, row, card, panel } = drawn(INSIDE, false)
    try {
      layUnderRow(panel, card, row, section)
      expect(panel.getAttribute('style')).toBeNull()
    } finally {
      library.remove()
    }
  })
})

describe('inRowView', () => {
  /**
   * A card at `left`, `width` px wide, in a row whose view, its padding box,
   * runs from 20 to 1020: the list's box at 16, a 4 px border.
   */
  function seen(left: number, width = 144) {
    const row = document.createElement('ul')
    const card = document.createElement('li')
    row.append(card)
    vi.spyOn(row, 'getBoundingClientRect').mockReturnValue({
      left: 16,
    } as DOMRect)
    Object.defineProperty(row, 'clientLeft', { value: 4 })
    Object.defineProperty(row, 'clientWidth', { value: 1000 })
    vi.spyOn(card, 'getBoundingClientRect').mockReturnValue({
      left,
      right: left + width,
    } as DOMRect)
    return inRowView(card, row)
  }

  it('holds a card with any of it in the view, and not one wholly past either edge', () => {
    expect({
      pastStart: seen(20 - 144),
      pixelIn: seen(21 - 144),
      inside: seen(400),
      lastPixel: seen(1019),
      pastEnd: seen(1020),
    }).toEqual({
      pastStart: false,
      pixelIn: true,
      inside: true,
      lastPixel: true,
      pastEnd: false,
    })
  })
})
