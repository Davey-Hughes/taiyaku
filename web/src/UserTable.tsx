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
import { lastSeen, seesLine, type User } from './users'

export interface UserTableProps {
  users: User[] | undefined
  /** The account this browser is signed in as, where it is known. */
  actingAs?: number
  /** What the screen says above the table. */
  notices?: ReactNode
  /**
   * The person was sent here by an action whose button went with the page
   * it was on, so the heading takes focus.
   */
  arrived?: boolean
}

export function UserTable({
  users,
  actingAs,
  notices,
  arrived = false,
}: UserTableProps) {
  const heading = useRef<HTMLHeadingElement>(null)
  useEffect(() => {
    if (arrived) heading.current?.focus()
  }, [arrived])
  return (
    <div className="user-table">
      <h2 ref={heading} tabIndex={-1}>
        Users
      </h2>
      {notices}
      <p className="new-item">
        <Link to="/config/users/new">New account</Link>
      </p>
      {users !== undefined && (
        <table className="config-table">
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
              return (
                // A row holds no state, so either key would do; by name, to
                // match the account's page.
                <tr key={user.name}>
                  <td className="lead">
                    <Link className="row-link" to={`/config/users/${user.id}`}>
                      {user.name}
                    </Link>
                    {actingAs === user.id && (
                      <span className="mark"> · this is you</span>
                    )}
                    {!user.can_log_in && (
                      <span className="mark locked"> · locked</span>
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
                    {seen ?? '—'}
                  </td>
                </tr>
              )
            })}
          </tbody>
        </table>
      )}
    </div>
  )
}
