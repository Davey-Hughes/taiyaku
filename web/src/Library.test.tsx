import { act } from 'react'
import { Link, MemoryRouter, useLocation, useNavigate } from 'react-router'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { Me } from './api'
import { BrowseControlsContext } from './browseModule'
import { GateContext } from './gate'
import * as library from './library'
import type { LibraryStatus } from './library'
import { Library } from './Library'
import { StatusContext } from './shell'
import { Sidebar } from './Sidebar'
import { deferred, flush } from './testing/async'
import { mount, rerender } from './testing/mount'

vi.mock('./library', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./library')>()),
  home: vi.fn(),
  grid: vi.fn(),
  search: vi.fn(),
}))

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
  cover: '/api/library/series/1/cover',
  ...over,
})

const work = (over: Partial<library.WorkCard> = {}): library.WorkCard => ({
  id: 1,
  slug: 'a-series-v01',
  title: 'A Series v01',
  volume: 1,
  aligned: true,
  pages: 180,
  editions: [{ key: 'raw', label: 'Raw', members: 1, pages: 180, missing: 0 }],
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
 * Mount at `path` and let the two loads settle. A library's page is
 * `/?library=N`.
 *
 * Two flushes, not one: the effect fires a `Promise.all` over two mocked
 * promises and *then* sets state, so a single microtask turn lands between the
 * resolution and the re-render and the assertions run against a screen that has
 * not drawn yet.
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
            { key: 'raw', label: 'Raw', members: 1, pages: 180, missing: 2 },
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
            { key: 'raw', label: 'Raw', members: 1, pages: 180, missing: 2 },
            { key: 'en', label: 'En', members: 1, pages: 178, missing: 0 },
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
            { key: 'raw', label: 'Raw', members: 1, pages: 180, missing: 2 },
            { key: 'en', label: 'En', members: 1, pages: 178, missing: 0 },
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
            { key: 'raw', label: 'Raw', members: 1, pages: 180, missing: 2 },
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
            { key: 'raw', label: 'Raw', members: 1, pages: 180, missing: 2 },
            { key: 'en', label: 'En', members: 1, pages: 178, missing: 0 },
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
    // Only `grid` is made to fail — `Promise.all` rejects on the first
    // rejection regardless of the other, which is the same shape a real
    // failure takes. `mockRejectedValueOnce` overrides the `beforeEach`
    // default for exactly one call; the retry below falls through to that
    // default (`{ series: [], unfiled: [] }`), which is enough to prove the
    // *second* load succeeded — the failing banner's own text is the pin.
    vi.mocked(library.grid).mockRejectedValueOnce(new Error('offline'))

    // The effect re-fires on `library` alone (`Library.tsx`'s own dependency
    // array), and nothing inside `Library` changes it here. A sibling link
    // inside the same `MemoryRouter`, changing only the `?library=` query
    // parameter `Library` reads via `useSearchParams`, is what stands in for
    // "the reader retried" without inventing a retry control this task did
    // not ask for.
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
   * The effect re-fires on `library`, so switching library while the first
   * load is still in flight leaves two `Promise.all`s outstanding, and the
   * store need not answer them in order. The cleanup's `cancelled = true`
   * drops the *departed* library's answer when it lands late, which would
   * otherwise replace the answer for the library on screen. While the next
   * library's own load is in flight, and after it fails, the page draws only
   * what was loaded for that library: nothing of the one it left.
   *
   * The tests hold a load open with `deferred` rather than ordering two
   * `mockResolvedValueOnce`s: mocked promises resolve in call order, so the
   * guard is never true and the branch is never taken.
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
        '/api/library/series/1/cover?w=256',
      )
      // O133: the browser picks the size for its screen's density — 768 px on
      // a 3× phone, where the 160 px filmstrip tile used to be upscaled.
      expect(host.querySelector('img')?.getAttribute('srcset')).toBe(
        '/api/library/series/1/cover?w=256 1x, /api/library/series/1/cover?w=512 2x, /api/library/series/1/cover?w=768 3x',
      )
      expect(host.querySelector('.cover-missing')).toBeNull()
    })
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
            { key: 'raw', label: 'Raw', members: 1, pages: 180, missing: 0 },
            { key: 'en', label: 'En', members: 1, pages: 178, missing: 0 },
          ],
        }),
      ],
      added: [],
      read: [],
    })
    const host = await show()
    const strip = host.querySelector('[data-testid="continue"]')
    expect(strip?.querySelector('img')?.getAttribute('src')).toBe(
      '/api/works/half-read-v02/en/pages/0/thumb?w=256',
    )
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
            { key: 'raw', label: 'Raw', members: 1, pages: 180, missing: 1 },
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
    // Call history, not just the resolved value: each test counts its own
    // searches.
    vi.mocked(library.search).mockClear()
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
   * The view at `path` under `status`, with the URL drawn beside it. A
   * `rerender` of it with another status is the shell's fetch landing under
   * a view already drawn: `MemoryRouter` reads `initialEntries` once, so the
   * URL stays where the box put it.
   */
  const tree = (path: string, status: LibraryStatus | undefined) => (
    <MemoryRouter initialEntries={[path]}>
      <StatusContext.Provider value={status}>
        <Library />
        {locationProbe()}
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
})
