/**
 * Making an account: a name, a password and a role.
 *
 * **The refusal is drawn in the form**, and stays until the next attempt: it
 * names whether the name is taken or the role is one the schema refuses. A
 * refusal that comes once the form has gone is handed to the screen, which
 * says it on the page the person is on.
 */
import { type ReactNode, useRef, useState } from 'react'
import { Link } from 'react-router'
import { useDrawn } from './hooks'
import { sentenceOf } from './message'
import { Refusal } from './Refusal'
import { addUser, ROLES, type User } from './users'

export interface NewAccountProps {
  /**
   * What the screen says above the page: a list that could not be read, or
   * an answer that came once the page it was asked from had gone.
   */
  notices?: ReactNode
  /**
   * The account is made; the screen keeps it. `stillHere` says this form is
   * still drawn, so the person is where they asked and its page is opened. A
   * form left before the answer came is not, even when the person has since
   * opened a fresh one at the same address.
   */
  onAdded: (user: User, stillHere: boolean) => void
  /**
   * The make was refused after this form was left, with the sentence that
   * says so and names the account.
   */
  onLateRefusal?: (sentence: string) => void
  /** Cancel was pressed: the screen goes back to the table. */
  onCancel: () => void
}

export function NewAccount({
  notices,
  onAdded,
  onLateRefusal,
  onCancel,
}: NewAccountProps) {
  const [name, setName] = useState('')
  const [password, setPassword] = useState('')
  const [role, setRole] = useState<string>('user')
  const [adding, setAdding] = useState(false)
  /** Why the last add was refused. Cleared only by the next add. */
  const [failure, setFailure] = useState<string | undefined>()
  /** A ref as well as `adding`: two submits before a render both read false. */
  const inFlight = useRef(false)
  const drawn = useDrawn()

  const submit = (e: React.FormEvent) => {
    e.preventDefault()
    if (name === '' || password === '') return
    if (inFlight.current) return
    inFlight.current = true
    setAdding(true)
    setFailure(undefined)
    addUser(name, password, role)
      .then((user) => {
        inFlight.current = false
        setAdding(false)
        onAdded(user, drawn.current)
      })
      .catch((err: unknown) => {
        inFlight.current = false
        setAdding(false)
        const sentence = sentenceOf(err)
        if (drawn.current) setFailure(sentence)
        else onLateRefusal?.(`The account ${name} was not made (${sentence}).`)
      })
  }

  return (
    <div className="new-account">
      <p className="back">
        <Link to="/config/users">‹ Users</Link>
      </p>
      <h2>New account</h2>
      {notices}
      <form onSubmit={submit} className="add-user">
        {/* O175: each `aria-label` below repeats its label's words because
            `.field` draws them in capitals, and Chromium's accessibility
            tree named the control in those capitals. */}
        <label className="field">
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
            aria-label="Name"
            value={name}
            autoCapitalize="none"
            autoCorrect="off"
            spellCheck={false}
            onChange={(e) => setName(e.target.value)}
          />
        </label>
        <label className="field">
          <span>Password</span>
          <input
            type="password"
            name="new-password"
            aria-label="Password"
            value={password}
            onChange={(e) => setPassword(e.target.value)}
          />
        </label>
        <label className="field">
          <span>Role</span>
          <select
            name="new-role"
            aria-label="Role"
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
        <div className="actions">
          <button
            type="submit"
            disabled={adding || name === '' || password === ''}
          >
            {adding ? 'Creating…' : 'Add'}
          </button>
          <button
            type="button"
            name="cancel"
            disabled={adding}
            onClick={onCancel}
          >
            Cancel
          </button>
        </div>
        {failure !== undefined && <Refusal>{failure}</Refusal>}
        {/* Spec C4 §4's rule — "No self-service registration. An admin
            creates accounts, per A2 §11." — in the words this form shows. */}
        <p className="why">
          There is no sign-up: an administrator makes accounts. A new account
          gets the user role unless Role is set to admin.
        </p>
      </form>
    </div>
  )
}
