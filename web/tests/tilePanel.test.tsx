import { afterEach, describe, expect, it, vi } from 'vitest'
import { laidGapOf, layPanel, panelSpan } from '../src/tilePanel'

/**
 * `tilePanel.ts` places an open volume's panel from the layout, which only a
 * browser draws, so `Series.panel.browser.test.tsx` reaches it there and the
 * coverage run does not. These hand `panelSpan` and `laidGapOf` what a
 * layout would, and stub the reads `layPanel` makes.
 */

afterEach(() => {
  vi.restoreAllMocks()
})

describe('panelSpan', () => {
  const AREA = { start: 24, end: 1000 }

  it('lies from the tile’s left edge, two tiles and the gap wide, where that fits', () => {
    expect(panelSpan({ left: 100, right: 230, width: 130 }, 16, AREA)).toEqual({
      left: 100,
      width: 276,
    })
  })

  it('lies back from the tile’s right edge where rightward would pass the area', () => {
    expect(panelSpan({ left: 870, right: 1000, width: 130 }, 16, AREA)).toEqual(
      { left: 724, width: 276 },
    )
  })

  it('is the area’s width from either column where two tiles and the gap are wider', () => {
    const phone = { start: 16, end: 374 }
    expect({
      left: panelSpan({ left: 16, right: 187, width: 171 }, 16, phone),
      right: panelSpan({ left: 203, right: 374, width: 171 }, 16, phone),
    }).toEqual({
      left: { left: 16, width: 358 },
      right: { left: 16, width: 358 },
    })
  })

  it('is held to the area’s start when its tile ends a fraction short of the area’s end', () => {
    expect(
      panelSpan({ left: 202.75, right: 373.75, width: 171 }, 16, {
        start: 16,
        end: 374,
      }),
    ).toEqual({ left: 16, width: 358 })
  })

  it('lies from the tile’s left edge, held to the area’s end, where two tiles and the gap run half a pixel past it', () => {
    // The second-to-last column, its gap read a fraction wider than laid out.
    expect(
      panelSpan({ left: 724.5, right: 854.5, width: 130 }, 16, AREA),
    ).toEqual({ left: 724.5, width: 275.5 })
  })
})

describe('laidGapOf', () => {
  it('is the width the tracks leave, over the gaps between them', () => {
    expect(laidGapOf('144px 144px 144px', 500)).toBe(34)
  })

  it('reads past the line names a resolved track list holds', () => {
    expect(laidGapOf('[a] 144px [b] 144px', 300)).toBe(12)
  })

  it('has no gap to read from one track or none', () => {
    expect({
      one: laidGapOf('144px', 300),
      none: laidGapOf('', 300),
    }).toEqual({ one: undefined, none: undefined })
  })
})

describe('layPanel', () => {
  /** The last of a row's columns, and the one before it. */
  const LAST = { left: 870.5, right: 1000.5, width: 130 }
  const BEFORE_LAST = { left: 724.5, right: 854.5, width: 130 }

  /**
   * `tile` with its panel in a grid, `.library > main` around the grid
   * unless `inArea` is false. The area is 1024.5 px wide at the window's
   * left with 24 px of padding, its `clientWidth` rounded down to 1024, and
   * the grid fills its content box, from 24 to 1000.5.
   */
  function drawn(
    at: { left: number; right: number; width: number },
    inArea = true,
  ) {
    const library = document.createElement('div')
    library.className = 'library'
    const main = document.createElement('main')
    main.style.padding = '0 24px'
    const grid = document.createElement('ul')
    grid.style.columnGap = '16px'
    const tile = document.createElement('li')
    const panel = document.createElement('div')
    tile.append(panel)
    grid.append(tile)
    if (inArea) {
      main.append(grid)
      library.append(main)
    } else {
      library.append(grid)
    }
    document.body.append(library)
    vi.spyOn(main, 'getBoundingClientRect').mockReturnValue({
      left: 0,
      right: 1024.5,
      width: 1024.5,
    } as DOMRect)
    Object.defineProperty(main, 'clientLeft', { value: 0 })
    Object.defineProperty(main, 'clientWidth', { value: 1024 })
    vi.spyOn(grid, 'getBoundingClientRect').mockReturnValue({
      left: 24,
      right: 1000.5,
      width: 976.5,
    } as DOMRect)
    vi.spyOn(tile, 'getBoundingClientRect').mockReturnValue(at as DOMRect)
    return { library, tile, panel }
  }

  it('lays a panel from its tile, its grid’s gap and its page area', () => {
    const { library, tile, panel } = drawn(LAST)
    try {
      layPanel(panel, tile)
      expect({ left: panel.style.left, width: panel.style.width }).toEqual({
        left: '-146px',
        width: '276px',
      })
    } finally {
      library.remove()
    }
  })

  it('ends the area where the grid’s own box ends, not at the area’s rounded clientWidth', () => {
    const { library, tile, panel } = drawn(BEFORE_LAST)
    try {
      layPanel(panel, tile)
      expect({ left: panel.style.left, width: panel.style.width }).toEqual({
        left: '0px',
        width: '276px',
      })
    } finally {
      library.remove()
    }
  })

  it('lays nothing outside a page area', () => {
    const { library, tile, panel } = drawn(LAST, false)
    try {
      layPanel(panel, tile)
      expect(panel.getAttribute('style')).toBeNull()
    } finally {
      library.remove()
    }
  })
})
