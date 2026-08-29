import { Link } from 'react-router'
import { Cover } from './Cover'
import { type SeriesCard, unreadable, type WorkCard } from './library'

/**
 * The library's tiles: series, then the works that belong to none.
 *
 * **One component because two screens draw the same thing.** `/` and `/search`
 * both answer with §5.4's `{ series, unfiled }` shape — deliberately, so one
 * component renders either — and two copies of a cover fallback is two places
 * for the `⇄` mark to end up in different positions, which is exactly what
 * Plan A's ledger carried forward.
 */
export function SeriesGrid({
  series,
  unfiled,
}: {
  series: SeriesCard[]
  unfiled: WorkCard[]
}) {
  return (
    <ul className="grid">
      {series.map((s) => (
        <li key={s.id}>
          <Link to={`/series/${s.id}`}>
            <span className="cover-box">
              <Cover src={s.cover} title={s.title} />
              {s.aligned > 0 && <span className="aligned">⇄ {s.aligned}</span>}
            </span>
            <span className="title">{s.title}</span>
          </Link>
        </li>
      ))}
      {unfiled.map((w) => (
        <li key={w.slug}>
          {unreadable(w) ? (
            // **O62 — the link is the treatment, not a badge.** I1 of the
            // whole-branch review: this component draws `/`'s and
            // `/search`'s unfiled works, and the reader link carried none
            // of the guard `Series.tsx`'s `Table` already had. The volume
            // keeps its name, same as there.
            <span className="tile-unreadable">
              <span className="title">{w.title}</span>{' '}
              <span className="unreadable">
                files missing — cannot be opened
              </span>
            </span>
          ) : (
            // An unfiled work has no series page — §5.4 — so it links
            // straight into the reader.
            <Link to={`/read/${w.slug}`}>
              <span className="title">{w.title}</span>
            </Link>
          )}
        </li>
      ))}
    </ul>
  )
}
