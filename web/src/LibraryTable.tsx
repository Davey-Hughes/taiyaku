/**
 * Every library, one row each, and the way to make another.
 *
 * **A row is a link to the library's page**, through the link on its name,
 * which the stylesheet stretches over the whole row: a row is one place to
 * go, and the name is what a screen reader reads it as.
 *
 * **Last scan is the latest of its directories'**, since a library is scanned
 * through its directories and each keeps its own, and while a scan holds any
 * of them it is that scan's step and count, from the feed (O312). The
 * passes, the fingerprint pass and the match pass, hold none, so each is a
 * line over the table, not a row's.
 */
import { type ReactNode, useEffect, useRef } from 'react'
import { Link } from 'react-router'
import { lastScan } from './config'
import { libraryPath, NEW_LIBRARY } from './configAddress'
import { JOINING_DOT } from './joiningDot'
import type { Library } from './libraries'
import { ROW_CELLS, useLineEnds } from './lineEnds'
import { PassLine } from './PassLine'
import {
  fingerprintJob,
  jobsHolding,
  matchJob,
  stepLine,
  useScanFeed,
} from './scanFeed'
import { PageHeader, PageLoading, Section } from './settingsLayout'

export interface LibraryTableProps {
  /** `undefined` while the list has not been read, or could not be. */
  libraries: Library[] | undefined
  /**
   * What the screen says above the table: a list that could not be read, a
   * library's removal report, or an answer that came once the page it was
   * asked from had gone.
   */
  notices?: ReactNode
  /** The list's wait has held long enough to draw "Loading…" for it. */
  loadingDrawn?: boolean
  /**
   * The person was sent here by an action whose button went with the page
   * it was on, so the heading takes focus.
   */
  arrived?: boolean
}

/** The directory of `library` scanned last, or `undefined` if none has been. */
function latestScanned(library: Library) {
  let latest: Library['dirs'][number] | undefined
  for (const root of library.dirs) {
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
  loadingDrawn = false,
  arrived = false,
}: LibraryTableProps) {
  const heading = useRef<HTMLHeadingElement>(null)
  /** Which cells end a line where a row is drawn in two. */
  const cellEnds = useLineEnds<HTMLTableElement>(ROW_CELLS)
  const feed = useScanFeed()
  const pass = fingerprintJob(feed)
  const matching = matchJob(feed)
  useEffect(() => {
    if (arrived) heading.current?.focus()
  }, [arrived])
  return (
    <div className="library-table">
      <PageHeader
        title="Libraries"
        description="Each library is a name and the directories its files are in."
        titleRef={heading}
        actions={
          <span className="new-item">
            <Link to={NEW_LIBRARY}>New library</Link>
          </span>
        }
      />
      {notices}
      {pass !== undefined && <PassLine key={pass.id} job={pass} />}
      {matching !== undefined && <PassLine key={matching.id} job={matching} />}
      {libraries === undefined && <PageLoading drawn={loadingDrawn} />}
      {libraries !== undefined && (
        <Section>
          {libraries.length === 0 ? (
            <p className="empty">
              No libraries yet. A library is made with one directory, and more
              can be added on its page.
            </p>
          ) : (
            <table ref={cellEnds} className="config-table">
              <thead>
                <tr>
                  <th scope="col">Name</th>
                  <th scope="col">Directories</th>
                  <th scope="col">Works</th>
                  <th scope="col">Last scan</th>
                </tr>
              </thead>
              <tbody>
                {libraries.map((library) => {
                  const latest = latestScanned(library)
                  const job = jobsHolding(feed, library.id)[0]
                  return (
                    // By id: a library's id is never given to a later library.
                    <tr key={library.id}>
                      <td className="lead">
                        <Link
                          className="row-link"
                          to={libraryPath(library.name)}
                        >
                          {library.name}
                        </Link>
                      </td>
                      <td>
                        {library.dirs.length}
                        <span className="cell-unit">
                          {' '}
                          {library.dirs.length === 1
                            ? 'directory'
                            : 'directories'}
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
                        {job !== undefined
                          ? `Scanning${JOINING_DOT}${stepLine(job)}`
                          : latest === undefined
                            ? 'Never'
                            : lastScan(latest)}
                      </td>
                    </tr>
                  )
                })}
              </tbody>
            </table>
          )}
        </Section>
      )}
    </div>
  )
}
