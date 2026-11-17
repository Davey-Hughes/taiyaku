import { useRef } from 'react'
import { Link } from 'react-router'
import { Cover } from './Cover'
import {
  narrowedTo,
  readerPath,
  type SeriesCard,
  unreadable,
  type WorkCard,
} from './library'
import { useReseat } from './reseat'

/** "42 volumes", "1 volume": a series' `works`, as its tile says it. */
function volumeCount(works: number): string {
  return works === 1 ? '1 volume' : `${works} volumes`
}

/**
 * The library's tiles: series, then the works that belong to none.
 *
 * **One component because the Library view draws the same thing twice.** Its
 * grid and its search hits both answer with §5.4's `{ series, unfiled }` shape
 * — deliberately, so one component renders either — and two copies of a cover
 * fallback is two places for the `⇄` mark to end up in different positions.
 */
export function SeriesGrid({
  series,
  unfiled,
  library,
}: {
  series: SeriesCard[]
  unfiled: WorkCard[]
  /**
   * The library the tiles are drawn for. A series can hold works under
   * several libraries' directories, so its page is opened with `?library=`
   * to count what the tile counted (spec §3.6).
   */
  library?: number
}) {
  const list = useRef<HTMLUListElement>(null)
  useReseat(list, true)
  return (
    <ul ref={list} className="grid">
      {series.map((s) => (
        <li key={s.id}>
          <Link to={narrowedTo(`/series/${s.id}`, library)}>
            <span className="cover-box">
              <Cover src={s.cover} title={s.title} />
              {s.aligned > 0 && <span className="aligned">⇄ {s.aligned}</span>}
            </span>
            <span className="title">{s.title}</span>
            <span className="meta">{volumeCount(s.works)}</span>
          </Link>
        </li>
      ))}
      {unfiled.map((w) => (
        <li key={w.slug}>
          {unreadable(w) ? (
            // **O62 — the link is the treatment, not a badge.** This component
            // draws the grid's and the search hits' unfiled works, which need
            // the same guard as `Series.tsx`'s `Table`. The volume keeps its
            // name, same as there.
            <span className="tile-unreadable">
              <span className="title">{w.title}</span>{' '}
              <span className="unreadable">
                files missing — cannot be opened
              </span>
            </span>
          ) : (
            // An unfiled work has no series page — §5.4 — so it links
            // straight into the reader.
            <Link to={readerPath(w)}>
              <span className="title">{w.title}</span>
            </Link>
          )}
        </li>
      ))}
    </ul>
  )
}
