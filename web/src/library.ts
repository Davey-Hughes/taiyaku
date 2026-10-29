/**
 * The `/api/library/*` browse API. Knows HTTP; knows nothing about screens.
 *
 * Spec C1 §5 and §5.5. Separate from `api.ts` — which is the *reader's* API,
 * sidecars and pages and positions — because the two share a session and
 * nothing else: no browse screen touches a resolver, and no reader screen
 * reads a series. They share `getJson`, so the 401 rule is one rule.
 *
 * The chip and sort helpers are here rather than in a component because
 * §7 declines a `?sort=` round trip on measured numbers (127 series), which
 * makes "which series does this chip keep" a pure function over data the
 * browser already holds — and a pure function is the part worth testing.
 */
import { getJson } from './api'
import type { SeriesTitle } from './metadata'

export interface EditionCard {
  key: string
  label: string
  members: number
  pages: number
  /**
   * How many of `members` have left the disk since the last scan — O62,
   * spec C1 §8's empty state. `0` on every edition until a file actually
   * goes missing; never negative and never greater than `members`.
   */
  missing: number
  /**
   * Page 0's cover address, `/api/works/{work}/{edition}/pages/0/thumb?v=…`,
   * as the server builds it: `v` names the bytes, which lets the route mark
   * the image `immutable`, and `coverSources` adds the size. `null`: the
   * edition has no page.
   */
  cover: string | null
}

/** One edition pair's alignment — only `work()` carries these. */
export interface AlignmentCard {
  from: string
  to: string
  verdict: string | null
  slots: number | null
  paired: number | null
}

export interface WorkCard {
  id: number
  slug: string
  title: string
  volume: number | null
  aligned: boolean
  pages: number
  editions: EditionCard[]
  /**
   * The edition this caller's reader will open — O84, and **not** the hub.
   *
   * `startup.ts` restores the stored position's edition when the resolver
   * knows it and the hub otherwise, so the server computes the same rule and
   * sends the answer rather than the ingredients. It is a key of one of
   * `editions` whenever the grant shows that edition; see `unreadable`.
   */
  opens: string
}

export interface WorkDetail extends WorkCard {
  alignments: AlignmentCard[]
}

export interface SeriesCard {
  id: number
  title: string
  works: number
  editions: number
  /** Works with at least one alignment — §3's `⇄ 3` mark. */
  aligned: number
  /** The highest `work.id` in the series; §5.5's recency proxy. */
  added: number
  /** Works with no position for this reader. */
  unread: number
  /**
   * The cover address of the edition the series' cover is taken from, as
   * `EditionCard.cover`. `null`: no edition of the series is visible to this
   * caller, or the one the cover is taken from has no page; the tile draws
   * the missing-cover box and asks for nothing.
   */
  cover: string | null
}

/**
 * One series as `/api/library/series/{id}` answers it: the tile, and every
 * title the series holds (O83).
 *
 * `titles` is the list `metadata.ts`'s `patchSeries` sends and the server
 * replaces whole, in the order it was written, derivation's own title
 * included — `title` above is only the primary title. `primary` is
 * always present here, `false` included. The grid and search answer a
 * `SeriesCard`, without it.
 */
export interface SeriesDetail extends SeriesCard {
  titles: Required<SeriesTitle>[]
}

/** A grid, or a search over one: series, and the works that belong to none. */
export interface Grid {
  series: SeriesCard[]
  unfiled: WorkCard[]
}

export interface SearchHits extends Grid {
  /** Echoed back, so §8's empty state can name what was searched. */
  query: string
}

export interface HomeLists {
  continuing: WorkCard[]
  added: WorkCard[]
  read: WorkCard[]
}

/**
 * One library, as `/api/library/status` lists it — spec F §3. `scanned` says
 * whether any of its directories has been walked, so a library's page with
 * nothing on it can say "not read yet" rather than "nothing here".
 */
export interface LibraryEntry {
  id: number
  name: string
  scanned: boolean
}

export interface LibraryStatus {
  libraries: LibraryEntry[]
  /**
   * The account sees only the libraries granted to it. With no library
   * listed, this is what tells an account granted none from a server that
   * has none: the first waits on a grant, the second on a library being
   * made.
   */
  restricted: boolean
}

/**
 * The library a `?library=` value names: `undefined` where there is no
 * value, and `'invalid'` where the value is no library id.
 *
 * **Every screen that reads the parameter reads it through this**, the
 * library page, the sidebar's mark and Search's narrowing alike, so that no
 * two of them take one URL to mean different things: the library page once
 * read the value with `Number` while the sidebar compared its text, so `01`
 * was library 1 on the page and no library in the sidebar. An id is a
 * positive decimal integer with no leading zero, the form the sidebar's
 * links write; `Number` would read `01` as 1, an empty value as 0 and `abc`
 * as `NaN`.
 */
export function libraryParam(
  raw: string | null,
): number | undefined | 'invalid' {
  if (raw === null) return undefined
  return /^[1-9]\d*$/.test(raw) ? Number(raw) : 'invalid'
}

/**
 * `path` narrowed to one library, or as it is for every library. Exported for
 * `pageReads.ts`, whose store addresses must be the paths these calls send.
 */
export const narrowedTo = (path: string, library?: number) =>
  library === undefined
    ? path
    : `${path}${path.includes('?') ? '&' : '?'}library=${library}`

export const home = (library?: number) =>
  getJson<HomeLists>(narrowedTo('/api/library/home', library))

export const grid = (library?: number) =>
  getJson<Grid>(narrowedTo('/api/library/series', library))

export const oneSeries = (id: number) =>
  getJson<{ series: SeriesDetail; works: WorkCard[] }>(
    `/api/library/series/${id}`,
  )

/**
 * The long edges a library cover is offered at, one per screen density — O133.
 *
 * A tile is drawn about 256 CSS px on its long edge (a phone's two-column
 * grid; a desktop tile is a little smaller), so 256, 512 and 768 are what one,
 * two and three device pixels per CSS pixel draw it at. Covers came from the
 * filmstrip's tile until O133, 160 px on its long edge — upscaled nearly
 * fivefold on a 3× phone.
 *
 * **The server holds the same three** (`cover.rs`'s `COVER_LONG_EDGES`): the
 * tile route sizes page 0 to those and to nothing else, and answers any other
 * `w` with the 160 px tile, so a size added here alone would draw a blurry
 * cover rather than fail.
 */
const COVER_LONG_EDGES = [256, 512, 768] as const

/**
 * A cover address at each of `COVER_LONG_EDGES`, for an `<img>`'s `src` and
 * `srcSet`. The server's addresses carry `?v=`, so the size goes after it.
 */
export function coverSources(url: string): { src: string; srcSet: string } {
  const at = (edge: number) => `${url}${url.includes('?') ? '&' : '?'}w=${edge}`
  return {
    src: at(COVER_LONG_EDGES[0]),
    srcSet: COVER_LONG_EDGES.map((edge, i) => `${at(edge)} ${i + 1}x`).join(
      ', ',
    ),
  }
}

/**
 * One edition as a single CBZ — spec C2's download route.
 *
 * **A URL, not a fetch.** The series page links to it with a plain
 * `<a download>`: an anchor navigation carries the session cookie like every
 * other browser request, and the browser's own download manager takes the
 * file, where a `fetch`-to-blob would buffer a 1.49 GB volume in the tab.
 */
export const editionDownload = (slug: string, edition: string) =>
  `/api/works/${encodeURIComponent(slug)}/${encodeURIComponent(edition)}/download`

export const work = (slug: string) =>
  getJson<WorkDetail>(`/api/library/works/${encodeURIComponent(slug)}`)

export const search = (q: string, library?: number) =>
  getJson<SearchHits>(
    narrowedTo(`/api/library/search?q=${encodeURIComponent(q)}`, library),
  )

export const status = () => getJson<LibraryStatus>('/api/library/status')

/**
 * Whether nothing this caller can see of `w` can still be opened — O62,
 * spec C1 §8's last empty state.
 *
 * **Here, not on a component**, for the same reason `matchesChip` and
 * `sorted` are below: every reader link in the browse screens is offered
 * from a `WorkCard` a caller already holds, so this is a pure function over
 * data already in memory rather than a fetch, and it is shared because four
 * places now need to agree on it — `Series.tsx`'s grid and table, the
 * per-edition download beside each, `Library.tsx`'s Continue strip, and
 * `SeriesGrid.tsx`'s unfiled works. A copy in each would drift the moment one
 * of them "simplified" the guard below without touching its siblings; a
 * whole-branch review already found the guard undefended in the *one* place
 * it lived, so five undefended copies is worse, not better.
 *
 * **The edition that opens, not every edition — O84.** This asked
 * `editions.every(missing > 0)` until then, which is right about the case it
 * was written for (one edition loses its files, a sibling stays intact, the
 * volume is still readable through the sibling) and wrong about the volume
 * whose *own* opening edition is the broken one. `w.opens` is the server's
 * answer to "which edition will this caller's reader open", computed from the
 * same rule `reading.ts::restore` applies — the stored position's edition, or
 * the hub. So the guard can ask the only question that matters.
 *
 * **Asking the hub instead would have been cheaper and wrong**, and the
 * Continue strip is where it shows: a reader part-way through the intact
 * sibling of a work whose hub has lost its files would find it gone from the
 * one list that exists to bring them back to it.
 *
 * **An `opens` naming no edition here is not unreadable-by-missing-files.**
 * That happens when the grant hides the hub itself, which leaves the reader
 * broken for an unrelated reason — the resolver has no slots for an edition
 * it was never sent — and is neither this guard's question nor a regression
 * from the `every` form, which offered that work too.
 */
export function unreadable(w: WorkCard): boolean {
  const opening = w.editions.find((e) => e.key === w.opens)
  return opening !== undefined && opening.missing > 0
}

/** The grid's filter chips — §2. */
export type Chip = 'all' | 'multi' | 'unread'

/**
 * Whether a chip keeps this series.
 *
 * `multi` is `editions > works` **exactly**, not approximately: a work with no
 * edition is not counted at all, so the two counts differ if and only if some
 * work here has a second edition.
 *
 * `unread` is `unread === works` — nothing here started. `unread > 0` was the
 * alternative and leaves a 14-volume series in the chip forever after volume 1.
 */
export function matchesChip(series: SeriesCard, chip: Chip): boolean {
  if (chip === 'multi') return series.editions > series.works
  if (chip === 'unread') return series.unread === series.works
  return true
}

export type Sort = 'title' | 'added'

/** A sorted copy — never the caller's array reordered under it. */
export function sorted(series: SeriesCard[], sort: Sort): SeriesCard[] {
  return [...series].sort((a, b) =>
    sort === 'added' ? b.added - a.added : a.title.localeCompare(b.title),
  )
}

/**
 * The Library view's chip and sort, which its address holds as `?show=` and
 * `?sort=` so that Back, a link and the sidebar's library switch bring them
 * back (spec `2026-10-28-loading-library-design.md` §5.1).
 */
export interface View {
  chip: Chip
  sort: Sort
}

/**
 * The chip and the sort `params` name. A value missing, or naming no chip or
 * sort, is the default, All or Title: an address cannot select what the
 * buttons do not offer.
 */
export function viewIn(params: URLSearchParams): View {
  const show = params.get('show')
  return {
    chip: show === 'multi' || show === 'unread' ? show : 'all',
    sort: params.get('sort') === 'added' ? 'added' : 'title',
  }
}

/**
 * A copy of `params` naming `view`, every other parameter kept. A default is
 * left out, so choosing All or Title gives the address the view had before
 * anything was chosen. The parameters are put in the order of their names,
 * so one view has one address whichever was pressed first: the sidebar's
 * link to the view shown, which this writes too, is then the address shown,
 * and React Router makes it a replace, which adds no entry.
 */
export function withView(params: URLSearchParams, view: View): URLSearchParams {
  const next = new URLSearchParams(params)
  if (view.chip === 'all') next.delete('show')
  else next.set('show', view.chip)
  if (view.sort === 'title') next.delete('sort')
  else next.set('sort', view.sort)
  next.sort()
  return next
}
