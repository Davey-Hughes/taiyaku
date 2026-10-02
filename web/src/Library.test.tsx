import { act } from 'react'
import { Link, MemoryRouter } from 'react-router'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { Me } from './api'
import { GateContext } from './gate'
import * as library from './library'
import type { LibraryStatus } from './library'
import { Library } from './Library'
import { StatusContext } from './shell'
import { deferred, flush } from './testing/async'
import { mount } from './testing/mount'

vi.mock('./library', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./library')>()),
  home: vi.fn(),
  grid: vi.fn(),
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

const defaultStatus: LibraryStatus = { roots: [], scanned: true }

/**
 * Mount and let the two loads settle.
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
const show = async (who?: Me, status: LibraryStatus = defaultStatus) => {
  let host!: HTMLElement
  const screen = (
    <MemoryRouter>
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

  it('a library with no roots is told so, rather than shown an empty grid', async () => {
    const host = await show(undefined, { roots: [], scanned: false })
    expect(host.textContent).toContain('library scan --add')
  })

  it('a configured but unscanned library is told the other thing', async () => {
    const host = await show(undefined, {
      roots: [{ id: 1, name: 'Manga' }],
      scanned: false,
    })
    expect(host.textContent).not.toContain('--add')
    expect(host.textContent).toContain('library scan')
  })

  it('a scanned library with no series says nothing matched, not that it is unconfigured', async () => {
    const host = await show(undefined, {
      roots: [{ id: 1, name: 'Manga' }],
      scanned: true,
    })
    expect(host.textContent).not.toContain('library scan')
    // Positive half: the no-matches empty state actually rendered, rather
    // than nothing at all — the same shape `Routes.test.tsx`'s "does not
    // mount the reader anywhere else" pins with `.library`/`.sidebar` beside
    // its own negative assertion.
    expect(host.textContent).toContain('No series match this filter.')
  })

  it('a failed load followed by a successful retry clears the stale failure banner', async () => {
    // Only `grid` is made to fail — `Promise.all` rejects on the first
    // rejection regardless of the other two, which is the same shape a real
    // failure takes. `mockRejectedValueOnce` overrides the `beforeEach`
    // default for exactly one call; the retry below falls through to that
    // default (`{ series: [], unfiled: [] }`), which is enough to prove the
    // *second* load succeeded — the failing banner's own text is the pin.
    vi.mocked(library.grid).mockRejectedValueOnce(new Error('offline'))

    // The effect re-fires on `root` alone (`Library.tsx`'s own dependency
    // array), and nothing inside `Library` changes it here. A sibling link
    // inside the same `MemoryRouter`, changing only the `?root=` query
    // parameter `Library` reads via `useSearchParams`, is what stands in for
    // "the reader retried" without inventing a retry control this task did
    // not ask for.
    let host!: HTMLElement
    await act(async () => {
      host = mount(
        <MemoryRouter initialEntries={['/?root=1']}>
          <StatusContext.Provider value={defaultStatus}>
            <div>
              <Link to="/?root=2">retry</Link>
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
      host.querySelector<HTMLAnchorElement>('a[href="/?root=2"]')?.click()
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
   * **The two `cancelled` guards, which no test had reached.**
   *
   * The effect re-fires on `root`, so switching root while the first load is
   * still in flight leaves two `Promise.all`s outstanding — and the store need
   * not answer them in order. The cleanup's `cancelled = true` is what stops
   * the *departed* root's answer from being drawn under the new root's
   * heading, which is a grid of the wrong library showing rows the grant for
   * this root may not include.
   *
   * Both tests hold the first load open with `deferred` rather than ordering
   * two `mockResolvedValueOnce`s: mocked promises resolve in call order, so
   * the guard is never true and the branch is never taken.
   */
  describe('a load for a root that has since been left', () => {
    const twoRoots = async (first: Promise<library.Grid>) => {
      const second = deferred<library.Grid>()
      vi.mocked(library.grid)
        .mockReturnValueOnce(first)
        .mockReturnValueOnce(second.promise)
      let host!: HTMLElement
      await act(async () => {
        host = mount(
          <MemoryRouter initialEntries={['/?root=1']}>
            <StatusContext.Provider value={defaultStatus}>
              <div>
                <Link to="/?root=2">the other root</Link>
                <Library />
              </div>
            </StatusContext.Provider>
          </MemoryRouter>,
        )
      })
      await flush()
      await act(async () => {
        host.querySelector<HTMLAnchorElement>('a[href="/?root=2"]')?.click()
      })
      await flush(() =>
        second.resolve({
          series: [series({ id: 2, title: 'Two' })],
          unfiled: [],
        }),
      )
      expect(host.querySelector('ul.grid .title')?.textContent).toBe('Two')
      return host
    }

    it('is not drawn when it answers after the root was changed', async () => {
      const first = deferred<library.Grid>()
      const host = await twoRoots(first.promise)
      await flush(() =>
        first.resolve({
          series: [series({ id: 1, title: 'One' })],
          unfiled: [],
        }),
      )
      expect(host.textContent).not.toContain('One')
      expect(host.querySelector('ul.grid .title')?.textContent).toBe('Two')
    })

    it('does not put its failure over the root that loaded', async () => {
      const first = deferred<library.Grid>()
      const host = await twoRoots(first.promise)
      await flush(() => first.reject(new Error('root 1 is no longer mounted')))
      expect(host.querySelector('[role="alert"]')).toBeNull()
      expect(host.querySelector('ul.grid .title')?.textContent).toBe('Two')
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
    expect(host.firstElementChild).toBe(h1s[0])
  })

  it('names its tab Library — O173', async () => {
    document.title = 'stale'
    await show()
    expect(document.title).toBe('Library — taiyaku')
  })
})
