import { act } from 'react'
import { MemoryRouter, Route, Routes, useNavigate } from 'react-router'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { Me } from './api'
import { BrowsePrefsContext, browseModule } from './browseModule'
import { GateContext } from './gate'
import * as library from './library'
import { Series } from './Series'
import { mount } from './testing/mount'

vi.mock('./library', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./library')>()),
  oneSeries: vi.fn(),
  work: vi.fn(),
}))

const edition = (
  over: Partial<library.EditionCard> = {},
): library.EditionCard => ({
  key: 'raw',
  label: 'Manga (RAW)',
  members: 1,
  pages: 180,
  missing: 0,
  ...over,
})

const work = (over: Partial<library.WorkCard> = {}): library.WorkCard => ({
  id: 1,
  slug: 'gotoubun-v01',
  title: 'Go-Toubun no Hanayome v01',
  volume: 1,
  aligned: false,
  pages: 180,
  editions: [edition()],
  ...over,
})

const who = (over: Partial<Me> = {}): Me => ({
  user_id: 1,
  admin: false,
  can_download: true,
  can_stream: true,
  ...over,
})

const card = (over: Partial<library.SeriesCard> = {}): library.SeriesCard => ({
  id: 7,
  title: 'Go-Toubun no Hanayome',
  works: 3,
  editions: 5,
  aligned: 1,
  added: 3,
  unread: 2,
  cover: '/api/library/series/7/cover',
  ...over,
})

/**
 * Mount at `/series/7` and let the load settle — two flushes, as
 * `Library.test.tsx` explains.
 *
 * `Series` takes its three switches from `BrowsePrefsContext` rather than as
 * props (§11 step 8), so `props` here is layered over `browseModule.defaults`
 * and handed to the provider instead of spread onto the component. The tests
 * below are unchanged by that: each one still says `settleWith({ layout:
 * 'table' })` and reads as before.
 */
async function settleWith(
  props: Partial<
    Pick<
      React.ContextType<typeof BrowsePrefsContext>,
      'layout' | 'editionSelector' | 'panel'
    >
  > = {},
  me?: Me,
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
          <MemoryRouter initialEntries={['/series/7']}>
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

describe('Series, grid layout', () => {
  beforeEach(() => {
    vi.clearAllMocks()
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
    expect(vi.mocked(library.oneSeries)).toHaveBeenCalledWith(7)
  })

  it('names the series and lists its volumes in order', async () => {
    const host = await settle()
    expect(host.querySelector('h1')?.textContent).toBe('Go-Toubun no Hanayome')
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
    // I7 of the whole-branch review: spec C1 §3 puts the mark *on the
    // cover*, and `index.css` has exactly one rule for it —
    // `.cover-box .aligned` — so outside that box the mark loses its
    // positioning and its chip. `textContent`/presence checks above cannot
    // tell "on the cover" from "moved below the title"; only the ancestor
    // can.
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

  it('draws each volume cover from its own edition rather than the series redirect', async () => {
    const host = await settle()
    expect(host.querySelector('.volumes img')?.getAttribute('src')).toBe(
      '/api/works/gotoubun-v01/raw/pages/0/thumb',
    )
  })

  it('says so when the series cannot be read', async () => {
    vi.mocked(library.oneSeries).mockRejectedValue(
      new Error('/api/library/series/7: 404'),
    )
    const host = await settle()
    expect(host.querySelector('[role="alert"]')?.textContent).toContain('404')
    expect(host.querySelector('.volumes')).toBeNull()
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

  it('offers a download per tile, for the edition the tile is showing', async () => {
    const host = await settleWith({}, who())
    const links = [
      ...host.querySelectorAll<HTMLAnchorElement>('.volumes a.get'),
    ]
    expect(links).toHaveLength(3)
    expect(links[0].getAttribute('href')).toBe(
      '/api/works/gotoubun-v01/raw/download',
    )
    // Bare, with no value. The server's `Content-Disposition` carries an
    // ASCII `filename=` and an RFC 6266 `filename*=`, and a value here would
    // override both with a guess made in the browser.
    expect(links[0].getAttribute('download')).toBe('')
  })

  it('keeps the download anchor outside the tile link, not nested in it', async () => {
    const host = await settleWith({}, who())
    const get = host.querySelector('.volumes a.get')
    expect(get).not.toBeNull()
    // A nested anchor is invalid HTML and browsers reparent it, which a
    // snapshot would render identically — so this is asserted structurally.
    // The tile's own `<Link>` is the anchor holding `.cover-box`.
    const tileLink = host.querySelector('.volumes li a:has(.cover-box)')
    expect(tileLink).not.toBeNull()
    expect(tileLink?.contains(get)).toBe(false)
  })

  it('shows no download at all when the account may not download', async () => {
    const host = await settleWith({}, who({ can_download: false }))
    expect(host.querySelectorAll('.volumes a.get')).toHaveLength(0)
  })

  it('shows no download when no gate sits above the screen', async () => {
    // `useGate()` is `undefined` here, and `gate.ts` says the safe reading of
    // that is "no account known" rather than a permissive one.
    const host = await settle()
    expect(host.querySelectorAll('.volumes a.get')).toHaveLength(0)
  })
})

/**
 * I1 of the whole-branch review: O62's guard (`unreadable`, spec C1 §8) was
 * applied only inside `Table`, and `browseModule.ts` makes `layout: 'grid'`
 * the **default** — every fixture above this block passes `layout: 'table'`
 * to exercise the guarded path, and `settle()`/`settleWith({})` below pass
 * nothing, exactly the "one click from the shipped defaults" the finding
 * itself describes. Before this fix, a reader met an unguarded
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

  it('keeps the reader link when only one edition of a volume has lost its files', async () => {
    vi.mocked(library.oneSeries).mockResolvedValue({
      series: card(),
      works: [
        work({
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

  it('withholds the per-tile download for the edition it would show, when that edition has lost its files, even though the volume itself stays readable', async () => {
    vi.mocked(library.oneSeries).mockResolvedValue({
      series: card(),
      works: [
        work({
          editions: [
            // `chosen === 'all'` shows `editions[0]` — the broken one —
            // while `en` keeps the volume itself readable. Downloading
            // files that are not on disk is not an offer this screen
            // should make, independent of whether the volume opens.
            edition({ key: 'raw', missing: 2 }),
            edition({ key: 'en', missing: 0 }),
          ],
        }),
      ],
    })
    const host = await settleWith({}, who())
    expect(
      host.querySelector('.volumes a[href="/read/gotoubun-v01"]'),
    ).not.toBeNull()
    expect(host.querySelector('.volumes a.get')).toBeNull()
  })
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
    // the run — which is how this shipped green on a focused run.
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
      host.querySelector<HTMLInputElement>(
        '.metadata-editor input[name="title"]',
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
   * C1 of the whole-branch review: the selector's own visibility check never
   * looked at `layout`, so it rendered — and governed nothing, since `Table`
   * takes no `chosen` — the moment `editionSelector` was on, table or not.
   * `editionSelector: true` and `panel: 'button'` are `browseModule.defaults`
   * already, so this needs no override beyond `layout` — the same "one click
   * from the shipped defaults" the finding itself describes.
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
          editions: [edition({ key: 'raw', label: 'Manga (RAW)', pages: 190 })],
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

  it('draws the chosen edition’s cover', async () => {
    const host = await settleWith({ editionSelector: true })
    const select = host.querySelector('select') as HTMLSelectElement
    act(() => {
      select.value = 'en'
      select.dispatchEvent(new Event('change', { bubbles: true }))
    })
    expect(host.querySelector('.volumes img')?.getAttribute('src')).toBe(
      '/api/works/gotoubun-v01/en/pages/0/thumb',
    )
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
      '/api/works/gotoubun-v01/raw/pages/0/thumb',
    )
    expect(
      [...host.querySelectorAll('.volumes li')].some((li) =>
        li.classList.contains('without'),
      ),
    ).toBe(false)
  })

  /**
   * I8 of the whole-branch review: `Cover.tsx` remembers a failed cover *by
   * `src`* rather than as a boolean, precisely so a series whose cover URL
   * changes is retried rather than left broken for the life of the page.
   * Until this branch nothing could exercise that rule — a series tile's
   * `cover` never changes — but `Series.tsx` now feeds `workCover(w.slug,
   * chosen)` into a `Cover` kept mounted across an edition change by the
   * `<li key={w.slug}>` above it. This is the first place the URL changes
   * under a live `Cover` instance, and the discriminating mutation (`if
   * (failed === src)` → `if (failed !== undefined)`) leaves the placeholder
   * up forever once any cover has ever failed, regardless of a later `src`
   * change — so this test must check both directions: the placeholder
   * appears on failure, and a *different* URL clears it again.
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
      '/api/works/gotoubun-v01/raw/pages/0/thumb',
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
      '/api/works/gotoubun-v01/en/pages/0/thumb',
    )
  })

  /**
   * Unreachable through today's routes — the only link to `/series/:id`
   * comes from a different route, so `Series` always remounts and `chosen`'s
   * `useState('all')` initialiser runs fresh. But Task 10 adds a search
   * screen whose results link into `/series/:id`, at which point navigating
   * from one series to another *reuses* this `Series` instance the same way
   * `Routes.test.tsx`'s "navigating from one work to another" test reuses
   * `Reader` when nothing keys it — so this drives a real `useNavigate()`
   * rather than a second `mount()`, which would hand back a fresh instance
   * regardless of any fix and could not tell "the state reset" from "there
   * was never any state to carry over".
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
})

describe('Series, the alignment panel', () => {
  beforeEach(() => {
    // Unlike the three `describe`s above, these tests assert on
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
   * I5 of the whole-branch review: `reveal(slug)` always calls `setOpen(slug)`
   * and nothing ever set it back, so `aria-expanded` went `false`, `true`,
   * `true` — the panel had no way to close once any button had opened it.
   * The fix belongs at the button's own `onClick`, not inside `reveal`:
   * `on-select`'s hover and focus paths call `reveal` too, and a toggle
   * there would close the panel the second time a pointer merely passed back
   * over the same tile.
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
   * D1 of the browser pass: `derive` never writes an alignment itself, so
   * `alignments.length === 0` is the state of *every* volume in a freshly
   * scanned library, however many editions it holds. The panel must tell
   * "truly nothing to pair" (fewer than two editions) apart from "has
   * something to pair and simply has not been aligned yet" (two or more).
   * This test is the first case, where the existing sentence is correct.
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
   * The second case, and the one the browser pass actually caught: a volume
   * with two editions and no alignments is not "one edition, nothing to pair
   * with" — that sentence is false on this fixture. The table layout renders
   * the same two editions as two columns on one screen, so the wrong
   * sentence and the correct data would have appeared together.
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
