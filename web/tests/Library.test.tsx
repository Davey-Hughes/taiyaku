import { act } from 'react'
import {
  Link,
  MemoryRouter,
  type MemoryRouterProps,
  Route,
  Routes,
  useLocation,
  useNavigate,
  useNavigationType,
} from 'react-router'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { Me } from '../src/api'
import {
  BrowseControlsContext,
  type BrowsePrefs,
  BrowsePrefsContext,
  browseModule,
} from '../src/browseModule'
import { GateContext } from '../src/gate'
import * as library from '../src/library'
import type { LibraryStatus } from '../src/library'
import { Library } from '../src/Library'
import * as matches from '../src/matches'
import * as metadata from '../src/metadata'
import { invalidateLibraryViews } from '../src/pageReads'
import { PHONE_LAYOUT_PARTS } from '../src/phoneLayout'
import { SCAN_POLL_MS } from '../src/config'
import type { ScanFeed } from '../src/scanFeed'
import { SEARCH_BOX_ID } from '../src/searchHits'
import { STATUS_FAILED, StatusContext } from '../src/shell'
import { Sidebar } from '../src/Sidebar'
import { deferred, flush, land } from './support/async'
import { emptyDerive } from './support/derive'
import { FirstCommit } from './support/firstCommit'
import { press, submit, typeInto } from './support/input'
import { mount, rerender } from './support/mount'
import { feedJob, serveFeed } from './support/scanFeed'
import { StatusFromStore } from './support/statusFromStore'
import {
  holdTheClock,
  pass,
  WAIT_DRAWN_AFTER_MS,
  watchDrawn,
} from './support/wait'

vi.mock('../src/library', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/library')>()),
  home: vi.fn(),
  grid: vi.fn(),
  search: vi.fn(),
  // The Continue row's panel reads its card's work as it opens.
  work: vi.fn(),
}))

vi.mock('../src/metadata', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/metadata')>()),
  patchWork: vi.fn(),
}))

vi.mock('../src/matches', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/matches')>()),
  // A Split pressed in the Continue row's panel.
  splitMatch: vi.fn(),
}))

/**
 * The address the server sends for page 0 of `slug`'s `key` edition. Its
 * version is opaque to the client, which hands it back untouched.
 */
const coverOf = (slug: string, key: string) =>
  `/api/works/${slug}/${key}/pages/0/thumb?v=c0ffee-0-1`

const series = (
  over: Partial<library.SeriesCard> = {},
): library.SeriesCard => ({
  id: 1,
  title: 'A Series',
  works: 3,
  editions: 3,
  aligned: 1,
  added: 10,
  unread: 3,
  cover: coverOf('a-series-v01', 'raw'),
  ...over,
})

const work = (over: Partial<library.WorkCard> = {}): library.WorkCard => ({
  id: 1,
  slug: 'a-series-v01',
  title: 'A Series v01',
  volume: 1,
  aligned: true,
  pages: 180,
  editions: [
    {
      key: 'raw',
      label: 'Raw',
      members: 1,
      pages: 180,
      missing: 0,
      cover: coverOf(over.slug ?? 'a-series-v01', 'raw'),
    },
  ],
  opens: 'raw',
  ...over,
})

const defaultStatus: LibraryStatus = {
  libraries: [{ id: 1, name: 'Manga', scanned: true }],
  restricted: false,
}

/** Two libraries, both read, for the tests that move from one to the other. */
const twoLibraries: LibraryStatus = {
  libraries: [
    { id: 1, name: 'Manga', scanned: true },
    { id: 2, name: 'Comics', scanned: true },
  ],
  restricted: false,
}

/**
 * Mount at `path` and let the two reads settle. A library's page is
 * `/?library=N`.
 *
 * A second `act` after the mount's: each read waits for the progress saves
 * sent before it, then calls its mocked client function, and the store
 * applies the two answers as one set once the last has settled, so the
 * screen is drawn from them only some microtask turns after the mount.
 *
 * **The gate's context is supplied here rather than the gate itself.** `Gate`
 * would bring `me()`, `Login` and a real `fetch` along with it, none of which
 * this file is about — and every one of them is pinned in `Gate.test.tsx`
 * against a stubbed server. `who` defaults to `undefined`, which is what a
 * screen mounted with no gate above it genuinely reads.
 *
 * **The status is supplied through `StatusContext` directly rather than
 * mocked.** `Shell` owns the `status()` fetch — `Library` only reads the
 * context it publishes — so a test of `Library` alone hands it the value
 * `Shell` would have resolved to, synchronously, the way `Shell.test.tsx`
 * hands `Shell` itself a mocked fetch.
 */
const showAt = async (
  path: string,
  status: LibraryStatus = defaultStatus,
  who?: Me,
) => {
  let host!: HTMLElement
  const screen = (
    <MemoryRouter initialEntries={[path]}>
      <StatusContext.Provider value={status}>
        <Library />
      </StatusContext.Provider>
    </MemoryRouter>
  )
  await act(async () => {
    host = mount(
      who === undefined ? (
        screen
      ) : (
        <GateContext value={{ who, signOut: () => undefined }}>
          {screen}
        </GateContext>
      ),
    )
  })
  await act(async () => {
    await Promise.resolve()
    await Promise.resolve()
  })
  return host
}

/** `showAt` the page of every library. */
const show = (who?: Me, status: LibraryStatus = defaultStatus) =>
  showAt('/', status, who)

/** What the empty state says, its heading and each line under it. */
const said = (host: HTMLElement) => {
  const empty = host.querySelector('.empty')
  return [
    empty?.querySelector('h2')?.textContent,
    ...[...(empty?.querySelectorAll('p') ?? [])].map((p) => p.textContent),
  ]
}

/** What a read library with no works says with no chip or search set (O520). */
const nothingToRead = [
  'Nothing to read yet',
  'The scans so far found no series or volume to read.',
]

describe('the library screen', () => {
  beforeEach(() => {
    vi.mocked(library.home).mockResolvedValue({
      continuing: [],
      added: [],
      read: [],
    })
    vi.mocked(library.grid).mockResolvedValue({ series: [], unfiled: [] })
  })

  it('lists every series it was given, with a link to each', async () => {
    vi.mocked(library.grid).mockResolvedValue({
      series: [
        series({ id: 1, title: 'One' }),
        series({ id: 2, title: 'Two' }),
      ],
      unfiled: [],
    })
    const host = await show()
    const links = [...host.querySelectorAll('a[href^="/series/"]')]
    expect(links.map((a) => a.getAttribute('href'))).toEqual([
      '/series/1',
      '/series/2',
    ])
  })

  it('links each tile of a library’s page to its series as that library holds it — spec §3.6', async () => {
    vi.mocked(library.grid).mockResolvedValue({
      series: [
        series({ id: 1, title: 'One' }),
        series({ id: 2, title: 'Two' }),
      ],
      unfiled: [],
    })
    const host = await showAt('/?library=2', twoLibraries)
    const links = [...host.querySelectorAll('a[href^="/series/"]')]
    expect(links.map((a) => a.getAttribute('href'))).toEqual([
      '/series/1?library=2',
      '/series/2?library=2',
    ])
  })

  it('marks how many volumes of a series are aligned', async () => {
    // `aligned` distinct from `works`, `editions` and `unread` — the default
    // fixture's `works: 3, editions: 3, unread: 3` would all equal `aligned`
    // if this only overrode `aligned` to another `3`, and `toContain('3')`
    // could not tell which field it actually saw.
    vi.mocked(library.grid).mockResolvedValue({
      series: [series({ works: 3, editions: 3, unread: 3, aligned: 7 })],
      unfiled: [],
    })
    const host = await show()
    // The rendered mark itself, not a bare digit anywhere on the page.
    expect(host.querySelector('.aligned')?.textContent).toBe('⇄ 7')
    // Spec C1 §3 puts the mark *on the cover*, and `styles/library.css` has
    // exactly one rule for it — `.cover-box .aligned` — so outside that box the
    // mark loses its positioning and its chip. `textContent` alone cannot tell
    // "on the cover" from "moved below the title"; only the ancestor can.
    expect(host.querySelector('.cover-box .aligned')).not.toBeNull()
  })

  it('links an unfiled work straight into the reader, since it has no series page', async () => {
    vi.mocked(library.grid).mockResolvedValue({
      series: [],
      unfiled: [work({ slug: 'imported-01' })],
    })
    const host = await show()
    expect(host.querySelector('a[href="/read/imported-01"]')).not.toBeNull()
  })

  /**
   * `SeriesGrid` draws the unfiled works of the Library view's grid and of
   * its search hits, and its reader link is under O62's guard too.
   */
  it('withholds an unfiled work’s reader link when every edition it holds has lost its files', async () => {
    vi.mocked(library.grid).mockResolvedValue({
      series: [],
      unfiled: [
        work({
          slug: 'imported-01',
          editions: [
            {
              key: 'raw',
              label: 'Raw',
              members: 1,
              pages: 180,
              missing: 2,
              cover: coverOf('imported-01', 'raw'),
            },
          ],
        }),
      ],
    })
    const host = await show()
    expect(host.querySelector('a[href="/read/imported-01"]')).toBeNull()
    // Still named, and still says why.
    expect(host.textContent).toContain('A Series v01')
    expect(host.querySelector('.unreadable')?.textContent).toBeTruthy()
  })

  it('keeps an unfiled work’s reader link when the edition it opens is the intact one', async () => {
    vi.mocked(library.grid).mockResolvedValue({
      series: [],
      unfiled: [
        work({
          slug: 'imported-01',
          opens: 'en',
          editions: [
            {
              key: 'raw',
              label: 'Raw',
              members: 1,
              pages: 180,
              missing: 2,
              cover: coverOf('imported-01', 'raw'),
            },
            {
              key: 'en',
              label: 'En',
              members: 1,
              pages: 178,
              missing: 0,
              cover: coverOf('imported-01', 'en'),
            },
          ],
        }),
      ],
    })
    const host = await show()
    expect(host.querySelector('a[href="/read/imported-01"]')).not.toBeNull()
  })

  /**
   * **O84, and the reason `opens` is the caller's edition rather than the
   * hub.** A work whose opening edition has lost its files is not readable,
   * even though a sibling is intact — the link would fail at the first page,
   * which is spec C1 §8's exact prohibition. The mirror of the test above.
   */
  it('withholds the continue-strip link when the edition it opens has lost its files', async () => {
    vi.mocked(library.home).mockResolvedValue({
      continuing: [
        work({
          slug: 'half-read-v02',
          opens: 'raw',
          editions: [
            {
              key: 'raw',
              label: 'Raw',
              members: 1,
              pages: 180,
              missing: 2,
              cover: coverOf('half-read-v02', 'raw'),
            },
            {
              key: 'en',
              label: 'En',
              members: 1,
              pages: 178,
              missing: 0,
              cover: coverOf('half-read-v02', 'en'),
            },
          ],
        }),
      ],
      added: [],
      read: [],
    })
    const host = await show()
    expect(host.querySelector('a[href="/read/half-read-v02"]')).toBeNull()
  })

  it('the multi-edition chip keeps only the series with a second edition', async () => {
    vi.mocked(library.grid).mockResolvedValue({
      series: [
        series({ id: 1, title: 'Single', works: 3, editions: 3 }),
        series({ id: 2, title: 'Double', works: 3, editions: 5 }),
      ],
      unfiled: [],
    })
    const host = await show()
    const chip = [...host.querySelectorAll('button')].find((b) =>
      /multi/i.test(b.textContent ?? ''),
    )
    expect(chip, 'a multi-edition chip').toBeDefined()
    await act(async () => {
      chip?.click()
      await Promise.resolve()
    })
    expect(host.textContent).toContain('Double')
    expect(host.textContent).not.toContain('Single')
  })

  it('the continue strip links to what was being read', async () => {
    vi.mocked(library.home).mockResolvedValue({
      continuing: [work({ slug: 'half-read-v02' })],
      added: [],
      read: [],
    })
    const host = await show()
    expect(host.querySelector('a[href="/read/half-read-v02"]')).not.toBeNull()
  })

  it('draws no continue strip when nothing is half-read', async () => {
    const host = await show()
    expect(host.querySelector('[data-testid="continue"]')).toBeNull()
  })

  /**
   * O62's guard (spec C1 §8) covers the Continue strip as well as `Table` in
   * `Series.tsx`: the strip is the likeliest place in the product to meet a
   * work whose files just vanished, because the reader was mid-read when
   * they went.
   */
  it('withholds the continue-strip link for a work whose every edition has lost its files', async () => {
    vi.mocked(library.home).mockResolvedValue({
      continuing: [
        work({
          slug: 'half-read-v02',
          editions: [
            {
              key: 'raw',
              label: 'Raw',
              members: 1,
              pages: 180,
              missing: 2,
              cover: coverOf('half-read-v02', 'raw'),
            },
          ],
        }),
      ],
      added: [],
      read: [],
    })
    const host = await show()
    expect(host.querySelector('a[href="/read/half-read-v02"]')).toBeNull()
    // Still named, and still says why — the same rule `Series.tsx`'s `Table`
    // follows: withholding the link must not withhold the work.
    const strip = host.querySelector('[data-testid="continue"]')
    expect(strip?.textContent).toContain('A Series v01')
    expect(strip?.querySelector('.unreadable')?.textContent).toBeTruthy()
  })

  it('keeps the continue-strip link when the edition it opens is the intact one', async () => {
    vi.mocked(library.home).mockResolvedValue({
      continuing: [
        work({
          slug: 'half-read-v02',
          opens: 'en',
          editions: [
            {
              key: 'raw',
              label: 'Raw',
              members: 1,
              pages: 180,
              missing: 2,
              cover: coverOf('half-read-v02', 'raw'),
            },
            {
              key: 'en',
              label: 'En',
              members: 1,
              pages: 178,
              missing: 0,
              cover: coverOf('half-read-v02', 'en'),
            },
          ],
        }),
      ],
      added: [],
      read: [],
    })
    const host = await show()
    expect(host.querySelector('a[href="/read/half-read-v02"]')).not.toBeNull()
  })

  it('a server with no library is told so, rather than shown an empty grid', async () => {
    const host = await show(undefined, { libraries: [], restricted: false })
    expect(host.textContent).toContain('No library yet')
    expect(host.textContent).toContain('Configuration → Libraries')
    expect(host.textContent).not.toContain('No library shared')
  })

  it('an account granted no library is told none is shared, not that the server has none', async () => {
    const host = await show(undefined, { libraries: [], restricted: true })
    expect(host.querySelector('.empty h2')?.textContent).toBe(
      'No library shared',
    )
    expect(host.textContent).toContain('Configuration → Users')
    expect(host.textContent).not.toContain('No library yet')
  })

  it('an account granted a library not yet read is told it is not read yet', async () => {
    const host = await show(undefined, {
      libraries: [{ id: 1, name: 'Manga', scanned: false }],
      restricted: true,
    })
    expect(host.querySelector('.empty h2')?.textContent).toBe('Not read yet')
  })

  it('a library page asks whether its own library has been read', async () => {
    const status = {
      libraries: [
        { id: 1, name: 'Manga', scanned: true },
        { id: 2, name: 'Comics', scanned: false },
      ],
      restricted: false,
    }
    const comics = await showAt('/?library=2', status)
    expect(said(comics)[0]).toBe('Not read yet')
    const manga = await showAt('/?library=1', status)
    expect(said(manga)).toEqual(nothingToRead)
  })

  it('the page of every library is not read yet only while none is', async () => {
    const one = await show(undefined, {
      libraries: [
        { id: 1, name: 'Manga', scanned: false },
        { id: 2, name: 'Comics', scanned: true },
      ],
      restricted: false,
    })
    expect(said(one)).toEqual(nothingToRead)
    const none = await show(undefined, {
      libraries: [{ id: 1, name: 'Manga', scanned: false }],
      restricted: false,
    })
    expect(said(none)[0]).toBe('Not read yet')
  })

  it('a library the status does not list is said to be no library here', async () => {
    const host = await showAt('/?library=7', {
      libraries: [{ id: 1, name: 'Manga', scanned: true }],
      restricted: false,
    })
    expect(host.textContent).toContain('No such library')
  })

  it('a library page for an account granted no library says none is shared', async () => {
    const host = await showAt('/?library=7', {
      libraries: [],
      restricted: true,
    })
    expect(host.querySelector('.empty h2')?.textContent).toBe(
      'No library shared',
    )
    expect(host.textContent).not.toContain('No such library')
  })

  it('a library page is no library here on a server with none, or for an account granted another', async () => {
    const none = await showAt('/?library=7', {
      libraries: [],
      restricted: false,
    })
    expect(none.querySelector('.empty h2')?.textContent).toBe('No such library')
    const another = await showAt('/?library=7', {
      libraries: [{ id: 1, name: 'Manga', scanned: true }],
      restricted: true,
    })
    expect(another.querySelector('.empty h2')?.textContent).toBe(
      'No such library',
    )
  })

  it('a library page for an account granted that library asks whether it has been read', async () => {
    const read = await showAt('/?library=1', {
      libraries: [{ id: 1, name: 'Manga', scanned: true }],
      restricted: true,
    })
    expect(said(read)).toEqual(nothingToRead)
    const unread = await showAt('/?library=1', {
      libraries: [{ id: 1, name: 'Manga', scanned: false }],
      restricted: true,
    })
    expect(said(unread)[0]).toBe('Not read yet')
  })

  it('a read library with no works says so with no filter set, and a chip or a search still says nothing matched', async () => {
    vi.mocked(library.search).mockResolvedValue({
      series: [],
      unfiled: [],
      query: '花嫁',
    })
    // The read library listed second: the page of every library asks
    // whether any library is read, not whether the first is.
    const status: LibraryStatus = {
      libraries: [
        { id: 2, name: 'Comics', scanned: false },
        { id: 1, name: 'Manga', scanned: true },
      ],
      restricted: false,
    }
    expect(said(await showAt('/?library=1', status))).toEqual(nothingToRead)
    expect(said(await showAt('/?library=1&show=unread', status))).toEqual([
      'Nothing here',
      'No series match this filter.',
    ])
    expect(said(await showAt('/?library=1&q=花嫁', status))).toEqual([
      'Nothing here',
      'Nothing matched 花嫁.',
    ])
    expect(said(await showAt('/', status))).toEqual(nothingToRead)
    expect(said(await showAt('/?show=unread', status))).toEqual([
      'Nothing here',
      'No series match this filter.',
    ])
  })

  describe('the empty view while a scan holds its library (O311)', () => {
    /** Lets `ms` of the clock run, and the reads sent meanwhile land. */
    const clock = (ms: number) =>
      act(async () => {
        await vi.advanceTimersByTimeAsync(ms)
      })

    /** Manga, library 1, read or not. */
    const manga = (scanned: boolean): LibraryStatus => ({
      libraries: [{ id: 1, name: 'Manga', scanned }],
      restricted: false,
    })

    /** The feed as an administrator is sent it, while job 7 reads library `id`'s files. */
    const adminFeed = (id = 1): ScanFeed => ({
      ended: 0,
      libraries: [{ id, step: 'resolving' }],
      jobs: [feedJob({ libraries: [{ id, name: 'Manga' }] })],
    })

    /** `showAt`, and the feed's first answer landed. */
    const shownAt = async (path: string, status: LibraryStatus) => {
      const host = await showAt(path, status)
      await clock(0)
      return host
    }

    let probe: MutationObserver | undefined

    /**
     * Every heading an empty state draws from now on, in order, once each as
     * it changes: what is seen on the way, not only where the view ends.
     */
    const headingsDrawn = () => {
      const drawn: string[] = []
      probe = new MutationObserver(() => {
        const now = document.querySelector('.empty h2')?.textContent
        if (now != null && now !== drawn.at(-1)) drawn.push(now)
      })
      probe.observe(document.body, {
        childList: true,
        subtree: true,
        characterData: true,
      })
      return drawn
    }

    beforeEach(() => {
      vi.useFakeTimers({ shouldAdvanceTime: true })
    })

    afterEach(() => {
      probe?.disconnect()
      probe = undefined
      vi.useRealTimers()
      // jsdom's own getter, on the prototype, answers again.
      Reflect.deleteProperty(document, 'visibilityState')
    })

    /** The tab turns `state`, as the browser tells a page. */
    const turn = (state: DocumentVisibilityState) =>
      act(async () => {
        Object.defineProperty(document, 'visibilityState', {
          value: state,
          configurable: true,
        })
        document.dispatchEvent(new Event('visibilitychange'))
      })

    it('a library a job holds draws Scanning, never Not read yet', async () => {
      const drawn = headingsDrawn()
      serveFeed(adminFeed())
      const host = await shownAt('/?library=1', manga(false))
      expect(drawn).toEqual(['Scanning'])
      expect(said(host)).toEqual([
        'Scanning',
        'Reading files · 120 of 2,097 · directory 1 of 1',
      ])
    })

    it('a library a job holds draws Scanning, never Nothing to read yet', async () => {
      const drawn = headingsDrawn()
      serveFeed(adminFeed())
      const host = await shownAt('/?library=1', manga(true))
      expect(drawn).toEqual(['Scanning'])
      expect(said(host)).toEqual([
        'Scanning',
        'Reading files · 120 of 2,097 · directory 1 of 1',
      ])
    })

    it('a view drawn in a hidden tab waits for the feed until the tab is shown, and never says Not read yet', async () => {
      const drawn = headingsDrawn()
      const feed = serveFeed(adminFeed())
      Object.defineProperty(document, 'visibilityState', {
        value: 'hidden',
        configurable: true,
      })
      const host = await shownAt('/?library=1', manga(false))
      await clock(WAIT_DRAWN_AFTER_MS)
      const whileHidden = [
        host.querySelector('[role="status"]')?.textContent,
        feed.polls(),
      ]
      await turn('visible')
      await clock(0)
      expect([whileHidden, drawn]).toEqual([['Loading…', 0], ['Scanning']])
      expect(said(host)[0]).toBe('Scanning')
    })

    it('a view drawn in a hidden tab falls back to the status when the feed cannot be read once shown', async () => {
      serveFeed(500)
      Object.defineProperty(document, 'visibilityState', {
        value: 'hidden',
        configurable: true,
      })
      const host = await shownAt('/?library=1', manga(false))
      const whileHidden = host.querySelector('.empty')
      await turn('visible')
      await clock(0)
      expect([whileHidden, said(host)[0]]).toEqual([null, 'Not read yet'])
    })

    it('a job that starts while the view is drawn shows within one idle poll', async () => {
      const feed = serveFeed({ ended: 0, libraries: [] })
      const host = await shownAt('/?library=1', manga(false))
      const atMount = said(host)[0]
      feed.answer(adminFeed())
      // The pinned idle poll, not the constant, so that a slower poll fails.
      await clock(5000)
      expect([atMount, said(host)[0]]).toEqual(['Not read yet', 'Scanning'])
    })

    it('an arrival draws nothing from a feed answer kept from a view that has gone', async () => {
      const feed = serveFeed({ ended: 0, libraries: [] })
      await shownAt('/?library=1', manga(false))
      // The view goes, and the feed's last answer, no job, is kept.
      act(() => rerender(<div />))
      feed.answer(adminFeed())
      const drawn = headingsDrawn()
      const host = await shownAt('/?library=1', manga(false))
      expect(drawn).toEqual(['Scanning'])
      expect(said(host)[0]).toBe('Scanning')
    })

    it('draws the wait, and no state, until the feed answers', async () => {
      serveFeed(adminFeed())
      // Every ask for the feed held until `answer` is called.
      const answer = deferred<void>()
      const served = globalThis.fetch
      vi.stubGlobal('fetch', (input: RequestInfo | URL, init?: RequestInit) =>
        String(input) === '/api/library/scans'
          ? answer.promise.then(() => served(input, init))
          : served(input, init),
      )
      const host = await shownAt('/?library=1', manga(false))
      await clock(WAIT_DRAWN_AFTER_MS)
      expect(host.querySelector('.empty')).toBeNull()
      expect(host.querySelector('[role="status"]')?.textContent).toBe(
        'Loading…',
      )
      answer.resolve()
      await clock(0)
      expect(host.querySelector('[role="status"]')).toBeNull()
      expect(said(host)[0]).toBe('Scanning')
    })

    it('draws the status’s state when the feed cannot be read, and no scan an answer kept from before showed', async () => {
      const feed = serveFeed(adminFeed())
      await shownAt('/?library=1', manga(false))
      act(() => rerender(<div />))
      feed.answer(500)
      const host = await shownAt('/?library=1', manga(false))
      expect(said(host)[0]).toBe('Not read yet')
      const filtered = await shownAt('/?library=1&show=unread', manga(false))
      expect(said(filtered)).toEqual([
        'Nothing here',
        'No series match this filter.',
      ])
    })

    it('a library’s page draws Scanning for a job holding its own library, and the page of every library for one holding any it lists', async () => {
      serveFeed(adminFeed(2))
      const status: LibraryStatus = {
        libraries: [
          { id: 1, name: 'Manga', scanned: false },
          { id: 2, name: 'Comics', scanned: true },
        ],
        restricted: false,
      }
      const own = await shownAt('/?library=1', status)
      expect(said(own)[0]).toBe('Not read yet')
      const every = await shownAt('/', status)
      expect(said(every)).toEqual([
        'Scanning',
        'Reading files · 120 of 2,097 · directory 1 of 1',
      ])
    })

    it('a reader sees the step alone', async () => {
      serveFeed({ ended: 0, libraries: [{ id: 1, step: 'hashing' }] })
      const host = await shownAt('/?library=1', manga(false))
      expect(said(host)).toEqual(['Scanning', 'Hashing'])
      expect(host.querySelector('.empty')?.textContent).not.toMatch(/\d/)
    })

    it('a paused never-scanned library shows Not read yet to a reader', async () => {
      // A cancelled first scan: no job holds the library, and the feed sends a
      // reader nothing of the pause.
      serveFeed({ ended: 3, libraries: [] })
      const host = await shownAt('/?library=1', manga(false))
      expect(said(host)[0]).toBe('Not read yet')
    })

    it('with a search active the view keeps its no-match state and says a scan is still running', async () => {
      vi.mocked(library.search).mockResolvedValue({
        series: [],
        unfiled: [],
        query: '花嫁',
      })
      serveFeed(adminFeed())
      const host = await shownAt('/?library=1&q=花嫁', manga(false))
      expect(said(host)).toEqual([
        'Nothing here',
        'Nothing matched 花嫁.',
        'A scan is still running.',
      ])
    })

    it('a search that finds nothing before the status lands is no match, and says a scan is still running', async () => {
      // The box is offered on the grid's rows before the status lands.
      vi.mocked(library.grid).mockResolvedValue({
        series: [series()],
        unfiled: [],
      })
      vi.mocked(library.search).mockResolvedValue({
        series: [],
        unfiled: [],
        query: '花嫁',
      })
      serveFeed(adminFeed())
      let host!: HTMLElement
      await act(async () => {
        host = mount(
          <MemoryRouter initialEntries={['/?library=1&q=花嫁']}>
            <Library />
          </MemoryRouter>,
        )
      })
      await clock(0)
      expect(said(host)).toEqual([
        'Nothing here',
        'Nothing matched 花嫁.',
        'A scan is still running.',
      ])
    })

    it('with a chip filter active the view keeps its no-match state and says a scan is still running', async () => {
      serveFeed(adminFeed())
      const host = await shownAt('/?library=1&show=unread', manga(false))
      expect(said(host)).toEqual([
        'Nothing here',
        'No series match this filter.',
        'A scan is still running.',
      ])
    })

    it('with a chip filter active and no scan, a library not read yet draws no match, never Not read yet', async () => {
      // A first scan cancelled between works leaves works and `scanned`
      // false: Not read yet under a filter could sit over a library with
      // works the filter hides.
      serveFeed({ ended: 1, libraries: [] })
      const host = await shownAt('/?library=1&show=unread', manga(false))
      expect(said(host)).toEqual([
        'Nothing here',
        'No series match this filter.',
      ])
    })

    it('the view polls the feed while it draws works: every five seconds with no scan, every second with one, never while hidden', async () => {
      vi.mocked(library.grid).mockResolvedValue({
        series: [series()],
        unfiled: [],
      })
      const feed = serveFeed({ ended: 0, libraries: [] })
      const host = await shownAt('/?library=1', manga(true))
      expect(host.querySelectorAll('a[href^="/series/"]')).toHaveLength(1)
      const polls = [feed.polls()]
      // The pinned intervals, not the constants, so that a slower poll fails.
      await clock(1000)
      polls.push(feed.polls())
      feed.answer(adminFeed())
      // The scan's answer lands at the idle poll, and the next waits a second.
      await clock(4000)
      polls.push(feed.polls())
      await clock(1000)
      polls.push(feed.polls())
      await clock(1000)
      polls.push(feed.polls())
      await turn('hidden')
      await clock(20000)
      polls.push(feed.polls())
      await turn('visible')
      polls.push(feed.polls())
      expect(polls).toEqual([1, 1, 2, 3, 4, 4, 5])
    })

    describe('the scan line over works (O314)', () => {
      const alpha = series({ id: 1, title: 'Alpha' })
      const beta = series({ id: 2, title: 'Beta' })

      /** The scan line's text, `undefined` while none is drawn. */
      const lineOf = (host: Element | null | undefined) =>
        host?.querySelector('.scan-line')?.textContent

      /** Each series card's link, in the order drawn. */
      const cards = (host: Element | null | undefined) =>
        [...(host?.querySelectorAll('a[href^="/series/"]') ?? [])].map((a) =>
          a.getAttribute('href'),
        )

      /**
       * The feed as an administrator is sent it while job 7 hashes library 1,
       * its early passes having written `ready` volumes, under marker
       * `written`.
       */
      const hashing = (ready: number, written: number): ScanFeed => ({
        ended: 0,
        written,
        libraries: [{ id: 1, step: 'hashing' }],
        jobs: [
          feedJob({
            step: 'hashing',
            done: 1024 ** 2,
            of: 5 * 1024 ** 3,
            ready,
            libraries: [{ id: 1, name: 'Manga' }],
          }),
        ],
      })

      /** The feed as a reader is sent it while a scan hashes library 1. */
      const readerHashing: ScanFeed = {
        ended: 0,
        libraries: [{ id: 1, step: 'hashing' }],
      }

      /** What the scan line draws: its words, its room alone, or nothing. */
      const lineState = (host: Element) => {
        const line = host.querySelector('.scan-line')
        if (line === null) return 'none'
        return line.hasAttribute('data-stale') ? 'room' : 'words'
      }

      /**
       * The scan line's state at each change to `host` from now on that
       * leaves a card drawn. The room filling with words changes only the
       * line's attribute, so attributes are watched too.
       */
      const linesDrawn = (host: Element) => {
        const frames: string[] = []
        probe = new MutationObserver(() => {
          if (host.querySelector('a[href^="/series/"]') !== null)
            frames.push(lineState(host))
        })
        probe.observe(host, {
          childList: true,
          subtree: true,
          characterData: true,
          attributes: true,
          attributeFilter: ['data-stale'],
        })
        return frames
      }

      /** `frames` with each run of one state cut to one. */
      const runsOf = (frames: string[]) =>
        frames.filter((frame, i) => frame !== frames[i - 1])

      /**
       * Draws library 1's view on a scan running, leaves it by drawing the
       * root with no `Library`, lets `away` of the clock run, and returns
       * to it, a remount, with the feed's answer `next` held until the
       * return has drawn. Hands back the host, the runs of the line's states
       * drawn before that answer lands, and the line's state at each change
       * from the return on, with a last one once that answer has landed.
       */
      const leaveAndReturn = async (away: number, next: ScanFeed) => {
        const feed = serveFeed(readerHashing)
        const host = await shownAt('/?library=1', manga(true))
        expect(lineState(host)).toBe('words')
        rerender(<p>Elsewhere</p>)
        await clock(away)
        feed.answer(next)
        const release = feed.hold()
        const frames = linesDrawn(host)
        rerender(
          <MemoryRouter initialEntries={['/?library=1']}>
            <StatusContext.Provider value={manga(true)}>
              <Library />
            </StatusContext.Provider>
          </MemoryRouter>,
        )
        await clock(0)
        // Taken before the last state is added, which alone would pass a
        // case expecting the words throughout.
        const atReturn = runsOf(frames)
        release()
        await clock(SCAN_POLL_MS)
        frames.push(lineState(host))
        return { host, atReturn, frames }
      }

      beforeEach(() => {
        vi.mocked(library.grid).mockResolvedValue({
          series: [alpha],
          unfiled: [],
        })
      })

      it('a view drawing works says a scan is running, draws the series a pass wrote, and drops the line when the scan ends', async () => {
        const drawn = headingsDrawn()
        const feed = serveFeed(hashing(3, 1))
        const host = await shownAt('/?library=1', manga(true))
        const alphaLink = host.querySelector('a[href="/series/1?library=1"]')
        expect(alphaLink).not.toBeNull()
        expect(lineOf(host)).toBe(
          'A scan is running: Hashing · 1.0 MiB of 5.0 GiB · 3 volumes ready. More series appear as they are read.',
        )
        vi.mocked(library.grid).mockResolvedValue({
          series: [alpha, beta],
          unfiled: [],
        })
        feed.answer(hashing(5, 2))
        await clock(1000)
        // The same card, so the view read its works again in place.
        expect(cards(host)).toEqual([
          '/series/1?library=1',
          '/series/2?library=1',
        ])
        expect(host.querySelector('a[href="/series/1?library=1"]')).toBe(
          alphaLink,
        )
        expect(lineOf(host)).toBe(
          'A scan is running: Hashing · 1.0 MiB of 5.0 GiB · 5 volumes ready. More series appear as they are read.',
        )
        feed.answer({ ended: 1, written: 2, libraries: [], jobs: [] })
        await clock(1000)
        expect(host.querySelector('.scan-line')).toBeNull()
        expect(cards(host)).toEqual([
          '/series/1?library=1',
          '/series/2?library=1',
        ])
        // No empty state was drawn on the way.
        expect(drawn).toEqual([])
      })

      it('the line holds through the commit that swaps Scanning for a scan’s first works', async () => {
        vi.mocked(library.grid).mockResolvedValue({ series: [], unfiled: [] })
        const feed = serveFeed(hashing(0, 1))
        const host = await shownAt('/?library=1', manga(false))
        const before = host.querySelector('.empty h2')?.textContent
        // Its words, not only a line: a room drawn at the swap would mark an
        // answer polled a moment ago.
        const frames = linesDrawn(host)
        vi.mocked(library.grid).mockResolvedValue({
          series: [alpha],
          unfiled: [],
        })
        feed.answer(hashing(1, 2))
        await clock(1000)
        expect(before).toBe('Scanning')
        expect(cards(host)).toEqual(['/series/1?library=1'])
        expect(runsOf(frames)).toEqual(['words'])
      })

      it('a return after a scan ended while away keeps the line’s room until the end is published, then closes it (O451)', async () => {
        const { host, atReturn, frames } = await leaveAndReturn(
          2 * SCAN_POLL_MS,
          { ended: 1, libraries: [] },
        )
        expect(atReturn, 'before the answer lands').toEqual(['room'])
        expect(runsOf(frames)).toEqual(['room', 'none'])
        expect(cards(host)).toEqual(['/series/1?library=1'])
      })

      it('a return while the scan still runs keeps the line’s room, and its words fill it (O451)', async () => {
        const { host, atReturn, frames } = await leaveAndReturn(
          2 * SCAN_POLL_MS,
          readerHashing,
        )
        expect(atReturn, 'before the answer lands').toEqual(['room'])
        expect(runsOf(frames)).toEqual(['room', 'words'])
        expect(cards(host)).toEqual(['/series/1?library=1'])
      })

      it('a return inside the bound draws the line’s words throughout (O451)', async () => {
        const { host, atReturn, frames } = await leaveAndReturn(
          SCAN_POLL_MS / 2,
          readerHashing,
        )
        expect(atReturn, 'before the answer lands').toEqual(['words'])
        expect(runsOf(frames)).toEqual(['words'])
        expect(cards(host)).toEqual(['/series/1?library=1'])
      })

      it('the scan line shows only for a library the view shows, and a reader sees the step alone', async () => {
        const feed = serveFeed(adminFeed(2))
        const own = await shownAt('/?library=1', twoLibraries)
        expect(cards(own)).toEqual(['/series/1?library=1'])
        expect(lineOf(own)).toBeUndefined()
        const every = await shownAt('/', twoLibraries)
        expect(lineOf(every)).toBe(
          'A scan is running: Reading files · 120 of 2,097 · directory 1 of 1. More series appear as they are read.',
        )
        feed.answer({ ended: 0, libraries: [{ id: 1, step: 'hashing' }] })
        await clock(1000)
        expect(lineOf(own)).toBe(
          'A scan is running: Hashing. More series appear as they are read.',
        )
        expect(lineOf(own)).not.toMatch(/\d/)
      })

      it('a search drawing hits says a scan is running too', async () => {
        vi.mocked(library.search).mockResolvedValue({
          series: [beta],
          unfiled: [],
          query: 'Beta',
        })
        serveFeed(adminFeed())
        const host = await shownAt('/?library=1&q=Beta', manga(true))
        const hits = host.querySelector('.search-hits')
        expect(cards(hits)).toEqual(['/series/2?library=1'])
        expect(lineOf(hits)).toBe(
          'A scan is running: Reading files · 120 of 2,097 · directory 1 of 1. More series appear as they are read.',
        )
      })

      it('the scan line is no live region', async () => {
        serveFeed(adminFeed())
        const host = await shownAt('/?library=1', manga(true))
        const line = host.querySelector('.scan-line')
        expect(line).not.toBeNull()
        // An administrator's counts change at every poll: a live region
        // would have a screen reader read each one.
        expect(
          line?.closest(
            '[aria-live], [role="status"], [role="alert"], [role="log"]',
          ),
        ).toBeNull()
      })
    })

    /**
     * Stubs the status, answered with what `answer` last set, and counts its
     * reads. Stubbed before the feed, which hands on what it does not answer.
     */
    const serveStatus = (first: LibraryStatus) => {
      let now = first
      let reads = 0
      vi.stubGlobal(
        'fetch',
        vi.fn((input: RequestInfo | URL) => {
          const url = String(input)
          if (url !== '/api/library/status')
            return Promise.reject(new Error(`unexpected ${url}`))
          reads += 1
          return Promise.resolve(
            new Response(JSON.stringify(now), {
              headers: { 'content-type': 'application/json' },
            }),
          )
        }),
      )
      return {
        /** Answers every read of the status from now on with `next`. */
        answer(next: LibraryStatus) {
          now = next
        },
        /** How many reads of the status have been sent. */
        reads: () => reads,
      }
    }

    /**
     * The page at `path` under the status read through the store, as the
     * shell reads it, once the status, the view and the feed's first answer
     * have landed.
     */
    const shownFromStore = async (path: string) => {
      let host!: HTMLElement
      await act(async () => {
        host = mount(
          <MemoryRouter initialEntries={[path]}>
            <StatusFromStore>
              <Library />
            </StatusFromStore>
          </MemoryRouter>,
        )
      })
      await clock(0)
      return host
    }

    it.each([
      {
        path: '/',
        restricted: false,
        before: 'No library yet',
        scanning: adminFeed(7),
        step: 'Reading files · 120 of 2,097 · directory 1 of 1',
      },
      {
        path: '/?library=7',
        restricted: false,
        before: 'No such library',
        scanning: adminFeed(7),
        step: 'Reading files · 120 of 2,097 · directory 1 of 1',
      },
      {
        // A reader's feed: the step alone, and no job.
        path: '/',
        restricted: true,
        before: 'No library shared',
        scanning: {
          ended: 0,
          libraries: [{ id: 7, step: 'hashing' }],
        } satisfies ScanFeed,
        step: 'Hashing',
      },
    ])(
      'the view at $path, saying $before before library 7 was made, says Scanning within one poll of the feed listing it (O322)',
      async ({ path, restricted, before, scanning, step }) => {
        const status = serveStatus({ libraries: [], restricted })
        const feed = serveFeed({ ended: 0, libraries: [] })
        const host = await shownFromStore(path)
        expect(said(host)[0]).toBe(before)

        status.answer({
          libraries: [{ id: 7, name: 'Manga', scanned: false }],
          restricted,
        })
        feed.answer(scanning)
        // The pinned idle poll, not the constant, so that a slower poll fails.
        await clock(5000)
        expect(said(host)).toEqual(['Scanning', step])
        expect(status.reads()).toBe(2)
      },
    )

    it('the status is read again once for each library the feed lists on every poll and the status does not, and never for one it lists', async () => {
      const status = serveStatus(manga(false))
      const feed = serveFeed(adminFeed(1))
      await shownFromStore('/')
      await clock(2 * SCAN_POLL_MS)
      expect(status.reads()).toBe(1)

      feed.answer({
        ended: 0,
        libraries: [
          { id: 1, step: 'resolving' },
          { id: 9, step: 'resolving' },
        ],
        jobs: [
          feedJob({
            libraries: [
              { id: 1, name: 'Manga' },
              { id: 9, name: 'Comics' },
            ],
          }),
        ],
      })
      const polled = feed.polls()
      await clock(6 * SCAN_POLL_MS)
      expect(feed.polls() - polled).toBeGreaterThanOrEqual(6)
      expect(status.reads()).toBe(2)

      // Library 11 is made after library 9 had its read: it has one of its
      // own, which the status then lists, and 9 still has no second.
      status.answer({
        libraries: [
          { id: 1, name: 'Manga', scanned: false },
          { id: 11, name: 'Magazines', scanned: false },
        ],
        restricted: false,
      })
      feed.answer({
        ended: 0,
        libraries: [
          { id: 1, step: 'resolving' },
          { id: 9, step: 'resolving' },
          { id: 11, step: 'resolving' },
        ],
        jobs: [
          feedJob({
            libraries: [
              { id: 1, name: 'Manga' },
              { id: 9, name: 'Comics' },
              { id: 11, name: 'Magazines' },
            ],
          }),
        ],
      })
      await clock(3 * SCAN_POLL_MS)
      expect(status.reads()).toBe(3)
    })
  })

  it('asks the server for the library in the URL, for the grid and for home', async () => {
    await showAt('/?library=4', {
      libraries: [{ id: 4, name: 'Manga', scanned: true }],
      restricted: false,
    })
    expect(vi.mocked(library.grid)).toHaveBeenLastCalledWith(4)
    expect(vi.mocked(library.home)).toHaveBeenLastCalledWith(4)
  })

  it('titles a library page with the library', async () => {
    document.title = 'stale'
    const host = await showAt('/?library=4', {
      libraries: [{ id: 4, name: 'Manga', scanned: true }],
      restricted: false,
    })
    expect(document.title).toContain('Manga')
    expect(host.querySelector('h1')?.textContent).toBe('Manga')
  })

  it('a failed load followed by a successful retry clears the stale failure banner', async () => {
    // Only `grid` is made to fail, which is enough here: the view says the
    // failure of either read, and home's alone is pinned in "says the failure
    // of home alone", below. `mockRejectedValueOnce` overrides the
    // `beforeEach` default for exactly one call; the retry below falls
    // through to that default (`{ series: [], unfiled: [] }`), which is
    // enough to prove the *second* load succeeded — the failing banner's own
    // text is the pin.
    vi.mocked(library.grid).mockRejectedValueOnce(new Error('offline'))

    // The view reads again when it is asked for another library, and nothing
    // inside `Library` changes the library here, so a sibling link inside the
    // same `MemoryRouter`, changing only the `?library=` query parameter
    // `Library` reads via `useSearchParams`, stands in for a second read of
    // another library.
    let host!: HTMLElement
    await act(async () => {
      host = mount(
        <MemoryRouter initialEntries={['/?library=1']}>
          <StatusContext.Provider value={twoLibraries}>
            <div>
              <Link to="/?library=2">retry</Link>
              <Library />
            </div>
          </StatusContext.Provider>
        </MemoryRouter>,
      )
    })
    await act(async () => {
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(host.querySelector('[role="alert"]')?.textContent).toBe('offline')

    await act(async () => {
      host.querySelector<HTMLAnchorElement>('a[href="/?library=2"]')?.click()
    })
    await act(async () => {
      await Promise.resolve()
      await Promise.resolve()
    })
    // The data from the successful retry is on screen and the stale banner
    // is gone — not merely replaced by a new one, which `.toBeNull()` alone
    // would not distinguish from "still broken, differently".
    expect(host.querySelector('[role="alert"]')).toBeNull()
  })

  it('names the three filters as one group, and leaves the sort outside it — O156', async () => {
    const host = await show()
    const group = host.querySelector('[role="group"][aria-label="Show"]')
    if (!group) throw new Error('no Show group')
    expect(
      [...group.querySelectorAll('button')].map((b) => [
        b.textContent,
        b.getAttribute('aria-pressed'),
      ]),
    ).toEqual([
      ['All', 'true'],
      ['Multi-edition', 'false'],
      ['Unread', 'false'],
    ])
    const added = [...host.querySelectorAll('button')].find(
      (b) => b.textContent === 'Added',
    )
    expect(added?.closest('[role="group"]')).toBeNull()
    expect(added?.getAttribute('aria-pressed')).toBe('false')
  })

  /**
   * **What is drawn when the library changes under a load.**
   *
   * Switching library while the first library's reads are in flight leaves
   * them outstanding, and the server need not answer in order. The store
   * keeps each answer under its own library's address, and the view draws
   * only the addresses it names now, so the *departed* library's answer,
   * landing late, never takes the place of the one on screen. While the next
   * library's own reads are in flight, and after one fails, the page draws
   * only what was read for that library: nothing of the one it left.
   *
   * The tests hold a read open with `deferred` rather than ordering two
   * `mockResolvedValueOnce`s: mocked promises resolve in call order, so no
   * answer would land after its library was left.
   */
  describe('a load for a library that has since been left', () => {
    /** Library 1's page, with a link to library 2's beside it. */
    const mountAtOne = async () => {
      let host!: HTMLElement
      await act(async () => {
        host = mount(
          <MemoryRouter initialEntries={['/?library=1']}>
            <StatusContext.Provider value={twoLibraries}>
              <div>
                <Link to="/?library=2">the other library</Link>
                <Library />
              </div>
            </StatusContext.Provider>
          </MemoryRouter>,
        )
      })
      await flush()
      return host
    }

    const followTheLink = (host: HTMLElement) =>
      act(async () => {
        host.querySelector<HTMLAnchorElement>('a[href="/?library=2"]')?.click()
      })

    const switchLibraries = async (first: Promise<library.Grid>) => {
      const second = deferred<library.Grid>()
      vi.mocked(library.grid)
        .mockReturnValueOnce(first)
        .mockReturnValueOnce(second.promise)
      const host = await mountAtOne()
      await followTheLink(host)
      await flush(() =>
        second.resolve({
          series: [series({ id: 2, title: 'Two' })],
          unfiled: [],
        }),
      )
      expect(host.querySelector('ul.grid .title')?.textContent).toBe('Two')
      return host
    }

    it('is not drawn when it answers after the library was changed', async () => {
      const first = deferred<library.Grid>()
      const host = await switchLibraries(first.promise)
      await flush(() =>
        first.resolve({
          series: [series({ id: 1, title: 'One' })],
          unfiled: [],
        }),
      )
      expect(host.textContent).not.toContain('One')
      expect(host.querySelector('ul.grid .title')?.textContent).toBe('Two')
    })

    it('does not put its failure over the library that loaded', async () => {
      const first = deferred<library.Grid>()
      const host = await switchLibraries(first.promise)
      await flush(() => first.reject(new Error('library 1 is no longer there')))
      expect(host.querySelector('[role="alert"]')).toBeNull()
      expect(host.querySelector('ul.grid .title')?.textContent).toBe('Two')
    })

    /**
     * Library 1 drawn, a series and a continue card, then library 2's page
     * with its grid held open.
     */
    const leaveOne = async () => {
      const second = deferred<library.Grid>()
      vi.mocked(library.grid)
        .mockResolvedValueOnce({
          series: [series({ id: 1, title: 'One' })],
          unfiled: [],
        })
        .mockReturnValueOnce(second.promise)
      vi.mocked(library.home).mockResolvedValueOnce({
        continuing: [work({ slug: 'one-v01', title: 'One v01' })],
        added: [],
        read: [],
      })
      const host = await mountAtOne()
      expect(host.querySelector('ul.grid .title')?.textContent).toBe('One')
      expect(
        host.querySelector('[data-testid="continue"] .title')?.textContent,
      ).toBe('One v01')
      await followTheLink(host)
      await flush()
      expect(host.querySelector('h1')?.textContent).toBe('Comics')
      return { host, second }
    }

    it('draws nothing of the library it left while the next one loads', async () => {
      const { host } = await leaveOne()
      expect(host.querySelector('ul.grid .title')).toBeNull()
      expect(host.querySelector('[data-testid="continue"]')).toBeNull()
    })

    it('draws nothing of the library it left when the next one fails to load', async () => {
      const { host, second } = await leaveOne()
      await flush(() => second.reject(new Error('/api/library/series: 500')))
      expect(host.querySelector('[role="alert"]')?.textContent).toBe(
        '/api/library/series: 500',
      )
      expect(host.querySelector('ul.grid .title')).toBeNull()
      expect(host.querySelector('[data-testid="continue"]')).toBeNull()
    })
  })

  /**
   * **The grid and home are read through the store** (spec
   * `2026-10-28-loading-library-design.md` §6.1): a first visit waits for both,
   * a return draws what was kept and reads it again.
   */
  describe('the view, read through the store', () => {
    /**
     * The view at `path`, with a link to each of the two libraries, and
     * `after` drawn after it.
     */
    const screenAt = (path: string, after?: React.ReactNode) => (
      <MemoryRouter initialEntries={[path]}>
        <StatusContext.Provider value={twoLibraries}>
          <div>
            <Link to="/?library=1">one</Link>
            <Link to="/?library=2">two</Link>
            <Library />
            {after}
          </div>
        </StatusContext.Provider>
      </MemoryRouter>
    )

    /**
     * What the view draws below its heading row: the wait line by its words,
     * each Continue card and each grid tile by its title and its cover's
     * `src`, `null` where it draws none.
     */
    const drawn = (host: HTMLElement) =>
      JSON.stringify({
        waiting: [...host.querySelectorAll('[role="status"]')].map(
          (line) => line.textContent,
        ),
        continuing: [
          ...host.querySelectorAll('[data-testid="continue"] li'),
        ].map((card) => [
          card.querySelector('.title')?.textContent,
          card.querySelector('img')?.getAttribute('src') ?? null,
        ]),
        grid: [...host.querySelectorAll('ul.grid li')].map((tile) => [
          tile.querySelector('.title')?.textContent,
          tile.querySelector('img')?.getAttribute('src') ?? null,
        ]),
      })

    const nothing = JSON.stringify({ waiting: [], continuing: [], grid: [] })
    const waiting = JSON.stringify({
      waiting: ['Loading…'],
      continuing: [],
      grid: [],
    })
    const oneDrawn = JSON.stringify({
      waiting: [],
      continuing: [
        ['One v01', '/api/works/one-v01/raw/pages/0/thumb?v=c0ffee-0-1&w=256'],
      ],
      grid: [
        ['One', '/api/works/one-v01/raw/pages/0/thumb?v=c0ffee-0-1&w=256'],
        ['Two', null],
      ],
    })
    const oneGrid: library.Grid = {
      series: [
        series({ id: 1, title: 'One', cover: coverOf('one-v01', 'raw') }),
        series({ id: 2, title: 'Two', cover: null }),
      ],
      unfiled: [],
    }
    const oneHome: library.HomeLists = {
      continuing: [
        work({
          slug: 'one-v01',
          title: 'One v01',
          editions: [
            {
              key: 'raw',
              label: 'Raw',
              members: 1,
              pages: 180,
              missing: 0,
              cover: coverOf('one-v01', 'raw'),
            },
          ],
        }),
      ],
      added: [],
      read: [],
    }

    it('draws Loading… in the grid’s place from 200 ms on a first visit, and the Continue strip with the grid in one step once both have landed', async () => {
      holdTheClock()
      const grid = deferred<library.Grid>()
      const home = deferred<library.HomeLists>()
      vi.mocked(library.grid).mockReturnValueOnce(grid.promise)
      vi.mocked(library.home).mockReturnValueOnce(home.promise)
      let host!: HTMLElement
      await act(async () => {
        host = mount(screenAt('/'))
      })
      await flush()
      expect(drawn(host)).toBe(nothing)
      pass(WAIT_DRAWN_AFTER_MS - 1)
      expect(drawn(host)).toBe(nothing)
      pass(1)
      expect(drawn(host)).toBe(waiting)
      expect(
        host.querySelector('.library-chips + [role="status"]')?.textContent,
      ).toBe('Loading…')

      const seen = watchDrawn(() => drawn(host))
      await flush(() => grid.resolve(oneGrid))
      await flush()
      await flush(() => home.resolve(oneHome))
      await flush()
      seen.stop()
      expect([...seen.seen]).toEqual([waiting, oneDrawn])
    })

    it('draws the kept grid and Continue strip in its first render on a return, each tile with the cover it had, and reads each again once', async () => {
      vi.mocked(library.grid).mockResolvedValue(oneGrid)
      vi.mocked(library.home).mockResolvedValue(oneHome)
      let host!: HTMLElement
      await act(async () => {
        host = mount(screenAt('/'))
      })
      await flush()
      await flush()
      expect(drawn(host)).toBe(oneDrawn)

      rerender(<div />)
      vi.mocked(library.grid).mockClear()
      vi.mocked(library.home).mockClear()
      const grid = deferred<library.Grid>()
      const home = deferred<library.HomeLists>()
      vi.mocked(library.grid).mockReturnValueOnce(grid.promise)
      vi.mocked(library.home).mockReturnValueOnce(home.promise)
      const first: { drawn?: string } = {}
      rerender(
        screenAt('/', <FirstCommit read={() => drawn(host)} into={first} />),
      )
      expect(first.drawn).toBe(oneDrawn)
      expect(drawn(host)).toBe(oneDrawn)
      await flush()
      await flush()
      expect(drawn(host)).toBe(oneDrawn)
      expect(vi.mocked(library.grid).mock.calls).toEqual([[undefined]])
      expect(vi.mocked(library.home).mock.calls).toEqual([[undefined]])
    })

    it('draws a library kept in this tab at once on a switch back to it, and reads it again', async () => {
      vi.mocked(library.grid).mockImplementation(async (id) =>
        id === 1
          ? oneGrid
          : { series: [series({ id: 3, title: 'Three' })], unfiled: [] },
      )
      vi.mocked(library.home).mockImplementation(async (id) =>
        id === 1 ? oneHome : { continuing: [], added: [], read: [] },
      )
      let host!: HTMLElement
      await act(async () => {
        host = mount(screenAt('/?library=1'))
      })
      await flush()
      await flush()
      expect(drawn(host)).toBe(oneDrawn)
      const follow = (to: string) =>
        act(async () => {
          host.querySelector<HTMLAnchorElement>(`a[href="${to}"]`)?.click()
        })
      await follow('/?library=2')
      await flush()
      await flush()
      expect(host.querySelector('h1')?.textContent).toBe('Comics')
      expect(
        [...host.querySelectorAll('ul.grid .title')].map((t) => t.textContent),
      ).toEqual(['Three'])

      vi.mocked(library.grid).mockClear()
      vi.mocked(library.home).mockClear()
      vi.mocked(library.grid).mockReturnValueOnce(
        deferred<library.Grid>().promise,
      )
      vi.mocked(library.home).mockReturnValueOnce(
        deferred<library.HomeLists>().promise,
      )
      await follow('/?library=1')
      expect(host.querySelector('h1')?.textContent).toBe('Manga')
      expect(drawn(host)).toBe(oneDrawn)
      await flush()
      await flush()
      expect(drawn(host)).toBe(oneDrawn)
      expect(vi.mocked(library.grid).mock.calls).toEqual([[1]])
      expect(vi.mocked(library.home).mock.calls).toEqual([[1]])
    })

    /** Mount the view at `/` and let both reads land. */
    const landed = async () => {
      let host!: HTMLElement
      await act(async () => {
        host = mount(screenAt('/'))
      })
      await flush()
      await flush()
      expect(drawn(host)).toBe(oneDrawn)
      return host
    }

    /** Each alert the view draws, by its words. */
    const alerts = (host: HTMLElement) =>
      [...host.querySelectorAll('[role="alert"]')].map((a) => a.textContent)

    it('draws neither the grid nor the Continue strip on an arrival at answers marked stale, until they are read again', async () => {
      vi.mocked(library.grid).mockResolvedValue(oneGrid)
      vi.mocked(library.home).mockResolvedValue(oneHome)
      const host = await landed()
      rerender(<div />)
      invalidateLibraryViews()
      const grid = deferred<library.Grid>()
      const home = deferred<library.HomeLists>()
      vi.mocked(library.grid).mockReturnValueOnce(grid.promise)
      vi.mocked(library.home).mockReturnValueOnce(home.promise)
      rerender(screenAt('/'))
      await flush()
      expect(drawn(host)).toBe(nothing)
      expect(host.querySelector('[data-testid="continue"]')).toBeNull()
      expect(host.querySelector('ul.grid')).toBeNull()

      // The same answers as those kept: what is pinned is the re-read
      // clearing the mark, not a change of rows.
      await flush(() => grid.resolve(oneGrid))
      await flush(() => home.resolve(oneHome))
      await flush()
      expect(drawn(host)).toBe(oneDrawn)
    })

    it('says the failure of home alone, and draws the grid that was read', async () => {
      vi.mocked(library.grid).mockResolvedValue(oneGrid)
      vi.mocked(library.home).mockRejectedValue(new Error('home: 500'))
      let host!: HTMLElement
      await act(async () => {
        host = mount(screenAt('/'))
      })
      await flush()
      await flush()
      expect(alerts(host)).toEqual(['home: 500'])
      expect(drawn(host)).toBe(
        JSON.stringify({
          ...JSON.parse(oneDrawn),
          continuing: [],
        }),
      )
    })

    it('says the grid’s failure where both reads fail', async () => {
      vi.mocked(library.grid).mockRejectedValue(new Error('grid: 500'))
      vi.mocked(library.home).mockRejectedValue(new Error('home: 500'))
      let host!: HTMLElement
      await act(async () => {
        host = mount(screenAt('/'))
      })
      await flush()
      await flush()
      expect(alerts(host)).toEqual(['grid: 500'])
    })

    it('keeps the rows it drew beside the alert when a re-read fails', async () => {
      vi.mocked(library.grid).mockResolvedValue(oneGrid)
      vi.mocked(library.home).mockResolvedValue(oneHome)
      const host = await landed()
      expect(alerts(host)).toEqual([])
      rerender(<div />)
      vi.mocked(library.grid).mockRejectedValueOnce(new Error('grid: 500'))
      rerender(screenAt('/'))
      await flush()
      await flush()
      expect(alerts(host)).toEqual(['grid: 500'])
      expect(drawn(host)).toBe(oneDrawn)
    })
  })

  /**
   * **The chip and the sort are in the address** (spec
   * `2026-10-28-loading-library-design.md` §5.1): `?show=` and `?sort=`, the
   * defaults left out, each press written with `replace`.
   */
  describe('the chip and the sort, in the address', () => {
    /** The address, its navigation state and how it was arrived at. */
    function Where() {
      const location = useLocation()
      const type = useNavigationType()
      const navigate = useNavigate()
      return (
        <>
          <output data-testid="where">
            {JSON.stringify({
              at: location.pathname + location.search,
              state: location.state,
              type,
            })}
          </output>
          <button
            type="button"
            data-testid="back"
            onClick={() => {
              void navigate(-1)
            }}
          >
            back
          </button>
        </>
      )
    }

    const where = (host: HTMLElement) =>
      JSON.parse(
        host.querySelector('[data-testid="where"]')?.textContent ?? 'null',
      ) as { at: string; state: unknown; type: string }

    /** The view at the second of `entries`, the first standing before it. */
    const mountAt = async (entries: MemoryRouterProps['initialEntries']) => {
      let host!: HTMLElement
      await act(async () => {
        host = mount(
          <MemoryRouter initialEntries={entries} initialIndex={1}>
            <StatusContext.Provider value={defaultStatus}>
              <Library />
              <Where />
            </StatusContext.Provider>
          </MemoryRouter>,
        )
      })
      await flush()
      await flush()
      return host
    }

    /** Each chip and the sort, by its words and whether it is pressed. */
    const pressed = (host: HTMLElement) =>
      [...host.querySelectorAll('.library-chips button')].map((b) => [
        b.textContent,
        b.getAttribute('aria-pressed'),
      ])

    const press = (host: HTMLElement, words: string) =>
      act(async () => {
        ;[...host.querySelectorAll<HTMLButtonElement>('.library-chips button')]
          .find((b) => b.textContent === words)
          ?.click()
      })

    const titles = (host: HTMLElement) =>
      [...host.querySelectorAll('ul.grid .title')].map((t) => t.textContent)

    beforeEach(() => {
      vi.mocked(library.grid).mockResolvedValue({
        series: [
          series({ id: 1, title: 'Alpha', added: 1, works: 3, unread: 3 }),
          series({ id: 2, title: 'Beta', added: 9, works: 3, unread: 3 }),
          series({ id: 3, title: 'Gamma', added: 5, works: 3, unread: 1 }),
        ],
        unfiled: [],
      })
    })

    it('draws the chip and the sort ?show= and ?sort= name on arrival', async () => {
      const host = await mountAt(['/before', '/?show=unread&sort=added'])
      expect(pressed(host)).toEqual([
        ['All', 'false'],
        ['Multi-edition', 'false'],
        ['Unread', 'true'],
        ['Added', 'true'],
      ])
      expect(titles(host)).toEqual(['Beta', 'Alpha'])
    })

    it('reads a ?show= or ?sort= that names neither as All and Title', async () => {
      const host = await mountAt(['/before', '/?show=bogus&sort=bogus'])
      expect(pressed(host)).toEqual([
        ['All', 'true'],
        ['Multi-edition', 'false'],
        ['Unread', 'false'],
        ['Added', 'false'],
      ])
      expect(titles(host)).toEqual(['Alpha', 'Beta', 'Gamma'])
    })

    it('writes each press with replace, keeping every other parameter and the search box’s tag', async () => {
      const host = await mountAt([
        '/before',
        {
          pathname: '/',
          search: '?library=1&q=',
          state: { searchBox: 'elsewhere:1' },
        },
      ])
      await press(host, 'Unread')
      expect(where(host)).toEqual({
        at: '/?library=1&q=&show=unread',
        state: { searchBox: 'elsewhere:1' },
        type: 'REPLACE',
      })
      await press(host, 'Added')
      expect(where(host)).toEqual({
        at: '/?library=1&q=&show=unread&sort=added',
        state: { searchBox: 'elsewhere:1' },
        type: 'REPLACE',
      })
      expect(pressed(host)).toEqual([
        ['All', 'false'],
        ['Multi-edition', 'false'],
        ['Unread', 'true'],
        ['Added', 'true'],
      ])
      expect(titles(host)).toEqual(['Beta', 'Alpha'])
      await press(host, 'All')
      await press(host, 'Added')
      expect(where(host).at).toBe('/?library=1&q=')
      expect(pressed(host)).toEqual([
        ['All', 'true'],
        ['Multi-edition', 'false'],
        ['Unread', 'false'],
        ['Added', 'false'],
      ])

      await act(async () => {
        host.querySelector<HTMLButtonElement>('[data-testid="back"]')?.click()
      })
      expect(where(host).at).toBe('/before')
    })

    it('carries no focusSearch over a press, so the press does not focus the search box', async () => {
      const host = await mountAt([
        '/before',
        { pathname: '/', search: '', state: { focusSearch: true } },
      ])
      expect(document.activeElement?.id).not.toBe(SEARCH_BOX_ID)
      await press(host, 'Multi-edition')
      expect(where(host)).toEqual({
        at: '/?show=multi',
        state: null,
        type: 'REPLACE',
      })
      expect(document.activeElement?.id).not.toBe(SEARCH_BOX_ID)
    })
  })

  /**
   * **One reading of `?library=` for the page and the sidebar beside it**:
   * `library.ts`'s `libraryParam`, which Search reads the value through too.
   * A value that is no library id is no library's page, so the page says so
   * and asks the server nothing, and the sidebar marks neither a library nor
   * the Library.
   */
  describe('a ?library= value, read alike by the page and the sidebar', () => {
    beforeEach(() => {
      vi.mocked(library.grid).mockClear()
      vi.mocked(library.home).mockClear()
    })

    const at = async (path: string) => {
      let host!: HTMLElement
      await act(async () => {
        host = mount(
          <MemoryRouter initialEntries={[path]}>
            <StatusContext.Provider value={twoLibraries}>
              <div>
                <Sidebar libraries={twoLibraries.libraries} />
                <Library />
              </div>
            </StatusContext.Provider>
          </MemoryRouter>,
        )
      })
      await flush()
      return host
    }

    const marked = (host: HTMLElement) =>
      [...host.querySelectorAll('.sidebar [aria-current="page"]')].map((a) =>
        a.getAttribute('href'),
      )

    for (const raw of ['abc', '', '0', '01', '-1']) {
      it(`is no library for ?library=${raw === '' ? ' (empty)' : raw}: the page says so and asks nothing, and the sidebar marks nothing`, async () => {
        const host = await at(`/?library=${raw}`)
        expect(host.querySelector('section.empty h2')?.textContent).toBe(
          'No such library',
        )
        expect(host.querySelector('h1')?.textContent).toBe('Library')
        expect(vi.mocked(library.grid)).not.toHaveBeenCalled()
        expect(vi.mocked(library.home)).not.toHaveBeenCalled()
        expect(marked(host)).toEqual([])
      })
    }

    it('is library 1 for ?library=1, on the page and in the sidebar', async () => {
      const host = await at('/?library=1')
      expect(host.querySelector('h1')?.textContent).toBe('Manga')
      expect(vi.mocked(library.grid)).toHaveBeenLastCalledWith(1)
      expect(vi.mocked(library.home)).toHaveBeenLastCalledWith(1)
      expect(marked(host)).toEqual(['/?library=1'])
    })
  })

  /**
   * Spec C1 §8's third empty state: "A series with no cover — a generated
   * placeholder carrying the title, not a broken image."
   */
  describe('a cover that will not load', () => {
    const withCover = async () => {
      vi.mocked(library.grid).mockResolvedValue({
        series: [series({ id: 3, title: 'Gotoubun no Hanayome' })],
        unfiled: [],
      })
      return await show()
    }

    it('is replaced by a placeholder carrying the series title', async () => {
      const host = await withCover()
      const img = host.querySelector('img')
      expect(img, 'a cover to begin with').not.toBeNull()

      // jsdom loads no images at all, so the failure has to be delivered the
      // way the browser would deliver it.
      await act(async () => {
        img?.dispatchEvent(new Event('error'))
        await Promise.resolve()
      })

      expect(host.querySelector('img'), 'the broken image has gone').toBeNull()
      const placeholder = host.querySelector('.cover-missing')
      expect(placeholder?.textContent).toBe('Gotoubun no Hanayome')
      // Hidden from a screen reader: the `<a>` already carries the same title
      // as text, and this stands in for artwork rather than adding a fact.
      expect(placeholder?.getAttribute('aria-hidden')).toBe('true')
      // And the tile is still a link to the series — a failed cover loses the
      // picture, not the navigation.
      expect(host.querySelector('a[href="/series/3"]')).not.toBeNull()
    })

    it('leaves a cover that loads alone', async () => {
      const host = await withCover()
      expect(host.querySelector('img')?.getAttribute('src')).toBe(
        '/api/works/a-series-v01/raw/pages/0/thumb?v=c0ffee-0-1&w=256',
      )
      // O133: the browser picks the size for the cover's drawn width.
      expect(host.querySelector('img')?.getAttribute('srcset')).toBe(
        '/api/works/a-series-v01/raw/pages/0/thumb?v=c0ffee-0-1&w=256 171w, /api/works/a-series-v01/raw/pages/0/thumb?v=c0ffee-0-1&w=512 341w, /api/works/a-series-v01/raw/pages/0/thumb?v=c0ffee-0-1&w=768 512w',
      )
      expect(host.querySelector('.cover-missing')).toBeNull()
    })

    // The size off the scale is drawn at the nearest position's, so its
    // cover is asked for at that.
    const filledAt = (width: number) =>
      PHONE_LAYOUT_PARTS.map((part) => `${part} ${width}px`).join(', ')
    it.each([
      [240, `${filledAt(512)}, 240px`],
      [151, `${filledAt(336)}, 152px`],
    ])(
      'with a %i px cover size in the preferences, asks for its image with sizes %s',
      async (stored, sizes) => {
        vi.mocked(library.grid).mockResolvedValue({
          series: [series({ id: 3, title: 'Gotoubun no Hanayome' })],
          unfiled: [],
        })
        let host!: HTMLElement
        await act(async () => {
          host = mount(
            <MemoryRouter>
              <StatusContext.Provider value={defaultStatus}>
                <BrowsePrefsContext
                  value={{ ...browseModule.defaults, coverSize: stored }}
                >
                  <Library />
                </BrowsePrefsContext>
              </StatusContext.Provider>
            </MemoryRouter>,
          )
        })
        await flush()
        expect(host.querySelector('img')?.getAttribute('sizes')).toBe(sizes)
      },
    )
  })

  /**
   * The server sends `cover: null` for a series none of whose editions this
   * caller may see, or whose cover edition has no page: there is no address
   * to ask for, so the tile is the placeholder from its first render.
   */
  it('draws a series with no cover as the placeholder, never as an image', async () => {
    vi.mocked(library.grid).mockResolvedValue({
      series: [
        series({ id: 1, title: 'Alpha' }),
        series({ id: 2, title: 'Beta', cover: null }),
      ],
      unfiled: [],
    })
    // The `src` of every `img` the page held at any moment: an image drawn
    // and then swapped for the placeholder would already have asked for its
    // address. Removed nodes as well as added ones, because a record is read
    // after the fact, when an added subtree may have lost the image again.
    const held = new Set<string | null>()
    const record = (records: MutationRecord[]) => {
      for (const { addedNodes, removedNodes } of records)
        for (const node of [...addedNodes, ...removedNodes])
          if (node instanceof Element)
            for (const img of [node, ...node.querySelectorAll('img')])
              if (img.tagName === 'IMG') held.add(img.getAttribute('src'))
    }
    const watch = new MutationObserver(record)
    watch.observe(document.body, { childList: true, subtree: true })
    const host = await show()
    record(watch.takeRecords())
    watch.disconnect()

    const tile = (id: number) => host.querySelector(`a[href="/series/${id}"]`)
    expect(tile(2)?.querySelector('img')).toBeNull()
    expect(tile(2)?.querySelector('.cover-missing')?.textContent).toBe('Beta')
    expect(tile(1)?.querySelector('img')?.getAttribute('src')).toBe(
      '/api/works/a-series-v01/raw/pages/0/thumb?v=c0ffee-0-1&w=256',
    )
    expect(tile(1)?.querySelector('.cover-missing')).toBeNull()
    expect([...held]).toEqual([
      '/api/works/a-series-v01/raw/pages/0/thumb?v=c0ffee-0-1&w=256',
    ])
  })

  it('the Added button reorders the grid, not merely toggles its own label', async () => {
    vi.mocked(library.grid).mockResolvedValue({
      series: [
        series({ id: 1, title: 'Alpha', added: 1 }),
        series({ id: 2, title: 'Beta', added: 9 }),
      ],
      unfiled: [],
    })
    const host = await show()
    const hrefs = () =>
      [...host.querySelectorAll('a[href^="/series/"]')].map((a) =>
        a.getAttribute('href'),
      )
    // Title order first — `sort` starts at `'title'`.
    expect(hrefs()).toEqual(['/series/1', '/series/2'])

    const added = [...host.querySelectorAll('button')].find(
      (b) => b.textContent === 'Added',
    )
    expect(added, 'an Added sort button').toBeDefined()
    await act(async () => {
      added?.click()
      await Promise.resolve()
    })
    // Highest `added` first, per `sorted()`'s own rule — the reverse of the
    // title order above, which is what proves the click reordered the grid
    // rather than only flipping `aria-pressed`.
    expect(added?.getAttribute('aria-pressed')).toBe('true')
    expect(hrefs()).toEqual(['/series/2', '/series/1'])
  })

  it('draws each continue card from the edition its reader will open', async () => {
    vi.mocked(library.home).mockResolvedValue({
      continuing: [
        work({
          slug: 'half-read-v02',
          opens: 'en',
          editions: [
            {
              key: 'raw',
              label: 'Raw',
              members: 1,
              pages: 180,
              missing: 0,
              cover: coverOf('half-read-v02', 'raw'),
            },
            {
              key: 'en',
              label: 'En',
              members: 1,
              pages: 178,
              missing: 0,
              cover: coverOf('half-read-v02', 'en'),
            },
          ],
        }),
      ],
      added: [],
      read: [],
    })
    const host = await show()
    const strip = host.querySelector('[data-testid="continue"]')
    expect(strip?.querySelector('img')?.getAttribute('src')).toBe(
      '/api/works/half-read-v02/en/pages/0/thumb?v=c0ffee-0-1&w=256',
    )
  })

  it('draws the missing-cover box on a continue card whose opening edition has no page, and no other edition’s cover', async () => {
    vi.mocked(library.home).mockResolvedValue({
      continuing: [
        work({
          slug: 'pageless-v02',
          title: 'Pageless v02',
          opens: 'en',
          editions: [
            {
              key: 'raw',
              label: 'Raw',
              members: 1,
              pages: 180,
              missing: 0,
              cover: coverOf('pageless-v02', 'raw'),
            },
            {
              key: 'en',
              label: 'En',
              members: 1,
              pages: 0,
              missing: 0,
              cover: null,
            },
          ],
        }),
        work({ slug: 'paged-v01', title: 'Paged v01' }),
      ],
      added: [],
      read: [],
    })
    const host = await show()
    const cards = [...host.querySelectorAll('[data-testid="continue"] li')]
    expect(
      cards.map((card) => [
        card.querySelector('.title')?.textContent,
        card.querySelector('img')?.getAttribute('src') ?? null,
        card.querySelector('.cover-missing')?.textContent ?? null,
      ]),
    ).toEqual([
      ['Pageless v02', null, 'Pageless v02'],
      [
        'Paged v01',
        '/api/works/paged-v01/raw/pages/0/thumb?v=c0ffee-0-1&w=256',
        null,
      ],
    ])
  })

  it('says which volume a continue card is, and nothing where it has no number', async () => {
    vi.mocked(library.home).mockResolvedValue({
      continuing: [
        work({ slug: 'numbered', title: 'Numbered v03', volume: 3 }),
        work({ slug: 'special', title: 'A Special', volume: null }),
      ],
      added: [],
      read: [],
    })
    const host = await show()
    const cards = [...host.querySelectorAll('[data-testid="continue"] li')]
    expect(cards.map((c) => c.querySelector('.meta')?.textContent)).toEqual([
      'Vol. 3',
      undefined,
    ])
  })

  it('keeps the cover on a continue card whose files are gone', async () => {
    vi.mocked(library.home).mockResolvedValue({
      continuing: [
        work({
          slug: 'gone-v01',
          editions: [
            {
              key: 'raw',
              label: 'Raw',
              members: 1,
              pages: 180,
              missing: 1,
              cover: coverOf('gone-v01', 'raw'),
            },
          ],
        }),
      ],
      added: [],
      read: [],
    })
    const host = await show()
    const strip = host.querySelector('[data-testid="continue"]')
    expect(strip?.querySelector('a')).toBeNull()
    expect(strip?.querySelector('.tile-unreadable img')).not.toBeNull()
  })

  it('says how many volumes each series holds', async () => {
    vi.mocked(library.grid).mockResolvedValue({
      series: [
        series({ id: 1, title: 'Many', works: 42 }),
        series({ id: 2, title: 'One', works: 1 }),
      ],
      unfiled: [],
    })
    const host = await show()
    expect(
      [...host.querySelectorAll('.grid .meta')].map((m) => m.textContent),
    ).toEqual(['42 volumes', '1 volume'])
  })

  it('opens with one h1, Library, for a screen reader alone — O163', async () => {
    const host = await show()
    const h1s = [...host.querySelectorAll('h1')]
    expect(h1s.map((h) => [h.textContent, h.className])).toEqual([
      ['Library', 'visually-hidden'],
    ])
  })

  it('names its tab Library — O173', async () => {
    document.title = 'stale'
    await show()
    expect(document.title).toBe('Library — taiyaku')
  })
})

describe('the library page’s View panel — spec §2', () => {
  beforeEach(() => {
    vi.mocked(library.home).mockResolvedValue({
      continuing: [],
      added: [],
      read: [],
    })
    vi.mocked(library.grid).mockResolvedValue({ series: [], unfiled: [] })
  })

  /** Library 1's page with a shell's controls, and a link to library 2's. */
  const mountWithView = async () => {
    let host!: HTMLElement
    await act(async () => {
      host = mount(
        <MemoryRouter initialEntries={['/?library=1']}>
          <StatusContext.Provider value={twoLibraries}>
            <BrowseControlsContext.Provider
              value={{ change: () => {}, reset: () => {} }}
            >
              <Link to="/?library=2">the other library</Link>
              <Library />
            </BrowseControlsContext.Provider>
          </StatusContext.Provider>
        </MemoryRouter>,
      )
    })
    await flush()
    return host
  }

  const viewButton = (host: HTMLElement) => {
    const button = host.querySelector<HTMLButtonElement>(
      '.page-head .view-button',
    )
    if (!button) throw new Error('no View button in the top row')
    return button
  }

  const openView = (host: HTMLElement) => {
    act(() => {
      viewButton(host).click()
    })
    const panel = document.getElementById(
      viewButton(host).getAttribute('aria-controls') ?? '',
    )
    if (!panel || panel.hidden) throw new Error('View opened nothing')
    return panel
  }

  it('draws the search box, then View, in the top row — spec §2', async () => {
    const host = await mountWithView()
    expect(
      [...(host.querySelector('.page-head')?.children ?? [])].map(
        (child) => child.className,
      ),
    ).toEqual(['visually-hidden', 'search-box', 'view-menu'])
  })

  it('holds Continue strip, Back from reading, Panel, Cover size and Reset, in sentence case', async () => {
    const panel = openView(await mountWithView())
    // `control`, as the slider's label names its range by `for`.
    expect(
      [...panel.querySelectorAll('label')].map((label) => {
        const control = label.control
        return [
          (label.querySelector('span') ?? label).textContent,
          control instanceof HTMLInputElement ? control.type : control?.tagName,
          [...label.querySelectorAll('option')].map((o) => o.textContent),
        ]
      }),
    ).toEqual([
      ['Continue strip', 'checkbox', []],
      ['Back from reading', 'SELECT', ['Series page', 'Where you came from']],
      ['Panel', 'SELECT', ['Off', 'Button', 'On-select']],
      ['Cover size', 'range', []],
    ])
    expect(
      [...panel.querySelectorAll('button')].map((b) => b.textContent),
    ).toEqual(['Reset'])
  })

  it('closes when the page navigates, the page staying', async () => {
    // `click()` sends no pointer press and moves no focus, so only the
    // navigation itself can close the panel here.
    const host = await mountWithView()
    const panel = openView(host)
    const button = viewButton(host)
    await act(async () => {
      host.querySelector<HTMLAnchorElement>('a[href="/?library=2"]')?.click()
    })
    // The same button: the page did not remount, which would have closed
    // the panel whatever the hook did.
    expect(viewButton(host)).toBe(button)
    expect(button.getAttribute('aria-expanded')).toBe('false')
    expect(panel.hidden).toBe(true)
  })
})

/**
 * The Continue row's `i` (O417): the series page's panel, drawn under the
 * row as a child of its section, for each readable card. Three cards,
 * readable, unreadable, readable, whose slugs are not their ids, so a read
 * or a download asked by id or by position names the wrong work.
 */
describe('the Continue row’s panel — O417', () => {
  const bride = work({
    id: 21,
    slug: 'bride-v02',
    title: 'Bride v02',
    volume: 2,
  })
  const lost = work({
    id: 22,
    slug: 'lost-v01',
    title: 'Lost v01',
    editions: [
      {
        key: 'raw',
        label: 'Raw',
        members: 1,
        pages: 180,
        missing: 1,
        cover: coverOf('lost-v01', 'raw'),
      },
    ],
  })
  const heron = work({
    id: 23,
    slug: 'heron-v05',
    title: 'Heron v05',
    volume: 5,
  })

  const who = (over: Partial<Me> = {}): Me => ({
    user_id: 1,
    admin: false,
    can_download: true,
    can_stream: true,
    can_stamp: false,
    ...over,
  })

  beforeEach(() => {
    vi.mocked(library.grid).mockResolvedValue({ series: [], unfiled: [] })
    vi.mocked(library.home).mockResolvedValue({
      continuing: [bride, lost, heron],
      added: [],
      read: [],
    })
    // Each work's verdict its own, so a panel drawn from another card's read
    // says so.
    vi.mocked(library.work).mockReset()
    vi.mocked(library.work).mockImplementation(async (slug) => ({
      ...work({ slug }),
      series: null,
      alignments: [
        {
          from: 'raw',
          to: 'en',
          verdict: slug === 'heron-v05' ? 'confident' : 'partial',
          slots: 180,
          paired: 178,
        },
      ],
    }))
    vi.mocked(metadata.patchWork).mockReset()
  })

  /**
   * The home page with `panel` set, under a router that draws the reader's
   * address as a page of its own, so a link followed leaves the row. In
   * `.library > main` as the shell draws it, where focus goes when a press
   * in the panel takes its card away. Drawn again with another `panel`, the
   * same page sees the preference change, as View's control changes it.
   */
  const rowTree = (panel: BrowsePrefs['panel'], me?: Me) => (
    <GateContext
      value={me === undefined ? undefined : { who: me, signOut: () => {} }}
    >
      <BrowsePrefsContext value={{ ...browseModule.defaults, panel }}>
        <MemoryRouter initialEntries={['/']}>
          <StatusContext.Provider value={defaultStatus}>
            <div className="library">
              <main>
                <Routes>
                  <Route path="/" element={<Library />} />
                  <Route
                    path="/read/:slug"
                    element={<p className="reader" />}
                  />
                </Routes>
              </main>
            </div>
          </StatusContext.Provider>
        </MemoryRouter>
      </BrowsePrefsContext>
    </GateContext>
  )
  /** `rowTree` mounted, its reads landed and its row drawn. */
  const row = async (panel: BrowsePrefs['panel'], me?: Me) => {
    let host!: HTMLElement
    await act(async () => {
      host = mount(rowTree(panel, me))
    })
    await land()
    if (host.querySelector('section.continue') === null) {
      throw new Error('no Continue row drawn')
    }
    return host
  }

  const cards = (host: HTMLElement) => [
    ...host.querySelectorAll<HTMLLIElement>('section.continue > ul > li'),
  ]

  /** The `index`th card's `i`, pressed, and the read it sends landed. */
  const pressI = async (host: HTMLElement, index: number) => {
    const i = cards(host)[index].querySelector<HTMLButtonElement>(
      ':scope > .panel-open',
    )
    if (i === null) throw new Error(`card ${index} has no i`)
    await flush(() => {
      i.click()
    })
    await land()
    return i
  }

  /** The alignment lines of every panel drawn, by their words. */
  const verdicts = (host: HTMLElement) =>
    [...host.querySelectorAll('.tile-panel')].map((panel) =>
      [...panel.querySelectorAll('.alignment-panel li')].map(
        (li) => li.textContent,
      ),
    )

  /** The row's panel, a child of its section. */
  const panelOf = (host: HTMLElement) =>
    host.querySelector<HTMLElement>('section.continue > .tile-panel')
  const need = <T extends Element>(el: T | null | undefined, what: string) => {
    if (!el) throw new Error(`no ${what}`)
    return el
  }
  const iOf = (card: Element) =>
    need(card.querySelector<HTMLButtonElement>(':scope > .panel-open'), 'i')
  const linkOf = (card: Element) =>
    need(card.querySelector<HTMLAnchorElement>(':scope > a'), 'link')
  const titlesOf = (host: HTMLElement) =>
    cards(host).map((li) => li.querySelector('.title')?.textContent)
  const fieldIn = (host: HTMLElement) =>
    need(
      panelOf(host)?.querySelector<HTMLTextAreaElement>(
        'textarea[name="title"]',
      ),
      'Title field',
    )
  const focusOn = (el: HTMLElement) =>
    flush(() => {
      el.focus()
    })
  /** A press dispatched where it would land: jsdom has no pointer. */
  const pressOn = (target: Element) =>
    flush(() => {
      target.dispatchEvent(new MouseEvent('pointerdown', { bubbles: true }))
    })
  /** Escape pressed where focus is, as a keyboard sends it. */
  const pressEscape = () =>
    flush(() => {
      ;(document.activeElement ?? document.body).dispatchEvent(
        new KeyboardEvent('keydown', {
          key: 'Escape',
          bubbles: true,
          cancelable: true,
        }),
      )
    })
  /**
   * Tab pressed where focus is, as a keyboard sends it, with `held` the
   * modifiers held; whether the page took it from the document's order.
   * jsdom moves no focus for a Tab, so focus moves only where the page
   * moves it.
   */
  const pressTab = async (held: KeyboardEventInit = {}) => {
    const tab = new KeyboardEvent('keydown', {
      key: 'Tab',
      bubbles: true,
      cancelable: true,
      ...held,
    })
    await flush(() => {
      ;(document.activeElement ?? document.body).dispatchEvent(tab)
    })
    return tab.defaultPrevented
  }
  /** A pointer entering `target`; React's `onMouseEnter` hears `mouseover`. */
  const hoverOn = async (target: Element) => {
    await flush(() => {
      target.dispatchEvent(new MouseEvent('mouseover', { bubbles: true }))
    })
    await land()
  }
  /**
   * The pointer moving from `from` straight into `to`: React hears the
   * move from the `mouseout`, whose `relatedTarget` is where it went, as
   * one leave and one enter.
   */
  const crossTo = async (from: Element, to: Element) => {
    await flush(() => {
      from.dispatchEvent(
        new MouseEvent('mouseout', { bubbles: true, relatedTarget: to }),
      )
      to.dispatchEvent(
        new MouseEvent('mouseover', { bubbles: true, relatedTarget: from }),
      )
    })
    await land()
  }
  /** The home lists answer `drawn` from the next read on. */
  const continuing = (...drawn: library.WorkCard[]) => {
    vi.mocked(library.home).mockResolvedValue({
      continuing: drawn,
      added: [],
      read: [],
    })
  }

  it('gives each readable card one i, named for it, beside its link and not in it, and an unreadable card none', async () => {
    const host = await row('button')
    expect(
      cards(host).map((li) => {
        const buttons = [...li.querySelectorAll('button.panel-open')]
        const link = li.querySelector(':scope > a')
        return {
          buttons: buttons.length,
          name: buttons[0]?.getAttribute('aria-label'),
          child: buttons[0]?.parentElement === li,
          inLink: buttons[0] !== undefined && link?.contains(buttons[0]),
          expanded: buttons[0]?.getAttribute('aria-expanded'),
        }
      }),
    ).toEqual([
      {
        buttons: 1,
        name: 'details for Bride v02',
        child: true,
        inLink: false,
        expanded: 'false',
      },
      {
        buttons: 0,
        name: undefined,
        child: false,
        inLink: false,
        expanded: undefined,
      },
      {
        buttons: 1,
        name: 'details for Heron v05',
        child: true,
        inLink: false,
        expanded: 'false',
      },
    ])
  })

  it('draws no i with the panel off, and opens no panel on a pointer or a focus', async () => {
    const host = await row('off')
    expect(host.querySelectorAll('.panel-open')).toHaveLength(0)
    for (const li of cards(host)) {
      const target = li.firstElementChild
      if (target === null) throw new Error('an empty card')
      await flush(() => {
        target.dispatchEvent(new MouseEvent('mouseover', { bubbles: true }))
        target.dispatchEvent(new FocusEvent('focusin', { bubbles: true }))
      })
    }
    await land()
    expect(host.querySelectorAll('.tile-panel')).toHaveLength(0)
    expect(vi.mocked(library.work)).not.toHaveBeenCalled()
  })

  it('draws no i on select, and opens the panel of the readable card focused or pointed at', async () => {
    const host = await row('on-select')
    expect(host.querySelectorAll('.panel-open')).toHaveLength(0)
    const [first, unreadableCard, third] = cards(host)
    await flush(() => {
      third
        .querySelector(':scope > a')
        ?.dispatchEvent(new FocusEvent('focusin', { bubbles: true }))
    })
    await land()
    expect(vi.mocked(library.work).mock.calls).toEqual([['heron-v05']])
    expect(verdicts(host)).toEqual([['raw → en confident · 178 of 180']])
    // An unreadable card has no panel in any mode, so a pointer over it
    // leaves the panel where it is.
    await flush(() => {
      unreadableCard.firstElementChild?.dispatchEvent(
        new MouseEvent('mouseover', { bubbles: true }),
      )
    })
    await land()
    expect(vi.mocked(library.work).mock.calls).toEqual([['heron-v05']])
    expect(verdicts(host)).toEqual([['raw → en confident · 178 of 180']])
    await flush(() => {
      first
        .querySelector(':scope > a')
        ?.dispatchEvent(new MouseEvent('mouseover', { bubbles: true }))
    })
    await land()
    expect(vi.mocked(library.work).mock.calls).toEqual([
      ['heron-v05'],
      ['bride-v02'],
    ])
    expect(verdicts(host)).toEqual([['raw → en partial · 178 of 180']])
  })

  it('still opens the reader from a card’s link on select', async () => {
    const host = await row('on-select')
    const link = cards(host)[2].querySelector<HTMLAnchorElement>(':scope > a')
    expect(link?.getAttribute('href')).toBe('/read/heron-v05')
    await flush(() => {
      link?.click()
    })
    expect(host.querySelector('section.continue')).toBeNull()
    expect(host.querySelector('p.reader')).not.toBeNull()
  })

  it('opens a card’s panel under the row, from one read of that card’s work, and i names the panel', async () => {
    const host = await row('button')
    const section = host.querySelector('section.continue')
    const i = await pressI(host, 2)
    expect(vi.mocked(library.work).mock.calls).toEqual([['heron-v05']])
    const panels = [...host.querySelectorAll<HTMLElement>('.tile-panel')]
    expect(panels).toHaveLength(1)
    expect(panels[0].parentElement).toBe(section)
    expect(verdicts(host)).toEqual([['raw → en confident · 178 of 180']])
    expect(
      cards(host).map((li) =>
        li.querySelector(':scope > .panel-open')?.getAttribute('aria-expanded'),
      ),
    ).toEqual(['false', undefined, 'true'])
    expect(panels[0].id).not.toBe('')
    expect(i.getAttribute('aria-controls')).toBe(panels[0].id)
  })

  it('draws the Title field afresh for each card its panel opens under, and saves it for that card', async () => {
    const host = await row('button', who({ admin: true }))
    const field = () => {
      const found = host.querySelector<HTMLTextAreaElement>(
        '.tile-panel textarea[name="title"]',
      )
      if (found === null) throw new Error('no Title field')
      return found
    }
    await pressI(host, 0)
    expect(field().value).toBe('Bride v02')
    await pressI(host, 2)
    expect(field().value).toBe('Heron v05')
    vi.mocked(metadata.patchWork).mockResolvedValueOnce({
      taiyaku_library: 1,
      updated: '2026-11-22T00:00:00Z',
      title: 'Heron, the fifth',
      persisted: true,
    })
    await typeInto(field(), 'Heron, the fifth')
    const form = host.querySelector('.tile-panel .metadata-editor')
    if (form === null) throw new Error('no editor')
    await submit(form)
    await land()
    expect(vi.mocked(metadata.patchWork).mock.calls).toEqual([
      ['heron-v05', { title: 'Heron, the fifth' }],
    ])
  })

  describe('↓ Download at the panel’s foot', () => {
    /** Heron read in its second edition, which its card shows. */
    const heronInEnglish = work({
      ...heron,
      editions: [
        {
          key: 'raw',
          label: 'Raw',
          members: 1,
          pages: 180,
          missing: 0,
          cover: coverOf('heron-v05', 'raw'),
        },
        {
          key: 'en',
          label: 'English',
          members: 1,
          pages: 184,
          missing: 0,
          cover: coverOf('heron-v05', 'en'),
        },
      ],
      opens: 'en',
    })

    beforeEach(() => {
      vi.mocked(library.home).mockResolvedValue({
        continuing: [bride, lost, heronInEnglish],
        added: [],
        read: [],
      })
    })

    it('downloads the edition the card shows', async () => {
      const host = await row('button', who())
      await pressI(host, 2)
      expect(
        [...host.querySelectorAll('a.get')].map((a) => [
          a.parentElement?.className,
          a.getAttribute('href'),
          a.textContent,
          a.getAttribute('download'),
          a.getAttribute('aria-label'),
        ]),
      ).toEqual([
        [
          'tile-panel',
          '/api/works/heron-v05/en/download',
          '↓ Download',
          '',
          'download Heron v05 — English',
        ],
      ])
    })

    it('is not offered to an account that may not download', async () => {
      const host = await row('button', who({ can_download: false }))
      await pressI(host, 2)
      expect(verdicts(host)).toEqual([['raw → en confident · 178 of 180']])
      expect(host.querySelectorAll('a.get')).toHaveLength(0)
    })
  })

  it('closes on Escape with focus back on the card’s i, on i again, and on a press outside', async () => {
    const host = await row('button', who())
    const card = cards(host)[2]
    await pressI(host, 2)
    await focusOn(
      need(panelOf(host)?.querySelector<HTMLElement>('a.get'), '↓ Download'),
    )
    await pressEscape()
    expect(panelOf(host), 'after Escape').toBeNull()
    expect(document.activeElement).toBe(iOf(card))

    await pressI(host, 2)
    expect(panelOf(host), 'opened again').not.toBeNull()
    await pressI(host, 2)
    expect(panelOf(host), 'after i again').toBeNull()

    await pressI(host, 2)
    await pressOn(need(host.querySelector('section.continue > h2'), 'heading'))
    expect(panelOf(host), 'after a press outside').toBeNull()
  })

  it('stays open on a press inside its panel, which lies outside its card', async () => {
    const host = await row('button', who({ admin: true }))
    await pressI(host, 2)
    const panel = need(panelOf(host), 'panel')
    for (const [what, inside] of [
      ['the Title field', panel.querySelector('textarea[name="title"]')],
      ['the alignment detail', panel.querySelector('.alignment-panel')],
      ['↓ Download', panel.querySelector('a.get')],
      ['the card’s link', linkOf(cards(host)[2])],
    ] as const) {
      await pressOn(need(inside, what))
      expect(panelOf(host), `after a press on ${what}`).toBe(panel)
    }
  })

  it('stays open on focus arriving inside its panel in button mode, and closes on focus arriving outside', async () => {
    const host = await row('button', who({ admin: true }))
    await pressI(host, 2)
    const panel = need(panelOf(host), 'panel')
    for (const [what, inside] of [
      ['the Title field', panel.querySelector<HTMLElement>('textarea')],
      ['↓ Download', panel.querySelector<HTMLElement>('a.get')],
    ] as const) {
      await focusOn(need(inside, what))
      expect(panelOf(host), `after focus on ${what}`).toBe(panel)
    }
    await focusOn(
      need(
        host.querySelector<HTMLElement>('.library-chip-group button'),
        'chip',
      ),
    )
    expect(panelOf(host), 'after focus outside').toBeNull()
  })

  it('joins a card and its panel for Tab: from i into the panel, back by Shift+Tab, and from its end past an unreadable card to the next card’s link', async () => {
    const host = await row('button', who({ admin: true }))
    const [first, , last] = cards(host)
    await pressI(host, 0)
    await focusOn(iOf(first))
    expect(
      { taken: await pressTab(), focus: document.activeElement },
      'Tab from i',
    ).toEqual({ taken: true, focus: fieldIn(host) })
    expect(
      {
        taken: await pressTab({ shiftKey: true }),
        focus: document.activeElement,
      },
      'Shift+Tab from the Title field',
    ).toEqual({ taken: true, focus: iOf(first) })
    await focusOn(
      need(panelOf(host)?.querySelector<HTMLElement>('a.get'), '↓ Download'),
    )
    expect(
      {
        taken: await pressTab(),
        focus: document.activeElement,
        panel: panelOf(host),
      },
      'Tab from ↓ Download',
    ).toEqual({ taken: true, focus: linkOf(last), panel: null })
  })

  it('leaves every other Tab to the document’s order: no panel open, the last card’s panel’s end, Shift+Tab from i, a modifier held, from the link and inside the panel', async () => {
    const host = await row('button', who({ admin: true }))
    const [first, , last] = cards(host)
    await focusOn(iOf(first))
    expect(await pressTab(), 'Tab from i, no panel open').toBe(false)

    await pressI(host, 2)
    await focusOn(
      need(panelOf(host)?.querySelector<HTMLElement>('a.get'), '↓ Download'),
    )
    expect(await pressTab(), 'Tab from the last card’s panel').toBe(false)
    await focusOn(iOf(last))
    expect(await pressTab({ shiftKey: true }), 'Shift+Tab from i').toBe(false)
    for (const held of ['ctrlKey', 'metaKey', 'altKey'] as const) {
      expect(await pressTab({ [held]: true }), `Tab from i, ${held}`).toBe(
        false,
      )
    }
    await focusOn(linkOf(last))
    expect(await pressTab(), 'Tab from the link').toBe(false)
    await focusOn(fieldIn(host))
    expect(await pressTab(), 'Tab inside the panel').toBe(false)
  })

  it('passes over a panel with no stop, a reader’s, for Tab from its card’s i', async () => {
    const host = await row('button', who({ can_download: false }))
    await pressI(host, 0)
    // The fixture tells: the panel is open and holds nothing Tab stops on.
    expect(
      need(panelOf(host), 'panel').querySelectorAll(
        'a[href], button, input, select, textarea, [tabindex]',
      ),
    ).toHaveLength(0)
    await focusOn(iOf(cards(host)[0]))
    expect(await pressTab()).toBe(false)
  })

  it('goes back by Shift+Tab from the next readable card’s link to the panel’s last stop while a save that failed holds the panel open, and leaves Shift+Tab from its own card’s link to the document’s order', async () => {
    vi.mocked(metadata.patchWork).mockRejectedValue(
      new Error('the library is read-only'),
    )
    const host = await row('button', who({ admin: true }))
    const [first, , last] = cards(host)
    await pressI(host, 0)
    await typeInto(fieldIn(host), 'Bride, the second')
    await focusOn(linkOf(last))
    await land()
    // The fixture tells: focus on Heron's link, past the unreadable card,
    // and Bride's panel held open by the save that failed.
    expect({
      focus: document.activeElement,
      open: iOf(first).getAttribute('aria-expanded'),
      saves: vi.mocked(metadata.patchWork).mock.calls.length,
    }).toEqual({ focus: linkOf(last), open: 'true', saves: 1 })
    expect(
      {
        taken: await pressTab({ shiftKey: true }),
        focus: document.activeElement,
      },
      'Shift+Tab from Heron’s link',
    ).toEqual({
      taken: true,
      focus: panelOf(host)?.querySelector('a.get'),
    })
    await focusOn(linkOf(first))
    expect(
      await pressTab({ shiftKey: true }),
      'Shift+Tab from Bride’s link',
    ).toBe(false)
  })

  it('closes an on-select panel on Escape with focus on the card’s link', async () => {
    const host = await row('on-select', who({ admin: true }))
    const card = cards(host)[2]
    await hoverOn(linkOf(card))
    await focusOn(fieldIn(host))
    // The field's own Escape: the panel stays.
    await pressEscape()
    expect(panelOf(host)?.contains(document.activeElement)).toBe(true)
    await pressEscape()
    expect(panelOf(host)).toBeNull()
    expect(document.activeElement).toBe(linkOf(card))
  })

  // Held against the arm from the row's last stop left out, and against the
  // arm back to it left out.
  it('goes from the row’s last stop to the stop after the row, and back by Shift+Tab, while a save that failed holds an earlier card’s panel open', async () => {
    vi.mocked(metadata.patchWork).mockRejectedValue(
      new Error('the library is read-only'),
    )
    const host = await row('button', who({ admin: true }))
    const [first, , last] = cards(host)
    await pressI(host, 0)
    await typeInto(fieldIn(host), 'Bride, the second')
    await focusOn(iOf(last))
    await land()
    const chip = need(
      host.querySelector<HTMLElement>('.library-chip-group button'),
      'chip',
    )
    // The fixture tells: focus on the row's last stop, and Bride's panel
    // held open by the save that failed.
    expect({
      focus: document.activeElement,
      open: iOf(first).getAttribute('aria-expanded'),
      saves: vi.mocked(metadata.patchWork).mock.calls.length,
    }).toEqual({ focus: iOf(last), open: 'true', saves: 1 })
    expect(
      { taken: await pressTab(), focus: document.activeElement },
      'Tab from Heron’s i',
    ).toEqual({ taken: true, focus: chip })
    expect(
      {
        taken: await pressTab({ shiftKey: true }),
        focus: document.activeElement,
      },
      'Shift+Tab from the first chip',
    ).toEqual({ taken: true, focus: iOf(last) })
  })

  // Held against the arm back from the stop after the row left out, and
  // against that arm taken under the row's last card too.
  it('goes back by Shift+Tab from the stop after the row to the last card’s link while the pointer holds an earlier card’s panel open on select, and leaves that Shift+Tab to the document’s order under the last card', async () => {
    const host = await row('on-select', who({ admin: true }))
    const [first, , last] = cards(host)
    await hoverOn(linkOf(first))
    const chip = need(
      host.querySelector<HTMLElement>('.library-chip-group button'),
      'chip',
    )
    await focusOn(chip)
    // The fixture tells: Bride's panel open, focus on the first chip.
    expect({ verdicts: verdicts(host), focus: document.activeElement }).toEqual(
      { verdicts: [['raw → en partial · 178 of 180']], focus: chip },
    )
    expect(
      {
        taken: await pressTab({ shiftKey: true }),
        focus: document.activeElement,
      },
      'Shift+Tab under Bride',
    ).toEqual({ taken: true, focus: linkOf(last) })
    await land()
    // The fixture tells: focus on Heron's link moved the panel there.
    expect(verdicts(host)).toEqual([['raw → en confident · 178 of 180']])
    await focusOn(chip)
    expect(await pressTab({ shiftKey: true }), 'Shift+Tab under Heron').toBe(
      false,
    )
  })

  describe('on select, a move to another card waiting on the save', () => {
    const saving = () => {
      const save = deferred<metadata.PatchResult>()
      vi.mocked(metadata.patchWork).mockReturnValueOnce(save.promise)
      return async () => {
        await flush(() => {
          save.resolve({
            taiyaku_library: 1,
            updated: '2026-11-22T00:00:00Z',
            title: 'Bride, the second',
            persisted: true,
          })
        })
        await land()
      }
    }
    /** Bride's panel opened by `open`, and a title typed in it. */
    const typed = async (open: (host: HTMLElement) => Promise<unknown>) => {
      const host = await row('on-select', who({ admin: true }))
      await open(host)
      await land()
      await focusOn(fieldIn(host))
      await typeInto(fieldIn(host), 'Bride, the second')
      return host
    }
    const BRIDE = [['raw → en partial · 178 of 180']]
    const HERON = [['raw → en confident · 178 of 180']]

    // Held against the panel left without its card's focus handler.
    it('is dropped as focus comes back into the panel, by Shift+Tab from the next card’s link, and focus stays there', async () => {
      const landed = saving()
      const host = await typed((at) => focusOn(linkOf(cards(at)[0])))
      const [, , last] = cards(host)
      // On to Heron's link: the field's leave sends its save, and the move
      // to Heron waits on it.
      await focusOn(linkOf(last))
      const taken = await pressTab({ shiftKey: true })
      // The fixture tells: focus back in Bride's panel, held open with its
      // save out.
      expect({
        taken,
        verdicts: verdicts(host),
        focus: panelOf(host)?.contains(document.activeElement),
        saves: vi.mocked(metadata.patchWork).mock.calls,
      }).toEqual({
        taken: true,
        verdicts: BRIDE,
        focus: true,
        saves: [['bride-v02', { title: 'Bride, the second' }]],
      })
      const at = document.activeElement

      await landed()
      expect({
        verdicts: verdicts(host),
        focus: document.activeElement === at,
        saves: vi.mocked(metadata.patchWork).mock.calls.length,
      }).toEqual({ verdicts: BRIDE, focus: true, saves: 1 })
    })

    // Held against the panel left without its card's pointer handler.
    it('is dropped as the pointer comes back into the panel from that card', async () => {
      const landed = saving()
      const host = await typed((at) => hoverOn(linkOf(cards(at)[0])))
      const [, , last] = cards(host)
      const panel = need(panelOf(host), 'panel')
      const detail = need(panel.querySelector('.alignment-panel'), 'detail')
      // A press on the panel's blank space takes focus from the field, whose
      // leave sends its save; the pointer goes up to Heron's card.
      await flush(() => {
        fieldIn(host).blur()
      })
      await crossTo(detail, linkOf(last))
      // The fixture tells: Bride's panel held with its save out.
      expect({
        verdicts: verdicts(host),
        saves: vi.mocked(metadata.patchWork).mock.calls.length,
      }).toEqual({ verdicts: BRIDE, saves: 1 })

      await crossTo(linkOf(last), detail)
      await landed()
      expect(verdicts(host)).toEqual(BRIDE)
    })

    // Held against either check left out: the panel's enter heard from
    // its own card, or its card's enter heard from the panel.
    it('is kept as the pointer crosses between the open card and its panel', async () => {
      const landed = saving()
      const host = await typed((at) => hoverOn(linkOf(cards(at)[0])))
      const [first, , last] = cards(host)
      const detail = need(
        panelOf(host)?.querySelector('.alignment-panel'),
        'detail',
      )
      // Focus on to Heron's link: the field's leave sends its save, and the
      // move to Heron waits on it.
      await focusOn(linkOf(last))
      await crossTo(linkOf(first), detail)
      await crossTo(detail, linkOf(first))
      // The fixture tells: Bride's panel held with its save out.
      expect({
        verdicts: verdicts(host),
        saves: vi.mocked(metadata.patchWork).mock.calls.length,
      }).toEqual({ verdicts: BRIDE, saves: 1 })

      await landed()
      expect({
        verdicts: verdicts(host),
        focus: document.activeElement === linkOf(last),
      }).toEqual({ verdicts: HERON, focus: true })
    })
  })

  it('reads home again after a title saved in its panel, and the card draws the new title', async () => {
    const host = await row('button', who({ admin: true }))
    await pressI(host, 2)
    vi.mocked(metadata.patchWork).mockResolvedValueOnce({
      taiyaku_library: 1,
      updated: '2026-11-22T00:00:00Z',
      title: 'Heron, the fifth',
      persisted: true,
    })
    vi.mocked(library.home).mockClear()
    continuing(bride, lost, { ...heron, title: 'Heron, the fifth' })
    await typeInto(fieldIn(host), 'Heron, the fifth')
    await submit(need(panelOf(host)?.querySelector('.metadata-editor'), 'form'))
    await land()
    expect(vi.mocked(library.home)).toHaveBeenCalledTimes(1)
    expect(titlesOf(host)).toEqual([
      'Bride v02',
      'Lost v01',
      'Heron, the fifth',
    ])
    expect(iOf(cards(host)[2]).getAttribute('aria-label')).toBe(
      'details for Heron, the fifth',
    )
    expect(panelOf(host), 'still open under its card').not.toBeNull()
  })

  describe('a Split answered in its panel', () => {
    /** Heron's own read: its English joined to it by content, pair 7. */
    const heronMarked = (): library.WorkDetail => ({
      ...heron,
      editions: [
        ...heron.editions,
        {
          key: 'en',
          label: 'English',
          members: 1,
          pages: 184,
          missing: 0,
          cover: coverOf('heron-v05', 'en'),
          matched: { id: 7, score: 0.91 },
        },
      ],
      series: null,
      alignments: [],
      kept_apart: [],
    })

    /**
     * Heron's panel open in `mode` for an administrator, focus on Split's
     * confirm, and the press made: the home lists answer `after` from then
     * on. In `button` mode focus arriving on `main` closes the panel by
     * itself; only `on-select` holds the row's own close.
     */
    const splitHeron = async (
      mode: 'button' | 'on-select',
      ...after: library.WorkCard[]
    ) => {
      vi.mocked(library.work).mockImplementation(async (slug) =>
        slug === 'heron-v05'
          ? heronMarked()
          : { ...work({ slug }), series: null, alignments: [] },
      )
      vi.mocked(matches.splitMatch).mockReset()
      vi.mocked(matches.splitMatch).mockImplementation(async () => {
        continuing(...after)
        return { split: [7], derive: emptyDerive }
      })
      const host = await row(mode, who({ admin: true }))
      if (mode === 'button') await pressI(host, 2)
      else await hoverOn(linkOf(cards(host)[2]))
      await press(
        need(panelOf(host)?.querySelector('button[name="split"]'), 'Split…'),
      )
      const confirm = need(
        panelOf(host)?.querySelector<HTMLElement>(
          'button[name="confirm-split"]',
        ),
        'its confirm',
      )
      await focusOn(confirm)
      vi.mocked(library.home).mockClear()
      await press(confirm)
      await land()
      expect(vi.mocked(matches.splitMatch).mock.calls).toEqual([
        [7, 'heron-v05', 'en'],
      ])
      expect(vi.mocked(library.home)).toHaveBeenCalledTimes(1)
      return host
    }

    /** A later read, as a scan's end sends, answering `drawn`. */
    const readAgain = async (...drawn: library.WorkCard[]) => {
      continuing(...drawn)
      await flush(() => {
        invalidateLibraryViews()
      })
      await land()
    }

    it('leaves focus on the panel where the read after it draws no Undo, and Shift+Tab from there goes back to the card’s i, the panel staying open', async () => {
      const host = await splitHeron('button', bride, lost, heron)
      const panel = need(panelOf(host), 'panel')
      // The fixture tells: focus on the panel itself.
      expect(document.activeElement).toBe(panel)
      expect(
        {
          taken: await pressTab({ shiftKey: true }),
          focus: document.activeElement,
          panel: panelOf(host),
        },
        'Shift+Tab from the panel',
      ).toEqual({ taken: true, focus: iOf(cards(host)[2]), panel })
    })

    it.each(['button', 'on-select'] as const)(
      'gives focus to main where the read after it has no such card, and draws no panel when the card comes back (%s)',
      async (mode) => {
        const host = await splitHeron(mode, bride, lost)
        expect(titlesOf(host)).toEqual(['Bride v02', 'Lost v01'])
        expect(panelOf(host)).toBeNull()
        expect(document.activeElement).toBe(
          host.querySelector('.library > main'),
        )

        await readAgain(bride, lost, heron)
        expect(titlesOf(host)).toEqual(['Bride v02', 'Lost v01', 'Heron v05'])
        expect(panelOf(host)).toBeNull()
        expect(
          cards(host)[2].querySelector('[aria-expanded="true"]'),
        ).toBeNull()
      },
    )

    it.each(['button', 'on-select'] as const)(
      'closes as for a card that left where the read after it finds the card unreadable (%s)',
      async (mode) => {
        const heronLost = work({
          ...heron,
          editions: [{ ...heron.editions[0], missing: 1 }],
        })
        const host = await splitHeron(mode, bride, lost, heronLost)
        expect(titlesOf(host)).toEqual(['Bride v02', 'Lost v01', 'Heron v05'])
        expect(cards(host)[2].querySelector('.panel-open')).toBeNull()
        expect(panelOf(host)).toBeNull()
        expect(document.activeElement).toBe(
          host.querySelector('.library > main'),
        )

        await readAgain(bride, lost, heron)
        // The fixture tells: the card is readable again.
        expect(cards(host)[2].querySelector(':scope > a')).not.toBeNull()
        expect(panelOf(host)).toBeNull()
        expect(
          cards(host)[2].querySelector('[aria-expanded="true"]'),
        ).toBeNull()
      },
    )
  })

  it('closes an on-select panel when the strip goes, so the strip comes back with none', async () => {
    vi.mocked(library.search).mockResolvedValue({
      series: [],
      unfiled: [],
      query: 'heron',
    })
    const host = await row('on-select')
    await hoverOn(linkOf(cards(host)[2]))
    expect(verdicts(host)).toEqual([['raw → en confident · 178 of 180']])
    const box = need(
      host.querySelector<HTMLInputElement>('input[type="search"]'),
      'search box',
    )
    // On select, focus arriving outside keeps the panel.
    await focusOn(box)
    expect(panelOf(host), 'focus in the box').not.toBeNull()
    await typeInto(box, 'heron')
    await land()
    expect(host.querySelector('section.continue'), 'searching').toBeNull()
    await typeInto(box, '')
    await land()
    expect(host.querySelector('section.continue'), 'cleared').not.toBeNull()
    expect(panelOf(host)).toBeNull()
  })

  it.each(['button', 'on-select'] as const)(
    'closes an open panel when Panel is switched to off, so switching back to %s draws none',
    async (mode) => {
      const host = await row(mode)
      if (mode === 'button') await pressI(host, 2)
      else await hoverOn(linkOf(cards(host)[2]))
      expect(verdicts(host)).toEqual([['raw → en confident · 178 of 180']])
      rerender(rowTree('off'))
      await land()
      expect(panelOf(host), 'off').toBeNull()
      rerender(rowTree(mode))
      await land()
      expect(panelOf(host), `back to ${mode}`).toBeNull()
      if (mode === 'button') {
        expect(iOf(cards(host)[2]).getAttribute('aria-expanded')).toBe('false')
      }
    },
  )

  /**
   * The open panel follows its card (O417): laid again as the row scrolls,
   * as `useContinueRow` writes the row's style, and at a resize of the card,
   * the row or the section; and closed once the card has wholly left the
   * row's view. jsdom lays nothing out and has no `ResizeObserver`, so the
   * boxes are stubbed as a browser lays them, and a stub observer keeps what
   * each one watches and reports a resize to those watching it. The style
   * watch is jsdom's own `MutationObserver`.
   */
  describe('the panel following its card', () => {
    class StubResizeObserver {
      static made: StubResizeObserver[] = []
      watched: Element[] = []
      report: ResizeObserverCallback
      constructor(report: ResizeObserverCallback) {
        this.report = report
        StubResizeObserver.made.push(this)
      }
      observe(el: Element) {
        this.watched.push(el)
      }
      unobserve(el: Element) {
        this.watched = this.watched.filter((each) => each !== el)
      }
      disconnect() {
        this.watched = []
      }
    }

    /** What each observer watches, leaving out those that watch nothing. */
    const watching = () =>
      StubResizeObserver.made
        .map((each) => each.watched)
        .filter((watched) => watched.length > 0)

    /** A resize of `el`, reported to each observer watching it. */
    const resize = (el: Element) =>
      flush(() => {
        for (const each of [...StubResizeObserver.made]) {
          if (each.watched.includes(el)) {
            each.report([], each as unknown as ResizeObserver)
          }
        }
      })

    beforeEach(() => {
      StubResizeObserver.made = []
      vi.stubGlobal('ResizeObserver', StubResizeObserver)
    })
    afterEach(() => {
      vi.unstubAllGlobals()
    })

    /**
     * The row's boxes as a browser lays them beside the sidebar: the section
     * is the page area, 976 px wide from 264, its top at 120; the row's
     * view, its padding box, runs from 264 to 1240, and its foot is at 380.
     * Each card is `at.wide` px wide, `at.wide` and the 23 px gap on from
     * the one before, drawn `at.scrolled` px left of where it stands at
     * rest. At rest, Heron's panel lies 334 px into the area, 311 px wide.
     */
    const laidOut = (host: HTMLElement) => {
      const section = need(
        host.querySelector<HTMLElement>('section.continue'),
        'section',
      )
      const list = need(
        section.querySelector<HTMLElement>(':scope > ul'),
        'row',
      )
      const at = { scrolled: 0, wide: 144 }
      // Written before any panel opens, so no style watch hears it.
      list.style.columnGap = '23px'
      vi.spyOn(section, 'getBoundingClientRect').mockReturnValue({
        left: 264,
        right: 1240,
        width: 976,
        top: 120,
        bottom: 420,
      } as DOMRect)
      vi.spyOn(list, 'getBoundingClientRect').mockReturnValue({
        left: 260,
        right: 1244,
        width: 984,
        top: 160,
        bottom: 380,
      } as DOMRect)
      Object.defineProperty(list, 'clientLeft', { value: 4 })
      Object.defineProperty(list, 'clientWidth', { value: 976 })
      cards(host).forEach((card, index) => {
        vi.spyOn(card, 'getBoundingClientRect').mockImplementation(() => {
          const left = 264 + index * (at.wide + 23) - at.scrolled
          return {
            left,
            right: left + at.wide,
            width: at.wide,
            top: 164,
            bottom: 372,
          } as DOMRect
        })
      })
      return { section, list, at }
    }

    /** The row's panel's place, as its style lays it. */
    const placeOf = (host: HTMLElement) => {
      const panel = panelOf(host)
      return panel === null
        ? null
        : {
            left: panel.style.left,
            top: panel.style.top,
            width: panel.style.width,
          }
    }
    const AT_REST = { left: '334px', top: '260px', width: '311px' }
    /** The row scrolled `by` px on, and its `scroll` sent. */
    const scrollRow = (
      list: HTMLElement,
      at: { scrolled: number },
      by: number,
    ) =>
      flush(() => {
        at.scrolled += by
        list.dispatchEvent(new Event('scroll'))
      })

    // Held against a follow that adds no `scroll` listener to the row.
    it('lies under its card as the row scrolls sideways, and inside the page area with the card half out of view', async () => {
      const host = await row('button')
      const { list, at } = laidOut(host)
      await pressI(host, 2)
      const panel = panelOf(host)
      expect(placeOf(host), 'opened').toEqual(AT_REST)

      await scrollRow(list, at, 167)
      expect(panelOf(host), 'the same panel').toBe(panel)
      expect(placeOf(host), 'one card on').toEqual({
        left: '167px',
        top: '260px',
        width: '311px',
      })

      await scrollRow(list, at, 233)
      expect(placeOf(host), 'its card half out').toEqual({
        left: '0px',
        top: '260px',
        width: '311px',
      })
    })

    // Held against a style watch that observes nothing.
    it('lies under its card as the row is laid through its style, as a slide lays it', async () => {
      const host = await row('button')
      const { list, at } = laidOut(host)
      await pressI(host, 2)
      expect(placeOf(host), 'opened').toEqual(AT_REST)
      await flush(() => {
        at.wide = 160
        list.style.setProperty('--continue-col', '160px')
      })
      expect(placeOf(host)).toEqual({
        left: '366px',
        top: '260px',
        width: '343px',
      })
    })

    // Held against each of the three left unwatched.
    it.each(['card', 'row', 'section'] as const)(
      'lies under its card after a resize of its %s',
      async (which) => {
        const host = await row('button')
        const { section, list, at } = laidOut(host)
        await pressI(host, 2)
        expect(placeOf(host), 'opened').toEqual(AT_REST)
        at.wide = 160
        await resize({ card: cards(host)[2], row: list, section }[which])
        expect(placeOf(host)).toEqual({
          left: '366px',
          top: '260px',
          width: '343px',
        })
      },
    )

    // Held against the close left out, and against a close that gives focus
    // to nothing.
    it.each([
      ['button', 'i'],
      ['on-select', 'link'],
    ] as const)(
      'in %s mode, closes once its card has wholly left the row’s view, with focus from its panel on the card’s %s',
      async (mode, to) => {
        const host = await row(mode, who({ admin: true }))
        const { list, at } = laidOut(host)
        const card = cards(host)[2]
        if (mode === 'button') await pressI(host, 2)
        else await hoverOn(linkOf(card))
        await focusOn(fieldIn(host))

        await scrollRow(list, at, 400)
        expect(placeOf(host), 'its card half out').toEqual({
          left: '0px',
          top: '260px',
          width: '311px',
        })

        await scrollRow(list, at, 101)
        await land()
        expect(panelOf(host), 'wholly out').toBeNull()
        expect(document.activeElement, `focus on the card’s ${to}`).toBe(
          mode === 'button' ? iOf(card) : linkOf(card),
        )
        expect(vi.mocked(metadata.patchWork)).not.toHaveBeenCalled()
      },
    )

    // Held against a close that gives focus to the card wherever focus is.
    it('leaves focus where it is outside its panel as it closes on select', async () => {
      const host = await row('on-select')
      const { list, at } = laidOut(host)
      await hoverOn(linkOf(cards(host)[2]))
      const box = need(
        host.querySelector<HTMLInputElement>('input[type="search"]'),
        'search box',
      )
      await focusOn(box)
      expect(panelOf(host), 'focus in the box').not.toBeNull()

      await scrollRow(list, at, 501)
      await land()
      expect(panelOf(host), 'wholly out').toBeNull()
      expect(document.activeElement).toBe(box)
    })

    // Held against the close dropped by a choice of the card it closes:
    // focus or the pointer coming back into its panel while the save is out.
    it.each(['focus', 'the pointer'] as const)(
      'closes on select once its card has wholly left the row’s view and the save has landed, though %s came back into its panel meanwhile',
      async (by) => {
        const save = deferred<metadata.PatchResult>()
        vi.mocked(metadata.patchWork).mockReturnValueOnce(save.promise)
        const host = await row('on-select', who({ admin: true }))
        const { list, at } = laidOut(host)
        await hoverOn(linkOf(cards(host)[2]))
        await focusOn(fieldIn(host))
        await typeInto(fieldIn(host), 'Heron v05 (2)')
        // A press on the panel's blank space takes focus from the field,
        // whose leave sends its save.
        await flush(() => {
          fieldIn(host).blur()
        })
        await scrollRow(list, at, 501)
        await land()
        const panel = need(panelOf(host), 'panel')
        // The fixture tells: the card wholly out, and its panel held by the
        // save out.
        expect(vi.mocked(metadata.patchWork).mock.calls).toEqual([
          ['heron-v05', { title: 'Heron v05 (2)' }],
        ])
        if (by === 'focus') await focusOn(fieldIn(host))
        else
          await hoverOn(need(panel.querySelector('.alignment-panel'), 'detail'))

        await flush(() => {
          save.resolve({
            taiyaku_library: 1,
            updated: '2026-11-22T00:00:00Z',
            title: 'Heron v05 (2)',
            persisted: true,
          })
        })
        await land()
        expect({
          drawn: panelOf(host) !== null,
          saves: vi.mocked(metadata.patchWork).mock.calls.length,
        }).toEqual({ drawn: false, saves: 1 })
      },
    )

    /**
     * Heron's panel open in `mode` for an administrator, a title typed in it
     * and its save held out, focus taken from the field, and Heron then
     * scrolled wholly out of the row's view and back: answers the host and
     * the save's landing.
     */
    const outAndBack = async (mode: 'button' | 'on-select') => {
      const save = deferred<metadata.PatchResult>()
      vi.mocked(metadata.patchWork).mockReturnValueOnce(save.promise)
      const host = await row(mode, who({ admin: true }))
      const { list, at } = laidOut(host)
      if (mode === 'button') await pressI(host, 2)
      else await hoverOn(linkOf(cards(host)[2]))
      await focusOn(fieldIn(host))
      await typeInto(fieldIn(host), 'Heron v05 (2)')
      // A press on the panel's blank space takes focus from the field,
      // whose leave sends its save.
      await flush(() => {
        fieldIn(host).blur()
      })
      await scrollRow(list, at, 501)
      await scrollRow(list, at, -501)
      await land()
      // The fixture tells: the save out, and the panel held for it.
      expect({
        drawn: panelOf(host) !== null,
        saves: vi.mocked(metadata.patchWork).mock.calls,
      }).toEqual({
        drawn: true,
        saves: [['heron-v05', { title: 'Heron v05 (2)' }]],
      })
      const landed = async () => {
        await flush(() => {
          save.resolve({
            taiyaku_library: 1,
            updated: '2026-11-22T00:00:00Z',
            title: 'Heron v05 (2)',
            persisted: true,
          })
        })
        await land()
      }
      return { host, landed }
    }

    // Held against the close left waiting as the card comes back into view.
    it.each(['button', 'on-select'] as const)(
      'stays open on its card in %s mode once its save lands, the card having left the row’s view and come back',
      async (mode) => {
        const { host, landed } = await outAndBack(mode)
        await landed()
        expect({
          drawn: panelOf(host) !== null,
          saves: vi.mocked(metadata.patchWork).mock.calls.length,
        }).toEqual({ drawn: true, saves: 1 })
      },
    )

    // Held against the close left waiting as the card comes back into view,
    // which the card's own focus or pointer then leaves waiting too.
    it.each(['focus', 'the pointer'] as const)(
      'stays open on select once its save lands, the card having come back into the row’s view and %s having come onto it',
      async (by) => {
        const { host, landed } = await outAndBack('on-select')
        const link = linkOf(cards(host)[2])
        if (by === 'focus') await focusOn(link)
        else await hoverOn(link)
        await landed()
        expect({
          drawn: panelOf(host) !== null,
          focus: document.activeElement === link,
          saves: vi.mocked(metadata.patchWork).mock.calls.length,
        }).toEqual({ drawn: true, focus: by === 'focus', saves: 1 })
      },
    )

    // Held against a card coming back into view dropping whatever waits on
    // the save, rather than only the close its leaving asked.
    it.each(['an Escape close', 'a move to Bride'] as const)(
      'leaves %s waiting on the save as its card comes back into the row’s view, and makes it once the save lands',
      async (what) => {
        const save = deferred<metadata.PatchResult>()
        vi.mocked(metadata.patchWork).mockReturnValueOnce(save.promise)
        const host = await row('on-select', who({ admin: true }))
        const { list, at } = laidOut(host)
        await hoverOn(linkOf(cards(host)[2]))
        await focusOn(fieldIn(host))
        await typeInto(fieldIn(host), 'Heron v05 (2)')
        // A press on the panel's blank space takes focus from the field,
        // whose leave sends its save.
        await flush(() => {
          fieldIn(host).blur()
        })
        await scrollRow(list, at, 501)
        if (what === 'an Escape close') await pressEscape()
        else await focusOn(linkOf(cards(host)[0]))
        await scrollRow(list, at, -501)
        await land()
        // The fixture tells: Heron's panel held, its save out.
        expect({
          verdicts: verdicts(host),
          saves: vi.mocked(metadata.patchWork).mock.calls,
        }).toEqual({
          verdicts: [['raw → en confident · 178 of 180']],
          saves: [['heron-v05', { title: 'Heron v05 (2)' }]],
        })

        await flush(() => {
          save.resolve({
            taiyaku_library: 1,
            updated: '2026-11-22T00:00:00Z',
            title: 'Heron v05 (2)',
            persisted: true,
          })
        })
        await land()
        expect(verdicts(host)).toEqual(
          what === 'an Escape close' ? [] : [['raw → en partial · 178 of 180']],
        )
      },
    )

    // Held against the close asked at each scroll with the card out.
    it('stays open on its card’s failed save, and sends it once, as the row scrolls on', async () => {
      vi.mocked(metadata.patchWork).mockRejectedValue(
        new Error('the server is away'),
      )
      const host = await row('button', who({ admin: true }))
      const { list, at } = laidOut(host)
      await pressI(host, 2)
      const panel = panelOf(host)
      await typeInto(fieldIn(host), 'Heron v05 (2)')

      await scrollRow(list, at, 501)
      await land()
      const saved = [['heron-v05', { title: 'Heron v05 (2)' }]]
      expect({
        same: panelOf(host) === panel,
        failed: panel?.querySelector('[role="alert"]') !== null,
        saves: vi.mocked(metadata.patchWork).mock.calls,
      }).toEqual({ same: true, failed: true, saves: saved })

      await scrollRow(list, at, 167)
      await land()
      expect({
        same: panelOf(host) === panel,
        saves: vi.mocked(metadata.patchWork).mock.calls,
      }).toEqual({ same: true, saves: saved })
    })

    // Held against each of the three not undone when the follow moves to
    // another card: the `scroll` listener, the style watch and the observer.
    it('follows only the card its panel moved to, through a scroll, a style write and a resize', async () => {
      const host = await row('button')
      const { list, at } = laidOut(host)
      await pressI(host, 0)
      await pressI(host, 2)
      expect(placeOf(host), 'moved to Heron').toEqual(AT_REST)

      // Bride wholly out of the view, Heron in it.
      await scrollRow(list, at, 167)
      const laid = { left: '167px', top: '260px', width: '311px' }
      expect(placeOf(host), 'after the scroll').toEqual(laid)
      await flush(() => {
        list.style.setProperty('--continue-gap', '23px')
      })
      expect(placeOf(host), 'after a style write').toEqual(laid)
      await resize(list)
      expect(placeOf(host), 'after a resize').toEqual(laid)
      expect(verdicts(host)).toEqual([['raw → en confident · 178 of 180']])
    })

    // Held against the observer not disconnected as the panel closes.
    it('watches its card, the row and the section while open, and none of them once closed', async () => {
      const host = await row('button')
      const { section, list } = laidOut(host)
      const card = cards(host)[2]
      expect(watching(), 'none open').toEqual([[list]])
      await pressI(host, 2)
      expect(watching(), 'open').toEqual([[list], [card, list, section]])
      await pressI(host, 2)
      expect(panelOf(host), 'closed').toBeNull()
      expect(watching(), 'closed').toEqual([[list]])
    })
  })
})

/**
 * A child that draws the router's query string, so a test can read what the
 * box put in the URL. Declared inside a function for the reason
 * `Shell.test.tsx`'s `reporter()` gives: `style/useComponentExportOnlyModules`
 * refuses an unexported component at module scope, and
 * `suspicious/noExportsInTest` refuses the export.
 */
function locationProbe() {
  function Probe() {
    return <output data-testid="location">{useLocation().search}</output>
  }
  return <Probe />
}

describe('the search bar — spec §2', () => {
  const hit = (over: Partial<library.SeriesCard> = {}) =>
    series({ id: 7, title: '五等分の花嫁', ...over })

  beforeEach(() => {
    vi.useFakeTimers()
    // Call history and queued answers, not just the resolved value: each
    // test counts its own searches, and an answer queued `Once` that an
    // earlier test never asked for must not answer this one's.
    vi.mocked(library.search).mockReset()
    vi.mocked(library.search).mockResolvedValue({
      series: [hit()],
      unfiled: [],
      query: '花嫁',
    })
    vi.mocked(library.home).mockResolvedValue({
      continuing: [work()],
      added: [],
      read: [],
    })
    vi.mocked(library.grid).mockResolvedValue({
      series: [series()],
      unfiled: [],
    })
  })

  afterEach(() => {
    vi.useRealTimers()
  })

  /**
   * The view at `path` under `status`, with the URL drawn beside it and
   * `after` after that. A `rerender` of it with another status is the
   * shell's fetch landing under a view already drawn: `MemoryRouter` reads
   * `initialEntries` once, so the URL stays where the box put it.
   */
  const tree = (
    path: string,
    status: LibraryStatus | typeof STATUS_FAILED | undefined,
    after?: React.ReactNode,
  ) => (
    <MemoryRouter initialEntries={[path]}>
      <StatusContext.Provider value={status}>
        <Library />
        {locationProbe()}
        {after}
      </StatusContext.Provider>
    </MemoryRouter>
  )

  /** Wait out the debounce, and let the loads and the search settle. */
  const settle = () =>
    act(async () => {
      vi.advanceTimersByTime(300)
      await Promise.resolve()
      await Promise.resolve()
    })

  /** Mount at `path`; `'in flight'` is the shell's status not yet landed. */
  const viewAt = async (
    path: string,
    status: LibraryStatus | 'in flight' = twoLibraries,
  ) => {
    let host!: HTMLElement
    await act(async () => {
      host = mount(tree(path, status === 'in flight' ? undefined : status))
    })
    await settle()
    return host
  }

  const boxOf = (host: HTMLElement) =>
    host.querySelector<HTMLInputElement>('input[type="search"]')

  /**
   * Set the box's value and fire `input`, without waiting out the debounce.
   * The native setter, not `box.value = q`: React tracks the value of a
   * controlled input, so a plain assignment leaves the `input` event finding
   * nothing changed and `onChange` never runs.
   */
  async function setValue(host: HTMLElement, q: string) {
    const box = boxOf(host)
    if (!box) throw new Error('no search box')
    await act(async () => {
      Object.getOwnPropertyDescriptor(
        HTMLInputElement.prototype,
        'value',
      )?.set?.call(box, q)
      box.dispatchEvent(new Event('input', { bubbles: true }))
    })
  }

  /** Type into the box and let the debounce and the search settle. */
  async function type(host: HTMLElement, q: string) {
    await setValue(host, q)
    await settle()
  }

  /** The query string the box has put in the URL, as a plain object. */
  const urlOf = (host: HTMLElement) =>
    Object.fromEntries(
      new URLSearchParams(
        host.querySelector('[data-testid="location"]')?.textContent ?? '',
      ),
    )

  it('searches what was typed, once the typing stops', async () => {
    const host = await viewAt('/')
    await type(host, '花嫁')
    expect(vi.mocked(library.search)).toHaveBeenCalledTimes(1)
    expect(vi.mocked(library.search)).toHaveBeenCalledWith('花嫁', undefined)
    expect(host.querySelector('.search-hits .grid .title')?.textContent).toBe(
      '五等分の花嫁',
    )
  })

  it('runs the query in the URL on arrival, so a search can be a link', async () => {
    const host = await viewAt('/?q=%E8%8A%B1%E5%AB%81')
    expect(vi.mocked(library.search)).toHaveBeenCalledWith('花嫁', undefined)
    expect(boxOf(host)?.value).toBe('花嫁')
  })

  it('names what was searched when nothing matched', async () => {
    vi.mocked(library.search).mockResolvedValue({
      series: [],
      unfiled: [],
      query: '花婿',
    })
    const host = await viewAt('/')
    await type(host, '花婿')
    expect(host.querySelector('.search-hits .empty')?.textContent).toContain(
      '花婿',
    )
  })

  it('says so when the search fails', async () => {
    vi.mocked(library.search).mockRejectedValue(
      new Error('/api/library/search: 500'),
    )
    const host = await viewAt('/')
    await type(host, '花嫁')
    expect(
      host.querySelector('.search-hits [role="alert"]')?.textContent,
    ).toContain('500')
  })

  it('draws no answer that lands after the box was cleared, not even under the next query', async () => {
    const stale = deferred<library.SearchHits>()
    const next = deferred<library.SearchHits>()
    vi.mocked(library.search)
      .mockReturnValueOnce(stale.promise)
      .mockReturnValueOnce(next.promise)
    const host = await viewAt('/')
    await type(host, '花嫁')
    expect(vi.mocked(library.search)).toHaveBeenCalledTimes(1)
    await setValue(host, '')
    await flush(() =>
      stale.resolve({ series: [hit()], unfiled: [], query: '花嫁' }),
    )
    expect(host.querySelector('.search-hits')).toBeNull()
    expect(boxOf(host)?.value).toBe('')
    // A new query whose own answer is still out: with the box empty the
    // hits are not drawn at all, so only here could the stale answer show.
    await type(host, '花')
    expect(vi.mocked(library.search)).toHaveBeenCalledTimes(2)
    expect(host.querySelector('.search-hits')).not.toBeNull()
    expect(
      [...host.querySelectorAll('.search-hits .title')].map(
        (title) => title.textContent,
      ),
    ).toEqual([])
  })

  it('shows what ?q= holds when something other than the box changes it: Back, or a link', async () => {
    // Declared here for the reason `locationProbe` gives.
    function Back() {
      const navigate = useNavigate()
      return (
        <button type="button" onClick={() => void navigate(-1)}>
          Back
        </button>
      )
    }
    let host!: HTMLElement
    await act(async () => {
      host = mount(
        <MemoryRouter initialEntries={['/?q=old', '/?q=new']} initialIndex={1}>
          <StatusContext.Provider value={twoLibraries}>
            <Back />
            <Link to="/">every library</Link>
            <Library />
          </StatusContext.Provider>
        </MemoryRouter>,
      )
    })
    await settle()
    const box = boxOf(host)
    expect(box?.value).toBe('new')
    await act(async () => {
      host.querySelector<HTMLButtonElement>('button')?.click()
    })
    await settle()
    // The same box: the view did not remount, which would have drawn the
    // URL's value whatever the box did with it.
    expect(boxOf(host)).toBe(box)
    expect(box?.value).toBe('old')
    await act(async () => {
      host.querySelector<HTMLAnchorElement>('a[href="/"]')?.click()
    })
    await settle()
    expect(boxOf(host)).toBe(box)
    expect(box?.value).toBe('')
  })

  it('searches the library in the URL on arrival', async () => {
    await viewAt('/?q=Series&library=2')
    // Every call, not the last: one search, narrowed, and none before it of
    // another scope.
    expect(vi.mocked(library.search).mock.calls).toEqual([['Series', 2]])
  })

  it('links a hit to its series as the library searched holds it, and a hit of every library to the whole series — spec §3.6', async () => {
    const hits = (host: HTMLElement) =>
      [...host.querySelectorAll('.search-hits a[href^="/series/"]')].map((a) =>
        a.getAttribute('href'),
      )
    expect(hits(await viewAt('/?q=Series&library=2'))).toEqual([
      '/series/7?library=2',
    ])
    expect(hits(await viewAt('/?q=Series'))).toEqual(['/series/7'])
  })

  it('searches a link’s library once, when the status lands after the search has gone', async () => {
    const host = await viewAt('/?q=Series&library=2', 'in flight')
    expect(vi.mocked(library.search).mock.calls).toEqual([['Series', 2]])
    await act(async () => {
      rerender(tree('/?q=Series&library=2', twoLibraries))
    })
    await settle()
    expect(vi.mocked(library.search).mock.calls).toEqual([['Series', 2]])
    expect(urlOf(host)).toEqual({ q: 'Series', library: '2' })
  })

  it('keeps the library in the URL while the box is typed in', async () => {
    const host = await viewAt('/?q=Ser&library=2')
    await type(host, 'Series')
    expect(urlOf(host)).toEqual({ q: 'Series', library: '2' })
    expect(vi.mocked(library.search)).toHaveBeenLastCalledWith('Series', 2)
  })

  it('keeps its tab’s name while the box is typed in — O173', async () => {
    document.title = 'stale'
    const host = await viewAt('/')
    expect(document.title).toBe('Library — taiyaku')
    await type(host, 'one')
    expect(document.title).toBe('Library — taiyaku')
  })

  it('puts the hits in place of the grid and hides the tabs, the sort and the continue strip, and clearing the box brings them back', async () => {
    const host = await viewAt('/')
    const drawn = () => ({
      continue: host.querySelector('[data-testid="continue"]') !== null,
      tabs: host.querySelector('[role="group"][aria-label="Show"]') !== null,
      sort: host.querySelector('button.sort') !== null,
      grid: [...host.querySelectorAll('.grid .title')].map(
        (title) => title.textContent,
      ),
    })
    const browsing = {
      continue: true,
      tabs: true,
      sort: true,
      grid: ['A Series'],
    }
    expect(drawn()).toEqual(browsing)
    await type(host, '花嫁')
    expect(drawn()).toEqual({
      continue: false,
      tabs: false,
      sort: false,
      grid: ['五等分の花嫁'],
    })
    await type(host, '')
    expect(drawn()).toEqual(browsing)
  })

  for (const [path, status, placeholder, scope] of [
    ['/', twoLibraries, 'Search every library', undefined],
    ['/?library=2', twoLibraries, 'Search Comics', 2],
    ['/?library=2', 'in flight', 'Search this library', 2],
  ] as const) {
    it(`searches the library ${path} shows, and its placeholder says “${placeholder}”`, async () => {
      const host = await viewAt(path, status)
      expect(boxOf(host)?.placeholder).toBe(placeholder)
      await type(host, 'Series')
      expect(vi.mocked(library.search).mock.calls).toEqual([['Series', scope]])
    })
  }

  // The box follows what the grid shows: where the grid says there is no
  // such library, there is no scope to search and the empty state says why.
  // A value that is no id says so before the status lands, so it is held
  // with the status in flight as well.
  for (const [path, status] of [
    ['/?library=7', twoLibraries],
    ['/?q=Series&library=7', twoLibraries],
    ['/?library=abc', twoLibraries],
    ['/?q=Series&library=abc', twoLibraries],
    ['/?q=Series&library=abc', 'in flight'],
  ] as const) {
    const when = status === 'in flight' ? ', the status in flight' : ''
    it(`offers no box at ${path}${when}, and says there is no such library`, async () => {
      // What the server answers for a library outside the grant: nothing.
      vi.mocked(library.grid).mockResolvedValue({ series: [], unfiled: [] })
      vi.mocked(library.home).mockResolvedValue({
        continuing: [],
        added: [],
        read: [],
      })
      const host = await viewAt(path, status)
      expect(boxOf(host)).toBeNull()
      expect(host.querySelector('.empty h2')?.textContent).toBe(
        'No such library',
      )
      expect(vi.mocked(library.search)).not.toHaveBeenCalled()
    })
  }

  // While the status is loading the grid names no empty state and draws
  // only the rows it has, so the box comes with the rows or with the
  // status: never drawn and then taken away.
  const loads: [
    string,
    string,
    LibraryStatus,
    library.SeriesCard[],
    boolean,
  ][] = [
    ['an id the status does not list', '/?library=7', twoLibraries, [], false],
    [
      'every library, for an account that sees none',
      '/',
      { libraries: [], restricted: true },
      [],
      false,
    ],
    [
      'a library the status lists',
      '/?library=2',
      twoLibraries,
      [series()],
      true,
    ],
  ]
  for (const [what, path, status, rows, offered] of loads) {
    it(`draws the box for ${what} only once it stays, as the grid and then the status land`, async () => {
      const grid = deferred<library.Grid>()
      vi.mocked(library.grid).mockReturnValue(grid.promise)
      vi.mocked(library.home).mockResolvedValue({
        continuing: [],
        added: [],
        read: [],
      })
      const host = await viewAt(path, 'in flight')
      const drawn = [boxOf(host) !== null]
      await flush(() => grid.resolve({ series: rows, unfiled: [] }))
      drawn.push(boxOf(host) !== null)
      await act(async () => {
        rerender(tree(path, status))
      })
      await settle()
      drawn.push(boxOf(host) !== null)
      expect(drawn).toEqual(
        offered ? [false, true, true] : [false, false, false],
      )
    })
  }

  it('offers an account that sees no library no box, and says none is shared — O221', async () => {
    // What the server answers such an account: nothing.
    vi.mocked(library.grid).mockResolvedValue({ series: [], unfiled: [] })
    vi.mocked(library.home).mockResolvedValue({
      continuing: [],
      added: [],
      read: [],
    })
    const host = await viewAt('/?q=Series', { libraries: [], restricted: true })
    expect(boxOf(host)).toBeNull()
    expect(host.querySelector('.empty h2')?.textContent).toBe(
      'No library shared',
    )
    expect(vi.mocked(library.search)).not.toHaveBeenCalled()
  })

  it('offers no box on a server with no library, where the grid says none is made yet', async () => {
    vi.mocked(library.grid).mockResolvedValue({ series: [], unfiled: [] })
    vi.mocked(library.home).mockResolvedValue({
      continuing: [],
      added: [],
      read: [],
    })
    const host = await viewAt('/?q=Series', {
      libraries: [],
      restricted: false,
    })
    expect(boxOf(host)).toBeNull()
    expect(host.querySelector('.empty h2')?.textContent).toBe('No library yet')
    expect(vi.mocked(library.search)).not.toHaveBeenCalled()
  })

  // The query runs only where the box is offered, so a status still out
  // past the debounce sends nothing that its answer could then contradict.
  for (const [what, path, status, state] of [
    [
      'an id the status does not list',
      '/?q=Series&library=7',
      twoLibraries,
      'No such library',
    ],
    [
      'an account that sees no library',
      '/?q=Series',
      { libraries: [], restricted: true },
      'No library shared',
    ],
  ] as [string, string, LibraryStatus, string][]) {
    it(`asks nothing for ${what} while the status is out, and then says ${state}`, async () => {
      vi.mocked(library.search).mockResolvedValue({
        series: [],
        unfiled: [],
        query: 'Series',
      })
      vi.mocked(library.grid).mockResolvedValue({ series: [], unfiled: [] })
      vi.mocked(library.home).mockResolvedValue({
        continuing: [],
        added: [],
        read: [],
      })
      const host = await viewAt(path, 'in flight')
      await settle()
      expect(vi.mocked(library.search)).not.toHaveBeenCalled()
      // No empty state of any kind yet: neither "Nothing matched" nor the
      // one the status will name.
      expect(host.querySelector('.empty')).toBeNull()
      await act(async () => {
        rerender(tree(path, status))
      })
      await settle()
      expect(vi.mocked(library.search)).not.toHaveBeenCalled()
      expect(host.querySelector('.empty h2')?.textContent).toBe(state)
    })
  }

  it('stays in search mode while the status is out, and sends the query once it lands', async () => {
    // A grid with no rows, so only the status can offer the box.
    vi.mocked(library.grid).mockResolvedValue({ series: [], unfiled: [] })
    let host!: HTMLElement
    await act(async () => {
      host = mount(tree('/?q=Series', undefined))
    })
    const drawn = () => ({
      continue: host.querySelector('[data-testid="continue"]') !== null,
      tabs: host.querySelector('[role="group"][aria-label="Show"]') !== null,
      sort: host.querySelector('button.sort') !== null,
      hits: [...host.querySelectorAll('.search-hits .title')].map(
        (title) => title.textContent,
      ),
      searches: vi.mocked(library.search).mock.calls.length,
    })
    const seen = [drawn()]
    await settle()
    seen.push(drawn())
    await act(async () => {
      rerender(tree('/?q=Series', twoLibraries))
    })
    await settle()
    seen.push(drawn())
    const waiting = { continue: false, tabs: false, sort: false, hits: [] }
    expect(seen).toEqual([
      { ...waiting, searches: 0 },
      { ...waiting, searches: 0 },
      { ...waiting, hits: ['五等分の花嫁'], searches: 1 },
    ])
    expect(host.querySelector('.search-hits')).not.toBeNull()
  })

  /**
   * **The hits are read through the store** (spec
   * `2026-10-28-loading-library-design.md` §6.2): an arrival draws what is kept
   * at once and reads it again, or reads at once with nothing kept; typing
   * keeps its delay and the hits drawn.
   */
  describe('the hits, read through the store', () => {
    /**
     * What the view draws where the hits go: `null` with no hits area, and
     * otherwise its wait line by its words and each hit by its title.
     */
    const hitsDrawn = (host: HTMLElement) => {
      const area = host.querySelector('.search-hits')
      return JSON.stringify(
        area === null
          ? null
          : {
              waiting: [...area.querySelectorAll('[role="status"]')].map(
                (line) => line.textContent,
              ),
              hits: [...area.querySelectorAll('.title')].map(
                (title) => title.textContent,
              ),
            },
      )
    }
    const showing = (...hits: string[]) => JSON.stringify({ waiting: [], hits })
    const empty = JSON.stringify({ waiting: [], hits: [] })
    const loading = JSON.stringify({ waiting: ['Loading…'], hits: [] })

    const answer = (title: string): library.SearchHits => ({
      series: [hit({ title })],
      unfiled: [],
      query: title,
    })

    it('sends the query an arrival names at once, not after the typing delay', async () => {
      let host!: HTMLElement
      await act(async () => {
        host = mount(tree('/?q=%E8%8A%B1%E5%AB%81', twoLibraries))
      })
      await land()
      expect(vi.mocked(library.search).mock.calls).toEqual([
        ['花嫁', undefined],
      ])
      expect(hitsDrawn(host)).toBe(showing('五等分の花嫁'))
    })

    it('sends at once the query Back returns to, though the box set it, and the one a link names', async () => {
      function Back() {
        const navigate = useNavigate()
        return (
          <button
            type="button"
            data-testid="back"
            onClick={() => void navigate(-1)}
          >
            Back
          </button>
        )
      }
      vi.mocked(library.search).mockImplementation(async (q) => answer(q))
      let host!: HTMLElement
      await act(async () => {
        host = mount(
          <MemoryRouter
            initialEntries={[
              // What the box leaves on an entry it typed into: its tag.
              { pathname: '/', search: '?q=old', state: { searchBox: 'x:1' } },
              '/',
            ]}
            initialIndex={1}
          >
            <StatusContext.Provider value={twoLibraries}>
              <Back />
              <Link to="/?q=linked">linked</Link>
              <Library />
            </StatusContext.Provider>
          </MemoryRouter>,
        )
      })
      await land()
      expect(vi.mocked(library.search)).not.toHaveBeenCalled()
      await act(async () => {
        host.querySelector<HTMLButtonElement>('[data-testid="back"]')?.click()
      })
      await land()
      expect(vi.mocked(library.search).mock.calls).toEqual([['old', undefined]])
      expect(hitsDrawn(host)).toBe(showing('old'))
      await act(async () => {
        host.querySelector<HTMLAnchorElement>('a[href="/?q=linked"]')?.click()
      })
      await land()
      expect(vi.mocked(library.search).mock.calls).toEqual([
        ['old', undefined],
        ['linked', undefined],
      ])
      expect(hitsDrawn(host)).toBe(showing('linked'))
    })

    it('draws the hits kept for a search in its first render on a return, and reads them again once', async () => {
      vi.mocked(library.search).mockResolvedValue(answer('kept'))
      let host!: HTMLElement
      await act(async () => {
        host = mount(tree('/?q=kept', twoLibraries))
      })
      await land()
      expect(hitsDrawn(host)).toBe(showing('kept'))

      rerender(<div />)
      vi.mocked(library.search).mockClear()
      const again = deferred<library.SearchHits>()
      vi.mocked(library.search).mockReturnValueOnce(again.promise)
      const seen = watchDrawn(() => hitsDrawn(host))
      const first: { drawn?: string } = {}
      await act(async () => {
        rerender(
          tree(
            '/?q=kept',
            twoLibraries,
            <FirstCommit read={() => hitsDrawn(host)} into={first} />,
          ),
        )
      })
      await land()
      expect(first.drawn).toBe(showing('kept'))
      expect(vi.mocked(library.search).mock.calls).toEqual([
        ['kept', undefined],
      ])
      seen.stop()
      expect([...seen.seen]).toEqual(['null', showing('kept')])
      await flush(() => again.resolve(answer('kept, read again')))
      await land()
      expect(hitsDrawn(host)).toBe(showing('kept, read again'))
    })

    it('draws Loading… where the hits go from 200 ms on an arrival with none kept, and the hits once they land', async () => {
      const found = deferred<library.SearchHits>()
      vi.mocked(library.search).mockReturnValueOnce(found.promise)
      let host!: HTMLElement
      await act(async () => {
        host = mount(tree('/?q=new', twoLibraries))
      })
      await land()
      expect(vi.mocked(library.search).mock.calls).toEqual([['new', undefined]])
      expect(hitsDrawn(host)).toBe(empty)
      pass(WAIT_DRAWN_AFTER_MS - 1)
      expect(hitsDrawn(host)).toBe(empty)
      pass(1)
      expect(hitsDrawn(host)).toBe(loading)
      await flush(() => found.resolve(answer('new')))
      await land()
      expect(hitsDrawn(host)).toBe(showing('new'))
    })

    it('keeps the hits drawn while the next typed query waits and is out, with no Loading…, and draws its hits in one step', async () => {
      vi.mocked(library.search).mockResolvedValueOnce(answer('fo'))
      const host = await viewAt('/')
      await type(host, 'fo')
      await land()
      expect(hitsDrawn(host)).toBe(showing('fo'))

      const foo = deferred<library.SearchHits>()
      vi.mocked(library.search).mockReturnValueOnce(foo.promise)
      const seen = watchDrawn(() => hitsDrawn(host))
      await setValue(host, 'foo')
      await land()
      // Typing waits out its delay before it sends.
      expect(vi.mocked(library.search)).toHaveBeenCalledTimes(1)
      pass(250)
      await land()
      expect(vi.mocked(library.search)).toHaveBeenCalledTimes(2)
      expect(vi.mocked(library.search)).toHaveBeenLastCalledWith(
        'foo',
        undefined,
      )
      pass(WAIT_DRAWN_AFTER_MS * 5)
      expect(hitsDrawn(host)).toBe(showing('fo'))
      await flush(() => foo.resolve(answer('foo')))
      await land()
      seen.stop()
      expect([...seen.seen]).toEqual([showing('fo'), showing('foo')])
    })

    it('sends the query an arrival names once the box is offered, at once, and nothing before', async () => {
      const rows = deferred<library.Grid>()
      vi.mocked(library.grid).mockReturnValue(rows.promise)
      let host!: HTMLElement
      await act(async () => {
        host = mount(tree('/?q=Series', undefined))
      })
      await land()
      pass(1000)
      await land()
      expect(boxOf(host)).toBeNull()
      expect(vi.mocked(library.search)).not.toHaveBeenCalled()
      // Rows offer the box with the status still out.
      await flush(() => rows.resolve({ series: [series()], unfiled: [] }))
      await land()
      expect(boxOf(host)).not.toBeNull()
      expect(vi.mocked(library.search).mock.calls).toEqual([
        ['Series', undefined],
      ])
    })

    // An arrival waits for the box to be offered, by rows or by the status,
    // and then for its search: the wait line counts from the arrival, and
    // stays drawn as the query is sent.
    for (const by of ['rows', 'the status'] as const) {
      it(`draws Loading… from 200 ms while the box waits for ${by}, sends nothing until it is offered, and keeps the line until the hits land`, async () => {
        const rows = deferred<library.Grid>()
        vi.mocked(library.grid).mockReturnValue(
          by === 'rows'
            ? rows.promise
            : Promise.resolve({ series: [], unfiled: [] }),
        )
        const found = deferred<library.SearchHits>()
        vi.mocked(library.search).mockReturnValueOnce(found.promise)
        let host!: HTMLElement
        await act(async () => {
          host = mount(tree('/?q=Series', undefined))
        })
        await land()
        expect(hitsDrawn(host)).toBe(empty)
        pass(WAIT_DRAWN_AFTER_MS - 1)
        expect(hitsDrawn(host)).toBe(empty)
        pass(1)
        expect(hitsDrawn(host)).toBe(loading)
        expect(vi.mocked(library.search)).not.toHaveBeenCalled()

        const seen = watchDrawn(() => hitsDrawn(host))
        if (by === 'rows') {
          await flush(() => rows.resolve({ series: [series()], unfiled: [] }))
        } else {
          await act(async () => {
            rerender(tree('/?q=Series', twoLibraries))
          })
        }
        await land()
        expect(vi.mocked(library.search).mock.calls).toEqual([
          ['Series', undefined],
        ])
        expect(hitsDrawn(host)).toBe(loading)
        await flush(() => found.resolve(answer('Series')))
        await land()
        seen.stop()
        expect([...seen.seen]).toEqual([loading, showing('Series')])
      })
    }

    /** Each alert and each wait line the whole view draws, by its words. */
    const said = (host: HTMLElement) => ({
      alerts: [...host.querySelectorAll('[role="alert"]')].map(
        (alert) => alert.textContent,
      ),
      waiting: [...host.querySelectorAll('[role="status"]')].map(
        (line) => line.textContent,
      ),
    })

    it('stops waiting for the box once the grid’s read fails, and says that failure alone', async () => {
      const rows = deferred<library.Grid>()
      vi.mocked(library.grid).mockReturnValue(rows.promise)
      let host!: HTMLElement
      await act(async () => {
        host = mount(tree('/?q=Series', undefined))
      })
      await land()
      pass(WAIT_DRAWN_AFTER_MS)
      expect(said(host)).toEqual({ alerts: [], waiting: ['Loading…'] })
      await flush(() => rows.reject(new Error('grid: 500')))
      await land()
      pass(1000)
      expect(said(host)).toEqual({ alerts: ['grid: 500'], waiting: [] })
      expect(vi.mocked(library.search)).not.toHaveBeenCalled()
    })

    it('stops waiting for the box once the status read fails with no status kept and the grid has no rows', async () => {
      vi.mocked(library.grid).mockResolvedValue({ series: [], unfiled: [] })
      let host!: HTMLElement
      await act(async () => {
        host = mount(tree('/?q=Series', undefined))
      })
      await land()
      pass(WAIT_DRAWN_AFTER_MS)
      expect(said(host)).toEqual({ alerts: [], waiting: ['Loading…'] })
      await act(async () => {
        rerender(tree('/?q=Series', STATUS_FAILED))
      })
      await land()
      pass(1000)
      // The shell says the status's failure above the screen.
      expect(said(host)).toEqual({ alerts: [], waiting: [] })
      expect(vi.mocked(library.search)).not.toHaveBeenCalled()
    })

    it('draws the alert alone, with no hits, where a typed query fails with nothing kept for it', async () => {
      vi.mocked(library.search)
        .mockResolvedValueOnce(answer('fo'))
        .mockRejectedValueOnce(new Error('/api/library/search: 500'))
      const host = await viewAt('/')
      await type(host, 'fo')
      await land()
      expect(hitsDrawn(host)).toBe(showing('fo'))
      await type(host, 'foo')
      await land()
      // Long past the wait line's delay: a failure is no wait.
      pass(WAIT_DRAWN_AFTER_MS * 5)
      const area = host.querySelector('.search-hits')
      expect(
        [...(area?.querySelectorAll('[role="alert"]') ?? [])].map(
          (alert) => alert.textContent,
        ),
      ).toEqual(['/api/library/search: 500'])
      expect(
        [...(area?.querySelectorAll('.title') ?? [])].map(
          (title) => title.textContent,
        ),
      ).toEqual([])
      expect(
        [...(area?.querySelectorAll('[role="status"]') ?? [])].map(
          (line) => line.textContent,
        ),
      ).toEqual([])
    })

    it('reads the library Back returns to under the same words, and draws its hits, not those of the library left', async () => {
      function Back() {
        const navigate = useNavigate()
        return (
          <button
            type="button"
            data-testid="back"
            onClick={() => void navigate(-1)}
          >
            Back
          </button>
        )
      }
      vi.mocked(library.search).mockImplementation(async (q, scope) =>
        answer(`${q} in ${scope ?? 'every library'}`),
      )
      let host!: HTMLElement
      await act(async () => {
        host = mount(
          <MemoryRouter
            initialEntries={[
              '/?q=foo',
              // Library 2's view, where the box typed `foo`.
              {
                pathname: '/',
                search: '?library=2&q=foo',
                state: { searchBox: 'x:1' },
              },
            ]}
            initialIndex={1}
          >
            <StatusContext.Provider value={twoLibraries}>
              <Back />
              <Library />
            </StatusContext.Provider>
          </MemoryRouter>,
        )
      })
      await land()
      expect(hitsDrawn(host)).toBe(showing('foo in 2'))
      await act(async () => {
        host.querySelector<HTMLButtonElement>('[data-testid="back"]')?.click()
      })
      await land()
      expect(vi.mocked(library.search).mock.calls).toEqual([
        ['foo', 2],
        ['foo', undefined],
      ])
      expect(hitsDrawn(host)).toBe(showing('foo in every library'))
    })

    it('sends at once a query set by a replace that carries no tag of the box’s', async () => {
      function Replace() {
        const navigate = useNavigate()
        return (
          <button
            type="button"
            data-testid="replace"
            onClick={() => void navigate('/?q=x', { replace: true })}
          >
            Replace
          </button>
        )
      }
      let host!: HTMLElement
      await act(async () => {
        host = mount(
          <MemoryRouter initialEntries={['/']}>
            <StatusContext.Provider value={twoLibraries}>
              <Replace />
              <Library />
            </StatusContext.Provider>
          </MemoryRouter>,
        )
      })
      await land()
      expect(vi.mocked(library.search)).not.toHaveBeenCalled()
      await act(async () => {
        host
          .querySelector<HTMLButtonElement>('[data-testid="replace"]')
          ?.click()
      })
      await land()
      expect(vi.mocked(library.search).mock.calls).toEqual([['x', undefined]])
    })
  })
})
