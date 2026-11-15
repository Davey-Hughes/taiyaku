import { act } from 'react'
import { beforeEach, describe, expect, it, onTestFinished, vi } from 'vitest'
import { page, userEvent } from 'vitest/browser'
// **Required**, the same reason `Config.browser.test.tsx` gives.
import '../src/index.css'
import type { Me } from '../src/api'
import * as derive from '../src/libraries'
import * as library from '../src/library'
import * as metadata from '../src/metadata'
import * as stamp from '../src/stamp'
import { deferred, settleUntil } from './support/async'
import { holdOf } from './support/held'
import { lineCount, lineOf, oneLineIn, pastTheEdgeOf } from './support/lines'
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
 * Twenty volumes: three rows at 1280 px, seven to a row comfortable and
 * eight compact, and ten rows of two at 390 px.
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
  const i = tile.querySelector<HTMLButtonElement>(':scope > .panel-open')
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
  compact = false,
  who = ADMIN,
  text = 'Kagami no Kuni no Harisugawa v01',
}: {
  width: number
  compact?: boolean
  who?: Me
  text?: string
}) {
  const host = await seriesPage({ width, compact, panel: 'button', who })
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
      tile.querySelector(':scope > :is(a, .tile-unreadable)') as Element,
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
          tile.querySelector(':scope > a'),
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
    ['comfortable', false, 16],
    ['compact', true, 9.6],
  ] as const)(
    'is two tiles and the gap wide, from its tile’s left edge, at 1280 px (%s)',
    async (_, compact, gap) => {
      onTestFinished(pointerRulesAs('mouse'))
      const { main, tiles } = await drawn({ width: 1280, compact })
      const tile = rect(tiles[0])
      // The fixture tells: the grid's gap is the density's, and two tiles
      // and the gap fit right of the tile's left edge.
      expect(rect(tiles[1]).left - tile.right).toBeCloseTo(gap, 1)
      expect(tile.left + 2 * tile.width + gap).toBeLessThan(
        contentOf(main).right,
      )

      const box = rect(await open(tiles[0]))
      expect(box.left).toBeCloseTo(tile.left, 0)
      expect(box.width).toBeCloseTo(2 * tile.width + gap, 0)
    },
  )

  it('lies back from its tile’s right edge in the row’s last column at 1280 px', async () => {
    onTestFinished(pointerRulesAs('mouse'))
    const { main, tiles } = await drawn({ width: 1280 })
    const last = tiles[6]
    const tile = rect(last)
    // The fixture tells: the tile's right edge is the area's, and two tiles
    // and the gap from its left edge run past the area.
    expect(tile.right).toBeCloseTo(contentOf(main).right, 0)
    expect(tile.left + 2 * tile.width + 16).toBeGreaterThan(
      contentOf(main).right + 0.5,
    )

    const panel = await open(last)
    expect(rect(panel).right).toBeCloseTo(tile.right, 0)
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
      const { tiles } = await drawn({ width, compact: true })
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
    expect(rect(panel).width).toBeCloseTo(2 * tile.width + 16, 0)
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
    const last = tiles[6]
    const panel = await open(last)
    // The fixture tells: at 1280 px the tile is the row's last, and its
    // panel lies back from its right edge.
    expect(rect(last).right).toBeCloseTo(contentOf(main).right, 0)
    expect(rect(panel).left).toBeLessThan(rect(last).left - 1)

    await page.viewport(800, 800)
    await expect.poll(() => window.innerWidth).toBe(800)
    // The fixture tells: no longer the row's last, and two tiles and the
    // gap from its left edge end 8 px or more inside the area.
    const tile = rect(last)
    const area = contentOf(main)
    expect(tile.right).toBeLessThan(area.right - 1)
    expect(tile.left + 2 * tile.width + 16).toBeLessThanOrEqual(area.right - 8)

    await expect
      .poll(() => rect(panel).left, { message: 'the panel’s left edge' })
      .toBeCloseTo(tile.left, 0)
    const box = rect(panel)
    expect(box.left).toBeGreaterThanOrEqual(area.left - 0.5)
    expect(box.right).toBeLessThanOrEqual(area.right + 0.5)
  })

  it('is laid again, its field fitted again, when the page area’s width changes and the window’s does not', async () => {
    onTestFinished(pointerRulesAs('mouse'))
    const title =
      'Kagami no Kuni no Harisugawa: The Complete Digital Edition, Volume 01 (Collector’s Box Set, Ed.)'
    serve(volumes({ 0: { title } }))
    const { host, tiles } = await drawn({ width: 1280, text: title })
    const panel = await open(tiles[0])
    const field = panel.querySelector<HTMLTextAreaElement>("[name='title']")
    if (field === null) throw new Error('no Title field')
    const was = { panel: rect(panel).width, line: lineOf(field) }

    // A wider sidebar column takes the width from the page area alone.
    const shell = host.querySelector<HTMLElement>('.library')
    if (shell === null) throw new Error('no .library mounted')
    shell.style.gridTemplateColumns = '28rem 1fr'
    const tile = rect(tiles[0])
    const wide = 2 * tile.width + 16

    // The fixture tells: the window is as it was, the tile's width is not,
    // and the title takes another count of lines at the field's new width.
    expect(window.innerWidth).toBe(1280)
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
      'alignment-panel',
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
      textTop: expect.closeTo(0, 0),
    })
    // The stamp section's heading starts at its part's top.
    expect(rect(parts[1].querySelector(':scope > h3')).top).toBeCloseTo(
      rect(parts[1]).top,
      0,
    )
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
      userEvent.hover(tiles[1].querySelector(':scope > a') as Element),
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
