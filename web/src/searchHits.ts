/**
 * The search a box asks for, over the endpoint that shipped in #81 — spec C1
 * §11 step 9. The Library view draws the box and the hits (`SearchBar.tsx`,
 * `Library.tsx`); this is the request half.
 *
 * **Read through the store** (`searchRead`, `usePage`), one address per
 * query and library: a return to a search draws the hits kept for it in its
 * first render and reads them again, and only the answer for the query read
 * now is drawn, whatever order the answers land in.
 *
 * **An arrival is read at once; typing waits.** A query the view arrives
 * with (the hook's first, or one Back, Forward or a link sets) is read at
 * once. One the box typed is read once `TYPING_DELAY_MS` pass with no
 * further key.
 *
 * **This is the one place a view bends `usePage`'s first-visit rule** (spec
 * `2026-10-28-loading-library-design.md` §6.2). While a typed query waits or is
 * out, what was drawn when it was sent stays drawn, with no wait line, so a key
 * never blanks the hits; the next query's hits replace them in one step. An
 * arrival draws nothing of the query before it, as a first visit draws nothing
 * of another page: what is kept for it, in the render it arrives in, or nothing
 * and then `Loading…`.
 *
 * **What the tokeniser makes possible here**, and what it does not: migration
 * 8's `trigram` index answers any substring of three characters or more in
 * either script, and `browse::search` answers shorter ones with `LIKE`. So
 * 花嫁 finds 五等分の花嫁 — which the shipped `unicode61` index could not do
 * at any query length, because it held the title as a single token.
 */
import { useEffect, useRef, useState } from 'react'
import type { SearchHits } from './library'
import { messageOf } from './message'
import { searchRead } from './pageReads'
import { usePage } from './usePage'

/** The Library view's search box's id, which the phone bar's search icon focuses (`Shell.tsx`). */
export const SEARCH_BOX_ID = 'library-search'

/**
 * The part of a navigation's state the search box set, the tag that marks a
 * change to the address as the box's own (`SearchBar.tsx`, O223), or `null`
 * where it set none: what a change the box did not make carries over.
 */
export function boxStateOf(state: unknown): { searchBox: string } | null {
  const tag = (state as { searchBox?: unknown } | null)?.searchBox
  return typeof tag === 'string' ? { searchBox: tag } : null
}

/**
 * How long the box waits after a key before it reads: typing 五等分の花嫁 is
 * six keystrokes and one request.
 */
const TYPING_DELAY_MS = 250

/** A search: its words, and the library it is narrowed to. */
interface Query {
  readonly q: string
  readonly library: number | undefined
}

/** What one search read to the end gave the view to draw. */
interface Shown {
  readonly hits: SearchHits | undefined
  readonly failure: string | undefined
}

/** Whether `a` and `b` are the same search; `undefined` is none. */
function same(a: Query | undefined, b: Query | undefined): boolean {
  if (a === undefined || b === undefined) return a === b
  return a.q === b.q && a.library === b.library
}

/**
 * The hits for `q` within `library` (`undefined`: every library the account
 * may read). An empty `q` is no search: no hits, no failure, nothing sent,
 * and nothing kept to draw under the next query. `typed` says `q` is the
 * box's own change, which waits out the delay; any other is an arrival.
 */
export function useSearchHits(
  q: string,
  library: number | undefined,
  typed: boolean,
): {
  hits: SearchHits | undefined
  failure: string | undefined
  /**
   * A search is out with nothing to draw for it yet. The view draws its
   * wait line from this, not the hook, since the view also waits before it
   * offers the box, and one wait across both keeps the line from going and
   * coming back as the query is sent.
   */
  waiting: boolean
} {
  const wanted: Query | undefined = q === '' ? undefined : { q, library }
  /** The search read now: the one wanted, once a typed one has waited. */
  const [asked, setAsked] = useState(wanted)
  /** What was drawn as the typed search `asked` was sent, until it lands. */
  const [held, setHeld] = useState<Shown | undefined>()

  // Set while rendering, so an arrival draws what is kept for it in the
  // render it arrives in, and never the hits of the search before it. A box
  // emptied is set here too: it replaces the search rather than changing it.
  const arriving = !same(asked, wanted) && (wanted === undefined || !typed)
  if (arriving) {
    setAsked(wanted)
    setHeld(undefined)
  }
  // What follows is worked out for the search as this render leaves it. React
  // runs the render again before drawing it, and the caller's own state set
  // while rendering from what the first run returned is kept: a wait it saw
  // fall would start again.
  const reading = arriving ? wanted : asked
  const holding = arriving ? undefined : held

  const page = usePage<[SearchHits]>(
    reading === undefined ? [] : [searchRead(reading.q, reading.library)],
  )
  const failed = page.failures[0]
  const shown: Shown | undefined =
    reading === undefined
      ? undefined
      : page.ready
        ? {
            hits: page.answers[0],
            failure: failed === undefined ? undefined : messageOf(failed),
          }
        : holding
  /** What the last render drew, which a typed search holds as it is sent. */
  const drawn = useRef(shown)
  useEffect(() => {
    drawn.current = shown
  })

  // Only a typed search is ever pending: the render sets every other at
  // once. Each key restarts the wait, and a box emptied or a view gone ends
  // it, so only the words standing when the typing stops are read.
  const pending = !same(reading, wanted)
  useEffect(() => {
    if (!pending) return
    const timer = setTimeout(() => {
      setHeld(drawn.current)
      setAsked({ q, library })
    }, TYPING_DELAY_MS)
    return () => clearTimeout(timer)
  }, [pending, q, library])

  return {
    hits: shown?.hits,
    failure: shown?.failure,
    waiting: reading !== undefined && !page.ready && shown === undefined,
  }
}
