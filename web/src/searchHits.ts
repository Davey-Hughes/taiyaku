/**
 * The search a box asks for, over the endpoint that shipped in #81 — spec C1
 * §11 step 9. The Library view draws the box and the hits (`SearchBar.tsx`,
 * `Library.tsx`); this is the request half.
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
import { debounce } from './debounce'
import { search as fetchSearch, type SearchHits } from './library'
import { messageOf } from './message'

/** The Library view's search box's id, which the phone bar's search icon focuses (`Shell.tsx`). */
export const SEARCH_BOX_ID = 'library-search'

/**
 * The hits for `q` within `library` (`undefined`: every library the account
 * may read). An empty `q` is no search: no hits, no failure, nothing pending.
 */
export function useSearchHits(
  q: string,
  library: number | undefined,
): { hits: SearchHits | undefined; failure: string | undefined } {
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

  return { hits, failure }
}
