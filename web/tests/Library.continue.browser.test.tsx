import { act } from 'react'
import { beforeEach, describe, expect, it, onTestFinished, vi } from 'vitest'
import { Link, MemoryRouter, Route, Routes, useParams } from 'react-router'
import { cdp, page, userEvent } from 'vitest/browser'
import * as aboutApi from '../src/about'
import { COVER_TALL, pxOf } from '../src/continueRow'
import { SIDEBAR_LAYOUT } from '../src/phoneLayout'
// **Required**, the same reason `Config.browser.test.tsx` gives.
import '../src/index.css'
import * as library from '../src/library'
import { Library } from '../src/Library'
import { homeRead } from '../src/pageReads'
import { invalidate } from '../src/reads'
import { NEAR_PX } from '../src/reseatTiming'
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
import {
  afterMotion,
  dragged,
  eachFrame,
  frames,
  ramp,
  reducedMotion,
  scrollerOf,
  wheeledTo,
} from './support/motion'
import { mount } from './support/mount'
import { mouse } from './support/mouse'
import { coarseRulesOn } from './support/pointer'
import { rootFontAt } from './support/rootFont'

/**
 * The home page's Continue row (spec O288): over the grid's columns (item 2,
 * O285's two layouts), beside the sidebar its cards the cover size at the
 * grid's spread gap, in the phone layout the grid's filled column at the
 * least gap, also where the row overflows, from its first paint, across the
 * phone layout's edge, and with no render of `Library` for a resize; its
 * fade (item 3, eased by O303); its heading level with the sidebar's and the
 * line under it (items 1, 4). Every expectation is read from the page as
 * drawn. jsdom lays nothing out.
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
 * The home page at `width` px (by `height`, or `homePage`'s default), the
 * cover size `size`, `entries` volumes in progress, `series` tiles in the
 * grid, once the row's cards and the grid's tiles are all drawn.
 */
async function drawn({
  width,
  height,
  size,
  entries,
  series = SERIES,
  onCommit,
}: {
  width: number
  height?: number
  size: number
  entries: number
  series?: number
  onCommit?: () => void
}) {
  vi.mocked(library.home).mockResolvedValue({
    continuing: inProgress(entries),
    added: [],
    read: [],
  })
  vi.mocked(library.grid).mockResolvedValue({
    series: Array.from({ length: series }, (_, i) => seriesCard(i + 1)),
    unfiled: [],
  })
  const host = await homePage({ width, height, coverSize: size, onCommit })
  await settleUntil(
    'the Continue row and the grid',
    () =>
      host.querySelectorAll('.continue ul > li').length === entries &&
      host.querySelectorAll('main .grid > li').length === series,
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

/** Drags the window through `widths`, its height held, and gives the width each frame drew. */
async function widthsDrawn(widths: readonly number[]): Promise<number[]> {
  const seen = await dragged(widths.map((width) => ({ width })))
  return seen.map(({ width }) => width)
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
 * The row's two marks and its mask as drawn, read from the computed
 * `mask-image`: a layer a side, as the alpha at its first stop and its
 * stops, each a length in px and the alpha there. A colour hint has no
 * colour of its own: the alpha there is halfway between its neighbours', as
 * a hint places the midpoint. A side's layer runs from that side's edge (`to
 * right` from the left, the side before); a side that does not fade reads
 * alpha 1 at every stop.
 */
function fade(row: HTMLElement) {
  const image = getComputedStyle(row).maskImage
  const sides: Record<string, { alpha: number; stops: [number, number][] }> = {}
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
    const read = stops.map(({ px, alpha }, i): [number, number] => [
      px,
      alpha ??
        ((stops[i - 1]?.alpha ?? NaN) + (stops[i + 1]?.alpha ?? NaN)) / 2,
    ])
    sides[side] = { alpha: read[0][1], stops: read }
  }
  return {
    before: row.hasAttribute('data-more-before'),
    after: row.hasAttribute('data-more-after'),
    mask: sides,
  }
}

/** Each side's alpha at its first stop: 0 the full fade, 1 none. */
function edges(row: HTMLElement) {
  const { mask } = fade(row)
  return { before: mask.before?.alpha, after: mask.after?.alpha }
}

/** A computed `rgb()` or `rgba()` colour's alpha. */
function alphaOf(colour: string): number {
  const parts = /rgba?\(([^)]*)\)/.exec(colour)?.[1].split(/[\s,/]+/)
  if (parts === undefined) throw new Error(`no alpha in ${colour}`)
  return parts.length > 3 ? Number(parts[3]) : 1
}

/** Where the fade is clear, half see-through and solid, in px from its edge. */
const WIDE = [12, 50, 80] as const
/** The same, half width, in the phone layout. */
const NARROW = [6, 25, 40] as const

/** A side's layer with `alpha` at its clear edge, its stops at those lengths. */
const side = (
  alpha: number,
  [clear, half, solid]: readonly number[] = WIDE,
) => ({
  alpha,
  stops: [
    [clear, alpha],
    [half, (alpha + 1) / 2],
    [solid, 1],
  ],
})
const FADES = side(0)
const WHOLE = side(1)
const NONE = {
  before: false,
  after: false,
  mask: { before: WHOLE, after: WHOLE },
}
const AFTER = {
  before: false,
  after: true,
  mask: { before: WHOLE, after: FADES },
}
const BOTH = {
  before: true,
  after: true,
  mask: { before: FADES, after: FADES },
}
const BEFORE = {
  before: true,
  after: false,
  mask: { before: FADES, after: WHOLE },
}

/** The fade's transition, as `library.css` times it. */
const FADE_MS = 200

/**
 * Resolves as `row`'s mark `name` turns `on`, in the frame callback that sets
 * it, before that frame's style starts the transition it brings. Fails after
 * `POLL`'s timeout, so a mark that never comes fails the case with its name.
 */
function marked(row: HTMLElement, name: string, on: boolean) {
  return new Promise<void>((done, fail) => {
    const late = setTimeout(() => {
      watch.disconnect()
      fail(new Error(`${name} did not turn ${on ? 'on' : 'off'}`))
    }, POLL.timeout)
    const watch = new MutationObserver(() => {
      if (row.hasAttribute(name) !== on) return
      watch.disconnect()
      clearTimeout(late)
      done()
    })
    watch.observe(row, { attributeFilter: [name] })
  })
}

/** `alpha` where it lies strictly between no fade and the full fade. */
const between = (alpha: number | undefined) =>
  alpha !== undefined && alpha > 0 && alpha < 1 ? 'between' : alpha

/**
 * The transition of `side`'s fade just started on `row`, seeked to its
 * middle: its timing, and the side's alpha there. Seeking, not waiting, so
 * no timer stands between the start and the read; with no transition, the
 * alpha as it stands.
 */
function halfway(row: HTMLElement, side: 'before' | 'after') {
  const run = row
    .getAnimations()
    .find(
      (each) =>
        each instanceof CSSTransition &&
        each.transitionProperty === `--edge-${side}`,
    )
  const declared = run?.effect?.getTiming()
  const timing = declared && {
    duration: declared.duration,
    delay: declared.delay,
    easing: declared.easing,
  }
  if (run) run.currentTime = FADE_MS / 2
  return { run, timing, midway: between(edges(row)[side]) }
}

/** `halfway`'s timing for a fade of `FADE_MS` with no delay, eased out. */
const EASED = { duration: FADE_MS, delay: 0, easing: 'ease-out' }

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
        await afterMotion()
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
    await afterMotion()
    const fits = travel() <= 1
    await slide(host, '{ArrowRight}'.repeat(13))
    await expect
      .poll(() => gridColumns(host)[0]?.width, POLL)
      .toBeCloseTo(144, 1)
    await afterMotion()
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
        await afterMotion()
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
    const seen = await widthsDrawn(widths)
    await afterMotion()
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
    const seenOut = await widthsDrawn(out)
    await expect.poll(stepped, POLL).toBe(true)
    const short = { end: near(travel() - row.scrollLeft, 0), off: off() > 1 }
    const back = ramp(844, 390, 20)
    const seenBack = await widthsDrawn(back)
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
    const seen = await widthsDrawn(widths)
    await expect.poll(stepped, POLL).toBe(true)
    await afterMotion()
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
    await afterMotion()
    const fits = travel() <= 1
    await page.viewport(1280, 844)
    await expect.poll(() => gridColumns(host).length, POLL).toBe(6)
    await expect.poll(stepped, POLL).toBe(true)
    await afterMotion()
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
      const name = matchMedia(SIDEBAR_LAYOUT).matches
        ? '--continue-gap'
        : '--continue-col'
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
      const expected = matchMedia(SIDEBAR_LAYOUT).matches
        ? two.left - one.right
        : one.width
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

/** What a glide moves of the grid's tile `index`: its content. */
function coverOf(host: HTMLElement, index: number): Element {
  return need(host, `main .grid > li:nth-child(${index + 1}) > :first-child`)
}

/**
 * The home page at `from` px, 60 series in its grid, scrolled `down` px by
 * the wheel, through the window steps `to`, then once at rest `back`, alone
 * and under reduced motion where `still`: how far down it stood as the steps
 * began; from the frame before the first step, each frame's scroll, row gap
 * and row height, and each leg's scrolls from the frame before its own;
 * whether its covers glid; the row's box in that first frame and at rest,
 * and its first cover's and that cover's text's in that frame; where the
 * page rests. Then hides it.
 */
async function scrolledThrough({
  from,
  to,
  back = [],
  down,
  still,
}: {
  from: number
  to: number[]
  back?: number[]
  down: number
  still: boolean
}) {
  if (still) {
    onTestFinished(async () => {
      await reducedMotion(false)
    })
    await reducedMotion(true)
  }
  const { host, row } = await drawn({
    width: from,
    size: 144,
    entries: 12,
    series: 60,
  })
  await wheeledTo(host, down)
  const scroller = scrollerOf(host)
  const grid = need(host, 'main .grid')
  const cover = need(row, ':scope > li :is(img, .cover-missing)')
  // A placeholder's title; an image's is empty.
  const text = document.createRange()
  text.selectNodeContents(cover)
  const seen: {
    width: number
    scroll: number
    gap: number
    gliding: boolean
    row: DOMRect
    cover: DOMRect
    text: DOMRect
  }[] = []
  eachFrame(() => {
    seen.push({
      width: innerWidth,
      scroll: scroller.scrollTop,
      gap: pxOf(row.style.getPropertyValue('--continue-gap')),
      gliding: grid.hasAttribute('data-reseating'),
      row: row.getBoundingClientRect(),
      cover: cover.getBoundingClientRect(),
      text: text.getBoundingClientRect(),
    })
  })
  await frames(2)
  const start = scroller.scrollTop
  for (const leg of [to, back]) {
    if (leg.length === 0) continue
    await act(async () => {
      await dragged(leg.map((width) => ({ width })))
    })
    await afterMotion()
  }
  await frames(4)
  if (still) await reducedMotion(false)
  const at = seen.findIndex(({ width }) => width === to[0])
  if (at < 1) {
    const widths = seen.map(({ width }) => width).join(', ')
    throw new Error(`no frame at ${from} px then ${to[0]} px: ${widths}`)
  }
  const last = to[to.length - 1]
  const turn =
    back.length === 0
      ? seen.length + 1
      : seen.findIndex(
          ({ width }, frame) =>
            frame > at && width === back[0] && seen[frame - 1].width === last,
        )
  if (turn < 1) {
    const widths = seen.map(({ width }) => width).join(', ')
    throw new Error(`no frame at ${last} px then ${back[0]} px: ${widths}`)
  }
  const scrollsOf = (run: typeof seen) => run.map(({ scroll }) => scroll)
  const rest = scroller.scrollTop
  const after = row.getBoundingClientRect()
  // Where the list's content, and its first cover, begins under its box.
  const { borderTopWidth, paddingTop } = getComputedStyle(row)
  const padding =
    Number.parseFloat(borderTopWidth) + Number.parseFloat(paddingTop)
  // Out of the way of the next drive's page, which would lie under it.
  host.hidden = true
  return {
    start,
    scrolls: scrollsOf(seen.slice(at - 1)),
    legs: [
      scrollsOf(seen.slice(at - 1, turn - 1)),
      scrollsOf(seen.slice(turn - 1)),
    ],
    gaps: seen.slice(at - 1).map(({ gap }) => gap),
    heights: seen.slice(at - 1).map(({ row }) => row.height),
    glid: seen.some(({ gliding }) => gliding),
    row: {
      before: seen[at - 1].row,
      after,
      cover: seen[at - 1].cover,
      text: seen[at - 1].text,
      padding,
    },
    rest,
  }
}

/**
 * Whether some frame drew a value strictly between the least drawn and the
 * most: a row laid at once draws only its two ends, each leg's start and end.
 */
function slid(values: number[]): boolean {
  const [low, high] = [Math.min(...values), Math.max(...values)]
  return values.some((value) => value > low && value < high)
}

/**
 * Each frame of the glide drive's legs whose scroll lies over 0.5 px from
 * the same frame of the reduced-motion drive's.
 */
function offFrom(
  glide: Awaited<ReturnType<typeof scrolledThrough>>,
  still: Awaited<ReturnType<typeof scrolledThrough>>,
): string[] {
  return glide.legs.flatMap((leg, n) => {
    const reduced = still.legs[n]
    return leg
      .slice(0, reduced.length)
      .flatMap((scroll, frame) =>
        Math.abs(scroll - reduced[frame]) > 0.5
          ? [`leg ${n + 1}, frame ${frame}: ${scroll}, not ${reduced[frame]}`]
          : [],
      )
  })
}

/**
 * Through `step`, which drops a column of the home page `host` with its
 * row's list `row` at its start: each frame where card k is drawn over 0.5
 * px from the grid's cover k, as spec O304's "In step" bounds it. With it,
 * what the fixture tells: the row stands at its start, the columns
 * changed, and a cover the row's cards stand over was drawn away from its
 * box mid-glide.
 */
async function inStepThrough(
  host: HTMLElement,
  row: HTMLElement,
  step: () => Promise<void>,
) {
  await expect.poll(() => offGrid(host), POLL).toEqual([])
  const columns = [gridColumns(host).length]
  const grid = need(host, 'main .grid')
  const seen: {
    gliding: boolean
    cards: number[]
    covers: number[]
    laid: number[]
  }[] = []
  eachFrame(() => {
    seen.push({
      gliding: grid.hasAttribute('data-reseating'),
      cards: [...row.children].map((card) => card.getBoundingClientRect().left),
      covers: [...row.children].map(
        (_, k) => coverOf(host, k).getBoundingClientRect().left,
      ),
      laid: [...row.children].map(
        (_, k) => grid.children[k].getBoundingClientRect().left,
      ),
    })
  })
  await frames(2)
  await step()
  await afterMotion()
  columns.push(gridColumns(host).length)
  const shared = Math.min(...columns)
  const off = seen.flatMap(({ cards, covers }, frame) =>
    cards
      .slice(0, shared)
      .flatMap((left, k) =>
        Math.abs(left - covers[k]) > 0.5
          ? [`frame ${frame}: card ${k + 1} at ${left}, cover ${covers[k]}`]
          : [],
      ),
  )
  return {
    start: row.scrollLeft,
    columns: columns[1] === columns[0] - 1,
    glid: seen.some(
      ({ gliding, covers, laid }) =>
        gliding && Math.abs(covers[shared - 1] - laid[shared - 1]) > 2,
    ),
    off,
  }
}

const IN_STEP = { start: 0, columns: true, glid: true, off: [] }

describe('the Continue row through a glide (O304)', () => {
  // [the layout, the window's width before the step and after]: one column
  // fewer, the row at its start.
  it.each([
    ['beside the sidebar', 1280, 1200],
    ['in the phone layout', 824, 808],
  ] as const)(
    'keeps card k over the grid’s cover k through a glide, %s',
    async (_, from, to) => {
      const { host, row } = await drawn({ width: from, size: 144, entries: 12 })
      expect(
        await inStepThrough(host, row, async () => {
          await act(async () => {
            await dragged([{ width: to }])
          })
        }),
      ).toEqual(IN_STEP)
    },
  )

  // [the layout, the window's width, the cover size one step short of a
  // column fewer]. The slider's key lands in a task between frames, so the
  // row is laid in React's commit there, before the frame the covers'
  // glide starts in.
  it.each([
    ['beside the sidebar', 1280, 131],
    ['in the phone layout', 824, 144],
  ] as const)(
    'keeps card k over the grid’s cover k through a Cover size step’s glide, %s',
    async (_, width, size) => {
      const { host, row } = await drawn({ width, size, entries: 12 })
      await act(async () => {
        await userEvent.click(page.getByRole('button', { name: 'View' }))
      })
      expect(
        await inStepThrough(host, row, () => slide(host, '{ArrowRight}')),
      ).toEqual(IN_STEP)
    },
  )

  // [the layout, the window's width before the step and after, its
  // height]: on one side of square, since a window that turns square
  // snaps, as a phone's turn does.
  it.each([
    ['at 1280 px', 1280, 1200, 844],
    ['in the phone layout', 856, 776, 900],
  ] as const)(
    'scrolls nothing sideways in any frame of a glide, %s',
    async (_, from, to, height) => {
      const { host } = await drawn({
        width: from,
        height,
        size: 144,
        entries: 12,
      })
      const main = need(host, 'main')
      const root = document.scrollingElement ?? document.documentElement
      const grid = need(host, 'main .grid')
      const seen: { gliding: boolean; main: number[]; page: number[] }[] = []
      eachFrame(() => {
        seen.push({
          gliding: grid.hasAttribute('data-reseating'),
          main: [main.scrollWidth - main.clientWidth, main.scrollLeft],
          page: [root.scrollWidth - root.clientWidth, root.scrollLeft],
        })
      })
      await frames(2)
      await act(async () => {
        await dragged([{ width: to }])
      })
      await afterMotion()
      expect({
        glid: seen.some(({ gliding }) => gliding),
        sideways: seen.filter(
          (frame) =>
            frame.main.some((px) => px !== 0) ||
            frame.page.some((px) => px !== 0),
        ),
      }).toEqual({ glid: true, sideways: [] })
    },
  )

  it('holds main’s scroll through a glide', async () => {
    const drive = { from: 1280, to: [1200], down: 500 }
    const glide = await scrolledThrough({ ...drive, still: false })
    const still = await scrolledThrough({ ...drive, still: true })
    const count = Math.min(glide.scrolls.length, still.scrolls.length)
    const { bottom } = glide.row.before
    // The fixture tells: both start alike, the row above the view but near
    // enough to move with the covers, and only one glides, its row too.
    expect({
      starts: glide.start === still.start,
      above: bottom < 0 && bottom > -NEAR_PX,
      glid: [glide.glid, still.glid],
      slid: [slid(glide.gaps), slid(still.gaps)],
      scrolls: glide.scrolls.slice(0, count),
    }).toEqual({
      starts: true,
      above: true,
      glid: [true, false],
      slid: [true, false],
      scrolls: still.scrolls.slice(0, count),
    })
  })

  it('moves the page only as the reduced-motion drive does, and rests where it rests, in the phone layout', async () => {
    const drive = { from: 820, to: [812], down: 1600 }
    const glide = await scrolledThrough({ ...drive, still: false })
    const still = await scrolledThrough({ ...drive, still: true })
    const count = Math.min(glide.scrolls.length, still.scrolls.length)
    const tall = (row: DOMRect) => row.height
    // The fixture tells: both start alike, the row lies far above the view,
    // and its height changes, which the browser corrects the page for. The
    // frames as well as the rest: Chromium's corrections add up to the one
    // a row laid at once has it make, so only the frames tell them apart.
    expect({
      starts: glide.start === still.start,
      far: glide.row.before.bottom < -NEAR_PX,
      grew: tall(still.row.after) - tall(still.row.before) > 20,
      rest: Math.abs(glide.rest - still.rest) <= 1 ? still.rest : glide.rest,
      scrolls: glide.scrolls.slice(0, count),
    }).toEqual({
      starts: true,
      far: true,
      grew: true,
      rest: still.rest,
      scrolls: still.scrolls.slice(0, count),
    })
  })

  // [where the row lies as the first change is made, how far down the
  // wheel turns the page]: with its covers above the view, where the
  // browser anchors the page below them, so the row's height moves it.
  it.each([
    ['just above the view', 480],
    ['across the view’s top edge, its covers above the view', 340],
  ] as const)(
    'moves the page only as the reduced-motion drive does with the row %s, in the phone layout',
    async (where, down) => {
      const drive = { from: 820, to: [812], back: [820], down }
      const glide = await scrolledThrough({ ...drive, still: false })
      const still = await scrolledThrough({ ...drive, still: true })
      const { before: row, cover, padding } = glide.row
      const lies = () =>
        row.bottom < 0 && row.bottom > -NEAR_PX
          ? 'just above the view'
          : row.top < 0 && cover.bottom < 0 && row.bottom > 0
            ? 'across the view’s top edge, its covers above the view'
            : `from ${row.top} to ${row.bottom}, covers to ${cover.bottom}`
      const [first] = still.legs
      // The fixture tells: both start alike, the row lies where the case
      // says, its covers' foot lies where the row works it out from its box
      // and column, only one drive glides, and the reduced-motion drive
      // moves the page, so the browser corrects it for the row's height.
      const foot = row.top + padding + cover.width * COVER_TALL
      expect({
        starts: glide.start === still.start,
        where: lies(),
        foot: Math.abs(foot - cover.bottom) <= 0.5,
        glid: [glide.glid, still.glid],
        corrected: Math.abs(first[first.length - 1] - first[0]) > 20,
        off: offFrom(glide, still),
        rest: Math.abs(glide.rest - still.rest) <= 1 ? still.rest : glide.rest,
      }).toEqual({
        starts: true,
        where,
        foot: true,
        glid: [true, false],
        corrected: true,
        off: [],
        rest: still.rest,
      })
    },
  )

  it('moves the page only as the reduced-motion drive does with a card’s placeholder text across the top of the view, in the phone layout', async () => {
    const drive = { from: 820, to: [812], back: [820], down: 150 }
    const glide = await scrolledThrough({ ...drive, still: false })
    const still = await scrolledThrough({ ...drive, still: true })
    const { before: row, text } = glide.row
    const across =
      row.top < 0 &&
      text.height > 0 &&
      text.top >= 0 &&
      text.bottom <= innerHeight
    const grown = (heights: number[]) =>
      Math.max(...heights) - Math.min(...heights) > 20
    // The fixture tells: both start alike, the row's top lies above the view
    // and its first card's placeholder text wholly in it, the row's height
    // changes in both drives, and only one glides and slides the row.
    expect({
      starts: glide.start === still.start,
      across:
        across || `row from ${row.top}, text ${text.top} to ${text.bottom}`,
      grew: [grown(glide.heights), grown(still.heights)],
      glid: [glide.glid, still.glid],
      slid: [slid(glide.heights), slid(still.heights)],
      off: offFrom(glide, still),
      rest: Math.abs(glide.rest - still.rest) <= 1 ? still.rest : glide.rest,
    }).toEqual({
      starts: true,
      across: true,
      grew: [true, true],
      glid: [true, false],
      slid: [true, false],
      off: [],
      rest: still.rest,
    })
  })

  it('starts the covers where they were drawn with the row across the view’s top edge and its covers in view, in the phone layout', async () => {
    const { host, row } = await drawn({
      width: 820,
      size: 144,
      entries: 12,
      series: 60,
    })
    await wheeledTo(host, 268)
    const grid = need(host, 'main .grid')
    const covers = [...grid.children].map((_, k) => coverOf(host, k))
    const seen: { width: number; gliding: boolean; covers: DOMRect[] }[] = []
    eachFrame(() => {
      seen.push({
        width: innerWidth,
        gliding: grid.hasAttribute('data-reseating'),
        covers: covers.map((cover) => cover.getBoundingClientRect()),
      })
    })
    await frames(2)
    const across = {
      row: row.getBoundingClientRect().top,
      covers: need(
        row,
        ':scope > li :is(img, .cover-missing)',
      ).getBoundingClientRect().bottom,
    }
    await act(async () => {
      await dragged([{ width: 812 }])
    })
    await afterMotion()
    const at = seen.findIndex(({ width }) => width === 812)
    const shown = (box: DOMRect) => box.bottom > 0 && box.top < innerHeight
    // Each cover in view, in the frame the columns change against the frame
    // before: a glide starts where the cover was drawn.
    const off = seen[at].covers.flatMap((box, k) => {
      const was = seen[at - 1].covers[k]
      const moved = Math.max(
        Math.abs(box.top - was.top),
        Math.abs(box.left - was.left),
        Math.abs(box.width - was.width),
      )
      return (shown(box) || shown(was)) && moved > 0.5
        ? [`cover ${k + 1}: ${moved} px`]
        : []
    })
    // The fixture tells: the row's top lies above the view and its covers'
    // foot in it, the change was drawn, and the covers glid.
    expect({
      across: across.row < 0 && across.covers > 0,
      drawn: at >= 1,
      glid: seen.some(({ gliding }) => gliding),
      off,
    }).toEqual({ across: true, drawn: true, glid: true, off: [] })
  })
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
    await afterMotion(FADE_MS)
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
    await afterMotion()
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
    await afterMotion()
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
    await afterMotion()
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
    await afterMotion(FADE_MS)
    const focused = document.activeElement
    expect({
      inRow: focused !== null && row.contains(focused),
      ring: focused?.matches(':focus-visible'),
      fade: fade(row),
    }).toEqual({
      inRow: true,
      ring: true,
      fade: { ...AFTER, mask: { before: WHOLE, after: WHOLE } },
    })
  })

  it('fades in and out over 200 ms', async () => {
    const { row } = await drawn({ width: 1280, size: 144, entries: 20 })
    await expect.poll(() => fade(row), POLL).toEqual(AFTER)
    const travel = row.scrollWidth - row.clientWidth
    const seen: unknown[] = []
    // The side before starts fading as the row leaves its start, and stops
    // as it comes back.
    for (const left of [travel / 2, 0]) {
      const changed = marked(row, 'data-more-before', left > 0)
      row.scrollLeft = left
      await changed
      const { run, ...middle } = halfway(row, 'before')
      await run?.finished
      await frames(1)
      seen.push({ ...middle, end: edges(row).before })
    }
    expect(seen).toEqual([
      { timing: EASED, midway: 'between', end: 0 },
      { timing: EASED, midway: 'between', end: 1 },
    ])
  })

  it('lifts the fade at once for a focus ring, and brings it back over 200 ms', async () => {
    const { host, row } = await drawn({ width: 1280, size: 144, entries: 20 })
    await expect.poll(() => fade(row), POLL).toEqual(AFTER)
    // Each read in the first frame drawn after focus moves, before that
    // frame's style starts any transition the move brings.
    function firstFrame<T>(type: 'focusin' | 'focusout', read: () => T) {
      return new Promise<T>((done) => {
        row.addEventListener(
          type,
          () => requestAnimationFrame(() => done(read())),
          { once: true },
        )
      })
    }
    const entered = firstFrame('focusin', () => edges(row))
    const left = firstFrame('focusout', () => halfway(row, 'after'))
    // Inside the frame first, as `slide` says; a card is next in the order.
    await act(async () => need(host, '.view-button').focus())
    await act(async () => {
      await userEvent.keyboard('{Tab}')
    })
    const focused = document.activeElement
    const ring =
      focused !== null &&
      row.contains(focused) &&
      focused.matches(':focus-visible')
    const lifted = await entered
    await act(async () => {
      await userEvent.keyboard('{Shift>}{Tab}{/Shift}')
    })
    const { run, ...middle } = await left
    await run?.finished
    await frames(1)
    expect({
      ring,
      lifted,
      middle,
      back: edges(row),
      gone: row.contains(document.activeElement),
    }).toEqual({
      ring: true,
      lifted: { before: 1, after: 1 },
      middle: { timing: EASED, midway: 'between' },
      back: { before: 1, after: 0 },
      gone: false,
    })
  })

  it('flips at once under reduced motion', async () => {
    onTestFinished(async () => {
      await reducedMotion(false)
    })
    await reducedMotion(true)
    const { row } = await drawn({ width: 1280, size: 144, entries: 20 })
    await expect.poll(() => fade(row), POLL).toEqual(AFTER)
    const changed = marked(row, 'data-more-before', true)
    row.scrollLeft = (row.scrollWidth - row.clientWidth) / 2
    await changed
    await frames(1)
    expect({
      reduced: window.matchMedia('(prefers-reduced-motion: reduce)').matches,
      duration: getComputedStyle(row).transitionDuration,
      before: edges(row).before,
    }).toEqual({ reduced: true, duration: '0s', before: 0 })
  })

  it.each([
    ['6, 25 and 40', '390×844', 390, 844, NARROW],
    ['6, 25 and 40', '956×440', 956, 440, NARROW],
    ['12, 50 and 80', '1280×800', 1280, 800, WIDE],
  ] as const)('fades %s px at %s', async (_, __, width, height, at) => {
    const { row } = await drawn({ width, height, size: 144, entries: 20 })
    await expect
      .poll(() => fade(row), POLL)
      .toEqual({
        before: false,
        after: true,
        mask: { before: side(1, at), after: side(0, at) },
      })
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

  it.each([
    [390, 844, 'fine'],
    [390, 844, 'coarse'],
    [956, 440, 'fine'],
    [956, 440, 'coarse'],
  ] as const)(
    'leaves the heading as it was in the phone layout at %ix%i (%s pointer)',
    async (width, height, pointer) => {
      if (pointer === 'coarse') onTestFinished(coarseRulesOn())
      const { host } = await drawn({ width, height, size: 144, entries: 6 })
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
