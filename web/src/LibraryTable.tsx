/**
 * Every library, one row each, and the way to make another.
 *
 * **A row is a link to the library's page**, through the link on its name,
 * which the stylesheet stretches over the whole row: a row is one place to
 * go, and the name is what a screen reader reads it as.
 *
 * **Last scan is the latest of its folders'**, since a library is scanned
 * through its folders and each keeps its own.
 */
import { type ReactNode, useEffect, useRef } from 'react'
import { Link } from 'react-router'
import { lastScan } from './config'
import type { Library } from './libraries'

export interface LibraryTableProps {
  libraries: Library[] | undefined
  /**
   * What the screen says above the table: a list that could not be read, a
   * library's removal report, or an answer that came once the page it was
   * asked from had gone.
   */
  notices?: ReactNode
  /**
   * The person was sent here by an action whose button went with the page
   * it was on, so the heading takes focus.
   */
  arrived?: boolean
}

/** The folder of `library` scanned last, or `undefined` if none has been. */
function latestScanned(library: Library) {
  let latest: Library['folders'][number] | undefined
  for (const root of library.folders) {
    if (root.last_scan_at === null) continue
    // The server writes these as `YYYY-MM-DD HH:MM:SS`, which sort as text.
    if (latest?.last_scan_at == null || root.last_scan_at > latest.last_scan_at)
      latest = root
  }
  return latest
}

export function LibraryTable({
  libraries,
  notices,
  arrived = false,
}: LibraryTableProps) {
  const heading = useRef<HTMLHeadingElement>(null)
  useEffect(() => {
    if (arrived) heading.current?.focus()
  }, [arrived])
  return (
    <div className="library-table">
      <h2 ref={heading} tabIndex={-1}>
        Libraries
      </h2>
      {notices}
      <p className="new-item">
        <Link to="/config/libraries/new">New library</Link>
      </p>
      {libraries !== undefined &&
        (libraries.length === 0 ? (
          <p className="empty">
            No libraries yet. A library is a name and the folder its files are
            in; more folders can be added once it is made.
          </p>
        ) : (
          <table className="config-table">
            <thead>
              <tr>
                <th scope="col">Name</th>
                <th scope="col">Folders</th>
                <th scope="col">Works</th>
                <th scope="col">Last scan</th>
              </tr>
            </thead>
            <tbody>
              {libraries.map((library) => {
                const latest = latestScanned(library)
                return (
                  // By id: a library's id is never given to a later library.
                  <tr key={library.id}>
                    <td className="lead">
                      <Link
                        className="row-link"
                        to={`/config/libraries/${library.id}`}
                      >
                        {library.name}
                      </Link>
                    </td>
                    <td>
                      {library.folders.length}
                      <span className="cell-unit">
                        {' '}
                        {library.folders.length === 1 ? 'folder' : 'folders'}
                      </span>
                    </td>
                    <td>
                      {library.works}
                      <span className="cell-unit">
                        {' '}
                        {library.works === 1 ? 'work' : 'works'}
                      </span>
                    </td>
                    <td>
                      <span className="cell-label">Last scan: </span>
                      {latest === undefined ? 'Never' : lastScan(latest)}
                    </td>
                  </tr>
                )
              })}
            </tbody>
          </table>
        ))}
    </div>
  )
}
