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
  cover: string
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

export interface RootCard {
  id: number
  name: string
}

export interface LibraryStatus {
  roots: RootCard[]
  scanned: boolean
}

export const home = () => getJson<HomeLists>('/api/library/home')

export const grid = (root?: number) =>
  getJson<Grid>(
    root === undefined
      ? '/api/library/series'
      : `/api/library/series?root=${root}`,
  )

export const oneSeries = (id: number) =>
  getJson<{ series: SeriesCard; works: WorkCard[] }>(
    `/api/library/series/${id}`,
  )

/**
 * A volume's cover: page 0 of one of its editions, through the thumbnail route
 * the reader already has.
 *
 * **Not a `/api/library/…` route, and there is deliberately no such thing.**
 * `/api/library/series/{id}/cover` is a 307 to exactly this URL — see
 * `library_api.rs::cover`, whose whole argument is that a series cover *is* an
 * edition cover and a second pipeline would be a second implementation of
 * caching, guarding and `Cache-Control`. A volume tile already knows which
 * edition it is showing, so it needs no redirect to find out.
 */
export const workCover = (slug: string, edition: string) =>
  `/api/works/${encodeURIComponent(slug)}/${encodeURIComponent(edition)}/pages/0/thumb`

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

export const search = (q: string) =>
  getJson<SearchHits>(`/api/library/search?q=${encodeURIComponent(q)}`)

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
 * **Every edition, not any.** One edition losing its files while a sibling
 * stays intact leaves the volume readable through the sibling, and
 * withholding the link there would hide a work that still works. The
 * narrower case this does not close: a volume whose *hub* edition is the
 * broken one still offers the link, because `WorkCard` does not say which
 * edition the reader opens. Closing that needs the hub's identity in the
 * browse response, and is recorded as an open item rather than guessed at.
 *
 * A work with no visible edition at all is not unreadable-by-missing-files —
 * it is a grant hiding everything, which is a different fact and already has
 * its own treatment.
 */
export function unreadable(w: WorkCard): boolean {
  return w.editions.length > 0 && w.editions.every((e) => e.missing > 0)
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
