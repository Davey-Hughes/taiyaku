import { describe, expect, it, vi } from 'vitest'
import { Unauthenticated } from './api'
import {
  grid,
  home,
  matchesChip,
  oneSeries,
  search,
  type SeriesCard,
  sorted,
  status,
  work,
} from './library'

const card = (over: Partial<SeriesCard> = {}): SeriesCard => ({
  id: 1,
  title: 'A Series',
  works: 3,
  editions: 3,
  aligned: 0,
  added: 10,
  unread: 3,
  cover: '/api/library/series/1/cover',
  ...over,
})

const json = (body: unknown) =>
  new Response(JSON.stringify(body), {
    status: 200,
    headers: { 'content-type': 'application/json' },
  })

describe('the browse client', () => {
  it('asks for the grid without a root, and with one', async () => {
    // A fresh `Response` per call, not `mockResolvedValue` with one shared
    // instance: `Response.json()` consumes its body stream, so the same
    // instance returned to two `getJson` calls throws "Body is unusable" on
    // the second read rather than exercising the second assertion.
    const fetchMock = vi
      .fn()
      .mockImplementation(() =>
        Promise.resolve(json({ series: [], unfiled: [] })),
      )
    vi.stubGlobal('fetch', fetchMock)

    await grid()
    expect(fetchMock.mock.calls[0]?.[0]).toBe('/api/library/series')
    await grid(7)
    expect(fetchMock.mock.calls[1]?.[0]).toBe('/api/library/series?root=7')
  })

  // `oneSeries` and `search` have no caller yet — Plan B's series and search
  // screens are what will reach them — so this is the only place either is
  // exercised at all. Same shape as `grid`'s own test above: the endpoint hit
  // and, for `search`, the escaping `work`'s test already pins for its own
  // path segment.
  it('asks for one series by id', async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValue(json({ series: card(), works: [] }))
    vi.stubGlobal('fetch', fetchMock)
    await oneSeries(7)
    expect(fetchMock.mock.calls[0]?.[0]).toBe('/api/library/series/7')
  })

  it('escapes a search query rather than pasting it into the URL', async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValue(json({ series: [], unfiled: [], query: 'a b' }))
    vi.stubGlobal('fetch', fetchMock)
    await search('a b')
    expect(fetchMock.mock.calls[0]?.[0]).toBe('/api/library/search?q=a%20b')
  })

  // `home`'s one caller, `Library.tsx`, is exercised only through
  // `Library.test.tsx`'s mock of this whole module (`vi.mock('./library', …
  // home: vi.fn())`) — the real implementation below is never reached from
  // there, or from anywhere else, which left it the one function in this
  // file coverage-floor still flagged once `oneSeries` and `search` had
  // their own tests.
  it('asks for the home lists', async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValue(json({ continuing: [], added: [], read: [] }))
    vi.stubGlobal('fetch', fetchMock)
    await home()
    expect(fetchMock.mock.calls[0]?.[0]).toBe('/api/library/home')
  })

  it('escapes a slug rather than pasting it into the path', async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValue(json({ id: 1, slug: 'a b', alignments: [] }))
    vi.stubGlobal('fetch', fetchMock)
    await work('a b')
    expect(fetchMock.mock.calls[0]?.[0]).toBe('/api/library/works/a%20b')
  })

  it('routes a 401 through the one place that notices it', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue(new Response(null, { status: 401 })),
    )
    await expect(status()).rejects.toBeInstanceOf(Unauthenticated)
  })

  it('reports a failing status by its code rather than parsing the body', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue(new Response('boom', { status: 500 })),
    )
    await expect(status()).rejects.toThrow(/500/)
  })

  it('multi-edition means some work here has a second edition', () => {
    expect(matchesChip(card({ works: 3, editions: 3 }), 'multi')).toBe(false)
    expect(matchesChip(card({ works: 3, editions: 4 }), 'multi')).toBe(true)
  })

  it('unread means nothing here has been started', () => {
    expect(matchesChip(card({ works: 3, unread: 3 }), 'unread')).toBe(true)
    expect(matchesChip(card({ works: 3, unread: 2 }), 'unread')).toBe(false)
  })

  it('all keeps everything', () => {
    expect(matchesChip(card({ works: 3, unread: 0 }), 'all')).toBe(true)
  })

  it('sorts by added, highest first, without mutating its input', () => {
    const input = [card({ id: 1, added: 5 }), card({ id: 2, added: 9 })]
    expect(sorted(input, 'added').map((s) => s.id)).toEqual([2, 1])
    expect(input.map((s) => s.id)).toEqual([1, 2])
  })

  it('sorts by title with a locale comparison rather than code points', () => {
    // `'beta'` vs `'Alpha'` sorts identically under `localeCompare` and under
    // plain code-point order (lowercase `a`-`z` all sort after uppercase
    // `A`-`Z` in both), so it cannot tell `a.title.localeCompare(b.title)`
    // apart from `a.title < b.title`. `'apple'` vs `'Banana'` can: code-point
    // order puts capital `B` (0x42) ahead of lowercase `a` (0x61), so
    // `'Banana' < 'apple'` — the wrong answer — while `localeCompare` gives
    // the alphabetical one, `'apple'` first.
    const input = [
      card({ id: 1, title: 'Banana' }),
      card({ id: 2, title: 'apple' }),
    ]
    expect(sorted(input, 'title').map((s) => s.id)).toEqual([2, 1])
  })
})
