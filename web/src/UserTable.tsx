/**
 * Every account, one row each, and the way to make another.
 *
 * **A row is a link to the account's page**, through the link on its name,
 * which the stylesheet stretches over the whole row, as the libraries table
 * does.
 *
 * **Libraries says what the account sees, from `every_library` first.** An
 * empty grant list is a limit to no library, never every library, so the
 * flag is read before the names. An administrator sees every library
 * whatever is chosen, and where the choice says otherwise the cell says so.
 */
import { type ReactNode, useEffect, useRef } from 'react'
import { Link } from 'react-router'
import { accountPath, NEW_ACCOUNT } from './configAddress'
import { JOINING_DOT } from './joiningDot'
import { ROW_CELLS, useLineEnds } from './lineEnds'
import { PageHeader, Section } from './settingsLayout'
import { lastSeen, seesLine, type User } from './users'
import { inUtc } from './utc'

export interface UserTableProps {
  users: User[] | undefined
  /** The account this browser is signed in as, where it is known. */
  actingAs?: number
  /** The account whose page is open beside the table, whose row says so. */
  opened?: number
  /** What the screen says above the table. */
  notices?: ReactNode
  /**
   * The person was sent here by an action whose button went with the page
   * it was on, so the heading takes focus.
   */
  arrived?: boolean
  /**
   * A row's or New account's link was followed, with the address it goes
   * to: the page opened takes focus, since the link that had it may be
   * hidden beside it or gone.
   */
  onOpen?: (to: string) => void
}

export function UserTable({
  users,
  actingAs,
  opened,
  notices,
  arrived = false,
  onOpen,
}: UserTableProps) {
  const heading = useRef<HTMLHeadingElement>(null)
  /** Which cells end a line where a row is drawn in two. */
  const cellEnds = useLineEnds<HTMLTableElement>(ROW_CELLS)
  useEffect(() => {
    if (arrived) heading.current?.focus()
  }, [arrived])
  return (
    <div className="user-table">
      <PageHeader
        title="Users"
        description="Who can sign in, what each account may do, and what it sees."
        titleRef={heading}
        actions={
          <span className="new-item">
            <Link to={NEW_ACCOUNT} onClick={() => onOpen?.(NEW_ACCOUNT)}>
              New account
            </Link>
          </span>
        }
      />
      {notices}
      {users !== undefined && (
        <Section>
          <table ref={cellEnds} className="config-table">
            <thead>
              <tr>
                <th scope="col">Name</th>
                <th scope="col">Role</th>
                <th scope="col">Libraries</th>
                <th scope="col">Last seen</th>
              </tr>
            </thead>
            <tbody>
              {users.map((user) => {
                const seen = lastSeen(user)
                const to = accountPath(user.name)
                return (
                  // A row holds no state, so either key would do; by name, to
                  // match the account's page.
                  <tr key={user.name}>
                    <td className="lead">
                      {/* The row whose account is open beside the table says
                          so. Marked by the account the screen found, not by
                          `NavLink`'s match of the address, which folds case
                          whole: "Sam" and "sam" are two accounts, and an
                          address may spell either name in another case. */}
                      <Link
                        className="row-link"
                        to={to}
                        aria-current={user.id === opened ? 'page' : undefined}
                        onClick={() => onOpen?.(to)}
                      >
                        {user.name}
                      </Link>
                      {actingAs === user.id && (
                        <span className="mark">{JOINING_DOT}this is you</span>
                      )}
                      {!user.can_log_in && (
                        <span className="mark locked">{JOINING_DOT}locked</span>
                      )}
                    </td>
                    <td>
                      <span className="cell-label">Role: </span>
                      {user.role}
                    </td>
                    <td className="wraps">
                      <span className="cell-label">Libraries: </span>
                      {seesLine(user)}
                    </td>
                    <td>
                      <span className="cell-label">Last seen: </span>
                      {seen === undefined ? '—' : inUtc(seen)}
                    </td>
                  </tr>
                )
              })}
            </tbody>
          </table>
        </Section>
      )}
    </div>
  )
}
