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
import { AppRoutes } from '../src/Routes'
import * as usersApi from '../src/users'
import { ABOUT } from './support/about'
import { deferred } from './support/async'
import { licenceTextsLanded } from './support/licence'
import { mount } from './support/mount'

/**
 * Back to the Library view or a series page, and the reader's way out,
 * with real scrolling, under the router the app runs (`BrowserRouter`), at
 * a desktop's width, where `<main>` scrolls, and a phone's, where the
 * document does (spec 2026-10-28 §5.2). Every offset is read from both
 * scrollers, so a case says which of the two moved as well as how far.
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

const dir = (id: number, path: string): config.Root => ({
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
      dirs: [dir(10 + i, `/srv/shelf ${i + 1}`)],
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
 * ends, at a window `width` wide.
 */
async function app(width: number, path: string): Promise<HTMLElement> {
  const before = location.href
  onTestFinished(() => {
    history.replaceState(null, '', before)
    history.scrollRestoration = 'auto'
  })
  history.replaceState(null, '', path)
  await page.viewport(width, 844)
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

/** The element that scrolls at `width`: `<main>` at 900 px and wider. */
const scrollerAt = (host: HTMLElement, width: number) =>
  width >= 900 ? need(host, 'main') : (document.scrollingElement as HTMLElement)

/** The reader scrolls the page to `y`, and the frame in which the browser reports the scroll passes. */
async function scroll(host: HTMLElement, width: number, y: number) {
  scrollerAt(host, width).scrollTop = y
  await aFrame()
  await aFrame()
}

/**
 * A real click on the link at `selector`, through the phone's menu below
 * 900 px when the link is in it, with nothing in `act`, and the time `idle`
 * gives.
 */
async function follow(host: HTMLElement, width: number, selector: string) {
  if (width < 900 && need(host, selector).closest('#library-menu') !== null) {
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

/** `y` on the scroller at `width`, and the other one at its top. */
const at = (width: number, y: number) => (width >= 900 ? [y, 0] : [0, y])

const here = () => `${location.pathname}${location.search}`

describe.each([1280, 390])('the scroll keeper at %i px', (width) => {
  it('lands Back to the Library view at its scroll, and a series opened from it at the top', async () => {
    const host = await app(width, '/')
    await scroll(host, width, 1500)
    await open('/series/1')
    const opened = scrolled(host)
    await scroll(host, width, 600)
    await traverse()
    const restored = scrolled(host)
    // Saving goes on for the entry Back arrived at.
    await scroll(host, width, 800)
    await open('/series/1')
    await traverse()
    expect([opened, here(), restored, scrolled(host)]).toEqual([
      [0, 0],
      '/',
      at(width, 1500),
      at(width, 800),
    ])
  })

  it('lands Back to a series page at its scroll', async () => {
    const host = await app(width, '/series/1')
    await scroll(host, width, 1500)
    await open('/read/work-1')
    await traverse()
    expect([here(), scrolled(host)]).toEqual(['/series/1', at(width, 1500)])
  })

  it('returns the reader’s Library button to the last Library view, at its scroll', async () => {
    const host = await app(width, '/')
    await act(async () => {
      need<HTMLButtonElement>(host, 'button.sort').click()
    })
    await scroll(host, width, 1500)
    await open('/series/1')
    await scroll(host, width, 600)
    await open('/read/work-1')
    await act(async () => {
      await userEvent.click(need(host, '[data-testid="reader"] button'))
    })
    await settle()
    expect([here(), scrolled(host)]).toEqual(['/?sort=added', at(width, 1500)])
  })

  it('returns the reader’s Library button to the top of `/` where no Library view was drawn', async () => {
    const host = await app(width, '/series/1')
    await scroll(host, width, 1500)
    await open('/read/work-1')
    await act(async () => {
      await userEvent.click(need(host, '[data-testid="reader"] button'))
    })
    await settle()
    expect([here(), scrolled(host)]).toEqual(['/', [0, 0]])
  })

  it('lands the sidebar’s heading at the top of a Library view kept from before', async () => {
    const host = await app(width, '/')
    await scroll(host, width, 1500)
    await open('/series/1')
    await scroll(host, width, 1200)
    if (width < 900) {
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
    await scroll(host, width, 1500)
    if (width < 900) {
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
    await scroll(host, width, 800)
    await open('/series/1')
    await traverse()
    expect([switched, back, [here(), scrolled(host)]]).toEqual([
      ['/?library=1', [0, 0]],
      ['/', at(width, 1500)],
      ['/', at(width, 800)],
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
      scrollerAt(host, width).scrollTop = 2500
      await idle()
      landed.push(scrolled(host))
      await follow(host, width, `.sidebar-libraries a[href="${address}"]`)
      landed.push([here(), scrolled(host)])
      scrollerAt(host, width).scrollTop = 1800
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
      at(width, 2500),
      [address, [0, 0]],
      ['/?library=1&show=unread', at(width, 1800)],
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
      scrollerAt(host, width).scrollTop = 2500
      await idle()
      landed.push(scrolled(host))
      const entries = history.length
      await follow(host, width, '.sidebar-libraries a')
      landed.push([here(), scrolled(host), history.length - entries])
      await back()
      landed.push(here())
    } finally {
      globalThis.IS_REACT_ACT_ENVIRONMENT = true
    }
    expect(landed).toEqual([
      address,
      at(width, 2500),
      [address, [0, 0], 0],
      '/series/1',
    ])
  })

  /**
   * Each way to `/` lands at its top with `/` drawn, as a push there does:
   * the sidebar's heading and the column's wordmark at either width, and the
   * phone bar's wordmark where the bar is drawn, below 900 px.
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
      scrollerAt(host, width).scrollTop = 2500
      await idle()
      landed.push(scrolled(host))
      await follow(host, width, link)
      landed.push([here(), scrolled(host)])
      await back()
      landed.push(here())
    } finally {
      globalThis.IS_REACT_ACT_ENVIRONMENT = true
    }
    expect(landed).toEqual([at(width, 2500), ['/', [0, 0]], '/series/1'])
  })

  it('leaves a scroll alone when the sort is pressed after a restore, and the reader’s Library button returns to the sorted view', async () => {
    const host = await app(width, '/')
    await scroll(host, width, 1500)
    await open('/series/1')
    await traverse()
    const restored = scrolled(host)
    await scroll(host, width, 900)
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
      at(width, 1500),
      ['/?sort=added', at(width, 900)],
      ['/?sort=added', at(width, 900)],
    ])
  })

  it('restores a body drawn after a wait once it draws', async () => {
    const host = await app(width, '/')
    await scroll(host, width, 1500)
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
      at(width, 1500),
    ])
  })

  it('restores a series page drawn after a wait once it draws', async () => {
    const host = await app(width, '/series/1')
    await scroll(host, width, 1500)
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
      at(width, 1500),
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
    const scroller = scrollerAt(host, width)
    const top =
      tile().getBoundingClientRect().top -
      (width >= 900 ? scroller.getBoundingClientRect().top : 0) +
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
        scrollerAt(host, width).scrollTop = 600 + round
        await idle()
        const popped = new Promise((resolve) => {
          window.addEventListener('popstate', resolve, { once: true })
        })
        history.back()
        await popped
        await idle()
        landed.push([here(), scrolled(host)])
        wanted.push(['/', at(width, y)])
      }
    } finally {
      globalThis.IS_REACT_ACT_ENVIRONMENT = true
    }
    expect(landed).toEqual(wanted)
  })

  it('restores a search once its hits draw, not once the grid does', async () => {
    vi.mocked(library.search).mockResolvedValue({ ...GRID, query: 'kaguya' })
    const host = await app(width, '/?q=kaguya')
    await scroll(host, width, 1500)
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
      at(width, 1500),
    ])
  })

  it('leaves the scroll alone when the quiet re-read lands after the restore', async () => {
    const host = await app(width, '/')
    await scroll(host, width, 1500)
    await open('/series/1')
    const reread = deferred<library.Grid>()
    vi.mocked(library.grid).mockReturnValueOnce(reread.promise)
    await traverse()
    const restored = scrolled(host)
    await scroll(host, width, 700)
    await act(async () => {
      reread.resolve({ ...GRID, series: [series(0), ...GRID.series] })
    })
    await settle()
    expect([restored, scrolled(host)]).toEqual([
      at(width, 1500),
      at(width, 700),
    ])
  })

  it('lands Forward to the Library view at its scroll after Back to a page that keeps no scroll of its own', async () => {
    const host = await app(width, '/account')
    await scroll(host, width, 400)
    await open('/')
    await scroll(host, width, 1500)
    await traverse()
    const back = [here(), scrolled(host)]
    await traverse(1)
    expect([back, [here(), scrolled(host)]]).toEqual([
      ['/account', width >= 900 ? [1500, 0] : [0, 400]],
      ['/', at(width, 1500)],
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
    'lands %s, left for %s, with `<main>` where the next page left it at 900 px and wider, and the document where it was below',
    async (from, to) => {
      const host = await app(width, from)
      await scroll(host, width, 400)
      await open(to)
      await scroll(host, width, 200)
      await traverse()
      expect([here(), scrolled(host)]).toEqual([
        from,
        width >= 900 ? [200, 0] : [0, 400],
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
    'lands /account, opened from %s arrived at by Back, with `<main>` where the next page left it at 900 px and wider, and the document where it was below',
    async (view) => {
      const host = await app(width, view)
      await open('/about')
      await traverse()
      await open('/account')
      await scroll(host, width, 400)
      await open('/about')
      await scroll(host, width, 200)
      await traverse()
      expect([here(), scrolled(host)]).toEqual([
        '/account',
        width >= 900 ? [200, 0] : [0, 400],
      ])
    },
  )
})
