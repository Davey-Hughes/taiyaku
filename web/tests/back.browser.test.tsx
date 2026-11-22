import { act } from 'react'
import { BrowserRouter } from 'react-router'
import { beforeEach, describe, expect, it, onTestFinished, vi } from 'vitest'
import { page, userEvent } from 'vitest/browser'
import '../src/index.css'
import * as aboutApi from '../src/about'
import * as api from '../src/api'
import type { AppProps } from '../src/App'
import { forgetBack } from '../src/back'
import * as bookApi from '../src/book'
import type { Book } from '../src/book'
import * as config from '../src/config'
import * as curation from '../src/curation'
import { GateContext } from '../src/gate'
import * as librariesApi from '../src/libraries'
import * as library from '../src/library'
import { SIDEBAR_LAYOUT } from '../src/phoneLayout'
import { wasmResolver } from '../src/resolver'
import { AppRoutes } from '../src/Routes'
import { ABOUT } from './support/about'
import { deferred } from './support/async'
import { inProgress } from './support/continueRow'
import { fakeResolver, whole } from './support/fixtures'
import { beforeUnmount, mount, rerender } from './support/mount'
import { coarseRulesOn } from './support/pointer'

/**
 * Back goes where you came from, else the library: the reader's way out and
 * the series page's Back, under the router the app runs (`BrowserRouter`),
 * with real clicks and nothing in `act`, at a desktop's width, where
 * `<main>` scrolls, and a phone's, where the document does.
 *
 * The routes and the book reader are the real ones; the comic reader is the
 * real `App` where a case reads its pages, place or keys, and elsewhere a
 * stand-in drawing the real top bar with what `Routes.tsx` hands `App`, which
 * `App.test.tsx` pins `App` hands on.
 */

/**
 * Whether the comic reader drawn is the real `App` rather than the stand-in:
 * the cases on its link to its series read its pages and its place.
 */
const comic = vi.hoisted(() => ({ real: false }))

vi.mock('../src/App', async (importOriginal) => {
  const { TopBar } = await import('../src/TopBar')
  const { default: App } = await importOriginal<typeof import('../src/App')>()
  return {
    default: (props: AppProps) =>
      comic.real ? (
        <App {...props} />
      ) : (
        <div data-testid="reader">
          <TopBar
            editionLine="Raw"
            settingsOpen={false}
            onSettings={() => {}}
            onLeave={props.onLeave}
            leaveWord={props.leaveTo?.word}
          />
        </div>
      ),
  }
})
// The real `App` loads without the generated wasm package, as
// `App.browser.test.tsx` has it.
vi.mock('taiyaku-wasm', () => ({
  default: async () => undefined,
  WasmSidecar: {
    fromJson: () => {
      throw new Error(
        'taiyaku-wasm is mocked: nothing here may build a real sidecar',
      )
    },
  },
}))
vi.mock('../src/resolver', () => ({ wasmResolver: vi.fn() }))
vi.mock('../src/about', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/about')>()),
  about: vi.fn(),
}))
// A page or a strip tile is an address that fails with no request, one for
// each page, so nothing asks a server that is not there.
vi.mock('../src/api', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/api')>()),
  getNotes: vi.fn(),
  getProgress: vi.fn(),
  getSidecarWithEtag: vi.fn(),
  pageUrl: (_: string, edition: string, page: number) =>
    `data:image/png;base64,!${edition}-${page}`,
  thumbUrl: (_: string, edition: string, page: number) =>
    `data:image/png;base64,!${edition}-${page}-thumb`,
}))
vi.mock('../src/book', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/book')>()),
  getBook: vi.fn(),
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
vi.mock('../src/library', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/library')>()),
  status: vi.fn(),
  home: vi.fn(),
  grid: vi.fn(),
  oneSeries: vi.fn(),
  search: vi.fn(),
  work: vi.fn(),
}))

/** A cover that fails with no request, as `scrollKeeper.browser.test.tsx` draws its tiles. */
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

/** Volume `n`, read in the book reader where `book` says so. */
const volume = (n: number, book = false): library.WorkCard => ({
  id: n,
  slug: `work-${n}`,
  title: `Kaguya-sama wa Kokurasetai v${n}`,
  volume: n,
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
      ...(book && { book: true }),
    },
  ],
  opens: 'raw',
})

/** Series enough that the grid is far taller than the window at either width. */
const GRID: library.Grid = {
  series: Array.from({ length: 200 }, (_, i) => series(i + 1)),
  unfiled: [],
}

/** A series page of `count` volumes, each read in the book reader where `book` says so. */
const seriesPage = (count: number, book = false) => ({
  series: { ...series(1), titles: [] },
  works: Array.from({ length: count }, (_, i) => volume(i + 1, book)),
})

beforeEach(() => {
  comic.real = false
  localStorage.clear()
  window.scrollTo(0, 0)
  vi.mocked(aboutApi.about).mockResolvedValue(ABOUT)
  vi.mocked(api.getProgress).mockResolvedValue(undefined)
  vi.mocked(api.getNotes).mockResolvedValue({
    doc: { work: 'work-1', notes: [] },
    etag: '"n"',
  } as never)
  vi.mocked(bookApi.getBook).mockRejectedValue(new Error('loading work-1: 503'))
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
  vi.mocked(librariesApi.listLibraries).mockResolvedValue({ libraries: [] })
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
  vi.mocked(library.oneSeries).mockResolvedValue(seriesPage(80))
  vi.mocked(library.search).mockImplementation((q) =>
    Promise.resolve({ ...GRID, query: q }),
  )
  vi.mocked(library.work).mockResolvedValue(filedWork())
})

/** Real time for answers and effects to land, then a frame. */
async function settle(ms = 150) {
  await act(async () => {
    await new Promise((resolve) => setTimeout(resolve, ms))
  })
  await act(async () => {
    await new Promise((resolve) => requestAnimationFrame(resolve))
  })
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

/** The app's routes, signed in as an administrator, under `BrowserRouter` at the address the frame is at. */
async function draw(): Promise<HTMLElement> {
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
  await settle()
  return host
}

/**
 * The app at `path` on a fresh entry of this frame's own history, which is
 * put back when the test ends, at a window `width` wide and 844 tall. The
 * entry holds no state, so the router takes it for the tab's first.
 */
async function app(width: number, path: string): Promise<HTMLElement> {
  const before = location.href
  onTestFinished(() => {
    history.replaceState(null, '', before)
    history.scrollRestoration = 'auto'
  })
  history.replaceState(null, '', path)
  await page.viewport(width, 844)
  return draw()
}

function need<T extends Element = HTMLElement>(
  host: ParentNode,
  selector: string,
): T {
  const found = host.querySelector<T>(selector)
  if (!found) throw new Error(`no ${selector}`)
  return found
}

/** The element that scrolls in the layout drawn. */
const scrollerAt = (host: HTMLElement) =>
  matchMedia(SIDEBAR_LAYOUT).matches
    ? need(host, 'main')
    : (document.scrollingElement as HTMLElement)

/** How far each of the two scrollers is scrolled: `<main>`, and the document. */
const scrolled = (host: HTMLElement) => [
  need(host, 'main').scrollTop,
  Math.round(scrollY),
]

/** `y` on the scroller in the layout drawn, and the other one at its top. */
const at = (y: number) => (matchMedia(SIDEBAR_LAYOUT).matches ? [y, 0] : [0, y])

const here = () => `${location.pathname}${location.search}`

/** The entry's place in the router's history: 0 for the first the app drew. */
const idx = () => (history.state as { idx: number }).idx

/** The top bar's way out of the comic reader. */
const readerBack = (host: HTMLElement) =>
  need(host, '[data-testid="reader"] .reader-back')

/** The series page's Back. */
const pageBack = (host: HTMLElement) => need(host, 'main .page-back')

/** The scroll that puts `link` 300 px below the top of the scroller, in view at either width. */
function above(host: HTMLElement, link: Element): number {
  const scroller = scrollerAt(host)
  return Math.round(
    link.getBoundingClientRect().top -
      (matchMedia(SIDEBAR_LAYOUT).matches
        ? scroller.getBoundingClientRect().top
        : 0) +
      scroller.scrollTop -
      300,
  )
}

/** Runs `steps` with nothing in `act`, as the app runs. */
async function outsideAct<T>(steps: () => Promise<T>): Promise<T> {
  globalThis.IS_REACT_ACT_ENVIRONMENT = false
  try {
    return await steps()
  } finally {
    globalThis.IS_REACT_ACT_ENVIRONMENT = true
  }
}

describe.each([1280, 390])('Back at %i px', (width) => {
  /**
   * Grid, series, reader, and Back twice, three rounds: each Back is the
   * browser's own, so the entry index falls by one, the history grows by
   * none, and the grid lands at the scroll it was left at.
   */
  it('goes from the reader to the series page and from there to the grid, each a step back, round after round', async () => {
    const host = await app(width, '/')
    const tile = () => need(host, 'ul.grid a[href="/series/60"]')
    const top = above(host, tile())
    await outsideAct(async () => {
      for (let round = 0; round < 3; round++) {
        const y = top + round * 7
        scrollerAt(host).scrollTop = y
        await idle()
        await userEvent.click(tile())
        await idle()
        await userEvent.click(need(host, '.volumes a[href="/read/work-1"]'))
        await idle()
        const entries = history.length
        const from = idx()
        const word = readerBack(host).textContent
        await userEvent.click(readerBack(host))
        await idle()
        expect(
          [
            word,
            here(),
            idx() - from,
            history.length - entries,
            host.querySelector('main .page-back')?.textContent,
          ],
          `round ${round + 1}, the reader's way out`,
        ).toEqual(['Series', '/series/60', -1, 0, 'Library'])
        await userEvent.click(pageBack(host))
        await idle()
        expect(
          [here(), scrolled(host), idx() - from, history.length - entries],
          `round ${round + 1}, the series page's Back`,
        ).toEqual(['/', at(y), -2, 0])
      }
    })
  })

  it('goes from a book that would not open to the series page, and from there to the grid, each a step back', async () => {
    vi.mocked(library.oneSeries).mockResolvedValue(seriesPage(80, true))
    const host = await app(width, '/')
    const tile = () => need(host, 'ul.grid a[href="/series/60"]')
    const y = above(host, tile())
    const landed = await outsideAct(async () => {
      scrollerAt(host).scrollTop = y
      await idle()
      await userEvent.click(tile())
      await idle()
      await userEvent.click(need(host, '.volumes a[href="/book/work-1"]'))
      await idle()
      const out = need(host, '.book-reader .reader-button')
      const book = [
        here(),
        need(host, '.book-failed').textContent,
        out.textContent,
      ]
      const from = idx()
      await userEvent.click(out)
      await idle()
      expect([
        book,
        [
          here(),
          idx() - from,
          host.querySelector('main .page-back')?.textContent,
        ],
      ]).toEqual([
        [
          '/book/work-1',
          'This book could not be opened: Error: loading work-1: 503',
          'Back to the series',
        ],
        ['/series/60', -1, 'Library'],
      ])
      await userEvent.click(pageBack(host))
      await idle()
      return [here(), scrolled(host), idx() - from]
    })
    expect(landed).toEqual(['/', at(y), -2])
  })

  /**
   * A query typed into the Library view, a hit opened, and Back: the hits
   * are drawn again, each tile under its own title, with the query in the
   * box, at the scroll they were left at.
   */
  it('goes from a reader opened from search hits back to the hits, the query in the box, at the scroll left', async () => {
    const hits: library.Grid = {
      series: GRID.series.slice(0, 100),
      unfiled: Array.from({ length: 60 }, (_, i) => volume(i + 1)),
    }
    vi.mocked(library.search).mockImplementation((q) =>
      Promise.resolve({ ...hits, query: q }),
    )
    const host = await app(width, '/')
    const titles = () =>
      [...host.querySelectorAll('.search-hits .grid > li .title')].map(
        (title) => title.textContent,
      )
    const landed = await outsideAct(async () => {
      await userEvent.type(need(host, 'input[type="search"]'), 'kaguya')
      await idle()
      const hit = () => need(host, '.search-hits a[href="/read/work-3"]')
      scrollerAt(host).scrollTop = above(host, hit())
      await idle()
      // Where the hits were left, once the covers that fail have drawn their
      // boxes: the scroll the page settles at, not the one asked for.
      const left = scrolled(host)
      await userEvent.click(hit())
      await idle()
      const from = idx()
      const entries = history.length
      const word = readerBack(host).textContent
      await userEvent.click(readerBack(host))
      await idle()
      return {
        left,
        word,
        back: [
          here(),
          need<HTMLInputElement>(host, 'input[type="search"]').value,
          scrolled(host),
          idx() - from,
          history.length - entries,
        ],
        titles: titles(),
      }
    })
    const far = Math.max(...landed.left)
    expect([landed, far > 2000]).toEqual([
      {
        left: at(far),
        word: 'Search',
        back: ['/?q=kaguya', 'kaguya', at(far), -1, 0],
        titles: [
          ...hits.series.map((s) => s.title),
          ...hits.unfiled.map((w) => w.title),
        ],
      },
      true,
    ])
  })

  /**
   * The reader's button as drawn in the first commit that shows the reader,
   * read by an observer of the route's root as that commit lands, not
   * waited on: it already names where it goes.
   */
  it('names where it goes in the first frame the reader draws', async () => {
    vi.mocked(library.grid).mockResolvedValue({
      series: GRID.series.slice(0, 20),
      unfiled: [volume(1)],
    })
    const host = await app(width, '/')
    const firstDrawn: (string | null)[] = []
    const observer = new MutationObserver(() => {
      const button = host.querySelector('[data-testid="reader"] .reader-back')
      if (button === null || firstDrawn.length > 0) return
      firstDrawn.push(button.textContent)
    })
    observer.observe(host, { childList: true, subtree: true })
    beforeUnmount(() => observer.disconnect())
    await outsideAct(async () => {
      await userEvent.click(need(host, 'ul.grid a[href="/read/work-1"]'))
      await idle()
    })
    expect([firstDrawn, readerBack(host).textContent]).toEqual([
      ['Library'],
      'Library',
    ])
  })

  /**
   * A reload keeps the entry index the router wrote, and forgets the record
   * of where each entry was pushed from: the button cannot name the series
   * page, and still goes back to it.
   */
  it('goes a step back after a reload, called Back', async () => {
    const host = await app(width, '/series/1')
    await outsideAct(async () => {
      await userEvent.click(need(host, '.volumes a[href="/read/work-1"]'))
      await idle()
    })
    const before = readerBack(host).textContent
    forgetBack()
    rerender(null)
    const reloaded = await draw()
    const landed = await outsideAct(async () => {
      const word = readerBack(reloaded).textContent
      await userEvent.click(readerBack(reloaded))
      await idle()
      return [word, here(), idx()]
    })
    expect([before, landed]).toEqual(['Series', ['Back', '/series/1', 0]])
  })

  /**
   * Opened cold, on the app's first entry in the tab, after a page that is
   * not the app's, there is nothing to go back to in the app: the way out
   * pushes the library, at its top, and never steps back out to that page,
   * whose entry carries no index of the router's.
   */
  it.each([
    ['the reader', '/read/work-1', readerBack],
    ['the series page', '/series/1', pageBack],
  ])(
    'pushes the library from %s opened cold, never stepping back out of the app',
    async (_, path, button) => {
      history.pushState(null, '', location.href)
      const host = await app(width, path)
      const landed = await outsideAct(async () => {
        const word = button(host).textContent
        await userEvent.click(button(host))
        await idle()
        const entry = (history.state as { idx?: number } | null)?.idx
        return [word, here(), entry, scrolled(host)]
      })
      expect(landed).toEqual(['Library', '/', 1, [0, 0]])
    },
  )

  /**
   * The Continue row's card, a progress save still out as the reader goes,
   * and Back: the row is drawn from what was kept until the save settles,
   * and then read again, in the order the server gives once it has.
   */
  it('goes from a reader opened from the Continue row back to the row, read again once the last progress save settles', async () => {
    const [first, second, third] = inProgress(3)
    vi.mocked(library.home)
      .mockResolvedValueOnce({
        continuing: [first, second, third],
        added: [],
        read: [],
      })
      .mockResolvedValue({
        continuing: [second, first, third],
        added: [],
        read: [],
      })
    const host = await app(width, '/')
    const cards = () =>
      [...host.querySelectorAll('.continue li')].map((card) =>
        card.querySelector('a')?.getAttribute('href'),
      )
    const saved = deferred<Response>()
    const fetches = vi.spyOn(window, 'fetch')
    onTestFinished(() => fetches.mockRestore())
    const landed = await outsideAct(async () => {
      await userEvent.click(
        need(host, `.continue a[href="/read/${second.slug}"]`),
      )
      await idle()
      fetches.mockImplementationOnce(() => saved.promise)
      void api.putProgress(second.slug, { edition: 'raw', slot: 40 })
      const reads = vi.mocked(library.home).mock.calls.length
      const word = readerBack(host).textContent
      await userEvent.click(readerBack(host))
      await idle()
      const waiting = [
        here(),
        idx(),
        vi.mocked(library.home).mock.calls.length - reads,
        cards(),
      ]
      saved.resolve(new Response(null, { status: 204 }))
      await idle()
      return [
        word,
        waiting,
        [vi.mocked(library.home).mock.calls.length - reads, cards()],
      ]
    })
    const order = (works: library.WorkCard[]) =>
      works.map((w) => `/read/${w.slug}`)
    expect(landed).toEqual([
      'Library',
      ['/', 0, 0, order([first, second, third])],
      [1, order([second, first, third])],
    ])
  })
})

/**
 * A way back pressed again before its step has landed: the browser's Back
 * lands a moment after it is asked for, so the screen left still takes
 * presses. The second press steps nowhere: one step back, the history no
 * longer, nothing in `act`.
 */
describe('A way back pressed twice before it lands (O422)', () => {
  /** From the grid to series 60 and its volume 1, the real comic reader: entries 1 and 2. */
  async function intoReader(host: HTMLElement) {
    await userEvent.click(need(host, 'ul.grid a[href="/series/60"]'))
    await until(
      () => host.querySelector('.volumes a[href="/read/work-1"]') !== null,
    )
    await userEvent.click(need(host, '.volumes a[href="/read/work-1"]'))
    await until(() => cells(host).length > 0)
  }

  it('steps back once for a double-click on the series page’s Back', async () => {
    progressRoute({ current: { edition: 'raw', slot: 0 } })
    realComic()
    const host = await app(1280, '/')
    const landed = await outsideAct(async () => {
      await intoReader(host)
      need(host, '[data-reader-page]').focus()
      await userEvent.tab()
      await userEvent.click(seriesLink(host))
      await until(() => host.querySelector('main .page-back') !== null)
      const entries = history.length
      const from = idx()
      await userEvent.dblClick(pageBack(host))
      await until(() => here() !== '/series/41')
      await idle()
      await idle()
      return [here(), idx() - from, history.length - entries]
    })
    expect(landed).toEqual(['/read/work-1', -1, 0])
  })

  it('steps back once for Escape pressed twice in the reader', async () => {
    progressRoute({ current: { edition: 'raw', slot: 0 } })
    realComic()
    const host = await app(1280, '/')
    const landed = await outsideAct(async () => {
      await intoReader(host)
      need(host, '[data-reader-page]').focus()
      const entries = history.length
      const from = idx()
      // Whether the reader was drawn as each press arrived: both presses
      // are the reader's, sent together so both come before the step lands.
      const pressed: boolean[] = []
      const heard = () => pressed.push(cells(host).length > 0)
      addEventListener('keydown', heard, true)
      onTestFinished(() => removeEventListener('keydown', heard, true))
      await Promise.all([
        userEvent.keyboard('{Escape}'),
        userEvent.keyboard('{Escape}'),
      ])
      await until(() => here() !== '/read/work-1')
      await idle()
      await idle()
      return [pressed, here(), idx() - from, history.length - entries]
    })
    expect(landed).toEqual([[true, true], '/series/60', -1, 0])
  })
})

/** The series volume 1 is filed in: its id is not the work's. */
const FILED = { id: 41, title: 'Kaguya-sama wa Kokurasetai' }

/** Volume 1 as the library answers it, filed in `series`. */
function filedWork(
  series: library.WorkDetail['series'] = FILED,
  book = false,
): library.WorkDetail {
  return { ...volume(1, book), alignments: [], series }
}

/** The comic reader's pages, read left to right. */
const PAGES = 6

/** The progress the server holds for volume 1, as `getProgress` answers it. */
interface Held {
  current: api.StoredProgress
}

/**
 * The server's progress route for volume 1: a `PUT` is kept once `settles`
 * lets it, and read back by `getProgress`. Anything else goes to the real
 * `fetch`.
 */
function progressRoute(
  held: Held,
  settles: () => Promise<void> = async () => {},
) {
  const real = window.fetch.bind(window)
  vi.mocked(api.getProgress).mockImplementation(async () => held.current)
  const fetches = vi
    .spyOn(window, 'fetch')
    .mockImplementation(async (input, init) => {
      if (
        init?.method !== 'PUT' ||
        String(input) !== '/api/works/work-1/progress'
      )
        return real(input, init)
      const { edition, slot } = JSON.parse(String(init.body))
      await settles()
      held.current = { edition, slot }
      return new Response(null, { status: 204 })
    })
  onTestFinished(() => fetches.mockRestore())
}

/** The real comic reader for volume 1: one edition of `PAGES` pages. */
function realComic() {
  comic.real = true
  vi.mocked(api.getSidecarWithEtag).mockResolvedValue({
    json: JSON.stringify({
      work: { reading: 'ltr' },
      editions: { raw: {} },
      hub: 'raw',
      alignments: [],
    }),
    etag: 's1',
  })
  vi.mocked(wasmResolver).mockImplementation(async () =>
    fakeResolver({ raw: whole(PAGES) }, {}),
  )
}

/** Each page the comic reader draws, by its alt text. */
const cells = (host: HTMLElement) =>
  [...host.querySelectorAll('[data-reader-page] img')].map((img) =>
    img.getAttribute('alt'),
  )

/** Real time, with nothing in `act`, until `check` holds; throws if it never does. */
async function until(check: () => boolean, ms = 4000) {
  const deadline = performance.now() + ms
  while (!check()) {
    if (performance.now() > deadline) throw new Error('condition never held')
    await new Promise((resolve) => setTimeout(resolve, 20))
  }
}

/** The top bar's link to the series. */
const seriesLink = (host: HTMLElement) =>
  need<HTMLAnchorElement>(host, '.reader-top-bar a.reader-title-series')

describe.each([1280, 390])(
  'The comic reader’s link to its series at %i px (O427)',
  (width) => {
    /**
     * A page turned, the bars brought up, the link clicked: the series page,
     * whose Back names the book and steps back to it, and the reader reopens
     * at the page it was left on. Three rounds, each from the last.
     */
    it('goes to the series page, and Back reopens the reader at the page it was left on, round after round', async () => {
      const held: Held = { current: { edition: 'raw', slot: 0 } }
      progressRoute(held)
      realComic()
      const host = await app(width, '/read/work-1')
      const rounds = await outsideAct(async () => {
        const seen: unknown[] = []
        for (let round = 0; round < 3; round++) {
          await until(() => cells(host).length > 0)
          need(host, '[data-reader-page]').focus()
          await userEvent.keyboard('{ArrowRight}')
          await until(() => cells(host)[0] === `page ${round + 2}`)
          await userEvent.tab()
          const link = seriesLink(host)
          const named = [link.textContent, link.getAttribute('href')]
          const from = idx()
          await userEvent.click(link)
          await until(() => host.querySelector('main .page-back') !== null)
          const series = [here(), idx() - from, pageBack(host).textContent]
          await userEvent.click(pageBack(host))
          await until(() => cells(host).length > 0)
          await idle()
          seen.push({
            named,
            series,
            back: [here(), idx() - from, cells(host)],
          })
        }
        return seen
      })
      expect(rounds).toEqual(
        [2, 3, 4].map((page) => ({
          named: [FILED.title, '/series/41'],
          series: ['/series/41', 1, 'Book'],
          back: ['/read/work-1', 0, [`page ${page}`]],
        })),
      )
    })

    /**
     * A page turned and the link clicked at once, the progress save that
     * leaving sends held open by the server through the series page and Back:
     * the reader that Back reopens waits for it, and opens at the page turned
     * to, not the one before.
     */
    it('reopens at the page turned to when Back comes before the leave’s progress save has settled', async () => {
      const held: Held = { current: { edition: 'raw', slot: 0 } }
      const saving = deferred<void>()
      progressRoute(held, () => saving.promise)
      realComic()
      const host = await app(width, '/read/work-1')
      const landed = await outsideAct(async () => {
        await until(() => cells(host).length > 0)
        need(host, '[data-reader-page]').focus()
        await userEvent.tab()
        await userEvent.keyboard('{ArrowRight}')
        await until(() => cells(host)[0] === 'page 2')
        await userEvent.click(seriesLink(host))
        await until(() => host.querySelector('main .page-back') !== null)
        // The save leaving flushed: sent, and held open.
        await until(() => api.progressSavesOut() === 1)
        const out = here()
        await userEvent.click(pageBack(host))
        await idle()
        saving.resolve()
        await until(() => cells(host).length > 0)
        await idle()
        return [out, here(), cells(host), held.current]
      })
      expect(landed).toEqual([
        '/series/41',
        '/read/work-1',
        ['page 2'],
        { edition: 'raw', slot: 1 },
      ])
    })
  },
)

/**
 * Under a finger at a tablet's width: a press on the work's title, or on the
 * bar's blank space beyond the link's press, leaves the reader where it is;
 * a press in the link's 44 px, above its words, goes to the series page.
 */
it('at 768 px under a finger, the work’s title and the bar’s blank space stay, and the link’s press goes to the series', async () => {
  onTestFinished(coarseRulesOn())
  progressRoute({ current: { edition: 'raw', slot: 0 } })
  realComic()
  const host = await app(768, '/read/work-1')
  const landed = await outsideAct(async () => {
    await until(() => cells(host).length > 0)
    need(host, '[data-reader-page]').focus()
    await userEvent.tab()
    await until(() => host.querySelector('.reader-title-series') !== null)
    await document.fonts.ready
    const bar = need(host, '.reader-top-bar')
    const edge = bar.getBoundingClientRect()
    const at = seriesLink(host).getBoundingClientRect()
    const block = need(host, '.reader-title').getBoundingClientRect()
    const words = document.createRange()
    words.selectNodeContents(need(host, '.reader-title-work'))
    const said = words.getBoundingClientRect()
    // A press that has already left the reader leaves no bar to press.
    const press = async (x: number, y: number) => {
      if (!bar.isConnected) return 'no bar'
      await userEvent.click(bar, {
        position: { x: x - edge.left, y: y - edge.top },
      })
      await idle()
      return here()
    }
    return [
      await press(said.left + said.width / 2, said.top + said.height / 2),
      await press(block.right - 4, (edge.top + edge.bottom) / 2),
      await press(at.left + 4, at.bottom - 36),
    ]
  })
  expect(landed).toEqual(['/read/work-1', '/read/work-1', '/series/41'])
})

/** A book of one edition, long enough to scroll at either width. */
const BOOK: Book = {
  title: 'Kaguya-sama wa Kokurasetai v1',
  hub: 'en',
  editions: {
    en: {
      label: 'English',
      language: 'en',
      blocks: Array.from({ length: 40 }, (_, i) => {
        const text = `Paragraph ${i + 1}. ${'The council meets after class. '.repeat(8)}`
        return { text, heading: false, sentences: [[0, text.length]] }
      }),
    },
  },
  alignments: [],
}

/** The block at the top of the book's text: the first not scrolled past it. */
function topBlock(host: HTMLElement): string | undefined {
  const text = host.querySelector('.book-text')
  if (!text) return undefined
  const top = text.getBoundingClientRect().top
  return [...text.querySelectorAll<HTMLElement>('[data-block]')].find(
    (block) => block.getBoundingClientRect().bottom > top + 1,
  )?.dataset.block
}

/** The book's bars, brought up by a tap in the middle of its text. */
async function barsUp(host: HTMLElement) {
  await userEvent.click(need(host, '.book-text'))
  await until(() => host.querySelector('.reader-top-bar') !== null)
}

describe('The book’s link to its series (O427)', () => {
  /**
   * The book moved on a screen, the bars brought up, the link clicked: the
   * series page, whose Back names the book and steps back to it, and the
   * book reopens where it was moved to. Three rounds, each a screen on from
   * the last.
   */
  it('goes to the series page, and Back reopens the book where it was moved to, round after round', async () => {
    vi.mocked(library.work).mockResolvedValue(filedWork(FILED, true))
    vi.mocked(bookApi.getBook).mockResolvedValue(BOOK)
    const held: Held = { current: { edition: 'en', slot: 12 } }
    progressRoute(held)
    const host = await app(1280, '/book/work-1')
    const rounds = await outsideAct(async () => {
      const seen: {
        opened?: string
        left?: string
        named: unknown[]
        series: unknown[]
        back: unknown[]
      }[] = []
      for (let round = 0; round < 3; round++) {
        await until(() => topBlock(host) !== undefined)
        await idle()
        const opened = topBlock(host)
        await barsUp(host)
        await userEvent.keyboard('{PageDown}')
        await until(() => topBlock(host) !== opened)
        await new Promise((resolve) => requestAnimationFrame(resolve))
        const left = topBlock(host)
        const link = seriesLink(host)
        const named = [link.textContent, link.getAttribute('href')]
        const from = idx()
        await userEvent.click(link)
        await until(() => host.querySelector('main .page-back') !== null)
        const series = [here(), idx() - from, pageBack(host).textContent]
        await userEvent.click(pageBack(host))
        await until(() => topBlock(host) !== undefined)
        await idle()
        seen.push({
          opened,
          left,
          named,
          series,
          back: [here(), idx() - from, topBlock(host)],
        })
      }
      return seen
    })
    const moved = rounds.map(({ opened, left }) => [
      Number(opened),
      Number(left),
    ])
    expect(rounds).toEqual(
      rounds.map(({ opened, left }) => ({
        opened,
        left,
        named: [FILED.title, '/series/41'],
        series: ['/series/41', 1, 'Book'],
        back: ['/book/work-1', 0, left],
      })),
    )
    // Each round opens where the last was left, and is moved on from there.
    expect([
      moved[0][0],
      moved.every(([opened, left]) => left > opened),
      moved.slice(1).map(([opened]) => opened),
    ]).toEqual([12, true, moved.slice(0, -1).map(([, left]) => left)])
  })

  /**
   * The book moved on a screen and the link clicked at once, the progress
   * save that leaving sends held open by the server through the series page
   * and Back: the book that Back reopens waits for it, and opens where it was
   * moved to, not where it was before.
   */
  it('reopens where it was moved to when Back comes before the leave’s progress save has settled', async () => {
    vi.mocked(library.work).mockResolvedValue(filedWork(FILED, true))
    vi.mocked(bookApi.getBook).mockResolvedValue(BOOK)
    const held: Held = { current: { edition: 'en', slot: 12 } }
    const saving = deferred<void>()
    progressRoute(held, () => saving.promise)
    const host = await app(1280, '/book/work-1')
    const landed = await outsideAct(async () => {
      await until(() => topBlock(host) !== undefined)
      await idle()
      await barsUp(host)
      await userEvent.keyboard('{PageDown}')
      await until(() => topBlock(host) !== '12')
      await new Promise((resolve) => requestAnimationFrame(resolve))
      const left = topBlock(host)
      await userEvent.click(seriesLink(host))
      await until(() => host.querySelector('main .page-back') !== null)
      // The save leaving flushed: sent, and held open.
      await until(() => api.progressSavesOut() === 1)
      const out = here()
      await userEvent.click(pageBack(host))
      await idle()
      saving.resolve()
      await until(() => topBlock(host) !== undefined)
      await idle()
      return { left, out, back: [here(), topBlock(host)], held: held.current }
    })
    expect(landed).toEqual({
      left: landed.left,
      out: '/series/41',
      back: ['/book/work-1', landed.left],
      held: { edition: 'en', slot: Number(landed.left) },
    })
    expect(Number(landed.left)).toBeGreaterThan(12)
  })

  /**
   * A book in no series has no series page to link to, and a work answer
   * that fails names none: either way the title block is the book's title
   * over its edition, and no link.
   */
  it.each([
    ['filed in no series', () => Promise.resolve(filedWork(null, true))],
    [
      'whose work answer fails',
      () => Promise.reject(new Error('/api/library/works/work-1: 503')),
    ],
  ])('draws no link for a book %s', async (_, answer) => {
    const logged = vi.spyOn(console, 'error').mockImplementation(() => {})
    onTestFinished(() => logged.mockRestore())
    vi.mocked(library.work).mockImplementation(answer)
    vi.mocked(bookApi.getBook).mockResolvedValue(BOOK)
    progressRoute({ current: { edition: 'en', slot: 0 } })
    const host = await app(1280, '/book/work-1')
    const drawn = await outsideAct(async () => {
      await until(() => topBlock(host) !== undefined)
      await idle()
      await barsUp(host)
      return [...host.querySelectorAll('.reader-title *')].map((el) => [
        el.tagName.toLowerCase(),
        el.className,
        el.textContent,
        el.getAttribute('href'),
      ])
    })
    expect(drawn).toEqual([
      ['span', 'reader-title-work', BOOK.title, null],
      ['span', 'reader-title-line', 'English', null],
      ['span', 'reader-title-edition', 'English', null],
    ])
  })
})

/**
 * `Escape` with nothing open is the reader's way out (O419), so it steps back
 * where the reader was opened from as `Back` does: the series page, a step
 * back each round, the history no longer, nothing in `act`.
 */
describe.each([1280, 390])('Escape at %i px (O419)', (width) => {
  it('steps back from the comic reader to the series page it was opened from, round after round', async () => {
    progressRoute({ current: { edition: 'raw', slot: 0 } })
    realComic()
    const host = await app(width, '/series/60')
    const volume = '.volumes a[href="/read/work-1"]'
    const rounds = await outsideAct(async () => {
      const seen: unknown[] = []
      for (let round = 0; round < 3; round++) {
        await until(() => host.querySelector(volume) !== null)
        await userEvent.click(need(host, volume))
        await until(() => cells(host).length > 0)
        const entries = history.length
        const from = idx()
        need(host, '[data-reader-page]').focus()
        await userEvent.keyboard('{Escape}')
        await until(() => host.querySelector('main .page-back') !== null)
        await idle()
        seen.push([here(), idx() - from, history.length - entries])
      }
      return seen
    })
    expect(rounds).toEqual([1, 2, 3].map(() => ['/series/60', -1, 0]))
  })
})

/** The same from the book, its bars up from a tap, which do not take the key. */
it('steps back from a book with its bars up from a tap to the series page it was opened from, round after round (O419)', async () => {
  vi.mocked(library.oneSeries).mockResolvedValue(seriesPage(80, true))
  vi.mocked(library.work).mockResolvedValue(filedWork(FILED, true))
  vi.mocked(bookApi.getBook).mockResolvedValue(BOOK)
  progressRoute({ current: { edition: 'en', slot: 0 } })
  const host = await app(1280, '/series/60')
  const volume = '.volumes a[href="/book/work-1"]'
  const rounds = await outsideAct(async () => {
    const seen: unknown[] = []
    for (let round = 0; round < 3; round++) {
      await until(() => host.querySelector(volume) !== null)
      await userEvent.click(need(host, volume))
      await until(() => topBlock(host) !== undefined)
      await idle()
      await barsUp(host)
      const entries = history.length
      const from = idx()
      await userEvent.keyboard('{Escape}')
      await until(() => host.querySelector('main .page-back') !== null)
      await idle()
      seen.push([here(), idx() - from, history.length - entries])
    }
    return seen
  })
  expect(rounds).toEqual([1, 2, 3].map(() => ['/series/60', -1, 0]))
})

/**
 * A progress save the network holds open, as a half-open connection can, is
 * still out as a reader opens: each reader waits for it a moment, then reads
 * its place anyway and opens, nothing in `act`.
 */
describe('A progress save that never settles', () => {
  /** A save of volume 1's place that is held open until the test ends. */
  function heldOpen(held: Held) {
    const saving = deferred<void>()
    progressRoute(held, () => saving.promise)
    onTestFinished(async () => {
      saving.resolve()
      await api.progressSaved()
    })
    void api.putProgress('work-1', { edition: held.current.edition, slot: 3 })
    expect(api.progressSavesOut()).toBe(1)
  }

  it('keeps the comic reader from opening no longer than the wait’s bound', async () => {
    heldOpen({ current: { edition: 'raw', slot: 1 } })
    realComic()
    const host = await app(1280, '/read/work-1')
    const drawn = await outsideAct(async () => {
      await until(() => cells(host).length > 0, 2 * api.OPENING_SAVE_WAIT_MS)
      return [cells(host), api.progressSavesOut()]
    })
    expect(drawn).toEqual([['page 2'], 1])
  })

  it('keeps the book from opening no longer than the wait’s bound', async () => {
    vi.mocked(library.work).mockResolvedValue(filedWork(FILED, true))
    vi.mocked(bookApi.getBook).mockResolvedValue(BOOK)
    heldOpen({ current: { edition: 'en', slot: 7 } })
    const host = await app(1280, '/book/work-1')
    const drawn = await outsideAct(async () => {
      await until(
        () => topBlock(host) !== undefined,
        2 * api.OPENING_SAVE_WAIT_MS,
      )
      await idle()
      return [topBlock(host), api.progressSavesOut()]
    })
    expect(drawn).toEqual(['7', 1])
  })
})
