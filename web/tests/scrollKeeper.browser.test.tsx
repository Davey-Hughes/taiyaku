import { act } from 'react'
import { BrowserRouter, type NavigateFunction, useNavigate } from 'react-router'
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
import { invalidateLibraryViews } from '../src/pageReads'
import { PHONE_LAYOUT, SIDEBAR_LAYOUT } from '../src/phoneLayout'
import { AppRoutes } from '../src/Routes'
import * as usersApi from '../src/users'
import { ABOUT } from './support/about'
import { deferred } from './support/async'
import { licenceTextsLanded } from './support/licence'
import { inProgress } from './support/continueRow'
import { BROWSE_KEY } from './support/homePage'
import { dragged, frameBox, frames, ramp, screenTurns } from './support/motion'
import { beforeUnmount, mount } from './support/mount'
import { mouse } from './support/mouse'

/**
 * Back to the Library view or a series page, and the reader's way out,
 * with real scrolling, under the router the app runs (`BrowserRouter`), at
 * a desktop's width, where `<main>` scrolls, and a phone's, where the
 * document does (spec `2026-10-28-loading-library-design.md` §5.2). Every
 * offset is read from both scrollers, so a case says which of the two moved as
 * well as how far.
 *
 * The app's routes are the real ones; the reader is a stand-in whose one
 * button is the exit the top bar's Library button, its key and the failed
 * load's page share (`Routes.tsx`'s `onLeave`).
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

/**
 * A cover that fails with no request: a data address that does not decode,
 * with the size `coverSources` adds or without it. A server's address would
 * go through the dev server's proxy, and a server answering there would
 * decide what the tiles draw.
 */
const FAILS = 'data:image/png;base64,!'

const series = (id: number): library.SeriesCard => ({
  id,
  title: `Kaguya-sama wa Kokurasetai ${id}`,
  works: 3,
  editions: 1,
  aligned: 0,
  added: id,
  unread: 3,
  cover: FAILS,
})

/**
 * Series enough that the grid is far taller than the window at either
 * width. Every cover is asked for and fails, so each tile in view swaps its
 * image for the placeholder after the scroll is put back, as a cold cover
 * does: the placeholder takes the image's room (`Cover.browser.test.tsx`),
 * so the scroll stays where it was put.
 */
const GRID: library.Grid = {
  series: Array.from({ length: 200 }, (_, i) => series(i + 1)),
  unfiled: [],
}

/** Volumes enough that a series page is far taller than the window too. */
const SERIES_PAGE = {
  series: { ...series(1), titles: [] },
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

/** A directory, the only one of library `id`, which is named `name`. */
const dir = (
  id: number,
  path: string,
  name: string,
): librariesApi.LibraryDir => ({
  id,
  path,
  edition_key: 'en',
  default_kind: 'manga',
  default_language: 'en',
  exclude_globs: null,
  scan_interval_secs: 86400,
  keeps_documents: false,
  last_scan_at: '2026-10-05T21:14:09Z',
  last_scan_status: 'ok',
  next_scan_at: null,
  paused_at: null,
  libraries: [{ id, name }],
  nested: [],
  inside: [],
})

beforeEach(() => {
  localStorage.clear()
  window.scrollTo(0, 0)
  vi.mocked(aboutApi.about).mockResolvedValue(ABOUT)
  vi.mocked(api.listKeys).mockResolvedValue(
    Array.from({ length: 30 }, (_, i) => ({
      id: i + 1,
      name: `key ${i + 1}`,
      created: '2026-10-01T00:00:00Z',
      last_used: null,
    })),
  )
  vi.mocked(api.listSessions).mockResolvedValue(
    Array.from({ length: 30 }, (_, i) => ({
      id: `s${i}`,
      created: '2026-10-01T00:00:00Z',
      last_seen: '2026-10-01T00:00:00Z',
      user_agent: 'Mozilla/5.0',
      ip: '10.0.0.1',
      current: i === 0,
    })),
  )
  vi.mocked(curation.listProposals).mockResolvedValue({
    proposals: [],
    counts: {},
  } as never)
  vi.mocked(config.scanStatus).mockImplementation((id) =>
    Promise.resolve({
      root_id: id,
      running: false,
      report: null,
      last_scan_at: null,
      last_scan_status: null,
    }),
  )
  vi.mocked(config.listRoots).mockResolvedValue({ roots: [] })
  vi.mocked(librariesApi.libraryScanStatus).mockImplementation((id) =>
    Promise.resolve({
      library_id: id,
      held_removals: [],
      moves_held: false,
      derive_running: false,
      scans: [],
      running: false,
      elapsed_secs: null,
      dirs: [],
    }),
  )
  vi.mocked(librariesApi.listLibraries).mockResolvedValue({
    libraries: Array.from({ length: 30 }, (_, i) => ({
      id: 10 + i,
      name: `Shelf ${i + 1}`,
      works: 0,
      books: 0,
      dirs: [dir(10 + i, `/srv/shelf ${i + 1}`, `Shelf ${i + 1}`)],
    })),
  })
  vi.mocked(usersApi.listUsers).mockResolvedValue({ users: [] })
  vi.mocked(library.status).mockResolvedValue({
    libraries: [{ id: 1, name: 'Volumes', scanned: true }],
    restricted: false,
  })
  vi.mocked(library.home).mockResolvedValue({
    continuing: [],
    added: [],
    read: [],
  })
  vi.mocked(library.grid).mockResolvedValue(GRID)
  vi.mocked(library.oneSeries).mockResolvedValue(SERIES_PAGE)
})

let go: NavigateFunction = () => {
  throw new Error('no router')
}

/** Real time for answers and effects to land, then a frame. */
async function settle(ms = 150) {
  await act(async () => {
    await new Promise((resolve) => setTimeout(resolve, ms))
  })
  await aFrame()
}

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

async function aFrame() {
  await act(async () => {
    await new Promise((resolve) => requestAnimationFrame(resolve))
  })
}

/**
 * The app's routes, signed in as an administrator, under `BrowserRouter`
 * at `path` in this frame's own history, which is put back when the test
 * ends, at a window `width` wide and `height` tall.
 */
async function app(
  width: number,
  path: string,
  height = 844,
): Promise<HTMLElement> {
  const before = location.href
  onTestFinished(() => {
    history.replaceState(null, '', before)
    history.scrollRestoration = 'auto'
  })
  history.replaceState(null, '', path)
  await page.viewport(width, height)
  // Declared here for the reason `Gate.test.tsx`'s `probe` gives.
  function Probe() {
    go = useNavigate()
    return null
  }
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
        <Probe />
        <AppRoutes />
      </GateContext.Provider>
    </BrowserRouter>,
  )
  // The height chain `base.css` gives `#root`, as `Shell.browser.test.tsx`'s
  // `at()` says.
  host.style.height = '100%'
  await settle()
  return host
}

/** A push, as a link within the app makes. */
async function open(to: string) {
  await act(async () => {
    await go(to)
  })
  await settle()
}

/**
 * Back, or Forward with `entries` 1, with the router's render held a task
 * so it runs after the browser has put back any scroll it keeps for the
 * entry, as `Shell.browser.test.tsx`'s `back()` does.
 */
async function traverse(entries = -1) {
  await act(async () => {
    const popped = new Promise((resolve) => {
      window.addEventListener('popstate', resolve, { once: true })
    })
    history.go(entries)
    await popped
    await new Promise((resolve) => setTimeout(resolve, 0))
  })
  await settle()
}

function need<T extends Element = HTMLElement>(
  host: ParentNode,
  selector: string,
): T {
  const found = host.querySelector<T>(selector)
  if (!found) throw new Error(`no ${selector}`)
  return found
}

/**
 * The element that scrolls in the layout drawn: `<main>` beside the sidebar,
 * the document in the phone layout.
 */
const scrollerAt = (host: HTMLElement) =>
  matchMedia(SIDEBAR_LAYOUT).matches
    ? need(host, 'main')
    : (document.scrollingElement as HTMLElement)

/** The reader scrolls the page to `y`, and the frame in which the browser reports the scroll passes. */
async function scroll(host: HTMLElement, y: number) {
  scrollerAt(host).scrollTop = y
  await aFrame()
  await aFrame()
}

/**
 * A real click on the link at `selector`, through the drawer in the phone
 * layout when the link is in it, with nothing in `act`, and the time `idle`
 * gives.
 */
async function follow(host: HTMLElement, selector: string) {
  if (
    matchMedia(PHONE_LAYOUT).matches &&
    need(host, selector).closest('#library-menu') !== null
  ) {
    await userEvent.click(need(host, 'button[aria-controls="library-menu"]'))
    await idle()
  }
  await userEvent.click(need(host, selector))
  await idle()
}

/** Back, by the browser's own history, with nothing in `act`. */
async function back() {
  const popped = new Promise((resolve) => {
    window.addEventListener('popstate', resolve, { once: true })
  })
  history.back()
  await popped
  await idle()
}

/** How far each of the two scrollers is scrolled: `<main>`, and the document. */
const scrolled = (host: HTMLElement) => [
  need(host, 'main').scrollTop,
  Math.round(scrollY),
]

/** `y` on the scroller in the layout drawn, and the other one at its top. */
const at = (y: number) => (matchMedia(SIDEBAR_LAYOUT).matches ? [y, 0] : [0, y])

const here = () => `${location.pathname}${location.search}`

describe.each([1280, 390])('the scroll keeper at %i px', (width) => {
  it('lands Back to the Library view at its scroll, and a series opened from it at the top', async () => {
    const host = await app(width, '/')
    await scroll(host, 1500)
    await open('/series/1')
    const opened = scrolled(host)
    await scroll(host, 600)
    await traverse()
    const restored = scrolled(host)
    // Saving goes on for the entry Back arrived at.
    await scroll(host, 800)
    await open('/series/1')
    await traverse()
    expect([opened, here(), restored, scrolled(host)]).toEqual([
      [0, 0],
      '/',
      at(1500),
      at(800),
    ])
  })

  it('lands Back to a series page at its scroll', async () => {
    const host = await app(width, '/series/1')
    await scroll(host, 1500)
    await open('/read/work-1')
    await traverse()
    expect([here(), scrolled(host)]).toEqual(['/series/1', at(1500)])
  })

  it('returns the reader’s Library button to the last Library view, at its scroll', async () => {
    const host = await app(width, '/')
    await act(async () => {
      need<HTMLButtonElement>(host, 'button.sort').click()
    })
    await scroll(host, 1500)
    await open('/series/1')
    await scroll(host, 600)
    await open('/read/work-1')
    await act(async () => {
      await userEvent.click(need(host, '[data-testid="reader"] button'))
    })
    await settle()
    expect([here(), scrolled(host)]).toEqual(['/?sort=added', at(1500)])
  })

  it('returns the reader’s Library button to the top of `/` where no Library view was drawn', async () => {
    const host = await app(width, '/series/1')
    await scroll(host, 1500)
    await open('/read/work-1')
    await act(async () => {
      await userEvent.click(need(host, '[data-testid="reader"] button'))
    })
    await settle()
    expect([here(), scrolled(host)]).toEqual(['/', [0, 0]])
  })

  it('lands the sidebar’s heading at the top of a Library view kept from before', async () => {
    const host = await app(width, '/')
    await scroll(host, 1500)
    await open('/series/1')
    await scroll(host, 1200)
    if (matchMedia(PHONE_LAYOUT).matches) {
      await act(async () => {
        await userEvent.click(
          need(host, 'button[aria-controls="library-menu"]'),
        )
      })
    }
    await act(async () => {
      await userEvent.click(need(host, '.sidebar-heading a'))
    })
    await settle()
    expect([here(), scrolled(host)]).toEqual(['/', [0, 0]])
  })

  it('lands a library’s link at the top of that library, kept from before', async () => {
    const host = await app(width, '/?library=1')
    await open('/')
    await scroll(host, 1500)
    if (matchMedia(PHONE_LAYOUT).matches) {
      await act(async () => {
        await userEvent.click(
          need(host, 'button[aria-controls="library-menu"]'),
        )
      })
    }
    await act(async () => {
      await userEvent.click(need(host, '.sidebar-libraries a'))
    })
    await settle()
    const switched = [here(), scrolled(host)]
    // Back within the one view, which stays drawn, and saving goes on there.
    await traverse()
    const back = [here(), scrolled(host)]
    await scroll(host, 800)
    await open('/series/1')
    await traverse()
    expect([switched, back, [here(), scrolled(host)]]).toEqual([
      ['/?library=1', [0, 0]],
      ['/', at(1500)],
      ['/', at(800)],
    ])
  })

  /**
   * **A link to the view already drawn lands at the top too.** The link to
   * the address shown is a replace, so the click adds no entry and Back
   * leaves the view; a press of the sort after it, a replace on the entry
   * the click made, keeps its scroll.
   */
  it('lands the sidebar’s link to the library drawn at the top, adding no entry, and a sort pressed after it where it is', async () => {
    const host = await app(width, '/series/1')
    const address = '/?library=1&show=unread&sort=added'
    await open(address)
    const landed = []
    globalThis.IS_REACT_ACT_ENVIRONMENT = false
    try {
      scrollerAt(host).scrollTop = 2500
      await idle()
      landed.push(scrolled(host))
      await follow(host, `.sidebar-libraries a[href="${address}"]`)
      landed.push([here(), scrolled(host)])
      scrollerAt(host).scrollTop = 1800
      await idle()
      // The DOM's click: a real one scrolls the button into view first.
      need<HTMLButtonElement>(host, 'button.sort').click()
      await idle()
      landed.push([here(), scrolled(host)])
      await back()
      landed.push(here())
    } finally {
      globalThis.IS_REACT_ACT_ENVIRONMENT = true
    }
    expect(landed).toEqual([
      at(2500),
      [address, [0, 0]],
      ['/?library=1&show=unread', at(1800)],
      '/series/1',
    ])
  })

  /**
   * The address a press writes is the one the sidebar's link names, whichever
   * of the sort and the chip was pressed first, so the link is a replace.
   * Real clicks, with nothing in `act`; the presses come before any scroll,
   * so a click scrolling its button into view moves nothing.
   */
  it('lands the sidebar’s link to the library drawn at the top, adding no entry, after the sort is pressed before a chip', async () => {
    const host = await app(width, '/series/1')
    await open('/?library=1')
    const address = '/?library=1&show=unread&sort=added'
    const unread = () => {
      const chip = [
        ...host.querySelectorAll<HTMLButtonElement>(
          '.library-chip-group button',
        ),
      ].find((button) => button.textContent === 'Unread')
      if (!chip) throw new Error('no Unread chip')
      return chip
    }
    const landed = []
    globalThis.IS_REACT_ACT_ENVIRONMENT = false
    try {
      await userEvent.click(need(host, 'button.sort'))
      await idle()
      await userEvent.click(unread())
      await idle()
      landed.push(here())
      scrollerAt(host).scrollTop = 2500
      await idle()
      landed.push(scrolled(host))
      const entries = history.length
      await follow(host, '.sidebar-libraries a')
      landed.push([here(), scrolled(host), history.length - entries])
      await back()
      landed.push(here())
    } finally {
      globalThis.IS_REACT_ACT_ENVIRONMENT = true
    }
    expect(landed).toEqual([
      address,
      at(2500),
      [address, [0, 0], 0],
      '/series/1',
    ])
  })

  /**
   * Each way to `/` lands at its top with `/` drawn, as a push there does:
   * the sidebar's heading and the column's wordmark at either width, and the
   * phone bar's wordmark where the bar is drawn, in the phone layout. The
   * cases are listed before any window is sized, so the width picks them:
   * `app` draws the phone layout at 390 px and the sidebar's at 1280.
   */
  it.each([
    ['the sidebar’s heading', '.sidebar-heading a'],
    ['the column’s wordmark', '.sidebar-column > .wordmark-home'],
    ...(width < 900 ? [['the phone bar’s wordmark', '.phone-bar .home']] : []),
  ])('lands %s at the top of `/` drawn, adding no entry', async (_, link) => {
    const host = await app(width, '/series/1')
    await open('/')
    const landed = []
    globalThis.IS_REACT_ACT_ENVIRONMENT = false
    try {
      scrollerAt(host).scrollTop = 2500
      await idle()
      landed.push(scrolled(host))
      await follow(host, link)
      landed.push([here(), scrolled(host)])
      await back()
      landed.push(here())
    } finally {
      globalThis.IS_REACT_ACT_ENVIRONMENT = true
    }
    expect(landed).toEqual([at(2500), ['/', [0, 0]], '/series/1'])
  })

  it('leaves a scroll alone when the sort is pressed after a restore, and the reader’s Library button returns to the sorted view', async () => {
    const host = await app(width, '/')
    await scroll(host, 1500)
    await open('/series/1')
    await traverse()
    const restored = scrolled(host)
    await scroll(host, 900)
    await act(async () => {
      need<HTMLButtonElement>(host, 'button.sort').click()
    })
    await settle()
    const pressed = [here(), scrolled(host)]
    await open('/read/work-1')
    await act(async () => {
      await userEvent.click(need(host, '[data-testid="reader"] button'))
    })
    await settle()
    expect([restored, pressed, [here(), scrolled(host)]]).toEqual([
      at(1500),
      ['/?sort=added', at(900)],
      ['/?sort=added', at(900)],
    ])
  })

  it('restores a body drawn after a wait once it draws', async () => {
    const host = await app(width, '/')
    await scroll(host, 1500)
    await open('/series/1')
    const reread = deferred<library.Grid>()
    vi.mocked(library.grid).mockReturnValueOnce(reread.promise)
    act(() => {
      invalidateLibraryViews()
    })
    await traverse()
    const waiting = [
      host.querySelector('main ul.grid') === null,
      scrolled(host),
    ]
    await act(async () => {
      reread.resolve(GRID)
    })
    await settle()
    expect([waiting, here(), scrolled(host)]).toEqual([
      [true, [0, 0]],
      '/',
      at(1500),
    ])
  })

  it('restores a series page drawn after a wait once it draws', async () => {
    const host = await app(width, '/series/1')
    await scroll(host, 1500)
    await open('/read/work-1')
    const reread = deferred<Awaited<ReturnType<typeof library.oneSeries>>>()
    vi.mocked(library.oneSeries).mockReturnValueOnce(reread.promise)
    act(() => {
      invalidateLibraryViews()
    })
    await traverse()
    const waiting = [
      host.querySelector('main .volumes') === null,
      scrolled(host),
    ]
    await act(async () => {
      reread.resolve(SERIES_PAGE)
    })
    await settle()
    expect([waiting, here(), scrolled(host)]).toEqual([
      [true, [0, 0]],
      '/series/1',
      at(1500),
    ])
  })

  /**
   * **Navigations outside `act`, as the app makes them**, round after round:
   * `BrowserRouter` commits each in a transition, whose passive effects run
   * in a task of their own, so a frame can come between the commit and the
   * cleanup of the view left. The series is kept, so its page is drawn and
   * put at the top in the commit itself, before that frame. Real clicks on a
   * tile in view, and the browser's own Back.
   */
  it('lands Back to the Library view at its scroll round after round, with nothing in act', async () => {
    const host = await app(width, '/series/60')
    await open('/')
    const tile = () => need(host, 'ul.grid a[href="/series/60"]')
    const scroller = scrollerAt(host)
    const top =
      tile().getBoundingClientRect().top -
      (matchMedia(SIDEBAR_LAYOUT).matches
        ? scroller.getBoundingClientRect().top
        : 0) +
      scroller.scrollTop
    const landed = []
    const wanted = []
    globalThis.IS_REACT_ACT_ENVIRONMENT = false
    try {
      for (let round = 0; round < 8; round++) {
        const y = Math.round(top - 300 + round * 7)
        scroller.scrollTop = y
        await idle()
        await userEvent.click(tile())
        await idle()
        scrollerAt(host).scrollTop = 600 + round
        await idle()
        const popped = new Promise((resolve) => {
          window.addEventListener('popstate', resolve, { once: true })
        })
        history.back()
        await popped
        await idle()
        landed.push([here(), scrolled(host)])
        wanted.push(['/', at(y)])
      }
    } finally {
      globalThis.IS_REACT_ACT_ENVIRONMENT = true
    }
    expect(landed).toEqual(wanted)
  })

  it('restores a search once its hits draw, not once the grid does', async () => {
    vi.mocked(library.search).mockResolvedValue({ ...GRID, query: 'kaguya' })
    const host = await app(width, '/?q=kaguya')
    await scroll(host, 1500)
    await open('/series/1')
    const reread = deferred<library.SearchHits>()
    vi.mocked(library.search).mockReturnValueOnce(reread.promise)
    act(() => {
      invalidateLibraryViews()
    })
    await traverse()
    const waiting = [
      host.querySelector('main ul.grid') === null,
      scrolled(host),
    ]
    await act(async () => {
      reread.resolve({ ...GRID, query: 'kaguya' })
    })
    await settle()
    expect([waiting, here(), scrolled(host)]).toEqual([
      [true, [0, 0]],
      '/?q=kaguya',
      at(1500),
    ])
  })

  it('leaves the scroll alone when the quiet re-read lands after the restore', async () => {
    const host = await app(width, '/')
    await scroll(host, 1500)
    await open('/series/1')
    const reread = deferred<library.Grid>()
    vi.mocked(library.grid).mockReturnValueOnce(reread.promise)
    await traverse()
    const restored = scrolled(host)
    await scroll(host, 700)
    await act(async () => {
      reread.resolve({ ...GRID, series: [series(0), ...GRID.series] })
    })
    await settle()
    expect([restored, scrolled(host)]).toEqual([at(1500), at(700)])
  })

  it('lands Forward to the Library view at its scroll after Back to a page that keeps no scroll of its own', async () => {
    const host = await app(width, '/account')
    await scroll(host, 400)
    await open('/')
    await scroll(host, 1500)
    await traverse()
    const back = [here(), scrolled(host)]
    await traverse(1)
    expect([back, [here(), scrolled(host)]]).toEqual([
      ['/account', matchMedia(SIDEBAR_LAYOUT).matches ? [1500, 0] : [0, 400]],
      ['/', at(1500)],
    ])
  })

  it('leaves the browser’s restoration off while the Library view is drawn after the licence page', async () => {
    await app(width, '/about/licence')
    await licenceTextsLanded()
    const modes = [history.scrollRestoration]
    await open('/')
    modes.push(history.scrollRestoration)
    await open('/about')
    modes.push(history.scrollRestoration)
    expect(modes).toEqual(['manual', 'manual', 'auto'])
  })
})

/**
 * **Back onto a page that keeps no scroll lands where the browser puts it**:
 * the page is left scrolled to 400 for one of these views or for About,
 * which is scrolled to 200 before Back. At a phone's width the browser puts
 * back the document's 400; at a desktop's it restores nothing, and `<main>`,
 * which the shell keeps across routes, is where the page left it.
 */
describe.each([1280, 390])('Back onto another page at %i px', (width) => {
  it.each([
    ['/config/libraries', '/'],
    ['/config/libraries', '/series/1'],
    ['/config/libraries', '/about'],
    ['/account', '/'],
    ['/account', '/series/1'],
    ['/account', '/about'],
  ])(
    'lands %s, left for %s, with `<main>` where the next page left it beside the sidebar, and the document where it was in the phone layout',
    async (from, to) => {
      const host = await app(width, from)
      await scroll(host, 400)
      await open(to)
      await scroll(host, 200)
      await traverse()
      expect([here(), scrolled(host)]).toEqual([
        from,
        matchMedia(SIDEBAR_LAYOUT).matches ? [200, 0] : [0, 400],
      ])
    },
  )

  /**
   * A view arrived at by Back from a page that keeps no scroll is drawn on
   * an entry it turned the browser's restoration off for, and a push from
   * it makes an entry that starts off too: the view must turn it on for the
   * entry it leaves for, whatever it found, or Back to that page restores
   * nothing. The licence page turns it off for its own entries the same way.
   */
  it.each(['/', '/about/licence'])(
    'lands /account, opened from %s arrived at by Back, with `<main>` where the next page left it beside the sidebar, and the document where it was in the phone layout',
    async (view) => {
      const host = await app(width, view)
      await open('/about')
      await traverse()
      await open('/account')
      await scroll(host, 400)
      await open('/about')
      await scroll(host, 200)
      await traverse()
      expect([here(), scrolled(host)]).toEqual([
        '/account',
        matchMedia(SIDEBAR_LAYOUT).matches ? [200, 0] : [0, 400],
      ])
    },
  )
})

/** The view's top: `<main>`'s beside the sidebar, the phone bar's foot in the phone layout. */
function lineOf(host: HTMLElement): number {
  if (!matchMedia(SIDEBAR_LAYOUT).matches) {
    return need(host, '.phone-bar').getBoundingClientRect().bottom
  }
  const main = need(host, 'main')
  return main.getBoundingClientRect().top + main.clientTop
}

/** Within a pixel: 0, or how far off. */
const near = (actual: number, expected: number) =>
  Math.abs(actual - expected) <= 1 ? 0 : actual - expected

/**
 * What the view holds at its top: in a cover list the first cover on the
 * top row seen, elsewhere the deepest element in the flow the top crosses,
 * first in the page's order, or the first below it.
 */
function anchorOf(host: HTMLElement): Element {
  const line = lineOf(host)
  let within: Element = need(host, 'main')
  for (;;) {
    if (within.matches(':is(.grid, .volumes)')) {
      const covers = [...within.children]
      const seen = covers.find((li) => li.getBoundingClientRect().bottom > line)
      if (seen === undefined) return within
      const { top } = seen.getBoundingClientRect()
      return covers.find(
        (li) => Math.abs(li.getBoundingClientRect().top - top) < 0.5,
      ) as Element
    }
    const next = [...within.children].find(
      (child) =>
        child.getBoundingClientRect().bottom > line &&
        ['static', 'relative'].includes(getComputedStyle(child).position),
    )
    if (next === undefined) return within
    if (next.getBoundingClientRect().top >= line) return next
    within = next
  }
}

/** Waits for the page to stop moving: a wheel's scroll is animated. */
async function rested(host: HTMLElement) {
  let still = 0
  let was = scrolled(host).join()
  while (still < 3) {
    await frames(1)
    const now = scrolled(host).join()
    still = now === was ? still + 1 : 0
    was = now
  }
}

/** Turns the mouse's wheel over the page until `share` of its way down. */
async function wheeled(host: HTMLElement, share: number) {
  const scroller = scrollerAt(host)
  const goal = (scroller.scrollHeight - scroller.clientHeight) * share
  for (let turn = 0; scroller.scrollTop < goal - 1 && turn < 20; turn++) {
    await userEvent.wheel(need(host, 'main'), {
      delta: { y: Math.ceil(goal - scroller.scrollTop) },
    })
    await rested(host)
  }
}

/**
 * How far down `el` the view's top falls, as a share of its height: 0
 * where its top is at or below the view's top.
 */
function shareOf(host: HTMLElement, el: Element): number {
  const { top, height } = el.getBoundingClientRect()
  const line = lineOf(host)
  return line > top ? (line - top) / height : 0
}

/**
 * Whether the anchor rule picks `el` at the view's top: `el`, or any
 * element inside it, which the rule descends to where `el`'s top stands
 * even a fraction of a px above the top; or a cover in `el`'s list whose
 * top is within half a px of `el`'s. The rule picks the first cover of a
 * row, and a cover's column changes with the layout.
 */
function picks(host: HTMLElement, el: Element): boolean {
  const picked = anchorOf(host)
  if (picked === el || el.contains(picked)) return true
  return (
    picked.parentElement === el.parentElement &&
    picked.matches(':is(.grid, .volumes) > li') &&
    Math.abs(
      picked.getBoundingClientRect().top - el.getBoundingClientRect().top,
    ) < 0.5
  )
}

/** A frame `<main>` is laid at a new size in, as painted. */
interface Laid {
  phone: boolean
  /** Whether the anchor rule picks the anchor in this frame (`picks`). */
  same: boolean
  /** How far below where its share of its height puts it the anchor stands. */
  off: number
  view: number[]
  atEnd: boolean
}

/**
 * Each frame `<main>` is laid at a new size in, read by an observer made
 * after the shell's, so after the shell's has put the place back and
 * before the frame is painted. `share` is the anchor's at the start.
 */
function watched(host: HTMLElement, anchor: () => Element, share = 0): Laid[] {
  const seen: Laid[] = []
  const observer = new ResizeObserver(() => {
    const scroller = scrollerAt(host)
    const el = anchor()
    const { top, height } = el.getBoundingClientRect()
    seen.push({
      phone: matchMedia(PHONE_LAYOUT).matches,
      same: picks(host, el),
      off: top - (lineOf(host) - share * height),
      view: scrolled(host),
      atEnd:
        scroller.scrollTop >= scroller.scrollHeight - scroller.clientHeight - 1,
    })
  })
  observer.observe(need(host, 'main'))
  beforeUnmount(() => observer.disconnect())
  return seen
}

/** Each layout's first frame: its layout, the same anchor, and within a px of its place. */
const held = (seen: Laid[]) =>
  entered(seen).map((frame) => [frame.phone, frame.same, near(frame.off, 0)])

/** The first frame drawn in each layout a drag entered. */
const entered = (seen: Laid[]) =>
  seen.filter((frame, i) => i > 0 && frame.phone !== seen[i - 1].phone)

/** A drag from `from` to `to` and back, a step a frame, the other side held. */
const across = (side: 'width' | 'height', from: number, to: number) =>
  [...ramp(from, to, 20), ...ramp(to, from, 20)].map((size) => ({
    [side]: size,
  }))

const PAGES = ['/', '/?q=kaguya', '/series/1', '/config/libraries']

/**
 * The app at `path` in a window 1280 by 800, with nothing in `act` from
 * here on, as the app runs: a wheel and a drag draw covers as they come
 * into view.
 */
async function drawnAt(path: string): Promise<HTMLElement> {
  const host = await app(1280, path, 800)
  globalThis.IS_REACT_ACT_ENVIRONMENT = false
  onTestFinished(() => {
    globalThis.IS_REACT_ACT_ENVIRONMENT = true
  })
  return host
}

describe('the place across the scroller swap, from 1280 × 800', () => {
  beforeEach(() => {
    vi.mocked(library.search).mockResolvedValue({ ...GRID, query: 'kaguya' })
  })

  it.each(PAGES)(
    'keeps the element at the top through a width drag to 880 px and back, on %s',
    async (path) => {
      const host = await drawnAt(path)
      await wheeled(host, 0.5)
      const away = scrolled(host)[0] > 200
      const anchor = anchorOf(host)
      const seen = watched(host, () => anchor, shareOf(host, anchor))
      const drawn = await dragged(across('width', 1280, 880))
      // The fixture tells: the page stood away from its top, and the drag
      // reached 880 px.
      expect({
        away,
        narrowest: Math.min(...drawn.map(({ width }) => width)),
        held: held(seen),
      }).toEqual({
        away: true,
        narrowest: 880,
        held: [
          [true, true, 0],
          [false, true, 0],
        ],
      })
    },
  )

  it.each(PAGES)(
    'keeps the element at the top through a height drag to 480 px and back, on %s',
    async (path) => {
      const host = await drawnAt(path)
      await wheeled(host, 0.5)
      const away = scrolled(host)[0] > 200
      const anchor = anchorOf(host)
      const seen = watched(host, () => anchor, shareOf(host, anchor))
      const drawn = await dragged(across('height', 800, 480))
      expect({
        away,
        shortest: Math.min(...drawn.map(({ height }) => height)),
        held: held(seen),
      }).toEqual({
        away: true,
        shortest: 480,
        held: [
          [true, true, 0],
          [false, true, 0],
        ],
      })
    },
  )

  it('keeps the same anchor over ten crossings', async () => {
    const host = await drawnAt('/')
    await wheeled(host, 0.5)
    const anchor = anchorOf(host)
    const seen = watched(host, () => anchor, shareOf(host, anchor))
    for (let round = 0; round < 5; round++) {
      await dragged(across('width', 1280, 880))
    }
    expect(held(seen)).toEqual(
      Array.from({ length: 10 }, (_, i) => [i % 2 === 0, true, 0]),
    )
  })

  it('leaves a page at its top at its top', async () => {
    const host = await drawnAt('/')
    const seen = watched(host, () => need(host, 'main'))
    for (let round = 0; round < 5; round++) {
      await dragged(across('width', 1280, 880))
    }
    expect(entered(seen).map((frame) => [frame.phone, frame.view])).toEqual(
      Array.from({ length: 10 }, (_, i) => [i % 2 === 0, [0, 0]]),
    )
  })

  // Scrolled to its end in the phone layout, where the page is the longer
  // of the two, so the sidebar's layout cannot scroll that far.
  it('holds a page scrolled to its end over ten crossings', async () => {
    const host = await drawnAt('/config/libraries')
    await dragged(ramp(1280, 880, 20).map((width) => ({ width })))
    await wheeled(host, 1)
    const anchor = anchorOf(host)
    const share = shareOf(host, anchor)
    // How far short of its share the page's end held the anchor at the
    // start, where the way back lands it again.
    const { top, height } = anchor.getBoundingClientRect()
    const short = top - (lineOf(host) - share * height)
    const seen = watched(host, () => anchor, share)
    for (let round = 0; round < 5; round++) {
      await dragged(across('width', 880, 1280))
    }
    // The drag's last step crosses: its frame's observers run after it ends.
    await frames(2)
    const views = entered(seen)
    expect({
      held: views.map((frame) => {
        if (frame.same && near(frame.off, short) === 0) return 'in place'
        return frame.atEnd && frame.off > short
          ? 'at its end'
          : frame.off - short
      }),
      again: views
        .slice(2)
        .map((frame, i) =>
          frame.view.map((y, side) => near(y, views[i].view[side])),
        ),
    }).toEqual({
      held: Array.from({ length: 10 }, (_, i) =>
        i % 2 === 0 ? 'at its end' : 'in place',
      ),
      again: Array.from({ length: 8 }, () => [0, 0]),
    })
  })

  it('lands Back after a swap at the place put back', async () => {
    const host = await drawnAt('/')
    await wheeled(host, 0.5)
    const anchor = anchorOf(host)
    const share = shareOf(host, anchor)
    const href = need(anchor, 'a').getAttribute('href')
    await dragged(ramp(1280, 880, 20).map((width) => ({ width })))
    await idle()
    const line = lineOf(host)
    // A tile wholly in view, clear of the phone bar.
    const tile = [...host.querySelectorAll('main .grid > li a')].find((a) => {
      const { top, bottom } = a.getBoundingClientRect()
      return top > line + 50 && bottom < innerHeight - 50
    }) as HTMLElement
    await userEvent.click(tile)
    await idle()
    const opened = here()
    await back()
    const again = need(host, `main .grid > li a[href="${href}"]`).closest(
      'li',
    ) as Element
    const { top, height } = again.getBoundingClientRect()
    expect({
      opened,
      back: here(),
      phone: matchMedia(PHONE_LAYOUT).matches,
      same: picks(host, again),
      off: near(top, lineOf(host) - share * height),
    }).toEqual({
      opened: tile.getAttribute('href'),
      back: '/',
      phone: true,
      same: true,
      off: 0,
    })
  })

  // At 880 a cover fills its column and stands taller than at 1280, where
  // it is exact: kept as a distance in px, this one would be put back
  // wholly above the view.
  it('keeps a cover the top crosses in its last few px at 880 at the top in the first frame beside the sidebar, widening to 1280', async () => {
    const host = await drawnAt('/')
    await dragged(ramp(1280, 880, 20).map((width) => ({ width })))
    // The drag's last step crosses: its frame's observers run after it ends.
    await frames(2)
    const covers = host.querySelectorAll('main .grid > li')
    const cover = covers[Math.floor(covers.length / 2)]
    const scroller = scrollerAt(host)
    scroller.scrollTop +=
      cover.getBoundingClientRect().bottom - 8 - lineOf(host)
    await frames(3)
    const anchor = anchorOf(host)
    const into = Math.round(
      anchor.getBoundingClientRect().bottom - lineOf(host),
    )
    const seen = watched(host, () => anchor, shareOf(host, anchor))
    await dragged(ramp(880, 1280, 20).map((width) => ({ width })))
    await frames(2)
    expect({ into, held: held(seen) }).toEqual({
      into: 8,
      held: [[false, true, 0]],
    })
  })

  // The Continue row lays a new column in the swap's frame, after the
  // shell's observer, and its new height moves a place inside it, which a
  // page that does not anchor leaves where it moved.
  it.each([
    ['where the page does not anchor', false],
    ['where the page anchors', true],
  ])(
    'keeps a sliver of the Continue row at the top through a width drag to 880 px and back, %s',
    async (_, anchors) => {
      vi.mocked(library.home).mockResolvedValue(HOME)
      const host = await drawnAt('/')
      const roots = [
        document.documentElement,
        document.body,
        need(host, 'main'),
      ]
      for (const root of roots)
        root.style.overflowAnchor = anchors ? '' : 'none'
      onTestFinished(() => {
        for (const root of roots) root.style.overflowAnchor = ''
      })
      const row = need(host, '.continue ul')
      await topAt(host, row, row.getBoundingClientRect().height - 40)
      const anchor = anchorOf(host)
      const seen = watched(host, () => anchor, shareOf(host, anchor))
      const drawn = await dragged(across('width', 1280, 880))
      expect({
        inRow: row.contains(anchor),
        narrowest: Math.min(...drawn.map(({ width }) => width)),
        held: held(seen),
      }).toEqual({
        inRow: true,
        narrowest: 880,
        held: [
          [true, true, 0],
          [false, true, 0],
        ],
      })
    },
  )
})

/**
 * A phone at `path` with `coverSize` px covers, held upright (440 × 956) or
 * on its side (956 × 440), its screen turned to match, and nothing in `act`
 * from here on. The page anchors its scroll or does not, as `anchors` says:
 * the keeper's put-back must land the same whichever a phone's browser
 * does through a turn.
 */
async function phoneAt(
  path: string,
  {
    upright,
    coverSize,
    anchors,
  }: { upright: boolean; coverSize: number; anchors: boolean },
) {
  localStorage.setItem(BROWSE_KEY, JSON.stringify({ coverSize }))
  // The screen first, as a phone's faces one way from the start: the
  // keeper counts a turn by it.
  const turn = await screenTurns()
  await turn(upright ? 'portrait-primary' : 'landscape-primary')
  const host = await app(upright ? 440 : 956, path, upright ? 956 : 440)
  const roots = [document.documentElement, document.body]
  for (const root of roots) root.style.overflowAnchor = anchors ? '' : 'none'
  onTestFinished(() => {
    for (const root of roots) root.style.overflowAnchor = ''
  })
  globalThis.IS_REACT_ACT_ENVIRONMENT = false
  onTestFinished(() => {
    globalThis.IS_REACT_ACT_ENVIRONMENT = true
  })
  await idle()
  let held = upright
  /**
   * Turns the phone: its screen, then the frame's box in one task, as iOS
   * turns the screen ahead of the new size. iOS sets its own scroll with
   * the new size (`lands`: at the page's top, at the same share of its
   * height, or at the same offset as far as the page allows) and sends the
   * resize event ahead of the frame's observers; Chromium sends its own
   * after them, so the test sets that scroll and sends the event here.
   * Gives the size laid, whether the scroll landed there, whether the
   * page's end clamped it, and `kept`'s anchor in the first frame laid.
   */
  const turned = async (lands: 'top' | 'share' | 'offset', kept: Kept) => {
    const root = document.documentElement
    const was = { y: scrollY, height: root.scrollHeight }
    held = !held
    await turn(held ? 'portrait-primary' : 'landscape-primary')
    const first = firstFrame(host, kept)
    const box = frameBox()
    box.style.width = `${held ? 440 : 956}px`
    box.style.height = `${held ? 956 : 440}px`
    const target = {
      top: 0,
      share: Math.round((was.y * root.scrollHeight) / was.height),
      offset: Math.round(was.y),
    }[lands]
    const end = root.scrollHeight - innerHeight
    scrollTo(0, target)
    const landed = {
      size: `${innerWidth} × ${innerHeight}`,
      ios: scrollOf() === Math.min(target, end),
      clamped: target > end,
    }
    dispatchEvent(new Event('resize'))
    await idle()
    return { ...landed, first: await first }
  }
  return { host, turned }
}

/** The page's scroll, as the document's. */
const scrollOf = () => Math.round(scrollY)

/** Scrolls the page so that `el`'s top stands `above` px above the view's top. */
async function topAt(host: HTMLElement, el: Element, above: number) {
  const scroller = scrollerAt(host)
  scroller.scrollTop += el.getBoundingClientRect().top - lineOf(host) + above
  await frames(2)
}

/**
 * The place the view holds on `anchor`: how far down it the view's top
 * falls, as a share of its height, or how far below the top its top
 * stands where the top does not cross it.
 */
interface Kept {
  anchor: Element
  share: number
  below: number
}

const keptOf = (host: HTMLElement, anchor: Element): Kept => {
  const { top } = anchor.getBoundingClientRect()
  const line = lineOf(host)
  return {
    anchor,
    share: shareOf(host, anchor),
    below: Math.max(0, top - line),
  }
}

/** How many covers `list` lays in its first row. */
function columnsOf(list: Element): number {
  const tops = [...list.children].map((li) => li.getBoundingClientRect().top)
  return tops.filter((top) => Math.abs(top - tops[0]) < 0.5).length
}

/** How far `kept`'s anchor stands from its place: 0 within a px. */
function offOf(host: HTMLElement, { anchor, share, below }: Kept): number {
  const { top, height } = anchor.getBoundingClientRect()
  return near(top, lineOf(host) - share * height + below)
}

/** Where `kept`'s anchor stands: the same anchor, and within a px of its place. */
const placed = (host: HTMLElement, kept: Kept) => ({
  same: picks(host, kept.anchor),
  off: offOf(host, kept),
})

/**
 * Where `kept`'s anchor stands in the next frame, the first laid at a new
 * size, by an observer made after the shell's and the Continue row's: after
 * every put-back, before the frame is painted. A put-back a frame late is
 * the one iOS paints over, as the frame the size is committed with.
 */
function firstFrame(host: HTMLElement, kept: Kept) {
  return new Promise<{ off: number; atEnd: boolean }>((resolve) => {
    const observer = new ResizeObserver(() => {
      observer.disconnect()
      const root = document.documentElement
      resolve({
        off: offOf(host, kept),
        atEnd: root.scrollTop >= root.scrollHeight - innerHeight - 1,
      })
    })
    observer.observe(need(host, 'main'))
    beforeUnmount(() => observer.disconnect())
  })
}

/**
 * The home page's grid cover the cases start at: the first of its row
 * with 3 columns upright, and in the middle of a row with 6 on its side,
 * where the anchor rule picks another cover, so a place read again there
 * comes back a row out.
 */
const MIDDLE = 63

const HOME = {
  continuing: inProgress(8),
  added: [],
  read: [],
}

describe.each([
  { anchors: false, where: 'where the page does not anchor' },
  { anchors: true, where: 'where the page anchors' },
])('the place across a phone turn, $where', ({ anchors }) => {
  beforeEach(() => {
    vi.mocked(library.home).mockResolvedValue(HOME)
  })

  it('keeps a sliver of the Continue row at the top, turned upright', async () => {
    const { host, turned } = await phoneAt('/', {
      upright: false,
      coverSize: 121,
      anchors,
    })
    const row = need(host, '.continue ul')
    await topAt(host, row, row.getBoundingClientRect().height - 40)
    const sliver = Math.round(row.getBoundingClientRect().bottom - lineOf(host))
    const kept = keptOf(host, anchorOf(host))
    const landed = await turned('top', kept)
    expect({
      sliver,
      inRow: row.contains(kept.anchor),
      landed,
      ...placed(host, kept),
      seen: row.getBoundingClientRect().bottom > lineOf(host),
    }).toEqual({
      sliver: 40,
      inRow: true,
      landed: {
        size: '440 × 956',
        ios: true,
        clamped: false,
        first: { off: 0, atEnd: false },
      },
      same: true,
      off: 0,
      seen: true,
    })
  })

  it.each(['top', 'share'] as const)(
    'keeps the cover at the top on its side, iOS landing at the %s',
    async (lands) => {
      const { host, turned } = await phoneAt('/', {
        upright: true,
        coverSize: 121,
        anchors,
      })
      const cover = host.querySelectorAll('main .grid > li')[MIDDLE]
      await topAt(host, cover, 30)
      const picked = anchorOf(host) === cover
      const kept = keptOf(host, cover)
      const landed = await turned(lands, kept)
      expect({
        picked,
        landed,
        ...placed(host, kept),
      }).toEqual({
        picked: true,
        landed: {
          size: '956 × 440',
          ios: true,
          clamped: false,
          first: { off: 0, atEnd: false },
        },
        same: true,
        off: 0,
      })
    },
  )

  it.each(['top', 'share'] as const)(
    'lands a turn and its return where the reader started, iOS landing at the %s',
    async (lands) => {
      const { host, turned } = await phoneAt('/', {
        upright: true,
        coverSize: 121,
        anchors,
      })
      const cover = host.querySelectorAll('main .grid > li')[MIDDLE]
      await topAt(host, cover, 30)
      const start = scrollOf()
      const kept = keptOf(host, cover)
      const there = await turned(lands, kept)
      const sideways = anchorOf(host) !== cover
      const back = await turned(lands, kept)
      expect({
        sideways,
        first: [there.first.off, back.first.off],
        back: near(scrollOf(), start),
        ...placed(host, kept),
      }).toEqual({
        sideways: true,
        first: [0, 0],
        back: 0,
        same: true,
        off: 0,
      })
    },
  )

  it.each(['top', 'share', 'offset'] as const)(
    'lands a turn and its return where the reader started on a series page, iOS landing at the %s',
    async (lands) => {
      const { host, turned } = await phoneAt('/series/1', {
        upright: true,
        coverSize: 96,
        anchors,
      })
      // The first of its row with 3 columns upright, and in the middle of
      // a row with 8 on its side, as `MIDDLE` is; the page on its side ends
      // above its offset upright.
      const cover = host.querySelectorAll('main .volumes > li')[36]
      await topAt(host, cover, 20)
      const start = scrollOf()
      const kept = keptOf(host, cover)
      const there = await turned(lands, kept)
      const sideways = anchorOf(host) !== cover
      const back = await turned(lands, kept)
      expect({
        clamped: there.clamped,
        sideways,
        first: [there.first.off, back.first.off],
        back: near(scrollOf(), start),
        ...placed(host, kept),
      }).toEqual({
        clamped: lands === 'offset',
        sideways: true,
        first: [0, 0],
        back: 0,
        same: true,
        off: 0,
      })
    },
  )

  it.each(['top', 'share'] as const)(
    'lands a turn and its return where the reader started where the page’s end stops the put-back, iOS landing at the %s',
    async (lands) => {
      vi.mocked(library.oneSeries).mockResolvedValue({
        ...SERIES_PAGE,
        works: SERIES_PAGE.works.slice(0, 33),
      })
      const { host, turned } = await phoneAt('/series/1', {
        upright: false,
        coverSize: 96,
        anchors,
      })
      // On its side 8 columns: cover 24 leads the fourth row of five, the
      // last holding one cover. Upright, with 3 columns, it stands within
      // three rows of the end, nearer than the view is tall.
      const cover = host.querySelectorAll('main .volumes > li')[24]
      await topAt(host, cover, 20)
      const start = scrollOf()
      const picked = anchorOf(host) === cover
      const kept = keptOf(host, cover)
      const upright = await turned(lands, kept)
      const stopped = upright.first.atEnd && upright.first.off > 1
      const back = await turned(lands, kept)
      expect({
        picked,
        stopped,
        first: back.first.off,
        back: near(scrollOf(), start),
        ...placed(host, kept),
      }).toEqual({
        picked: true,
        stopped: true,
        first: 0,
        back: 0,
        same: true,
        off: 0,
      })
    },
  )

  it('keeps a cover whose top stands below the view’s top as far below it, through a turn and its return', async () => {
    const { host, turned } = await phoneAt('/', {
      upright: false,
      coverSize: 121,
      anchors,
    })
    // The first of its row with 6 columns on its side, and with 3 upright;
    // its top 10 px below the view's, which falls in the gap above its row.
    const cover = host.querySelectorAll('main .grid > li')[60]
    await topAt(host, cover, -10)
    const start = scrollOf()
    const picked = anchorOf(host) === cover
    const kept = keptOf(host, cover)
    const there = await turned('top', kept)
    const upright = placed(host, kept)
    const back = await turned('top', kept)
    expect({
      picked,
      below: Math.round(kept.below),
      first: [there.first.off, back.first.off],
      upright,
      back: near(scrollOf(), start),
      ...placed(host, kept),
    }).toEqual({
      picked: true,
      below: 10,
      first: [0, 0],
      upright: { same: true, off: 0 },
      back: 0,
      same: true,
      off: 0,
    })
  })

  it('keeps the place the reader scrolled to after a turn on the way back', async () => {
    const { host, turned } = await phoneAt('/', {
      upright: true,
      coverSize: 121,
      anchors,
    })
    const cover = host.querySelectorAll('main .grid > li')[MIDDLE]
    await topAt(host, cover, 30)
    const start = scrollOf()
    await turned('top', keptOf(host, cover))
    const turnedTo = scrollOf()
    // Over the view's middle: `<main>`'s is far below it.
    await mouse.moveTo(innerWidth / 2, innerHeight / 2)
    await mouse.wheel(300)
    await rested(host)
    const scrolled = scrollOf() - turnedTo
    const kept = keptOf(host, anchorOf(host))
    const back = await turned('top', kept)
    expect({
      scrolled: scrolled > 200,
      first: back.first.off,
      ...placed(host, kept),
      start: near(scrollOf(), start) === 0,
    }).toEqual({ scrolled: true, first: 0, same: true, off: 0, start: false })
  })

  // With no Continue row, whose own sideways scroll at a step the keeper
  // sees as a scroll, and no cover images, of which those a step brings
  // into view would fail and swap in their placeholders, a change in the
  // page the keeper sees too: the step is all that moves the page, as it
  // is on a phone whose covers load.
  it('keeps the place a Cover size step leaves after a turn on the way back', async () => {
    vi.mocked(library.home).mockResolvedValue({ ...HOME, continuing: [] })
    vi.mocked(library.grid).mockResolvedValue({
      ...GRID,
      series: GRID.series.map((card) => ({ ...card, cover: null })),
    })
    const { host, turned } = await phoneAt('/', {
      upright: true,
      coverSize: 121,
      anchors,
    })
    // Opened at the page's top by a real click, which focuses the page
    // too; the panel stays open as the page scrolls and turns.
    await userEvent.click(page.getByRole('button', { name: 'View' }))
    const slider = page.getByRole('slider', { name: /cover size/i }).element()
    const grid = need(host, 'main .grid')
    const cover = grid.children[MIDDLE]
    await topAt(host, cover, 30)
    const start = scrollOf()
    const kept = keptOf(host, cover)
    await turned('top', kept)
    const columns = [columnsOf(grid)]
    // Stepped from where the page stands, which a reader on a phone with
    // a keyboard can do: the slider keeps its focus as the page scrolls.
    ;(slider as HTMLElement).focus({ preventScroll: true })
    await userEvent.keyboard('{ArrowLeft}')
    await idle()
    columns.push(columnsOf(grid))
    const moved = offOf(host, kept) !== 0
    const steppedTo = keptOf(host, anchorOf(host))
    const back = await turned('top', steppedTo)
    const returned = {
      first: back.first.off,
      ...placed(host, steppedTo),
      start: near(scrollOf(), start) === 0,
    }
    // The step ends one pairing, not every one after it: a further turn
    // and its return, with nothing between, land where this return did.
    const returnedTo = scrollOf()
    const there = await turned('top', steppedTo)
    const again = await turned('top', steppedTo)
    expect({
      columns,
      moved,
      returned,
      again: {
        first: [there.first.off, again.first.off],
        ...placed(host, steppedTo),
        back: near(scrollOf(), returnedTo),
      },
    }).toEqual({
      columns: [6, 7],
      moved: true,
      returned: { first: 0, same: true, off: 0, start: false },
      again: { first: [0, 0], same: true, off: 0, back: 0 },
    })
  })
})

describe('the place through a desktop window dragged across square', () => {
  beforeEach(() => {
    vi.mocked(library.home).mockResolvedValue(HOME)
  })

  // The screen does not turn, so the keeper puts nothing back and the
  // browser anchors the page, as it does for any other drag.
  it('ends where it started, from 800 × 700 in the phone layout to 600 wide and back', async () => {
    localStorage.setItem(BROWSE_KEY, JSON.stringify({ coverSize: 121 }))
    const host = await app(800, '/', 700)
    globalThis.IS_REACT_ACT_ENVIRONMENT = false
    onTestFinished(() => {
      globalThis.IS_REACT_ACT_ENVIRONMENT = true
    })
    await idle()
    await topAt(host, host.querySelectorAll('main .grid > li')[MIDDLE], 30)
    const anchor = anchorOf(host)
    const start = anchor.getBoundingClientRect().top
    const facing = screen.orientation.type
    const drawn = await dragged(across('width', 800, 600))
    await idle()
    const widths = drawn.map(({ width }) => width)
    expect({
      phone: matchMedia(PHONE_LAYOUT).matches,
      widths: [Math.min(...widths), widths[widths.length - 1]],
      facing: screen.orientation.type === facing,
      end: near(anchor.getBoundingClientRect().top, start),
    }).toEqual({ phone: true, widths: [600, 800], facing: true, end: 0 })
  })
})
