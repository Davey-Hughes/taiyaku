import { Link } from 'react-router'
import { Cover } from './Cover'
import type { SeriesCard, WorkCard } from './library'

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
          {/* An unfiled work has no series page — §5.4 — so it links
              straight into the reader. */}
          <Link to={`/read/${w.slug}`}>
            <span className="title">{w.title}</span>
          </Link>
        </li>
      ))}
    </ul>
  )
}
