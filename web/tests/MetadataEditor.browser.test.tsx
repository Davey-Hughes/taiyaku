import { act } from 'react'
import { beforeEach, describe, expect, it, onTestFinished, vi } from 'vitest'
import { page, userEvent } from 'vitest/browser'
// Required, the same reason every other file in this project gives: the
// rules under test live in `styles/`, whose sheets reach the app through
// `index.css`. Without this the boxes below measure browser defaults.
import '../src/index.css'
import { SCAN_POLL_MS } from '../src/config'
import * as derive from '../src/libraries'
import { MetadataEditor } from '../src/MetadataEditor'
import * as stamp from '../src/stamp'
import { settleUntil } from './support/async'
import { pressOn } from './support/callout'
import { holdOf } from './support/held'
import { mount } from './support/mount'

vi.mock('../src/stamp', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/stamp')>()),
  stampPlan: vi.fn(),
  performStamp: vi.fn(),
}))
vi.mock('../src/libraries', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/libraries')>()),
  deriveStatus: vi.fn(),
}))

/**
 * `MetadataEditor.tsx`'s title field renders `<label className="field title">`
 * on the Series page. A bare `.field` selector would reach it and restyle a
 * screen nobody decided to change.
 *
 * Built from the class list by hand rather than by mounting `MetadataEditor`
 * itself — the defect is entirely about which stylesheet rule a class list
 * reaches, not about the component's own behaviour. The pair compared is a
 * hoisted `.field` beside an untouched control in the same `.library`.
 */
describe('the Series page’s title field, after the Rules screen hoisted .field', () => {
  it('is not restyled by a class the Series page never opted into', () => {
    const host = mount(
      <div className="library">
        <label className="field title">
          <span>Title</span>
          <input />
        </label>
        <label className="untouched">
          <span>Untouched</span>
          <input />
        </label>
      </div>,
    )
    const label = host.querySelector('.field.title')
    const hoisted = host.querySelector('.field.title input')
    const untouched = host.querySelector('.untouched input')
    if (!label || !hoisted || !untouched) {
      throw new Error('the fixture did not render')
    }

    // Compared against the untouched control rather than against hard-coded
    // numbers, so this still holds if a token's value changes.
    expect(getComputedStyle(label).textTransform).toBe('none')
    expect(getComputedStyle(hoisted).backgroundColor).toBe(
      getComputedStyle(untouched).backgroundColor,
    )
    expect(getComputedStyle(hoisted).borderRadius).toBe(
      getComputedStyle(untouched).borderRadius,
    )
    expect(getComputedStyle(hoisted).fontSize).toBe(
      getComputedStyle(untouched).fontSize,
    )
  })
})

/**
 * An open volume draws the editor inside its tile, `.library` > `main` >
 * `ul.volumes` > `li`, beside the tile's own `span.title`, and the editor's
 * Title field is `label.field.title`: a rule for the tile's title that named
 * any `.title` in `.volumes` would draw the field in the title face too.
 * The editor has one field, so the label is held to the form's other text,
 * `.current`, and its box to the form's other control, Save, which takes the
 * form's font through `base.css`'s `font: inherit` as the box does.
 */
describe('the editor’s Title field in a volume’s tile', () => {
  it('draws its label and box in the editor’s face, not the tile title’s', async () => {
    let host: HTMLElement | undefined
    await act(async () => {
      host = mount(
        <div className="library">
          <main>
            <ul className="volumes">
              <li>
                <a href="/read/w">
                  <span className="title">t</span>
                </a>
                <MetadataEditor
                  slug="w"
                  title="t"
                  onPatch={vi.fn()}
                  canStamp={false}
                />
              </li>
            </ul>
          </main>
        </div>,
      )
    })
    const need = (selector: string) => {
      const el = host?.querySelector(selector)
      if (el == null) throw new Error(`no ${selector} mounted`)
      return el
    }
    const face = (el: Element) => {
      const style = getComputedStyle(el)
      return {
        family: style.fontFamily,
        weight: style.fontWeight,
        size: style.fontSize,
        lineHeight: style.lineHeight,
      }
    }
    const tile = face(need('.volumes li > a > .title'))
    const text = face(need('.metadata-editor .current'))

    // The fixture tells: the tile's title differs from the form's text in
    // each of the four, so a field drawn in the title face cannot match.
    expect(tile.family).not.toBe(text.family)
    expect(tile.weight).not.toBe(text.weight)
    expect(tile.size).not.toBe(text.size)
    expect(tile.lineHeight).not.toBe(text.lineHeight)

    // One comparison, so a failure shows the label and the box together.
    expect({
      label: face(need('.metadata-editor .field.title')),
      box: face(need(".metadata-editor .field.title [name='title']")),
    }).toEqual({
      label: text,
      box: face(need(".metadata-editor button[type='submit']")),
    })
  })
})

describe('Write ComicInfo.xml’s confirm, held in a real browser (O51)', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    vi.mocked(stamp.stampPlan).mockResolvedValue({
      archives: [
        { member_id: 7, edition_key: 'e1', path: '/lib/v01.cbz', bytes: 1024 },
      ],
      total_bytes: 1024,
      skipped_formats: [],
    })
    vi.mocked(stamp.performStamp).mockResolvedValue({
      rewritten: 1,
      content_key: null,
      document_moved: false,
      document_rewritten: false,
      sidecar_rewritten: false,
    })
  })

  it('sends nothing for a real mouse press while the job lock holds, and stamps once a poll finds it free', async () => {
    vi.mocked(derive.deriveStatus)
      .mockResolvedValueOnce({ derive_running: false, job_held: true })
      .mockResolvedValue({ derive_running: false, job_held: false })
    let host: HTMLElement | undefined
    await act(async () => {
      host = mount(
        <MetadataEditor
          slug="w"
          title="t"
          onPatch={vi.fn().mockResolvedValue({ title: 't', persisted: true })}
          canStamp
        />,
      )
    })
    if (host === undefined) throw new Error('nothing mounted')
    const ask = host.querySelector("button[name='plan-stamp']") as HTMLElement
    await act(async () => {
      await userEvent.click(ask)
    })
    const confirm = () =>
      host?.querySelector("button[name='confirm-stamp']") as HTMLButtonElement
    await settleUntil(
      'the confirm, held',
      () => holdOf(confirm()) === 'held',
      1000,
    )
    await pressOn(confirm())
    expect(stamp.performStamp).not.toHaveBeenCalled()

    await settleUntil(
      'the confirm, free',
      () => holdOf(confirm()) === 'free',
      SCAN_POLL_MS * 3,
    )
    await pressOn(confirm())
    expect(stamp.performStamp).toHaveBeenCalledTimes(1)
  })
})

/**
 * The width of `text` set on one line in `el`'s own font: a span inside
 * `el` inherits it, and is taken out again before anything reads `el`.
 */
const oneLineIn = (el: Element, text: string) => {
  const probe = document.createElement('span')
  probe.style.position = 'absolute'
  probe.style.whiteSpace = 'pre'
  probe.style.visibility = 'hidden'
  probe.textContent = text
  el.append(probe)
  const width = probe.getBoundingClientRect().width
  probe.remove()
  return width
}

/**
 * The editor as a Series page draws it, in a volume's tile (`.library` >
 * `main` > `ul.volumes` > `li`), mostly in the right-hand tile of a phone's
 * two, which ends where the window does, so whatever overruns it widens the
 * page. The editor and the alignment panel stay inside their tile
 * (style-pass §7 ②), though a plan's path, `<content_key>.library.json`,
 * the title and an error naming a path hold no space to break at; in a
 * desktop's column the title box keeps to its tile too.
 */
describe('a stamp plan drawn in a volume’s tile (O274)', () => {
  /** Its last name has no `/`, `-` or space to break at. */
  const LONG =
    '/mnt/user/manga/Kagami_no_Kuni_no_Harisugawa_Complete_Digital_Edition_Volume_01.cbz'

  /** A title with no space in it, as a title taken from a file name can be. */
  const TITLE = 'Kagami_no_Kuni_no_Harisugawa_Complete_Digital_Edition'

  /** A content_key as `identity::content_key` writes it: sha256 in hex. */
  const KEY = '9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08'

  beforeEach(() => {
    vi.clearAllMocks()
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

  /** How wide a line `block` lays out is: its content box. */
  const lineOf = (block: Element) => {
    const style = getComputedStyle(block)
    return (
      block.clientWidth -
      parseFloat(style.paddingLeft) -
      parseFloat(style.paddingRight)
    )
  }

  /**
   * How wide `el` is drawn with no width or `max-width` of its own: a copy
   * beside it, in the same context, taken out again before anything reads
   * the page.
   */
  const unboundedWidthOf = (el: Element) => {
    const copy = el.cloneNode() as HTMLElement
    copy.style.position = 'absolute'
    copy.style.visibility = 'hidden'
    copy.style.width = 'auto'
    copy.style.maxWidth = 'none'
    el.after(copy)
    const width = copy.getBoundingClientRect().width
    copy.remove()
    return width
  }

  /**
   * Every element and every text node in `tile` whose box ends past the
   * tile's right edge. Text is asked on its own: text that overflows its
   * block leaves the block's box where it was.
   */
  const pastTheEdgeOf = (tile: Element) => {
    const edge = tile.getBoundingClientRect().right + 0.5
    const past: string[] = []
    for (const el of tile.querySelectorAll('*')) {
      if (el.getBoundingClientRect().right > edge) {
        past.push(`<${el.tagName.toLowerCase()} class="${el.className}">`)
      }
    }
    const walker = document.createTreeWalker(tile, NodeFilter.SHOW_TEXT)
    for (let node = walker.nextNode(); node; node = walker.nextNode()) {
      const range = document.createRange()
      range.selectNodeContents(node)
      if (range.getBoundingClientRect().right > edge) {
        past.push(JSON.stringify(node.textContent?.slice(0, 40)))
      }
    }
    return past
  }

  /**
   * The editor in the first (`left`) or second (`right`) of two volume
   * tiles, in a window `width` px wide, 390 unless given, where they are the
   * row's two columns; wider, the row has more. Its plan read; hands back
   * the tile, the volume's `li`, not `.path`'s nearest `li`, the archive's
   * row. A save answers `saved`. `inMain` draws the grid inside
   * `.library > main`, as the Series page does, whose padding narrows the
   * tiles.
   */
  async function planInTile(
    column: 'left' | 'right',
    {
      saved = { title: 't', persisted: true },
      inMain = false,
      alignmentError,
      width = 390,
    }: {
      saved?: { title: string; persisted: boolean; why?: string[] }
      inMain?: boolean
      alignmentError?: string
      width?: number
    } = {},
  ) {
    const was = [window.innerWidth, window.innerHeight] as const
    onTestFinished(() => page.viewport(...was))
    await page.viewport(width, 844)
    const editor = (
      <li>
        {/* Above the editor in the same tile, as the Series page draws it.
            `AlignmentPanel` is private to `Series.tsx`, so this is its error
            branch's markup by hand: the rule under test reads its root. */}
        {alignmentError !== undefined && (
          <div className="alignment-panel">
            <p role="alert">{alignmentError}</p>
          </div>
        )}
        <MetadataEditor
          slug="w"
          title={TITLE}
          onPatch={vi.fn().mockResolvedValue(saved)}
          canStamp
        />
      </li>
    )
    const volumes = (
      <ul className="volumes">
        {column === 'left' ? editor : <li />}
        {column === 'left' ? <li /> : editor}
      </ul>
    )
    let host: HTMLElement | undefined
    await act(async () => {
      host = mount(
        <div className="library">
          {/* From 900 px `.library` is a grid whose first column is the
              sidebar's (`shell.css`); with none drawn, `main` would take
              that narrow column instead of the page's. */}
          {inMain && width >= 900 && <div className="sidebar-column" />}
          {inMain ? <main>{volumes}</main> : volumes}
        </div>,
      )
    })
    if (host === undefined) throw new Error('nothing mounted')
    const ask = host.querySelector("button[name='plan-stamp']") as HTMLElement
    await act(async () => {
      await userEvent.click(ask)
    })
    await settleUntil(
      'the plan’s path',
      () => host?.querySelector('.archives .path') != null,
      1000,
    )
    const tile = host.querySelector(
      `.volumes > li:nth-child(${column === 'left' ? 1 : 2})`,
    )
    if (tile?.querySelector('.metadata-editor') == null) {
      throw new Error('the editor is not in the tile')
    }
    return { host, tile }
  }

  it('O274: a stamp plan’s path stays inside its tile at 390 px', async () => {
    const { host, tile } = await planInTile('left')
    const paths = [...host.querySelectorAll('.archives .path')]
    expect(paths).toHaveLength(1)
    for (const path of paths) {
      // The fixture tells: unbroken, the last name cannot fit the tile.
      expect(
        oneLineIn(path, LONG.slice(LONG.lastIndexOf('/') + 1)),
        'the last name on one line against the tile’s content box',
      ).toBeGreaterThan(lineOf(tile))
      expect(
        path.getBoundingClientRect().right,
        'the path’s right edge against its tile’s',
      ).toBeLessThanOrEqual(tile.getBoundingClientRect().right + 0.5)
    }
    expect(document.documentElement.scrollWidth).toBeLessThanOrEqual(
      window.innerWidth,
    )
  })

  it('O274: the editor stays inside a right-hand tile at 390 px, so the page does not scroll sideways', async () => {
    const { host, tile } = await planInTile('right', {
      alignmentError: `${LONG}: no such file or directory`,
    })
    const title = host.querySelector(".metadata-editor [name='title']")
    const current = host.querySelector('.metadata-editor .current')
    const alignment = host.querySelector('.alignment-panel p')
    const run = '<content_key>.library.json'
    const line = [...host.querySelectorAll('.consequences li')].find((li) =>
      li.textContent?.includes(run),
    )
    if (
      title === null ||
      current === null ||
      alignment === null ||
      line === undefined
    ) {
      throw new Error('no title box, title line, alignment error or run')
    }

    // The fixture tells, each part first. The tile ends where the window
    // does, so whatever runs past it widens the page.
    expect(tile.getBoundingClientRect().right).toBeCloseTo(window.innerWidth, 0)
    expect(
      oneLineIn(current, TITLE),
      'the title on one line against its line',
    ).toBeGreaterThan(lineOf(current))
    expect(
      oneLineIn(alignment, LONG.slice(LONG.lastIndexOf('/') + 1)),
      'the alignment error’s last name on one line against its line',
    ).toBeGreaterThan(lineOf(alignment))
    expect(
      oneLineIn(line, run),
      'the content_key file’s name on one line against its line',
    ).toBeGreaterThan(lineOf(line))
    expect(
      oneLineIn(
        host.querySelector('.archives .path') as Element,
        LONG.slice(LONG.lastIndexOf('/') + 1),
      ),
      'the last name on one line against the tile’s content box',
    ).toBeGreaterThan(lineOf(tile))

    // Per element and per text node, so a failure names what overran.
    expect(pastTheEdgeOf(tile)).toEqual([])
    expect(document.documentElement.scrollWidth).toBeLessThanOrEqual(
      window.innerWidth,
    )
  })

  it('O274: the title box keeps to its tile’s column at 1280 px', async () => {
    const { host, tile } = await planInTile('right', {
      width: 1280,
      inMain: true,
    })
    const box = host.querySelector(".metadata-editor [name='title']")
    if (box === null) throw new Error('no title box')
    const within = tile.getBoundingClientRect()

    // The fixture tells: at its own default width the box is wider than
    // the tile, so only the bound keeps it in.
    expect(
      unboundedWidthOf(box),
      'the title box at its own default width against the tile’s',
    ).toBeGreaterThan(within.width)

    const edges = box.getBoundingClientRect()
    expect(
      edges.left,
      'the box’s left edge against the tile’s',
    ).toBeGreaterThanOrEqual(within.left - 0.5)
    expect(
      edges.right,
      'the box’s right edge against the tile’s',
    ).toBeLessThanOrEqual(within.right + 0.5)
  })

  it('O274: a stamp’s outcome and a save’s unwritten reason stay inside a right-hand tile at 390 px', async () => {
    // The reason names the document by its path, as `hand::save` does.
    const { host, tile } = await planInTile('right', {
      saved: {
        title: TITLE,
        persisted: false,
        why: [
          `/mnt/user/manga/${KEY}.library.json: the root refused the write`,
        ],
      },
      inMain: true,
    })
    const confirm = () => host.querySelector("button[name='confirm-stamp']")
    await settleUntil(
      'the confirm, free',
      () => {
        const button = confirm()
        return button !== null && holdOf(button) === 'free'
      },
      1000,
    )
    await act(async () => {
      await userEvent.click(confirm() as HTMLElement)
    })
    await settleUntil(
      'the stamp’s outcome',
      () => host.querySelector('.stamped .document') != null,
      1000,
    )
    const save = host.querySelector(
      ".metadata-editor button[type='submit']",
    ) as HTMLElement
    await act(async () => {
      await userEvent.click(save)
    })
    await settleUntil(
      'the unwritten reason',
      () => host.querySelector('.unwritten li') != null,
      1000,
    )
    const moved = host.querySelector('.stamped .key-moved')
    const rewritten = host.querySelector('.stamped .document')
    const reason = host.querySelector('.unwritten li')
    if (moved === null || rewritten === null || reason === null) {
      throw new Error('no moved key, rewritten document or unwritten reason')
    }

    // The fixture tells, each part first, in a tile as wide as a phone's.
    expect(
      oneLineIn(moved, KEY),
      'the content_key on one line against its line',
    ).toBeGreaterThan(lineOf(moved))
    expect(
      oneLineIn(rewritten, '<content_key>.library.json'),
      'the document’s name on one line against its line',
    ).toBeGreaterThan(lineOf(rewritten))
    expect(
      oneLineIn(reason, `${KEY}.library.json:`),
      'the reason’s document name on one line against its line',
    ).toBeGreaterThan(lineOf(reason))

    // Per element and per text node, so a failure names what overran.
    expect(pastTheEdgeOf(tile)).toEqual([])
    expect(document.documentElement.scrollWidth).toBeLessThanOrEqual(
      window.innerWidth,
    )
  })
})

describe('the Title box opens downward while it is edited (O274)', () => {
  /** 96 characters, as long as a series title with its edition runs. */
  const LONG_TITLE =
    'Kagami no Kuni no Harisugawa: The Complete Digital Edition, Volume 01 (Collector’s Box Set, Ed.)'

  /** `el`'s content box, left to right. */
  const contentOf = (el: Element) => {
    const box = el.getBoundingClientRect()
    const style = getComputedStyle(el)
    const left = box.left + el.clientLeft
    return {
      left: left + parseFloat(style.paddingLeft),
      right: left + el.clientWidth - parseFloat(style.paddingRight),
    }
  }

  const need = <T extends Element>(host: Element, selector: string) => {
    const el = host.querySelector<T>(selector)
    if (el === null) throw new Error(`no ${selector} mounted`)
    return el
  }

  /**
   * The editor in a tile of `.library > main` in a window `width` by
   * `height`, in the `first` or the `last` of a row of `columns` tiles, with
   * one tile more on the row below. Every other tile is drawn as the Series
   * page draws a volume, its cover in a positioned box.
   */
  async function titleIn(
    width: number,
    height: number,
    columns: number,
    column: 'first' | 'last',
    title = LONG_TITLE,
  ) {
    const was = [window.innerWidth, window.innerHeight] as const
    onTestFinished(() => page.viewport(...was))
    await page.viewport(width, height)
    const editorAt = column === 'first' ? 0 : columns - 1
    const tiles = [...Array(columns + 1).keys()].map((n) =>
      n === editorAt ? (
        <li key={n}>
          <MetadataEditor
            slug="w"
            title={title}
            onPatch={vi.fn().mockResolvedValue({ title, persisted: true })}
          />
        </li>
      ) : (
        <li key={n}>
          <a href={`/read/v${n}`}>
            <span className="cover-box">
              <span className="cover-missing">cover</span>
            </span>
            <span className="title">Volume {n}</span>
          </a>
        </li>
      ),
    )
    let host: HTMLElement | undefined
    await act(async () => {
      host = mount(
        <div className="library">
          {width >= 900 && <div className="sidebar-column" />}
          <main>
            <ul className="volumes">{tiles}</ul>
          </main>
        </div>,
      )
    })
    if (host === undefined) throw new Error('nothing mounted')
    // A face that lands between two readings would move Save by itself.
    await document.fonts.ready
    const nth = (n: number) =>
      need(host as Element, `.volumes > li:nth-child(${n + 1})`)
    return {
      main: need(host, '.library > main'),
      grid: need<HTMLElement>(host, '.volumes'),
      tile: nth(editorAt),
      beside: nth(column === 'first' ? 1 : columns - 2),
      below: nth(columns),
      label: need(host, '.metadata-editor .field.title'),
      box: need<HTMLTextAreaElement>(host, ".metadata-editor [name='title']"),
      save: need<HTMLElement>(host, ".metadata-editor button[type='submit']"),
    }
  }

  /** Within `tile`'s left and right edges. */
  const withinTile = (box: Element, tile: Element) => {
    const edges = box.getBoundingClientRect()
    const within = tile.getBoundingClientRect()
    return edges.left >= within.left - 0.5 && edges.right <= within.right + 0.5
  }

  async function opensOver(
    width: number,
    height: number,
    columns: number,
    column: 'first' | 'last',
  ) {
    const { main, grid, tile, beside, below, label, box, save } = await titleIn(
      width,
      height,
      columns,
      column,
    )
    const area = contentOf(main)
    const edge = tile.getBoundingClientRect()
    const gap = parseFloat(getComputedStyle(grid).columnGap)
    const rest = box.getBoundingClientRect()
    // Inside the open box, on the tile beside its own.
    const point = {
      x:
        column === 'first'
          ? edge.right + gap + edge.width / 2
          : edge.left - gap - edge.width / 2,
      y: rest.top + rest.height / 2,
    }

    // The fixture tells, each part first: the title runs past the tile on
    // one line, the tile is in the row's first or last column, a row
    // follows, and the point lies on the next tile's positioned cover.
    expect(
      oneLineIn(label, LONG_TITLE),
      'the title on one line against its tile',
    ).toBeGreaterThan(edge.width)
    if (column === 'first') {
      expect(edge.left, 'the tile’s left edge').toBeCloseTo(area.left, 0)
    } else {
      expect(edge.right, 'the tile’s right edge').toBeCloseTo(area.right, 0)
    }
    expect(below.getBoundingClientRect().top).toBeGreaterThan(edge.bottom)
    const covered = document.elementFromPoint(point.x, point.y)
    expect(
      covered !== null &&
        beside.contains(covered) &&
        covered.closest('.cover-box') !== null,
      'the point on the cover beside the tile',
    ).toBe(true)

    expect(withinTile(box, tile), 'at rest, within its tile').toBe(true)
    const saveAt = save.getBoundingClientRect().toJSON()
    const belowAt = below.getBoundingClientRect().toJSON()

    await act(async () => {
      box.focus()
      await userEvent.click(box)
    })

    const open = box.getBoundingClientRect()
    expect(open.width, 'open, against its tile').toBeGreaterThan(
      edge.width + 0.5,
    )
    expect(
      open.width,
      'open, against two tiles and the gap between',
    ).toBeLessThanOrEqual(2 * edge.width + gap + 0.5)
    expect(box.scrollWidth, 'its title wraps').toBeLessThanOrEqual(
      box.clientWidth + 1,
    )
    expect(open.height, 'open, against one line').toBeGreaterThan(
      rest.height + 0.5,
    )
    expect(open.left, 'its left edge').toBeGreaterThanOrEqual(area.left - 0.5)
    expect(open.right, 'its right edge').toBeLessThanOrEqual(area.right + 0.5)
    expect(open.left).toBeGreaterThanOrEqual(0)
    expect(open.right).toBeLessThanOrEqual(window.innerWidth)
    expect(main.scrollWidth, 'the page area’s width').toBe(main.clientWidth)
    expect(document.documentElement.scrollWidth).toBeLessThanOrEqual(
      window.innerWidth,
    )
    if (column === 'first') {
      expect(open.left, 'lined up with its tile’s left edge').toBeCloseTo(
        edge.left,
        0,
      )
    } else {
      expect(open.right, 'lined up with its tile’s right edge').toBeCloseTo(
        edge.right,
        0,
      )
    }
    expect(save.getBoundingClientRect().toJSON(), 'Save').toEqual(saveAt)
    expect(below.getBoundingClientRect().toJSON(), 'the row below').toEqual(
      belowAt,
    )
    expect(document.elementFromPoint(point.x, point.y), 'on top').toBe(box)

    // Escape closes it and keeps the focus; typing opens it again.
    await act(async () => {
      await userEvent.keyboard('{Escape}')
    })
    expect(document.activeElement).toBe(box)
    expect(withinTile(box, tile), 'after Escape, within its tile').toBe(true)
    await act(async () => {
      await userEvent.keyboard('x')
    })
    expect(box.getBoundingClientRect().width, 'typed in again').toBeGreaterThan(
      edge.width + 0.5,
    )

    await act(async () => {
      await userEvent.tab()
    })
    expect(document.activeElement).toBe(save)
    expect(withinTile(box, tile), 'closed, within its tile').toBe(true)
  }

  it('O274: opens rightward from the first column at 1280 px', async () => {
    await opensOver(1280, 800, 7, 'first')
  })

  it('O274: opens leftward from the last column at 1280 px', async () => {
    await opensOver(1280, 800, 7, 'last')
  })

  it('O274: opens across both columns from the left one at 390 px', async () => {
    await opensOver(390, 844, 2, 'first')
  })

  it('O274: opens across both columns from the right one at 390 px', async () => {
    await opensOver(390, 844, 2, 'last')
  })

  it('O274: at rest the box is drawn as a text input is, on one line', async () => {
    const { tile, label, box, save } = await titleIn(390, 844, 2, 'first')
    const look = (el: Element) => {
      const style = getComputedStyle(el)
      const edges = el.getBoundingClientRect()
      return {
        top: edges.top,
        left: edges.left,
        height: edges.height,
        border: style.border,
        borderRadius: style.borderRadius,
        padding: style.padding,
        font: style.font,
        background: style.backgroundColor,
        saveTop: save.getBoundingClientRect().top,
      }
    }
    const input = document.createElement('input')
    input.value = LONG_TITLE
    box.style.display = 'none'
    box.after(input)
    const asInput = look(input)
    input.remove()
    box.style.display = ''

    expect(look(box)).toEqual(asInput)
    expect(box.scrollWidth, 'its title runs on past it').toBeGreaterThan(
      box.clientWidth + 1,
    )
    expect(withinTile(box, tile)).toBe(true)
    expect(label.contains(box)).toBe(true)
  })

  it('O274: past five lines the open box scrolls inside', async () => {
    const title = `${LONG_TITLE} ${LONG_TITLE} ${LONG_TITLE}`
    const { label, box } = await titleIn(1280, 800, 7, 'first', title)
    await act(async () => {
      box.focus()
      await userEvent.click(box)
    })
    const style = getComputedStyle(box)
    const line = parseFloat(style.lineHeight)
    const probe = document.createElement('div')
    probe.style.cssText = `position: absolute; visibility: hidden; width: ${box.clientWidth - parseFloat(style.paddingLeft) - parseFloat(style.paddingRight)}px; white-space: pre-wrap; overflow-wrap: break-word`
    probe.textContent = title
    label.append(probe)
    const lines = probe.getBoundingClientRect().height / line
    probe.remove()

    // The fixture tells: open, the title takes more than five lines.
    expect(lines).toBeGreaterThan(5)
    expect(
      box.clientHeight -
        parseFloat(style.paddingTop) -
        parseFloat(style.paddingBottom),
      'five lines',
    ).toBe(5 * line)
    expect(box.scrollHeight, 'scrolls inside').toBeGreaterThan(box.clientHeight)
    expect(style.overflowY, 'scrolled by a wheel or a finger').toBe('auto')
  })

  it('O274: a pasted line break is a space, and the caret is left after the paste', async () => {
    const { box } = await titleIn(390, 844, 2, 'first', 'Kagami Volume 1')
    const source = document.createElement('textarea')
    source.value = 'no\nKuni '
    document.body.append(source)
    onTestFinished(() => source.remove())
    source.select()
    await userEvent.copy()

    await act(async () => {
      box.focus()
      box.setSelectionRange(7, 7)
      await userEvent.paste()
    })

    expect({ value: box.value, caret: box.selectionStart }).toEqual({
      value: 'Kagami no Kuni Volume 1',
      caret: 'Kagami no Kuni '.length,
    })
  })

  it('O274: the open box is laid again when the window’s width changes', async () => {
    const { main, tile, box } = await titleIn(390, 844, 2, 'last')
    await act(async () => {
      box.focus()
      await userEvent.click(box)
    })
    expect(box.getBoundingClientRect().right).toBeCloseTo(
      tile.getBoundingClientRect().right,
      0,
    )

    await page.viewport(600, 844)
    await new Promise(requestAnimationFrame)

    // The fixture tells: wider, the tile is no longer the row's last.
    const area = contentOf(main)
    const edge = tile.getBoundingClientRect()
    expect(edge.right).toBeLessThan(area.right - 1)
    const open = box.getBoundingClientRect()
    expect(open.left, 'lined up with its tile’s left edge').toBeCloseTo(
      edge.left,
      0,
    )
    expect(open.right).toBeLessThanOrEqual(area.right + 0.5)
    expect(open.width).toBeGreaterThan(edge.width + 0.5)
  })

  it('O274: the open box is laid again when the page area’s width changes and the window’s does not', async () => {
    const { main, grid, tile, box } = await titleIn(1280, 800, 7, 'last')
    await act(async () => {
      box.focus()
      await userEvent.click(box)
    })
    expect(box.getBoundingClientRect().right).toBeCloseTo(
      tile.getBoundingClientRect().right,
      0,
    )

    // A wider sidebar column takes the width from the page area alone.
    const library = main.closest<HTMLElement>('.library')
    if (library === null) throw new Error('no .library mounted')
    library.style.gridTemplateColumns = '30rem 1fr'
    // One frame for the layout, and the next for what follows from it.
    await new Promise(requestAnimationFrame)
    await new Promise(requestAnimationFrame)

    // The fixture tells: the window is as it was, and in the narrower area
    // the tile is no longer the row's last.
    expect(window.innerWidth).toBe(1280)
    const area = contentOf(main)
    const edge = tile.getBoundingClientRect()
    expect(edge.right).toBeLessThan(area.right - 1)
    const gap = parseFloat(getComputedStyle(grid).columnGap)
    const open = box.getBoundingClientRect()
    expect(open.left, 'lined up with its tile’s left edge').toBeCloseTo(
      edge.left,
      0,
    )
    expect(open.width, 'two of its tiles and the gap').toBeCloseTo(
      2 * edge.width + gap,
      0,
    )
    expect(open.left).toBeGreaterThanOrEqual(area.left - 0.5)
    expect(open.right).toBeLessThanOrEqual(area.right + 0.5)
  })

  it('O274: the open box keeps inside the page area when its tile and the area’s edges disagree by half a pixel', async () => {
    const { main, grid, tile, box } = await titleIn(390, 844, 2, 'first')
    // The grid half a pixel right of the area's edges, standing for any
    // sub-pixel disagreement between a tile's left and the area's start.
    grid.style.marginLeft = '0.5px'
    grid.style.marginRight = '-0.5px'
    const area = contentOf(main)
    const edge = tile.getBoundingClientRect()
    const gap = parseFloat(getComputedStyle(grid).columnGap)
    const wide = Math.min(2 * edge.width + gap, area.right - area.left)

    // The fixture tells: from the first column's left edge the open width
    // runs past the area's right edge, and back from its right edge, a
    // whole tile past the area's left.
    expect(edge.left, 'the tile’s left edge').toBeGreaterThan(area.left)
    expect(edge.left + wide, 'opened rightward').toBeGreaterThan(area.right)
    expect(edge.right - wide, 'opened leftward').toBeLessThan(
      area.left - edge.width,
    )

    await act(async () => {
      box.focus()
      await userEvent.click(box)
    })

    const open = box.getBoundingClientRect()
    expect(open.width).toBeCloseTo(wide, 0)
    expect(open.left, 'its left edge').toBeGreaterThanOrEqual(area.left - 0.5)
    expect(open.right, 'its right edge').toBeLessThanOrEqual(area.right + 0.5)
  })
})
