/**
 * The build's label at the sidebar's foot, under More: the short commit, or
 * "unknown build" when the build carries none. It is a link to About, and
 * the way there for every account, so it is drawn with or without a commit.
 *
 * **Nothing is drawn until `/api/about` answers, and nothing if it fails**,
 * the open count's rule in `Shell.tsx` for the same reason: the label is an
 * ornament on every screen, and About itself is where a failure is shown.
 * The line it takes is held either way (`shell.css`), so the foot does not
 * move under the pointer when the answer lands.
 *
 * **Current on `/about` and the pages beneath it**, which `NavLink` marks
 * with `aria-current` on its own. More is not marked there: About is not
 * behind it, and `AccountMenu.tsx`'s `INSIDE` does not list it.
 */
import { useEffect, useState } from 'react'
import { NavLink } from 'react-router'
import { type About, about, shortCommit } from './about'

export function VersionLabel() {
  const [answer, setAnswer] = useState<About | undefined>()

  useEffect(() => {
    let cancelled = false
    about()
      .then((next) => {
        if (!cancelled) setAnswer(next)
      })
      .catch(() => {})
    return () => {
      cancelled = true
    }
  }, [])

  const commit = answer?.commit ?? null
  const short = commit === null ? undefined : shortCommit(commit)
  return (
    <p className="version-label">
      {answer !== undefined && (
        <NavLink
          to="/about"
          aria-label={
            short === undefined
              ? 'About, unknown build'
              : `About, build ${short}`
          }
        >
          {short ?? 'unknown build'}
        </NavLink>
      )}
    </p>
  )
}
