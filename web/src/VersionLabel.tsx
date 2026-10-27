/**
 * The build's label at the sidebar's foot, above More: the short commit, or
 * "unknown build" when the build carries none. It is a link to About, and
 * the way there for every account, so it is drawn with or without a commit.
 *
 * **A `*` after the commit marks a build whose tracked files differed from
 * it**, which `/api/about` answers as `dirty`. The link's
 * name says so in words, which a star read aloud does not.
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

  const label = answer === undefined ? undefined : labelOf(answer)
  return (
    <p className="version-label">
      {label !== undefined && (
        <NavLink to="/about" aria-label={label.name}>
          {label.text}
        </NavLink>
      )}
    </p>
  )
}

/** What the link reads, and the name it is announced by. */
function labelOf(answer: About): { text: string; name: string } {
  if (answer.commit === null) {
    return { text: 'unknown build', name: 'About, unknown build' }
  }
  const short = shortCommit(answer.commit)
  return answer.dirty
    ? {
        text: `${short}*`,
        name: `About, build ${short}, with uncommitted changes`,
      }
    : { text: short, name: `About, build ${short}` }
}
