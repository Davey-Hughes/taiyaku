/**
 * The Library view's search box, at the top of the page beside View.
 *
 * **`?q=` is in the URL**, so a search is a link: the same reason `?library=`
 * is a query parameter on the library screen rather than component state. The
 * library the view shows is the search's scope, so a search narrowed to one
 * library is a link as well (spec F §3), and the sidebar is what changes it.
 *
 * The label is visually hidden: the box sits alone in the page's heading row,
 * where its placeholder names the scope, and a screen reader is told "Search".
 */
import { useEffect, useRef, useState } from 'react'
import { useLocation, useSearchParams } from 'react-router'
import { boxStateOf, SEARCH_BOX_ID } from './searchHits'

/** The tag a navigation the box made carries in its state, if it has one. */
const tagOf = (state: unknown): string | undefined =>
  boxStateOf(state)?.searchBox

export function SearchBar({ placeholder }: { placeholder: string }) {
  const [params, setParams] = useSearchParams()
  const location = useLocation()
  const q = params.get('q') ?? ''

  // **The box draws its own text, not the URL's.** `BrowserRouter` commits a
  // location inside `startTransition`, so the URL's `q` lags the keystroke
  // that set it; a box drawn from it is put back to the older value by
  // React's controlled-input reset, and the next key lands on that.
  //
  // So `?q=` sets the text only on a change the box did not make (spec §2):
  // Back or Forward, a link, the redirect from `/search`. **Each change the
  // box makes is tagged** in the navigation's state, and `pending` holds the
  // tags it has issued that no committed location has carried yet. A
  // location carrying one is the box's own change arriving late — the router
  // can commit an earlier key's URL after the box has taken the next — so it
  // is not drawn, and it settles every tag issued before it. Any other
  // location replaces the text. A tag rather than the value, because the box
  // can ask for one value twice ("a", deleted, "a" again), and by value its
  // own late request and a link to the same query cannot be told apart. A tag
  // already settled, or one another mount of the box issued, is not pending,
  // so an entry Back restores is never taken for the box's own; the instance
  // part of the tag is what keeps a remounted box from matching an older
  // one's.
  const [text, setText] = useState(q)
  const [instance] = useState(() => Math.random().toString(36).slice(2))
  const issued = useRef(0)
  const pending = useRef<string[]>([])
  useEffect(() => {
    const tag = tagOf(location.state)
    const at = tag === undefined ? -1 : pending.current.indexOf(tag)
    if (at >= 0) {
      pending.current.splice(0, at + 1)
      return
    }
    pending.current = []
    setText(q)
  }, [location, q])
  return (
    <label className="search-box">
      <span className="visually-hidden">Search</span>
      <input
        id={SEARCH_BOX_ID}
        type="search"
        value={text}
        placeholder={placeholder}
        onChange={(e) => {
          setText(e.target.value)
          issued.current += 1
          const tag = `${instance}:${issued.current}`
          pending.current.push(tag)
          // `replace`, not a push: typing six characters must not put six
          // entries in the reader's history for Back to walk out of. A copy
          // of the params rather than `{ q }`, so the library the search is
          // narrowed to stays in the URL while the box changes.
          const next = new URLSearchParams(params)
          if (e.target.value === '') next.delete('q')
          else next.set('q', e.target.value)
          setParams(next, { replace: true, state: { searchBox: tag } })
        }}
      />
    </label>
  )
}
