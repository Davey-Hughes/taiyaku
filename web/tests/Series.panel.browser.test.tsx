import { act } from 'react'
import { beforeEach, describe, expect, it, onTestFinished, vi } from 'vitest'
import { page, userEvent } from 'vitest/browser'
// **Required**, the same reason `Config.browser.test.tsx` gives.
import '../src/index.css'
import type { Me } from '../src/api'
import * as derive from '../src/libraries'
import * as library from '../src/library'
import * as metadata from '../src/metadata'
import { PHONE_LAYOUT, SIDEBAR_LAYOUT } from '../src/phoneLayout'
import * as stamp from '../src/stamp'
import { deferred, settleUntil } from './support/async'
import { columnsOf } from './support/columns'
import { emptyDerive } from './support/derive'
import { holdOf } from './support/held'
import { lineCount, lineOf, oneLineIn, pastTheEdgeOf } from './support/lines'
import {
  afterMotion,
  dragged,
  eachFrame,
  reducedMotion,
  scriptedIn,
} from './support/motion'
import { areaOf, overlapOf, pointerRulesAs } from './support/pointer'
import { seriesPage } from './support/seriesPage'

vi.mock('../src/library', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/library')>()),
  oneSeries: vi.fn(),
  work: vi.fn(),
}))
vi.mock('../src/metadata', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/metadata')>()),
  patchWork: vi.fn(),
}))
vi.mock('../src/stamp', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/stamp')>()),
  stampPlan: vi.fn(),
  performStamp: vi.fn(),
}))
vi.mock('../src/libraries', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/libraries')>()),
  deriveStatus: vi.fn(),
}))

const work = (
  id: number,
  over: Partial<library.WorkCard> = {},
): library.WorkCard => ({
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
  ...over,
})

/**
 * Twenty volumes: at the default size four rows at 1280 px, six to a row,
 * and ten rows of two at 390 px; at 104 px three rows at 1280 px, eight to
 * a row.
 */
const volumes = (over: Record<number, Partial<library.WorkCard>> = {}) =>
  Array.from({ length: 20 }, (_, n) => work(n + 1, over[n]))

/** The series `oneSeries` answers with `works`. */
function serve(works: library.WorkCard[]) {
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

/** The same, and the panel draws Write ComicInfo.xml. */
const STAMPER: Me = { ...ADMIN, can_stamp: true }

/** Its last name has no `/`, `-` or space to break at. */
const LONG =
  '/mnt/user/manga/Kagami_no_Kuni_no_Harisugawa_Complete_Digital_Edition_Volume_01.cbz'

/** A content_key as `identity::content_key` writes it: sha256 in hex. */
const KEY = '9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08'

beforeEach(() => {
  vi.clearAllMocks()
  serve(volumes())
  vi.mocked(metadata.patchWork).mockImplementation(async (_, fields) => ({
    taiyaku_library: 1,
    updated: '2026-11-14T00:00:00Z',
    title: fields.title,
    persisted: true,
  }))
  vi.mocked(stamp.stampPlan).mockResolvedValue({
    archives: [{ member_id: 7, edition_key: 'e1', path: LONG, bytes: 1024 }],
    total_bytes: 1024,
    skipped_formats: [],
  })
  // Moved and rewritten, so the key and both document lines are drawn.
  vi.mocked(stamp.performStamp).mockResolvedValue({
    rewritten: 1,
    content_key: KEY,
    document_moved: true,
    document_rewritten: true,
    sidecar_rewritten: false,
  })
  vi.mocked(derive.deriveStatus).mockResolvedValue({
    derive_running: false,
    job_held: false,
  })
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

/**
 * The left and right of `area`'s content box, its right to the whole pixel
 * `clientWidth` is rounded to: only for a page area a whole number of
 * pixels wide, as every case that reads it draws.
 */
const contentOf = (area: Element) => {
  const box = area.getBoundingClientRect()
  const style = getComputedStyle(area)
  const left = box.left + area.clientLeft
  return {
    left: left + parseFloat(style.paddingLeft),
    right: left + area.clientWidth - parseFloat(style.paddingRight),
  }
}

const iOf = (tile: Element) => {
  const i = tile.querySelector<HTMLButtonElement>(
    ':scope > .tile-body > .panel-open',
  )
  if (i === null) throw new Error('no i on this tile')
  return i
}

/** The field's content height, and the height of one of its lines. */
const heightOf = (field: HTMLTextAreaElement) => {
  const style = getComputedStyle(field)
  return {
    line: parseFloat(style.lineHeight),
    inner:
      field.clientHeight -
      parseFloat(style.paddingTop) -
      parseFloat(style.paddingBottom),
  }
}

/**
 * The page drawn `width` px wide for `who`, its volumes in, and the face
 * the form is drawn in loaded for `text`: the Title field is fitted as the
 * panel mounts, and a face that landed later would change its lines and
 * not its height.
 */
async function drawn({
  width,
  coverSize,
  who = ADMIN,
  text = 'Kagami no Kuni no Harisugawa v01',
}: {
  width: number
  coverSize?: number
  who?: Me
  text?: string
}) {
  const host = await seriesPage({ width, coverSize, panel: 'button', who })
  const all = () => [...host.querySelectorAll<HTMLElement>('.volumes > li')]
  await settleUntil('the volumes', () => all().length > 0, 1000)
  const face = getComputedStyle(host.querySelector('.volumes') as Element)
  await document.fonts.load(
    `${face.fontStyle} ${face.fontWeight} ${face.fontSize} ${face.fontFamily}`,
    text,
  )
  await document.fonts.ready
  const main = host.querySelector('.library > main')
  if (main === null) throw new Error('no page area')
  return { host, main, tiles: all() }
}

/**
 * Opens `tile`'s panel as a mouse does, the pointer onto the tile and then
 * a click on its `i`, and waits for its alignment detail to be read.
 */
async function open(tile: HTMLElement) {
  await real(() =>
    userEvent.hover(
      tile.querySelector(
        ':scope > .tile-body > :is(a, .tile-unreadable)',
      ) as Element,
    ),
  )
  await real(() => userEvent.click(iOf(tile)))
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

describe('a volume’s panel over the rows below (O283)', () => {
  it('opens over the rows below, 4 px under its tile, and moves no tile', async () => {
    onTestFinished(pointerRulesAs('mouse'))
    const { tiles } = await drawn({ width: 1280 })
    const below = tiles[7]
    // The fixture tells: a row lies below the first.
    expect(rect(below).top).toBeGreaterThan(rect(tiles[0]).bottom)

    const boxes = () =>
      tiles.flatMap((tile) =>
        [
          tile,
          tile.querySelector(':scope > .tile-body > a'),
          tile.querySelector('.title'),
        ].map((el) => rect(el).toJSON()),
      )
    const before = boxes()
    const where = areaOf(rect(below))
    const panel = await open(tiles[0])

    // The fixture tells: the panel lies where a tile of that row is drawn.
    expect(
      overlapOf(areaOf(rect(panel)), where),
      'the panel over the row below',
    ).toBeGreaterThan(0)
    expect(boxes(), 'every tile, link and title').toEqual(before)
    expect(rect(panel).top).toBeCloseTo(rect(tiles[0]).bottom + 4, 0)
  })

  it.each([
    [1280, 144],
    [1280, 104],
    [899, 144],
    [390, 104],
  ] as const)(
    'is two tiles and the laid gap wide at %i px (%i px covers)',
    async (width, size) => {
      onTestFinished(pointerRulesAs('mouse'))
      const { main, tiles } = await drawn({ width, coverSize: size })
      const columns = columnsOf(main.querySelector('.volumes') as Element, size)
      const tile = rect(tiles[0])
      const gap = rect(tiles[1]).left - tile.right
      // The fixture tells: beside the sidebar the tile is the size and the
      // gap the spread one, wider than the least; in the phone layout the
      // gap is the least and the tile the filled column, wider than the
      // size. Either way two tiles and the gap fit right of the tile's left
      // edge.
      const spread = (columns.width - columns.n * size) / (columns.n - 1)
      const filled = (columns.width - (columns.n - 1) * columns.gap) / columns.n
      expect(
        matchMedia(SIDEBAR_LAYOUT).matches
          ? { tile: tile.width, gap, wider: gap > columns.gap + 1 }
          : { tile: tile.width, gap, wider: tile.width > size + 1 },
      ).toEqual(
        matchMedia(SIDEBAR_LAYOUT).matches
          ? { tile: size, gap: expect.closeTo(spread, 1), wider: true }
          : {
              tile: expect.closeTo(filled, 1),
              gap: expect.closeTo(columns.gap, 1),
              wider: true,
            },
      )
      expect(tile.left + 2 * tile.width + gap).toBeLessThan(
        contentOf(main).right,
      )

      const box = rect(await open(tiles[0]))
      expect({ left: box.left, width: box.width }).toEqual({
        left: expect.closeTo(tile.left, 0),
        width: expect.closeTo(2 * tile.width + gap, 0),
      })
    },
  )

  it('is the area wide with one column (390 px, 240 px covers)', async () => {
    onTestFinished(pointerRulesAs('mouse'))
    const { main, tiles } = await drawn({ width: 390, coverSize: 240 })
    const list = main.querySelector('.volumes') as Element
    const columns = columnsOf(list, 240)
    const tile = rect(tiles[0])
    // The fixture tells: one column, the tile the list's content width, so
    // the grid's resolved columns hold no gap to read.
    expect({
      n: columns.n,
      tracks: getComputedStyle(list).gridTemplateColumns.split(' ').length,
      tile: tile.width,
      below: rect(tiles[1]).top > tile.bottom,
    }).toEqual({
      n: 1,
      tracks: 1,
      tile: expect.closeTo(columns.width, 0),
      below: true,
    })

    const box = rect(await open(tiles[0]))
    expect({ left: box.left, width: box.width }).toEqual({
      left: expect.closeTo(columns.left, 0),
      width: expect.closeTo(columns.width, 0),
    })
  })

  it('lies back from its tile’s right edge in the row’s last column at 1280 px', async () => {
    onTestFinished(pointerRulesAs('mouse'))
    const { main, tiles } = await drawn({ width: 1280 })
    const { gap, n } = columnsOf(main.querySelector('.volumes') as Element, 144)
    const last = tiles[n - 1]
    const tile = rect(last)
    // The fixture tells: the tile ends the first row, its right edge is the
    // area's, and two tiles and the least gap from its left edge run past
    // the area.
    expect(rect(tiles[n]).top).toBeGreaterThan(tile.bottom)
    expect(tile.right).toBeCloseTo(contentOf(main).right, 0)
    expect(tile.left + 2 * tile.width + gap).toBeGreaterThan(
      contentOf(main).right + 0.5,
    )

    const panel = await open(last)
    expect(rect(panel).right).toBeCloseTo(tile.right, 0)
    // Over the column before, not held to the tile's own width.
    expect(rect(panel).left).toBeLessThan(tile.left - tile.width)
    expect(main.scrollWidth, 'the page area scrolls sideways by').toBe(
      main.clientWidth,
    )
  })

  it.each([
    [1280, 8],
    [390, 3],
  ] as const)(
    'lies from its tile’s left edge in a row’s second-to-last column at %i px, %i to a row (compact)',
    async (width, perRow) => {
      onTestFinished(pointerRulesAs('mouse'))
      const { tiles } = await drawn({ width, coverSize: 104 })
      const tile = rect(tiles[perRow - 2])
      const last = rect(tiles[perRow - 1])
      // The fixture tells: the first row holds `perRow` tiles, so the next
      // tile ends the row, and the rule lays this one's panel rightward,
      // where laying it back would cover the column to its left.
      expect({
        row: tiles.slice(0, perRow).map((t) => rect(t).top),
        below: rect(tiles[perRow]).top > tile.bottom,
      }).toEqual({ row: Array(perRow).fill(tile.top), below: true })

      const box = rect(await open(tiles[perRow - 2]))
      // Against the layout's own edges: the tile's left, and the row's end.
      expect({
        left: box.left,
        right: box.right,
        pageInWindow: document.documentElement.scrollWidth <= window.innerWidth,
      }).toEqual({
        left: expect.closeTo(tile.left, 0),
        right: expect.closeTo(last.right, 0),
        pageInWindow: true,
      })
    },
  )

  it.each([
    ['left', 0],
    ['right', 1],
  ] as const)(
    'spans the page area at 390 px from the %s column',
    async (column, index) => {
      onTestFinished(pointerRulesAs('mouse'))
      const { main, tiles } = await drawn({ width: 390 })
      const area = contentOf(main)
      // The fixture tells: the tile is in that column of two.
      const tile = rect(tiles[index])
      if (column === 'left') expect(tile.left).toBeCloseTo(area.left, 0)
      else expect(tile.right).toBeCloseTo(area.right, 0)
      expect(rect(tiles[2]).top).toBeGreaterThan(tile.bottom)

      const box = rect(await open(tiles[index]))
      // One comparison, so a failure shows every edge and the page together.
      expect({
        left: box.left,
        right: box.right,
        pageInWindow: document.documentElement.scrollWidth <= window.innerWidth,
      }).toEqual({
        left: expect.closeTo(area.left, 0),
        right: expect.closeTo(area.right, 0),
        pageInWindow: true,
      })
    },
  )

  it('keeps a five-line field and its own width for a title with no space, longer than five lines', async () => {
    onTestFinished(pointerRulesAs('mouse'))
    const title = 'Kagami_no_Kuni_no_Harisugawa_Complete_Digital_Edition_'
      .repeat(6)
      .slice(0, 300)
    serve(volumes({ 0: { title } }))
    const { tiles } = await drawn({ width: 1280, text: title })
    const panel = await open(tiles[0])
    const field = panel.querySelector<HTMLTextAreaElement>("[name='title']")
    if (field === null) throw new Error('no Title field')

    // The fixture tells: 300 word characters, no space, past five lines.
    expect(title).toMatch(/^\w{300}$/)
    expect(lineCount(field, title), 'the title’s lines').toBeGreaterThan(5)

    const tile = rect(tiles[0])
    const gap = rect(tiles[1]).left - tile.right
    expect(rect(panel).width).toBeCloseTo(2 * tile.width + gap, 0)
    expect(pastTheEdgeOf(panel)).toEqual([])
    const { line, inner } = heightOf(field)
    expect(
      { lines: inner / line, scrolls: field.scrollHeight > field.clientHeight },
      'the field’s height, and whether it scrolls inside',
    ).toEqual({ lines: 5, scrolls: true })
  })

  it('is laid again when the window’s width changes', async () => {
    onTestFinished(pointerRulesAs('mouse'))
    const { main, tiles } = await drawn({ width: 1280 })
    const list = main.querySelector('.volumes') as Element
    const last = tiles[5]
    const panel = await open(last)
    const was = {
      width: rect(last).width,
      gap: rect(tiles[1]).left - rect(tiles[0]).right,
    }
    // The fixture tells: six columns at 1280 px, so the tile ends the first
    // row and its panel lies back from its right edge.
    expect(columnsOf(list, 144).n).toBe(6)
    expect(rect(last).right).toBeCloseTo(contentOf(main).right, 0)
    expect(rect(panel).left).toBeLessThan(rect(last).left - 1)

    await page.viewport(1100, 800)
    await expect.poll(() => window.innerWidth).toBe(1100)
    await afterMotion()
    // The fixture tells: five columns, so the tile opens the second row as
    // wide as it was, and the gap the columns are laid apart by moved; two
    // tiles and that gap fit right of its left edge.
    const tile = rect(last)
    const gap = rect(tiles[6]).left - tile.right
    const area = contentOf(main)
    expect({
      n: columnsOf(list, 144).n,
      left: tile.left,
      width: tile.width,
      gapMoved: Math.abs(gap - was.gap) > 1,
    }).toEqual({
      n: 5,
      left: expect.closeTo(area.left, 0),
      width: was.width,
      gapMoved: true,
    })
    expect(tile.left + 2 * tile.width + gap).toBeLessThan(area.right)

    await expect
      .poll(() => ({ left: rect(panel).left, width: rect(panel).width }), {
        message: 'the panel’s left edge and width',
      })
      .toEqual({
        left: expect.closeTo(tile.left, 0),
        width: expect.closeTo(2 * tile.width + gap, 0),
      })
    const box = rect(panel)
    expect(box.left).toBeGreaterThanOrEqual(area.left - 0.5)
    expect(box.right).toBeLessThanOrEqual(area.right + 0.5)
  })

  it('is laid again across 900 px', async () => {
    onTestFinished(pointerRulesAs('mouse'))
    const { main, tiles } = await drawn({ width: 900 })
    const list = main.querySelector('.volumes') as Element
    const panel = await open(tiles[0])
    /** The tile's width, and two tiles and the laid gap, as drawn now. */
    const laid = () => {
      const tile = rect(tiles[0])
      const gap = rect(tiles[1]).left - tile.right
      return { tile: tile.width, wide: 2 * tile.width + gap }
    }
    // The fixture tells: opened at 900 px, two tiles of the size and the
    // spread gap wide.
    expect(laid().tile).toBe(144)
    expect(rect(panel).width).toBeCloseTo(laid().wide, 0)

    for (const width of [899, 900]) {
      await page.viewport(width, 800)
      await expect.poll(() => window.innerWidth).toBe(width)
      const columns = columnsOf(list, 144)
      const { tile, wide } = laid()
      // The fixture tells: below 900 px the tile is the filled column, wider
      // than the size; at 900 px it is the size again.
      expect(tile, `the tile at ${width} px`).toBeCloseTo(
        matchMedia(PHONE_LAYOUT).matches
          ? (columns.width - (columns.n - 1) * columns.gap) / columns.n
          : 144,
        1,
      )
      if (matchMedia(PHONE_LAYOUT).matches)
        expect(tile).toBeGreaterThan(144 + 1)

      await expect
        .poll(() => rect(panel).width, {
          message: `the panel’s width at ${width} px`,
        })
        .toBeCloseTo(wide, 0)
      const area = contentOf(main)
      const box = rect(panel)
      expect({
        left: box.left >= area.left - 0.5,
        right: box.right <= area.right + 0.5,
      }).toEqual({ left: true, right: true })
    }
  })

  it('is laid again, its field fitted again, when the page area’s width changes and the window’s does not', async () => {
    onTestFinished(pointerRulesAs('mouse'))
    const title =
      'Kagami no Kuni no Harisugawa: The Complete Digital Edition, Volume 01 (Collector’s Box Set, Deluxe Limited Ed.)'
    serve(volumes({ 0: { title } }))
    const { host, main, tiles } = await drawn({ width: 1280, text: title })
    const list = main.querySelector('.volumes') as Element
    const panel = await open(tiles[0])
    const field = panel.querySelector<HTMLTextAreaElement>("[name='title']")
    if (field === null) throw new Error('no Title field')
    const was = {
      panel: rect(panel).width,
      line: lineOf(field),
      tile: rect(tiles[0]).width,
    }

    // A wider sidebar column takes the width from the page area alone.
    const shell = host.querySelector<HTMLElement>('.library')
    if (shell === null) throw new Error('no .library mounted')
    shell.style.gridTemplateColumns = '30rem 1fr'
    const tile = rect(tiles[0])
    const wide = 2 * tile.width + (rect(tiles[1]).left - tile.right)

    // The fixture tells: the window and the tile's width are as they were,
    // the columns are fewer and laid further apart, and the title takes
    // another count of lines at the field's new width.
    expect({
      window: window.innerWidth,
      tile: tile.width,
      n: columnsOf(list, 144).n,
    }).toEqual({ window: 1280, tile: was.tile, n: 4 })
    expect(Math.abs(wide - was.panel)).toBeGreaterThan(1)
    const lines = lineCount(field, title, was.line + wide - was.panel)
    expect(lines, 'its lines after, against before').not.toBe(
      lineCount(field, title, was.line),
    )

    await expect
      .poll(() => rect(panel).width, { message: 'the panel’s width' })
      .toBeCloseTo(wide, 0)
    await expect
      .poll(() => heightOf(field).inner, { message: 'the field’s height' })
      .toBe(lines * heightOf(field).line)
  })

  it('draws over the covers and the marks of the row below', async () => {
    onTestFinished(pointerRulesAs('touch laptop'))
    serve(volumes({ 7: { aligned: true } }))
    const { tiles } = await drawn({ width: 1280, who: STAMPER })
    const lower = tiles[7]
    const panel = await open(tiles[0])
    const box = rect(panel)
    /** The middle of each mark: where it is drawn, and what is drawn there. */
    const marks = Object.entries({
      '⇄': lower.querySelector('.cover-box .aligned'),
      i: iOf(lower),
    }).map(([name, mark]) => {
      const at = rect(mark)
      const x = at.left + at.width / 2
      const y = at.top + at.height / 2
      return {
        name,
        within: x > box.left && x < box.right && y > box.top && y < box.bottom,
        hit: document.elementFromPoint(x, y)?.closest('.tile-panel') === panel,
      }
    })
    // The fixture tells: each mark's middle is inside the panel's box.
    expect(marks.map(({ name, within }) => ({ name, within }))).toEqual([
      { name: '⇄', within: true },
      { name: 'i', within: true },
    ])
    expect(
      marks.map(({ name, hit }) => ({ name, hit })),
      'a press on each lands in the panel',
    ).toEqual([
      { name: '⇄', hit: true },
      { name: 'i', hit: true },
    ])
  })

  it('is drawn as View’s panel is', async () => {
    onTestFinished(pointerRulesAs('mouse'))
    const { host, tiles } = await drawn({ width: 1280 })
    const panel = await open(tiles[0])
    const view = host.querySelector('.view-panel')
    if (view === null) throw new Error('no View panel')
    const look = (el: Element) => {
      const style = getComputedStyle(el)
      const sides = ['Top', 'Right', 'Bottom', 'Left'] as const
      return {
        padding: style.padding,
        border: sides.map((side) => [
          style.getPropertyValue(`border-${side.toLowerCase()}-width`),
          style.getPropertyValue(`border-${side.toLowerCase()}-style`),
          style.getPropertyValue(`border-${side.toLowerCase()}-color`),
        ]),
        backgroundColor: style.backgroundColor,
        boxShadow: style.boxShadow,
      }
    }
    // The fixture tells: View's panel is drawn, with a border.
    expect(look(view).border[0]).toEqual(['1px', 'solid', expect.any(String)])
    expect(look(panel)).toEqual(look(view))
  })

  it('holds the Title field, the stamp section, the alignment detail and ↓ Download, top to bottom, 12 px apart, the alignment detail bare', async () => {
    onTestFinished(pointerRulesAs('mouse'))
    const { tiles } = await drawn({ width: 1280, who: STAMPER })
    const panel = await open(tiles[0])
    const parts = [...panel.children]
    expect(parts.map((el) => el.className)).toEqual([
      'metadata-editor',
      'stamp-comicinfo',
      'edition-group',
      'get',
    ])
    expect(
      parts.slice(1).map((el, n) => rect(el).top - rect(parts[n]).bottom),
      'the space over each part',
    ).toEqual([
      expect.closeTo(12, 0),
      expect.closeTo(12, 0),
      expect.closeTo(12, 0),
    ])

    // The alignment detail drawn bare: it spans the panel's line, and its
    // text starts at its top-left corner, no border, padding or margin
    // between them.
    const detail = rect(parts[2])
    const text = rect(parts[2].firstElementChild)
    // From the panel's own box, not `clientWidth`, which is rounded.
    const outer = rect(panel)
    const style = getComputedStyle(panel)
    const line = {
      left:
        outer.left +
        parseFloat(style.borderLeftWidth) +
        parseFloat(style.paddingLeft),
      right:
        outer.right -
        parseFloat(style.borderRightWidth) -
        parseFloat(style.paddingRight),
    }
    expect({
      left: detail.left - line.left,
      right: line.right - detail.right,
      textLeft: text.left - detail.left,
      textTop: text.top - detail.top,
    }).toEqual({
      left: expect.closeTo(0, 0),
      right: expect.closeTo(0, 0),
      textLeft: expect.closeTo(0, 0),
      // An administrator's alignment detail sits under the editions
      // section's 1 px rule and the panel's 12 px gap.
      textTop: expect.closeTo(13, 0),
    })
    // The stamp section's heading starts at its part's top.
    expect(rect(parts[1].querySelector(':scope > h3')).top).toBeCloseTo(
      rect(parts[1]).top,
      0,
    )
  })
})

/** What a glide moves in a volume's tile: its body, which holds the cover. */
const bodyOf = (tile: Element) => {
  const body = tile.querySelector(':scope > .tile-body')
  if (body === null) throw new Error('no body on this tile')
  return body
}

/**
 * Through a glide (O304), the open panel stands at its tile's laid box while
 * the covers move to theirs. The drive is one step of a drag from six
 * columns to five.
 */
describe('a volume’s panel through a glide (O304)', () => {
  it('stays over the covers mid-glide', async () => {
    onTestFinished(pointerRulesAs('mouse'))
    const { main, tiles } = await drawn({ width: 1280 })
    const list = main.querySelector('.volumes') as HTMLElement
    // The third: a tile further along its row moves further as the gap
    // between the columns changes, and one that moved less than a snap's
    // threshold would not glide.
    const which = 2
    const tile = tiles[which]
    const panel = await open(tile)
    const seen: {
      glides: boolean
      under: number[]
      hit: string
    }[] = []
    eachFrame(() => {
      if (!list.hasAttribute('data-reseating')) return
      const box = rect(panel)
      const x = box.left + box.width / 2
      const y = box.top + box.height / 2
      const at = document.elementFromPoint(x, y)
      const hitTile = at?.closest('.volumes > li')
      seen.push({
        glides: scriptedIn(tile) > 0,
        // Each later cover drawn under the point while it glides.
        under: tiles.flatMap((later, i) => {
          if (i <= which || scriptedIn(later) === 0) return []
          const drawnAt = rect(bodyOf(later))
          const over =
            x > drawnAt.left &&
            x < drawnAt.right &&
            y > drawnAt.top &&
            y < drawnAt.bottom
          return over ? [i] : []
        }),
        hit:
          at?.closest('.tile-panel') === panel
            ? 'panel'
            : `tile ${hitTile ? tiles.indexOf(hitTile as HTMLElement) : '-'}`,
      })
    })
    await act(async () => {
      await dragged([{ width: 1100 }])
    })
    await afterMotion()

    // The fixture tells: the page went from six columns to five, the tile
    // holding the panel glided, and a later cover, gliding, was drawn under
    // the panel's centre in a frame.
    const covered = seen.filter(({ under }) => under.length > 0)
    expect({
      n: columnsOf(list, 144).n,
      tileGlides: seen.some(({ glides }) => glides),
      covered: covered.length > 0,
    }).toEqual({ n: 5, tileGlides: true, covered: true })
    expect(
      covered.map(({ hit }) => hit),
      'what is hit at the panel’s centre in each frame a gliding cover is drawn under it',
    ).toEqual(covered.map(() => 'panel'))
  })

  it('is laid where the reduced-motion drive lays it after a glide', async () => {
    onTestFinished(pointerRulesAs('mouse'))
    onTestFinished(async () => {
      await reducedMotion(false)
    })
    /**
     * The sixth tile's panel opened at 1280 px, where the tile ends the
     * first row, and its left edge and width once a step to 1100 px has
     * come to rest, where it opens the second.
     */
    const laidAfter = async () => {
      const { host, main, tiles } = await drawn({ width: 1280 })
      const list = main.querySelector('.volumes') as HTMLElement
      const tile = tiles[5]
      const panel = await open(tile)
      const before = {
        n: columnsOf(list, 144).n,
        tile: rect(tile),
        area: contentOf(main),
      }
      let glided = false
      eachFrame(() => {
        glided ||= scriptedIn(tile) > 0
      })
      await act(async () => {
        await dragged([{ width: 1100 }])
      })
      await afterMotion()
      const box = rect(panel)
      const laid = {
        before,
        n: columnsOf(list, 144).n,
        tile: rect(tile),
        area: contentOf(main),
        glided,
        left: box.left,
        width: box.width,
      }
      // Out of the way of the next drive's page, which would lie under it.
      host.hidden = true
      return laid
    }
    const glide = await laidAfter()
    await reducedMotion(true)
    const still = await laidAfter()
    await reducedMotion(false)

    // The fixture tells: from the last of six columns to the first of five,
    // so the panel lies back from the tile's right edge before and from its
    // left edge after; and the tile glided in the first drive alone.
    for (const drive of [glide, still]) {
      expect({
        before: drive.before.n,
        endsRow:
          Math.abs(drive.before.tile.right - drive.before.area.right) < 0.5,
        after: drive.n,
        opensRow: Math.abs(drive.tile.left - drive.area.left) < 0.5,
      }).toEqual({ before: 6, endsRow: true, after: 5, opensRow: true })
    }
    expect({ glide: glide.glided, still: still.glided }).toEqual({
      glide: true,
      still: false,
    })
    expect(
      { left: glide.left, width: glide.width },
      'the panel’s left edge and width after the glide',
    ).toEqual({
      left: expect.closeTo(still.left, 0),
      width: expect.closeTo(still.width, 0),
    })
  })
})

/**
 * Its closes, each waiting for the Title field's save (O283). A real press
 * outside reaches the document as `pointerdown` before the field's blur,
 * so the close asks for the save before the field's own leave does.
 */
describe('closing a volume’s panel (O283)', () => {
  const TYPED = 'Kagami no Kuni no Harisugawa v01 (2)'

  /** The first tile's panel open, ` (2)` typed into its field by real keys. */
  async function typed() {
    const { host, tiles } = await drawn({ width: 1280 })
    const panel = await open(tiles[0])
    const field = panel.querySelector<HTMLTextAreaElement>("[name='title']")
    if (field === null) throw new Error('no Title field')
    await real(() => userEvent.click(field))
    await real(() => userEvent.keyboard('{Control>}{End}{/Control} (2)'))
    expect(field.value).toBe(TYPED)
    return { host, tiles, field }
  }

  const calls = () => vi.mocked(metadata.patchWork).mock.calls
  const drawnIn = (tiles: Element[]) =>
    tiles.map((tile) => tile.querySelector('.tile-panel') !== null)
  /**
   * Whether `tile`'s save has been answered: its panel closed, or the
   * panel says "Saved" or draws the failure.
   */
  const answered = (tile: Element) => {
    const panel = tile.querySelector('.tile-panel')
    return (
      panel === null ||
      panel.querySelector('.saved-note')?.textContent === 'Saved' ||
      panel.querySelector('p.failure') !== null
    )
  }

  it('sends one save and closes once it lands, for a real press outside while the field’s own save is out', async () => {
    onTestFinished(pointerRulesAs('mouse'))
    const save = deferred<metadata.PatchResult>()
    vi.mocked(metadata.patchWork).mockReturnValueOnce(save.promise)
    const { host, tiles } = await typed()
    await real(() => userEvent.click(host.querySelector('h1') as Element))
    expect({ drawn: drawnIn(tiles)[0], calls: calls() }).toEqual({
      drawn: true,
      calls: [['kagami-v01', { title: TYPED }]],
    })

    await real(async () => {
      save.resolve({
        taiyaku_library: 1,
        updated: '2026-11-14T00:00:00Z',
        title: TYPED,
        persisted: true,
      })
    })
    await settleUntil('the save answered', () => answered(tiles[0]), 1000)
    expect({ drawn: drawnIn(tiles)[0], calls: calls().length }).toEqual({
      drawn: false,
      calls: 1,
    })
  })

  // The panel a close waits on stays open, so the wait is drawn in it as in
  // a panel that stays: a slow save says so (spec item 9).
  it('says Saving… in the panel a real press outside is closing, while the save it waits on is out past 200 ms', async () => {
    onTestFinished(pointerRulesAs('mouse'))
    const save = deferred<metadata.PatchResult>()
    vi.mocked(metadata.patchWork).mockReturnValueOnce(save.promise)
    const { host, tiles } = await typed()
    await real(() => userEvent.click(host.querySelector('h1') as Element))
    await act(async () => {
      await new Promise((r) => setTimeout(r, 250))
    })
    expect({
      drawn: drawnIn(tiles)[0],
      note: tiles[0].querySelector('.tile-panel .saved-note')?.textContent,
      calls: calls().length,
    }).toEqual({ drawn: true, note: 'Saving…', calls: 1 })

    await real(async () => {
      save.resolve({
        taiyaku_library: 1,
        updated: '2026-11-14T00:00:00Z',
        title: TYPED,
        persisted: true,
      })
    })
    await settleUntil('the save answered', () => answered(tiles[0]), 1000)
    expect({ drawn: drawnIn(tiles)[0], calls: calls().length }).toEqual({
      drawn: false,
      calls: 1,
    })
  })

  it('keeps the panel and shows the failure, for a real press outside while the field’s own save is out and fails', async () => {
    onTestFinished(pointerRulesAs('mouse'))
    const save = deferred<metadata.PatchResult>()
    vi.mocked(metadata.patchWork).mockReturnValueOnce(save.promise)
    const { host, tiles } = await typed()
    await real(() => userEvent.click(host.querySelector('h1') as Element))
    expect({ drawn: drawnIn(tiles)[0], calls: calls().length }).toEqual({
      drawn: true,
      calls: 1,
    })

    await real(async () => {
      save.reject(new Error('/api/library/works/kagami-v01: 500'))
    })
    await settleUntil('the save answered', () => answered(tiles[0]), 1000)
    expect({
      drawn: drawnIn(tiles)[0],
      failure: tiles[0].querySelector('.tile-panel p.failure')?.textContent,
      calls: calls().length,
    }).toEqual({
      drawn: true,
      failure: '/api/library/works/kagami-v01: 500',
      calls: 1,
    })
  })

  it('closes on real Escapes: the first restores the title, the second closes and focuses i', async () => {
    onTestFinished(pointerRulesAs('mouse'))
    const { tiles, field } = await typed()
    await real(() => userEvent.keyboard('{Escape}'))
    expect({
      drawn: drawnIn(tiles)[0],
      value: field.value,
      focused: document.activeElement === field,
    }).toEqual({
      drawn: true,
      value: 'Kagami no Kuni no Harisugawa v01',
      focused: true,
    })

    await real(() => userEvent.keyboard('{Escape}'))
    expect({
      drawn: drawnIn(tiles)[0],
      focused: document.activeElement === iOf(tiles[0]),
      calls: calls().length,
    }).toEqual({ drawn: false, focused: true, calls: 0 })
  })

  it('a real press on another tile’s i saves the typed title, then opens that tile’s panel', async () => {
    onTestFinished(pointerRulesAs('mouse'))
    const save = deferred<metadata.PatchResult>()
    vi.mocked(metadata.patchWork).mockReturnValueOnce(save.promise)
    const { tiles } = await typed()
    await real(() =>
      userEvent.hover(
        tiles[1].querySelector(':scope > .tile-body > a') as Element,
      ),
    )
    await real(() => userEvent.click(iOf(tiles[1])))
    expect({ drawn: drawnIn(tiles).slice(0, 2), calls: calls() }).toEqual({
      drawn: [true, false],
      calls: [['kagami-v01', { title: TYPED }]],
    })

    await real(async () => {
      save.resolve({
        taiyaku_library: 1,
        updated: '2026-11-14T00:00:00Z',
        title: TYPED,
        persisted: true,
      })
    })
    await settleUntil(
      'the first panel closed or the second open',
      () => !drawnIn(tiles)[0] || drawnIn(tiles)[1],
      1000,
    )
    expect({
      drawn: drawnIn(tiles).slice(0, 2),
      calls: calls().length,
    }).toEqual({ drawn: [false, true], calls: 1 })
  })
})

describe('a volume’s content matches, pressed with a real mouse', () => {
  const edition = (
    key: string,
    matched: library.ContentMatch | null,
  ): library.EditionCard => ({
    key,
    label: key,
    members: 1,
    pages: 180,
    missing: 0,
    cover: null,
    matched,
  })
  const REFUSED =
    'a scan or another library job is running; split again once it has finished'

  const json = (body: unknown, status = 200) =>
    new Response(JSON.stringify(body), {
      status,
      headers: { 'content-type': 'application/json' },
    })

  /**
   * Volume 1 of three editions, the raw hub and two joined to it by content:
   * its read carries both marks and pair 9, kept apart from volume 5. Once a
   * Split has answered, en-fan has no mark and, with `lands`, pair 7 is kept
   * apart; once an Undo has, pair 9 is not, and with `lands` it joins fr.
   * A read after an answer waits for `held`. `fetch` answers the match
   * routes with `answer` and keeps what each sent; every other request goes
   * to the `fetch` before. Every other volume as `serve` answers it.
   */
  function serveMatches(
    answer: () => Promise<Response>,
    {
      lands = true,
      held,
      // The Split that kept pair 9 apart kept pair 10 apart too, from the
      // fan's volume 5, and pair 10 joins fr again.
      together = false,
    }: { lands?: boolean; held?: Promise<void>; together?: boolean } = {},
  ) {
    const works = volumes({
      0: {
        editions: [
          edition('raw', null),
          edition('en', null),
          edition('en-fan', null),
        ],
      },
    })
    serve(works)
    let split = false
    let undone = false
    vi.mocked(library.work).mockImplementation(async (slug) => {
      const card = works.find((w) => w.slug === slug) as library.WorkCard
      if (slug !== 'kagami-v01') return { ...card, alignments: [] }
      if ((split || undone) && held !== undefined) await held
      return {
        ...card,
        editions: [
          edition('raw', null),
          edition('en', { id: 5, score: 0.93 }),
          edition('en-fan', split ? null : { id: 7, score: 0.88 }),
          ...(undone && lands
            ? [edition('fr', { id: together ? 10 : 9, score: 0.91 })]
            : []),
        ],
        alignments: [],
        kept_apart: [
          ...(undone
            ? []
            : [
                {
                  id: 9,
                  split: 9,
                  series: 'Kagami no Kuni no Harisugawa',
                  volume: 5,
                  slug: 'kagami-v05',
                },
                ...(together
                  ? [
                      {
                        id: 10,
                        split: 9,
                        series: 'Kagami no Kuni no Harisugawa (fan)',
                        volume: 5,
                        slug: 'kagami-fan-v05',
                      },
                    ]
                  : []),
              ]),
          ...(split && lands
            ? [
                {
                  id: 7,
                  split: 7,
                  series: 'Kagami no Kuni no Harisugawa (fan)',
                  volume: 1,
                  slug: 'kagami-fan-v01',
                },
              ]
            : []),
        ],
      }
    })
    const sent: { method?: string; url: string; body?: unknown }[] = []
    const before = globalThis.fetch
    onTestFinished(() => {
      vi.unstubAllGlobals()
    })
    vi.stubGlobal(
      'fetch',
      vi.fn((input: RequestInfo | URL, init?: RequestInit) => {
        const url = String(input)
        if (!/^\/api\/library\/matches\/\d+\/split$/.test(url))
          return before(input, init)
        sent.push({
          method: init?.method,
          url,
          ...(init?.body === undefined
            ? {}
            : { body: JSON.parse(String(init.body)) as unknown }),
        })
        return answer().then((res) => {
          if (res.ok && init?.method === 'POST') split = true
          if (res.ok && init?.method === 'DELETE') undone = true
          return res
        })
      }),
    )
    return sent
  }

  /** Volume 1's panel opened with a real mouse, its marks drawn. */
  async function marksOpen() {
    const { tiles } = await drawn({ width: 1280 })
    const panel = await open(tiles[0])
    await settleUntil(
      'the marks',
      () => panel.querySelector('.match-marks') !== null,
      1000,
    )
    return panel
  }

  /** The `index`th button named `name` in `panel`'s marks. */
  const named = (panel: Element, name: string, index = 0) => {
    const found = panel.querySelectorAll<HTMLButtonElement>(
      `.match-marks button[name="${name}"]`,
    )[index]
    if (found === undefined) throw new Error(`no ${name} button ${index}`)
    return found
  }
  const question = (panel: Element) =>
    panel.querySelector('.match-marks [role="group"]')
  const marks = (panel: Element) =>
    [...panel.querySelectorAll('.match-marks li > .mark > p')].map(
      (p) => p.textContent,
    )
  const reads = () =>
    vi.mocked(library.work).mock.calls.filter(([slug]) => slug === 'kagami-v01')
      .length

  it('opens the question on Split…, and its Cancel sends nothing', async () => {
    const sent = serveMatches(async () =>
      json({ split: [7], derive: emptyDerive }),
    )
    const panel = await marksOpen()
    await real(() => userEvent.click(named(panel, 'split', 1)))
    expect(question(panel)?.getAttribute('aria-label')).toBe(
      'Split en-fan off this volume',
    )
    expect(sent).toEqual([])

    await real(() => userEvent.click(named(panel, 'cancel-split')))
    expect(question(panel)).toBeNull()
    expect(sent).toEqual([])
    expect(reads()).toBe(1)
  })

  it('sends one Split, naming the volume and the second marked edition pressed, and reads the volume again', async () => {
    const sent = serveMatches(async () =>
      json({ split: [7], derive: emptyDerive }),
    )
    const panel = await marksOpen()
    await real(() => userEvent.click(named(panel, 'split', 1)))
    await real(() => userEvent.click(named(panel, 'confirm-split')))
    await settleUntil('the read after the Split', () => reads() === 2, 1000)
    await settleUntil(
      'the marks read again',
      () => !marks(panel).some((line) => line?.startsWith('en-fan')),
      1000,
    )
    expect(sent).toEqual([
      {
        method: 'POST',
        url: '/api/library/matches/7/split',
        body: { work: 'kagami-v01', edition: 'en-fan' },
      },
    ])
    expect(marks(panel)).toEqual([
      'en matched by content · 0.93',
      'kept apart from Kagami no Kuni no Harisugawa v05',
      'kept apart from Kagami no Kuni no Harisugawa (fan) v01',
    ])
  })

  it('sends one Undo, a DELETE of the pair kept apart', async () => {
    const sent = serveMatches(async () => json({ derive: emptyDerive }))
    const panel = await marksOpen()
    await real(() => userEvent.click(named(panel, 'undo-split')))
    await settleUntil('the read after the Undo', () => reads() === 2, 1000)
    expect(sent).toEqual([
      { method: 'DELETE', url: '/api/library/matches/9/split' },
    ])
  })

  it('draws a Split that kept the volume apart from two volumes as one line with one Undo, which sends one DELETE and gives focus to the Split… of the edition it rejoined', async () => {
    const sent = serveMatches(async () => json({ derive: emptyDerive }), {
      together: true,
    })
    const panel = await marksOpen()
    expect(marks(panel)).toEqual([
      'en matched by content · 0.93',
      'en-fan matched by content · 0.88',
      'kept apart from Kagami no Kuni no Harisugawa v05 and Kagami no Kuni no Harisugawa (fan) v05',
    ])
    expect(
      panel.querySelectorAll('.match-marks button[name="undo-split"]'),
    ).toHaveLength(1)

    await real(() => userEvent.click(named(panel, 'undo-split')))
    await settleUntil('the read after the Undo', () => reads() === 2, 1000)
    await settleUntil(
      'the rejoined edition’s Split…',
      () =>
        document.activeElement ===
        panel.querySelector(
          '.match-marks button[name="split"][data-pair="10"]',
        ),
      1000,
    )
    expect(sent).toEqual([
      { method: 'DELETE', url: '/api/library/matches/9/split' },
    ])
    expect(marks(panel)).toEqual([
      'en matched by content · 0.93',
      'en-fan matched by content · 0.88',
      'fr matched by content · 0.91',
    ])
  })

  it('shows a refused Split in the server’s words, and its button sends again when pressed again', async () => {
    const sent = serveMatches(async () => json({ error: REFUSED }, 409))
    const panel = await marksOpen()
    await real(() => userEvent.click(named(panel, 'split', 1)))
    await real(() => userEvent.click(named(panel, 'confirm-split')))
    await settleUntil(
      'the refusal',
      () => panel.querySelector('.match-marks [role="alert"]') !== null,
      1000,
    )
    expect(
      panel.querySelector('.match-marks [role="alert"]')?.textContent,
    ).toBe(REFUSED)
    const confirm = named(panel, 'confirm-split')
    expect([confirm.textContent, holdOf(confirm)]).toEqual([
      'Split en-fan',
      'free',
    ])

    await real(() => userEvent.click(confirm))
    await settleUntil('the second send', () => sent.length === 2, 1000)
    expect(sent.map(({ method, url }) => [method, url])).toEqual([
      ['POST', '/api/library/matches/7/split'],
      ['POST', '/api/library/matches/7/split'],
    ])
    expect(reads()).toBe(1)
  })

  it('sends once for two presses inside one act, before the hold is drawn', async () => {
    const answer = deferred<Response>()
    const sent = serveMatches(() => answer.promise)
    const panel = await marksOpen()
    await real(() => userEvent.click(named(panel, 'split', 1)))
    const confirm = named(panel, 'confirm-split')
    // Dispatched, not real: React draws the hold after each real click, so
    // a second real click lands on a held button, which turns it down
    // whether or not the ref does. Two presses in one task, before a
    // render, are what only the ref turns down.
    await act(async () => {
      confirm.click()
      confirm.click()
      await Promise.resolve()
    })
    expect(sent).toHaveLength(1)

    answer.resolve(json({ split: [7], derive: emptyDerive }))
    await settleUntil('the read after the Split', () => reads() === 2, 1000)
    expect(sent).toHaveLength(1)
  })

  describe('where focus goes as a press takes away the control that had it', () => {
    /** Volume 1's panel opened by a real click, which put focus on its i. */
    async function focusedOpen() {
      const panel = await marksOpen()
      expect(document.activeElement).toBe(
        iOf(panel.closest('li') as HTMLElement),
      )
      return panel
    }
    const undoOf = (panel: Element, pair: number) =>
      panel.querySelector<HTMLElement>(
        `.match-marks button[name="undo-split"][data-pair="${pair}"]`,
      )
    const splitOf = (panel: Element, pair: number) =>
      panel.querySelector<HTMLElement>(
        `.match-marks button[name="split"][data-pair="${pair}"]`,
      )

    /**
     * Volume 2 holding only the file a Split sent back from volume 1, kept
     * apart from it by pair 9. The Undo joins that file to volume 1 again and
     * the derive empties volume 2 and deletes its work, as the server's does
     * (`fold::absorb`): the series read after it has no volume 2, and volume
     * 2's read is refused. Those two reads wait for `series` and `work`.
     */
    function emptiedByUndo(series: Promise<void>, work: Promise<void>) {
      const works = volumes()
      const page = (list: library.WorkCard[]) => ({
        series: {
          id: 7,
          title: 'Kagami no Kuni no Harisugawa',
          works: list.length,
          editions: list.length,
          aligned: 0,
          added: 1,
          unread: list.length,
          cover: null,
          titles: [],
        },
        works: list,
      })
      let undone = false
      vi.mocked(library.oneSeries).mockImplementation(async () => {
        if (!undone) return page(works)
        await series
        return page(works.filter((w) => w.slug !== 'kagami-v02'))
      })
      vi.mocked(library.work).mockImplementation(async (slug) => {
        const card = works.find((w) => w.slug === slug) as library.WorkCard
        if (slug !== 'kagami-v02') return { ...card, alignments: [] }
        if (undone) {
          await work
          throw new Error(
            '/api/library/works/kagami-v02: 404 {"error":"there is no work kagami-v02"}',
          )
        }
        return {
          ...card,
          alignments: [],
          kept_apart: [
            {
              id: 9,
              split: 9,
              series: 'Kagami no Kuni no Harisugawa',
              volume: 1,
              slug: 'kagami-v01',
            },
          ],
        }
      })
      const before = globalThis.fetch
      onTestFinished(() => {
        vi.unstubAllGlobals()
      })
      vi.stubGlobal(
        'fetch',
        vi.fn((input: RequestInfo | URL, init?: RequestInit) => {
          if (
            String(input) !== '/api/library/matches/9/split' ||
            init?.method !== 'DELETE'
          )
            return before(input, init)
          undone = true
          return Promise.resolve(json({ derive: emptyDerive }))
        }),
      )
    }

    /** Volume 2's panel opened by a real click, and its Undo pressed. */
    async function undoOnVolumeTwo() {
      const { host, tiles, main } = await drawn({ width: 1280 })
      const panel = await open(tiles[1])
      const undo = () =>
        panel.querySelector<HTMLElement>('button[name="undo-split"]')
      await settleUntil('volume 2’s Undo', () => undo() !== null, 1000)
      expect(document.activeElement).toBe(iOf(tiles[1]))
      await real(() => userEvent.click(undo() as Element))
      return { host, tiles, main, panel }
    }

    it('goes to the page’s main when an Undo empties the volume pressed on, its read refused first', async () => {
      const series = deferred<void>()
      emptiedByUndo(series.promise, Promise.resolve())
      const { tiles, main, panel } = await undoOnVolumeTwo()
      await settleUntil(
        'volume 2’s read refused',
        () => panel.querySelector('.alignment-panel [role="alert"]') !== null,
        1000,
      )
      expect(document.activeElement).toBe(panel)

      series.resolve()
      await settleUntil('volume 2 gone', () => !tiles[1].isConnected, 1000)
      expect(document.activeElement).toBe(main)
    })

    it('goes to the page’s main when an Undo empties the volume pressed on, the series read first', async () => {
      const work = deferred<void>()
      emptiedByUndo(Promise.resolve(), work.promise)
      const { tiles, main } = await undoOnVolumeTwo()
      await settleUntil('volume 2 gone', () => !tiles[1].isConnected, 1000)
      expect(document.activeElement).toBe(main)

      work.resolve()
      await real(() => new Promise((resolve) => setTimeout(resolve, 50)))
      expect(document.activeElement).toBe(main)
    })

    it('stays where the person moved it when an Undo empties the volume pressed on', async () => {
      const series = deferred<void>()
      emptiedByUndo(series.promise, Promise.resolve())
      const { tiles } = await undoOnVolumeTwo()
      const elsewhere = iOf(tiles[2])
      await real(async () => elsewhere.focus())
      series.resolve()
      await settleUntil('volume 2 gone', () => !tiles[1].isConnected, 1000)
      expect(document.activeElement).toBe(elsewhere)
    })

    it('goes back to the Split… that asked, when the question’s Cancel is pressed', async () => {
      serveMatches(async () => json({ split: [7], derive: emptyDerive }))
      const panel = await focusedOpen()
      const split = named(panel, 'split', 1)
      await real(() => userEvent.click(split))
      expect(document.activeElement).toBe(split)
      await real(() => userEvent.click(named(panel, 'cancel-split')))
      expect(question(panel)).toBeNull()
      expect(document.activeElement).toBe(split)
    })

    it('rests on the panel while a confirmed Split is read again, then goes to the Undo of the pair it split', async () => {
      const read = deferred<void>()
      serveMatches(async () => json({ split: [7], derive: emptyDerive }), {
        held: read.promise,
      })
      const panel = await focusedOpen()
      await real(() => userEvent.click(named(panel, 'split', 1)))
      await real(() => userEvent.click(named(panel, 'confirm-split')))
      await settleUntil(
        'the question gone',
        () => question(panel) === null,
        1000,
      )
      expect(document.activeElement).toBe(panel)

      read.resolve()
      await settleUntil(
        'the Undo of pair 7',
        () => undoOf(panel, 7) !== null,
        1000,
      )
      expect(document.activeElement).toBe(undoOf(panel, 7))
    })

    it('goes to the panel when the read after a Split draws no Undo of the pair it split', async () => {
      serveMatches(async () => json({ split: [7], derive: emptyDerive }), {
        lands: false,
      })
      const panel = await focusedOpen()
      await real(() => userEvent.click(named(panel, 'split', 1)))
      await real(() => userEvent.click(named(panel, 'confirm-split')))
      await settleUntil(
        'the marks read again',
        () => splitOf(panel, 7) === null && reads() === 2,
        1000,
      )
      expect(document.activeElement).toBe(panel)
    })

    it('goes to the Split… of the edition an Undo joined again', async () => {
      serveMatches(async () => json({ derive: emptyDerive }))
      const panel = await focusedOpen()
      await real(() => userEvent.click(named(panel, 'undo-split')))
      await settleUntil('fr’s Split…', () => splitOf(panel, 9) !== null, 1000)
      expect(document.activeElement).toBe(splitOf(panel, 9))
    })

    it('goes to the panel when the read after an Undo draws no Split… of that pair', async () => {
      serveMatches(async () => json({ derive: emptyDerive }), { lands: false })
      const panel = await focusedOpen()
      await real(() => userEvent.click(named(panel, 'undo-split')))
      await settleUntil('the Undo gone', () => undoOf(panel, 9) === null, 1000)
      expect(document.activeElement).toBe(panel)
      // Focusable only while it holds that focus.
      await real(() => userEvent.click(named(panel, 'split')))
      expect(document.activeElement).toBe(named(panel, 'split'))
      expect(panel.hasAttribute('tabindex')).toBe(false)
    })
  })
})

/**
 * The panel at a phone's width, where it spans the page area from either
 * column. What it prints, a plan's path, a content_key, the title and an
 * error or a reason naming a path, holds no space to break at, and stays
 * inside it (O274).
 */
describe('a stamp plan drawn in a volume’s panel (O274)', () => {
  /** A title with no space in it, as a title taken from a file name can be. */
  const TITLE = 'Kagami_no_Kuni_no_Harisugawa_Complete_Digital_Edition'

  /**
   * The panel of the first (`left`) or second (`right`) of two volumes at
   * 390 px, opened, and its plan read. With `alignmentError`, reading the
   * volume's detail fails with it; set after `serve`, whose answer would
   * replace it.
   */
  async function planIn(column: 'left' | 'right', alignmentError?: string) {
    serve([work(1, { title: TITLE }), work(2, { title: TITLE })])
    if (alignmentError !== undefined) {
      vi.mocked(library.work).mockRejectedValue(new Error(alignmentError))
    }
    const { host, main, tiles } = await drawn({
      width: 390,
      who: STAMPER,
      text: `${TITLE} ${LONG} ${KEY}`,
    })
    const panel = await open(tiles[column === 'left' ? 0 : 1])
    await real(() =>
      userEvent.click(
        panel.querySelector("button[name='plan-stamp']") as Element,
      ),
    )
    await settleUntil(
      'the plan’s path',
      () => panel.querySelector('.archives .path') !== null,
      1000,
    )
    return { host, main, panel }
  }

  it('O274: a stamp plan’s path stays inside the panel at 390 px', async () => {
    onTestFinished(pointerRulesAs('mouse'))
    const { panel } = await planIn('left')
    const paths = [...panel.querySelectorAll('.archives .path')]
    expect(paths).toHaveLength(1)
    for (const path of paths) {
      // The fixture tells: unbroken, the last name cannot fit the panel.
      expect(
        oneLineIn(path, LONG.slice(LONG.lastIndexOf('/') + 1)),
        'the last name on one line against the panel’s content box',
      ).toBeGreaterThan(lineOf(panel))
      expect(
        rect(path).right,
        'the path’s right edge against its panel’s',
      ).toBeLessThanOrEqual(rect(panel).right + 0.5)
    }
    expect(document.documentElement.scrollWidth).toBeLessThanOrEqual(
      window.innerWidth,
    )
  })

  it('O274: the editor stays inside the panel of a right-hand tile at 390 px, so the page does not scroll sideways', async () => {
    onTestFinished(pointerRulesAs('mouse'))
    const error = `${LONG}: no such file or directory`
    const { main, panel } = await planIn('right', error)
    const title = panel.querySelector(".metadata-editor [name='title']")
    const label = title?.closest('label')
    const alignment = panel.querySelector('.alignment-panel p')
    if (title == null || label == null || alignment === null) {
      throw new Error('no title field, its label or alignment detail')
    }

    // The fixture tells, each part first. The panel ends where the page
    // area does, and the alignment detail draws the error, whose last name
    // is wider than the panel's line.
    expect(rect(panel).right).toBeCloseTo(contentOf(main).right, 0)
    expect(
      oneLineIn(label, TITLE),
      'the title on one line against the field’s line',
    ).toBeGreaterThan(lineOf(title))
    expect(alignment.textContent, 'the alignment detail').toBe(error)
    expect(
      oneLineIn(alignment, LONG.slice(LONG.lastIndexOf('/') + 1)),
      'the alignment error’s last name on one line against the panel’s line',
    ).toBeGreaterThan(lineOf(panel))
    expect(
      oneLineIn(
        panel.querySelector('.archives .path') as Element,
        LONG.slice(LONG.lastIndexOf('/') + 1),
      ),
      'the last name on one line against the panel’s content box',
    ).toBeGreaterThan(lineOf(panel))

    // Per element and per text node, so a failure names what overran.
    expect(pastTheEdgeOf(panel)).toEqual([])
    expect(document.documentElement.scrollWidth).toBeLessThanOrEqual(
      window.innerWidth,
    )
  })

  it('O274: a stamp’s outcome and a save’s unwritten reason stay inside the panel of a right-hand tile at 390 px', async () => {
    onTestFinished(pointerRulesAs('mouse'))
    // The reason names the document by its path, as `hand::save` does.
    vi.mocked(metadata.patchWork).mockResolvedValue({
      taiyaku_library: 1,
      updated: '2026-11-14T00:00:00Z',
      title: TITLE,
      persisted: false,
      why: [`/mnt/user/manga/${KEY}.library.json: the root refused the write`],
    })
    const { panel } = await planIn('right')
    const confirm = () => panel.querySelector("button[name='confirm-stamp']")
    await settleUntil(
      'the confirm, free',
      () => {
        const button = confirm()
        return button !== null && holdOf(button) === 'free'
      },
      1000,
    )
    await real(() => userEvent.click(confirm() as HTMLElement))
    await settleUntil(
      'the stamp’s outcome',
      () => panel.querySelector('.stamped .document') !== null,
      1000,
    )
    const field = panel.querySelector(
      ".metadata-editor [name='title']",
    ) as HTMLElement
    await real(() => userEvent.click(field))
    await real(() => userEvent.keyboard('{Control>}{End}{/Control} (2){Enter}'))
    await settleUntil(
      'the unwritten reason',
      () => panel.querySelector('.unwritten li') !== null,
      1000,
    )
    const moved = panel.querySelector('.stamped .key-moved')
    const reason = panel.querySelector('.unwritten li')
    if (moved === null || reason === null) {
      throw new Error('no moved key or unwritten reason')
    }

    // The fixture tells, each part first, in a panel as wide as a phone's
    // page area.
    expect(
      oneLineIn(moved, KEY),
      'the content_key on one line against its line',
    ).toBeGreaterThan(lineOf(moved))
    expect(
      oneLineIn(reason, `${KEY}.library.json:`),
      'the reason’s document name on one line against its line',
    ).toBeGreaterThan(lineOf(reason))

    // Per element and per text node, so a failure names what overran.
    expect(pastTheEdgeOf(panel)).toEqual([])
    expect(document.documentElement.scrollWidth).toBeLessThanOrEqual(
      window.innerWidth,
    )
  })
})
