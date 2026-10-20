/**
 * The persistent navigation — spec C1 §2, and the sidebar spec's §1: the
 * "Libraries" heading, a link to every library, over a link per library.
 * The column's foot (the theme switch and More, `AccountMenu.tsx`) is
 * `Shell.tsx`'s.
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
 */
import { Link, useLocation, useSearchParams } from 'react-router'
import { type LibraryEntry, libraryParam } from './library'

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
  return (
    <nav className="sidebar" aria-label="Library">
      <h2 className="sidebar-heading">
        <Link
          to="/"
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
                to={`/?library=${library.id}`}
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
