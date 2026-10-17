/**
 * Search — spec C1 §11 step 9, over the endpoint that shipped in #81.
 *
 * **`?q=` is in the URL**, so a search is a link: the same reason `?library=`
 * is a query parameter on the library screen rather than component state.
 *
 * **`?library=` is in the URL here too, for `?q=`'s reason**: a search
 * narrowed to one library is a link as well (spec F §3). The "Search in"
 * select that sets it is drawn only where there are two libraries or more.
 * The library the URL names narrows the search where the select can show it,
 * and while there is no status to say whether it can: see `searchedLibrary`.
 *
 * **Debounced through `debounce.ts`**, which exists and is tested, at the same
 * 250 ms the reader writes progress at. Typing 五等分の花嫁 is six keystrokes
 * and one request.
 *
 * **A sequence number, because responses do not have to arrive in order.**
 * Three requests in flight and the first to return may be the first *sent*
 * rather than the last; without this the screen shows the answer to a query
 * the reader has already moved past. `cancelled` flags — the pattern
 * `Library.tsx` uses — solve unmount, not overtaking.
 *
 * **What the tokeniser makes possible here**, and what it does not: migration
 * 8's `trigram` index answers any substring of three characters or more in
 * either script, and `browse::search` answers shorter ones with `LIKE`. So
 * 花嫁 finds 五等分の花嫁 — which the shipped `unicode61` index could not do
 * at any query length, because it held the title as a single token.
 */
import { useEffect, useMemo, useRef, useState } from 'react'
import { useSearchParams } from 'react-router'
import { debounce } from './debounce'
import { EmptyState } from './EmptyState'
import {
  search as fetchSearch,
  type LibraryStatus,
  libraryParam,
  type SearchHits,
} from './library'
import { SeriesGrid } from './SeriesGrid'
import { messageOf } from './message'
import { useLibraryStatus } from './shell'
import { useDocumentTitle } from './title'

/**
 * The library a search is narrowed to: the URL's, while the status is in
 * flight or once it lists that library among two or more, and otherwise
 * none. A `?library=` that `libraryParam` reads as no library id arrives here
 * as none.
 *
 * Where the status does not list it — a stale link, a library outside the
 * grant — or lists one library only, the "Search in" select could not show
 * it: the first has no option for it, and the second draws no select.
 * Narrowing there would put a search of one library under a screen that says
 * every library, or that says nothing and offers no way back. The URL is left
 * as it is.
 *
 * **While the status is still in flight, the URL's value stands.** Otherwise
 * a link naming a library the status lists would be searched across every
 * library first whenever the status landed after the debounce, and again,
 * narrowed, once it did.
 */
function searchedLibrary(
  fromUrl: number | undefined,
  status: LibraryStatus | undefined,
): number | undefined {
  if (fromUrl === undefined || status === undefined) return fromUrl
  if (status.libraries.length < 2) return undefined
  return status.libraries.some((l) => l.id === fromUrl) ? fromUrl : undefined
}

export function Search() {
  useDocumentTitle('Search')
  const [params, setParams] = useSearchParams()
  const q = params.get('q') ?? ''
  const fromUrl = libraryParam(params.get('library'))
  const status = useLibraryStatus()
  const library = searchedLibrary(
    fromUrl === 'invalid' ? undefined : fromUrl,
    status,
  )

  const [hits, setHits] = useState<SearchHits | undefined>()
  const [failure, setFailure] = useState<string | undefined>()
  const latest = useRef(0)

  // One debounced function for the life of the component: a fresh one per
  // render would have nothing pending to collapse into, which is the whole
  // point of debouncing.
  const run = useMemo(
    () =>
      debounce((query: string, library: number | undefined) => {
        const seq = ++latest.current
        fetchSearch(query, library)
          .then((next) => {
            if (seq !== latest.current) return
            setHits(next)
            setFailure(undefined)
          })
          .catch((err: unknown) => {
            if (seq !== latest.current) return
            setFailure(messageOf(err))
          })
      }, 250),
    [],
  )

  // A third act for `cancel`, alongside the emptied box below: unmounting
  // mid-debounce also replaces "what's being searched for" outright — there
  // is no longer a screen for the eventual answer to land on — so the
  // pending call must be dropped here too, not just when the box is cleared.
  // `Library`, `Shell` and `Series` all carry an equivalent `cancelled` flag
  // for their own fetch effects; this is the debounced version of the same
  // rule.
  useEffect(() => {
    return () => {
      run.cancel()
    }
  }, [run])

  useEffect(() => {
    if (q === '') {
      // Clearing the box is exactly the kind of act `debounce.ts`'s own doc
      // reserves `cancel` for: it replaces "what's being searched for"
      // outright, rather than changing it, so a call still waiting out its
      // debounce — or already in flight — must not land after the box is
      // empty. `run.cancel()` drops the former; bumping `latest.current`
      // invalidates the latter, so its `seq !== latest.current` guard
      // discards the response instead of rendering it over an empty box.
      run.cancel()
      latest.current += 1
      setHits(undefined)
      setFailure(undefined)
      return
    }
    run(q, library)
  }, [q, library, run])

  return (
    <>
      {/* O163: one h1 per screen; the search box's own label says the rest. */}
      <h1 className="visually-hidden">Search</h1>
      <label className="search-box">
        <span>Search</span>
        <input
          type="search"
          value={q}
          onChange={(e) => {
            // `replace`, not a push: typing six characters must not put six
            // entries in the reader's history for Back to walk out of. A
            // copy of the params rather than `{ q }`, so the library the
            // search is narrowed to stays in the URL while the box changes.
            const next = new URLSearchParams(params)
            if (e.target.value === '') next.delete('q')
            else next.set('q', e.target.value)
            setParams(next, { replace: true })
          }}
        />
      </label>
      {status && status.libraries.length > 1 && (
        <label className="search-scope">
          <span>Search in</span>
          <select
            value={library === undefined ? '' : String(library)}
            onChange={(e) => {
              const next = new URLSearchParams(params)
              if (e.target.value === '') next.delete('library')
              else next.set('library', e.target.value)
              setParams(next, { replace: true })
            }}
          >
            <option value="">Every library</option>
            {status.libraries.map((l) => (
              <option key={l.id} value={l.id}>
                {l.name}
              </option>
            ))}
          </select>
        </label>
      )}
      {failure !== undefined && <p role="alert">{failure}</p>}
      {hits &&
        (hits.series.length === 0 && hits.unfiled.length === 0 ? (
          <EmptyState kind="no-matches" query={hits.query} />
        ) : (
          <SeriesGrid series={hits.series} unfiled={hits.unfiled} />
        ))}
    </>
  )
}
