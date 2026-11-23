import { act } from 'react'
import { BrowserRouter } from 'react-router'
import { beforeEach, describe, expect, it, onTestFinished, vi } from 'vitest'
import { page, userEvent } from 'vitest/browser'
import '../src/index.css'
import * as aboutApi from '../src/about'
import * as api from '../src/api'
import type { AppProps } from '../src/App'
import { BENEATH, forgetBack } from '../src/back'
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
import { forgetScroll } from '../src/scrollKeeper'
import { ABOUT } from './support/about'
import { deferred } from './support/async'
import { inProgress } from './support/continueRow'
import { fakeResolver, whole } from './support/fixtures'
import { BROWSE_KEY } from './support/homePage'
import { beforeUnmount, mount, rerender } from './support/mount'
import { coarseRulesOn } from './support/pointer'

/**
 * Back goes where you came from, else the library: the reader's way out and
 * the series page's Back, under the router the app runs (`BrowserRouter`),
 * with real clicks and nothing in `act`, at a desktop's width, where
 * `<main>` scrolls, and a phone's, where the document does.
 *
 * The routes and the book reader are real; the comic reader is the real `App`
 * where a case reads its pages, place, keys or series (O431), and elsewhere
 * a stand-in naming no series, so leaving by O422's rule, that draws the real
 * top bar with what `Routes.tsx` hands `App`, as `App` does (`App.test.tsx`).
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

/** The scroll that puts `link` `gap` px below the top of the scroller, in view at either width. */
function above(host: HTMLElement, link: Element, gap = 300): number {
  const scroller = scrollerAt(host)
  return Math.round(
    link.getBoundingClientRect().top -
      (matchMedia(SIDEBAR_LAYOUT).matches
        ? scroller.getBoundingClientRect().top
        : 0) +
      scroller.scrollTop -
      gap,
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
    const tile = () => need(host, 'ul.grid a[href="/series/41"]')
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
        ['/series/41', -1, 'Library'],
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
  /** From the grid to series `id`, 60 unless said, and its volume 1, the real comic reader: entries 1 and 2. */
  async function intoReader(host: HTMLElement, id = 60) {
    await userEvent.click(need(host, `ul.grid a[href="/series/${id}"]`))
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
      await intoReader(host, FILED.id)
      need(host, '[data-reader-page]').focus()
      const entries = history.length
      const from = idx()
      // Whether the reader was drawn as each press arrived: both presses
      // are the reader's, sent together, the step asked for held until both
      // are in, so both come before it lands.
      const pressed: boolean[] = []
      const heard = () => pressed.push(cells(host).length > 0)
      addEventListener('keydown', heard, true)
      onTestFinished(() => removeEventListener('keydown', heard, true))
      const steps = holdSteps()
      await Promise.all([
        userEvent.keyboard('{Escape}'),
        userEvent.keyboard('{Escape}'),
      ])
      await idle()
      await steps.release()
      await until(() => here() !== '/read/work-1')
      await idle()
      await idle()
      return [pressed, here(), idx() - from, history.length - entries]
    })
    expect(landed).toEqual([[true, true], '/series/41', -1, 0])
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
    const host = await app(width, '/series/41')
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
    expect(rounds).toEqual([1, 2, 3].map(() => ['/series/41', -1, 0]))
  })
})

/** The same from the book, its bars up from a tap, which do not take the key. */
it('steps back from a book with its bars up from a tap to the series page it was opened from, round after round (O419)', async () => {
  vi.mocked(library.oneSeries).mockResolvedValue(seriesPage(80, true))
  vi.mocked(library.work).mockResolvedValue(filedWork(FILED, true))
  vi.mocked(bookApi.getBook).mockResolvedValue(BOOK)
  progressRoute({ current: { edition: 'en', slot: 0 } })
  const host = await app(1280, '/series/41')
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
  expect(rounds).toEqual([1, 2, 3].map(() => ['/series/41', -1, 0]))
})

/** The real comic reader's way out: its top bar's button, drawn while the bars are up. */
const wayOut = (host: HTMLElement) =>
  need<HTMLButtonElement>(host, '.reader-top-bar .reader-back')

/** The series page the entry on screen is marked as having under it (O431). */
const beneath = () =>
  (history.state as { usr?: Record<string, unknown> | null }).usr?.[BENEATH]

/** Volume 1's link on a series page, and its card in the Continue row. */
const VOLUME_1 = '.volumes a[href="/read/work-1"]'
const CARD_1 = '.continue a[href="/read/work-1"]'

/** The Continue row holding volume 1, which is filed in series 41. */
function continuing() {
  vi.mocked(library.home).mockResolvedValue({
    continuing: [volume(1)],
    added: [],
    read: [],
  })
}

/**
 * The app at `path` as `app` draws it, on an entry pushed first so that it
 * is the frame's last: each push from it then adds one to the history.
 */
async function appAtTheEnd(width: number, path: string) {
  history.pushState(null, '', location.href)
  return app(width, path)
}

/**
 * Firefox's user agent until the test ends: its Back skips an entry a page
 * pushed before any input, so there the reader waits for one (O431).
 */
function asFirefox() {
  const agent = vi
    .spyOn(Navigator.prototype, 'userAgent', 'get')
    .mockReturnValue(
      'Mozilla/5.0 (X11; Linux x86_64; rv:145.0) Gecko/20100101 Firefox/145.0',
    )
  onTestFinished(() => agent.mockRestore())
}

/**
 * The comic reader's bars, brought up from its page by `Tab` as the O427
 * cases do, once the reader has settled: a page drawn again after the focus
 * would leave the key to find none.
 */
async function comicBars(host: HTMLElement) {
  await until(() => cells(host).length > 0)
  await idle()
  need(host, '[data-reader-page]').focus()
  await userEvent.tab()
  await until(() => host.querySelector('.reader-top-bar .reader-back') !== null)
}

/**
 * Real time until the comic reader's bar links to its series, drawn from the
 * work answer that names the way out's series page in the same render, and
 * what that render set off has run: a press after this is never in the
 * moment before the answer, when O422's rule still holds.
 */
async function seriesAnswered(host: HTMLElement) {
  await until(() => host.querySelector('.reader-title-series') !== null)
  await idle()
}

/** Real time until the comic reader has drawn its pages and its work answer is in, nothing pressed. */
async function answered(host: HTMLElement) {
  await until(() => cells(host).length > 0)
  await vi.mocked(library.work).mock.results.at(-1)?.value
  await idle()
}

/**
 * Real time until a way out pressed at `path`, on entry `at`, has landed
 * elsewhere, and a series page landed on has drawn its Back.
 */
async function untilLanded(host: HTMLElement, path: string, at: number) {
  await until(
    () =>
      (here() !== path || idx() !== at) &&
      (!location.pathname.startsWith('/series/') ||
        host.querySelector('main .page-back') !== null),
  )
  await idle()
}

/**
 * Every screen `host` draws from now on, in order, starting with the one
 * drawn: a library page by its path, `reader` for a reader, each once
 * however long it stays. The reader puts its series page under itself in one
 * step (O431), so that page is not among them until a way out lands on it.
 */
function screensDrawn(host: HTMLElement): string[] {
  const drawn: string[] = []
  const look = () => {
    const screen =
      host.querySelector('#content') !== null
        ? location.pathname
        : host.querySelector('.reader-route') !== null
          ? 'reader'
          : undefined
    if (screen !== undefined && drawn.at(-1) !== screen) drawn.push(screen)
  }
  const observer = new MutationObserver(look)
  observer.observe(host, { childList: true, subtree: true })
  beforeUnmount(() => observer.disconnect())
  look()
  return drawn
}

/**
 * The comic reader's way out goes to its work's series page, 41 (O431), by
 * where the reader was opened from: the history's index and length after
 * each way out and each Back, three rounds, nothing in `act`. Each history
 * starts on the frame's last entry, so a push adds one to its length.
 */
describe.each([1280, 390])(
  'The comic reader’s way out to its series page at %i px (O431)',
  (width) => {
    /**
     * Opened from its own series page, a query in the address: the reader
     * stamps its entry where it stands, a replace, and its way out steps back
     * to the page as it was left, query and scroll.
     */
    it('steps back to its series page opened with a query, at the scroll it was left at, round after round', async () => {
      progressRoute({ current: { edition: 'raw', slot: 0 } })
      realComic()
      const page = `/series/${FILED.id}?edition=raw`
      const host = await appAtTheEnd(width, page)
      const entries = history.length
      await outsideAct(async () => {
        for (let round = 0; round < 3; round++) {
          await until(() => host.querySelector(VOLUME_1) !== null)
          // Volume 1 is near its page's top, nearer than the grid cases' 300 px.
          const y = above(host, need(host, VOLUME_1), 100) + round * 7
          scrollerAt(host).scrollTop = y
          await idle()
          const left = scrolled(host)
          await userEvent.click(need(host, VOLUME_1))
          await comicBars(host)
          await seriesAnswered(host)
          const opened = [
            wayOut(host).textContent,
            here(),
            idx(),
            history.length - entries,
            beneath(),
          ]
          await userEvent.click(wayOut(host))
          await untilLanded(host, '/read/work-1', 1)
          expect(
            [
              left,
              opened,
              [here(), idx(), history.length - entries, scrolled(host)],
            ],
            `round ${round + 1}`,
          ).toEqual([
            at(y),
            ['Series', '/read/work-1', 1, 1, '/series/41'],
            [page, 0, 1, at(y)],
          ])
        }
      })
    })
  },
)

describe('The comic reader’s way out to its series page (O431)', () => {
  /**
   * Opened from another series' page by a click: the reader puts series 41
   * under itself as its answer lands, in one step the series page is never
   * drawn in. The way out steps back to it, and its Back, named for the page
   * the reader was opened from, steps back there.
   */
  it('steps back to the series page it put under itself, from another series’ page, whose Back steps back there', async () => {
    progressRoute({ current: { edition: 'raw', slot: 0 } })
    realComic()
    const host = await appAtTheEnd(1280, '/series/60')
    const entries = history.length
    await outsideAct(async () => {
      for (let round = 0; round < 3; round++) {
        await until(() => host.querySelector(VOLUME_1) !== null)
        const drawn = screensDrawn(host)
        await userEvent.click(need(host, VOLUME_1))
        await answered(host)
        // Read before any input: the Tab that brings the bars up is one, and
        // would set off a rewrite the answer had not (O431).
        const seen = [idx(), history.length - entries, [...drawn]]
        await comicBars(host)
        await seriesAnswered(host)
        const opened = [
          wayOut(host).textContent,
          here(),
          idx(),
          history.length - entries,
          seen,
        ]
        await userEvent.click(wayOut(host))
        await untilLanded(host, '/read/work-1', 2)
        expect(
          [
            opened,
            [
              here(),
              idx(),
              history.length - entries,
              host.querySelector('main .page-back')?.textContent,
            ],
          ],
          `round ${round + 1}, the reader's way out`,
        ).toEqual([
          ['Series', '/read/work-1', 2, 2, [2, 2, ['/series/60', 'reader']]],
          ['/series/41', 1, 2, 'Series'],
        ])
        await userEvent.click(pageBack(host))
        await untilLanded(host, '/series/41', 1)
        expect(
          [here(), idx(), history.length - entries],
          `round ${round + 1}, the series page's Back`,
        ).toEqual(['/series/60', 0, 2])
      }
    })
  })

  /**
   * Opened from the Continue row by a click: the reader puts series 41 under
   * itself as its answer lands, the way out steps back to it at its top, and
   * its Back, named for the library, steps back to the row.
   */
  it('steps back to the series page it put under itself, from the Continue row, whose Back steps back to the row', async () => {
    continuing()
    progressRoute({ current: { edition: 'raw', slot: 0 } })
    realComic()
    const host = await appAtTheEnd(1280, '/')
    const entries = history.length
    await outsideAct(async () => {
      for (let round = 0; round < 3; round++) {
        await until(() => host.querySelector(CARD_1) !== null)
        const drawn = screensDrawn(host)
        await userEvent.click(need(host, CARD_1))
        await answered(host)
        // Read before any input: the Tab that brings the bars up is one, and
        // would set off a rewrite the answer had not (O431).
        const seen = [idx(), history.length - entries, [...drawn]]
        await comicBars(host)
        await seriesAnswered(host)
        const opened = [
          wayOut(host).textContent,
          here(),
          idx(),
          history.length - entries,
          seen,
        ]
        await userEvent.click(wayOut(host))
        await untilLanded(host, '/read/work-1', 2)
        expect(
          [
            opened,
            [
              here(),
              idx(),
              history.length - entries,
              scrolled(host),
              host.querySelector('main .page-back')?.textContent,
            ],
          ],
          `round ${round + 1}, the reader's way out`,
        ).toEqual([
          ['Series', '/read/work-1', 2, 2, [2, 2, ['/', 'reader']]],
          ['/series/41', 1, 2, [0, 0], 'Library'],
        ])
        await userEvent.click(pageBack(host))
        await untilLanded(host, '/series/41', 1)
        expect(
          [here(), idx(), history.length - entries],
          `round ${round + 1}, the series page's Back`,
        ).toEqual(['/', 0, 2])
      }
    })
  })

  /**
   * In Firefox, opened cold with nothing under it: the reader puts series 41
   * under itself at its first key, not as its answer lands, and the way out
   * steps back to it; its Back, with no entry before it, pushes the library.
   * Each round is a cold open of its own.
   */
  it('in Firefox, puts its series page under a reader opened cold at the first key, and steps back to it, round after round', async () => {
    asFirefox()
    // No Library view drawn yet in this tab, as in a new one.
    forgetScroll()
    progressRoute({ current: { edition: 'raw', slot: 0 } })
    realComic()
    for (let round = 0; round < 3; round++) {
      let host: HTMLElement
      if (round === 0) host = await appAtTheEnd(1280, '/read/work-1')
      else {
        rerender(null)
        history.pushState(null, '', '/read/work-1')
        host = await draw()
      }
      const entries = history.length
      await outsideAct(async () => {
        await answered(host)
        const before = [here(), idx(), history.length - entries]
        const shown = cells(host)[0]
        need(host, '[data-reader-page]').focus()
        await userEvent.keyboard('{ArrowRight}')
        await until(() => cells(host)[0] !== shown)
        await idle()
        const keyed = [here(), idx(), history.length - entries, beneath()]
        await comicBars(host)
        const word = wayOut(host).textContent
        await userEvent.click(wayOut(host))
        await untilLanded(host, '/read/work-1', 1)
        expect(
          [
            before,
            keyed,
            word,
            [
              here(),
              idx(),
              history.length - entries,
              host.querySelector('main .page-back')?.textContent,
            ],
          ],
          `round ${round + 1}, the reader's way out`,
        ).toEqual([
          ['/read/work-1', 0, 0],
          ['/read/work-1', 1, 1, '/series/41'],
          'Series',
          ['/series/41', 0, 1, 'Library'],
        ])
        await userEvent.click(pageBack(host))
        await untilLanded(host, '/series/41', 0)
        expect(
          [here(), idx(), history.length - entries],
          `round ${round + 1}, the series page's Back`,
        ).toEqual(['/', 1, 1])
      })
    }
  })

  /**
   * In Firefox, from the Continue row, `Escape` as the first key: no input,
   * so nothing is under the reader yet, and the way out puts the series page
   * in the reader's place, a replace (O431).
   */
  it('in Firefox, puts the series page in the reader’s place for Escape as the first key, round after round', async () => {
    asFirefox()
    continuing()
    progressRoute({ current: { edition: 'raw', slot: 0 } })
    realComic()
    const host = await appAtTheEnd(1280, '/')
    const entries = history.length
    await outsideAct(async () => {
      for (let round = 0; round < 3; round++) {
        await until(() => host.querySelector(CARD_1) !== null)
        await userEvent.click(need(host, CARD_1))
        await answered(host)
        const opened = [here(), idx(), history.length - entries]
        need(host, '[data-reader-page]').focus()
        await userEvent.keyboard('{Escape}')
        await untilLanded(host, '/read/work-1', 1)
        expect(
          [
            opened,
            [
              here(),
              idx(),
              history.length - entries,
              host.querySelector('main .page-back')?.textContent,
            ],
          ],
          `round ${round + 1}, Escape`,
        ).toEqual([
          ['/read/work-1', 1, 1],
          ['/series/41', 1, 1, 'Library'],
        ])
        await userEvent.click(pageBack(host))
        await untilLanded(host, '/series/41', 1)
        expect(
          [here(), idx(), history.length - entries],
          `round ${round + 1}, the series page's Back`,
        ).toEqual(['/', 0, 1])
      }
    })
  })

  /**
   * The same, the work's answer landing only after the pages are drawn: the
   * reader's keys are heard from before its series is known, and `Escape`
   * still takes the way out the route hands in once it is (O431).
   */
  it('in Firefox, puts the series page in the reader’s place for Escape as the first key when the answer lands after the pages, round after round', async () => {
    asFirefox()
    continuing()
    progressRoute({ current: { edition: 'raw', slot: 0 } })
    realComic()
    const host = await appAtTheEnd(1280, '/')
    const entries = history.length
    await outsideAct(async () => {
      for (let round = 0; round < 3; round++) {
        const answer = deferred<library.WorkDetail>()
        vi.mocked(library.work).mockReturnValue(answer.promise)
        await until(() => host.querySelector(CARD_1) !== null)
        await userEvent.click(need(host, CARD_1))
        await until(() => cells(host).length > 0)
        answer.resolve(filedWork())
        await answer.promise
        await idle()
        const opened = [here(), idx(), history.length - entries]
        need(host, '[data-reader-page]').focus()
        await userEvent.keyboard('{Escape}')
        await untilLanded(host, '/read/work-1', 1)
        expect(
          [
            opened,
            [
              here(),
              idx(),
              history.length - entries,
              host.querySelector('main .page-back')?.textContent,
            ],
          ],
          `round ${round + 1}, Escape`,
        ).toEqual([
          ['/read/work-1', 1, 1],
          ['/series/41', 1, 1, 'Library'],
        ])
        await userEvent.click(pageBack(host))
        await untilLanded(host, '/series/41', 1)
        expect(
          [here(), idx(), history.length - entries],
          `round ${round + 1}, the series page's Back`,
        ).toEqual(['/', 0, 1])
      }
    })
  })
})

/**
 * Holds each step through the history the app asks for until `release`,
 * which takes them one at a time: the browser's Back lands a moment after it
 * is asked for, and this stretches the moment so that a second press always
 * falls in it, the reader still drawn.
 */
function holdSteps() {
  const held: (number | undefined)[] = []
  const go = vi
    .spyOn(History.prototype, 'go')
    .mockImplementation((delta?: number) => {
      held.push(delta)
    })
  onTestFinished(() => go.mockRestore())
  return {
    held,
    async release() {
      go.mockRestore()
      for (const delta of held.splice(0)) {
        history.go(delta)
        await idle()
      }
    },
  }
}

/**
 * The comic reader's way out pressed twice before its step lands, opened
 * from the Continue row: with series 41 under the reader, and with it not
 * yet there (Firefox, the answer in only after the bars are up), where the
 * first press puts it there. One step back to it either way; the second
 * press steps nowhere (O422, O431).
 */
describe.each([
  [
    '`L` pressed twice',
    'keydown',
    async (host: HTMLElement) => {
      need(host, '[data-reader-page]').focus()
      await userEvent.keyboard('L')
      // A hand's second press comes a frame or more after the first: after
      // all the first drew, before the step it asked for lands.
      await idle()
      await userEvent.keyboard('L')
    },
  ],
  [
    'its way out double-clicked',
    'click',
    async (host: HTMLElement) => {
      await userEvent.dblClick(wayOut(host))
    },
  ],
] as const)('The comic reader’s way out, %s (O431)', (_, press, twice) => {
  /**
   * The presses, from now until `stop` is called: whether the reader's
   * pages were drawn as each arrived, so both are the reader's.
   */
  function presses(host: HTMLElement) {
    const heard: boolean[] = []
    const hear = () => heard.push(cells(host).length > 0)
    addEventListener(press, hear, true)
    return {
      heard,
      stop: () => removeEventListener(press, hear, true),
    }
  }

  it('steps back once to the series page under the reader', async () => {
    continuing()
    progressRoute({ current: { edition: 'raw', slot: 0 } })
    realComic()
    const host = await appAtTheEnd(1280, '/')
    const entries = history.length
    await outsideAct(async () => {
      for (let round = 0; round < 3; round++) {
        await until(() => host.querySelector(CARD_1) !== null)
        await userEvent.click(need(host, CARD_1))
        await comicBars(host)
        await seriesAnswered(host)
        const opened = [
          wayOut(host).textContent,
          here(),
          idx(),
          history.length - entries,
        ]
        const steps = holdSteps()
        const pressed = presses(host)
        await twice(host)
        await idle()
        pressed.stop()
        const asked = steps.held.length
        await steps.release()
        await untilLanded(host, '/read/work-1', 2)
        expect(
          [
            opened,
            pressed.heard,
            asked,
            [here(), idx(), history.length - entries],
          ],
          `round ${round + 1}`,
        ).toEqual([
          ['Series', '/read/work-1', 2, 2],
          [true, true],
          1,
          ['/series/41', 1, 2],
        ])
        await userEvent.click(pageBack(host))
        await untilLanded(host, '/series/41', 1)
        expect(
          [here(), idx(), history.length - entries],
          `round ${round + 1}, the series page's Back`,
        ).toEqual(['/', 0, 2])
      }
    })
  })

  it('in Firefox, puts the series page under the reader at the first press and steps back to it once', async () => {
    asFirefox()
    continuing()
    progressRoute({ current: { edition: 'raw', slot: 0 } })
    realComic()
    const host = await appAtTheEnd(1280, '/')
    const entries = history.length
    await outsideAct(async () => {
      for (let round = 0; round < 3; round++) {
        const answer = deferred<library.WorkDetail>()
        vi.mocked(library.work).mockReturnValue(answer.promise)
        await until(() => host.querySelector(CARD_1) !== null)
        await userEvent.click(need(host, CARD_1))
        // Up before the answer: the key that brings them is heard by nothing.
        await comicBars(host)
        const before = wayOut(host).textContent
        answer.resolve(filedWork())
        await seriesAnswered(host)
        const opened = [
          wayOut(host).textContent,
          here(),
          idx(),
          history.length - entries,
        ]
        const steps = holdSteps()
        const pressed = presses(host)
        await twice(host)
        await idle()
        pressed.stop()
        const asked = steps.held.length
        await steps.release()
        await untilLanded(host, '/read/work-1', 1)
        expect(
          [
            before,
            opened,
            pressed.heard,
            asked,
            [here(), idx(), history.length - entries],
          ],
          `round ${round + 1}`,
        ).toEqual([
          'Library',
          ['Series', '/read/work-1', 1, 1],
          [true, true],
          1,
          ['/series/41', 1, 2],
        ])
        await userEvent.click(pageBack(host))
        await untilLanded(host, '/series/41', 1)
        expect(
          [here(), idx(), history.length - entries],
          `round ${round + 1}, the series page's Back`,
        ).toEqual(['/', 0, 2])
      }
    })
  })
})

/**
 * The history under the comic reader, walked with `history.back()` and
 * `history.forward()` as the browser's own Back and Forward walk it where
 * they skip nothing (docs/notes.md, "The reader's way back and the browser's
 * history (O431)"): the series page the reader puts under itself, by each
 * timing it may, never past the app's first entry in the frame. Three rounds
 * each, nothing in `act`; each history starts on the frame's last entry.
 */
describe('The history under the comic reader (O431)', () => {
  /**
   * `history.back()` or `history.forward()`, then real time until it has
   * landed and drawn what it landed on: a reader's pages, a series page's
   * Back, or the Continue row.
   */
  async function walk(host: HTMLElement, step: 'back' | 'forward') {
    const path = here()
    const at = idx()
    history[step]()
    await until(
      () =>
        (here() !== path || idx() !== at) &&
        (location.pathname.startsWith('/read/')
          ? cells(host).length > 0
          : location.pathname.startsWith('/series/')
            ? host.querySelector('main .page-back') !== null
            : host.querySelector(CARD_1) !== null),
    )
    await idle()
  }

  /**
   * Takes each step in turn, keeping what it landed on, and stops after the
   * first that lands other than `expected` says: a step from a wrong landing
   * could walk out of the app's entries into the frame's own.
   */
  async function walking(
    expected: unknown[],
    steps: (() => Promise<unknown>)[],
  ): Promise<unknown[]> {
    const landed: unknown[] = []
    for (const step of steps) {
      landed.push(await step())
      const at = landed.length - 1
      if (JSON.stringify(landed[at]) !== JSON.stringify(expected[at])) break
    }
    return landed
  }

  /**
   * Opened from the Continue row by a click, a page turned: Back lands on the
   * series page under the reader, at its top, Back again on the row, at the
   * scroll it was left at, and Forward twice on the reader, at its page.
   */
  it('walks back to the series page under the reader and on to the row, and forward to the reader at its page, round after round', async () => {
    continuing()
    progressRoute({ current: { edition: 'raw', slot: 0 } })
    realComic()
    const host = await appAtTheEnd(1280, '/')
    const entries = history.length
    const place = () => [here(), idx(), history.length - entries]
    await outsideAct(async () => {
      for (let round = 0; round < 3; round++) {
        await until(() => host.querySelector(CARD_1) !== null)
        const y = above(host, need(host, CARD_1), 40) + round * 7
        scrollerAt(host).scrollTop = y
        await idle()
        const left = scrolled(host)
        await userEvent.click(need(host, CARD_1))
        await answered(host)
        need(host, '[data-reader-page]').focus()
        await userEvent.keyboard('{ArrowRight}')
        await until(() => cells(host)[0] === `page ${round + 2}`)
        await comicBars(host)
        const opened = [left, wayOut(host).textContent, place()]
        const expected = [
          ['/series/41', 1, 2, [0, 0]],
          ['/', 0, 2, at(y)],
          ['/series/41', 1, 2, [0, 0]],
          ['/read/work-1', 2, 2, [`page ${round + 2}`]],
        ]
        const steps = ['back', 'back', 'forward', 'forward'] as const
        const walked = await walking(
          expected,
          steps.map((step) => async () => {
            await walk(host, step)
            return [
              ...place(),
              location.pathname.startsWith('/read/')
                ? cells(host)
                : scrolled(host),
            ]
          }),
        )
        expect([opened, walked], `round ${round + 1}`).toEqual([
          [at(y), 'Series', ['/read/work-1', 2, 2]],
          expected,
        ])
        history.go(-2)
        await until(() => here() === '/' && host.querySelector(CARD_1) !== null)
        await idle()
      }
    })
  })

  /**
   * In Firefox, opened from the Continue row by a click: the history stays as
   * the card's link left it, the reader's word "Series" shown, until the
   * first key, which puts the series page under the reader for Back.
   */
  it('in Firefox, keeps the history as the link left it until the first key, which puts the series page under the reader, round after round', async () => {
    asFirefox()
    continuing()
    progressRoute({ current: { edition: 'raw', slot: 0 } })
    realComic()
    const host = await appAtTheEnd(1280, '/')
    const entries = history.length
    const place = () => [here(), idx(), history.length - entries]
    await outsideAct(async () => {
      for (let round = 0; round < 3; round++) {
        const answer = deferred<library.WorkDetail>()
        vi.mocked(library.work).mockReturnValue(answer.promise)
        await until(() => host.querySelector(CARD_1) !== null)
        await userEvent.click(need(host, CARD_1))
        await until(() => cells(host).length > 0)
        const linked = history.state
        // Up before the answer: the key that brings them is heard by nothing.
        await comicBars(host)
        answer.resolve(filedWork())
        await seriesAnswered(host)
        await idle()
        await idle()
        const shown = [
          wayOut(host).textContent,
          history.length - entries,
          history.state,
          beneath(),
        ]
        const page = cells(host)[0]
        need(host, '[data-reader-page]').focus()
        await userEvent.keyboard('{ArrowRight}')
        await until(() => cells(host)[0] !== page)
        await idle()
        const keyed = [...place(), beneath()]
        await walk(host, 'back')
        const back = place()
        await walk(host, 'back')
        expect([shown, keyed, back, place()], `round ${round + 1}`).toEqual([
          ['Series', 1, linked, undefined],
          ['/read/work-1', 2, 2, '/series/41'],
          ['/series/41', 1, 2],
          ['/', 0, 2],
        ])
      }
    })
  })

  /**
   * In Firefox, opened cold: the first key puts series 41 under the reader,
   * at the app's first entry. Forward from it reopens the reader, named for
   * that page before its answer lands; the page's Back, with nothing before
   * it, pushes the library.
   */
  it('in Firefox, opened cold, walks back to the series page the first key put under the reader, and forward to the reader named for it, round after round', async () => {
    asFirefox()
    // No Library view drawn yet in this tab, as in a new one.
    forgetScroll()
    progressRoute({ current: { edition: 'raw', slot: 0 } })
    realComic()
    for (let round = 0; round < 3; round++) {
      let host: HTMLElement
      if (round === 0) host = await appAtTheEnd(1280, '/read/work-1')
      else {
        rerender(null)
        history.pushState(null, '', '/read/work-1')
        host = await draw()
      }
      const entries = history.length
      const place = () => [here(), idx(), history.length - entries]
      await outsideAct(async () => {
        await answered(host)
        const shown = cells(host)[0]
        need(host, '[data-reader-page]').focus()
        await userEvent.keyboard('{ArrowRight}')
        await until(() => cells(host)[0] !== shown)
        await idle()
        const keyed = [...place(), beneath()]
        await walk(host, 'back')
        const back = [...place(), pageBack(host).textContent]
        const answer = deferred<library.WorkDetail>()
        vi.mocked(library.work).mockReturnValue(answer.promise)
        await walk(host, 'forward')
        await comicBars(host)
        const forward = [...place(), wayOut(host).textContent]
        answer.resolve(filedWork())
        await seriesAnswered(host)
        vi.mocked(library.work).mockResolvedValue(filedWork())
        const afterAnswer = place()
        await walk(host, 'back')
        const again = place()
        await userEvent.click(pageBack(host))
        await untilLanded(host, '/series/41', 0)
        expect(
          [keyed, back, forward, afterAnswer, again, place()],
          `round ${round + 1}`,
        ).toEqual([
          ['/read/work-1', 1, 1, '/series/41'],
          ['/series/41', 0, 1, 'Library'],
          ['/read/work-1', 1, 1, 'Series'],
          ['/read/work-1', 1, 1],
          ['/series/41', 0, 1],
          ['/', 1, 1],
        ])
      })
    }
  })

  /**
   * Opened from the Continue row, a page turned, and the bar's link to the
   * series: pushed above the reader, whose Back reopens the reader at its
   * page (O427). The reader's way out then steps back to the series page
   * under it, and Forward walks to the reader, then to the link's page. The
   * reopened reader knows that page is under it from its entry's mark and
   * from the record of the push that put it there (O431): a way out that
   * heeded neither would replace the reader, and the first Forward would
   * land on the link's page.
   */
  it('steps back to the series page under the reader after the series link and its Back, and Forward walks to the reader and the link’s page, round after round', async () => {
    continuing()
    progressRoute({ current: { edition: 'raw', slot: 0 } })
    realComic()
    const host = await appAtTheEnd(1280, '/')
    const place = () => [here(), idx()]
    await outsideAct(async () => {
      for (let round = 0; round < 3; round++) {
        await until(() => host.querySelector(CARD_1) !== null)
        await userEvent.click(need(host, CARD_1))
        await answered(host)
        need(host, '[data-reader-page]').focus()
        await userEvent.keyboard('{ArrowRight}')
        await until(() => cells(host)[0] === `page ${round + 2}`)
        await comicBars(host)
        const opened = place()
        const expected = [
          ['/series/41', 3],
          ['/read/work-1', 2, [`page ${round + 2}`]],
          ['Series', '/series/41', 1],
          ['/read/work-1', 2, [`page ${round + 2}`]],
          ['/series/41', 3],
        ]
        const walked = await walking(expected, [
          async () => {
            await userEvent.click(seriesLink(host))
            await untilLanded(host, '/read/work-1', 2)
            return place()
          },
          async () => {
            await userEvent.click(pageBack(host))
            await until(() => cells(host).length > 0)
            await idle()
            return [...place(), cells(host)]
          },
          async () => {
            await comicBars(host)
            await seriesAnswered(host)
            const word = wayOut(host).textContent
            await userEvent.click(wayOut(host))
            await untilLanded(host, '/read/work-1', 2)
            return [word, ...place()]
          },
          async () => {
            await walk(host, 'forward')
            return [...place(), cells(host)]
          },
          async () => {
            await walk(host, 'forward')
            return place()
          },
        ])
        expect([opened, walked], `round ${round + 1}`).toEqual([
          ['/read/work-1', 2],
          expected,
        ])
        history.go(-3)
        await until(() => here() === '/' && host.querySelector(CARD_1) !== null)
        await idle()
      }
    })
  })
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

/**
 * The reader's way back where its work names no series or names it late,
 * after a reload, from the book, and the series page's Back after Forward
 * (O431, O434): the address and the router's index after each way out and
 * each step, three rounds, nothing in `act`. A whole-file run reaches the
 * tab's cap on its history's length by here, so a push is read from the
 * index, and the length only where a step must add no entry.
 */
describe('The reader’s way back with no series known, after a reload, from the book, and across Forward (O431, O434)', () => {
  /** Volume 1's card in the Continue row where it is read in the book reader. */
  const BOOK_CARD = '.continue a[href="/book/work-1"]'

  /**
   * Real time until a press at `path`, on entry `at`, has landed, or until
   * the wait gives up: a press that steps nowhere is then read where it
   * stayed, not thrown.
   */
  async function landing(host: HTMLElement, path: string, at: number) {
    await untilLanded(host, path, at).catch(() => undefined)
  }

  /**
   * `Escape` from the comic reader with its bars up from `Tab`: the first
   * takes the bars away, as the keyboard holds them; the second, from the
   * page, is the way out.
   */
  async function escapeOut(host: HTMLElement) {
    await userEvent.keyboard('{Escape}')
    await until(() => host.querySelector('.reader-top-bar') === null)
    need(host, '[data-reader-page]').focus()
    await userEvent.keyboard('{Escape}')
  }

  /**
   * An unfiled volume opened from the grid's tile: it has no series page,
   * so a key and a click put nothing under the reader and mark nothing, and
   * the way out, named for the grid, steps back to it at its scroll (O422).
   */
  it('steps back to the grid from a volume in no series, nothing put under the reader, round after round', async () => {
    vi.mocked(library.grid).mockResolvedValue({
      series: GRID.series.slice(0, 100),
      unfiled: [volume(1)],
    })
    vi.mocked(library.work).mockResolvedValue(filedWork(null))
    progressRoute({ current: { edition: 'raw', slot: 0 } })
    realComic()
    const host = await appAtTheEnd(1280, '/')
    const tile = 'ul.grid a[href="/read/work-1"]'
    await outsideAct(async () => {
      for (let round = 0; round < 3; round++) {
        await until(() => host.querySelector(tile) !== null)
        await idle()
        // The tile is the grid's last, so in view at the grid's foot.
        const scroller = scrollerAt(host)
        const y = scroller.scrollHeight - scroller.clientHeight - 20 - round * 7
        scroller.scrollTop = y
        await idle()
        const left = scrolled(host)
        await userEvent.click(need(host, tile))
        await answered(host)
        const opened = [here(), idx(), beneath()]
        const entries = history.length
        const shown = cells(host)[0]
        need(host, '[data-reader-page]').focus()
        await userEvent.keyboard('{ArrowRight}')
        await until(() => cells(host)[0] !== shown)
        // A click in the page's middle brings the bars up.
        await userEvent.click(need(host, '[data-reader-page]'))
        await until(
          () => host.querySelector('.reader-top-bar .reader-back') !== null,
        )
        await idle()
        const pressed = [here(), idx(), history.length - entries, beneath()]
        const word = wayOut(host).textContent
        await userEvent.click(wayOut(host))
        await landing(host, '/read/work-1', 1)
        await until(() => host.querySelector(tile) !== null)
        await idle()
        expect(
          [
            left,
            opened,
            pressed,
            word,
            [here(), idx(), history.length - entries, scrolled(host)],
          ],
          `round ${round + 1}`,
        ).toEqual([
          at(y),
          ['/read/work-1', 1, undefined],
          ['/read/work-1', 1, 0, undefined],
          'Library',
          ['/', 0, 0, at(y)],
        ])
      }
    })
  })

  /**
   * From the Continue row, the work's answer held: the way out is O422's,
   * named for the row and stepping back to it, and a key puts nothing under
   * the reader. With the answer let go before the press, the series page is
   * under the reader, and `Escape` steps back to it (O431).
   */
  it('steps back to the row while the work’s answer is out, and to the series page once it is in, round after round', async () => {
    continuing()
    progressRoute({ current: { edition: 'raw', slot: 0 } })
    realComic()
    const host = await appAtTheEnd(1280, '/')
    await outsideAct(async () => {
      for (let round = 0; round < 3; round++) {
        vi.mocked(library.work).mockReturnValue(
          deferred<library.WorkDetail>().promise,
        )
        await until(() => host.querySelector(CARD_1) !== null)
        await userEvent.click(need(host, CARD_1))
        await until(() => cells(host).length > 0)
        await idle()
        const entries = history.length
        const shown = cells(host)[0]
        need(host, '[data-reader-page]').focus()
        await userEvent.keyboard('{ArrowRight}')
        await until(() => cells(host)[0] !== shown)
        await idle()
        const keyed = [here(), idx(), history.length - entries, beneath()]
        await comicBars(host)
        const word = wayOut(host).textContent
        await escapeOut(host)
        await landing(host, '/read/work-1', 1)
        expect(
          [keyed, word, [here(), idx(), history.length - entries]],
          `round ${round + 1}, the answer out`,
        ).toEqual([['/read/work-1', 1, 0, undefined], 'Library', ['/', 0, 0]])
        const answer = deferred<library.WorkDetail>()
        vi.mocked(library.work).mockReturnValue(answer.promise)
        await until(() => host.querySelector(CARD_1) !== null)
        await userEvent.click(need(host, CARD_1))
        await until(() => cells(host).length > 0)
        answer.resolve(filedWork())
        await answered(host)
        // The bars' key is an input: by now the series page is under the
        // reader, whether it was put there as the answer landed or at the key.
        await comicBars(host)
        await seriesAnswered(host)
        const opened = [wayOut(host).textContent, here(), idx(), beneath()]
        const before = history.length
        await escapeOut(host)
        await landing(host, '/read/work-1', 2)
        expect(
          [opened, [here(), idx(), history.length - before]],
          `round ${round + 1}, the answer in`,
        ).toEqual([
          ['Series', '/read/work-1', 2, '/series/41'],
          ['/series/41', 1, 0],
        ])
        await userEvent.click(pageBack(host))
        await untilLanded(host, '/series/41', 1)
        expect(
          [here(), idx()],
          `round ${round + 1}, the series page's Back`,
        ).toEqual(['/', 0])
      }
    })
  })

  /**
   * From the Continue row, the series page put under the reader, then a
   * reload, which keeps the entry's state and forgets the record of each
   * push: the entry's mark alone says the page is under the reader, and the
   * way out steps back to it (O431).
   */
  it('steps back to the series page under a reloaded reader, by its entry’s mark, round after round', async () => {
    continuing()
    progressRoute({ current: { edition: 'raw', slot: 0 } })
    realComic()
    let host = await appAtTheEnd(1280, '/')
    for (let round = 0; round < 3; round++) {
      const put = await outsideAct(async () => {
        await until(() => host.querySelector(CARD_1) !== null)
        await userEvent.click(need(host, CARD_1))
        await answered(host)
        // The bars' key is an input: by now the series page is under the
        // reader, whether it was put there as the answer landed or at the key.
        await comicBars(host)
        await seriesAnswered(host)
        return [here(), idx(), beneath()]
      })
      forgetBack()
      rerender(null)
      host = await draw()
      await outsideAct(async () => {
        await answered(host)
        const reloaded = [here(), idx(), beneath()]
        await comicBars(host)
        await seriesAnswered(host)
        const word = wayOut(host).textContent
        const entries = history.length
        await userEvent.click(wayOut(host))
        await landing(host, '/read/work-1', 2)
        expect(
          [put, reloaded, word, [here(), idx(), history.length - entries]],
          `round ${round + 1}, the reloaded reader's way out`,
        ).toEqual([
          ['/read/work-1', 2, '/series/41'],
          ['/read/work-1', 2, '/series/41'],
          'Series',
          ['/series/41', 1, 0],
        ])
        await userEvent.click(pageBack(host))
        await untilLanded(host, '/series/41', 1)
        expect(
          [here(), idx()],
          `round ${round + 1}, the series page's Back`,
        ).toEqual(['/', 0])
      })
    }
  })

  /**
   * In Firefox, from the Continue row, reloaded before the work's first
   * answer landed: the entry carries no mark and the record is gone, so
   * nothing says the series page is under the reader, and `Escape`, which is
   * no input, puts the page in the reader's place (O431).
   */
  it('in Firefox, puts the series page in the place of a reader reloaded before its answer, for Escape, round after round', async () => {
    asFirefox()
    continuing()
    progressRoute({ current: { edition: 'raw', slot: 0 } })
    realComic()
    let host = await appAtTheEnd(1280, '/')
    for (let round = 0; round < 3; round++) {
      vi.mocked(library.work).mockReturnValue(
        deferred<library.WorkDetail>().promise,
      )
      const opened = await outsideAct(async () => {
        await until(() => host.querySelector(CARD_1) !== null)
        await userEvent.click(need(host, CARD_1))
        await until(() => cells(host).length > 0)
        await idle()
        return [here(), idx(), beneath()]
      })
      forgetBack()
      rerender(null)
      vi.mocked(library.work).mockResolvedValue(filedWork())
      host = await draw()
      await outsideAct(async () => {
        await answered(host)
        const reloaded = [here(), idx(), beneath()]
        const entries = history.length
        need(host, '[data-reader-page]').focus()
        await userEvent.keyboard('{Escape}')
        await landing(host, '/read/work-1', 1)
        expect(
          [opened, reloaded, [here(), idx(), history.length - entries]],
          `round ${round + 1}, Escape`,
        ).toEqual([
          ['/read/work-1', 1, undefined],
          ['/read/work-1', 1, undefined],
          ['/series/41', 1, 0],
        ])
        await userEvent.click(pageBack(host))
        await untilLanded(host, '/series/41', 1)
        expect(
          [here(), idx()],
          `round ${round + 1}, the series page's Back`,
        ).toEqual(['/', 0])
      })
    }
  })

  /**
   * The book, from the Continue row: it names its series as the comic reader
   * does, so its way out, the button in one round and `L` in the next, steps
   * back to the series page put under it (O431).
   */
  it('steps back from the book to the series page put under it, by its button and by `L`, round after round', async () => {
    vi.mocked(library.home).mockResolvedValue({
      continuing: [volume(1, true)],
      added: [],
      read: [],
    })
    vi.mocked(library.work).mockResolvedValue(filedWork(FILED, true))
    vi.mocked(bookApi.getBook).mockResolvedValue(BOOK)
    progressRoute({ current: { edition: 'en', slot: 0 } })
    const host = await appAtTheEnd(1280, '/')
    await outsideAct(async () => {
      for (const [round, press] of (
        ['click', 'L', 'click'] as const
      ).entries()) {
        await until(() => host.querySelector(BOOK_CARD) !== null)
        await userEvent.click(need(host, BOOK_CARD))
        await until(() => topBlock(host) !== undefined)
        await vi.mocked(library.work).mock.results.at(-1)?.value
        await idle()
        // The tap that brings the bars up is an input: by now the series page
        // is under the book, whether it was put there as the answer landed or
        // at the tap.
        await barsUp(host)
        await seriesAnswered(host)
        const opened = [wayOut(host).textContent, here(), idx(), beneath()]
        const entries = history.length
        if (press === 'click') await userEvent.click(wayOut(host))
        else await userEvent.keyboard('L')
        await landing(host, '/book/work-1', 2)
        expect(
          [opened, [here(), idx(), history.length - entries]],
          `round ${round + 1}, ${press}`,
        ).toEqual([
          ['Series', '/book/work-1', 2, '/series/41'],
          ['/series/41', 1, 0],
        ])
        await userEvent.click(pageBack(host))
        await untilLanded(host, '/series/41', 1)
        expect(
          [here(), idx()],
          `round ${round + 1}, the series page's Back`,
        ).toEqual(['/', 0])
      }
    })
  })

  /**
   * Opened from another series' page: the reader's way out steps back to
   * series 41 under it, whose Back steps back to series 60, the series page
   * staying mounted; Forward brings 41 back, and its Back steps back again,
   * since a press holds only until another entry is drawn (O434).
   */
  it('steps back from the series page again after Forward, round after round', async () => {
    progressRoute({ current: { edition: 'raw', slot: 0 } })
    realComic()
    const host = await appAtTheEnd(1280, '/series/60')
    await outsideAct(async () => {
      for (let round = 0; round < 3; round++) {
        await until(() => host.querySelector(VOLUME_1) !== null)
        await userEvent.click(need(host, VOLUME_1))
        await answered(host)
        // The bars' key is an input: by now the series page is under the
        // reader, whether it was put there as the answer landed or at the key.
        await comicBars(host)
        await seriesAnswered(host)
        const steps: unknown[] = [
          [wayOut(host).textContent, here(), idx(), beneath()],
        ]
        await userEvent.click(wayOut(host))
        await landing(host, '/read/work-1', 2)
        // The Back of the one series page drawn through every step after.
        const back = pageBack(host)
        steps.push([here(), idx(), back.textContent])
        await userEvent.click(back)
        await landing(host, '/series/41', 1)
        steps.push([here(), idx(), pageBack(host) === back])
        history.forward()
        await until(() => here() === '/series/41')
        await idle()
        steps.push([here(), idx(), pageBack(host) === back])
        await userEvent.click(pageBack(host))
        await landing(host, '/series/41', 1)
        steps.push([here(), idx()])
        expect(steps, `round ${round + 1}`).toEqual([
          ['Series', '/read/work-1', 2, '/series/41'],
          ['/series/41', 1, 'Series'],
          ['/series/60', 0, true],
          ['/series/41', 1, true],
          ['/series/60', 0],
        ])
      }
    })
  })
})

/**
 * "Back from reading" set to "Where you came from" on this device, under the
 * browse preferences' key, where the View panel keeps it: after the file's
 * `beforeEach` has cleared the store, and removed as the test ends, for the
 * files that run after this one (O431).
 */
function backToWhereYouCameFrom() {
  onTestFinished(() => localStorage.removeItem(BROWSE_KEY))
  localStorage.setItem(
    BROWSE_KEY,
    JSON.stringify({ backFromReading: 'whereYouCameFrom' }),
  )
}

/**
 * The comic reader under "Back from reading" set to "Where you came from"
 * (O431), opened by a click, so that under the default it would put its
 * series page under itself as its answer lands: from anywhere but its own
 * series page it puts nothing there, as the answer lands or at an input, and
 * its way out is O422's. The address and the router's index after each step,
 * three rounds, nothing in `act`. Chromium keeps 50 entries a tab, so no case
 * asserts a length grown: a push is read from the index, and the length only
 * over a step that must add no entry.
 */
describe('The comic reader’s way back under “Where you came from” (O431)', () => {
  /**
   * The comic reader's way out, its bars up from `Tab`: its button, or a key
   * from its page, where focus out of the bars lets `Escape` leave rather
   * than take the bars away.
   */
  async function leaveBy(host: HTMLElement, press: 'click' | 'L' | 'Escape') {
    if (press === 'click') {
      await userEvent.click(wayOut(host))
      return
    }
    need(host, '[data-reader-page]').focus()
    await userEvent.keyboard(press === 'L' ? 'L' : '{Escape}')
  }

  /**
   * From the Continue row: nothing is put under the reader, as its answer
   * lands or at the bars' key, and its way out, named for the library, steps
   * back to the row: by its button, then `L`, then `Escape`, one a round.
   */
  it('steps back to the Continue row, nothing put under the reader, by its button, `L` and `Escape`, round after round', async () => {
    backToWhereYouCameFrom()
    continuing()
    progressRoute({ current: { edition: 'raw', slot: 0 } })
    realComic()
    const host = await appAtTheEnd(1280, '/')
    await outsideAct(async () => {
      for (const [round, press] of (
        ['click', 'L', 'Escape'] as const
      ).entries()) {
        await until(() => host.querySelector(CARD_1) !== null)
        await userEvent.click(need(host, CARD_1))
        await answered(host)
        // Read before any input: the default puts the series page under the
        // reader here, and the Tab that brings the bars up is an input.
        const seen = [here(), idx(), beneath()]
        const entries = history.length
        await comicBars(host)
        await seriesAnswered(host)
        const opened = [
          wayOut(host).textContent,
          here(),
          idx(),
          history.length - entries,
          beneath(),
        ]
        await leaveBy(host, press)
        await untilLanded(host, '/read/work-1', 1)
        await until(() => host.querySelector(CARD_1) !== null).catch(
          () => undefined,
        )
        expect(
          [
            seen,
            opened,
            [
              here(),
              idx(),
              history.length - entries,
              host.querySelector(CARD_1) !== null,
            ],
          ],
          `round ${round + 1}, ${press}`,
        ).toEqual([
          ['/read/work-1', 1, undefined],
          ['Library', '/read/work-1', 1, 0, undefined],
          ['/', 0, 0, true],
        ])
      }
    })
  })

  /**
   * From its own series page, a query in the address: the reader stamps its
   * entry where it stands, a replace, as under the default, and its way out,
   * named for the page, steps back to it at its query and scroll.
   */
  it('stamps its entry opened from its own series page with a query, and steps back to the page at its scroll, round after round', async () => {
    backToWhereYouCameFrom()
    progressRoute({ current: { edition: 'raw', slot: 0 } })
    realComic()
    const page = `/series/${FILED.id}?edition=raw`
    const host = await appAtTheEnd(1280, page)
    await outsideAct(async () => {
      for (let round = 0; round < 3; round++) {
        await until(() => host.querySelector(VOLUME_1) !== null)
        // Volume 1 is near its page's top, nearer than the grid cases' 300 px.
        const y = above(host, need(host, VOLUME_1), 100) + round * 7
        scrollerAt(host).scrollTop = y
        await idle()
        const left = scrolled(host)
        // The work's answer held, so that the mark and the index are read
        // before the stamp as well as after it.
        const answer = deferred<library.WorkDetail>()
        vi.mocked(library.work).mockReturnValue(answer.promise)
        await userEvent.click(need(host, VOLUME_1))
        await until(() => cells(host).length > 0)
        await idle()
        const held = [here(), idx(), beneath()]
        const entries = history.length
        answer.resolve(filedWork())
        await answered(host)
        const stamped = [here(), idx(), history.length - entries, beneath()]
        await comicBars(host)
        await seriesAnswered(host)
        const word = wayOut(host).textContent
        await userEvent.click(wayOut(host))
        await untilLanded(host, '/read/work-1', 1)
        expect(
          [
            left,
            held,
            stamped,
            word,
            [here(), idx(), history.length - entries, scrolled(host)],
          ],
          `round ${round + 1}`,
        ).toEqual([
          at(y),
          ['/read/work-1', 1, undefined],
          ['/read/work-1', 1, 0, '/series/41'],
          'Series',
          [page, 0, 0, at(y)],
        ])
      }
    })
  })

  /**
   * From another series' page, opened from the grid: nothing is put under
   * the reader, and its way out, named for that page, steps back to it,
   * whose Back, named for the library, steps back to the grid.
   */
  it('steps back to another series’ page it was opened from, whose Back steps back to the grid, round after round', async () => {
    backToWhereYouCameFrom()
    progressRoute({ current: { edition: 'raw', slot: 0 } })
    realComic()
    const host = await appAtTheEnd(1280, '/')
    const tile = 'ul.grid a[href="/series/60"]'
    await outsideAct(async () => {
      for (let round = 0; round < 3; round++) {
        await until(() => host.querySelector(tile) !== null)
        await userEvent.click(need(host, tile))
        await until(() => host.querySelector(VOLUME_1) !== null)
        await userEvent.click(need(host, VOLUME_1))
        await answered(host)
        // Read before any input, as from the Continue row.
        const seen = [here(), idx(), beneath()]
        const entries = history.length
        await comicBars(host)
        await seriesAnswered(host)
        const opened = [
          wayOut(host).textContent,
          here(),
          idx(),
          history.length - entries,
          beneath(),
        ]
        await userEvent.click(wayOut(host))
        await untilLanded(host, '/read/work-1', 2)
        expect(
          [
            seen,
            opened,
            [
              here(),
              idx(),
              history.length - entries,
              host.querySelector('main .page-back')?.textContent,
            ],
          ],
          `round ${round + 1}, the reader's way out`,
        ).toEqual([
          ['/read/work-1', 2, undefined],
          ['Series', '/read/work-1', 2, 0, undefined],
          ['/series/60', 1, 0, 'Library'],
        ])
        await userEvent.click(pageBack(host))
        await untilLanded(host, '/series/60', 1)
        expect(
          [here(), idx(), history.length - entries],
          `round ${round + 1}, the series page's Back`,
        ).toEqual(['/', 0, 0])
      }
    })
  })
})

/**
 * "Back from reading" set to "Where you came from" (O431) for a comic reader
 * opened cold, for the book, and in the Library view's View panel, which the
 * next reader opened reads. The address and the router's index after each
 * step, nothing in `act`, and no length, since Chromium keeps 50 entries a
 * tab and a push past that adds none.
 */
describe('The reader’s way back under “Where you came from”, opened cold, in the book, and chosen in the View panel (O431)', () => {
  /**
   * `press`, a way out, and real time until it has landed, or until the wait
   * gives up: a press that goes nowhere is then read where it stayed.
   */
  async function leave(host: HTMLElement, press: () => Promise<unknown>) {
    const [path, at] = [here(), idx()]
    await press()
    await untilLanded(host, path, at).catch(() => undefined)
  }

  /**
   * In Firefox, opened cold with nothing under it: the first key puts nothing
   * under the reader, where the default puts the series page there, and the
   * way out, named for the library, pushes `/`. Each round is a cold open of
   * its own.
   */
  it('in Firefox, opened cold, puts nothing under the reader at the first key, and its way out pushes the library, round after round', async () => {
    backToWhereYouCameFrom()
    asFirefox()
    // No Library view drawn yet in this tab, as in a new one.
    forgetScroll()
    progressRoute({ current: { edition: 'raw', slot: 0 } })
    realComic()
    for (let round = 0; round < 3; round++) {
      let host: HTMLElement
      if (round === 0) host = await appAtTheEnd(1280, '/read/work-1')
      else {
        rerender(null)
        history.pushState(null, '', '/read/work-1')
        host = await draw()
      }
      await outsideAct(async () => {
        await answered(host)
        const before = [here(), idx(), beneath()]
        const shown = cells(host)[0]
        need(host, '[data-reader-page]').focus()
        await userEvent.keyboard('{ArrowRight}')
        await until(() => cells(host)[0] !== shown)
        await idle()
        const keyed = [here(), idx(), beneath()]
        await comicBars(host)
        await seriesAnswered(host)
        const word = wayOut(host).textContent
        await leave(host, () => userEvent.click(wayOut(host)))
        expect(
          [before, keyed, word, [here(), idx()]],
          `round ${round + 1}`,
        ).toEqual([
          ['/read/work-1', 0, undefined],
          ['/read/work-1', 0, undefined],
          'Library',
          ['/', 1],
        ])
      })
    }
  })

  /**
   * The book, from the Continue row: nothing is put under it, as its answer
   * lands or at the tap that brings its bars up, and its way out, named for
   * the library, steps back to the row: by its button, then `L`, then its
   * button again.
   */
  it('steps back from the book to the Continue row, nothing put under it, by its button and by `L`, round after round', async () => {
    backToWhereYouCameFrom()
    vi.mocked(library.home).mockResolvedValue({
      continuing: [volume(1, true)],
      added: [],
      read: [],
    })
    vi.mocked(library.work).mockResolvedValue(filedWork(FILED, true))
    vi.mocked(bookApi.getBook).mockResolvedValue(BOOK)
    progressRoute({ current: { edition: 'en', slot: 0 } })
    const host = await appAtTheEnd(1280, '/')
    const card = '.continue a[href="/book/work-1"]'
    await outsideAct(async () => {
      for (const [round, press] of (
        ['click', 'L', 'click'] as const
      ).entries()) {
        await until(() => host.querySelector(card) !== null)
        await userEvent.click(need(host, card))
        await until(() => topBlock(host) !== undefined)
        await vi.mocked(library.work).mock.results.at(-1)?.value
        await idle()
        // Read before any input: the default puts the series page under the
        // book here, and the tap that brings the bars up is an input.
        const seen = [here(), idx(), beneath()]
        await barsUp(host)
        await seriesAnswered(host)
        const opened = [wayOut(host).textContent, here(), idx(), beneath()]
        await leave(host, () =>
          press === 'click'
            ? userEvent.click(wayOut(host))
            : userEvent.keyboard('L'),
        )
        await until(() => host.querySelector(card) !== null).catch(
          () => undefined,
        )
        expect(
          [seen, opened, [here(), idx(), host.querySelector(card) !== null]],
          `round ${round + 1}, ${press}`,
        ).toEqual([
          ['/book/work-1', 1, undefined],
          ['Library', '/book/work-1', 1, undefined],
          ['/', 0, true],
        ])
      }
    })
  })

  /**
   * The setting chosen in the View panel, as a reader would choose it: under
   * "Where you came from" volume 1, opened from the Continue row, steps back
   * to the row; under "Series page" again, opened again, it steps back to its
   * series page put under it, since each reader reads the setting as it
   * opens.
   */
  it('takes the way back chosen in the View panel for the next reader opened, each choice in turn', async () => {
    onTestFinished(() => localStorage.removeItem(BROWSE_KEY))
    continuing()
    progressRoute({ current: { edition: 'raw', slot: 0 } })
    realComic()
    const host = await appAtTheEnd(1280, '/')
    await outsideAct(async () => {
      const steps: unknown[] = []
      for (const choice of ['Where you came from', 'Series page']) {
        await until(() => host.querySelector(CARD_1) !== null)
        await userEvent.click(need(host, 'main .view-button'))
        await userEvent.selectOptions(
          page.getByRole('combobox', { name: 'Back from reading' }),
          choice,
        )
        await userEvent.click(need(host, 'main .view-button'))
        await idle()
        await userEvent.click(need(host, CARD_1))
        await answered(host)
        await comicBars(host)
        await seriesAnswered(host)
        const word = wayOut(host).textContent
        await leave(host, () => userEvent.click(wayOut(host)))
        steps.push([choice, word, here(), idx()])
      }
      expect(steps).toEqual([
        ['Where you came from', 'Library', '/', 0],
        ['Series page', 'Series', '/series/41', 1],
      ])
    })
  })
})
