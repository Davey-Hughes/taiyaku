import { Profiler } from 'react'
import { BrowserRouter, MemoryRouter } from 'react-router'
import { beforeEach, describe, expect, it, onTestFinished, vi } from 'vitest'
import { page, userEvent } from 'vitest/browser'
import '../src/index.css'
import * as aboutApi from '../src/about'
import * as api from '../src/api'
import * as config from '../src/config'
import * as curation from '../src/curation'
import { GateContext } from '../src/gate'
import * as librariesApi from '../src/libraries'
import * as library from '../src/library'
import { REST_IDLE_MS, TURN_HOLD_MS, trackEnd } from '../src/phoneBar'
import { AppRoutes } from '../src/Routes'
import { onLanded } from '../src/scrollKeeper'
import { Shell } from '../src/Shell'
import * as usersApi from '../src/users'
import { ABOUT } from './support/about'
import { inProgress } from './support/continueRow'
import {
  eachFrame,
  frames,
  reducedMotion,
  scrollNeverEnds,
  stillPage,
} from './support/motion'
import { beforeUnmount, mount } from './support/mount'
import { mouse } from './support/mouse'
import { barRested, clickAt, clickShown } from './support/phoneBar'

/**
 * The phone bar moving with the page, measured in a real browser under a
 * real wheel: the track's end (`phoneBar.ts`) is checked as numbers in
 * `phoneBar.hook.test.tsx`, and here as the pixels the bar is drawn at, in
 * the app's own routes, upright (440 × 956) and on its side (956 × 440).
 * The harness is `scrollKeeper.browser.test.tsx`'s.
 */

vi.mock('../src/App', () => ({
  default: ({ onLeave }: { onLeave: () => void }) => (
    <div data-testid="reader">
      <button type="button" onClick={onLeave}>
        Library
      </button>
    </div>
  ),
}))
vi.mock('../src/about', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/about')>()),
  about: vi.fn(),
}))
vi.mock('../src/api', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/api')>()),
  listKeys: vi.fn(),
  listSessions: vi.fn(),
}))
vi.mock('../src/curation', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/curation')>()),
  listProposals: vi.fn(),
}))
vi.mock('../src/config', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/config')>()),
  scanStatus: vi.fn(),
  listRoots: vi.fn(),
}))
vi.mock('../src/libraries', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/libraries')>()),
  libraryScanStatus: vi.fn(),
  listLibraries: vi.fn(),
}))
vi.mock('../src/users', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/users')>()),
  listUsers: vi.fn(),
}))
vi.mock('../src/library', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/library')>()),
  status: vi.fn(),
  home: vi.fn(),
  grid: vi.fn(),
  oneSeries: vi.fn(),
  search: vi.fn(),
}))

/** The bar's height in the phone layout: `--sticky-top`, no inset here. */
const H = 52

/** A cover that fails with no request, as `scrollKeeper.browser.test.tsx` says. */
const FAILS = 'data:image/png;base64,!'

/** Series enough that the home page is many windows tall at either size. */
const GRID: library.Grid = {
  series: Array.from({ length: 200 }, (_, i) => ({
    id: i + 1,
    title: `Kaguya-sama wa Kokurasetai ${i + 1}`,
    works: 3,
    editions: 1,
    aligned: 0,
    added: i + 1,
    unread: 3,
    cover: FAILS,
  })),
  unfiled: [],
}

/** Volumes enough that a series page is many windows tall too. */
const SERIES_PAGE = {
  series: { ...GRID.series[0], titles: [] },
  works: Array.from(
    { length: 80 },
    (_, i): library.WorkCard => ({
      id: i + 1,
      slug: `work-${i + 1}`,
      title: `Kaguya-sama wa Kokurasetai v${i + 1}`,
      volume: i + 1,
      aligned: false,
      pages: 180,
      editions: [
        {
          key: 'raw',
          label: 'Raw',
          members: 1,
          pages: 180,
          missing: 0,
          cover: FAILS,
        },
      ],
      opens: 'raw',
    }),
  ),
}

beforeEach(() => {
  localStorage.clear()
  window.scrollTo(0, 0)
  vi.mocked(aboutApi.about).mockResolvedValue(ABOUT)
  vi.mocked(api.listKeys).mockResolvedValue([])
  vi.mocked(api.listSessions).mockResolvedValue([])
  vi.mocked(curation.listProposals).mockResolvedValue({
    proposals: [],
    counts: {},
  } as never)
  vi.mocked(config.listRoots).mockResolvedValue({ roots: [] })
  vi.mocked(librariesApi.listLibraries).mockResolvedValue({ libraries: [] })
  vi.mocked(usersApi.listUsers).mockResolvedValue({ users: [] })
  vi.mocked(library.status).mockResolvedValue({
    libraries: [{ id: 1, name: 'Volumes', scanned: true }],
    restricted: false,
  })
  vi.mocked(library.home).mockResolvedValue({
    continuing: inProgress(8),
    added: [],
    read: [],
  })
  vi.mocked(library.grid).mockResolvedValue(GRID)
  vi.mocked(library.oneSeries).mockResolvedValue(SERIES_PAGE)
})

/**
 * Real time with nothing in `act`, as the app runs: the scheduler's tasks,
 * a passive effect's included, and two frames.
 */
async function idle() {
  await new Promise((resolve) => setTimeout(resolve, 300))
  await new Promise((resolve) => requestAnimationFrame(resolve))
  await new Promise((resolve) => requestAnimationFrame(resolve))
  await new Promise((resolve) => setTimeout(resolve, 50))
}

/** Turns React's `act` warnings off for the case: nothing is in `act` here. */
function outsideAct() {
  globalThis.IS_REACT_ACT_ENVIRONMENT = false
  beforeUnmount(() => {
    globalThis.IS_REACT_ACT_ENVIRONMENT = true
  })
}

/**
 * The app's routes at `/`, signed in as an administrator, under
 * `BrowserRouter` in this frame's own history, which is put back when the
 * test ends, at a window `width` × `height`, drawn and idle.
 */
async function app(width: number, height: number): Promise<HTMLElement> {
  const before = location.href
  onTestFinished(() => {
    history.replaceState(null, '', before)
  })
  history.replaceState(null, '', '/')
  await page.viewport(width, height)
  const host = mount(
    <BrowserRouter>
      <GateContext.Provider
        value={{
          who: {
            user_id: 1,
            admin: true,
            can_download: false,
            can_stream: false,
            can_stamp: false,
          },
          signOut: () => {},
        }}
      >
        <AppRoutes />
      </GateContext.Provider>
    </BrowserRouter>,
  )
  // The height chain `base.css` gives `#root`, as `Shell.browser.test.tsx`'s
  // `at()` says.
  host.style.height = '100%'
  outsideAct()
  await idle()
  return host
}

function need<T extends Element = HTMLElement>(
  host: ParentNode,
  selector: string,
): T {
  const found = host.querySelector<T>(selector)
  if (!found) throw new Error(`no ${selector}`)
  return found
}

/** The bar's box top in the window. */
const barTop = (host: ParentNode) =>
  need(host, '.phone-bar').getBoundingClientRect().top

/** The track's laid height: the end of the run the bar is pinned in. */
const trackEndOf = (host: ParentNode) =>
  need(host, '.phone-bar-track').getBoundingClientRect().height

const maxScroll = () => document.documentElement.scrollHeight - innerHeight

/**
 * Turns the wheel `dy` px with the pointer over the page below the bar,
 * clear of the Continue row, and waits for the page to be still; gives how
 * far the page moved.
 */
async function wheeled(dy: number): Promise<number> {
  const from = scrollY
  await mouse.moveTo(innerWidth / 2, innerHeight - 24)
  await mouse.wheel(dy)
  await stillPage()
  return scrollY - from
}

/** Wheels down until the page is `y` px down, or as near as it goes. */
async function wheeledDownTo(y: number) {
  for (let turn = 0; scrollY < y - 1 && turn < 20; turn++) {
    await wheeled(Math.ceil(y - scrollY))
  }
}

/**
 * Wheels down to `y`, then up by more than the bar's height, so the bar has
 * slid out and come back whole, and waits for the page to be still.
 */
async function pinnedMidPage(host: HTMLElement, y: number) {
  await wheeledDownTo(y)
  await wheeled(-2 * H)
  expect(barTop(host), 'pinned mid-page').toBe(0)
  expect(scrollY, 'pinned mid-page').toBeGreaterThan(4 * H)
}

/**
 * `scrollY` and the bar's top in each frame while `drive` runs and until
 * the page is still.
 */
async function sampled(
  drive: () => Promise<void>,
): Promise<Array<{ y: number; top: number }>> {
  const samples: Array<{ y: number; top: number }> = []
  let on = true
  const sample = () => {
    if (!on) return
    samples.push({ y: scrollY, top: barTop(document) })
    requestAnimationFrame(sample)
  }
  requestAnimationFrame(sample)
  try {
    await drive()
    await stillPage()
  } finally {
    on = false
  }
  return samples
}

/** Each wheel moved the page by its delta, as Chromium scrolls a wheel's lines. */
const movedBy = (moves: number[], deltas: number[]) =>
  moves.map((moved, i) => Math.abs(moved - (deltas[i] ?? 0)) <= 0.5)

/** The page is tall enough that the bar can slide out and stay out. */
function fourWindowsTall() {
  expect(document.documentElement.scrollHeight).toBeGreaterThanOrEqual(
    4 * innerHeight,
  )
}

/**
 * From pinned mid-page, wheels down 20 and 100 and up 20 and 100: the bar
 * slides out with the page, stays out, slides in and is pinned again.
 */
async function movesFromPinned(width: number, height: number) {
  const host = await app(width, height)
  fourWindowsTall()
  await pinnedMidPage(host, 1500)
  scrollNeverEnds()
  const y = scrollY
  const deltas = [20, 100, -20, -100]
  const moves: number[] = []
  const tops: number[] = []
  for (const dy of deltas) {
    moves.push(await wheeled(dy))
    tops.push(barTop(host))
  }
  expect(movedBy(moves, deltas)).toEqual([true, true, true, true])
  expect(tops).toEqual([-20, -H, -(H - 20), 0])
  expect(scrollY).toBe(y)
  expect(trackEndOf(host)).toBe(y + H)
}

/**
 * Wheels down and back up 600 px with the pointer over the page, 30 px a
 * frame: Chromium here scrolls a wheel's whole delta in one frame, so one
 * turn of 600 would draw the bar's start and end and nothing between.
 */
async function downAndUp(): Promise<{
  down: Array<{ y: number; top: number }>
  up: Array<{ y: number; top: number }>
}> {
  const drive = (dy: number) => async () => {
    await mouse.moveTo(innerWidth / 2, innerHeight - 24)
    for (let moved = 0; moved < 600; moved += 30) {
      await mouse.wheel(Math.sign(dy) * 30)
      await new Promise(requestAnimationFrame)
    }
  }
  const down = await sampled(drive(600))
  const up = await sampled(drive(-600))
  return { down, up }
}

/** Hides `main`'s content, so the page is no taller than the window. */
function emptied(host: HTMLElement) {
  const shown = [...need(host, 'main').children] as HTMLElement[]
  for (const each of shown) each.style.display = 'none'
  onTestFinished(() => {
    for (const each of shown) each.style.display = ''
  })
}

/**
 * Pads the end of `main` by 200 px until the case ends; gives the shrink in
 * place that takes the padding off again.
 */
function paddedAtTheEnd(host: HTMLElement): () => void {
  const main = need(host, 'main')
  main.style.paddingBlockEnd = 'calc(var(--page-area-end) + 200px)'
  const unpad = () => {
    main.style.removeProperty('padding-block-end')
  }
  onTestFinished(unpad)
  return unpad
}

/**
 * Each frame from the next on, as drawn: how much taller the document is
 * than the page's own box, the bar's top in the window, and whether a
 * press at its middle lands on it, which a bar the clip cuts off fails.
 */
function heldEachFrame(host: HTMLElement) {
  const seen: Array<{ held: number; top: number; drawn: boolean }> = []
  const bar = need(host, '.phone-bar')
  eachFrame(() => {
    const own = need(host, '.library').getBoundingClientRect()
    const top = barTop(host)
    const hit = document.elementFromPoint(innerWidth / 2, top + H / 2)
    seen.push({
      held: Math.round(document.documentElement.scrollHeight - own.height),
      top: Math.round(top),
      drawn: hit !== null && bar.contains(hit),
    })
  })
  return seen
}

describe.each([
  [440, 956],
  [956, 440],
])('the phone bar at %i × %i', (width, height) => {
  it('moves with the scroll from pinned mid-page', async () => {
    await movesFromPinned(width, height)
  })

  it('stays at 0 at the top over the page’s first row, and a wheel past either end moves nothing', async () => {
    const host = await app(width, height)
    fourWindowsTall()
    expect(scrollY).toBe(0)
    const first = need(host, 'main').firstElementChild
    if (first === null) throw new Error('main is empty')
    const atTop = await wheeled(-100)
    expect({
      moved: atTop,
      top: barTop(host),
      end: trackEndOf(host),
      clear:
        first.getBoundingClientRect().top >=
        need(host, '.phone-bar').getBoundingClientRect().bottom,
    }).toEqual({ moved: 0, top: 0, end: H, clear: true })
    for (let turn = 0; scrollY < maxScroll() - 1 && turn < 20; turn++) {
      await wheeled(Math.ceil(maxScroll() - scrollY))
    }
    expect(Math.abs(scrollY - maxScroll()), 'at the end').toBeLessThan(1)
    const y = scrollY
    const end = trackEndOf(host)
    const atEnd = await wheeled(100)
    expect({ moved: atEnd, y: scrollY, end: trackEndOf(host) }).toEqual({
      moved: 0,
      y,
      end,
    })
  })

  it('keeps the bar where the band puts it in every frame of a wheel scroll', async () => {
    const host = await app(width, height)
    fourWindowsTall()
    await pinnedMidPage(host, 1500)
    scrollNeverEnds()
    let end = trackEndOf(host)
    const { down, up } = await downAndUp()
    const off: Array<{ y: number; top: number; expected: number }> = []
    for (const { y, top } of [...down, ...up]) {
      end = trackEnd(end, y, H, maxScroll())
      const expected = Math.min(0, Math.max(-H, end - H - y))
      if (Math.abs(top - expected) > 1) off.push({ y, top, expected })
    }
    const distinct = (samples: Array<{ y: number }>) =>
      new Set(samples.map(({ y }) => y)).size
    expect(
      [distinct(down), distinct(up)].map((count) => count >= 5),
      'five frames with distinct y each way',
    ).toEqual([true, true])
    expect(off).toEqual([])
  })

  it('tracks under reduced motion as without it', async () => {
    await reducedMotion(true)
    onTestFinished(async () => {
      await reducedMotion(false)
    })
    expect(matchMedia('(prefers-reduced-motion: reduce)').matches).toBe(true)
    await movesFromPinned(width, height)
  })

  it('brings the bar back when the page grows shorter than the window under it', async () => {
    const host = await app(width, height)
    fourWindowsTall()
    await pinnedMidPage(host, 1500)
    await wheeled(2 * H)
    expect(barTop(host), 'hidden mid-page').toBe(-H)
    // Past the window in which a scroll counts as the wheel's: the clamp
    // is a scroll no input began.
    await new Promise((resolve) => setTimeout(resolve, 2 * REST_IDLE_MS))
    emptied(host)
    await new Promise(requestAnimationFrame)
    await new Promise(requestAnimationFrame)
    expect(maxScroll(), 'no taller than the window').toBeLessThanOrEqual(0)
    expect({ y: scrollY, top: barTop(host), end: trackEndOf(host) }).toEqual({
      y: 0,
      top: 0,
      end: H,
    })
  })

  // The clamp's scroll event comes only after the frame with the clamp is
  // drawn, so nothing the hook does on it can make that frame right. Each
  // shrink clamps the scroll up past the bar's height, so the band rule
  // shows the bar whole.
  it.each([
    ['hidden mid-page', 'emptied', 'hidden'],
    ['at rest, pinned near the page’s end', 'emptied', 'rest'],
    ['on its track, pinned near the page’s end', 'emptied', 'track'],
    ['at rest, pinned near the page’s end', '200 px shorter', 'rest'],
    ['on its track, pinned near the page’s end', '200 px shorter', 'track'],
  ] as const)(
    'holds the page no taller than its content, and the bar whole at the top, in every frame of a shrink in place, the bar %s, the page %s',
    async (_, shrink, start) => {
      const host = await app(width, height)
      const unpadded = shrink === 'emptied' ? null : paddedAtTheEnd(host)
      fourWindowsTall()
      if (start === 'hidden') {
        await pinnedMidPage(host, 1500)
        await wheeled(2 * H)
      } else {
        await pinnedMidPage(host, maxScroll())
        if (start === 'track') {
          scrollNeverEnds()
          await wheeled(20)
          await wheeled(-20)
        }
      }
      // Past the window in which a scroll counts as the wheel's.
      await new Promise((resolve) => setTimeout(resolve, 2 * REST_IDLE_MS))
      const bar = need(host, '.phone-bar')
      expect(
        { top: barTop(host), rest: bar.hasAttribute('data-at-rest') },
        start,
      ).toEqual({
        top: start === 'hidden' ? -H : 0,
        rest: start !== 'track',
      })
      const y = scrollY
      const seen = heldEachFrame(host)
      if (unpadded === null) emptied(host)
      else unpadded()
      await idle()
      expect({ clamped: y - scrollY > H, frames: seen.length > 0 }).toEqual({
        clamped: true,
        frames: true,
      })
      expect(
        seen.filter(({ held, top, drawn }) => held > 1 || top !== 0 || !drawn),
      ).toEqual([])
    },
  )
})

/** Back, by the browser's own history, with nothing in `act`. */
async function back() {
  const popped = new Promise((resolve) => {
    addEventListener('popstate', resolve, { once: true })
  })
  history.back()
  await popped
  await idle()
}

/** A series' cover wholly in view, clear of the window's edges. */
function coverInView(host: HTMLElement): Element {
  const cover = [...host.querySelectorAll('main .grid > li a')].find((a) => {
    const { top, bottom } = a.getBoundingClientRect()
    return top > 50 && bottom < innerHeight - 50
  })
  if (cover === undefined) throw new Error('no cover wholly in view')
  return cover
}

/** Whether keyboard focus, as the browser marks it, is on `el`. */
const keyboardFocusOn = (el: Element) =>
  document.activeElement === el && el.matches(':focus-visible')

/**
 * **The bar held in view, and shown by a navigation** (spec "The bar is
 * forced fully in view", "A navigation shows the bar"): the open drawer
 * and keyboard focus in the bar hold it whole, focus a click left there
 * does not, and a new page, Back to a kept place among them, brings it in.
 */
describe('the phone bar upright, held in view and shown by a navigation', () => {
  it('brings the bar fully in when the drawer opens mid-page', async () => {
    const host = await app(440, 956)
    fourWindowsTall()
    await pinnedMidPage(host, 1500)
    const hid = await wheeled(2 * H)
    expect({ hid, top: barTop(host) }, 'hidden mid-page').toEqual({
      hid: 2 * H,
      top: -H,
    })
    const moved = await wheeled(-30)
    expect({ moved, top: barTop(host) }, '30 px in view').toEqual({
      moved: -30,
      top: -(H - 30),
    })
    await clickShown(need(host, '.menu-button'))
    await idle()
    expect({
      open: need(host, '.library').classList.contains('menu-open'),
      top: barTop(host),
      end: trackEndOf(host) - scrollY,
    }).toEqual({ open: true, top: 0, end: H })
  })

  // Keyboard focus reaches the bar mid-page from the control just after it,
  // `main`'s first, which a reader can be on and then scroll away from.
  it('brings the bar in on Shift+Tab into it without scrolling the page, and holds it while keyboard focus is there', async () => {
    const host = await app(440, 956)
    fourWindowsTall()
    const first = need(
      host,
      'main :is(a[href], button, input, select, textarea, [tabindex="0"])',
    )
    await clickShown(need(host, '.continue h2'))
    for (let turn = 0; document.activeElement !== first && turn < 5; turn++) {
      await userEvent.tab({ shift: true })
    }
    expect(document.activeElement, "main's first control").toBe(first)
    await pinnedMidPage(host, 1500)
    const hid = await wheeled(2 * H)
    expect({ hid, top: barTop(host) }, 'hidden mid-page').toEqual({
      hid: 2 * H,
      top: -H,
    })
    const y = scrollY
    await userEvent.tab({ shift: true })
    await idle()
    const search = need(host, '.phone-bar .search')
    expect({
      focus: keyboardFocusOn(search),
      moved: Math.abs(scrollY - y) <= 0.5 ? 0 : scrollY - y,
      top: barTop(host),
    }).toEqual({ focus: true, moved: 0, top: 0 })
    const down = await wheeled(300)
    expect({ down, top: barTop(host) }).toEqual({ down: 300, top: 0 })
  })

  it('lets the bar go after the drawer closes by a tap on a library', async () => {
    const host = await app(440, 956)
    fourWindowsTall()
    expect(scrollY).toBe(0)
    const menu = need(host, '.menu-button')
    await clickShown(menu)
    await idle()
    await userEvent.click(need(host, '#library-menu a[href="/?library=1"]'))
    await idle()
    expect({
      at: `${location.pathname}${location.search}`,
      open: need(host, '.library').classList.contains('menu-open'),
      focus: document.activeElement === menu,
      keyboard: menu.matches(':focus-visible'),
    }).toEqual({ at: '/?library=1', open: false, focus: true, keyboard: false })
    const down = await wheeled(300)
    expect({
      down,
      bottom: need(host, '.phone-bar').getBoundingClientRect().bottom <= 0,
    }).toEqual({ down: 300, bottom: true })
  })

  // A replace on the same path refines the page shown and leaves the scroll
  // where it is: no navigation, so the bar stays where the scroll put it.
  it('leaves the bar out when a press of the sort reorders the page under it', async () => {
    const host = await app(440, 956)
    fourWindowsTall()
    const sort = need(host, 'main .sort')
    const down = await wheeled(2 * H)
    const bar = need(host, '.phone-bar')
    expect({ down, out: bar.getBoundingClientRect().bottom <= 0 }).toEqual({
      down: 2 * H,
      out: true,
    })
    const y = scrollY
    const before = `${location.pathname}${location.search}`
    await clickShown(sort)
    await idle()
    expect({
      path: location.pathname,
      replaced: `${location.pathname}${location.search}` !== before,
      still: Math.abs(scrollY - y) <= 1,
      out: bar.getBoundingClientRect().bottom <= 0,
    }).toEqual({ path: '/', replaced: true, still: true, out: true })
  })

  it('holds the bar while Escape leaves keyboard focus on ☰', async () => {
    const host = await app(440, 956)
    fourWindowsTall()
    await pinnedMidPage(host, 1500)
    const menu = need(host, '.menu-button')
    await clickShown(menu)
    await idle()
    expect(need(host, '.library').classList.contains('menu-open')).toBe(true)
    await userEvent.keyboard('{Escape}')
    await idle()
    expect({
      open: need(host, '.library').classList.contains('menu-open'),
      focus: keyboardFocusOn(menu),
    }).toEqual({ open: false, focus: true })
    const down = await wheeled(300)
    expect({ down, top: barTop(host) }).toEqual({ down: 300, top: 0 })
  })

  it('shows the bar on Back to a kept place, round after round', async () => {
    const host = await app(440, 956)
    fourWindowsTall()
    const rounds = []
    for (const kept of [1500, 1800, 2100]) {
      await wheeledDownTo(kept)
      const y = scrollY
      const hidden = barTop(host) === -H
      await clickShown(coverInView(host))
      await idle()
      const series = location.pathname.startsWith('/series/')
      const down = await wheeled(600)
      await back()
      rounds.push({
        hidden,
        series,
        down,
        // Within a pixel of the place kept: 0, or how far off.
        back: Math.abs(scrollY - y) <= 1 ? 0 : scrollY - y,
        top: barTop(host),
        end: trackEndOf(host) - scrollY,
      })
    }
    const each = {
      hidden: true,
      series: true,
      down: 600,
      back: 0,
      top: 0,
      end: H,
    }
    expect(rounds).toEqual([each, each, each])
  })
})

/**
 * **A shorter page after a navigation ends at its own end** (spec "The
 * track"): the track's end, set for the page left, never holds the next
 * page's document taller than its content, so that page lands as it would
 * with no track, its content in view under the bar. Account and About keep
 * no scroll of their own: the browser's clamp is what brings them in.
 */
describe.each([
  [440, 956],
  [956, 440],
])(
  'the phone bar at %i × %i, at a navigation to a shorter page',
  (width, height) => {
    it.each([
      ['/account', true],
      ['/about', false],
    ])(
      'lands %s from deep in the Library with its content in view and the document no taller than it',
      async (path, viaMore) => {
        const host = await app(width, height)
        fourWindowsTall()
        await pinnedMidPage(host, 4000)
        const deep = scrollY
        await clickShown(need(host, '.menu-button'))
        await idle()
        if (viaMore) {
          await clickShown(need(host, '#library-menu .account-button'))
          await idle()
        }
        await clickShown(need(host, `#library-menu a[href="${path}"]`))
        await idle()
        const main = need(host, 'main').getBoundingClientRect()
        // The shell's box: `main` and the bar's room above it, never the track.
        const content = need(host, '.library').getBoundingClientRect().height
        const doc = document.documentElement.scrollHeight
        expect({
          // The fixture: the page left was scrolled past the next one's end.
          deep: deep > content,
          at: location.pathname,
          inView: main.top < innerHeight && main.bottom > H,
          end: Math.abs(doc - content) <= 1 ? 0 : doc - content,
          top: barTop(host),
        }).toEqual({ deep: true, at: path, inView: true, end: 0, top: 0 })
      },
    )
  },
)

/** How the bar is drawn: its computed position, whether it rests, its top. */
function drawn(host: ParentNode) {
  const bar = need(host, '.phone-bar')
  return {
    position: getComputedStyle(bar).position,
    rest: bar.hasAttribute('data-at-rest'),
    top: bar.getBoundingClientRect().top,
  }
}

/** Turns the wheel `dy` px with the pointer over the page below the bar. */
const turning = (dy: number) => async () => {
  await mouse.moveTo(innerWidth / 2, innerHeight - 24)
  await mouse.wheel(dy)
}

const RESTS_PINNED = { position: 'relative', rest: true, top: 0 }

/**
 * **At rest, the bar is part of the page** (spec "At rest, the bar is part
 * of the page"): whenever any of it is in view and the page is still, after
 * a scroll and after each show alike, it is `relative` at the pixels it was
 * drawn at, and the next wheel puts it back on the track.
 */
describe('the phone bar upright, at rest in the page', () => {
  it.each([
    ['shown', 0, -30, 0],
    ['26 px shown', 2 * H, -26, -(H - 26)],
  ])(
    'rests in the page when the scroll ends, at the pixels it was pinned at, %s',
    async (_, hide, dy, top) => {
      const host = await app(440, 956)
      fourWindowsTall()
      await pinnedMidPage(host, 1500)
      if (hide > 0) {
        const hid = await wheeled(hide)
        expect({ hid, top: barTop(host) }, 'hidden mid-page').toEqual({
          hid: hide,
          top: -H,
        })
      }
      const bar = need(host, '.phone-bar')
      // Added after the hook's own, so it reads the box that scroll drew.
      let last: { box: DOMRect; rest: boolean } | undefined
      const read = () => {
        last = {
          box: bar.getBoundingClientRect(),
          rest: bar.hasAttribute('data-at-rest'),
        }
      }
      addEventListener('scroll', read, { passive: true })
      onTestFinished(() => removeEventListener('scroll', read))
      const ended = new Promise((resolve) => {
        addEventListener('scrollend', resolve, { once: true })
      })
      const from = scrollY
      await turning(dy)()
      await ended
      await frames(1)
      removeEventListener('scroll', read)
      if (last === undefined) throw new Error('no scroll seen')
      const was = last.box
      const box = bar.getBoundingClientRect()
      const sides = ['top', 'bottom', 'left', 'right'] as const
      expect({
        moved: Math.abs(scrollY - from - dy) <= 0.5,
        tracked: { rest: last.rest, top: was.top },
        position: getComputedStyle(bar).position,
        rest: bar.hasAttribute('data-at-rest'),
        off: sides.filter((side) => Math.abs(box[side] - was[side]) > 0.5),
      }).toEqual({
        moved: true,
        tracked: { rest: false, top },
        position: 'relative',
        rest: true,
        off: [],
      })
    },
  )

  it('keeps the bar at the top in every frame of a wheel up from rest, and carries it out with the content on a wheel down', async () => {
    const host = await app(440, 956)
    fourWindowsTall()
    await pinnedMidPage(host, 1500)
    const runs = []
    for (const dy of [-200, 30]) {
      const from = { rest: await barRested(host), top: barTop(host) }
      const y0 = scrollY
      const samples = await sampled(turning(dy))
      const off = samples.filter(
        ({ y, top }) => Math.abs(top - Math.min(0, -(y - y0))) > 1,
      )
      runs.push({
        from,
        moved: Math.abs(scrollY - y0 - dy) <= 0.5,
        sawMove: samples.some(({ y }) => y !== y0),
        off,
        end: barTop(host),
      })
    }
    const fromRest = { rest: true, top: 0 }
    expect(runs).toEqual([
      { from: fromRest, moved: true, sawMove: true, off: [], end: 0 },
      { from: fromRest, moved: true, sawMove: true, off: [], end: -30 },
    ])
  })

  it('has left rest by the first scroll event of a wheel', async () => {
    const host = await app(440, 956)
    fourWindowsTall()
    await pinnedMidPage(host, 1500)
    const bar = need(host, '.phone-bar')
    const atRest = await barRested(host)
    // In the capture phase at the window, before the hook's own listener.
    const seen: boolean[] = []
    const read = () => {
      seen.push(bar.hasAttribute('data-at-rest'))
    }
    addEventListener('scroll', read, { capture: true, passive: true })
    onTestFinished(() => removeEventListener('scroll', read, { capture: true }))
    const moved = await wheeled(60)
    removeEventListener('scroll', read, { capture: true })
    expect({ atRest, moved, first: seen[0] }).toEqual({
      atRest: true,
      moved: 60,
      first: false,
    })
  })

  it('rests again within 150 ms of a sideways wheel on the Continue row', async () => {
    const host = await app(440, 956)
    fourWindowsTall()
    const down = await wheeled(200)
    const up = await wheeled(-100)
    const bar = need(host, '.phone-bar')
    const row = need(host, '.continue ul')
    const atRest = { rest: await barRested(host), top: barTop(host) }
    const box = row.getBoundingClientRect()
    const x = box.left + box.width / 2
    const y = (Math.max(H, box.top) + Math.min(innerHeight, box.bottom)) / 2
    const hit = document.elementFromPoint(x, y)
    expect(hit !== null && row.contains(hit), 'over the row').toBe(true)
    await mouse.moveTo(x, y)
    const y0 = scrollY
    // After the hook's own listener, so its wait starts after the hook's.
    const after = new Promise<{ left: boolean; back: boolean }>((resolve) => {
      addEventListener(
        'wheel',
        () => {
          const left = !bar.hasAttribute('data-at-rest')
          setTimeout(async () => {
            await frames(2)
            resolve({ left, back: bar.hasAttribute('data-at-rest') })
          }, REST_IDLE_MS)
        },
        { once: true, passive: true },
      )
    })
    await mouse.wheel(0, 200)
    const { left, back } = await after
    expect({
      down,
      up,
      atRest,
      still: Math.abs(scrollY - y0) <= 0.5,
      sideways: row.scrollLeft > 0,
      left,
      back,
    }).toEqual({
      down: 200,
      up: -100,
      atRest: { rest: true, top: 0 },
      still: true,
      sideways: true,
      left: true,
      back: true,
    })
  })

  // Chromium ends the clamp's scroll with no `scrollend`; held off all the
  // same, so the rest read is the wait's on any browser.
  it('rests within 150 ms after the clamp of a page grown shorter, a scroll no input began, with part of the bar in view', async () => {
    const host = await app(440, 956)
    fourWindowsTall()
    await pinnedMidPage(host, 1500)
    const hid = await wheeled(2 * H)
    const shown = await wheeled(-26)
    const bar = need(host, '.phone-bar')
    const start = { rest: await barRested(host), top: barTop(host) }
    // Past the window in which a scroll counts as the wheel's.
    await new Promise((resolve) => setTimeout(resolve, 2 * REST_IDLE_MS))
    scrollNeverEnds()
    const y0 = scrollY
    // After the hook's own listener, so its wait starts after the hook's.
    const after = new Promise<{ left: boolean; back: boolean }>((resolve) => {
      addEventListener(
        'scroll',
        () => {
          const left = !bar.hasAttribute('data-at-rest')
          setTimeout(async () => {
            await frames(2)
            resolve({ left, back: bar.hasAttribute('data-at-rest') })
          }, REST_IDLE_MS)
        },
        { once: true, passive: true },
      )
    })
    for (const tile of host.querySelectorAll<HTMLElement>('main .grid > li')) {
      if (tile.getBoundingClientRect().top > innerHeight / 4)
        tile.style.display = 'none'
    }
    const { left, back } = await after
    expect({
      hid,
      shown,
      start,
      clamped: scrollY < y0,
      inView: barTop(host) > -H,
      left,
      back,
      position: getComputedStyle(bar).position,
    }).toEqual({
      hid: 2 * H,
      shown: -26,
      start: { rest: true, top: -(H - 26) },
      clamped: true,
      inView: true,
      left: true,
      back: true,
      position: 'relative',
    })
  })

  it('rests while the drawer is open and after it closes', async () => {
    const host = await app(440, 956)
    fourWindowsTall()
    await pinnedMidPage(host, 1500)
    const hid = await wheeled(2 * H)
    const shown = await wheeled(-30)
    expect({ hid, shown, top: barTop(host) }, '30 px in view').toEqual({
      hid: 2 * H,
      shown: -30,
      top: -(H - 30),
    })
    const library = need(host, '.library')
    await clickShown(need(host, '.menu-button'))
    await frames(2)
    const open = {
      open: library.classList.contains('menu-open'),
      ...drawn(host),
    }
    const scrim = need(host, '.scrim')
    const x = innerWidth - 20
    const y = innerHeight / 2
    expect(document.elementFromPoint(x, y), 'over the scrim').toBe(scrim)
    await clickAt(x, y)
    await idle()
    const closed = {
      open: library.classList.contains('menu-open'),
      ...drawn(host),
    }
    expect({ open, closed }).toEqual({
      open: { open: true, ...RESTS_PINNED },
      closed: { open: false, ...RESTS_PINNED },
    })
  })

  it('rests after Back lands at a kept place', async () => {
    const host = await app(440, 956)
    fourWindowsTall()
    await wheeledDownTo(1500)
    const y = scrollY
    const hidden = barTop(host) === -H
    await clickShown(coverInView(host))
    await idle()
    const series = location.pathname.startsWith('/series/')
    const down = await wheeled(600)
    // Read at the landing, after the bar's own listener: the scroll Back
    // restores ends with a `scrollend` here, and no input began it, so either
    // would rest the bar by the time Back is over, landing or none.
    const bar = need(host, '.phone-bar')
    const landed: boolean[] = []
    onTestFinished(
      onLanded(() => landed.push(bar.hasAttribute('data-at-rest'))),
    )
    await back()
    expect({
      hidden,
      series,
      down,
      back: Math.abs(scrollY - y) <= 1 ? 0 : scrollY - y,
      landed,
      ...drawn(host),
    }).toEqual({
      hidden: true,
      series: true,
      down: 600,
      back: 0,
      landed: [true],
      ...RESTS_PINNED,
    })
  })

  it('rests at the top of the page', async () => {
    const host = await app(440, 956)
    await frames(2)
    expect({ y: scrollY, ...drawn(host) }).toEqual({ y: 0, ...RESTS_PINNED })
  })
})

/** The first grid tile any of which is in view. */
function firstTileInView(host: HTMLElement): HTMLElement {
  const tile = [...host.querySelectorAll<HTMLElement>('main .grid > li')].find(
    (each) => each.getBoundingClientRect().bottom > 0,
  )
  if (tile === undefined) throw new Error('no tile in view')
  return tile
}

/** How far `top` is from `was`, as 0 within 1 px. */
const shiftFrom = (was: number, top: number) =>
  Math.abs(top - was) <= 1 ? 0 : top - was

/**
 * The browser holds the page by a box in view as content above it changes,
 * and the bar and its track, which that leaves where they are, are never
 * the box: the page's content keeps its place whatever the bar shows.
 */
describe('the phone bar upright, never what the page is held by', () => {
  it.each([
    ['pinned', [-20], true, { position: 'sticky', rest: false, top: 0 }],
    ['26 px shown', [26], true, { position: 'sticky', rest: false, top: -26 }],
    ['hidden', [2 * H], true, { position: 'sticky', rest: false, top: -H }],
    [
      'at rest, 26 px shown',
      [2 * H, -26],
      false,
      { position: 'relative', rest: true, top: -26 },
    ],
  ])(
    'keeps the content in place when content grows above the view, %s',
    async (_, wheels, onTrack, bar) => {
      const host = await app(440, 956)
      fourWindowsTall()
      await pinnedMidPage(host, 1500)
      // No scroll's end puts the bar to rest, so it stays on its track.
      if (onTrack) scrollNeverEnds()
      const moves: number[] = []
      for (const dy of wheels) moves.push(await wheeled(dy))
      await barRested(host)
      const block = document.createElement('div')
      need(host, 'main').prepend(block)
      onTestFinished(() => block.remove())
      await frames(2)
      const before = drawn(host)
      const tile = firstTileInView(host)
      const was = tile.getBoundingClientRect().top
      const tall = document.documentElement.scrollHeight
      block.style.height = '40px'
      await frames(2)
      await stillPage()
      expect({
        moved: movedBy(moves, wheels),
        bar: before,
        above: block.getBoundingClientRect().bottom <= 0,
        grown: document.documentElement.scrollHeight - tall,
        shift: shiftFrom(was, tile.getBoundingClientRect().top),
      }).toEqual({
        moved: wheels.map(() => true),
        bar,
        above: true,
        grown: 40,
        shift: 0,
      })
    },
  )

  // A wheel up brings the bar in to rest in view, and the page is held as
  // it is where the bar has stayed out: by the same browser, so no box the
  // browser holds the page by is named here.
  it('anchors a width-only resize with the bar in view as with it out of view', async () => {
    const host = await app(440, 956)
    fourWindowsTall()
    await wheeledDownTo(1500)
    const out = drawn(host)
    const control = await narrowed(host)
    const moves = [await wheeled(2 * H), await wheeled(-2 * H)]
    await barRested(host)
    const shown = drawn(host)
    const held = await narrowed(host)
    expect({
      moved: movedBy(moves, [2 * H, -2 * H]),
      out: out.top,
      shown,
      grid: [control.grid, held.grid],
      shift: shiftFrom(control.tile, held.tile),
    }).toEqual({
      moved: [true, true],
      out: -H,
      shown: RESTS_PINNED,
      grid: [true, true],
      shift: 0,
    })
  })

  it('holds the content in view through a width-only resize after the bar has come in and gone out again', async () => {
    const host = await app(440, 956)
    fourWindowsTall()
    await wheeledDownTo(1604)
    const moves = [await wheeled(-2 * H), await wheeled(2 * H)]
    await barRested(host)
    const out = drawn(host)
    const { tile, grid } = await narrowed(host)
    expect({
      moved: movedBy(moves, [-2 * H, 2 * H]),
      out: out.top,
      grid,
      shift: shiftFrom(0, tile),
    }).toEqual({ moved: [true, true], out: -H, grid: true, shift: 0 })
  })

  // Turned on its side the Library is far shorter, and the track's end, set
  // for the upright page's end, would hold the page up, blank under the bar,
  // in every frame until the scroll came down to the page's own end.
  it('leaves the page a turn makes shorter no taller than its content in any frame', async () => {
    const host = await app(440, 956)
    fourWindowsTall()
    await pinnedMidPage(host, maxScroll())
    const rest = await barRested(host)
    const y = scrollY
    const held: number[] = []
    eachFrame(() => {
      const content = need(host, '.library').getBoundingClientRect().height
      const doc = document.documentElement.scrollHeight
      held.push(doc - content <= 1 ? 0 : Math.round(doc - content))
    })
    await page.viewport(956, 440)
    await new Promise((resolve) => setTimeout(resolve, TURN_HOLD_MS))
    await idle()
    const each = [...held]
    const content = need(host, '.library').getBoundingClientRect().height
    expect({
      rest,
      shorter: content < y,
      frames: each.length > 10,
      held: each,
    }).toEqual({
      rest: true,
      shorter: true,
      frames: true,
      held: each.map(() => 0),
    })
  })
})

/**
 * Narrows the window from 440 to 400 wide, a resize in the phone layout
 * that turns no screen, so the browser holds the page and not the keeper
 * (the bar's hook counts any width change as a turn, whose hold is waited
 * out), and back once the page is still: how far the first tile in view
 * moved, and whether the grid's page offset changed with the width.
 */
async function narrowed(
  host: HTMLElement,
): Promise<{ tile: number; grid: boolean }> {
  const tile = firstTileInView(host)
  const grid = need(host, 'main .grid')
  const was = { tile: tile.getBoundingClientRect().top, grid: pageTop(grid) }
  const settle = async () => {
    await new Promise((resolve) => setTimeout(resolve, TURN_HOLD_MS))
    await idle()
  }
  await page.viewport(400, 956)
  await settle()
  const moved = {
    tile: tile.getBoundingClientRect().top - was.tile,
    grid: Math.abs(pageTop(grid) - was.grid) > 1,
  }
  await page.viewport(440, 956)
  await settle()
  return moved
}

/** An element's top in the page. */
const pageTop = (el: Element) => el.getBoundingClientRect().top + scrollY

describe('the phone bar on its side, leaving the phone layout', () => {
  it('starts shown on coming back to the phone layout', async () => {
    const host = await app(956, 440)
    fourWindowsTall()
    await pinnedMidPage(host, 1500)
    await wheeled(2 * H)
    expect(barTop(host), 'hidden mid-page').toBe(-H)
    await page.viewport(1280, 800)
    await idle()
    expect(need(host, '.phone-bar-track').style.height).toBe('')
    await page.viewport(956, 440)
    await new Promise(requestAnimationFrame)
    await new Promise(requestAnimationFrame)
    expect({ phone: scrollY > 4 * H, top: barTop(host) }).toEqual({
      phone: true,
      top: 0,
    })
  })
})

describe('the phone bar upright, around a screen that loads nothing', () => {
  it('commits no React render while a wheel scrolls the page', async () => {
    await page.viewport(440, 956)
    const commits: string[] = []
    const host = mount(
      <MemoryRouter>
        <Profiler id="shell" onRender={(_, phase) => commits.push(phase)}>
          <Shell>
            <div style={{ height: 4 * 956 }} />
          </Shell>
        </Profiler>
      </MemoryRouter>,
    )
    host.style.height = '100%'
    outsideAct()
    await idle()
    fourWindowsTall()
    await pinnedMidPage(host, 1500)
    commits.length = 0
    const { down, up } = await downAndUp()
    expect(down.length + up.length).toBeGreaterThanOrEqual(10)
    expect(commits).toEqual([])
  })
})
