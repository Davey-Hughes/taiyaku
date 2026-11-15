import { act } from 'react'
import {
  MemoryRouter,
  type MemoryRouterProps,
  Route,
  Routes,
  useLocation,
  useNavigate,
  useNavigationType,
} from 'react-router'
import {
  afterEach,
  beforeEach,
  describe,
  expect,
  it,
  onTestFinished,
  vi,
} from 'vitest'
import type { Me } from '../src/api'
import {
  BrowseControlsContext,
  BrowsePrefsContext,
  browseModule,
} from '../src/browseModule'
import { GateContext } from '../src/gate'
import * as library from '../src/library'
import * as metadata from '../src/metadata'
import {
  gridRead,
  homeRead,
  invalidateLibraryViews,
  searchRead,
} from '../src/pageReads'
import { entryOf, type Read, readSet } from '../src/reads'
import { Series } from '../src/Series'
import * as tilePanel from '../src/tilePanel'
import { deferred, flush } from './support/async'
import { FirstCommit } from './support/firstCommit'
import { press, submit, typeInto } from './support/input'
import { mount, rerender } from './support/mount'
import {
  holdTheClock,
  pass,
  WAIT_DRAWN_AFTER_MS,
  watchDrawn,
} from './support/wait'

vi.mock('../src/library', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/library')>()),
  oneSeries: vi.fn(),
  work: vi.fn(),
  // For the grid, home and search a title save marks stale: read into the
  // store beside the page, never drawn by it.
  grid: vi.fn(),
  home: vi.fn(),
  search: vi.fn(),
}))

vi.mock('../src/metadata', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/metadata')>()),
  patchWork: vi.fn(),
}))

// jsdom lays nothing out, so placing the panel reads nothing here: watched
// for when and with what `Series` asks for it.
vi.mock('../src/tilePanel', async (importOriginal) => ({
  ...(await importOriginal<typeof import('../src/tilePanel')>()),
  layPanel: vi.fn(),
}))

/** What `oneSeries` answers: a series page's body. */
type SeriesPage = { series: library.SeriesDetail; works: library.WorkCard[] }

/**
 * The address the server sends for page 0 of `slug`'s `key` edition. Its
 * version is opaque to the client, which hands it back untouched.
 */
const coverOf = (slug: string, key: string) =>
  `/api/works/${slug}/${key}/pages/0/thumb?v=c0ffee-0-1`

const edition = (
  over: Partial<library.EditionCard> = {},
): library.EditionCard => ({
  key: 'raw',
  label: 'Manga (RAW)',
  members: 1,
  pages: 180,
  missing: 0,
  cover: coverOf('gotoubun-v01', over.key ?? 'raw'),
  ...over,
})

const work = (over: Partial<library.WorkCard> = {}): library.WorkCard => ({
  id: 1,
  slug: 'gotoubun-v01',
  title: 'Go-Toubun no Hanayome v01',
  volume: 1,
  aligned: false,
  pages: 180,
  editions: [edition({ cover: coverOf(over.slug ?? 'gotoubun-v01', 'raw') })],
  opens: 'raw',
  ...over,
})

const who = (over: Partial<Me> = {}): Me => ({
  user_id: 1,
  admin: false,
  can_download: true,
  can_stream: true,
  can_stamp: false,
  ...over,
})

const card = (
  over: Partial<library.SeriesDetail> = {},
): library.SeriesDetail => ({
  id: 7,
  title: 'Go-Toubun no Hanayome',
  works: 3,
  editions: 5,
  aligned: 1,
  added: 3,
  unread: 2,
  cover: coverOf('gotoubun-v01', 'raw'),
  titles: [{ title: 'Go-Toubun no Hanayome', kind: 'romaji', primary: true }],
  ...over,
})

/**
 * Mount at `at`, `/series/7` unless a test names another address, and let the
 * series' read settle: the store calls the
 * mocked `oneSeries` once the progress saves sent before it have settled, so
 * the answer is drawn some microtask turns after the mount.
 *
 * `Series` takes its three switches from `BrowsePrefsContext` rather than as
 * props, so `props` here is layered over `browseModule.defaults` and handed
 * to the provider instead of spread onto the component.
 */
async function settleWith(
  props: Partial<
    Pick<
      React.ContextType<typeof BrowsePrefsContext>,
      'layout' | 'editionSelector' | 'panel'
    >
  > = {},
  me?: Me,
  at = '/series/7',
): Promise<HTMLElement> {
  let host: HTMLElement | undefined
  await act(async () => {
    host = mount(
      <BrowsePrefsContext.Provider
        value={{ ...browseModule.defaults, ...props }}
      >
        <GateContext.Provider
          value={me ? { who: me, signOut: () => {} } : undefined}
        >
          <MemoryRouter initialEntries={[at]}>
            <Routes>
              <Route path="/series/:id" element={<Series />} />
            </Routes>
          </MemoryRouter>
        </GateContext.Provider>
      </BrowsePrefsContext.Provider>,
    )
    await Promise.resolve()
    await Promise.resolve()
  })
  if (!host) throw new Error('nothing mounted')
  return host
}

const settle = () => settleWith()

const titles = (host: HTMLElement) =>
  [...host.querySelectorAll('.volumes .title')].map((n) => n.textContent)

/**
 * The `index`th tile, its panel opened by its `i`. Checks the panel is
 * drawn first, so a case about what the panel holds cannot pass on a panel
 * that never opened.
 */
async function openTile(host: HTMLElement, index = 0): Promise<Element> {
  const tile = host.querySelectorAll('.volumes > li')[index]
  await act(async () => {
    tile.querySelector<HTMLButtonElement>('.panel-open')?.click()
    await Promise.resolve()
    await Promise.resolve()
  })
  expect(tile.querySelector('.tile-panel .alignment-panel')).not.toBeNull()
  return tile
}

describe('Series, grid layout', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    // Opening a panel fetches the work's detail; a bare `vi.fn()` answers
    // `undefined`, and `reveal`'s `.then` throws on it.
    vi.mocked(library.work).mockResolvedValue({ ...work(), alignments: [] })
    vi.mocked(library.oneSeries).mockResolvedValue({
      series: card(),
      works: [
        work(),
        work({
          id: 2,
          slug: 'gotoubun-v03',
          title: 'Go-Toubun no Hanayome v03',
          volume: 3,
          aligned: true,
        }),
        work({
          id: 3,
          slug: 'gotoubun-v08',
          title: 'Go-Toubun no Hanayome v08',
          volume: 8,
        }),
      ],
    })
  })

  it('asks for the series named in the URL', async () => {
    await settle()
    expect(vi.mocked(library.oneSeries).mock.calls).toEqual([[7, undefined]])
  })

  it('asks for what the library the URL names holds of the series — spec §3.6', async () => {
    await settleWith({}, undefined, '/series/7?library=3')
    expect(vi.mocked(library.oneSeries).mock.calls).toEqual([[7, 3]])
  })

  it('asks nothing for a library the URL names that is no library id, and says the series is not there', async () => {
    const host = await settleWith({}, undefined, '/series/7?library=01')
    expect(vi.mocked(library.oneSeries)).not.toHaveBeenCalled()
    expect(host.querySelector('[role="alert"]')?.textContent).toBe(
      '7 is not a series',
    )
    expect(host.querySelector('.volumes')).toBeNull()
  })

  it('names the series and lists its volumes in order', async () => {
    const host = await settle()
    expect(host.querySelector('h1')?.textContent).toBe('Go-Toubun no Hanayome')
    // One heading once loaded: the hidden stand-in is gone.
    expect(host.querySelectorAll('h1')).toHaveLength(1)
    expect(titles(host)).toEqual([
      'Go-Toubun no Hanayome v01',
      'Go-Toubun no Hanayome v03',
      'Go-Toubun no Hanayome v08',
    ])
  })

  it('marks the aligned volume and only the aligned volume', async () => {
    const host = await settle()
    const marks = [...host.querySelectorAll('.volumes li')].map((li) =>
      li.querySelector('.aligned') === null ? '-' : '⇄',
    )
    expect(marks).toEqual(['-', '⇄', '-'])
    // Spec C1 §3 puts the mark *on the cover*, and `styles/library.css` has
    // exactly one rule for it — `.cover-box .aligned` — so outside that box the
    // mark loses its positioning and its chip. `textContent`/presence checks
    // above cannot tell "on the cover" from "moved below the title"; only the
    // ancestor can.
    const alignedLi = [...host.querySelectorAll('.volumes li')].find(
      (li) => li.querySelector('.aligned') !== null,
    )
    expect(alignedLi?.querySelector('.cover-box .aligned')).not.toBeNull()
  })

  it('links each volume into the reader by its slug', async () => {
    const host = await settle()
    const hrefs = [...host.querySelectorAll('.volumes a')].map((a) =>
      a.getAttribute('href'),
    )
    expect(hrefs).toEqual([
      '/read/gotoubun-v01',
      '/read/gotoubun-v03',
      '/read/gotoubun-v08',
    ])
  })

  it('draws each volume cover from its own edition rather than the series cover', async () => {
    const host = await settle()
    expect(
      [...host.querySelectorAll('.volumes li')].map((li) =>
        li.querySelector('img')?.getAttribute('src'),
      ),
    ).toEqual([
      '/api/works/gotoubun-v01/raw/pages/0/thumb?v=c0ffee-0-1&w=256',
      '/api/works/gotoubun-v03/raw/pages/0/thumb?v=c0ffee-0-1&w=256',
      '/api/works/gotoubun-v08/raw/pages/0/thumb?v=c0ffee-0-1&w=256',
    ])
  })

  it('says so when the series cannot be read', async () => {
    vi.mocked(library.oneSeries).mockRejectedValue(
      new Error('/api/library/series/7: 404'),
    )
    const host = await settle()
    expect(host.querySelector('[role="alert"]')?.textContent).toContain('404')
    expect(host.querySelector('.volumes')).toBeNull()
  })

  it('has a heading for a screen reader when the series cannot be shown — O163', async () => {
    vi.mocked(library.oneSeries).mockRejectedValue(
      new Error('/api/library/series/7: 404'),
    )
    const host = await settle()
    expect(host.querySelector('[role="alert"]')?.textContent).toContain('404')
    expect(
      [...host.querySelectorAll('h1')].map((h) => [h.textContent, h.className]),
    ).toEqual([['Series', 'visually-hidden']])
  })

  it('has that heading while the series is still loading — O163', async () => {
    // Never answers: the state every visit passes through, held still.
    vi.mocked(library.oneSeries).mockReturnValue(new Promise(() => {}))
    const host = await settle()
    expect(host.querySelector('[role="alert"]')).toBeNull()
    expect(
      [...host.querySelectorAll('h1')].map((h) => [h.textContent, h.className]),
    ).toEqual([['Series', 'visually-hidden']])
  })

  it('does not fetch when the URL carries something that is not a number', async () => {
    let host: HTMLElement | undefined
    await act(async () => {
      host = mount(
        <MemoryRouter initialEntries={['/series/not-a-number']}>
          <Routes>
            <Route path="/series/:id" element={<Series />} />
          </Routes>
        </MemoryRouter>,
      )
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(vi.mocked(library.oneSeries)).not.toHaveBeenCalled()
    expect(host?.querySelector('[role="alert"]')?.textContent).toContain(
      'not-a-number',
    )
    // Not a series, and still a screen with a heading (O163).
    expect(
      [...(host?.querySelectorAll('h1') ?? [])].map((h) => [
        h.textContent,
        h.className,
      ]),
    ).toEqual([['Series', 'visually-hidden']])
  })

  it('does not fetch when the URL carries an id past Number.MAX_SAFE_INTEGER', async () => {
    // All digits, so `/^\d+$/` alone would let it through — but `Number` on
    // something this long either rounds to a different, wrong-but-plausible
    // id or loses integer shape entirely (see `Series.tsx`'s doc comment on
    // `seriesId`). Either way it must take the same "is not a series" path
    // the non-numeric case above does, rather than fetch a wrong id.
    const tooBig = '9007199254740993'
    let host: HTMLElement | undefined
    await act(async () => {
      host = mount(
        <MemoryRouter initialEntries={[`/series/${tooBig}`]}>
          <Routes>
            <Route path="/series/:id" element={<Series />} />
          </Routes>
        </MemoryRouter>,
      )
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(vi.mocked(library.oneSeries)).not.toHaveBeenCalled()
    expect(host?.querySelector('[role="alert"]')?.textContent).toContain(tooBig)
  })

  it('draws no download on a tile in button mode, and one at the foot of its open panel', async () => {
    const host = await settleWith({ panel: 'button' }, who())
    const tiles = [...host.querySelectorAll('.volumes > li')]
    expect(tiles).toHaveLength(3)
    expect(tiles.map((li) => li.querySelector(':scope > a.get'))).toEqual([
      null,
      null,
      null,
    ])
    await openTile(host, 1)
    expect(
      tiles.map((li) => li.querySelectorAll('.tile-panel a.get').length),
    ).toEqual([0, 1, 0])
    const get = tiles[1].querySelector('.tile-panel')?.lastElementChild
    expect(get?.matches('a.get')).toBe(true)
    expect(get?.textContent).toBe('↓ Download')
    expect(get?.getAttribute('href')).toBe(
      '/api/works/gotoubun-v03/raw/download',
    )
    // Bare, with no value. The server's `Content-Disposition` carries an
    // ASCII `filename=` and an RFC 6266 `filename*=`, and a value here would
    // override both with a guess made in the browser.
    expect(get?.getAttribute('download')).toBe('')
    expect(get?.getAttribute('aria-label')).toBe(
      'download Go-Toubun no Hanayome v03 — Manga (RAW)',
    )
  })

  it.each(['off', 'on-select'] as const)(
    'puts the download on the cover’s corner, one per tile, in %s mode',
    async (panel) => {
      const host = await settleWith({ panel }, who())
      const tiles = [...host.querySelectorAll('.volumes > li')]
      expect(
        tiles.map((li) => {
          const get = li.querySelector(':scope > a.get')
          return [
            get?.getAttribute('href'),
            get?.textContent,
            get?.getAttribute('download'),
            get?.getAttribute('aria-label'),
          ]
        }),
      ).toEqual([
        [
          '/api/works/gotoubun-v01/raw/download',
          '↓',
          '',
          'download Go-Toubun no Hanayome v01 — Manga (RAW)',
        ],
        [
          '/api/works/gotoubun-v03/raw/download',
          '↓',
          '',
          'download Go-Toubun no Hanayome v03 — Manga (RAW)',
        ],
        [
          '/api/works/gotoubun-v08/raw/download',
          '↓',
          '',
          'download Go-Toubun no Hanayome v08 — Manga (RAW)',
        ],
      ])
      expect(host.querySelectorAll('.volumes a.get')).toHaveLength(3)
      expect(host.querySelector('.panel-open')).toBeNull()
      if (panel === 'on-select') {
        await act(async () => {
          tiles[1]
            .querySelector('a')
            ?.dispatchEvent(new FocusEvent('focusin', { bubbles: true }))
          await Promise.resolve()
          await Promise.resolve()
        })
        expect(
          tiles[1].querySelector('.tile-panel .alignment-panel'),
        ).not.toBeNull()
        expect(tiles[1].querySelector('.tile-panel a.get')).toBeNull()
        expect(host.querySelectorAll('.volumes a.get')).toHaveLength(3)
      }
    },
  )

  it.each(['off', 'button'] as const)(
    'keeps the download anchor outside the tile link, not nested in it, in %s mode',
    async (panel) => {
      const host = await settleWith({ panel }, who())
      const tile =
        panel === 'button'
          ? await openTile(host)
          : host.querySelectorAll('.volumes > li')[0]
      const get = tile.querySelector('a.get')
      expect(get).not.toBeNull()
      // A nested anchor is invalid HTML and browsers reparent it, which a
      // snapshot would render identically — so this is asserted structurally.
      // The tile's own `<Link>` is the anchor holding `.cover-box`.
      const tileLink = tile.querySelector('a:has(.cover-box)')
      expect(tileLink).not.toBeNull()
      expect(tileLink?.contains(get)).toBe(false)
    },
  )

  it.each(['button', 'off', 'on-select'] as const)(
    'shows no download at all when the account may not download, in %s mode',
    async (panel) => {
      const host = await settleWith({ panel }, who({ can_download: false }))
      if (panel === 'button') await openTile(host)
      expect(host.querySelectorAll('.volumes a.get')).toHaveLength(0)
    },
  )

  it.each(['button', 'off', 'on-select'] as const)(
    'shows no download when no gate sits above the screen, in %s mode',
    async (panel) => {
      // `useGate()` is `undefined` here, and `gate.ts` says the safe reading of
      // that is "no account known" rather than a permissive one.
      const host = await settleWith({ panel })
      if (panel === 'button') await openTile(host)
      expect(host.querySelectorAll('.volumes a.get')).toHaveLength(0)
    },
  )

  it('names each i “details for” its volume', async () => {
    const host = await settleWith({ panel: 'button' }, who())
    expect(
      [...host.querySelectorAll('.volumes > li')].map((li) =>
        li.querySelector('.panel-open')?.getAttribute('aria-label'),
      ),
    ).toEqual([
      'details for Go-Toubun no Hanayome v01',
      'details for Go-Toubun no Hanayome v03',
      'details for Go-Toubun no Hanayome v08',
    ])
  })

  it('names its tab after the series, once it has loaded — O173', async () => {
    document.title = 'stale'
    await settle()
    expect(document.title).toBe('Go-Toubun no Hanayome — taiyaku')
  })
})

/**
 * O62's guard (`unreadable`, spec C1 §8) applies in the grid too, not only in
 * `Table`: `browseModule.ts` makes `layout: 'grid'` the **default**, and
 * `settle()`/`settleWith({})` below pass nothing, so these cases run the
 * shipped defaults. Without the guard a reader would meet an unguarded
 * `<Link to={/read/${w.slug}}>` and an unguarded per-edition download here.
 */
describe('Series, grid layout — the reader link and download withheld', () => {
  it('withholds the reader link for a volume whose every edition has lost its files', async () => {
    vi.mocked(library.oneSeries).mockResolvedValue({
      series: card(),
      works: [
        work({
          editions: [
            edition({ key: 'raw', missing: 3 }),
            edition({ key: 'en', missing: 1 }),
          ],
        }),
      ],
    })
    const host = await settle()
    expect(host.querySelector('.volumes a[href^="/read/"]')).toBeNull()
    // Still named, and still says why — same rule `Table` already follows.
    expect(host.querySelector('.volumes .title')?.textContent).toBe(
      'Go-Toubun no Hanayome v01',
    )
    expect(host.querySelector('.volumes .unreadable')?.textContent).toBeTruthy()
  })

  it('keeps the reader link when the edition it opens is the intact one', async () => {
    vi.mocked(library.oneSeries).mockResolvedValue({
      series: card(),
      works: [
        work({
          opens: 'en',
          editions: [
            edition({ key: 'raw', missing: 3 }),
            edition({ key: 'en', missing: 0 }),
          ],
        }),
      ],
    })
    const host = await settle()
    expect(
      host.querySelector('.volumes a[href="/read/gotoubun-v01"]'),
    ).not.toBeNull()
    expect(host.querySelector('.volumes .unreadable')).toBeNull()
  })

  it.each(['button', 'off'] as const)(
    'withholds the per-tile download for the edition it would show, when that edition has lost its files, even though the volume itself stays readable, in %s mode',
    async (panel) => {
      vi.mocked(library.work).mockResolvedValue({ ...work(), alignments: [] })
      vi.mocked(library.oneSeries).mockResolvedValue({
        series: card(),
        works: [
          work({
            // **`opens: 'en'` is what keeps this test about the download.**
            // `chosen === 'all'` shows `editions[0]` — the broken one — while
            // the volume itself opens `en` and stays readable. Downloading
            // files that are not on disk is not an offer this screen should
            // make, independent of whether the volume opens; leaving `opens`
            // at the broken edition would withhold the reader link too and the
            // test could no longer tell the two guards apart.
            opens: 'en',
            editions: [
              edition({ key: 'raw', missing: 2 }),
              edition({ key: 'en', missing: 0 }),
            ],
          }),
        ],
      })
      const host = await settleWith({ panel }, who())
      expect(
        host.querySelector('.volumes a[href="/read/gotoubun-v01"]'),
      ).not.toBeNull()
      if (panel === 'button') await openTile(host)
      expect(host.querySelector('.volumes a.get')).toBeNull()
    },
  )
})

describe('Series, the work-level editor', () => {
  /**
   * C3 §5: "contextual, from the thing being corrected" — a volume's detail
   * panel edits work-level fields, and there is no separate metadata screen
   * to navigate to. So the editor belongs where the panel already opens, and
   * nowhere else.
   */
  it('offers the editor inside an open panel, for an administrator', async () => {
    // Opening the panel fetches the work's detail; without this the mock
    // answers `undefined` and `reveal`'s `.then` throws an uncaught
    // TypeError. Vitest still reports the test itself as passing and fails
    // the run.
    vi.mocked(library.work).mockResolvedValue({ ...work(), alignments: [] })
    const host = await settleWith({ panel: 'button' }, who({ admin: true }))
    expect(host.querySelector('.metadata-editor')).toBeNull()
    const open = host.querySelector<HTMLButtonElement>('.panel-open')
    await act(async () => {
      open?.click()
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(host.querySelector('.metadata-editor')).not.toBeNull()
    expect(
      host.querySelector<HTMLTextAreaElement>(
        '.metadata-editor textarea[name="title"]',
      )?.value,
    ).toBe('Go-Toubun no Hanayome v01')
  })

  it('offers it to nobody else, because every PATCH route is admin-only', async () => {
    // A control that answers 403 is worse than no control: it invites the
    // correction and then refuses it. The routes are admin-gated in
    // `routes.rs`, so the affordance has to be too.
    vi.mocked(library.work).mockResolvedValue({ ...work(), alignments: [] })
    const host = await settleWith({ panel: 'button' }, who({ admin: false }))
    const open = host.querySelector<HTMLButtonElement>('.panel-open')
    await act(async () => {
      open?.click()
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(host.querySelector('.alignment-panel')).not.toBeNull()
    expect(host.querySelector('.metadata-editor')).toBeNull()
  })

  /**
   * C3 §7 / design doc §1.1: `can_stamp` is the one capability O86 does not
   * fold into `admin`, so the stamp section inside the editor has to be gated
   * on the column alone. These two are the case the column exists for and its
   * mirror — an administrator is necessary (the describe block above already
   * covers "no editor at all" without it) but not sufficient.
   */
  it('offers the ComicInfo stamp section to an administrator who holds can_stamp', async () => {
    vi.mocked(library.work).mockResolvedValue({ ...work(), alignments: [] })
    const host = await settleWith(
      { panel: 'button' },
      who({ admin: true, can_stamp: true }),
    )
    const open = host.querySelector<HTMLButtonElement>('.panel-open')
    await act(async () => {
      open?.click()
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(host.querySelector('.metadata-editor')).not.toBeNull()
    expect(host.querySelector('.stamp-comicinfo')).not.toBeNull()
  })

  it('withholds the ComicInfo stamp section from an administrator who does not hold can_stamp', async () => {
    vi.mocked(library.work).mockResolvedValue({ ...work(), alignments: [] })
    const host = await settleWith(
      { panel: 'button' },
      who({ admin: true, can_stamp: false }),
    )
    const open = host.querySelector<HTMLButtonElement>('.panel-open')
    await act(async () => {
      open?.click()
      await Promise.resolve()
      await Promise.resolve()
    })
    // The editor itself is still offered — this is about the one section
    // inside it, not the admin gate the two tests above already cover.
    expect(host.querySelector('.metadata-editor')).not.toBeNull()
    expect(host.querySelector('.stamp-comicinfo')).toBeNull()
  })

  /**
   * **A title saved marks the Library views stale** (spec
   * `2026-10-28-loading-library-design.md` §6.4): the series page drawing the
   * work reads its series again at once, and the grid, home and search kept
   * beside it are marked, to be read on their next arrival. Only once the save
   * has answered, and only when it has succeeded, whether or not it reached the
   * library's documents: a read sent before would answer with the title from
   * before the save.
   */
  describe('a title saved', () => {
    const renamed = 'Go-Toubun no Hanayome, volume 1'
    const saved = (persisted: boolean): metadata.PatchResult => ({
      taiyaku_library: 1,
      updated: '2026-10-28T00:00:00Z',
      title: renamed,
      persisted,
      ...(persisted ? {} : { why: ['read-only file system'] }),
    })

    /** The other Library views' reads, kept in the store beside the page. */
    const views = (): Read<unknown>[] => [
      gridRead(undefined),
      homeRead(undefined),
      searchRead('gotoubun', undefined),
    ]
    /** Each kept view's address, and whether it is marked stale. */
    const marked = () => views().map((r) => [r.address, entryOf(r)?.stale])
    const allMarked = (stale: boolean) => views().map((r) => [r.address, stale])

    beforeEach(() => {
      vi.clearAllMocks()
      vi.mocked(library.work).mockResolvedValue({ ...work(), alignments: [] })
      vi.mocked(library.grid).mockResolvedValue({ series: [], unfiled: [] })
      vi.mocked(library.home).mockResolvedValue({
        continuing: [],
        added: [],
        read: [],
      })
      vi.mocked(library.search).mockResolvedValue({
        series: [],
        unfiled: [],
        query: 'gotoubun',
      })
      // Reset, not only cleared: a once-answer a case left unused would
      // otherwise answer the next case's first call.
      vi.mocked(metadata.patchWork).mockReset()
      vi.mocked(library.oneSeries).mockReset()
      vi.mocked(library.oneSeries).mockResolvedValue({
        series: card(),
        works: [work()],
      })
    })

    /**
     * The page with the first volume's panel open and a title typed into its
     * editor, the other views read into the store beside it, and every mock's
     * calls cleared.
     */
    const editing = async () => {
      await readSet(views())
      expect(marked()).toEqual(allMarked(false))
      const host = await settleWith({ panel: 'button' }, who({ admin: true }))
      await flush(() => {
        host.querySelector<HTMLButtonElement>('.panel-open')?.click()
      })
      const box = host.querySelector<HTMLTextAreaElement>(
        '.metadata-editor textarea[name="title"]',
      )
      if (!box) throw new Error('no title box')
      await typeInto(box, renamed)
      vi.mocked(library.oneSeries).mockClear()
      vi.mocked(library.grid).mockClear()
      vi.mocked(library.home).mockClear()
      vi.mocked(library.search).mockClear()
      return host
    }

    /** Every alert drawn, by its words. */
    const alerts = (host: HTMLElement) =>
      [...host.querySelectorAll('[role="alert"]')].map((a) => a.textContent)

    const form = (host: HTMLElement) => {
      const found = host.querySelector('.metadata-editor')
      if (!found) throw new Error('no editor')
      return found
    }

    it('reads the series again once and marks the other views stale once the save answers, not before', async () => {
      const save = deferred<metadata.PatchResult>()
      vi.mocked(metadata.patchWork).mockReturnValueOnce(save.promise)
      const host = await editing()
      await submit(form(host))
      await flush()
      expect(vi.mocked(metadata.patchWork).mock.calls).toEqual([
        ['gotoubun-v01', { title: renamed }],
      ])
      expect(vi.mocked(library.oneSeries).mock.calls).toEqual([])
      expect(marked()).toEqual(allMarked(false))

      vi.mocked(library.oneSeries).mockResolvedValue({
        series: card(),
        works: [work({ title: renamed })],
      })
      await flush(() => save.resolve(saved(true)))
      await flush()
      await flush()
      expect(vi.mocked(library.oneSeries).mock.calls).toEqual([[7, undefined]])
      expect(
        host.querySelector('.volumes a[href="/read/gotoubun-v01"] .title')
          ?.textContent,
      ).toBe(renamed)
      expect(marked()).toEqual(allMarked(true))
      // Marked, not read: nothing draws them.
      expect(vi.mocked(library.grid).mock.calls).toEqual([])
      expect(vi.mocked(library.home).mock.calls).toEqual([])
      expect(vi.mocked(library.search).mock.calls).toEqual([])
    })

    it('reads the series again and marks the other views when the save answers that it did not reach the library’s documents', async () => {
      vi.mocked(metadata.patchWork).mockResolvedValueOnce(saved(false))
      const host = await editing()
      // The re-read's answer, set after the page drew the title the field
      // is mounted with: a title equal to that one is not sent.
      vi.mocked(library.oneSeries).mockResolvedValue({
        series: card(),
        works: [work({ title: renamed })],
      })
      await submit(form(host))
      await flush()
      await flush()
      expect(vi.mocked(metadata.patchWork).mock.calls).toEqual([
        ['gotoubun-v01', { title: renamed }],
      ])
      expect(host.querySelector('.metadata-editor .unwritten')).not.toBeNull()
      expect(vi.mocked(library.oneSeries).mock.calls).toEqual([[7, undefined]])
      expect(
        host.querySelector('.volumes a[href="/read/gotoubun-v01"] .title')
          ?.textContent,
      ).toBe(renamed)
      expect(marked()).toEqual(allMarked(true))
    })

    it('keeps the series it drew, and the editor open, beside the alert when the re-read after a save fails', async () => {
      vi.mocked(metadata.patchWork).mockResolvedValueOnce(saved(true))
      const host = await editing()
      expect(alerts(host)).toEqual([])
      vi.mocked(library.oneSeries).mockRejectedValueOnce(
        new Error('/api/library/series/7: 500'),
      )
      await submit(form(host))
      await flush()
      await flush()
      expect(vi.mocked(library.oneSeries).mock.calls).toEqual([[7, undefined]])
      expect(alerts(host)).toEqual(['/api/library/series/7: 500'])
      expect(host.querySelector('h1')?.textContent).toBe(
        'Go-Toubun no Hanayome',
      )
      expect(
        host.querySelector('.volumes a[href="/read/gotoubun-v01"] .title')
          ?.textContent,
      ).toBe('Go-Toubun no Hanayome v01')
      expect(
        host.querySelector<HTMLTextAreaElement>(
          '.metadata-editor textarea[name="title"]',
        )?.value,
      ).toBe(renamed)
    })

    it('reads nothing again and marks nothing when the save fails', async () => {
      vi.mocked(metadata.patchWork).mockRejectedValueOnce(
        new Error('/api/library/works/gotoubun-v01: 500'),
      )
      const host = await editing()
      await submit(form(host))
      await flush()
      await flush()
      expect(vi.mocked(metadata.patchWork)).toHaveBeenCalledTimes(1)
      expect(host.querySelector('p.failure')?.textContent).toBe(
        '/api/library/works/gotoubun-v01: 500',
      )
      expect(vi.mocked(library.oneSeries).mock.calls).toEqual([])
      expect(marked()).toEqual(allMarked(false))
    })
  })
})

describe('Series, table layout', () => {
  beforeEach(() => {
    vi.mocked(library.oneSeries).mockResolvedValue({
      series: card(),
      works: [
        work({
          editions: [
            edition({ key: 'raw', label: 'Manga (RAW)', pages: 180 }),
            edition({ key: 'en', label: 'Manga', pages: 178 }),
          ],
        }),
        work({
          id: 2,
          slug: 'gotoubun-v03',
          title: 'Go-Toubun no Hanayome v03',
          volume: 3,
          aligned: true,
          // Deliberately not 'Manga (RAW)': real edition labels are
          // generated per volume from member filenames, so two volumes
          // sharing the `raw` key routinely disagree on the label (a real
          // one: `Go-Toubun no Hanayome - Fan Translated - c000.cbz + 6
          // more`). A fixture where every volume agreed on the label could
          // not tell first-seen from last-seen apart — see the "takes the
          // column label from the first volume" test below.
          editions: [
            edition({
              key: 'raw',
              label: 'Go-Toubun no Hanayome v03 (RAW) [c024-036].cbz + 11 more',
              pages: 190,
            }),
          ],
        }),
      ],
    })
  })

  it('gives every edition in the series a column, in first-seen order', async () => {
    const host = await settleWith({ layout: 'table' })
    const heads = [...host.querySelectorAll('.editions thead th')].map(
      (th) => th.textContent,
    )
    expect(heads).toEqual(['Volume', 'Manga (RAW)', 'Manga'])
  })

  it('takes a shared edition’s column label from the first volume that mentions it, not the last', async () => {
    // `editionColumns`'s own doc comment: a `WorkCard` carries only the
    // editions its caller may see, so a grant can hide an edition from
    // volume 1 and not from volume 3 — the first volume to *mention* a key
    // is not always volume 1. That makes first-seen-wins a deliberate
    // choice, not an accident of iteration order, and it needs to survive
    // someone "simplifying" the `Map` in `editionColumns` to an
    // unconditional `.set()` (last-seen-wins) without noticing the key
    // order is unchanged. It matters in practice, not just in theory: real
    // edition labels are generated per volume from member filenames, so two
    // volumes sharing an edition key routinely disagree on the label — this
    // fixture's v01 and v03 both carry `raw`, with different labels, for
    // exactly that reason.
    const host = await settleWith({ layout: 'table' })
    const heads = [...host.querySelectorAll('.editions thead th')]
    // Column order (`raw` before `en`) is also first-seen, but that is
    // covered by the test above; this one only cares about the label in
    // whichever column `raw` lands in.
    const rawHeader = heads[1]
    expect(rawHeader?.textContent).toBe('Manga (RAW)')
    expect(rawHeader?.textContent).not.toBe(
      'Go-Toubun no Hanayome v03 (RAW) [c024-036].cbz + 11 more',
    )
  })

  it('shows the page count where a volume has that edition', async () => {
    const host = await settleWith({ layout: 'table' })
    const firstRow = host.querySelectorAll('.editions tbody tr')[0]
    const cells = [...(firstRow?.querySelectorAll('td') ?? [])].map(
      (td) => td.textContent,
    )
    expect(cells).toEqual(['Go-Toubun no Hanayome v01', '180', '178'])
  })

  it('marks a missing edition rather than leaving the cell blank', async () => {
    const host = await settleWith({ layout: 'table' })
    const secondRow = host.querySelectorAll('.editions tbody tr')[1]
    const missing = secondRow?.querySelectorAll('td')[2]
    expect(missing?.classList.contains('missing')).toBe(true)
    expect(missing?.textContent).toBe('—')
    // A dash alone is not a label. §9's rule: what a sighted reader infers
    // from a dashed cell, a screen reader has to be told.
    expect(missing?.getAttribute('aria-label')).toBe('no Manga edition')
  })

  it('still links each volume into the reader', async () => {
    const host = await settleWith({ layout: 'table' })
    const hrefs = [...host.querySelectorAll('.editions a')].map((a) =>
      a.getAttribute('href'),
    )
    expect(hrefs).toEqual(['/read/gotoubun-v01', '/read/gotoubun-v03'])
  })

  it('marks the aligned volume here too — the mark is the page, not the layout', async () => {
    const host = await settleWith({ layout: 'table' })
    const marked = [...host.querySelectorAll('.editions tbody tr')].map(
      (tr) => tr.querySelector('.aligned') !== null,
    )
    expect(marked).toEqual([false, true])
  })

  it('draws no grid when it is drawing a table', async () => {
    const host = await settleWith({ layout: 'table' })
    expect(host.querySelector('.volumes')).toBeNull()
    expect(host.querySelector('.editions')).not.toBeNull()
  })

  /**
   * The selector is hidden under the table layout: `Table` takes no `chosen`,
   * so a selector drawn there would govern nothing. `editionSelector: true`
   * and `panel: 'button'` are `browseModule.defaults` already, so this needs
   * no override beyond `layout`.
   */
  it('hides the edition selector and the alignment button, since the table already answers what they ask', async () => {
    const host = await settleWith({ layout: 'table' })
    expect(host.querySelector('.edition-selector')).toBeNull()
    expect(host.querySelector('.panel-open')).toBeNull()
  })

  /**
   * O62, and spec C1 §8's last empty state: a work whose members are gone
   * from disk must be *shown as unreadable rather than offered and then
   * failing at the first page*. The treatment that closes the trap is
   * withholding the link — a badge beside a live link still sends a reader
   * into a page that cannot load.
   */
  it('does not offer a download for an edition whose files have left the disk', async () => {
    vi.mocked(library.oneSeries).mockResolvedValue({
      series: card(),
      works: [
        work({
          editions: [
            edition({
              key: 'raw',
              label: 'Manga (RAW)',
              pages: 180,
              missing: 2,
            }),
            edition({ key: 'en', label: 'Manga', pages: 178 }),
          ],
        }),
      ],
    })
    const host = await settleWith({ layout: 'table' }, who())
    const cells = [
      ...(host
        .querySelectorAll('.editions tbody tr')[0]
        ?.querySelectorAll('td') ?? []),
    ]
    const gone = cells[1]
    const held = cells[2]
    expect(gone?.getAttribute('data-unreadable')).toBe('true')
    expect(gone?.querySelector('a')).toBeNull()
    expect(gone?.getAttribute('aria-label')).toContain('2')
    // The intact edition beside it is untouched, so this is the missing
    // count doing the work rather than the whole row being suppressed.
    expect(held?.getAttribute('data-unreadable')).toBeNull()
    expect(held?.querySelector('a')).not.toBeNull()
  })

  it('does not link a volume into the reader when every edition it holds is gone', async () => {
    vi.mocked(library.oneSeries).mockResolvedValue({
      series: card(),
      works: [
        work({
          editions: [
            edition({
              key: 'raw',
              label: 'Manga (RAW)',
              pages: 180,
              missing: 1,
            }),
            edition({ key: 'en', label: 'Manga', pages: 178, missing: 3 }),
          ],
        }),
      ],
    })
    const host = await settleWith({ layout: 'table' }, who())
    const first = host.querySelectorAll('.editions tbody tr')[0]
    expect(first?.querySelector('a[href^="/read/"]')).toBeNull()
    // Still named, and still says why — withholding the link must not
    // withhold the volume itself, which would read as a library that has
    // lost the work rather than one that cannot open it.
    expect(first?.textContent).toContain('Go-Toubun no Hanayome v01')
    expect(first?.querySelector('.unreadable')?.textContent).toBeTruthy()
  })

  it('makes each held edition’s page count a download link', async () => {
    const host = await settleWith({ layout: 'table' }, who())
    const cell = host.querySelector<HTMLAnchorElement>(
      'table.editions tbody tr td a.get',
    )
    expect(cell).not.toBeNull()
    expect(cell?.getAttribute('href')).toBe(
      '/api/works/gotoubun-v01/raw/download',
    )
    expect(cell?.textContent).toBe('180')
    expect(cell?.getAttribute('download')).toBe('')
    // `aria-label` *replaces* the element's content for the accessible name,
    // and this cell holds nothing but the anchor — so the count has to be in
    // the label too, or a screen reader stops announcing the one fact the
    // cell exists to carry.
    expect(cell?.getAttribute('aria-label')).toContain('180 pages')
  })

  it('leaves a missing edition’s cell as plain text', async () => {
    const host = await settleWith({ layout: 'table' }, who())
    const missing = host.querySelector('table.editions td.missing')
    expect(missing).not.toBeNull()
    expect(missing?.querySelector('a')).toBeNull()
    expect(missing?.textContent).toBe('—')
  })

  it('renders page counts as plain text when the account may not download', async () => {
    const host = await settleWith(
      { layout: 'table' },
      who({ can_download: false }),
    )
    expect(host.querySelectorAll('table.editions a.get')).toHaveLength(0)
    // The number is still there — only the link is gone.
    const cells = [...host.querySelectorAll('table.editions tbody td')]
    expect(cells.some((c) => c.textContent === '180')).toBe(true)
  })

  it('sits in a box of its own that scrolls sideways, so the page never does', async () => {
    const host = await settleWith({ layout: 'table' })
    expect(host.querySelector('.table-scroll > table.editions')).not.toBeNull()
  })

  it('marks the page counts, and only them, for the figures face', async () => {
    const host = await settleWith({ layout: 'table' })
    const marked = [...host.querySelectorAll('.editions tbody tr')].map((row) =>
      [...row.querySelectorAll('td')].map((td) =>
        td.classList.contains('pages'),
      ),
    )
    // The second volume has no `en` edition: its third cell is the dash.
    expect(marked).toEqual([
      [false, true, true],
      [false, true, false],
    ])
  })
})

describe('Series, the edition selector', () => {
  beforeEach(() => {
    vi.mocked(library.oneSeries).mockResolvedValue({
      series: card(),
      works: [
        work({
          editions: [
            edition({ key: 'raw', label: 'Manga (RAW)', pages: 180 }),
            edition({ key: 'en', label: 'Manga', pages: 178 }),
          ],
        }),
        work({
          id: 2,
          slug: 'gotoubun-v03',
          title: 'Go-Toubun no Hanayome v03',
          volume: 3,
          editions: [
            edition({
              key: 'raw',
              label: 'Manga (RAW)',
              pages: 190,
              cover: coverOf('gotoubun-v03', 'raw'),
            }),
          ],
        }),
      ],
    })
  })

  it('offers every edition in the series, plus all of them', async () => {
    const host = await settleWith({ editionSelector: true })
    const options = [...host.querySelectorAll('.edition-selector option')].map(
      (o) => o.textContent,
    )
    expect(options).toEqual(['All editions', 'Manga (RAW)', 'Manga'])
  })

  it('is absent when it is switched off', async () => {
    const host = await settleWith({ editionSelector: false })
    expect(host.querySelector('.edition-selector')).toBeNull()
  })

  it('draws each volume from the cover address of the edition it shows, and redraws it when another is chosen', async () => {
    const host = await settleWith({ editionSelector: true })
    const covers = () =>
      [...host.querySelectorAll('.volumes li')].map((li) =>
        li.querySelector('img')?.getAttribute('src'),
      )
    expect(covers()).toEqual([
      '/api/works/gotoubun-v01/raw/pages/0/thumb?v=c0ffee-0-1&w=256',
      '/api/works/gotoubun-v03/raw/pages/0/thumb?v=c0ffee-0-1&w=256',
    ])
    const select = host.querySelector('select') as HTMLSelectElement
    act(() => {
      select.value = 'en'
      select.dispatchEvent(new Event('change', { bubbles: true }))
    })
    expect(covers()).toEqual([
      '/api/works/gotoubun-v01/en/pages/0/thumb?v=c0ffee-0-1&w=256',
      // v03 has no `en`: it is dimmed, and drawn from the edition it leads with.
      '/api/works/gotoubun-v03/raw/pages/0/thumb?v=c0ffee-0-1&w=256',
    ])
  })

  it('dims a volume the chosen edition does not cover, rather than hiding it', async () => {
    const host = await settleWith({ editionSelector: true })
    const select = host.querySelector('select') as HTMLSelectElement
    act(() => {
      select.value = 'en'
      select.dispatchEvent(new Event('change', { bubbles: true }))
    })
    const items = [...host.querySelectorAll('.volumes li')]
    expect(items).toHaveLength(2)
    expect(items[0]?.classList.contains('without')).toBe(false)
    expect(items[1]?.classList.contains('without')).toBe(true)
  })

  it('goes back to every edition when All is chosen again', async () => {
    const host = await settleWith({ editionSelector: true })
    const select = host.querySelector('select') as HTMLSelectElement
    act(() => {
      select.value = 'en'
      select.dispatchEvent(new Event('change', { bubbles: true }))
    })
    act(() => {
      select.value = 'all'
      select.dispatchEvent(new Event('change', { bubbles: true }))
    })
    expect(host.querySelector('.volumes img')?.getAttribute('src')).toBe(
      '/api/works/gotoubun-v01/raw/pages/0/thumb?v=c0ffee-0-1&w=256',
    )
    expect(
      [...host.querySelectorAll('.volumes li')].some((li) =>
        li.classList.contains('without'),
      ),
    ).toBe(false)
  })

  /**
   * `Cover.tsx` remembers a failed cover *by `src`* rather than as a
   * boolean, so a series whose cover URL changes is retried rather than left
   * broken for the life of the page. `Series.tsx` feeds the shown edition's
   * `cover` into a `Cover` kept mounted across an edition change by the
   * `<li key={w.slug}>` above it, so here the URL changes under a live
   * `Cover` instance, and the discriminating mutation (`if (failed === src)`
   * → `if (failed !== undefined)`) leaves the placeholder up forever once any
   * cover has ever failed, regardless of a later `src` change — so this test
   * must check both directions: the placeholder appears on failure, and a
   * *different* URL clears it again.
   */
  it('retries a volume cover under a new edition’s URL rather than leaving it broken forever', async () => {
    const host = await settleWith({ editionSelector: true })
    // The first volume, `gotoubun-v01` — the one with both a `raw` and an
    // `en` edition, so switching the selector actually changes its cover
    // URL. Scoped to this one `<li>` rather than `.volumes` as a whole:
    // `gotoubun-v03` only carries `raw`, so its `<img>` never fails and
    // would otherwise satisfy a bare `.volumes img` query regardless of
    // what happened to the first volume's cover.
    const firstLi = host.querySelectorAll('.volumes li')[0] as HTMLElement
    const imgBefore = firstLi.querySelector('img')
    expect(imgBefore?.getAttribute('src')).toBe(
      '/api/works/gotoubun-v01/raw/pages/0/thumb?v=c0ffee-0-1&w=256',
    )

    // jsdom loads no images at all, so the failure has to be delivered the
    // way the browser would deliver it.
    act(() => {
      imgBefore?.dispatchEvent(new Event('error'))
    })
    expect(firstLi.querySelector('img')).toBeNull()
    expect(firstLi.querySelector('.cover-missing')).not.toBeNull()

    const select = host.querySelector('select') as HTMLSelectElement
    act(() => {
      select.value = 'en'
      select.dispatchEvent(new Event('change', { bubbles: true }))
    })

    // A different edition is a different cover URL — the failure that was
    // remembered for the old `src` must not apply to this one.
    expect(firstLi.querySelector('.cover-missing')).toBeNull()
    expect(firstLi.querySelector('img')?.getAttribute('src')).toBe(
      '/api/works/gotoubun-v01/en/pages/0/thumb?v=c0ffee-0-1&w=256',
    )
  })

  /**
   * A link from one series to another reuses this `Series` instance, as
   * `Routes.test.tsx`'s "navigating from one work to another" test reuses
   * `Reader` when nothing keys it, so this drives a real `useNavigate()`
   * rather than a second `mount()`, which would hand back a fresh instance
   * whatever the page carried over. The chosen edition is `?edition=`, which
   * the link to the next series does not carry.
   */
  it('forgets the chosen edition when the reader navigates to a different series', async () => {
    vi.mocked(library.oneSeries).mockImplementation((id) =>
      Promise.resolve(
        id === 7
          ? {
              series: card(),
              works: [
                work({
                  editions: [
                    edition({ key: 'raw', label: 'Manga (RAW)', pages: 180 }),
                    edition({ key: 'en', label: 'Manga', pages: 178 }),
                  ],
                }),
              ],
            }
          : {
              series: card({ id: 9, title: 'A Different Series' }),
              works: [
                work({
                  id: 4,
                  slug: 'different-v01',
                  title: 'A Different Series v01',
                  editions: [edition({ key: 'raw', label: 'Manga (RAW)' })],
                }),
              ],
            },
      ),
    )

    let navigate: ReturnType<typeof useNavigate> | undefined
    function Nav() {
      navigate = useNavigate()
      return (
        <Routes>
          <Route path="/series/:id" element={<Series />} />
        </Routes>
      )
    }
    let host!: HTMLElement
    await act(async () => {
      host = mount(
        // `editionSelector` is on in `browseModule.defaults` already, so this
        // needs no override — the provider is what makes `Series` see it.
        <BrowsePrefsContext.Provider value={browseModule.defaults}>
          <MemoryRouter initialEntries={['/series/7']}>
            <Nav />
          </MemoryRouter>
        </BrowsePrefsContext.Provider>,
      )
      await Promise.resolve()
      await Promise.resolve()
    })

    const select = host.querySelector('select') as HTMLSelectElement
    act(() => {
      select.value = 'en'
      select.dispatchEvent(new Event('change', { bubbles: true }))
    })
    expect(select.value).toBe('en')

    await act(async () => {
      navigate?.('/series/9')
      await Promise.resolve()
      await Promise.resolve()
    })

    const selectAfter = host.querySelector('select') as HTMLSelectElement
    expect(selectAfter.value).toBe('all')
    expect(
      [...host.querySelectorAll('.volumes li')].some((li) =>
        li.classList.contains('without'),
      ),
    ).toBe(false)
  })

  /**
   * Under a chosen edition each tile draws that edition's `cover`, which is
   * `null` for an edition with no page, and a volume without the chosen
   * edition draws the cover of the edition it leads with, `editions[0]`.
   */
  it('draws the missing-cover box for a chosen edition with no page, and a volume without it from the edition it leads with', async () => {
    vi.mocked(library.oneSeries).mockResolvedValue({
      series: card(),
      works: [
        work({
          editions: [
            edition({ key: 'raw', label: 'Manga (RAW)', pages: 180 }),
            edition({ key: 'en', label: 'Manga', pages: 0, cover: null }),
          ],
        }),
        work({
          id: 2,
          slug: 'gotoubun-v03',
          title: 'Go-Toubun no Hanayome v03',
          volume: 3,
          editions: [
            edition({ key: 'raw', cover: coverOf('gotoubun-v03', 'raw') }),
            edition({
              key: 'fan',
              label: 'Fan',
              cover: coverOf('gotoubun-v03', 'fan'),
            }),
          ],
        }),
      ],
    })
    const host = await settleWith({ editionSelector: true })
    const select = host.querySelector('select') as HTMLSelectElement
    act(() => {
      select.value = 'en'
      select.dispatchEvent(new Event('change', { bubbles: true }))
    })
    expect(
      [...host.querySelectorAll('.volumes li')].map((li) => [
        li.querySelector('.title')?.textContent,
        li.querySelector('img')?.getAttribute('src') ?? null,
        li.querySelector('.cover-missing')?.textContent ?? null,
        li.classList.contains('without'),
      ]),
    ).toEqual([
      ['Go-Toubun no Hanayome v01', null, 'Go-Toubun no Hanayome v01', false],
      [
        'Go-Toubun no Hanayome v03',
        '/api/works/gotoubun-v03/raw/pages/0/thumb?v=c0ffee-0-1&w=256',
        null,
        true,
      ],
    ])
  })

  /**
   * **The chosen edition is in the address** (spec
   * `2026-10-28-loading-library-design.md` §5.1): `?edition=<key>`, "all" left
   * out, each change written with `replace`.
   */
  describe('in the address', () => {
    /** The address and how it was arrived at. */
    function Where() {
      const location = useLocation()
      const type = useNavigationType()
      const navigate = useNavigate()
      return (
        <>
          <output data-testid="where">
            {JSON.stringify({
              at: location.pathname + location.search,
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
      ) as { at: string; type: string }

    /**
     * The page at the second of `entries`, the first standing before it, for
     * an account that may download. Drawn again with another `editionSelector`
     * through `rerender`, the router keeps its place in the history.
     *
     * With the panel off, so each tile's download is on its cover's corner,
     * where `downloads` reads it: in `button` mode a tile draws none outside
     * its panel.
     */
    const page = (
      entries: MemoryRouterProps['initialEntries'],
      editionSelector: boolean,
    ) => (
      <BrowsePrefsContext.Provider
        value={{ ...browseModule.defaults, editionSelector, panel: 'off' }}
      >
        <GateContext.Provider value={{ who: who(), signOut: () => {} }}>
          <MemoryRouter initialEntries={entries} initialIndex={1}>
            <Routes>
              <Route path="/series/:id" element={<Series />} />
              <Route path="*" element={null} />
            </Routes>
            <Where />
          </MemoryRouter>
        </GateContext.Provider>
      </BrowsePrefsContext.Provider>
    )

    const mountAt = async (
      entries: MemoryRouterProps['initialEntries'],
      editionSelector = true,
    ) => {
      let host!: HTMLElement
      await act(async () => {
        host = mount(page(entries, editionSelector))
      })
      await flush()
      await flush()
      return host
    }

    /** Each tile by its cover's `src` and whether it is dimmed. */
    const tiles = (host: HTMLElement) =>
      [...host.querySelectorAll('.volumes li')].map((li) => [
        li.querySelector('img')?.getAttribute('src'),
        li.classList.contains('without'),
      ])

    /**
     * Each tile's download on its cover's corner, by its address, `null`
     * where it offers none.
     */
    const downloads = (host: HTMLElement) =>
      [...host.querySelectorAll('.volumes > li')].map(
        (li) =>
          li.querySelector(':scope > a.get')?.getAttribute('href') ?? null,
      )

    const choose = (host: HTMLElement, key: string) =>
      act(async () => {
        const select = host.querySelector('select') as HTMLSelectElement
        select.value = key
        select.dispatchEvent(new Event('change', { bubbles: true }))
      })

    /** Every tile drawn from the edition it leads with, none dimmed. */
    const leading = [
      ['/api/works/gotoubun-v01/raw/pages/0/thumb?v=c0ffee-0-1&w=256', false],
      ['/api/works/gotoubun-v03/raw/pages/0/thumb?v=c0ffee-0-1&w=256', false],
    ]
    const leadingDownloads = [
      '/api/works/gotoubun-v01/raw/download',
      '/api/works/gotoubun-v03/raw/download',
    ]

    it('draws every tile as with no ?edition= while the selector is off, and leaves the address as it is', async () => {
      const host = await mountAt(['/before', '/series/7?edition=en'], false)
      expect(host.querySelector('.edition-selector')).toBeNull()
      expect(tiles(host)).toEqual(leading)
      expect(downloads(host)).toEqual(leadingDownloads)
      expect(where(host)).toEqual({ at: '/series/7?edition=en', type: 'POP' })
    })

    it('keeps an edition chosen on the page in the address while the selector is switched off, and applies it again once it is back on', async () => {
      const entries = ['/before', '/series/7']
      const host = await mountAt(entries, true)
      await choose(host, 'en')
      const chosen = [
        ['/api/works/gotoubun-v01/en/pages/0/thumb?v=c0ffee-0-1&w=256', false],
        ['/api/works/gotoubun-v03/raw/pages/0/thumb?v=c0ffee-0-1&w=256', true],
      ]
      expect(where(host)).toEqual({
        at: '/series/7?edition=en',
        type: 'REPLACE',
      })
      expect(tiles(host)).toEqual(chosen)

      rerender(page(entries, false))
      await flush()
      expect(host.querySelector('.edition-selector')).toBeNull()
      expect(tiles(host)).toEqual(leading)
      expect(downloads(host)).toEqual(leadingDownloads)
      expect(where(host)).toEqual({
        at: '/series/7?edition=en',
        type: 'REPLACE',
      })

      rerender(page(entries, true))
      await flush()
      expect((host.querySelector('select') as HTMLSelectElement).value).toBe(
        'en',
      )
      expect(tiles(host)).toEqual(chosen)
      expect(where(host)).toEqual({
        at: '/series/7?edition=en',
        type: 'REPLACE',
      })
      await act(async () => {
        host.querySelector<HTMLButtonElement>('[data-testid="back"]')?.click()
      })
      expect(where(host).at).toBe('/before')
    })

    it('applies ?edition= again once the selector is switched back on', async () => {
      const entries = ['/before', '/series/7?edition=en']
      const host = await mountAt(entries, false)
      expect(tiles(host)).toEqual(leading)
      rerender(page(entries, true))
      expect((host.querySelector('select') as HTMLSelectElement).value).toBe(
        'en',
      )
      expect(tiles(host)).toEqual([
        ['/api/works/gotoubun-v01/en/pages/0/thumb?v=c0ffee-0-1&w=256', false],
        ['/api/works/gotoubun-v03/raw/pages/0/thumb?v=c0ffee-0-1&w=256', true],
      ])
      expect(downloads(host)).toEqual([
        '/api/works/gotoubun-v01/en/download',
        null,
      ])
      expect(where(host)).toEqual({ at: '/series/7?edition=en', type: 'POP' })
    })

    it('selects the edition ?edition= names on arrival', async () => {
      const host = await mountAt(['/before', '/series/7?edition=en'])
      expect((host.querySelector('select') as HTMLSelectElement).value).toBe(
        'en',
      )
      expect(tiles(host)).toEqual([
        ['/api/works/gotoubun-v01/en/pages/0/thumb?v=c0ffee-0-1&w=256', false],
        ['/api/works/gotoubun-v03/raw/pages/0/thumb?v=c0ffee-0-1&w=256', true],
      ])
    })

    it('reads an ?edition= this series does not have as All', async () => {
      const host = await mountAt(['/before', '/series/7?edition=fan'])
      expect((host.querySelector('select') as HTMLSelectElement).value).toBe(
        'all',
      )
      expect(tiles(host)).toEqual([
        ['/api/works/gotoubun-v01/raw/pages/0/thumb?v=c0ffee-0-1&w=256', false],
        ['/api/works/gotoubun-v03/raw/pages/0/thumb?v=c0ffee-0-1&w=256', false],
      ])
    })

    it('writes each choice with replace, keeping every other parameter, and leaves All out', async () => {
      const host = await mountAt(['/before', '/series/7?other=1'])
      await choose(host, 'en')
      expect(where(host)).toEqual({
        at: '/series/7?other=1&edition=en',
        type: 'REPLACE',
      })
      expect(tiles(host)).toEqual([
        ['/api/works/gotoubun-v01/en/pages/0/thumb?v=c0ffee-0-1&w=256', false],
        ['/api/works/gotoubun-v03/raw/pages/0/thumb?v=c0ffee-0-1&w=256', true],
      ])
      await choose(host, 'all')
      expect(where(host)).toEqual({ at: '/series/7?other=1', type: 'REPLACE' })
      expect(tiles(host)).toEqual([
        ['/api/works/gotoubun-v01/raw/pages/0/thumb?v=c0ffee-0-1&w=256', false],
        ['/api/works/gotoubun-v03/raw/pages/0/thumb?v=c0ffee-0-1&w=256', false],
      ])

      await act(async () => {
        host.querySelector<HTMLButtonElement>('[data-testid="back"]')?.click()
      })
      expect(where(host).at).toBe('/before')
    })
  })
})

describe('Series, the alignment panel', () => {
  beforeEach(() => {
    // Unlike the `describe`s above, these tests assert on
    // `library.work`'s call count and arguments — assertions that leak
    // across `it` blocks sharing one `vi.fn()` unless each test starts
    // clean, the same reason the grid-layout `describe` clears here.
    vi.clearAllMocks()
    vi.mocked(library.oneSeries).mockResolvedValue({
      series: card(),
      works: [
        work({ aligned: true }),
        work({ id: 2, slug: 'gotoubun-v03', volume: 3 }),
      ],
    })
    vi.mocked(library.work).mockResolvedValue({
      ...work({ aligned: true }),
      alignments: [
        {
          from: 'raw',
          to: 'en',
          verdict: 'confident',
          slots: 180,
          paired: 178,
        },
        { from: 'raw', to: 'en-fan', verdict: null, slots: null, paired: null },
      ],
    })
  })

  const open = async (host: HTMLElement, index = 0) => {
    const buttons = host.querySelectorAll('.volumes .panel-open')
    await act(async () => {
      ;(buttons[index] as HTMLButtonElement).click()
      await Promise.resolve()
      await Promise.resolve()
    })
  }

  it('offers a button per volume and fetches nothing until one is pressed', async () => {
    const host = await settleWith({ panel: 'button' })
    expect(host.querySelectorAll('.volumes .panel-open')).toHaveLength(2)
    expect(vi.mocked(library.work)).not.toHaveBeenCalled()
  })

  it('shows the verdict and the paired count, which no list route carries', async () => {
    const host = await settleWith({ panel: 'button' })
    await open(host)
    const rows = [...host.querySelectorAll('.alignment-panel li')].map(
      (li) => li.textContent,
    )
    expect(rows[0]).toContain('raw → en')
    expect(rows[0]).toContain('confident')
    expect(rows[0]).toContain('178 of 180')
  })

  it('says an unaligned pair is unaligned rather than printing null', async () => {
    const host = await settleWith({ panel: 'button' })
    await open(host)
    const rows = [...host.querySelectorAll('.alignment-panel li')].map(
      (li) => li.textContent,
    )
    expect(rows[1]).toContain('raw → en-fan')
    expect(rows[1]).toContain('not aligned')
    expect(rows[1]).not.toContain('null')
  })

  it('fetches one volume once, however many times it is opened', async () => {
    const host = await settleWith({ panel: 'button' })
    await open(host)
    await open(host)
    expect(vi.mocked(library.work)).toHaveBeenCalledTimes(1)
    expect(vi.mocked(library.work)).toHaveBeenCalledWith('gotoubun-v01')

    // The two opens above are the same volume, so a cache keyed wrong —
    // by index, or by a single "have I fetched anything yet" flag rather
    // than by slug — would still satisfy both assertions above. Only a
    // *different* volume tells a correct per-slug cache apart from one of
    // those: it must still fetch, and by its own slug.
    await open(host, 1)
    expect(vi.mocked(library.work)).toHaveBeenCalledTimes(2)
    expect(vi.mocked(library.work)).toHaveBeenCalledWith('gotoubun-v03')
  })

  it('offers no button at all when the panel is off', async () => {
    const host = await settleWith({ panel: 'off' })
    expect(host.querySelector('.panel-open')).toBeNull()
    expect(host.querySelector('.alignment-panel')).toBeNull()
  })

  it('opens on focus when it is set to follow the selection', async () => {
    const host = await settleWith({ panel: 'on-select' })
    expect(host.querySelector('.panel-open')).toBeNull()
    // The *second* link, not the first — so "a panel opened somewhere"
    // cannot pass for "the panel opened for the volume that was actually
    // focused". An implementation that always reveals the first volume
    // regardless of focus target would still satisfy a weaker assertion
    // that only checks `.alignment-panel` exists.
    const links = host.querySelectorAll('.volumes a')
    const link = links[1] as HTMLAnchorElement
    await act(async () => {
      link.dispatchEvent(new FocusEvent('focusin', { bubbles: true }))
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(host.querySelector('.alignment-panel')).not.toBeNull()
    expect(vi.mocked(library.work)).toHaveBeenCalledWith('gotoubun-v03')
    expect(vi.mocked(library.work)).not.toHaveBeenCalledWith('gotoubun-v01')
  })

  it('still navigates on click even though on-select also listens for hover and focus', async () => {
    const host = await settleWith({ panel: 'on-select' })
    const link = host.querySelector('.volumes a') as HTMLAnchorElement
    await act(async () => {
      link.click()
      await Promise.resolve()
      await Promise.resolve()
    })
    // No route past `/series/:id` is registered in this harness, so a real
    // navigation leaves nothing here to render. If a hover or focus
    // handler had swallowed the click — `stopPropagation` ahead of
    // react-router's own listener, or a `preventDefault` added here — the
    // series page would still be showing.
    expect(host.querySelector('h1')).toBeNull()
  })

  it('says so when the detail cannot be read', async () => {
    vi.mocked(library.work).mockRejectedValue(
      new Error('/api/library/works/gotoubun-v01: 500'),
    )
    const host = await settleWith({ panel: 'button' })
    await open(host)
    expect(host.querySelector('.alignment-panel')?.textContent).toContain('500')
  })

  it('fetches once, not twice, when hover and focus both land on the same tile before the request settles', async () => {
    // A real mouse click first moves focus to the link, then fires the
    // click — so "hover onto a tile, then click it" routes through both
    // `onMouseEnter` and `onFocusCapture` before the first request can
    // possibly have returned. React's `onMouseEnter` listens for the
    // native `mouseover` event, not `mouseenter` — jsdom does not dispatch
    // `mouseenter` in a way React's delegated listener observes, so this
    // must dispatch `mouseover`.
    const host = await settleWith({ panel: 'on-select' })
    const link = host.querySelectorAll('.volumes a')[0] as HTMLAnchorElement
    act(() => {
      link.dispatchEvent(new MouseEvent('mouseover', { bubbles: true }))
    })
    act(() => {
      link.dispatchEvent(new FocusEvent('focusin', { bubbles: true }))
    })
    // Both handlers have already run synchronously, and the mocked
    // `work()` promise has not had a microtask tick to resolve yet — so
    // the panel must still read as loading, not as some sentinel a
    // same-slug in-flight guard might have written in its place.
    expect(host.querySelector('.alignment-panel')?.textContent).toContain(
      'Reading',
    )
    await act(async () => {
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(vi.mocked(library.work)).toHaveBeenCalledTimes(1)
  })

  it('opens on hover when it is set to follow the selection', async () => {
    const host = await settleWith({ panel: 'on-select' })
    expect(host.querySelector('.panel-open')).toBeNull()
    // The *second* link, not the first — same reasoning as the focus test
    // above: "a panel opened somewhere" must not pass for "the panel
    // opened for the volume actually hovered".
    const links = host.querySelectorAll('.volumes a')
    const link = links[1] as HTMLAnchorElement
    await act(async () => {
      link.dispatchEvent(new MouseEvent('mouseover', { bubbles: true }))
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(host.querySelector('.alignment-panel')).not.toBeNull()
    expect(vi.mocked(library.work)).toHaveBeenCalledWith('gotoubun-v03')
    expect(vi.mocked(library.work)).not.toHaveBeenCalledWith('gotoubun-v01')
  })

  /**
   * The toggle belongs at the button's own `onClick`, not inside `reveal`:
   * `reveal(slug)` always asks for `slug`'s panel, and `on-select`'s hover and
   * focus paths call `reveal` too, so a toggle there would close the panel the
   * second time a pointer merely passed back over the same tile. The button's
   * toggle is what lets the panel close once any button has opened it.
   */
  it('closes on a second press, and reopening costs no second request', async () => {
    const host = await settleWith({ panel: 'button' })
    const button = host.querySelectorAll(
      '.volumes .panel-open',
    )[0] as HTMLButtonElement

    await open(host)
    expect(button.getAttribute('aria-expanded')).toBe('true')
    expect(host.querySelector('.alignment-panel')).not.toBeNull()

    await act(async () => {
      button.click()
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(button.getAttribute('aria-expanded')).toBe('false')
    expect(host.querySelector('.alignment-panel')).toBeNull()

    // Reopening must not refetch — the cache from the first open still holds.
    await open(host)
    expect(button.getAttribute('aria-expanded')).toBe('true')
    expect(host.querySelector('.alignment-panel')).not.toBeNull()
    expect(vi.mocked(library.work)).toHaveBeenCalledTimes(1)
  })

  it('renders a verdict with no count when the pair has a verdict but no slot data', async () => {
    // `AlignmentPanel` guards the count span with
    // `a.paired !== null && a.slots !== null`; the shared fixture above
    // only ever pairs a non-null verdict with non-null counts, or a null
    // verdict with null everything, so that guard is never exercised.
    // This fixture is the missing combination: a real verdict whose pair
    // still has no slot data.
    vi.mocked(library.work).mockResolvedValue({
      ...work({ aligned: true }),
      alignments: [
        { from: 'raw', to: 'en', verdict: 'weak', slots: null, paired: null },
      ],
    })
    const host = await settleWith({ panel: 'button' })
    await open(host)
    const row = host.querySelector('.alignment-panel li')
    expect(row?.textContent).toContain('weak')
    expect(row?.querySelector('.paired')).toBeNull()
  })

  /**
   * `derive` never writes an alignment itself, so `alignments.length === 0`
   * is the state of *every* volume in a freshly scanned library, however many
   * editions it holds. The panel must tell "truly nothing to pair" (fewer
   * than two editions) apart from "has something to pair and simply has not
   * been aligned yet" (two or more). This test is the first case, where the
   * existing sentence is correct.
   */
  it('says a single-edition volume has nothing to pair, when that is true', async () => {
    vi.mocked(library.oneSeries).mockResolvedValue({
      series: card(),
      works: [work({ editions: [edition()] })],
    })
    vi.mocked(library.work).mockResolvedValue({
      ...work({ editions: [edition()] }),
      alignments: [],
    })
    const host = await settleWith({ panel: 'button' })
    await open(host)
    const text = host.querySelector('.alignment-panel')?.textContent
    expect(text).toContain('one edition')
  })

  /**
   * The second case: a volume with two editions and no alignments is not
   * "one edition, nothing to pair with" — that sentence is false on this
   * fixture. The table layout renders the same two editions as two columns
   * on one screen, so the wrong sentence and the correct data would have
   * appeared together.
   */
  it('does not claim a single edition when the volume has two and simply has not been aligned', async () => {
    const editions = [edition(), edition({ key: 'en', label: 'English' })]
    vi.mocked(library.oneSeries).mockResolvedValue({
      series: card(),
      works: [work({ editions })],
    })
    vi.mocked(library.work).mockResolvedValue({
      ...work({ editions }),
      alignments: [],
    })
    const host = await settleWith({ panel: 'button' })
    await open(host)
    const text = host.querySelector('.alignment-panel')?.textContent
    expect(text).not.toContain('one edition')
    expect(text).toContain('taiyaku align')
  })
})

/**
 * **A read for a series the reader has left, answering late.**
 *
 * A link from one series to another reuses this instance rather than
 * remounting it, the same thing the edition-selector test above drives, so
 * the departed series' read can still be out, and the server need not answer
 * in order. The store keeps each answer under its own series' address, and
 * the page draws only the address it names now, so the departed series'
 * answer or failure, landing late, is never drawn under the heading of the
 * one the reader asked for.
 *
 * `deferred` rather than two `mockResolvedValueOnce`s: mocked promises resolve
 * in call order, so no answer would land after its series was left.
 */
describe('Series, a load for a series the reader has already left', () => {
  const twoSeries = async (
    first: Promise<{ series: library.SeriesDetail; works: library.WorkCard[] }>,
  ) => {
    const second = deferred<{
      series: library.SeriesDetail
      works: library.WorkCard[]
    }>()
    vi.mocked(library.oneSeries)
      .mockReturnValueOnce(first)
      .mockReturnValueOnce(second.promise)

    let navigate: ReturnType<typeof useNavigate> | undefined
    function Nav() {
      navigate = useNavigate()
      return (
        <Routes>
          <Route path="/series/:id" element={<Series />} />
        </Routes>
      )
    }
    let host!: HTMLElement
    await act(async () => {
      host = mount(
        <BrowsePrefsContext.Provider value={browseModule.defaults}>
          <MemoryRouter initialEntries={['/series/7']}>
            <Nav />
          </MemoryRouter>
        </BrowsePrefsContext.Provider>,
      )
      await Promise.resolve()
      await Promise.resolve()
    })
    await flush(() => navigate?.('/series/9'))
    await flush(() =>
      second.resolve({
        series: card({ id: 9, title: 'A Different Series' }),
        works: [work({ id: 4, slug: 'different-v01' })],
      }),
    )
    expect(host.querySelector('h1')?.textContent).toBe('A Different Series')
    return host
  }

  it('is not drawn when it answers after the reader has moved on', async () => {
    const first = deferred<{
      series: library.SeriesDetail
      works: library.WorkCard[]
    }>()
    const host = await twoSeries(first.promise)
    await flush(() => first.resolve({ series: card(), works: [work()] }))
    expect(host.querySelector('h1')?.textContent).toBe('A Different Series')
  })

  it('does not put its failure over the series that loaded', async () => {
    const first = deferred<{
      series: library.SeriesDetail
      works: library.WorkCard[]
    }>()
    const host = await twoSeries(first.promise)
    await flush(() => first.reject(new Error('/api/library/series/7: 500')))
    expect(host.querySelector('[role="alert"]')).toBeNull()
    expect(host.querySelector('h1')?.textContent).toBe('A Different Series')
  })
})

/**
 * **The series is read through the store** (spec
 * `2026-10-28-loading-library-design.md` §6.3): a return draws what was kept
 * and reads it again, and a move to a series not kept draws nothing of the one
 * left while the next loads.
 */
describe('Series, read through the store', () => {
  const seven: SeriesPage = {
    series: card(),
    works: [
      work(),
      work({
        id: 2,
        slug: 'gotoubun-v03',
        title: 'Go-Toubun no Hanayome v03',
        volume: 3,
      }),
    ],
  }
  const nine: SeriesPage = {
    series: card({ id: 9, title: 'A Different Series' }),
    works: [
      work({
        id: 4,
        slug: 'different-v01',
        title: 'A Different Series v01',
      }),
    ],
  }

  /**
   * Each heading by its words and class, the wait line by its words, and
   * each volume tile by its title and its cover's `src`, `null` where it
   * draws none.
   */
  const drawn = (host: HTMLElement) =>
    JSON.stringify({
      headings: [...host.querySelectorAll('h1')].map((h) => [
        h.textContent,
        h.className,
      ]),
      waiting: [...host.querySelectorAll('[role="status"]')].map(
        (line) => line.textContent,
      ),
      volumes: [...host.querySelectorAll('.volumes li')].map((li) => [
        li.querySelector('.title')?.textContent,
        li.querySelector('img')?.getAttribute('src') ?? null,
      ]),
    })

  const sevenDrawn = JSON.stringify({
    headings: [['Go-Toubun no Hanayome', '']],
    waiting: [],
    volumes: [
      [
        'Go-Toubun no Hanayome v01',
        '/api/works/gotoubun-v01/raw/pages/0/thumb?v=c0ffee-0-1&w=256',
      ],
      [
        'Go-Toubun no Hanayome v03',
        '/api/works/gotoubun-v03/raw/pages/0/thumb?v=c0ffee-0-1&w=256',
      ],
    ],
  })
  const nineDrawn = JSON.stringify({
    headings: [['A Different Series', '']],
    waiting: [],
    volumes: [
      [
        'A Different Series v01',
        '/api/works/different-v01/raw/pages/0/thumb?v=c0ffee-0-1&w=256',
      ],
    ],
  })
  const nothing = JSON.stringify({
    headings: [['Series', 'visually-hidden']],
    waiting: [],
    volumes: [],
  })
  const waiting = JSON.stringify({
    headings: [['Series', 'visually-hidden']],
    waiting: ['Loading…'],
    volumes: [],
  })

  let navigate: ReturnType<typeof useNavigate> | undefined
  function Nav() {
    navigate = useNavigate()
    return (
      <Routes>
        <Route path="/series/:id" element={<Series />} />
      </Routes>
    )
  }
  /** A series page at `path`, with `after` drawn after it. */
  const screenAt = (path: string, after?: React.ReactNode) => (
    <BrowsePrefsContext.Provider value={browseModule.defaults}>
      <MemoryRouter initialEntries={[path]}>
        <Nav />
        {after}
      </MemoryRouter>
    </BrowsePrefsContext.Provider>
  )

  beforeEach(() => {
    vi.clearAllMocks()
  })

  it('draws the kept series in its first render on a remount, each tile with the cover it had, and reads it again once', async () => {
    vi.mocked(library.oneSeries).mockResolvedValue(seven)
    let host!: HTMLElement
    await act(async () => {
      host = mount(screenAt('/series/7'))
    })
    await flush()
    await flush()
    expect(drawn(host)).toBe(sevenDrawn)

    rerender(<div />)
    vi.mocked(library.oneSeries).mockClear()
    vi.mocked(library.oneSeries).mockReturnValueOnce(
      deferred<SeriesPage>().promise,
    )
    const first: { drawn?: string } = {}
    rerender(
      screenAt(
        '/series/7',
        <FirstCommit read={() => drawn(host)} into={first} />,
      ),
    )
    expect(first.drawn).toBe(sevenDrawn)
    expect(drawn(host)).toBe(sevenDrawn)
    await flush()
    await flush()
    expect(drawn(host)).toBe(sevenDrawn)
    expect(vi.mocked(library.oneSeries).mock.calls).toEqual([[7, undefined]])
  })

  it('draws nothing of a kept series marked stale before a return, and draws its re-read once it lands', async () => {
    vi.mocked(library.oneSeries).mockResolvedValue(seven)
    let host!: HTMLElement
    await act(async () => {
      host = mount(screenAt('/series/7'))
    })
    await flush()
    await flush()
    expect(drawn(host)).toBe(sevenDrawn)

    rerender(<div />)
    invalidateLibraryViews()
    vi.mocked(library.oneSeries).mockClear()
    const again = deferred<SeriesPage>()
    vi.mocked(library.oneSeries).mockReturnValueOnce(again.promise)
    rerender(screenAt('/series/7'))
    expect(drawn(host)).toBe(nothing)
    await flush(() => again.resolve(seven))
    await flush()
    expect(drawn(host)).toBe(sevenDrawn)
    expect(vi.mocked(library.oneSeries).mock.calls).toEqual([[7, undefined]])
  })

  it('draws nothing of the series left on a move to one not kept, Loading… from 200 ms, and the next series in one step once it lands', async () => {
    holdTheClock()
    const next = deferred<SeriesPage>()
    vi.mocked(library.oneSeries).mockImplementation((id) =>
      id === 7 ? Promise.resolve(seven) : next.promise,
    )
    let host!: HTMLElement
    await act(async () => {
      host = mount(screenAt('/series/7'))
    })
    await flush()
    await flush()
    expect(drawn(host)).toBe(sevenDrawn)

    await flush(() => {
      void navigate?.('/series/9')
    })
    expect(drawn(host)).toBe(nothing)
    pass(WAIT_DRAWN_AFTER_MS - 1)
    expect(drawn(host)).toBe(nothing)
    pass(1)
    expect(drawn(host)).toBe(waiting)
    expect(vi.mocked(library.oneSeries).mock.calls).toEqual([
      [7, undefined],
      [9, undefined],
    ])

    const seen = watchDrawn(() => drawn(host))
    await flush(() => next.resolve(nine))
    await flush()
    seen.stop()
    expect([...seen.seen]).toEqual([waiting, nineDrawn])
  })
})

describe('Series, its View panel — spec §2', () => {
  beforeEach(() => {
    vi.mocked(library.oneSeries).mockResolvedValue({
      series: card(),
      works: [work()],
    })
  })

  /** Each row of the open panel: its words, its kind and its choices. */
  const rows = (panel: HTMLElement) =>
    [...panel.querySelectorAll('label')].map((label) => {
      const control = label.querySelector('input, select')
      return [
        label.querySelector('span')?.textContent,
        control instanceof HTMLInputElement ? control.type : control?.tagName,
        [...label.querySelectorAll('option')].map((o) => o.textContent),
      ]
    })

  it('holds Layout, Edition selector, Panel, Density and Reset, in sentence case', async () => {
    let host!: HTMLElement
    await act(async () => {
      host = mount(
        <BrowseControlsContext.Provider
          value={{ change: () => {}, reset: () => {} }}
        >
          <MemoryRouter initialEntries={['/series/7']}>
            <Routes>
              <Route path="/series/:id" element={<Series />} />
            </Routes>
          </MemoryRouter>
        </BrowseControlsContext.Provider>,
      )
    })
    await flush()
    const button = host.querySelector<HTMLButtonElement>(
      '.page-head .view-button',
    )
    if (!button) throw new Error('no View button beside the title')
    act(() => {
      button.click()
    })
    const panel = document.getElementById(
      button.getAttribute('aria-controls') ?? '',
    )
    if (!panel || panel.hidden) throw new Error('View opened nothing')
    expect(rows(panel)).toEqual([
      ['Layout', 'SELECT', ['Grid', 'Table']],
      ['Edition selector', 'checkbox', []],
      ['Panel', 'SELECT', ['Off', 'Button', 'On-select']],
      ['Density', 'SELECT', ['Compact', 'Comfortable']],
    ])
    expect(
      [...panel.querySelectorAll('button')].map((b) => b.textContent),
    ).toEqual(['Reset'])
  })
})

/**
 * The open panel is laid at once and again from one `ResizeObserver` on its
 * tile, and on nothing else (O283). jsdom has no observer, so a stub stands
 * in: it keeps what each one watches and whether it is still connected, and
 * a case reports a resize through it.
 */
describe('Series, the open panel’s observer (O283)', () => {
  class StubResizeObserver {
    static made: StubResizeObserver[] = []
    watched: Element[] = []
    connected = true
    report: () => void
    constructor(report: () => void) {
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
      this.connected = false
      this.watched = []
    }
  }

  /** What each observer still connected watches. */
  const watching = () =>
    StubResizeObserver.made
      .filter((each) => each.connected)
      .map((each) => each.watched)

  beforeEach(() => {
    vi.clearAllMocks()
    StubResizeObserver.made = []
    vi.stubGlobal('ResizeObserver', StubResizeObserver)
    vi.mocked(library.work).mockResolvedValue({ ...work(), alignments: [] })
    vi.mocked(library.oneSeries).mockResolvedValue({
      series: card(),
      works: [
        work(),
        work({
          id: 2,
          slug: 'gotoubun-v03',
          title: 'Go-Toubun no Hanayome v03',
          volume: 3,
        }),
      ],
    })
  })
  afterEach(() => {
    vi.unstubAllGlobals()
  })

  it('watches the open tile alone, with one observer, until its panel closes or another opens', async () => {
    // A reader, so the panel holds no editor and its Title field's own
    // observer.
    const host = await settleWith({ panel: 'button' }, who())
    const tiles = [...host.querySelectorAll('.volumes > li')]
    expect(watching(), 'nothing open').toEqual([])

    await openTile(host, 0)
    expect(watching(), 'the first open').toEqual([[tiles[0]]])

    await openTile(host, 1)
    expect(watching(), 'the second open').toEqual([[tiles[1]]])

    await act(async () => {
      tiles[1].querySelector<HTMLButtonElement>('.panel-open')?.click()
      await Promise.resolve()
    })
    expect(tiles[1].querySelector('.tile-panel'), 'closed').toBeNull()
    expect(watching(), 'none open').toEqual([])
  })

  it('lays the panel from its tile as it opens, and again at each report', async () => {
    const host = await settleWith({ panel: 'button' }, who())
    const tile = await openTile(host, 0)
    const panel = tile.querySelector('.tile-panel')
    expect(vi.mocked(tilePanel.layPanel).mock.calls).toEqual([[panel, tile]])

    const [observer] = StubResizeObserver.made
    observer.report()
    expect(vi.mocked(tilePanel.layPanel).mock.calls).toEqual([
      [panel, tile],
      [panel, tile],
    ])
  })
})

/**
 * The panel's closes (O283): Escape, a press outside, `i` again, and in
 * `button` mode focus arriving outside; each waits for the Title field's
 * save, so none loses a typed title. jsdom has no pointer, so a press is
 * dispatched where it would land, as `Sidebar.test.tsx` does;
 * `Series.panel.browser.test.tsx` presses with a real mouse.
 */
describe('Series, closing a volume’s panel (O283)', () => {
  const renamed = 'Go-Toubun no Hanayome, volume 1'
  const third = work({
    id: 2,
    slug: 'gotoubun-v03',
    title: 'Go-Toubun no Hanayome v03',
    volume: 3,
  })
  const stored = (title: string): metadata.PatchResult => ({
    taiyaku_library: 1,
    updated: '2026-11-14T00:00:00Z',
    title,
    persisted: true,
  })

  beforeEach(() => {
    vi.clearAllMocks()
    // Reset, not only cleared: a once-answer a case left unused would
    // otherwise answer the next case's first call.
    vi.mocked(metadata.patchWork).mockReset()
    vi.mocked(library.oneSeries).mockReset()
    vi.mocked(library.work).mockResolvedValue({ ...work(), alignments: [] })
  })

  /**
   * The page in `panel` mode for an administrator who may download, in
   * `.library > main` as the shell draws it, with View's controls, so
   * `.view-button` is drawn beside the title.
   */
  async function page(
    panel: 'button' | 'on-select',
    works = [work(), third],
  ): Promise<HTMLElement> {
    vi.mocked(library.oneSeries).mockResolvedValue({ series: card(), works })
    let host: HTMLElement | undefined
    await act(async () => {
      host = mount(
        <BrowsePrefsContext.Provider
          value={{ ...browseModule.defaults, panel }}
        >
          <BrowseControlsContext.Provider
            value={{ change: () => {}, reset: () => {} }}
          >
            <GateContext.Provider
              value={{ who: who({ admin: true }), signOut: () => {} }}
            >
              <div className="library">
                <main>
                  <MemoryRouter initialEntries={['/series/7']}>
                    <Routes>
                      <Route path="/series/:id" element={<Series />} />
                    </Routes>
                  </MemoryRouter>
                </main>
              </div>
            </GateContext.Provider>
          </BrowseControlsContext.Provider>
        </BrowsePrefsContext.Provider>,
      )
      await Promise.resolve()
      await Promise.resolve()
    })
    if (!host) throw new Error('nothing mounted')
    expect(tilesOf(host)).toHaveLength(works.length)
    return host
  }

  const tilesOf = (host: HTMLElement) => [
    ...host.querySelectorAll<HTMLElement>('.volumes > li'),
  ]
  const need = <T extends Element>(el: T | null | undefined, what: string) => {
    if (!el) throw new Error(`no ${what}`)
    return el
  }
  const panelIn = (tile: Element) => tile.querySelector('.tile-panel')
  const iOf = (tile: Element) =>
    need(tile.querySelector<HTMLButtonElement>(':scope > .panel-open'), 'i')
  const linkOf = (tile: Element) =>
    need(tile.querySelector<HTMLAnchorElement>(':scope > a:not(.get)'), 'link')
  const fieldOf = (tile: Element) =>
    need(
      tile.querySelector<HTMLTextAreaElement>(
        ".tile-panel textarea[name='title']",
      ),
      'Title field',
    )
  const downloadIn = (tile: Element) =>
    need(tile.querySelector<HTMLAnchorElement>('.tile-panel a.get'), '↓')
  const headingOf = (host: HTMLElement) =>
    need(host.querySelector<HTMLElement>('h1'), 'heading')
  const viewOf = (host: HTMLElement) =>
    need(host.querySelector<HTMLButtonElement>('.view-button'), 'View')
  const calls = () => vi.mocked(metadata.patchWork).mock.calls

  const focusOn = (el: HTMLElement) =>
    flush(() => {
      el.focus()
    })
  /** A key pressed where focus is, as a keyboard sends it. */
  const key = (init: KeyboardEventInit) =>
    flush(() => {
      ;(document.activeElement ?? document.body).dispatchEvent(
        new KeyboardEvent('keydown', {
          bubbles: true,
          cancelable: true,
          ...init,
        }),
      )
    })
  const pressEscape = (init: KeyboardEventInit = {}) =>
    key({ key: 'Escape', ...init })
  const pressOn = (target: Element) =>
    flush(() => {
      target.dispatchEvent(new MouseEvent('pointerdown', { bubbles: true }))
    })
  /** React's `onMouseEnter` listens for `mouseover`. */
  const hoverOn = (target: Element) =>
    flush(() => {
      target.dispatchEvent(new MouseEvent('mouseover', { bubbles: true }))
    })

  /** The first tile's panel open in `button` mode, a title typed in it. */
  async function typedIn(title = renamed) {
    const host = await page('button')
    const tile = await openTile(host, 0)
    const field = fieldOf(tile)
    await focusOn(field)
    await typeInto(field, title)
    return { host, tile, field }
  }

  it('closes on Escape and sends focus back to i', async () => {
    const host = await page('button')
    const tile = await openTile(host, 0)
    await focusOn(downloadIn(tile))
    await pressEscape()
    expect(host.querySelector('.tile-panel')).toBeNull()
    expect(document.activeElement).toBe(iOf(tile))
  })

  it('puts the title back on an Escape in the Title field, and closes on the next', async () => {
    const { tile, field } = await typedIn()
    await pressEscape()
    expect({
      open: panelIn(tile) !== null,
      value: field.value,
      focused: document.activeElement === field,
    }).toEqual({
      open: true,
      value: 'Go-Toubun no Hanayome v01',
      focused: true,
    })

    await pressEscape()
    expect(panelIn(tile)).toBeNull()
    expect(document.activeElement).toBe(iOf(tile))
    expect(calls()).toEqual([])
  })

  it('stops the Escape that closes it on the way down', async () => {
    const host = await page('button')
    const heard: string[] = []
    const hear = (event: KeyboardEvent) => {
      heard.push(event.key)
    }
    document.addEventListener('keydown', hear)
    onTestFinished(() => {
      document.removeEventListener('keydown', hear)
    })
    const tile = await openTile(host, 0)
    await focusOn(downloadIn(tile))
    await pressEscape()
    expect(panelIn(tile)).toBeNull()
    expect(heard, 'the Escape that closed it').toEqual([])

    // With no panel open, the same Escape reaches the document.
    await pressEscape()
    expect(heard, 'an Escape with none open').toEqual(['Escape'])
  })

  it.each([
    ['isComposing', { isComposing: true }],
    ['keyCode 229', { keyCode: 229 }],
  ] as const)(
    'leaves an Escape that ends a composition alone (%s)',
    async (_, init) => {
      const host = await page('button')
      const tile = await openTile(host, 0)
      await focusOn(downloadIn(tile))
      await pressEscape(init)
      expect(panelIn(tile)).not.toBeNull()
    },
  )

  it('closes on a press outside its tile and panel, and not on one inside', async () => {
    const host = await page('button')
    const tile = await openTile(host, 0)
    for (const inside of [
      fieldOf(tile),
      need(tile.querySelector('.alignment-panel'), 'alignment detail'),
      iOf(tile),
      linkOf(tile),
    ]) {
      await pressOn(inside)
    }
    expect(panelIn(tile), 'after presses inside').not.toBeNull()
    await pressOn(headingOf(host))
    expect(panelIn(tile), 'after a press outside').toBeNull()
  })

  it('closes when focus arrives outside it in button mode, and not in on-select', async () => {
    const button = await page('button')
    const tile = await openTile(button, 0)
    await focusOn(fieldOf(tile))
    expect(panelIn(tile), 'focus inside').not.toBeNull()
    await focusOn(viewOf(button))
    expect(panelIn(tile), 'button mode').toBeNull()

    const onSelect = await page('on-select')
    const [first] = tilesOf(onSelect)
    await hoverOn(linkOf(first))
    expect(panelIn(first), 'opened by the pointer').not.toBeNull()
    await focusOn(viewOf(onSelect))
    expect(panelIn(first), 'on-select mode').not.toBeNull()
  })

  it('closes an on-select panel on Escape with focus back on the tile’s link, and leaves it closed', async () => {
    const host = await page('on-select')
    const [tile] = tilesOf(host)
    await hoverOn(linkOf(tile))
    await focusOn(fieldOf(tile))
    // The field's own Escape: the panel stays.
    await pressEscape()
    expect(panelIn(tile)?.contains(document.activeElement)).toBe(true)

    await pressEscape()
    expect(document.activeElement).toBe(linkOf(tile))
    await flush()
    expect(host.querySelector('.tile-panel')).toBeNull()
  })

  it('closes an unreadable tile’s on-select panel on Escape with focus still in the page', async () => {
    const lost = work({ editions: [edition({ missing: 2 })] })
    const host = await page('on-select', [lost, third])
    const [tile] = tilesOf(host)
    const box = need(
      tile.querySelector<HTMLElement>(':scope > .tile-unreadable'),
      'unreadable box',
    )
    await hoverOn(box)
    // The fixture tells: the panel is open, and the tile has no link and
    // no ↓ to send focus to.
    expect(panelIn(tile)).not.toBeNull()
    expect(tile.querySelector('a')).toBeNull()

    await pressEscape()
    expect(document.activeElement).toBe(box)
    await flush()
    expect(host.querySelector('.tile-panel')).toBeNull()
  })

  it('closes an unreadable tile’s on-select panel on Escape with focus on its corner ↓, where it has one', async () => {
    // Opens the broken raw edition, and shows the whole English one.
    const lost = work({
      editions: [
        edition({ key: 'en', label: 'English' }),
        edition({ missing: 2 }),
      ],
      opens: 'raw',
    })
    const host = await page('on-select', [lost, third])
    const [tile] = tilesOf(host)
    await hoverOn(
      need(tile.querySelector(':scope > .tile-unreadable'), 'unreadable box'),
    )
    const get = need(tile.querySelector<HTMLElement>(':scope > a.get'), '↓')
    // The fixture tells: the panel is open, and the tile has no link.
    expect(panelIn(tile)).not.toBeNull()
    expect(tile.querySelector(':scope > a:not(.get)')).toBeNull()

    await pressEscape()
    expect(document.activeElement).toBe(get)
    await flush()
    expect(host.querySelector('.tile-panel')).toBeNull()
  })

  it('keeps an on-select panel when the pointer leaves its tile', async () => {
    const host = await page('on-select')
    const [tile] = tilesOf(host)
    const link = linkOf(tile)
    await hoverOn(link)
    expect(panelIn(tile), 'opened by the pointer').not.toBeNull()
    await flush(() => {
      link.dispatchEvent(
        new MouseEvent('mouseout', {
          bubbles: true,
          relatedTarget: headingOf(host),
        }),
      )
    })
    expect(panelIn(tile)).not.toBeNull()
  })

  it.each([
    [
      'Escape after focus moves to ↓ Download',
      async (_: HTMLElement, tile: Element) => {
        await focusOn(downloadIn(tile))
        await pressEscape()
      },
      'closed',
    ],
    [
      'a press outside with focus still in the field',
      async (host: HTMLElement) => {
        await pressOn(headingOf(host))
      },
      'closed',
    ],
    [
      'a click on i',
      async (_: HTMLElement, tile: Element) => {
        await press(iOf(tile))
      },
      'closed',
    ],
    [
      'focus arriving on .view-button',
      async (host: HTMLElement) => {
        await focusOn(viewOf(host))
      },
      'closed',
    ],
    [
      'a click on the second tile’s i',
      async (host: HTMLElement) => {
        await press(iOf(tilesOf(host)[1]))
      },
      'second open',
    ],
  ] as const)(
    'saves a changed title before the panel closes, on %s',
    async (_, close, end) => {
      const save = deferred<metadata.PatchResult>()
      vi.mocked(metadata.patchWork).mockReturnValueOnce(save.promise)
      const { host, tile } = await typedIn()
      await close(host, tile)
      const sent = [['gotoubun-v01', { title: renamed }]]
      expect({ drawn: panelIn(tile) !== null, calls: calls() }).toEqual({
        drawn: true,
        calls: sent,
      })

      await flush(() => {
        save.resolve(stored(renamed))
      })
      await flush()
      const [first, second] = tilesOf(host)
      expect({
        first: panelIn(first) !== null,
        second: panelIn(second) !== null,
        calls: calls(),
      }).toEqual({
        first: false,
        second: end === 'second open',
        calls: sent,
      })
    },
  )

  it('keeps the panel open, its failure under the field, when the save a close needs fails', async () => {
    const save = deferred<metadata.PatchResult>()
    vi.mocked(metadata.patchWork).mockReturnValueOnce(save.promise)
    const { host, tile } = await typedIn()
    await pressOn(headingOf(host))
    await flush(() => {
      save.reject(new Error('/api/library/works/gotoubun-v01: 500'))
    })
    await flush()
    expect(panelIn(tile)).not.toBeNull()
    expect(tile.querySelector('.tile-panel p.failure')?.textContent).toBe(
      '/api/library/works/gotoubun-v01: 500',
    )
  })

  it('moves an on-select panel only once the save out has landed', async () => {
    const save = deferred<metadata.PatchResult>()
    vi.mocked(metadata.patchWork).mockReturnValueOnce(save.promise)
    const host = await page('on-select')
    const [first, second] = tilesOf(host)
    await hoverOn(linkOf(first))
    const field = fieldOf(first)
    await focusOn(field)
    await typeInto(field, renamed)
    await flush(() => {
      field.blur()
    })
    expect(calls()).toEqual([['gotoubun-v01', { title: renamed }]])

    await hoverOn(linkOf(second))
    expect({
      first: panelIn(first) !== null,
      second: panelIn(second) !== null,
    }).toEqual({ first: true, second: false })

    await flush(() => {
      save.resolve(stored(renamed))
    })
    await flush()
    expect({
      first: panelIn(first) !== null,
      second: panelIn(second) !== null,
      calls: calls().length,
    }).toEqual({ first: false, second: true, calls: 1 })
  })

  it('sends nothing when the pointer or focus comes back to the open on-select panel’s tile', async () => {
    vi.mocked(metadata.patchWork).mockRejectedValueOnce(
      new Error('/api/library/works/gotoubun-v01: 500'),
    )
    const host = await page('on-select')
    const [tile] = tilesOf(host)
    const link = linkOf(tile)
    await hoverOn(link)
    const field = fieldOf(tile)
    await focusOn(field)
    await typeInto(field, renamed)
    await flush(() => {
      field.blur()
    })
    // The fixture tells: the save failed, and the field holds the title
    // it did not store.
    expect({
      calls: calls().length,
      failure: tile.querySelector('.tile-panel p.failure')?.textContent,
      value: field.value,
    }).toEqual({
      calls: 1,
      failure: '/api/library/works/gotoubun-v01: 500',
      value: renamed,
    })

    await hoverOn(link)
    await focusOn(link)
    await focusOn(field)
    expect({ open: panelIn(tile) !== null, calls: calls().length }).toEqual({
      open: true,
      calls: 1,
    })
  })

  it('keeps the panel through a press on the page area itself or the root, and closes on one on the page’s heading', async () => {
    const host = await page('button')
    const tile = await openTile(host, 0)
    await pressOn(need(host.querySelector('.library > main'), 'page area'))
    expect(panelIn(tile), 'a press on the page area').not.toBeNull()
    await pressOn(document.documentElement)
    expect(panelIn(tile), 'a press on the root').not.toBeNull()
    await pressOn(headingOf(host))
    expect(panelIn(tile), 'a press on the heading').toBeNull()
  })

  it('saves what is typed while a close’s save is out before it closes', async () => {
    const first = deferred<metadata.PatchResult>()
    const again = deferred<metadata.PatchResult>()
    vi.mocked(metadata.patchWork)
      .mockReturnValueOnce(first.promise)
      .mockReturnValueOnce(again.promise)
    const { host, tile, field } = await typedIn('A')
    await pressOn(headingOf(host))
    expect(calls()).toEqual([['gotoubun-v01', { title: 'A' }]])
    expect(document.activeElement).toBe(field)
    await typeInto(field, 'AB')

    await flush(() => {
      first.resolve(stored('A'))
    })
    await flush()
    expect({ drawn: panelIn(tile) !== null, calls: calls() }).toEqual({
      drawn: true,
      calls: [
        ['gotoubun-v01', { title: 'A' }],
        ['gotoubun-v01', { title: 'AB' }],
      ],
    })

    await flush(() => {
      again.resolve(stored('AB'))
    })
    await flush()
    expect({ drawn: panelIn(tile) !== null, calls: calls().length }).toEqual({
      drawn: false,
      calls: 2,
    })
  })

  /**
   * The field's own Escape withdraws the ask a waiting close made, so the
   * save out answers `true` with newer text unsent in the field.
   */
  it('saves what is typed after an Escape in the field while a close waits on the field’s own save', async () => {
    const first = deferred<metadata.PatchResult>()
    const again = deferred<metadata.PatchResult>()
    vi.mocked(metadata.patchWork)
      .mockReturnValueOnce(first.promise)
      .mockReturnValueOnce(again.promise)
    const { host, tile, field } = await typedIn('A')
    await key({ key: 'Enter' })
    expect(calls(), 'the field’s own save').toEqual([
      ['gotoubun-v01', { title: 'A' }],
    ])
    await pressOn(headingOf(host))
    await pressEscape()
    expect(field.value, 'the title being sent, put back').toBe('A')
    await typeInto(field, 'AB')
    expect(
      { drawn: panelIn(tile) !== null, calls: calls().length },
      'before the save lands',
    ).toEqual({ drawn: true, calls: 1 })

    await flush(() => {
      first.resolve(stored('A'))
    })
    await flush()
    expect({ drawn: panelIn(tile) !== null, calls: calls() }).toEqual({
      drawn: true,
      calls: [
        ['gotoubun-v01', { title: 'A' }],
        ['gotoubun-v01', { title: 'AB' }],
      ],
    })

    await flush(() => {
      again.resolve(stored('AB'))
    })
    await flush()
    expect({ drawn: panelIn(tile) !== null, calls: calls().length }).toEqual({
      drawn: false,
      calls: 2,
    })
  })

  it('puts the stored title back on an Escape in the field once the save out has failed, and closes on the next', async () => {
    const save = deferred<metadata.PatchResult>()
    // A second send is held rather than answered, so it is counted.
    vi.mocked(metadata.patchWork)
      .mockReturnValueOnce(save.promise)
      .mockReturnValueOnce(deferred<metadata.PatchResult>().promise)
    const { tile, field } = await typedIn('A')
    await key({ key: 'Enter' })
    await typeInto(field, 'AB')
    await pressEscape()
    await flush(() => {
      save.reject(new Error('/api/library/works/gotoubun-v01: 500'))
    })
    await flush()
    // The fixture tells: the field, focused, holds the title that failed,
    // and the failure is under it.
    expect({
      value: field.value,
      focused: document.activeElement === field,
      failure: tile.querySelector('.tile-panel p.failure')?.textContent,
    }).toEqual({
      value: 'A',
      focused: true,
      failure: '/api/library/works/gotoubun-v01: 500',
    })

    await pressEscape()
    expect({
      open: panelIn(tile) !== null,
      value: field.value,
      calls: calls().length,
    }).toEqual({ open: true, value: 'Go-Toubun no Hanayome v01', calls: 1 })

    await pressEscape()
    expect({ open: panelIn(tile) !== null, calls: calls().length }).toEqual({
      open: false,
      calls: 1,
    })
  })

  it('stays open with the unwritten note when the save a close waits on is not written beside the library, and closes on the next press', async () => {
    const save = deferred<metadata.PatchResult>()
    vi.mocked(metadata.patchWork).mockReturnValueOnce(save.promise)
    const { host, tile } = await typedIn()
    await pressOn(headingOf(host))
    await flush(() => {
      save.resolve({ ...stored(renamed), persisted: false, why: ['disk full'] })
    })
    await flush()
    expect({
      open: panelIn(tile) !== null,
      note: tile.querySelector('.tile-panel .unwritten li')?.textContent,
    }).toEqual({ open: true, note: 'disk full' })

    await pressOn(headingOf(host))
    expect({ open: panelIn(tile) !== null, calls: calls().length }).toEqual({
      open: false,
      calls: 1,
    })
  })

  it('keeps an on-select panel with the unwritten note when the save a move waits on is not written beside the library, and moves on the next', async () => {
    const save = deferred<metadata.PatchResult>()
    vi.mocked(metadata.patchWork).mockReturnValueOnce(save.promise)
    const host = await page('on-select')
    const [first, second] = tilesOf(host)
    await hoverOn(linkOf(first))
    const field = fieldOf(first)
    await focusOn(field)
    await typeInto(field, renamed)
    await flush(() => {
      field.blur()
    })
    await hoverOn(linkOf(second))
    await flush(() => {
      save.resolve({ ...stored(renamed), persisted: false, why: ['disk full'] })
    })
    await flush()
    expect({
      first: panelIn(first) !== null,
      second: panelIn(second) !== null,
      note: first.querySelector('.tile-panel .unwritten li')?.textContent,
    }).toEqual({ first: true, second: false, note: 'disk full' })

    await hoverOn(linkOf(second))
    expect({
      first: panelIn(first) !== null,
      second: panelIn(second) !== null,
      calls: calls().length,
    }).toEqual({ first: false, second: true, calls: 1 })
  })

  it('leaves focus where it was moved while an on-select panel’s Escape close waited on the save out', async () => {
    const save = deferred<metadata.PatchResult>()
    vi.mocked(metadata.patchWork).mockReturnValueOnce(save.promise)
    const host = await page('on-select')
    const [tile] = tilesOf(host)
    await hoverOn(linkOf(tile))
    const field = fieldOf(tile)
    await focusOn(field)
    await typeInto(field, renamed)
    await key({ key: 'Enter' })
    // The field's own Escape, then the close, which waits on the save.
    await pressEscape()
    await pressEscape()
    expect(panelIn(tile), 'while the save is out').not.toBeNull()
    await focusOn(viewOf(host))

    await flush(() => {
      save.resolve(stored(renamed))
    })
    await flush()
    expect(panelIn(tile)).toBeNull()
    expect(document.activeElement).toBe(viewOf(host))
  })

  it.each([
    [
      '↓ Download, inside the panel',
      (tile: Element) => focusOn(downloadIn(tile)),
    ],
    [
      'the page’s body, the field left',
      (tile: Element) =>
        flush(() => {
          fieldOf(tile).blur()
        }),
    ],
  ] as const)(
    'sends focus to i once an Escape’s close has waited on the save out, focus having moved to %s',
    async (_, move) => {
      const save = deferred<metadata.PatchResult>()
      vi.mocked(metadata.patchWork).mockReturnValueOnce(save.promise)
      const { tile } = await typedIn()
      await key({ key: 'Enter' })
      await pressEscape()
      await pressEscape()
      expect(panelIn(tile), 'while the save is out').not.toBeNull()
      await move(tile)
      expect(document.activeElement, 'moved').not.toBe(fieldOf(tile))

      await flush(() => {
        save.resolve(stored(renamed))
      })
      await flush()
      expect(panelIn(tile)).toBeNull()
      expect(document.activeElement).toBe(iOf(tile))
    },
  )

  it('sends focus to i on an Escape from its tile’s link', async () => {
    const host = await page('button')
    const tile = await openTile(host, 0)
    await focusOn(linkOf(tile))
    expect(panelIn(tile), 'focus on the link, outside the panel').not.toBeNull()
    await pressEscape()
    expect(panelIn(tile)).toBeNull()
    expect(document.activeElement).toBe(iOf(tile))
  })

  it('sends focus to i once a click on it has closed the panel, where the click did not focus it', async () => {
    const host = await page('button')
    const tile = await openTile(host, 0)
    await focusOn(fieldOf(tile))
    await press(iOf(tile))
    expect(panelIn(tile)).toBeNull()
    expect(document.activeElement).toBe(iOf(tile))
  })

  it('sends a failed title again for no pointer entering another on-select tile, and moves once the field holds the stored title', async () => {
    vi.mocked(metadata.patchWork).mockRejectedValue(
      new Error('/api/library/works/gotoubun-v01: 500'),
    )
    const host = await page('on-select')
    const [first, second] = tilesOf(host)
    await hoverOn(linkOf(first))
    const field = fieldOf(first)
    await focusOn(field)
    await typeInto(field, renamed)
    await flush(() => {
      field.blur()
    })
    // The fixture tells: the save failed, and focus is off the panel.
    expect({
      calls: calls().length,
      failure: first.querySelector('.tile-panel p.failure')?.textContent,
      inPanel: panelIn(first)?.contains(document.activeElement),
    }).toEqual({
      calls: 1,
      failure: '/api/library/works/gotoubun-v01: 500',
      inPanel: false,
    })

    await hoverOn(linkOf(second))
    expect({
      first: panelIn(first) !== null,
      second: panelIn(second) !== null,
      calls: calls().length,
    }).toEqual({ first: true, second: false, calls: 1 })

    // Escape puts the stored title back, so nothing is left to send.
    await focusOn(field)
    await pressEscape()
    await focusOn(viewOf(host))
    await hoverOn(linkOf(second))
    expect({
      first: panelIn(first) !== null,
      second: panelIn(second) !== null,
      calls: calls().length,
    }).toEqual({ first: false, second: true, calls: 1 })
  })
})
