import { act } from 'react'
import { beforeEach, describe, expect, it, onTestFinished, vi } from 'vitest'
import { userEvent } from 'vitest/browser'
// **Required**, the same reason `Config.browser.test.tsx` gives.
import '../src/index.css'
import type { Me } from '../src/api'
import type { BrowsePrefs } from '../src/browseModule'
import * as library from '../src/library'
import { land, settleUntil } from './support/async'
import { mouse } from './support/mouse'
import { pointerRulesAs } from './support/pointer'
import { seriesPage } from './support/seriesPage'

/**
 * A volume's panel (O283) run past the end of a Series page that did not
 * scroll, beside a scrollbar that takes its width, as a classic one does.
 * The panel lies over the rows below, but the scrollbar it brings would
 * narrow the grid and move every tile: `library.css` keeps the gutter of
 * what scrolls the page while it draws the grid, so nothing moves.
 *
 * **Run only in the launch that shows scrollbars**, `vite.config.ts`'s
 * second `browser` instance: Playwright's headless Chromium hides them
 * otherwise, and a scrollbar of no width cannot narrow anything.
 */

vi.mock('../src/library', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/library')>()),
  oneSeries: vi.fn(),
  work: vi.fn(),
}))

const work = (id: number): library.WorkCard => ({
  id,
  slug: `kagami-v${String(id).padStart(2, '0')}`,
  title: `Kagami no Kuni no Harisugawa v${String(id).padStart(2, '0')}`,
  volume: id,
  aligned: false,
  pages: 180,
  editions: [
    {
      key: 'raw',
      label: 'Raw',
      members: 1,
      pages: 180,
      missing: 0,
      cover: null,
    },
  ],
  opens: 'raw',
})

/** The series `oneSeries` answers with `count` volumes. */
function serve(count: number) {
  const works = Array.from({ length: count }, (_, n) => work(n + 1))
  vi.mocked(library.oneSeries).mockResolvedValue({
    series: {
      id: 7,
      title: 'Kagami no Kuni no Harisugawa',
      works: works.length,
      editions: works.length,
      aligned: 0,
      added: 1,
      unread: works.length,
      cover: null,
      titles: [],
    },
    works,
  })
  vi.mocked(library.work).mockImplementation(async (slug) => ({
    ...(works.find((w) => w.slug === slug) as library.WorkCard),
    alignments: [],
  }))
}

/** An administrator who may download, and may not stamp. */
const ADMIN: Me = {
  user_id: 1,
  admin: true,
  can_download: true,
  can_stream: true,
  can_stamp: false,
}

beforeEach(() => {
  vi.clearAllMocks()
})

/** A real input, the React updates it causes drawn inside `act`. */
const real = (input: () => Promise<unknown>) =>
  act(async () => {
    await input()
  })

const rect = (el: Element | null) => {
  if (el === null) throw new Error('nothing drawn')
  return el.getBoundingClientRect()
}

/** The width a classic scrollbar takes here. */
function scrollbarWidth(): number {
  const probe = document.createElement('div')
  probe.style.cssText =
    'position: absolute; overflow-y: scroll; width: 100px; height: 10px'
  document.body.append(probe)
  const width = probe.offsetWidth - probe.clientWidth
  probe.remove()
  return width
}

/**
 * The page `width` by `height` px, laid out as `layout`, with the host the
 * window's height as the shell's root is, and drawn: the grid's face loaded
 * first, since the Title field is fitted as its panel mounts.
 */
async function drawn({
  width,
  height,
  layout = 'grid',
}: {
  width: number
  height: number
  layout?: BrowsePrefs['layout']
}) {
  const host = await seriesPage({
    width,
    height,
    panel: 'button',
    layout,
    who: ADMIN,
  })
  host.style.height = '100%'
  const main = host.querySelector<HTMLElement>('.library > main')
  if (main === null) throw new Error('no page area')
  await settleUntil(
    'the volumes or their table',
    () => main.querySelector('.volumes > li, table.editions') !== null,
    1000,
  )
  const face = getComputedStyle(main)
  await document.fonts.load(
    `${face.fontStyle} ${face.fontWeight} ${face.fontSize} ${face.fontFamily}`,
    'Kagami no Kuni no Harisugawa v01',
  )
  await document.fonts.ready
  const tiles = [...main.querySelectorAll<HTMLElement>('.volumes > li')]
  return { main, tiles }
}

/**
 * The first tile of the grid's last row, and how many rows the grid has.
 */
function lastRowFirst(tiles: HTMLElement[]) {
  const tops = [...new Set(tiles.map((tile) => rect(tile).top))]
  const last = tops[tops.length - 1]
  const tile = tiles.find((t) => rect(t).top === last)
  if (tile === undefined) throw new Error('no tiles')
  return { tile, rows: tops.length }
}

/** Each tile's left edge and width. */
const placesOf = (tiles: HTMLElement[]) =>
  tiles.map((tile) => {
    const { left, width } = rect(tile)
    return { left, width }
  })

/**
 * Opens `tile`'s panel as a mouse does, the pointer onto the tile and then
 * a click on its `i`, and waits for its alignment detail to be read.
 */
async function open(tile: HTMLElement) {
  await real(() => userEvent.hover(tile.querySelector(':scope > a') as Element))
  await real(() =>
    userEvent.click(tile.querySelector(':scope > .panel-open') as Element),
  )
  await settleUntil(
    'the panel’s alignment detail',
    () => {
      const detail = tile.querySelector('.tile-panel .alignment-panel')
      return detail !== null && detail.textContent !== 'Reading…'
    },
    1000,
  )
  return tile.querySelector('.tile-panel') as HTMLElement
}

describe('a volume’s panel run past the end of the page (O283)', () => {
  // Each page, the rows its grid draws, and a window's height that holds
  // the page but not the panel opened on the first tile of its last row:
  // one volume, and three full rows.
  it.each([
    ['one volume', 1, 1, 500],
    ['the last row’s first tile of three', 21, 3, 1050],
  ] as const)(
    'keeps every tile in place as a panel runs past the end of a page area that did not scroll, at 1280 px (%s)',
    async (_, count, rows, height) => {
      onTestFinished(pointerRulesAs('mouse'))
      serve(count)
      const { main, tiles } = await drawn({ width: 1280, height })
      const opened = lastRowFirst(tiles)
      // The fixture tells: the grid draws its rows, a scrollbar here takes
      // width, and the page area does not scroll.
      expect(opened.rows).toBe(rows)
      expect(rect(opened.tile).left).toBe(rect(tiles[0]).left)
      expect(scrollbarWidth()).toBeGreaterThan(0)
      expect(main.scrollHeight).toBeLessThanOrEqual(main.clientHeight)

      const before = placesOf(tiles)
      await open(opened.tile)
      expect(
        main.scrollHeight,
        'the page area’s height with the panel open',
      ).toBeGreaterThan(main.clientHeight)
      expect(placesOf(tiles), 'every tile’s left and width').toEqual(before)
    },
  )

  it.each([
    ['one volume', 1, 1, 575],
    ['the last row’s first tile of three', 6, 3, 1200],
  ] as const)(
    'keeps every tile in place as a panel runs past the end of a document that did not scroll, at 390 px (%s)',
    async (_, count, rows, height) => {
      onTestFinished(pointerRulesAs('mouse'))
      serve(count)
      const { tiles } = await drawn({ width: 390, height })
      const opened = lastRowFirst(tiles)
      const root = document.documentElement
      // The fixture tells: the grid draws its rows, a scrollbar here takes
      // width, and the document does not scroll.
      expect(opened.rows).toBe(rows)
      expect(rect(opened.tile).left).toBe(rect(tiles[0]).left)
      expect(scrollbarWidth()).toBeGreaterThan(0)
      expect(root.scrollHeight).toBeLessThanOrEqual(window.innerHeight)

      const before = placesOf(tiles)
      await open(opened.tile)
      expect(
        root.scrollHeight,
        'the document’s height with the panel open',
      ).toBeGreaterThan(window.innerHeight)
      expect(placesOf(tiles), 'every tile’s left and width').toEqual(before)
    },
  )

  it('scrolls the open panel’s foot into view', async () => {
    onTestFinished(pointerRulesAs('mouse'))
    serve(1)
    const { main, tiles } = await drawn({ width: 1280, height: 500 })
    const panel = await open(tiles[0])
    const download = panel.querySelector(
      'a[aria-label="download Kagami no Kuni no Harisugawa v01 — Raw"]',
    )
    const shown = () => rect(main).top + main.clientTop + main.clientHeight
    // The fixture tells: ↓ Download is drawn below what the page area shows.
    expect(rect(download).bottom).toBeGreaterThan(shown())

    main.scrollTop = main.scrollHeight
    expect(
      rect(download).bottom,
      '↓ Download’s bottom, the page area scrolled to its end',
    ).toBeLessThanOrEqual(shown())
  })

  it.each([1280, 390])(
    'reserves no gutter where the page draws no volume grid, at %i px',
    async (width) => {
      serve(1)
      const { main } = await drawn({ width, height: 500, layout: 'table' })
      // The fixture tells: the page draws its table and no grid.
      expect(main.querySelector('table.editions')).not.toBeNull()
      expect(main.querySelector('.volumes')).toBeNull()

      expect({
        main: getComputedStyle(main).scrollbarGutter,
        root: getComputedStyle(document.documentElement).scrollbarGutter,
      }).toEqual({ main: 'auto', root: 'auto' })
    },
  )

  // A press on the scroller itself closes nothing (`Series.tsx`), so ↓
  // Download can be reached by the page area's scrollbar.
  it('keeps the panel open through a real press on main’s scrollbar', async () => {
    onTestFinished(pointerRulesAs('mouse'))
    serve(1)
    const { main, tiles } = await drawn({ width: 1280, height: 500 })
    await open(tiles[0])
    const bar = main.offsetWidth - main.clientWidth
    const box = rect(main)
    const x = box.right - bar / 2
    const y = box.top + box.height * 0.9
    // The fixture tells: the page area scrolls, its scrollbar takes width,
    // and a press there lands on the page area itself.
    expect(main.scrollHeight).toBeGreaterThan(main.clientHeight)
    expect(bar).toBeGreaterThan(0)
    expect(document.elementFromPoint(x, y)).toBe(main)

    await real(async () => {
      await mouse.moveTo(x, y)
      await mouse.press()
      await mouse.release()
    })
    await land()
    expect(
      tiles[0].querySelector('.tile-panel'),
      'the panel after the press',
    ).not.toBeNull()
  })

  // Each page width, and a page of three rows in a window too short for
  // them, so that what scrolls the page scrolls with no panel open.
  const LONG = {
    1280: { count: 21, height: 600 },
    390: { count: 6, height: 600 },
  } as const

  /**
   * What scrolls the page at `width`: `main` at 1280, and below 900 px the
   * document, where `main` does not scroll (`shell.css`). `element` is it,
   * the root element for the document; `shown` is the span of the window it
   * shows its content in, and `end` scrolls it to its end.
   */
  function scroller(main: HTMLElement, width: number) {
    if (width >= 900) {
      return {
        element: main,
        scrolls: () => main.scrollHeight > main.clientHeight,
        shown: () => {
          const top = rect(main).top + main.clientTop
          return { top, bottom: top + main.clientHeight }
        },
        end: () => {
          main.scrollTop = main.scrollHeight
        },
      }
    }
    const root = document.documentElement
    return {
      element: root,
      scrolls: () => root.scrollHeight > root.clientHeight,
      shown: () => ({ top: 0, bottom: root.clientHeight }),
      end: () => {
        window.scrollTo(0, root.scrollHeight)
      },
    }
  }

  /** The page at `width`, drawn long, scrolled to its end. */
  async function scrolledToEnd(width: 1280 | 390) {
    serve(LONG[width].count)
    const { main, tiles } = await drawn({
      width,
      height: LONG[width].height,
    })
    const view = scroller(main, width)
    // The fixture tells: with no panel open, the page scrolls.
    expect(view.scrolls(), 'the page scrolls with no panel').toBe(true)
    view.end()
    return { main, tiles, view }
  }

  it.each([1280, 390] as const)(
    'leaves as much room under an open last-row panel, scrolled to the end, as under the last row with no panel, at %i px',
    async (width) => {
      onTestFinished(pointerRulesAs('mouse'))
      const { main, tiles, view } = await scrolledToEnd(width)
      const last = tiles[tiles.length - 1]
      const padding = parseFloat(getComputedStyle(main).paddingBlockEnd)
      const room0 = view.shown().bottom - rect(last).bottom

      const panel = await open(last)
      // The fixture tells: the panel's foot lies past the last tile's by
      // more than the room, so the page's end is the panel's.
      expect(rect(panel).bottom - rect(last).bottom).toBeGreaterThan(padding)
      view.end()
      const room1 = view.shown().bottom - rect(panel).bottom

      // Within 1 px: the scroll end is a whole pixel, so each room carries
      // its own rounding of the fractional end of what it measures.
      expect(room0, 'the room under the last row').toBeGreaterThan(0)
      expect(
        Math.abs(room0 - padding),
        `the room under the last row, ${room0}, against main's padding-block-end, ${padding}`,
      ).toBeLessThanOrEqual(1)
      expect(
        Math.abs(room1 - padding),
        `the room under the panel, ${room1}, against main's padding-block-end, ${padding}`,
      ).toBeLessThanOrEqual(1)
    },
  )

  // A press in the room lands on the scroller, `main` or the document's
  // root, and a press on either closes nothing (`Series.tsx`).
  it.each([1280, 390] as const)(
    'lets a press in the room under the panel reach the scroller under it, at %i px',
    async (width) => {
      onTestFinished(pointerRulesAs('mouse'))
      const { main, tiles, view } = await scrolledToEnd(width)
      const panel = await open(tiles[tiles.length - 1])
      view.end()
      const shown = view.shown()
      const room = shown.bottom - rect(panel).bottom
      const x = rect(panel).left + 8
      const y = rect(panel).bottom + room / 2
      // The fixture tells: the point lies inside what the page area shows.
      const area = rect(main)
      expect(x).toBeGreaterThan(area.left + main.clientLeft)
      expect(x).toBeLessThan(area.left + main.clientLeft + main.clientWidth)
      expect(y).toBeGreaterThan(shown.top)
      expect(y).toBeLessThan(shown.bottom)

      const hit = document.elementFromPoint(x, y)
      expect(
        { scroller: hit === view.element, panel: panel.contains(hit) },
        `what lies at the point, ${hit?.nodeName}`,
      ).toEqual({ scroller: true, panel: false })
    },
  )

  it('scrolls nothing sideways for a panel laid back from its tile’s right edge, with the room under it, at 390 px', async () => {
    onTestFinished(pointerRulesAs('mouse'))
    const { main, tiles, view } = await scrolledToEnd(390)
    const tile = tiles[tiles.length - 1]
    const panel = await open(tile)
    view.end()
    const root = document.documentElement
    // The fixture tells: the tile is in the last row's right-hand column,
    // and its panel is laid back from the tile's right edge.
    expect(rect(tile).top).toBe(rect(lastRowFirst(tiles).tile).top)
    expect(rect(tile).left).toBe(
      Math.max(...tiles.map((one) => rect(one).left)),
    )
    expect(rect(tile).left).toBeGreaterThan(rect(tiles[0]).left)
    expect(rect(panel).left).toBeLessThan(rect(tile).left)

    expect({
      document: root.scrollWidth,
      main: main.scrollWidth,
    }).toEqual({ document: root.clientWidth, main: main.clientWidth })
  })
})
