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
 * `showRoots` is a prop rather than a preference read here: the `browse` module
 * that makes it a setting is §11 step 8, in Plan B. Until then the default is
 * the caller's, and `Library` reads it from `Gate.tsx`'s account — on for an
 * admin, off for everyone else, off where no gate says. **This paragraph
 * described that before it was true**: `Library` passed a hardcoded `false`
 * from the day it was written until 2026-08-21, so no deployment ever drew a
 * root link and the four things behind one were unreachable. Stating a
 * caller's contract does not make the caller keep it.
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
