import type { CSSProperties } from 'react'
import { beforeEach, describe, expect, it, onTestFinished, vi } from 'vitest'
import { page } from 'vitest/browser'
import * as aboutApi from '../src/about'
import type { Me } from '../src/api'
// **Required**, the same reason `Config.browser.test.tsx` gives.
import '../src/index.css'
import * as library from '../src/library'
import { ABOUT } from './support/about'
import { settleUntil } from './support/async'
import { columnsOf, rowsOf } from './support/columns'
import { homePage } from './support/homePage'
import { mount } from './support/mount'
import { seriesPage } from './support/seriesPage'

/**
 * The cover lists' two layouts (spec O285 item 9), on the home page's grid
 * through the shell and on a series page's volumes: beside the sidebar every
 * cover is exactly the slider's size, the spare room spread into the gaps;
 * in the phone layout, narrow or short, the columns fill the row at the
 * least gap. jsdom lays nothing out. `W` is the list's content width and `g`
 * its least gap, both as drawn, and a row holds
 * `max(1, ⌊(W + g)/(size + g)⌋)`.
 */

// The build's label at the sidebar's foot asks for the build; answered here.
vi.mock('../src/about', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/about')>()),
  about: vi.fn(),
}))

vi.mock('../src/library', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/library')>()),
  status: vi.fn(),
  home: vi.fn(),
  grid: vi.fn(),
  oneSeries: vi.fn(),
  search: vi.fn(),
}))

const series = (id: number): library.SeriesCard => ({
  id,
  title: `Series ${id}`,
  works: 3,
  editions: 3,
  aligned: 0,
  added: id,
  unread: 3,
  cover: null,
})

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

/** A reader who may download and may not edit. */
const READER: Me = {
  user_id: 1,
  admin: false,
  can_download: true,
  can_stream: true,
  can_stamp: false,
}

beforeEach(() => {
  vi.clearAllMocks()
  vi.mocked(aboutApi.about).mockResolvedValue(ABOUT)
  vi.mocked(library.status).mockResolvedValue({
    libraries: [{ id: 1, name: 'Volumes', scanned: true }],
    restricted: false,
  })
  vi.mocked(library.home).mockResolvedValue({
    continuing: [],
    added: [],
    read: [],
  })
})

const HOME = 'the home grid'
const SERIES = 'a series page'
type Where = typeof HOME | typeof SERIES

/**
 * `where` at `width` px holding `count` covers, the cover size `size`: on
 * the home grid stored where the shell reads it, on a series page set as
 * the shell would. The window is `height` px tall, or the page's own default
 * where none is given. Its list, and the list's tiles once all are drawn.
 */
async function drawn(
  where: Where,
  width: number,
  size: number,
  count: number,
  height?: number,
) {
  let host: HTMLElement
  if (where === HOME) {
    vi.mocked(library.grid).mockResolvedValue({
      series: Array.from({ length: count }, (_, i) => series(i + 1)),
      unfiled: [],
    })
    // Panel off: these hold the page without a panel's gutter (O417).
    host = await homePage({
      width,
      height,
      coverSize: size,
      browse: { panel: 'off' },
    })
  } else {
    const works = Array.from({ length: count }, (_, i) => work(i + 1))
    vi.mocked(library.oneSeries).mockResolvedValue({
      series: {
        id: 7,
        title: 'Kagami no Kuni no Harisugawa',
        works: count,
        editions: count,
        aligned: 0,
        added: 1,
        unread: count,
        cover: null,
        titles: [],
      },
      works,
    })
    host = await seriesPage({
      width,
      height,
      panel: 'button',
      coverSize: size,
      who: READER,
    })
  }
  const selector = where === HOME ? 'main .grid' : 'main .volumes'
  const tiles = () => [...host.querySelectorAll(`${selector} > li`)]
  await settleUntil('the covers', () => tiles().length === count, 2000)
  await document.fonts.ready
  const list = host.querySelector(selector) as Element
  return { list, tiles: tiles() }
}

/**
 * `actual` where it lies within 0.1 px of `expected`, and itself where not,
 * so that a list of them compares whole and a failure shows the ones off.
 */
const near = (actual: number, expected: number) =>
  Math.abs(actual - expected) <= 0.1 ? expected : actual

const rect = (el: Element | null) => {
  if (el === null) throw new Error('nothing drawn')
  return el.getBoundingClientRect()
}

/**
 * Two full rows and one tile: `perRow` is how many a row holds here,
 * worked out by hand from the CSS, and only sizes the fixture. A series page
 * keeps its scrollbar's gutter (O283), which takes width in this browser, so
 * its rows can hold one fewer than the home grid's.
 */
const twoRowsAndOne = (perRow: number) => 2 * perRow + 1

describe('a cover list beside the sidebar (O285)', () => {
  // [where, window width, the size stored or set, the size drawn, perRow]
  it.each([
    [HOME, 1280, 96, 96, 9],
    [HOME, 1280, 144, 144, 6],
    [HOME, 1200, 240, 240, 3],
    [HOME, 900, 144, 144, 4],
    // Off the scale: the shell draws the nearest position's size.
    [HOME, 1024, 151, 152, 4],
    [SERIES, 1280, 96, 96, 9],
    [SERIES, 1280, 144, 144, 6],
    [SERIES, 1280, 240, 240, 3],
    [SERIES, 900, 144, 144, 3],
  ] as const)(
    'draws every cover on %s at %i px exactly the size (%i, drawn %i), the spare room in the gaps',
    async (where, width, size, drawnSize, perRow) => {
      const { list, tiles } = await drawn(
        where,
        width,
        size,
        twoRowsAndOne(perRow),
      )
      const { left, right, width: W, gap, n } = columnsOf(list, drawnSize)
      const rows = rowsOf(tiles)
      const first = rows[0]
      const last = rows[rows.length - 1]
      // The fixture tells: two full rows and a short last one, and room
      // left over at the least gap, which an exact fit would not show
      // spread.
      expect(rows.map((row) => row.length)).toEqual([perRow, perRow, 1])
      expect(W - n * drawnSize - (n - 1) * gap).toBeGreaterThan(1)

      const gaps = first.slice(1).map((box, i) => box.left - first[i].right)
      const widths = tiles.map((tile) => rect(tile.querySelector('.cover-box')))
      expect({
        covers: widths.map((box) => near(box.width, drawnSize)),
        perRow: first.length,
        gaps: gaps.map((one) => near(one, gaps[0])),
        nearest: Math.min(...gaps) >= gap - 0.1,
        firstLeft: near(first[0].left, left),
        lastRight: near(first[first.length - 1].right, right),
        lastRow: last.map((box, i) => near(box.left, first[i].left)),
      }).toEqual({
        covers: widths.map(() => drawnSize),
        perRow: n,
        gaps: gaps.map(() => gaps[0]),
        nearest: true,
        firstLeft: left,
        lastRight: right,
        lastRow: last.map((_, i) => first[i].left),
      })
    },
  )

  it.each([HOME, SERIES] as const)(
    'keeps a row shorter than the columns on the first columns, on %s at 1280 px',
    async (where) => {
      const { list, tiles } = await drawn(where, 1280, 144, 2)
      const { left, width: W, n } = columnsOf(list, 144)
      // The fixture tells: one row, with columns to spare.
      expect(rowsOf(tiles).map((row) => row.length)).toEqual([2])
      expect(n).toBeGreaterThan(2)

      // Where a full row would lay them: the spare room shared among all
      // of the row's gaps, not the two covers'.
      const step = 144 + (W - n * 144) / (n - 1)
      const covers = tiles.map((tile) => rect(tile.querySelector('.cover-box')))
      expect({
        lefts: covers.map((box, i) => near(box.left, left + i * step)),
        widths: covers.map((box) => near(box.width, 144)),
      }).toEqual({
        lefts: covers.map((_, i) => left + i * step),
        widths: covers.map(() => 144),
      })
    },
  )
})

describe('a cover list in the phone layout (O285)', () => {
  // [where, window width, window height, the size, perRow]
  it.each([
    [HOME, 390, 844, 104, 3],
    [HOME, 390, 844, 144, 2],
    [HOME, 390, 844, 240, 1],
    [HOME, 899, 844, 144, 5],
    [HOME, 956, 440, 144, 5],
    [SERIES, 390, 800, 104, 3],
    [SERIES, 390, 800, 144, 2],
    [SERIES, 390, 800, 240, 1],
    [SERIES, 899, 800, 144, 5],
    [SERIES, 956, 440, 144, 5],
  ] as const)(
    'fills the row on %s at %ix%i px with columns no narrower than %i px, at the least gap',
    async (where, width, height, size, perRow) => {
      const { list, tiles } = await drawn(
        where,
        width,
        size,
        twoRowsAndOne(perRow),
        height,
      )
      const { left, right, width: W, gap, n } = columnsOf(list, size)
      const rows = rowsOf(tiles)
      const first = rows[0]
      const last = rows[rows.length - 1]
      // The fixture tells: two full rows and a short last one.
      expect(rows.map((row) => row.length)).toEqual([perRow, perRow, 1])

      const column = (W - (n - 1) * gap) / n
      const gaps = first.slice(1).map((box, i) => box.left - first[i].right)
      const covers = tiles.map((tile) => rect(tile.querySelector('.cover-box')))
      expect({
        perRow: first.length,
        widths: covers.map((box) => near(box.width, column)),
        narrowest: Math.min(...covers.map((box) => box.width)) >= size,
        gaps: gaps.map((one) => near(one, gap)),
        firstLeft: near(first[0].left, left),
        lastRight: near(first[first.length - 1].right, right),
        lastRow: last.map((box, i) => near(box.left, first[i].left)),
      }).toEqual({
        perRow: n,
        widths: covers.map(() => column),
        narrowest: true,
        gaps: gaps.map(() => gap),
        firstLeft: left,
        lastRight: right,
        lastRow: last.map((_, i) => first[i].left),
      })
    },
  )

  it.each([HOME, SERIES] as const)(
    'never lays a cover wider than the list on %s, at a 260 px window and 240 px covers',
    async (where) => {
      const { list, tiles } = await drawn(where, 260, 240, twoRowsAndOne(1))
      const style = getComputedStyle(list)
      const W =
        list.clientWidth -
        parseFloat(style.paddingLeft) -
        parseFloat(style.paddingRight)
      // The fixture tells: the list is narrower than one cover, so the
      // slider's size cannot hold. `clientWidth` is whole pixels, hence the
      // half pixel below.
      expect(W).toBeLessThan(240)
      const within = (actual: number) =>
        Math.abs(actual - W) <= 0.5 ? W : actual
      const covers = tiles.map((tile) => rect(tile.querySelector('.cover-box')))
      expect({
        rows: rowsOf(tiles).map((row) => row.length),
        overflows: list.scrollWidth > list.clientWidth,
        tiles: tiles.map((tile) => within(rect(tile).width)),
        covers: covers.map((box) => within(box.width)),
      }).toEqual({
        rows: [1, 1, 1],
        overflows: false,
        tiles: tiles.map(() => W),
        covers: covers.map(() => W),
      })
    },
  )
})

describe('the cover size’s tokens (O285)', () => {
  it('derives the least gap from the size, and which layout is in force', async () => {
    const was = [window.innerWidth, window.innerHeight] as const
    onTestFinished(() => page.viewport(...was))
    const sizes = [96, 120, 144, 240]
    const host = mount(
      sizes.map((size) => (
        <div
          key={size}
          className="library"
          style={{ '--cover-n': size } as CSSProperties}
        />
      )),
    )
    const tokens = () =>
      [...host.querySelectorAll('.library')].map((el) => {
        const style = getComputedStyle(el)
        return {
          min: style.getPropertyValue('--tile-min'),
          gap: style.getPropertyValue('--tile-gap'),
          fill: style.getPropertyValue('--tile-fill').trim(),
        }
      })

    await page.viewport(900, 800)
    expect(tokens()).toEqual([
      { min: '96px', gap: '9.6px', fill: '0' },
      { min: '120px', gap: '12.8px', fill: '0' },
      { min: '144px', gap: '16px', fill: '0' },
      { min: '240px', gap: '16px', fill: '0' },
    ])
    await page.viewport(899, 800)
    expect(tokens().map((one) => one.fill)).toEqual(['1', '1', '1', '1'])
  })
})
