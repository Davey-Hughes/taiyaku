/**
 * `/search` sends a search link to the Library view's search bar: it lands
 * on `/` with its query and its library intact, so a bookmarked or shared
 * search still opens on its hits, `replace`d so Back does not return to a
 * redirect. An empty `library` is dropped, since `?library=` with no value is
 * no library and the link meant every one.
 *
 * The search itself is `searchHits.ts` and `SearchBar.tsx`, drawn by
 * `Library.tsx`.
 */
import { Navigate, useSearchParams } from 'react-router'

export function Search() {
  const [params] = useSearchParams()
  const next = new URLSearchParams()
  for (const key of ['q', 'library']) {
    const value = params.get(key)
    if (value !== null && value !== '') next.set(key, value)
  }
  const query = next.toString()
  return (
    <Navigate
      to={{ pathname: '/', search: query ? `?${query}` : '' }}
      replace
    />
  )
}
