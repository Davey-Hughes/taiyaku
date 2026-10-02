/**
 * The persistent navigation — spec C1 §2.
 *
 * **`ROOTS` is provisional and is marked as such here rather than discovered
 * later.** A root is a *storage* fact, and spec B's whole argument is that
 * where a file sits should stop being how you find it. So the rule that keeps
 * it harmless: **no navigation path may require it.** Everything reachable
 * through a root link is reachable from the grid without one. If it turns out
 * to be an administrative tool it moves to C4 and nothing breaks.
 *
 * **`showRoots` is a prop, and `Sidebar`'s whole contract for it is to draw
 * the roots list when told to and stay silent about who does the telling.**
 * Who decides — an account's `admin` bit, a stored preference, some later mix
 * of both — is deliberately left undescribed here, because describing it here
 * is exactly what has gone stale twice on this branch. Task 3 rewrote this
 * paragraph when the account read moved from `Library` to `Shell`. It went
 * stale again one task later: Task 9's `browse` module (§11 step 8, no longer
 * future tense) replaced that account read with a persisted preference,
 * folding the account's `admin` bit in only as a first-load default that a
 * stored value overrides. Both times the paragraph broke for the same
 * reason — it was describing *another file's* behaviour instead of this
 * one's. The current answer is `browseModule.ts`'s and `Shell.tsx`'s to keep
 * correct; this file no longer tries to also keep it correct.
 *
 * **`admin` is a plain prop, unlike `showRoots`.** The proposals inbox is
 * spec C3's, and nothing yet gives an account a reason to hide it once they
 * can see it — no `browse`-module preference exists for it — so `Shell.tsx`
 * hands over the gate's own `admin` bit directly, the same value it computes
 * for `showRoots`'s default. `undefined` reads as "not an admin", the same
 * safe default `gate.ts`'s own doc comment gives for a screen mounted with no
 * gate above it at all.
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
 * **The screen you are on is marked by `NavLink`'s `aria-current="page"`**,
 * which `shell.css` draws in the accent (spec 2026-09-17 §7.1). `/` needs no
 * `end`: React Router's `NavLink` counts a longer path as inside `to` only
 * where the next character is `/`, and after `/` it never is — read in
 * react-router 8.3.0 — so a series page marks nothing rather than the
 * library, which `Sidebar.test.tsx` holds. `/config` has no `end`, so every
 * configuration tab marks it. The root links stay plain `Link`s: `NavLink`
 * compares the path and not the query, so every `/?root=N` would read as
 * current on the library.
 *
 * **This paragraph once described behaviour that was not true, and that is
 * worth keeping on record rather than editing away.** `Library` passed a
 * hardcoded `false` from the day it was written until 2026-08-21, so no
 * deployment ever drew a root link and the four things behind one were
 * unreachable. Stating a caller's contract does not make the caller keep
 * it — which is also why this version states as little of that contract as
 * `Sidebar` itself needs.
 */
import { Link, NavLink } from 'react-router'
import type { RootCard } from './library'

export interface SidebarProps {
  roots: RootCard[]
  showRoots: boolean
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
  roots,
  showRoots,
  admin = false,
  openProposals,
}: SidebarProps) {
  return (
    <nav className="sidebar" aria-label="Library">
      <ul className="sidebar-primary">
        <li>
          <NavLink to="/">Library</NavLink>
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
      {showRoots && roots.length > 0 && (
        <>
          <h2 className="sidebar-heading">Roots</h2>
          <ul className="sidebar-roots">
            {roots.map((root) => (
              <li key={root.id}>
                <Link to={`/?root=${root.id}`}>{root.name}</Link>
              </li>
            ))}
          </ul>
        </>
      )}
    </nav>
  )
}
