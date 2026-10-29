/**
 * The persistent navigation — spec C1 §2, and the sidebar spec's §1: the
 * "Libraries" heading, a link to every library, over a link per library.
 * The column's foot (the build's label, More in `AccountMenu.tsx`, and the
 * theme switch) is `Shell.tsx`'s.
 *
 * **The list is every account's** (spec F §3): the libraries
 * `/api/library/status` answers for it, each linking to its own page,
 * `/?library=N`. The server lists only the libraries an account may read, so
 * this draws whatever it is handed and asks nothing about who is looking.
 * **The heading is drawn with no library too**: `/` is where an account with
 * none is told so (O220), and the heading is its way there.
 *
 * **The screen you are on is marked `aria-current="page"`**, which
 * `shell.css` draws in the accent (spec 2026-09-17 §7.1), and marked here by
 * path and query, because `NavLink` compares the path and not the query: it
 * would mark the heading on every library's page. So `/` marks the heading,
 * `/?library=N` marks that library's entry and not the heading, and a series
 * page, whose path is not `/`, marks neither. Nor does a `?library=` that
 * `library.ts`'s `libraryParam` reads as no library id, where the library
 * page says there is no such library.
 *
 * **Beside the Library view, a library's link keeps its chip and sort**
 * (spec `2026-10-28-loading-library-design.md` §5.1): the link is a push to
 * that library's page, which would otherwise open at All and Title, so a switch
 * of library would drop what the reader chose. Nothing else is carried: a query
 * belongs to the library searched. The heading opens the defaults.
 *
 * **Every link lands at the top**, the view already drawn included (spec
 * `2026-10-28-loading-library-design.md` §1 and §5.2). A link to the address
 * shown is a replace, so a click adds no entry for Back, and the scroll keeper
 * leaves a replace's scroll where it is, as a chip's press needs; so each link
 * marks its navigation `TO_TOP`.
 */
import { Link, useLocation, useSearchParams } from 'react-router'
import {
  type LibraryEntry,
  libraryParam,
  type View,
  viewIn,
  withView,
} from './library'
import { TO_TOP } from './scrollKeeper'

export interface SidebarProps {
  libraries: LibraryEntry[]
}

export function Sidebar({ libraries }: SidebarProps) {
  const { pathname } = useLocation()
  const [params] = useSearchParams()
  // `/?library=N` is a library's page; the heading is current only on `/`
  // with no library named.
  const shown =
    pathname === '/' ? libraryParam(params.get('library')) : undefined
  const view: View =
    pathname === '/' ? viewIn(params) : { chip: 'all', sort: 'title' }
  /** Library `id`'s page, at the chip and sort the Library view shows. */
  const pageOf = (id: number) =>
    `/?${withView(new URLSearchParams({ library: String(id) }), view)}`
  return (
    <nav className="sidebar" aria-label="Library">
      <h2 className="sidebar-heading">
        <Link
          to="/"
          state={TO_TOP}
          aria-current={
            pathname === '/' && shown === undefined ? 'page' : undefined
          }
        >
          Libraries
        </Link>
      </h2>
      {libraries.length > 0 && (
        <ul className="sidebar-libraries">
          {libraries.map((library) => (
            <li key={library.id}>
              <Link
                to={pageOf(library.id)}
                state={TO_TOP}
                aria-current={shown === library.id ? 'page' : undefined}
              >
                {library.name}
              </Link>
            </li>
          ))}
        </ul>
      )}
    </nav>
  )
}
