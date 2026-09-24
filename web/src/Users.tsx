/**
 * The users screen — spec C4 §4 and the build design's §6.2.
 *
 * **The list owns who exists; each card owns what it is doing.**
 * `Roots`/`RootRow`'s split, and the same reason: a patch, a revocation and a
 * dry run belong to the account being administered.
 *
 * **Two failure slots, not one.** `reload` is handed to every card as
 * `onChanged`, so a revocation, a capability toggle or a removal fires a list
 * read — and a single slot meant any of them silently erased the add form's
 * refusal, which is the one message naming why an account could not be
 * created. Found on the roots screen by PR A's second review pass; written
 * this way here rather than found again.
 *
 * **The roots list is fetched here, once, and handed down.** `config.ts`
 * already has the client — one client and two screens rather than a second
 * fetch that could disagree (§6.2) — and a card fetching its own would ask N
 * times for one answer. **A failed read is drawn**, because the grant editor's
 * empty state and its failure look identical on screen and mean opposite
 * things: an empty grant is *unrestricted*.
 *
 * **The caller's own account comes from the gate, not from a second request.**
 * `useGate()` already holds `me()`'s answer for the page load. `undefined`
 * there means "no account known" and is safe: the server refuses a
 * self-demotion whether or not this screen drew the control.
 *
 * **Keyed by name rather than id.** `user.id` is `INTEGER PRIMARY KEY` with no
 * `AUTOINCREMENT`, so SQLite reissues a freed id to the next account created —
 * and React matching on `key` would hand a recycled id the removed account's
 * card instance, with its open removal dialog and typed password. `user.name`
 * is `UNIQUE`; a rename remounts the card, which is the right thing to lose.
 * `Roots.tsx` carries the same note about `root.id`.
 */
import { useCallback, useEffect, useRef, useState } from 'react'
import { listRoots, type Root } from './config'
import { useGate } from './gate'
import { addUser, listUsers, ROLES, type User } from './users'
import { UserRow } from './UserRow'
import { messageOf } from './message'

export function Users() {
  const [users, setUsers] = useState<User[] | undefined>()
  const [roots, setRoots] = useState<Root[] | undefined>()
  const [rootsFailure, setRootsFailure] = useState<string | undefined>()
  /** Why the list could not be read. Separate from `addFailure`. */
  const [failure, setFailure] = useState<string | undefined>()
  /** Why the last add was refused. Cleared only by the next add. */
  const [addFailure, setAddFailure] = useState<string | undefined>()
  const [name, setName] = useState('')
  const [password, setPassword] = useState('')
  const [role, setRole] = useState<string>('user')
  const [adding, setAdding] = useState(false)
  /** Whether an add is already on its way. See this file's doc comment. */
  const inFlight = useRef(false)
  /** The newest `listUsers` call — responses need not arrive in order. */
  const latest = useRef(0)
  const gate = useGate()

  /**
   * Re-read the list, and **hand the promise back**.
   *
   * Every card holds its own controls until the reload it triggered has
   * landed — `UserRow`'s `settled` is the other end of this — because between
   * a write's 200 and this answer a card is still drawing its pre-write props
   * with its controls live. Returning the promise is what lets it wait
   * without knowing anything about this screen's state.
   */
  const reload = useCallback(() => {
    const seq = ++latest.current
    return listUsers()
      .then((body) => {
        if (seq !== latest.current) return
        setUsers(body.users)
        setFailure(undefined)
      })
      .catch((err: unknown) => {
        if (seq !== latest.current) return
        setFailure(messageOf(err))
      })
  }, [])

  useEffect(() => {
    void reload()
  }, [reload])

  useEffect(() => {
    let cancelled = false
    listRoots()
      .then((body) => {
        if (cancelled) return
        setRoots(body.roots)
        setRootsFailure(undefined)
      })
      .catch((err: unknown) => {
        if (cancelled) return
        setRootsFailure(messageOf(err))
      })
    return () => {
      cancelled = true
    }
  }, [])

  const submit = (e: React.FormEvent) => {
    e.preventDefault()
    if (name === '' || password === '') return
    if (inFlight.current) return
    inFlight.current = true
    setAdding(true)
    setAddFailure(undefined)
    addUser(name, password, role)
      .then(() => {
        inFlight.current = false
        setAdding(false)
        setName('')
        setPassword('')
        setRole('user')
        void reload()
      })
      .catch((err: unknown) => {
        inFlight.current = false
        setAdding(false)
        setAddFailure(messageOf(err))
      })
  }

  return (
    <section className="users">
      <h2>Users</h2>
      {failure !== undefined && <p role="alert">{failure}</p>}
      {rootsFailure !== undefined && (
        <p role="alert">
          The roots list could not be read ({rootsFailure}), so the grant
          editors below show nothing rather than what this library has.
        </p>
      )}

      {users !== undefined && (
        <ul className="user-list">
          {users.map((account) => (
            <UserRow
              key={account.name}
              user={account}
              users={users}
              roots={roots}
              actingAs={gate?.who.user_id}
              onChanged={reload}
            />
          ))}
        </ul>
      )}

      {addFailure !== undefined && <p role="alert">{addFailure}</p>}
      <form onSubmit={submit} className="add-user">
        <h3>Add an account</h3>
        <label>
          <span>Name</span>
          {/*
            The same reason `Login.tsx`'s name field carries these: account
            names are compared byte for byte, and iOS capitalises the first
            letter of an empty text field. Here it mints an account whose
            name the person then cannot type on the login screen.
          */}
          <input
            type="text"
            name="new-name"
            value={name}
            autoCapitalize="none"
            autoCorrect="off"
            spellCheck={false}
            onChange={(e) => setName(e.target.value)}
          />
        </label>
        <label>
          <span>Password</span>
          <input
            type="password"
            name="new-password"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
          />
        </label>
        <label>
          <span>Role</span>
          <select
            name="new-role"
            value={role}
            onChange={(e) => setRole(e.target.value)}
          >
            {ROLES.map((option) => (
              <option key={option} value={option}>
                {option}
              </option>
            ))}
          </select>
        </label>
        <button
          type="submit"
          disabled={adding || name === '' || password === ''}
        >
          {adding ? 'Creating…' : 'Add'}
        </button>
        {/*
          `<code>` rather than backticks: this is a rendered page, and the
          backticks were drawn as themselves — found by opening the screen
          against the real library rather than by reading the JSX.
        */}
        <p className="why">
          C4 §4: there is no self-service registration — an administrator makes
          accounts. A new account is a reader unless this says otherwise, which
          is the column default <code>004_auth.sql</code> chose because any
          insert that omitted the role minted an administrator.
        </p>
      </form>
    </section>
  )
}
