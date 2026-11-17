import { describe, expect, it, vi } from 'vitest'
import { Unauthenticated } from '../src/api'
import {
  coverSizes,
  coverSources,
  type EditionCard,
  grid,
  home,
  libraryParam,
  matchesChip,
  oneSeries,
  readerPath,
  search,
  type SeriesCard,
  sorted,
  status,
  unreadable,
  withView,
  work,
  type WorkCard,
} from '../src/library'
import { PHONE_LAYOUT_PARTS } from '../src/phoneLayout'

const edition = (over: Partial<EditionCard> = {}): EditionCard => ({
  key: 'raw',
  label: 'Manga (RAW)',
  members: 1,
  pages: 180,
  missing: 0,
  cover: '/api/works/gotoubun-v01/raw/pages/0/thumb?v=c0ffee-0-1',
  ...over,
})

const workCard = (over: Partial<WorkCard> = {}): WorkCard => ({
  id: 1,
  slug: 'gotoubun-v01',
  title: 'Go-Toubun no Hanayome v01',
  volume: 1,
  aligned: false,
  pages: 180,
  editions: [edition()],
  opens: 'raw',
  ...over,
})

const card = (over: Partial<SeriesCard> = {}): SeriesCard => ({
  id: 1,
  title: 'A Series',
  works: 3,
  editions: 3,
  aligned: 0,
  added: 10,
  unread: 3,
  cover: '/api/works/a-series-v01/raw/pages/0/thumb?v=c0ffee-0-1',
  ...over,
})

const json = (body: unknown) =>
  new Response(JSON.stringify(body), {
    status: 200,
    headers: { 'content-type': 'application/json' },
  })

describe('the browse client', () => {
  it('asks for the grid without a library, and with one', async () => {
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
    expect(fetchMock.mock.calls[1]?.[0]).toBe('/api/library/series?library=7')
  })

  // `oneSeries` and `search` are mocked in the tests of their callers,
  // `Series.test.tsx`, `searchHits.test.tsx` and `Library.test.tsx`, so the
  // requests they make are pinned here. Same shape as `grid`'s own test
  // above: the endpoint hit and, for `search`, the escaping `work`'s test
  // already pins for its own path segment.
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

  // The search path already carries a query, so the library joins it with
  // `&`: a second `?` would make it part of `q`.
  it('narrows a search to a library, after the query', async () => {
    const fetchMock = vi
      .fn()
      .mockImplementation(() =>
        Promise.resolve(json({ series: [], unfiled: [], query: 'a b' })),
      )
    vi.stubGlobal('fetch', fetchMock)
    await search('a b', 7)
    expect(fetchMock.mock.calls[0]?.[0]).toBe(
      '/api/library/search?q=a%20b&library=7',
    )
  })

  // `home`'s one caller, `pageReads.ts`, is exercised only through mocks of
  // this whole module (`Library.test.tsx`'s `vi.mock('../src/library', …
  // home: vi.fn())`, and others like it) — the real implementation below is
  // never reached from there, or from anywhere else.
  it('asks for the home lists without a library, and with one', async () => {
    const fetchMock = vi
      .fn()
      .mockImplementation(() =>
        Promise.resolve(json({ continuing: [], added: [], read: [] })),
      )
    vi.stubGlobal('fetch', fetchMock)
    await home()
    expect(fetchMock.mock.calls[0]?.[0]).toBe('/api/library/home')
    await home(7)
    expect(fetchMock.mock.calls[1]?.[0]).toBe('/api/library/home?library=7')
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

  describe('readerPath', () => {
    it('opens a work with a book edition in the book reader, and any other in the page reader', () => {
      const book = workCard({
        slug: 'w',
        editions: [edition({ key: 'raw' }), edition({ key: 'en', book: true })],
      })
      expect(readerPath(book)).toBe('/book/w')
      const comic = workCard({ slug: 'w', editions: [edition({ key: 'raw' })] })
      expect(readerPath(comic)).toBe('/read/w')
    })
  })

  describe('unreadable', () => {
    // O62's fixture: a volume with one broken edition and one intact.
    // **Which answer it must give depends on the edition `opens` names
    // (O84):** the guard asks about that one edition, so the same two
    // editions mean different things depending on which of them the reader
    // will open. Both directions are asserted, because a guard that only
    // ever sees one of them is the shape O62 was about.
    it('keeps a volume readable when the edition it opens is the intact one', () => {
      const w = workCard({
        opens: 'en',
        editions: [
          edition({ key: 'raw', missing: 3 }),
          edition({ key: 'en', missing: 0 }),
        ],
      })
      expect(unreadable(w)).toBe(false)
    })

    // **O84 itself.** The same volume, opened at its broken edition. Under an
    // `every` rule this would answer `false` and the reader would be handed a
    // link that fails at the first page — spec C1 §8's exact prohibition.
    it('marks a volume unreadable when the edition it opens has lost its files, even though a sibling is intact', () => {
      const w = workCard({
        opens: 'raw',
        editions: [
          edition({ key: 'raw', missing: 3 }),
          edition({ key: 'en', missing: 0 }),
        ],
      })
      expect(unreadable(w)).toBe(true)
    })

    it('marks a volume unreadable when every edition it holds has lost its files', () => {
      const w = workCard({
        opens: 'raw',
        editions: [
          edition({ key: 'raw', missing: 3 }),
          edition({ key: 'en', missing: 1 }),
        ],
      })
      expect(unreadable(w)).toBe(true)
    })

    // The other mutation O62 names: under an `every` rule, dropping the
    // emptiness guard would leave `[].every(...)` — vacuously `true` — as the
    // whole answer. The `find` form answers `undefined` instead, so the shape
    // is different and the requirement is not: a work with no visible edition
    // at all is a grant hiding everything, a different fact from missing files.
    it('does not call a work with no visible editions unreadable — that is a grant hiding it, not missing files', () => {
      const w = workCard({ editions: [] })
      expect(unreadable(w)).toBe(false)
    })

    // `opens` names an edition this caller cannot see, which happens when the
    // grant hides the hub. The reader is broken there for an unrelated reason
    // — the resolver is never sent that edition — and reporting it as
    // missing-files would be a different wrong answer, not a fix.
    it('does not call a work unreadable when `opens` names no edition it can see', () => {
      const w = workCard({
        opens: 'nothing-here',
        editions: [edition({ key: 'raw', missing: 0 })],
      })
      expect(unreadable(w)).toBe(false)
    })
  })
})

describe('withView', () => {
  it('writes one address for one view, whichever parameter came first', () => {
    const view = { chip: 'unread', sort: 'added' } as const
    expect(
      [
        new URLSearchParams({ library: '1' }),
        new URLSearchParams('library=1&sort=added'),
        new URLSearchParams('sort=title&show=multi&q=x&library=1'),
      ].map((params) => withView(params, view).toString()),
    ).toEqual([
      'library=1&show=unread&sort=added',
      'library=1&show=unread&sort=added',
      'library=1&q=x&show=unread&sort=added',
    ])
  })
})

/**
 * `?library=` as every screen reads it: an id is a positive decimal integer
 * with no leading zero, the form the sidebar's links write, and anything else
 * is no library rather than a number `Number` would make of it.
 */
describe('libraryParam', () => {
  it('reads no value as no narrowing', () => {
    expect(libraryParam(null)).toBeUndefined()
  })

  it('reads a positive integer with no leading zero as that library', () => {
    expect(libraryParam('1')).toBe(1)
    expect(libraryParam('12')).toBe(12)
  })

  it('reads anything else as no library id', () => {
    expect(
      ['abc', '', '0', '01', '-1', '1.5', ' 1', '1e2'].map((raw) => [
        raw,
        libraryParam(raw),
      ]),
    ).toEqual([
      ['abc', 'invalid'],
      ['', 'invalid'],
      ['0', 'invalid'],
      ['01', 'invalid'],
      ['-1', 'invalid'],
      ['1.5', 'invalid'],
      [' 1', 'invalid'],
      ['1e2', 'invalid'],
    ])
  })
})

/**
 * O133: a library cover is offered at three long edges, each described by its
 * width in a cover's 2:3 box, so the browser picks by the width it is drawn at.
 */
describe('cover sources', () => {
  it('adds 256, 512 and 768 px to the address the server sends, after its version, each described by its width at 2:3', () => {
    expect(
      coverSources(
        '/api/works/go%20toubun/raw%2Fen/pages/0/thumb?v=c0ffee-0-1',
      ),
    ).toEqual({
      src: '/api/works/go%20toubun/raw%2Fen/pages/0/thumb?v=c0ffee-0-1&w=256',
      srcSet:
        '/api/works/go%20toubun/raw%2Fen/pages/0/thumb?v=c0ffee-0-1&w=256 171w, /api/works/go%20toubun/raw%2Fen/pages/0/thumb?v=c0ffee-0-1&w=512 341w, /api/works/go%20toubun/raw%2Fen/pages/0/thumb?v=c0ffee-0-1&w=768 512w',
    })
  })

  it('starts the query on an address that carries none', () => {
    expect(coverSources('/api/works/w/raw/pages/0/thumb')).toEqual({
      src: '/api/works/w/raw/pages/0/thumb?w=256',
      srcSet:
        '/api/works/w/raw/pages/0/thumb?w=256 171w, /api/works/w/raw/pages/0/thumb?w=512 341w, /api/works/w/raw/pages/0/thumb?w=768 512w',
    })
  })
})

/**
 * In the phone layout a cover fills its column, which is narrower than two
 * covers and a gap; beside the sidebar it is the size.
 */
describe('cover sizes', () => {
  const filled = (width: number) =>
    PHONE_LAYOUT_PARTS.map((part) => `${part} ${width}px`).join(', ')
  it.each([
    [144, `${filled(320)}, 144px`],
    [240, `${filled(512)}, 240px`],
  ])('for a %i px cover is %s', (size, sizes) => {
    expect(coverSizes(size)).toBe(sizes)
  })
})
