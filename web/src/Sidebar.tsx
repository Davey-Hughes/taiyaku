/**
 * The persistent navigation — spec C1 §2.
 *
 * **The LIBRARIES list is every account's** (spec F §3): the libraries
 * `/api/library/status` answers for it, each linking to its own page,
 * `/?library=N`. The server lists only the libraries an account may read, so
 * this draws whatever it is handed and asks nothing about who is looking. With
 * no library it draws no heading either.
 *
 * **`admin` is a plain prop.** The proposals inbox is spec C3's, and nothing
 * yet gives an account a reason to hide it once they can see it — no
 * `browse`-module preference exists for it — so `Shell.tsx` hands over the
 * gate's own `admin` bit directly. `undefined` reads as "not an admin", the
 * same safe default `gate.ts`'s own doc comment gives for a screen mounted
 * with no gate above it at all.
 *
 * **`admin` now gates two entries, and the second is spec C4's.** The
 * settings area is admin-only for the same reason the proposals inbox is, and
 * takes the same prop rather than a second one: what an account may *see* is
 * the server's to enforce — every `/api/library/{rules,roots,users,backups}`
 * route answers a reader 403 — and this chooses only what to draw. A
 * non-admin who types `/config` reaches it and is shown the server's own
 * sentence, which is exactly what `/proposals` already does.
 *
 * **`/account` is drawn for everyone, and takes no prop at all.** O94: the
 * account screen — this account's password, API keys and sessions — was
 * reachable only from inside the reader until it had a route, and the users
 * screen refuses an administrator's reset of their own password by sending
 * them there. Every account has a password to change, so the two admin-only
 * entries beside it are the exception here, not the pattern to copy.
 *
 * **`openProposals` is the count spec C3 §6 and the build design's
 * file-structure table both asked for, and this entry shipped without.** The
 * deviation was recorded rather than silently taken, and its stated cost was
 * that this component mounts around every screen, so a live count is a
 * request on every page load. What settled it is that the request in question
 * is the flat one: `GET /api/library/proposals` reads `proposal` — joined to
 * `series` for the titles, grouped for the counts — and neither compiles a
 * rule nor walks `scan_entry`, which is what the detail and dry-run routes
 * both do. 0.10 ms at 8 rows and 0.23 ms at 25,008 (`notes.md`, 2026-08-26),
 * against 1.1–1.2 µs per entry for those two. Admin-only besides, while the
 * premise of C3 §3's whole argument is that somebody learns the inbox is
 * non-empty, and nothing else in this app ever told them.
 *
 * The other half of the deviation was a real question — *what* would it
 * count, given the screen defaults to score 1 and above and a badge counting
 * every open proposal would send somebody to an empty list. `INBOX_MIN_SCORE`
 * is the answer: one constant, read by this count and by the screen's own
 * first request, so the two cannot mean different things. This file does not
 * hold it, and draws the number it is handed.
 *
 * **The screen you are on is marked `aria-current="page"`**, which
 * `shell.css` draws in the accent (spec 2026-09-17 §7.1). `NavLink` marks the
 * entries a path alone names. `/config` has no `end`, so every configuration
 * tab marks it. **The Library entry and the library entries are marked here,
 * by path and query**, because `NavLink` compares the path and not the query:
 * it would mark the Library entry on every library's page, and every library
 * entry on the Library. So `/` marks the Library entry, `/?library=N` marks
 * that library's entry and not the Library's, and a series page, whose path
 * is not `/`, marks neither. Nor does a `?library=` that `library.ts`'s
 * `libraryParam` reads as no library id, where the library page says there
 * is no such library. A search narrowed to a library marks Search.
 */
import { Link, NavLink, useLocation, useSearchParams } from 'react-router'
import { type LibraryEntry, libraryParam } from './library'

export interface SidebarProps {
  libraries: LibraryEntry[]
  /** Admin-only: the proposals inbox and the settings area. See this file's own doc comment. */
  admin?: boolean
  /**
   * Open proposals at `curation.ts`'s `INBOX_MIN_SCORE`, once known.
   *
   * `undefined` covers never fetched, still in flight and failed alike — the
   * same three-states-as-one `StatusContext` uses — and draws no badge.
   * Zero is *not* that: an inbox somebody has emptied is worth saying.
   */
  openProposals?: number
}

export function Sidebar({
  libraries,
  admin = false,
  openProposals,
}: SidebarProps) {
  const { pathname } = useLocation()
  const [params] = useSearchParams()
  // `/?library=N` is a library's page. `NavLink` compares the path and not the
  // query, so it would mark the Library entry there too: both are marked here.
  const shown =
    pathname === '/' ? libraryParam(params.get('library')) : undefined
  return (
    <nav className="sidebar" aria-label="Library">
      <ul className="sidebar-primary">
        <li>
          <Link
            to="/"
            aria-current={
              pathname === '/' && shown === undefined ? 'page' : undefined
            }
          >
            Library
          </Link>
        </li>
        <li>
          <NavLink to="/search">Search</NavLink>
        </li>
        {admin && (
          <li>
            <NavLink
              to="/proposals"
              // O164: a name computed from the word and the badge ran them
              // together ("Proposals17") and said nothing of what the number
              // counts. The label says it in the inbox heading's words, and
              // only while there is a number to say.
              aria-label={
                openProposals === undefined
                  ? undefined
                  : `Proposals, ${openProposals} open`
              }
            >
              Proposals
              {openProposals !== undefined && (
                <span className="count">{openProposals}</span>
              )}
            </NavLink>
          </li>
        )}
        {admin && (
          <li>
            <NavLink to="/config">Configuration</NavLink>
          </li>
        )}
        <li>
          <NavLink to="/account">Account</NavLink>
        </li>
      </ul>
      {libraries.length > 0 && (
        <>
          <h2 className="sidebar-heading">Libraries</h2>
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
        </>
      )}
    </nav>
  )
}
