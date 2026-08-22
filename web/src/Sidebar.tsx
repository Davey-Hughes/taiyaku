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
 * **This paragraph once described behaviour that was not true, and that is
 * worth keeping on record rather than editing away.** `Library` passed a
 * hardcoded `false` from the day it was written until 2026-08-21, so no
 * deployment ever drew a root link and the four things behind one were
 * unreachable. Stating a caller's contract does not make the caller keep
 * it — which is also why this version states as little of that contract as
 * `Sidebar` itself needs.
 */
import { Link } from 'react-router'
import type { RootCard } from './library'

export interface SidebarProps {
  roots: RootCard[]
  showRoots: boolean
}

export function Sidebar({ roots, showRoots }: SidebarProps) {
  return (
    <nav className="sidebar" aria-label="Library">
      <ul className="sidebar-primary">
        <li>
          <Link to="/">Library</Link>
        </li>
        <li>
          <Link to="/search">Search</Link>
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
