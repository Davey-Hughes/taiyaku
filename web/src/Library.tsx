/**
 * The library screen: the search bar, the continue strip, the filter chips,
 * and the grid of series and unfiled works, which a search's hits replace.
 *
 * **The sidebar is `Shell`'s and the tiles are `SeriesGrid`'s.** This file drew
 * both until Plan B: the sidebar and the `/api/library/status` fetch moved to
 * `Shell` so every screen under it shares them, and the tiles moved to
 * `SeriesGrid` so the grid and the search hits cannot disagree about them.
 * What is left here is the part that is only the library's — which chips
 * exist, what they filter, and which empty state an empty grid is.
 *
 * Spec C1 §2 and §11 step 6. **No virtualisation** — §7's measured numbers are
 * 127 series and 111 volumes in the largest series, and covers carry
 * `loading="lazy"`, which is free. The figure that would change that is a
 * chapter-organised edition, and it should be produced rather than imagined.
 *
 * The chips filter and sort **client-side**, over data already held: §7's
 * numbers make a `?sort=` round trip a request for something the browser does
 * instantly. `?library=` is a query parameter for the opposite reason — it
 * changes which rows exist, and which rows exist is the grant's business. It
 * narrows the grid and the continue strip alike (spec F §3).
 */
import { useEffect, useState } from 'react'
import {
  Link,
  useLocation,
  useNavigationType,
  useSearchParams,
} from 'react-router'
import { type BrowsePrefs, useBrowsePrefs } from './browseModule'
import { Cover } from './Cover'
import { type EmptyKind, EmptyState } from './EmptyState'
import {
  type Chip,
  type Grid,
  grid as fetchGrid,
  home as fetchHome,
  type HomeLists,
  type LibraryStatus,
  libraryParam,
  matchesChip,
  type Sort,
  sorted,
  unreadable,
  type WorkCard,
  workCover,
} from './library'
import { SeriesGrid } from './SeriesGrid'
import { useLibraryStatus } from './shell'
import { useDocumentTitle } from './title'
import { messageOf } from './message'
import { ViewMenu } from './ViewMenu'
import { SearchBar } from './SearchBar'
import { SEARCH_BOX_ID, useSearchHits } from './searchHits'

/** What this page's View panel offers: the two preferences that act here. */
const LIBRARY_VIEW: readonly (keyof BrowsePrefs)[] = [
  'continueStrip',
  'density',
]

const CHIPS: { id: Chip; label: string }[] = [
  { id: 'all', label: 'All' },
  { id: 'multi', label: 'Multi-edition' },
  { id: 'unread', label: 'Unread' },
]

/**
 * One card in the continue strip — spec 2026-09-17 §7.2. The cover is the
 * thumbnail of the edition the reader will open for this account (`opens`,
 * O84), not the series cover, because the strip is about a volume. What the
 * card cannot show is where the reader is in it: `progress` holds a slot, and
 * the home route sends no page — O132.
 */
function ContinueCard({ w }: { w: WorkCard }) {
  return (
    <>
      <Cover src={workCover(w.slug, w.opens)} title={w.title} />
      <span className="title">{w.title}</span>
      {w.volume !== null && <span className="meta">Vol. {w.volume}</span>}
    </>
  )
}

/**
 * Which first-run screen an empty grid is (spec C1 §8). A library's page asks
 * of its own library, and the page of every library of them all: one library
 * read and another not is "nothing here" on the first and "not read yet" on
 * the second. With no library listed, `restricted` says whose doing that is:
 * an account granted none, or a server with none. A library's page asks it
 * too: an account granted none is told so there as well, rather than that
 * this one library is not to be seen.
 */
function emptyKind(
  status: LibraryStatus,
  library: number | undefined,
): EmptyKind {
  if (library !== undefined) {
    const entry = status.libraries.find((l) => l.id === library)
    if (entry === undefined)
      return status.libraries.length === 0 && status.restricted
        ? 'no-grant'
        : 'no-library'
    return entry.scanned ? 'no-matches' : 'not-scanned'
  }
  if (status.libraries.length === 0)
    return status.restricted ? 'no-grant' : 'no-libraries'
  return status.libraries.some((l) => l.scanned) ? 'no-matches' : 'not-scanned'
}

/**
 * What the last load to land found, and for which library: the grid and the
 * home lists, or why they could not be read. One value, so a failure takes
 * the place of the rows drawn before it rather than standing beside them.
 */
type Loaded =
  | { library: number | undefined; grid: Grid; home: HomeLists }
  | { library: number | undefined; failure: string }

export function Library() {
  const prefs = useBrowsePrefs()
  const [params] = useSearchParams()
  /** `'invalid'` is no library: the page says so, and asks the server nothing. */
  const library = libraryParam(params.get('library'))

  const status = useLibraryStatus()
  /** The library this page is narrowed to, once the status names it. */
  const entry =
    typeof library === 'number'
      ? status?.libraries.find((l) => l.id === library)
      : undefined
  const [loaded, setLoaded] = useState<Loaded | undefined>()
  const [chip, setChip] = useState<Chip>('all')
  const [sort, setSort] = useState<Sort>('title')
  useDocumentTitle(entry?.name ?? 'Library')

  // **Only what was loaded for the library now asked for is drawn.** The
  // effect's `cancelled` drops an answer that lands after its library was
  // left; this keeps the last library's rows, and its failure, off the next
  // library's page until that page's own load lands.
  const current = loaded?.library === library ? loaded : undefined
  const gridData = current && 'grid' in current ? current.grid : undefined
  const home = current && 'home' in current ? current.home : undefined
  const failure = current && 'failure' in current ? current.failure : undefined

  // **The box follows what the grid shows.** Its scope is the library the
  // view shows (spec §2), so where the grid's empty state says there is
  // none — "No such library", "No library yet" on a server with none, or
  // "No library shared" for an account that sees none (O221) — there is
  // nothing to search, and that state says why. A value that is no id is
  // "No such library" before anything is asked; the rest is known once the
  // status lands.
  const unsearchable =
    library === 'invalid' ||
    (status !== undefined &&
      (library === undefined
        ? status.libraries.length === 0
        : entry === undefined))
  // Until the status lands the grid names no empty state and draws only the
  // rows it has, so the box comes with those rows or with the status: drawn
  // any earlier, it would be taken away again when the status names one of
  // the states above.
  const offered =
    !unsearchable &&
    (status !== undefined ||
      (gridData !== undefined &&
        (gridData.series.length > 0 || gridData.unfiled.length > 0)))

  // The search bar's query, scoped to the library this view shows. While it
  // holds anything the page is in search mode — the hits replace the grid,
  // and the tabs, the sort and the continue strip go, since the search
  // endpoint takes none of them — unless the view is known to have nothing
  // to search. The layout does not wait for the status, so a link's query
  // never draws the tabs and the grid first. The call does wait, until the
  // box is offered: a `?q=` the page arrives with (the redirect from
  // `/search` keeps one) sends nothing whose answer — "Nothing matched" — the
  // empty state above would then contradict.
  const q = unsearchable ? '' : (params.get('q') ?? '')
  const searching = q !== ''
  const { hits, failure: searchFailure } = useSearchHits(
    offered ? q : '',
    typeof library === 'number' ? library : undefined,
  )
  const placeholder =
    typeof library !== 'number'
      ? 'Search every library'
      : entry
        ? `Search ${entry.name}`
        : 'Search this library'

  // The phone bar's search icon arrives here with `focusSearch` in the
  // navigation's state, from any screen; on this one it focuses the box
  // itself, inside the tap, which is what lets iOS raise its keyboard. The
  // state stays on the history entry, so Back or Forward to it (a `POP`) is
  // a return to the grid and focuses nothing: a focus there would raise the
  // phone's keyboard over what the reader came back to.
  const location = useLocation()
  const arrival = useNavigationType()
  const focusSearch =
    (location.state as { focusSearch?: boolean } | null)?.focusSearch === true
  // biome-ignore lint/correctness/useExhaustiveDependencies: `location.key` is a trigger — each push or replace arrival focuses the box once.
  useEffect(() => {
    if (focusSearch && arrival !== 'POP') {
      document.getElementById(SEARCH_BOX_ID)?.focus()
    }
  }, [location.key])

  useEffect(() => {
    if (library === 'invalid') return
    let cancelled = false
    Promise.all([fetchGrid(library), fetchHome(library)])
      .then(([grid, home]) => {
        if (cancelled) return
        setLoaded({ library, grid, home })
      })
      .catch((err: unknown) => {
        if (cancelled) return
        setLoaded({ library, failure: messageOf(err) })
      })
    return () => {
      cancelled = true
    }
  }, [library])

  const visible = gridData
    ? sorted(
        gridData.series.filter((s) => matchesChip(s, chip)),
        sort,
      )
    : []

  return (
    <>
      {/* O163: one h1 per screen, for a screen reader's heading list; the
          library shows no title of its own, so it is hidden. A library's
          page is named for its library. */}
      <div className="page-head">
        <h1 className="visually-hidden">{entry?.name ?? 'Library'}</h1>
        {offered && <SearchBar placeholder={placeholder} />}
        <ViewMenu keys={LIBRARY_VIEW} />
      </div>
      {failure !== undefined && <p role="alert">{failure}</p>}
      {searching ? (
        <div className="search-hits">
          {searchFailure !== undefined && <p role="alert">{searchFailure}</p>}
          {hits &&
            (hits.series.length === 0 && hits.unfiled.length === 0 ? (
              <EmptyState kind="no-matches" query={hits.query} />
            ) : (
              <SeriesGrid series={hits.series} unfiled={hits.unfiled} />
            ))}
        </div>
      ) : (
        <>
          {prefs.continueStrip && home && home.continuing.length > 0 && (
            <section data-testid="continue" className="continue">
              <h2>Continue</h2>
              <ul>
                {home.continuing.map((w) => (
                  <li key={w.slug}>
                    {unreadable(w) ? (
                      // **O62 — the link is the treatment, not a badge.** I1 of
                      // the whole-branch review names this strip as the
                      // likeliest place to meet a work whose files just
                      // vanished: the reader was mid-read when they went, so
                      // this is exactly where an unguarded link would send them
                      // back into a page that cannot load. The volume keeps its
                      // name, same as `Series.tsx`'s `Table`.
                      <span className="tile-unreadable">
                        <ContinueCard w={w} />
                        <span className="unreadable">
                          files missing — cannot be opened
                        </span>
                      </span>
                    ) : (
                      <Link to={`/read/${w.slug}`}>
                        <ContinueCard w={w} />
                      </Link>
                    )}
                  </li>
                ))}
              </ul>
            </section>
          )}
          <div className="library-chips">
            {/* O156: the filters are one-of-N, so they are grouped under one
            name. "Added" sorts rather than filters, and
            stays outside. */}
            <div className="library-chip-group" role="group" aria-label="Show">
              {CHIPS.map((c) => (
                <button
                  key={c.id}
                  type="button"
                  aria-pressed={chip === c.id}
                  onClick={() => setChip(c.id)}
                >
                  {c.label}
                </button>
              ))}
            </div>
            <button
              type="button"
              className="sort"
              aria-pressed={sort === 'added'}
              onClick={() => setSort(sort === 'added' ? 'title' : 'added')}
            >
              Added
            </button>
          </div>
          {library === 'invalid' ? (
            <EmptyState kind="no-library" />
          ) : gridData &&
            status &&
            visible.length === 0 &&
            gridData.unfiled.length === 0 ? (
            <EmptyState kind={emptyKind(status, library)} />
          ) : (
            <SeriesGrid series={visible} unfiled={gridData?.unfiled ?? []} />
          )}
        </>
      )}
    </>
  )
}
