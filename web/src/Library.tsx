/**
 * The library screen: the search bar, the continue strip, the filter chips,
 * and the grid of series and unfiled works, which a search's hits replace.
 *
 * **The sidebar is `Shell`'s and the tiles are `SeriesGrid`'s.** The sidebar
 * and the `/api/library/status` fetch are `Shell`'s, so every screen under it
 * shares them, and the tiles are `SeriesGrid`'s, so the grid and the search
 * hits cannot disagree about them. What is here is the part that is only the
 * library's — which chips exist, what they filter, and which empty state an
 * empty grid is.
 *
 * Spec C1 §2 and §11 step 6. **No virtualisation** — §7's numbers do not call
 * for it, and covers carry `loading="lazy"`, which is free. The figure that
 * would change that is a chapter-organised edition, and it should be produced
 * rather than imagined.
 *
 * The chips filter and sort **client-side**, over data already held: §7's
 * numbers make a round trip to the server a request for something the browser
 * does instantly. They are in the address all the same, as `?show=` and
 * `?sort=` (spec `2026-10-28-loading-library-design.md` §5.1), so that Back and
 * the sidebar's library switch bring them back; the server is never asked for
 * them. `?library=` is sent to the server for the opposite reason — it changes
 * which rows exist, and which rows exist is the grant's business. It narrows
 * the grid and the continue strip alike (spec F §3).
 *
 * **The grid and the home lists are read through the store** (`usePage`), as
 * one set, so a return draws what was kept in its first render and reads it
 * again, and a first visit draws both in one step.
 */
import { useEffect, useRef } from 'react'
import {
  Link,
  useLocation,
  useNavigationType,
  useSearchParams,
} from 'react-router'
import { type BrowsePrefs, coverSizeOf, useBrowsePrefs } from './browseModule'
import { useContinueRow } from './continueRow'
import { Cover } from './Cover'
import { type EmptyKind, EmptyState } from './EmptyState'
import {
  type Chip,
  type Grid,
  type HomeLists,
  type LibraryStatus,
  libraryParam,
  matchesChip,
  readerPath,
  sorted,
  unreadable,
  type View,
  viewIn,
  type WorkCard,
  withView,
} from './library'
import { useDrawnWait } from './hooks'
import { gridRead, homeRead } from './pageReads'
import { SeriesGrid } from './SeriesGrid'
import { PageLoading } from './settingsLayout'
import { useLibraryStatus, useStatusFailed } from './shell'
import { useDocumentTitle } from './title'
import { messageOf } from './message'
import { ViewMenu } from './ViewMenu'
import { SearchBar } from './SearchBar'
import { useKeptScroll } from './scrollKeeper'
import { boxStateOf, SEARCH_BOX_ID, useSearchHits } from './searchHits'
import { usePage } from './usePage'

/** What this page's View panel offers: the preferences that act here. */
const LIBRARY_VIEW: readonly (keyof BrowsePrefs)[] = [
  'continueStrip',
  'coverSize',
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
      <Cover
        src={w.editions.find((e) => e.key === w.opens)?.cover ?? null}
        title={w.title}
      />
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

export function Library() {
  const prefs = useBrowsePrefs()
  const [params, setParams] = useSearchParams()
  /** `'invalid'` is no library: the page says so, and asks the server nothing. */
  const library = libraryParam(params.get('library'))
  /** The library the search and the tiles carry, where the page names one. */
  const narrowed = typeof library === 'number' ? library : undefined
  const view = viewIn(params)
  const { chip, sort } = view

  const status = useLibraryStatus()
  /** The library this page is narrowed to, once the status names it. */
  const entry =
    typeof library === 'number'
      ? status?.libraries.find((l) => l.id === library)
      : undefined
  useDocumentTitle(entry?.name ?? 'Library')

  // **Only what was read for the library now asked for is drawn**, and only
  // once both its reads are in hand: `usePage` answers for the addresses it
  // is given, so a switch draws nothing of the library left, neither its
  // rows nor its failure, while a switch to a library kept in this tab draws
  // it at once. Where both reads fail the grid's failure is the one said,
  // since the grid is what the view is for.
  const {
    ready,
    answers: [gridAnswer, homeAnswer],
    failures: [gridFailure, homeFailure],
    loadingDrawn,
  } = usePage<[Grid, HomeLists]>(
    library === 'invalid' ? [] : [gridRead(library), homeRead(library)],
  )
  const gridData = ready ? gridAnswer : undefined
  const home = ready ? homeAnswer : undefined
  const failed = gridFailure ?? homeFailure
  const failure = failed === undefined ? undefined : messageOf(failed)

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
  // empty state above would then contradict. That holds for an arrival's
  // query too, which is read at once rather than after the typing delay.
  const q = unsearchable ? '' : (params.get('q') ?? '')
  const searching = q !== ''
  const location = useLocation()
  const arrival = useNavigationType()
  // The box writes `?q=` with `replace` and its tag (O223). Back or Forward
  // restores an entry with the tag it was left with, a push is never the
  // box's, and a replace without the tag is someone else's, as the redirect
  // from `/search` is: so only a replace carrying the tag is a key typed.
  const typed = arrival === 'REPLACE' && boxStateOf(location.state) !== null
  const {
    hits,
    failure: searchFailure,
    waiting: searchWaiting,
  } = useSearchHits(offered ? q : '', narrowed, typed)
  // Where the hits go, the view waits for the box to be offered (for the
  // status, or for rows) and then for the search: one wait over both, so a
  // line drawn before the query is sent stays drawn after it. A failed read
  // ends the wait before the offer, and the failure holds until a read of it
  // answers: the failure is what the page says meanwhile.
  const statusFailed = useStatusFailed()
  const searchWait =
    (!offered && failure === undefined && !statusFailed) || searchWaiting
  const searchLoadingDrawn = useDrawnWait(searching && searchWait)
  // The scroll is put back once the body this address draws is there: the
  // hits where a query is, the grid where none is.
  useKeptScroll(searching ? !searchWait : ready, { library: true })
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
  const focusSearch =
    (location.state as { focusSearch?: boolean } | null)?.focusSearch === true
  // biome-ignore lint/correctness/useExhaustiveDependencies: `location.key` is a trigger — each push or replace arrival focuses the box once.
  useEffect(() => {
    if (focusSearch && arrival !== 'POP') {
      document.getElementById(SEARCH_BOX_ID)?.focus()
    }
  }, [location.key])

  /**
   * Writes a pressed chip or sort into the address with `replace`, as the
   * search box writes `?q=`: a press adds no entry for Back to walk out of.
   * Every other parameter stays, and so does the box's tag (O223). The rest
   * of the navigation's state is for an arrival, as `focusSearch` is:
   * carried over a press, it would focus the box again.
   */
  const choose = (next: View) => {
    setParams(withView(params, next), {
      replace: true,
      state: boxStateOf(location.state),
    })
  }

  const continueList = useRef<HTMLUListElement>(null)
  // One condition for the strip and its hook, so they cannot disagree.
  const stripDrawn =
    !searching &&
    prefs.continueStrip &&
    home !== undefined &&
    home.continuing.length > 0
  useContinueRow(
    continueList,
    stripDrawn,
    coverSizeOf(prefs),
    home?.continuing.length ?? 0,
  )

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
          {hits === undefined ? (
            <PageLoading drawn={searchLoadingDrawn} />
          ) : hits.series.length === 0 && hits.unfiled.length === 0 ? (
            <EmptyState kind="no-matches" query={hits.query} />
          ) : (
            <SeriesGrid
              series={hits.series}
              unfiled={hits.unfiled}
              library={narrowed}
            />
          )}
        </div>
      ) : (
        <>
          {stripDrawn && (
            <section data-testid="continue" className="continue">
              <h2>Continue</h2>
              <ul ref={continueList}>
                {home.continuing.map((w) => (
                  <li key={w.slug}>
                    {unreadable(w) ? (
                      // **O62 — the link is the treatment, not a badge.** This
                      // strip is the likeliest place to meet a work whose files
                      // just vanished: the reader was mid-read when they went,
                      // so this is exactly where an unguarded link would send
                      // them back into a page that cannot load. The volume
                      // keeps its name, same as `Series.tsx`'s `Table`.
                      <span className="tile-unreadable">
                        <ContinueCard w={w} />
                        <span className="unreadable">
                          files missing — cannot be opened
                        </span>
                      </span>
                    ) : (
                      <Link to={readerPath(w)}>
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
                  onClick={() => choose({ ...view, chip: c.id })}
                >
                  {c.label}
                </button>
              ))}
            </div>
            <button
              type="button"
              className="sort"
              aria-pressed={sort === 'added'}
              onClick={() =>
                choose({ ...view, sort: sort === 'added' ? 'title' : 'added' })
              }
            >
              Added
            </button>
          </div>
          {library === 'invalid' ? (
            <EmptyState kind="no-library" />
          ) : !ready ? (
            <PageLoading drawn={loadingDrawn} />
          ) : gridData &&
            status &&
            visible.length === 0 &&
            gridData.unfiled.length === 0 ? (
            <EmptyState kind={emptyKind(status, library)} />
          ) : (
            <SeriesGrid
              series={visible}
              unfiled={gridData?.unfiled ?? []}
              library={narrowed}
            />
          )}
        </>
      )}
    </>
  )
}
