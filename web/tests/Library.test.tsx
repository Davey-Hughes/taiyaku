import { act } from 'react'
import {
  Link,
  MemoryRouter,
  type MemoryRouterProps,
  useLocation,
  useNavigate,
  useNavigationType,
} from 'react-router'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { Me } from '../src/api'
import { BrowseControlsContext } from '../src/browseModule'
import { GateContext } from '../src/gate'
import * as library from '../src/library'
import type { LibraryStatus } from '../src/library'
import { Library } from '../src/Library'
import { invalidateLibraryViews } from '../src/pageReads'
import { SEARCH_BOX_ID } from '../src/searchHits'
import { STATUS_FAILED, StatusContext } from '../src/shell'
import { Sidebar } from '../src/Sidebar'
import { deferred, flush, land } from './support/async'
import { FirstCommit } from './support/firstCommit'
import { mount, rerender } from './support/mount'
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
 * mocked.** `Shell` owns the `status()` fetch now — `Library` only reads the
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
    // I7 of the whole-branch review: spec C1 §3 puts the mark *on the
    // cover*, and `index.css` has exactly one rule for it —
    // `.cover-box .aligned` — so outside that box the mark loses its
    // positioning and its chip. `textContent` alone cannot tell "on the
    // cover" from "moved below the title"; only the ancestor can.
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
   * I1 of the whole-branch review: `SeriesGrid` draws `/`'s and `/search`'s
   * unfiled works, and its reader link carried none of O62's guard either.
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
   * I1 of the whole-branch review: O62's guard (spec C1 §8) reached `Table`
   * in `Series.tsx` and nowhere else. The Continue strip is named in the
   * finding as "the likeliest place in the product to meet a work whose
   * files just vanished, because the reader was mid-read when they went" —
   * so an unguarded `<Link>` here is the worst place for the gap, not an
   * incidental one.
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
    // already follows: withholding the link must not withhold the work.
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
    expect(comics.textContent).toContain('Not read yet')
    const manga = await showAt('/?library=1', status)
    expect(manga.textContent).toContain('No series match this filter.')
  })

  it('the page of every library is not read yet only while none is', async () => {
    const one = await show(undefined, {
      libraries: [
        { id: 1, name: 'Manga', scanned: false },
        { id: 2, name: 'Comics', scanned: true },
      ],
      restricted: false,
    })
    expect(one.textContent).toContain('No series match this filter.')
    const none = await show(undefined, {
      libraries: [{ id: 1, name: 'Manga', scanned: false }],
      restricted: false,
    })
    expect(none.textContent).toContain('Not read yet')
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
    expect(read.querySelector('.empty h2')?.textContent).toBe('Nothing here')
    expect(read.textContent).toContain('No series match this filter.')
    const unread = await showAt('/?library=1', {
      libraries: [{ id: 1, name: 'Manga', scanned: false }],
      restricted: true,
    })
    expect(unread.querySelector('.empty h2')?.textContent).toBe('Not read yet')
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
   * placeholder carrying the title, not a broken image." The plan built three
   * of the five and never mentioned this one.
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
      // O133: the browser picks the size for its screen's density — 768 px on
      // a 3× phone, where the 160 px filmstrip tile used to be upscaled.
      expect(host.querySelector('img')?.getAttribute('srcset')).toBe(
        '/api/works/a-series-v01/raw/pages/0/thumb?v=c0ffee-0-1&w=256 1x, /api/works/a-series-v01/raw/pages/0/thumb?v=c0ffee-0-1&w=512 2x, /api/works/a-series-v01/raw/pages/0/thumb?v=c0ffee-0-1&w=768 3x',
      )
      expect(host.querySelector('.cover-missing')).toBeNull()
    })
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

  it('holds Continue strip, Density and Reset, in sentence case', async () => {
    const panel = openView(await mountWithView())
    expect(
      [...panel.querySelectorAll('label')].map((label) => {
        const control = label.querySelector('input, select')
        return [
          label.querySelector('span')?.textContent,
          control instanceof HTMLInputElement ? control.type : control?.tagName,
          [...label.querySelectorAll('option')].map((o) => o.textContent),
        ]
      }),
    ).toEqual([
      ['Continue strip', 'checkbox', []],
      ['Density', 'SELECT', ['Compact', 'Comfortable']],
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
