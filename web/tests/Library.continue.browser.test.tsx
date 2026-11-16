import { act } from 'react'
import { beforeEach, describe, expect, it, onTestFinished, vi } from 'vitest'
import { Link, MemoryRouter, Route, Routes, useParams } from 'react-router'
import { cdp, page, userEvent } from 'vitest/browser'
import * as aboutApi from '../src/about'
import { pxOf } from '../src/continueRow'
// **Required**, the same reason `Config.browser.test.tsx` gives.
import '../src/index.css'
import * as library from '../src/library'
import { Library } from '../src/Library'
import { homeRead } from '../src/pageReads'
import { invalidate } from '../src/reads'
import { Shell } from '../src/Shell'
import { ABOUT } from './support/about'
import { settleUntil } from './support/async'
import { columnsOf } from './support/columns'
import {
  gridColumns,
  inProgress,
  offGrid,
  seriesCard,
} from './support/continueRow'
import { BROWSE_KEY, homePage } from './support/homePage'
import { mount } from './support/mount'
import { mouse } from './support/mouse'
import { coarseRulesOn } from './support/pointer'
import { rootFontAt } from './support/rootFont'

/**
 * The home page's Continue row (spec O288): over the grid's columns (item 2,
 * O285's two layouts), from 900 px up its cards the cover size at the grid's
 * spread gap, below it the grid's filled column at the least gap, also where
 * the row overflows, from its first paint, across the breakpoint, and with
 * no render of `Library` for a resize; its fade (item 3); its heading level
 * with the sidebar's and the line under it (items 1, 4). Every expectation
 * is read from the page as drawn. jsdom lays nothing out.
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

/** Enough series for two full rows at every size and width here. */
const SERIES = 20

beforeEach(() => {
  vi.clearAllMocks()
  vi.mocked(aboutApi.about).mockResolvedValue(ABOUT)
  vi.mocked(library.status).mockResolvedValue({
    libraries: [{ id: 1, name: 'Volumes', scanned: true }],
    restricted: false,
  })
  vi.mocked(library.grid).mockResolvedValue({
    series: Array.from({ length: SERIES }, (_, i) => seriesCard(i + 1)),
    unfiled: [],
  })
})

function need<T extends Element = HTMLElement>(
  scope: ParentNode,
  selector: string,
): T {
  const found = scope.querySelector<T>(selector)
  if (!found) throw new Error(`no ${selector}`)
  return found
}

/**
 * The home page at `width` px, the cover size `size`, `entries` volumes in
 * progress, once the row's cards and the grid's tiles are all drawn.
 */
async function drawn({
  width,
  size,
  entries,
  onCommit,
}: {
  width: number
  size: number
  entries: number
  onCommit?: () => void
}) {
  vi.mocked(library.home).mockResolvedValue({
    continuing: inProgress(entries),
    added: [],
    read: [],
  })
  const host = await homePage({ width, coverSize: size, onCommit })
  await settleUntil(
    'the Continue row and the grid',
    () =>
      host.querySelectorAll('.continue ul > li').length === entries &&
      host.querySelectorAll('main .grid > li').length === SERIES,
    2000,
  )
  await document.fonts.ready
  return { host, row: need(host, '.continue ul') }
}

/**
 * Where a card leads: the reader's route, naming the volume it opens.
 * Declared inside a function for the reason `Library.test.tsx`'s
 * `locationProbe()` gives.
 */
function opened() {
  function Opened() {
    return <p data-testid="opened">{useParams().slug}</p>
  }
  return <Opened />
}

/**
 * The home page at `width` px in the shell, with a way away from it and
 * back (two hidden links) and the reader's route, once its row and grid are
 * drawn.
 */
async function returnable(width: number) {
  localStorage.removeItem(BROWSE_KEY)
  onTestFinished(() => localStorage.removeItem(BROWSE_KEY))
  const was = [window.innerWidth, window.innerHeight] as const
  onTestFinished(() => page.viewport(...was))
  await page.viewport(width, 844)
  let host: HTMLElement | undefined
  await act(async () => {
    host = mount(
      <MemoryRouter initialEntries={['/']}>
        <nav hidden>
          <Link to="/away">Away</Link>
          <Link to="/">Home</Link>
        </nav>
        <Routes>
          <Route
            path="/"
            element={
              <Shell>
                <Library />
              </Shell>
            }
          />
          <Route path="/away" element={<p>Away</p>} />
          <Route path="/read/:slug" element={opened()} />
        </Routes>
      </MemoryRouter>,
    )
    // `homePage`'s height chain, for the same reason.
    host.style.height = '100%'
  })
  // `homePage`'s wait for the search bar's debounce, for the same reason.
  await act(async () => {
    await new Promise((resolve) => setTimeout(resolve, 300))
  })
  if (host === undefined) throw new Error('nothing mounted')
  const drawnHost = host
  await settleUntil(
    'the Continue row and the grid',
    () =>
      drawnHost.querySelectorAll('.continue ul > li').length > 0 &&
      drawnHost.querySelectorAll('main .grid > li').length === SERIES,
    2000,
  )
  return host
}

/** Which of its two properties the row's inline style holds. */
const held = (row: HTMLElement) => ({
  gap: row.style.getPropertyValue('--continue-gap') !== '',
  column: row.style.getPropertyValue('--continue-col') !== '',
})

/** `actual` where it lies more than 0.5 px from `expected`, else `expected`. */
const near = (actual: number, expected: number) =>
  Math.abs(actual - expected) <= 0.5 ? expected : actual

const POLL = { timeout: 3000 }

/**
 * The `ResizeObserver` loop errors the page hears until the test ends: a
 * size set in an observer's callback that it could not report in its frame.
 */
function loopErrors(): string[] {
  const loops: string[] = []
  const heard = (event: ErrorEvent) => {
    if (event.message.includes('ResizeObserver loop')) loops.push(event.message)
  }
  window.addEventListener('error', heard)
  onTestFinished(() => window.removeEventListener('error', heard))
  return loops
}

/** Lets `count` frames be drawn. */
async function frames(count: number) {
  for (let i = 0; i < count; i += 1) {
    await new Promise(requestAnimationFrame)
  }
}

/** `count` widths evenly from `from` to `to`, rounded, `from` left out. */
const ramp = (from: number, to: number, count: number) =>
  Array.from({ length: count }, (_, i) =>
    Math.round(from + ((to - from) * (i + 1)) / count),
  )

/**
 * Drags the window through `widths` as a drag's sizes arrive: each is set
 * on the test frame's box from the page around it, in a task between two
 * frames, so each frame lays a new width. `page.viewport` waits for a
 * frame before it answers, so it cannot pace a width a frame. Gives the
 * width each frame was drawn at, from the first to the one that drew the
 * last.
 */
async function dragged(widths: readonly number[]): Promise<number[]> {
  const box = window.frameElement?.parentElement
  if (!box) throw new Error('this file is not running in a frame')
  const outer = window.parent
  const seen: number[] = []
  let done = false
  const watch = () => {
    if (done) return
    seen.push(window.innerWidth)
    requestAnimationFrame(watch)
  }
  requestAnimationFrame(watch)
  await new Promise<void>((resolve) => {
    let i = 0
    const step = () => {
      if (i === widths.length) return resolve()
      const width = widths[i]
      i += 1
      outer.setTimeout(() => {
        box.style.width = `${width}px`
        requestAnimationFrame(step)
      }, 0)
    }
    requestAnimationFrame(step)
  })
  done = true
  return seen
}

/**
 * Scrolls `row` to `at`, its end or a card at its start edge, checks it
 * stands there, and lets a mark record the place. `off` is how far the row
 * lies from that place now. `stepped` is whether its cards step as the
 * grid's columns do, read from the cards' own edges since the row stands
 * scrolled: the row is laid for the grid as drawn.
 */
async function standAt(
  host: HTMLElement,
  row: HTMLElement,
  at: Element | 'its end',
) {
  const list = getComputedStyle(row)
  const start = () =>
    row.getBoundingClientRect().left +
    pxOf(list.borderLeftWidth) +
    pxOf(list.paddingLeft)
  const travel = () => row.scrollWidth - row.clientWidth
  row.scrollLeft =
    at === 'its end' ? travel() : at.getBoundingClientRect().left - start()
  // The fixture tells: the row stands where the case names, with cards
  // past its end only in the middle.
  expect({
    before: row.scrollLeft > 1,
    after: row.scrollLeft < travel() - 1,
  }).toEqual({ before: true, after: at !== 'its end' })
  // A mark records the row's place, at most once a frame.
  await frames(2)
  return {
    travel,
    off: () =>
      at === 'its end'
        ? travel() - row.scrollLeft
        : at.getBoundingClientRect().left - start(),
    stepped: () => {
      const [one, two] = gridColumns(host)
      const [a, b] = [...row.children].map((li) => li.getBoundingClientRect())
      return Math.abs(b.left - a.left - (two.left - one.left)) <= 0.5
    },
  }
}

/**
 * Real keys on the cover-size slider in the View panel, which this opens
 * where it is closed: `keys` such as `'{ArrowLeft}'.repeat(24)`, an arrow
 * moving one position.
 */
async function slide(host: HTMLElement, keys: string) {
  if (need(host, '.view-panel').hidden) {
    await act(async () => {
      await userEvent.click(need(host, '.view-button'))
    })
  }
  const range = need<HTMLInputElement>(
    host,
    '.view-panel .field-slider-track input[type="range"]',
  )
  // Inside the frame first: a key pressed before anything here has focus
  // goes to the frame around the test.
  await act(async () => range.focus())
  await act(async () => {
    await userEvent.keyboard(keys)
  })
}

/**
 * The row's two marks and its mask as drawn: `none`, or each side's fading
 * layer as its stops, read from the computed `mask-image`, each a length in
 * px and the alpha there. A colour hint has no colour of its own: the alpha
 * there is halfway between its neighbours', as a hint places the midpoint.
 * A side's layer runs from that side's edge (`to right` from the left, the
 * side before); a side that does not fade is a plain layer with no lengths.
 */
function fade(row: HTMLElement) {
  const image = getComputedStyle(row).maskImage
  const sides: Record<string, [number, number][]> = {}
  for (const layer of image.split(/,\s*(?=linear-gradient)/)) {
    const inside = layer.slice(layer.indexOf('(') + 1, layer.lastIndexOf(')'))
    // At the commas between arguments, not those inside a colour.
    const stops = inside.split(/,\s*(?![^(]*\))/).flatMap((arg) => {
      const at = /([\d.]+)px$/.exec(arg.trim())
      if (at === null) return []
      const colour = arg.trim().slice(0, at.index).trim()
      return [
        { px: Number(at[1]), alpha: colour === '' ? null : alphaOf(colour) },
      ]
    })
    if (stops.length === 0) continue
    const side = layer.includes('to right')
      ? 'before'
      : layer.includes('to left')
        ? 'after'
        : layer
    sides[side] = stops.map(({ px, alpha }, i) => [
      px,
      alpha ??
        ((stops[i - 1]?.alpha ?? NaN) + (stops[i + 1]?.alpha ?? NaN)) / 2,
    ])
  }
  return {
    before: row.hasAttribute('data-more-before'),
    after: row.hasAttribute('data-more-after'),
    mask: image === 'none' ? 'none' : sides,
  }
}

/** A computed `rgb()` or `rgba()` colour's alpha. */
function alphaOf(colour: string): number {
  const parts = /rgba?\(([^)]*)\)/.exec(colour)?.[1].split(/[\s,/]+/)
  if (parts === undefined) throw new Error(`no alpha in ${colour}`)
  return parts.length > 3 ? Number(parts[3]) : 1
}

/**
 * Clear for 12 px at the edge, half see-through at 50 px, solid at 80 px:
 * each stop's length and alpha.
 */
const STOPS = [
  [12, 0],
  [50, 0.5],
  [80, 1],
]
const NONE = { before: false, after: false, mask: 'none' }
const AFTER = { before: false, after: true, mask: { after: STOPS } }
const BOTH = {
  before: true,
  after: true,
  mask: { before: STOPS, after: STOPS },
}
const BEFORE = { before: true, after: false, mask: { before: STOPS } }

describe('the Continue row over the grid (O288 item 2)', () => {
  // [window width, the size stored, cards in the row]
  it.each([
    [1280, 144, 1],
    [1280, 144, 12],
    [1280, 96, 20],
    [1200, 240, 6],
    [1024, 151, 12],
    [899, 144, 12],
    [390, 104, 6],
    [390, 144, 6],
    [390, 240, 3],
    [260, 240, 3],
  ] as const)(
    'lays its cards over the grid’s columns and as wide as its tiles at %i px, %i px covers, %i cards',
    async (width, size, entries) => {
      const { host, row } = await drawn({ width, size, entries })
      const columns = gridColumns(host)
      const grid = need(host, 'main .grid')
      const least = Number.parseFloat(getComputedStyle(grid).columnGap)
      const spread =
        columns.length > 1 ? columns[1].left - columns[0].right : least
      const exact =
        getComputedStyle(grid).getPropertyValue('--tile-fill').trim() === '0'
      // The fixture tells: a row with more cards than the grid has columns
      // overflows, and only then; and where the covers are exact, the grid
      // spreads its columns wider than the least gap, so a row laid at the
      // least gap would miss them.
      expect({
        overflows: row.scrollWidth > row.clientWidth,
        room: !exact || spread - least > 0.5,
      }).toEqual({ overflows: entries > columns.length, room: true })
      row.scrollLeft = row.scrollWidth
      row.scrollLeft = 0
      expect(offGrid(host)).toEqual([])
    },
  )

  it('switches layout across 900 px, leaving nothing of the other set, rendering nothing', async () => {
    let commits = 0
    const { host, row } = await drawn({
      width: 1280,
      size: 144,
      entries: 12,
      onCommit: () => {
        commits += 1
      },
    })
    await expect.poll(() => offGrid(host), POLL).toEqual([])
    const states = [held(row)]
    const loops = loopErrors()
    // The Profiler counts: the mount's commits are in it.
    const mounted = commits
    commits = 0
    for (const width of [899, 1280, 899]) {
      await page.viewport(width, 844)
      await expect.poll(() => offGrid(host), POLL).toEqual([])
      states.push(held(row))
    }
    await frames(2)
    const exact = { gap: true, column: false }
    const filled = { gap: false, column: true }
    expect({ mounted: mounted > 0, states, commits, loops }).toEqual({
      mounted: true,
      states: [exact, filled, exact, filled],
      commits: 0,
      loops: [],
    })
  })

  it.each([
    [1280, 1100],
    [899, 600],
  ])(
    'follows a live resize from %i px to %i px, rendering nothing',
    async (from, to) => {
      let commits = 0
      const { host } = await drawn({
        width: from,
        size: 144,
        entries: 12,
        onCommit: () => {
          commits += 1
        },
      })
      await expect.poll(() => offGrid(host), POLL).toEqual([])
      const stepAt = () => {
        const [first, second] = gridColumns(host)
        return second.left - first.left
      }
      const before = stepAt()
      const loops = loopErrors()
      // The Profiler counts: the mount's commits are in it.
      const mounted = commits
      commits = 0
      await page.viewport(to, 844)
      await expect.poll(() => offGrid(host), POLL).toEqual([])
      await frames(2)
      // The fixture tells: the grid's columns moved, so a gap or a column
      // left from the first width would miss them.
      expect({
        mounted: mounted > 0,
        moved: Math.abs(stepAt() - before) > 1,
        commits,
        loops,
      }).toEqual({ mounted: true, moved: true, commits: 0, loops: [] })
    },
  )

  it('follows a slider change', async () => {
    const { host } = await drawn({ width: 1280, size: 144, entries: 12 })
    const stepAt = () => {
      const [first, second] = gridColumns(host)
      return second.left - first.left
    }
    const before = stepAt()
    await slide(host, '{ArrowRight}'.repeat(10))
    // Ten 2 px positions up from 144.
    await expect
      .poll(() => gridColumns(host)[0]?.width, POLL)
      .toBeCloseTo(164, 1)
    await expect.poll(() => offGrid(host), POLL).toEqual([])
    // The fixture tells: the columns' step changed with the size.
    expect(Math.abs(stepAt() - before)).toBeGreaterThan(1)
  })

  it.each(['its end', 'the card at its start edge'] as const)(
    'keeps %s across cover-size changes, up and down, also where the grid’s columns change',
    async (place) => {
      const { host, row } = await drawn({ width: 1280, size: 131, entries: 20 })
      // The seventh card, the last in view before the row scrolls: its
      // edge lies a whole number of px along at this size and width.
      const { off, stepped } = await standAt(
        host,
        row,
        place === 'its end' ? place : row.children[6],
      )
      const seen = [{ columns: gridColumns(host).length, off: near(off(), 0) }]
      // [the keys, the size they reach]: up to a column fewer, down to a
      // column more, then a long way up and a long way down, a change a key.
      const changes = [
        ['{ArrowRight}', 132],
        ['{ArrowLeft}', 131],
        ['{ArrowRight}'.repeat(23), 164],
        ['{ArrowLeft}'.repeat(34), 120],
      ] as const
      for (const [keys, size] of changes) {
        await slide(host, keys)
        await expect
          .poll(() => gridColumns(host)[0]?.width, POLL)
          .toBeCloseTo(size, 1)
        // Laid for the size: the cards step as the grid's columns do, read
        // from the cards' own edges, since the row stands scrolled.
        await expect.poll(stepped, POLL).toBe(true)
        seen.push({ columns: gridColumns(host).length, off: near(off(), 0) })
      }
      // The columns are the fixture's: each change moves the grid's.
      expect(seen).toEqual(
        [7, 6, 7, 5, 7].map((columns) => ({ columns, off: 0 })),
      )
    },
  )

  it('keeps its end through a size at which every card fits', async () => {
    const { host, row } = await drawn({ width: 1280, size: 144, entries: 7 })
    const travel = () => row.scrollWidth - row.clientWidth
    row.scrollLeft = travel()
    await frames(2)
    // Thirteen 1 px positions down from 144, where seven columns take the
    // seven cards, and back.
    await slide(host, '{ArrowLeft}'.repeat(13))
    await expect
      .poll(() => gridColumns(host)[0]?.width, POLL)
      .toBeCloseTo(131, 1)
    const fits = travel() <= 1
    await slide(host, '{ArrowRight}'.repeat(13))
    await expect
      .poll(() => gridColumns(host)[0]?.width, POLL)
      .toBeCloseTo(144, 1)
    // The fixture tells: the row fitted at the smaller size and overflows
    // again at this one.
    expect({
      fits,
      overflows: travel() > 1,
      end: near(travel() - row.scrollLeft, 0),
    }).toEqual({ fits: true, overflows: true, end: 0 })
  })

  it.each(['its end', 'the card at its start edge'] as const)(
    'keeps %s across window resizes, a phone turning and back, also across 900 px',
    async (place) => {
      const { host, row } = await drawn({ width: 390, size: 144, entries: 12 })
      // The third card, the first past the two in view at this width.
      const { off, stepped } = await standAt(
        host,
        row,
        place === 'its end' ? place : row.children[2],
      )
      const seen = [{ columns: gridColumns(host).length, off: near(off(), 0) }]
      // The phone turned, a window with the sidebar, one just under 900 px
      // without it, and the phone turned back.
      for (const width of [844, 1280, 899, 390]) {
        await page.viewport(width, 844)
        await expect.poll(() => window.innerWidth, POLL).toBe(width)
        // Laid for the width.
        await expect.poll(stepped, POLL).toBe(true)
        seen.push({ columns: gridColumns(host).length, off: near(off(), 0) })
      }
      // The columns are the fixture's: each width moves the grid's.
      expect(seen).toEqual(
        [2, 5, 6, 5, 2].map((columns) => ({ columns, off: 0 })),
      )
    },
  )

  it('does not creep over a window drag', async () => {
    const { host, row } = await drawn({ width: 390, size: 144, entries: 12 })
    const { off, stepped } = await standAt(host, row, row.children[2])
    // From the phone's width to its width turned, a new width each frame.
    const widths = ramp(390, 844, 20)
    const seen = await dragged(widths)
    await expect.poll(stepped, POLL).toBe(true)
    // The fixture tells: each frame drew the next width, and the drag moved
    // the grid's columns.
    expect({
      seen,
      columns: gridColumns(host).length,
      off: near(off(), 0),
    }).toEqual({ seen: [390, ...widths], columns: 5, off: 0 })
  })

  it('keeps a card near the row’s end at its start edge over a window drag out past the row’s reach and back', async () => {
    const { host, row } = await drawn({ width: 390, size: 144, entries: 12 })
    // The tenth card, which the row's end stops short of at 844 px.
    const { off, stepped, travel } = await standAt(host, row, row.children[9])
    const out = ramp(390, 844, 20)
    const seenOut = await dragged(out)
    await expect.poll(stepped, POLL).toBe(true)
    const short = { end: near(travel() - row.scrollLeft, 0), off: off() > 1 }
    const back = ramp(844, 390, 20)
    const seenBack = await dragged(back)
    await expect.poll(stepped, POLL).toBe(true)
    // The fixture tells: each frame drew the next width, and at the widest
    // the row stood at its end with the card still short of its start edge.
    expect({
      seen: [...seenOut, ...seenBack],
      short,
      off: near(off(), 0),
    }).toEqual({
      seen: [390, ...out, 844, ...back],
      short: { end: 0, off: true },
      off: 0,
    })
  })

  it('keeps a card near the row’s end at its start edge over a window drag across 900 px', async () => {
    const { host, row } = await drawn({ width: 1000, size: 144, entries: 12 })
    // The eighth card, which the row brings to its start edge short of its
    // end.
    const { off, stepped } = await standAt(host, row, row.children[7])
    const layouts = [held(row)]
    const widths = ramp(1000, 800, 20)
    const seen = await dragged(widths)
    await expect.poll(stepped, POLL).toBe(true)
    layouts.push(held(row))
    // The fixture tells: each frame drew the next width, and the layout
    // turned as the sidebar's column went.
    expect({ seen, layouts, off: near(off(), 0) }).toEqual({
      seen: [1000, ...widths],
      layouts: [
        { gap: true, column: false },
        { gap: false, column: true },
      ],
      off: 0,
    })
  })

  it('keeps its end through a window at which every card fits', async () => {
    const { host, row } = await drawn({ width: 1280, size: 144, entries: 7 })
    const { off, stepped, travel } = await standAt(host, row, 'its end')
    // Seven columns at this width, one for each card, and back to six.
    await page.viewport(1400, 844)
    await expect.poll(() => gridColumns(host).length, POLL).toBe(7)
    await expect.poll(stepped, POLL).toBe(true)
    const fits = travel() <= 1
    await page.viewport(1280, 844)
    await expect.poll(() => gridColumns(host).length, POLL).toBe(6)
    await expect.poll(stepped, POLL).toBe(true)
    // The fixture tells: the row fitted at the wider window and overflows
    // again at this one.
    expect({
      fits,
      overflows: travel() > 1,
      end: near(off(), 0),
    }).toEqual({ fits: true, overflows: true, end: 0 })
  })

  it('holds at a 20 px root', async () => {
    rootFontAt('20px')
    const { host } = await drawn({ width: 1290, size: 144, entries: 12 })
    const { width: W, gap, n } = columnsOf(need(host, 'main .grid'), 144)
    // The fixture tells: the least gap is 1rem at this root, and a 16 px
    // gap would count another column.
    expect({
      gap,
      moreAt16: Math.floor((W + 16) / (144 + 16)) > n,
    }).toEqual({ gap: 20, moreAt16: true })
    expect(offGrid(host)).toEqual([])
  })

  it.each([1280, 390])(
    'has the gap or column in place when a return to the page first draws the row at %i px',
    async (width) => {
      vi.mocked(library.home).mockResolvedValue({
        continuing: inProgress(12),
        added: [],
        read: [],
      })
      const host = await returnable(width)
      // Pressed as a script would, so that where the link lies does not
      // matter: the router handles the click the same way.
      await act(async () => need(host, 'a[href="/away"]').click())
      await expect.poll(() => host.querySelector('main')).toBeNull()
      const name = width >= 900 ? '--continue-gap' : '--continue-col'
      // What the row held as it was first inserted: a mutation record is
      // delivered after the commit that made it and before the next task.
      let first: string | undefined
      const watch = new MutationObserver(() => {
        const ul = host.querySelector<HTMLElement>('.continue ul')
        if (ul !== null && first === undefined) {
          first = ul.style.getPropertyValue(name)
        }
      })
      watch.observe(host, { childList: true, subtree: true })
      onTestFinished(() => watch.disconnect())
      // As the app runs: the router commits the return in a transition, and
      // the page kept from the first visit draws the row in that commit,
      // whose passive effects `act` would flush before it ended.
      globalThis.IS_REACT_ACT_ENVIRONMENT = false
      try {
        need(host, 'a[href="/"]').click()
        const until = performance.now() + 2000
        while (
          (first === undefined ||
            host.querySelectorAll('main .grid > li').length < SERIES) &&
          performance.now() < until
        ) {
          await new Promise((resolve) => setTimeout(resolve, 20))
        }
      } finally {
        globalThis.IS_REACT_ACT_ENVIRONMENT = true
      }
      const [one, two] = gridColumns(host)
      const expected = width >= 900 ? two.left - one.right : one.width
      expect(near(pxOf(first ?? ''), expected)).toBe(expected)
    },
  )

  it.each([1280, 390])(
    'falls back to the least gap when the gap is not a px length at %i px',
    async (width) => {
      unregister('--tile-gap')
      const { row } = await drawn({ width, size: 144, entries: 12 })
      const style = getComputedStyle(row)
      // The fixture tells: the token is the text it was written as.
      expect(pxOf(style.getPropertyValue('--tile-gap'))).toBeNaN()
      const widths = [...row.children].map(
        (card) => card.getBoundingClientRect().width,
      )
      expect({
        gap: row.style.getPropertyValue('--continue-gap'),
        column: row.style.getPropertyValue('--continue-col'),
        columnGap: style.columnGap,
        widths: widths.map((one) => near(one, 144)),
      }).toEqual({
        gap: '',
        column: '',
        columnGap: '16px',
        widths: widths.map(() => 144),
      })
    },
  )
})

describe('the Continue row’s fade (O288 item 3)', () => {
  it('fades the sides it can still scroll to, rendering nothing', async () => {
    let commits = 0
    const { row } = await drawn({
      width: 1280,
      size: 144,
      entries: 20,
      onCommit: () => {
        commits += 1
      },
    })
    await expect.poll(() => fade(row), POLL).toEqual(AFTER)
    // The Profiler counts: the mount's commits are in it.
    const mounted = commits
    commits = 0
    const travel = row.scrollWidth - row.clientWidth
    for (let step = 1; step <= 10; step += 1) {
      row.scrollLeft = (travel * step) / 10
      await frames(2)
      if (step === 5) await expect.poll(() => fade(row), POLL).toEqual(BOTH)
    }
    await expect.poll(() => fade(row), POLL).toEqual(BEFORE)
    await frames(2)
    expect({ mounted: mounted > 0, commits }).toEqual({
      mounted: true,
      commits: 0,
    })
  })

  it('does not fade a row that fits', async () => {
    const { row } = await drawn({ width: 1280, size: 144, entries: 1 })
    await frames(2)
    // The fixture tells: the one card leaves the row nowhere to scroll.
    expect({
      fits: row.scrollWidth <= row.clientWidth,
      fade: fade(row),
    }).toEqual({ fits: true, fade: NONE })
  })

  it('stops fading when a wider window lets every card fit', async () => {
    const { host, row } = await drawn({ width: 1280, size: 144, entries: 7 })
    await expect.poll(() => fade(row), POLL).toEqual(AFTER)
    // The fixture tells: six columns here, one card fewer than the row has.
    expect(gridColumns(host).length).toBe(6)
    await page.viewport(1400, 844)
    // And seven at this width, one for each card.
    await expect.poll(() => gridColumns(host).length, POLL).toBe(7)
    await expect.poll(() => fade(row), POLL).toEqual(NONE)
  })

  it('stops fading when a smaller size lets every card fit', async () => {
    const { host, row } = await drawn({ width: 1280, size: 144, entries: 7 })
    // To the last card and back first, so the mark the change has to clear
    // is one a scroll set, whatever set the first.
    row.scrollLeft = row.scrollWidth
    await expect.poll(() => fade(row), POLL).toEqual(BEFORE)
    row.scrollLeft = 0
    await expect.poll(() => fade(row), POLL).toEqual(AFTER)
    await slide(host, '{ArrowLeft}'.repeat(48))
    // Forty-eight 1 px positions down from 144, the least size.
    await expect
      .poll(() => gridColumns(host)[0]?.width, POLL)
      .toBeCloseTo(96, 1)
    // The fixture tells: more columns than cards at this size.
    expect(gridColumns(host).length).toBeGreaterThan(7)
    await expect.poll(() => fade(row), POLL).toEqual(NONE)
  })

  it('follows a slider change at any scroll position, its sides and its columns', async () => {
    const { host, row } = await drawn({ width: 1280, size: 144, entries: 20 })
    const travel = () => row.scrollWidth - row.clientWidth
    const columns = [gridColumns(host).length]
    row.scrollLeft = travel() / 2
    await expect.poll(() => fade(row), POLL).toEqual(BOTH)
    await slide(host, '{ArrowLeft}'.repeat(13))
    // Thirteen 1 px positions down from 144.
    await expect
      .poll(() => gridColumns(host)[0]?.width, POLL)
      .toBeCloseTo(131, 1)
    await expect.poll(() => fade(row), POLL).toEqual(BOTH)
    columns.push(gridColumns(host).length)
    // The fixture tells: the row still has travel on both sides of where it
    // stands.
    const half = row.scrollLeft
    expect(half > 1 && half < travel() - 1).toBe(true)
    row.scrollLeft = 0
    await expect.poll(() => offGrid(host), POLL).toEqual([])

    // From the end, one size up, which takes a column away: the room goes
    // to fewer, wider gaps and the row grows longer under where it stood.
    // The hook puts it back at its new end ("keeps its end …", above), so
    // the side before is still the only one that fades, where a row left
    // standing would have cards past its end and fade on both sides.
    row.scrollLeft = travel()
    await expect.poll(() => fade(row), POLL).toEqual(BEFORE)
    await slide(host, '{ArrowRight}')
    await expect
      .poll(() => gridColumns(host)[0]?.width, POLL)
      .toBeCloseTo(132, 1)
    await expect.poll(() => fade(row), POLL).toEqual(BEFORE)
    columns.push(gridColumns(host).length)
    // The fixture tells: each change moved the columns.
    expect({
      end: near(travel() - row.scrollLeft, 0),
      columns,
    }).toEqual({ end: 0, columns: [6, 7, 6] })
    row.scrollLeft = 0
    await expect.poll(() => offGrid(host), POLL).toEqual([])
  })

  it('lets a press through the fade', async () => {
    vi.mocked(library.home).mockResolvedValue({
      continuing: inProgress(20),
      added: [],
      read: [],
    })
    const host = await returnable(1280)
    await document.fonts.ready
    const row = need(host, '.continue ul')
    await expect.poll(() => fade(row), POLL).toEqual(AFTER)
    // Inside the fade's clear edge, where the card is not drawn at all.
    const x = row.getBoundingClientRect().right - 6
    const card = [...row.querySelectorAll('a')].find((link) => {
      const box = link.getBoundingClientRect()
      return box.left <= x && x <= box.right
    })
    if (card === undefined) throw new Error(`no card at ${x}`)
    const { top, height } = card.getBoundingClientRect()
    await act(async () => {
      await mouse.moveTo(x, top + height / 2)
      await mouse.press()
      await mouse.release()
    })
    await expect
      .poll(
        () => host.querySelector('[data-testid="opened"]')?.textContent,
        POLL,
      )
      .toBe('kagami-v06')
    // The fixture tells: the card there is the sixth, the last in view.
    expect(card.getAttribute('href')).toBe('/read/kagami-v06')
  })

  it('lifts the fade while a card has a focus ring', async () => {
    const { host, row } = await drawn({ width: 1280, size: 144, entries: 20 })
    await expect.poll(() => fade(row), POLL).toEqual(AFTER)
    // Inside the frame first, as `slide` says; a card is next in the order.
    await act(async () => need(host, '.view-button').focus())
    await act(async () => {
      await userEvent.keyboard('{Tab}')
    })
    const focused = document.activeElement
    expect({
      inRow: focused !== null && row.contains(focused),
      ring: focused?.matches(':focus-visible'),
      fade: fade(row),
    }).toEqual({ inRow: true, ring: true, fade: { ...AFTER, mask: 'none' } })
  })
})

/** `el`'s vertical middle, where its heading's hairline is drawn. */
function middle(el: Element) {
  const { top, height } = el.getBoundingClientRect()
  return top + height / 2
}

/** `token`'s colour as `scope` resolves it, read from a box painted in it. */
function resolved(scope: Element, token: string) {
  const probe = document.createElement('span')
  probe.style.backgroundColor = `var(${token})`
  scope.append(probe)
  try {
    return getComputedStyle(probe).backgroundColor
  } finally {
    probe.remove()
  }
}

/**
 * The home page at 1280 px, the size 144, `entries` volumes in progress and
 * the browse preferences `browse` stored, once its grid is drawn: the home
 * lists are drawn with it, so a row it does not draw stays undrawn.
 */
async function home(entries: number, browse: Record<string, unknown> = {}) {
  vi.mocked(library.home).mockResolvedValue({
    continuing: inProgress(entries),
    added: [],
    read: [],
  })
  const host = await homePage({ width: 1280, coverSize: 144, browse })
  await settleUntil(
    'the grid',
    () => host.querySelectorAll('main .grid > li').length === SERIES,
    2000,
  )
  await document.fonts.ready
  return host
}

/** Where the page head and View lie: what the Continue heading lies under. */
function above(host: HTMLElement) {
  const { top, left, width, height } = need(
    host,
    '.page-head',
  ).getBoundingClientRect()
  const view = need(host, '.view-button').getBoundingClientRect().top
  return { head: { top, left, width, height }, view }
}

/** The OS's colour-scheme preference, over the DevTools session. */
async function prefer(preference: 'light' | 'dark' | '') {
  await cdp().send('Emulation.setEmulatedMedia', {
    features: [{ name: 'prefers-color-scheme', value: preference }],
  })
}

/**
 * Each theme the app offers, as `theme.ts` applies it, and the OS's
 * preference under it: a pinned theme against the OS's, so the pin is what
 * decides.
 */
const THEMES = [
  ['System, light', 'system', 'light'],
  ['System, dark', 'system', 'dark'],
  ['Paper', 'paper', 'dark'],
  ['Ink', 'ink', 'light'],
] as const

describe('the Continue heading and the line under the row (O288 items 1, 4)', () => {
  it.each([
    ['fine', '16px'],
    ['coarse', '16px'],
    ['fine', '20px'],
  ] as const)(
    'draws the Continue rule level with the Libraries rule at 1280 px (%s pointer, %s root)',
    async (pointer, root) => {
      if (pointer === 'coarse') onTestFinished(coarseRulesOn())
      rootFontAt(root)
      const { host } = await drawn({ width: 1280, size: 144, entries: 12 })
      const heading = need(host, '.continue h2')
      const libraries = need(host, '.sidebar .sidebar-heading')
      const apart = middle(heading) - middle(libraries)
      // The fixture tells: the margin is what brings the two level, the
      // pointer's rules and the root are the ones the case names, and each
      // hairline is centred in its heading, a flex row of one line.
      expect({
        margin: Number.parseFloat(getComputedStyle(heading).marginTop) > 1,
        fingerLink: need(libraries, 'a').getBoundingClientRect().height >= 44,
        root: getComputedStyle(document.documentElement).fontSize,
        centred: [heading, libraries].map(
          (el) => getComputedStyle(el).alignItems,
        ),
        apart: Math.abs(apart) < 0.5 ? 0 : apart,
      }).toEqual({
        margin: true,
        fingerLink: pointer === 'coarse',
        root,
        centred: ['center', 'center'],
        apart: 0,
      })
    },
  )

  // Firefox, which has no project here, holds a one-line field's line box at
  // no less than `normal`, taller than the 1.5 the head row is built on;
  // forcing the search box's line taller draws that in Chromium.
  it.each([
    ['16px', '22px'],
    ['20px', '27px'],
  ] as const)(
    'keeps the head row and the Continue rule level when the search box’s line is taller than 1.5 (%s root, a %s line)',
    async (root, line) => {
      rootFontAt(root)
      const forced = document.createElement('style')
      forced.textContent = `.page-head input[type="search"] { line-height: ${line} !important }`
      document.head.append(forced)
      onTestFinished(() => forced.remove())
      const { host } = await drawn({ width: 1280, size: 144, entries: 12 })
      const head = need(host, '.page-head')
      const search = need(head, 'input[type="search"]')
      const apart =
        middle(need(host, '.continue h2')) -
        middle(need(host, '.sidebar .sidebar-heading'))
      const floor = pxOf(getComputedStyle(head).minBlockSize)
      const height = head.getBoundingClientRect().height
      // The fixture tells: the box is drawn with the forced line, and the
      // row's floor is the resolved `--head-row`.
      expect({
        line: getComputedStyle(search).lineHeight,
        floor: floor > 0,
        head: Math.abs(height - floor) < 0.05 ? floor : height,
        apart: Math.abs(apart) < 0.5 ? 0 : apart,
      }).toEqual({ line, floor: true, head: floor, apart: 0 })
    },
  )

  it.each(['fine', 'coarse'] as const)(
    'leaves the heading as it was below 900 px (%s pointer)',
    async (pointer) => {
      if (pointer === 'coarse') onTestFinished(coarseRulesOn())
      const { host } = await drawn({ width: 390, size: 144, entries: 6 })
      expect(getComputedStyle(need(host, '.continue h2')).marginTop).toBe('0px')
    },
  )

  it('moves nothing above the Continue heading', async () => {
    const host = await home(0)
    const without = above(host)
    // The fixture tells: no row at first.
    expect(host.querySelector('.continue')).toBeNull()
    // The same page, now with volumes in progress, re-read in place, as a
    // write re-reads the pages drawn.
    vi.mocked(library.home).mockResolvedValue({
      continuing: inProgress(12),
      added: [],
      read: [],
    })
    await act(async () => {
      await invalidate(homeRead(undefined))
    })
    await settleUntil(
      'the Continue row and the grid',
      () =>
        host.querySelectorAll('.continue ul > li').length === 12 &&
        host.querySelectorAll('main .grid > li').length === SERIES,
      2000,
    )
    expect(above(host)).toEqual(without)
  })

  it('draws a line across the row, 16 px under it and 16 px over the chips', async () => {
    const { host, row } = await drawn({ width: 1280, size: 144, entries: 12 })
    const section = need(host, '[data-testid="continue"]')
    const line = getComputedStyle(section, '::after')
    const box = getComputedStyle(section)
    // The line's own box, which no rect reports: up from the section's
    // bottom by what lies under the line, so a line moved inside the
    // section is read where it is drawn.
    const bottom =
      section.getBoundingClientRect().bottom -
      pxOf(box.paddingBottom) -
      pxOf(box.borderBottomWidth) -
      pxOf(line.marginBottom)
    const top = bottom - pxOf(line.height)
    const chips = need(host, '.library-chips').getBoundingClientRect().top
    const list = getComputedStyle(row)
    // The cards span the list's content box, the grid's width.
    const across =
      row.getBoundingClientRect().width -
      pxOf(list.paddingLeft) -
      pxOf(list.paddingRight)
    // The row's own `--space-2` padding under its cards makes the line's 8
    // px from the list 16 px from the cards.
    expect({
      height: line.height,
      colour: line.backgroundColor,
      underRow: near(top - row.getBoundingClientRect().bottom, 8),
      overChips: near(chips - bottom, 16),
      width: near(pxOf(line.width), across),
      grid: near(
        need(host, 'main .grid').getBoundingClientRect().width,
        across,
      ),
      padding: list.paddingBottom,
    }).toEqual({
      height: '1px',
      colour: resolved(section, '--line'),
      underRow: 8,
      overChips: 16,
      width: across,
      grid: across,
      padding: '8px',
    })
  })

  it('draws the line in each theme’s own --line', async () => {
    onTestFinished(async () => {
      document.documentElement.removeAttribute('data-theme')
      await prefer('')
    })
    const { host } = await drawn({ width: 1280, size: 144, entries: 12 })
    const section = need(host, '[data-testid="continue"]')
    const lines: Record<string, string> = {}
    const tokens: Record<string, string> = {}
    for (const [name, theme, preference] of THEMES) {
      if (theme === 'system') {
        document.documentElement.removeAttribute('data-theme')
      } else {
        document.documentElement.setAttribute('data-theme', theme)
      }
      await prefer(preference)
      // The emulation is what the System cases rest on.
      expect(matchMedia(`(prefers-color-scheme: ${preference})`).matches).toBe(
        true,
      )
      lines[name] = getComputedStyle(section, '::after').backgroundColor
      tokens[name] = resolved(section, '--line')
    }
    // The fixture tells: Paper's hairline is not Ink's, so a line in either
    // one's colour would miss the other.
    expect(tokens.Paper).not.toBe(tokens.Ink)
    expect(lines).toEqual(tokens)
  })

  it.each(['the strip turned off', 'a search', 'nothing in progress'] as const)(
    'draws no line with %s',
    async (state) => {
      if (state === 'a search') {
        // Two guards keep the row off search results, `stripDrawn`'s
        // `!searching` and the page's `searching ?`, so only a row drawn
        // past both turns this red.
        const host = await searched()
        expect(host.querySelector('[data-testid="continue"]')).toBeNull()
        return
      }
      const host = await home(
        state === 'nothing in progress' ? 0 : 12,
        state === 'the strip turned off' ? { continueStrip: false } : {},
      )
      const head = need(host, '.page-head')
      const chips = need(host, '.library-chips')
      const style = getComputedStyle(chips)
      expect({
        row: host.querySelector('[data-testid="continue"]'),
        margin: style.marginTop,
        border: style.borderTopWidth,
        headMargin: getComputedStyle(head).marginBottom,
        apart: near(
          chips.getBoundingClientRect().top -
            head.getBoundingClientRect().bottom,
          24,
        ),
      }).toEqual({
        row: null,
        margin: '24px',
        border: '0px',
        headMargin: '0px',
        apart: 24,
      })
    },
  )

  it('lays the row again once a search is cleared', async () => {
    const host = await searched()
    await act(async () => {
      await userEvent.fill(need(host, 'input[type="search"]'), '')
    })
    await settleUntil(
      'the Continue row and the grid',
      () =>
        host.querySelectorAll('.continue ul > li').length === 12 &&
        host.querySelectorAll('main .grid > li').length === SERIES,
      2000,
    )
    // The list drawn then is a new one, which the hook lays only if its
    // `drawn` turned off with the search and on again.
    await expect.poll(() => offGrid(host), POLL).toEqual([])
    expect(held(need(host, '.continue ul'))).toEqual({
      gap: true,
      column: false,
    })
  })
})

/**
 * The home page at 1280 px searched for "series", twelve volumes in
 * progress, once its two hits are drawn.
 */
async function searched() {
  vi.mocked(library.home).mockResolvedValue({
    continuing: inProgress(12),
    added: [],
    read: [],
  })
  vi.mocked(library.search).mockResolvedValue({
    query: 'series',
    series: [seriesCard(1), seriesCard(2)],
    unfiled: [],
  })
  const host = await homePage({
    width: 1280,
    coverSize: 144,
    entry: '/?q=series',
  })
  await settleUntil(
    'the hits',
    () => host.querySelectorAll('.search-hits .grid > li').length === 2,
    2000,
  )
  return host
}

/**
 * Deletes the `@property` rule registering `name` for this test, and puts
 * it back at its place when the test ends: the property then computes to
 * the text it was written as.
 */
function unregister(name: string) {
  const search = (rules: CSSRuleList): [CSSRuleList, number] | undefined => {
    for (let i = 0; i < rules.length; i += 1) {
      const rule = rules[i]
      if (rule instanceof CSSPropertyRule && rule.name === name) {
        return [rules, i]
      }
      if (rule instanceof CSSImportRule && rule.styleSheet !== null) {
        const found = search(rule.styleSheet.cssRules)
        if (found !== undefined) return found
      }
    }
    return undefined
  }
  for (const sheet of document.styleSheets) {
    const found = search(sheet.cssRules)
    if (found === undefined) continue
    const [rules, index] = found
    const text = rules[index].cssText
    const owner = rules[index].parentStyleSheet
    if (owner === null) throw new Error(`${name}'s rule has no sheet`)
    owner.deleteRule(index)
    onTestFinished(() => {
      owner.insertRule(text, index)
    })
    return
  }
  throw new Error(`no @property rule registers ${name}`)
}
