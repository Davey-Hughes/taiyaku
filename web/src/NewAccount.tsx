/**
 * Making an account: a name, a password typed twice, and a role.
 *
 * **The refusal is drawn in the form**, and stays until the name is edited
 * or the next attempt: it names why the server refused, such as a name that
 * is taken or a role the schema refuses. A refusal that comes once the form
 * has gone is handed to the screen, which says it on the page the person is
 * on.
 */
import { type ReactNode, useEffect, useId, useRef, useState } from 'react'
import { accountNameClash, NEW_ACCOUNT, USERS } from './configAddress'
import { useConfirmPassword } from './confirmPassword'
import { HeldButton } from './HeldButton'
import { useDrawn, useDrawnWait, useFocusLeavesHeld } from './hooks'
import { reasonOf, refusalOf } from './message'
import { NameClash } from './NameClash'
import { PasswordsDiffer } from './PasswordsDiffer'
import { Refusal } from './Refusal'
import { PageHeader, Row, Section } from './settingsLayout'
import { addUser, ROLES, type User } from './users'
import { WaitWords } from './WaitWords'

export interface NewAccountProps {
  /**
   * The accounts the list holds, so that a name one of them holds is
   * refused as it is typed; `undefined` while the list is unread, when only
   * the server refuses it.
   */
  users: readonly Pick<User, 'name'>[] | undefined
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
  /** The person opened this from the table's link, so the heading takes focus. */
  arrived?: boolean
}

/** Add's words while the account is being made. */
const CREATING = 'Creating…'

export function NewAccount({
  users,
  notices,
  onAdded,
  onLateRefusal,
  onCancel,
  arrived = false,
}: NewAccountProps) {
  const heading = useRef<HTMLHeadingElement>(null)
  useEffect(() => {
    if (arrived) heading.current?.focus()
  }, [arrived])
  const [name, setName] = useState('')
  const [password, setPassword] = useState('')
  const [role, setRole] = useState<string>('user')
  const [adding, setAdding] = useState(false)
  const addingDrawn = useDrawnWait(adding)
  /**
   * Why the last add was refused. Cleared by an edit of the name or by the
   * next add.
   */
  const [failure, setFailure] = useState<string | undefined>()
  /** A ref as well as `adding`: two submits before a render both read false. */
  const inFlight = useRef(false)
  const drawn = useDrawn()
  /**
   * Why the server would refuse the name typed, as the list judges it. Add
   * is held at once while there is one, since it is no wait: the server
   * still judges every add, against a list another admin may have changed.
   */
  const clash = accountNameClash(name, users ?? [])
  /**
   * The clash as it is said: not while a refusal in the same words is
   * drawn, which an edit of the name takes away, so that one sentence is
   * never said twice. Beside a refusal in other words it is said, since it
   * is what holds Add.
   */
  const saidClash = failure === clash ? undefined : clash
  const clashNote = useId()
  const nameBox = useRef<HTMLInputElement>(null)
  const addButton = useRef<HTMLButtonElement>(null)
  const cancelButton = useRef<HTMLButtonElement>(null)
  const roleBox = useRef<HTMLSelectElement>(null)
  useFocusLeavesHeld(clash !== undefined || addingDrawn, addButton, nameBox)
  const confirmation = useConfirmPassword(password)
  const { differ } = confirmation
  // The wait holds Cancel too, which a person may have tabbed to.
  useFocusLeavesHeld(addingDrawn, cancelButton, nameBox)

  const submit = (e: React.FormEvent) => {
    e.preventDefault()
    if (name === '' || password === '' || clash !== undefined) return
    if (differ) {
      confirmation.heldPress()
      return
    }
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
        const notMade = `The account ${name} was not made`
        if (drawn.current) setFailure(refusalOf(err, notMade))
        else onLateRefusal?.(`${notMade} (${reasonOf(err)}).`)
      })
  }

  return (
    <div className="settings-page new-account">
      {/* Spec C4 §4's rule — "No self-service registration. An admin
          creates accounts, per A2 §11." — in the words this page shows. */}
      <PageHeader
        back={{ to: USERS, label: 'Users', closes: NEW_ACCOUNT }}
        title="New account"
        description="There is no sign-up: an administrator makes accounts."
        titleRef={heading}
      />
      {notices}
      <Section>
        <form onSubmit={submit} className="add-user settings-group">
          {/* O175: each `aria-label` below is its row label's words as
              written, so the name keeps their case whatever case a
              stylesheet draws the label in. */}
          <Row label="Name">
            {(ids) => (
              /*
                The same reason `Login.tsx`'s name field carries these:
                account names are compared byte for byte, and iOS
                capitalises the first letter of an empty text field. Here it
                mints an account whose name the person then cannot type on
                the login screen.
              */
              <input
                ref={nameBox}
                type="text"
                id={ids.control}
                name="new-name"
                aria-label="Name"
                aria-invalid={saidClash === undefined ? undefined : true}
                aria-describedby={
                  saidClash === undefined ? undefined : clashNote
                }
                value={name}
                autoCapitalize="none"
                autoCorrect="off"
                spellCheck={false}
                onChange={(e) => {
                  setFailure(undefined)
                  setName(e.target.value)
                }}
              />
            )}
          </Row>
          <Row label="Password">
            {(ids) => (
              <input
                type="password"
                id={ids.control}
                name="new-password"
                aria-label="Password"
                value={password}
                onChange={(e) => setPassword(e.target.value)}
              />
            )}
          </Row>
          <Row label="Confirm password">
            {(ids) => (
              <input
                type="password"
                id={ids.control}
                name="confirm-password"
                aria-label="Confirm password"
                {...confirmation.confirmBox}
              />
            )}
          </Row>
          {/* Laid over Role's select, whole, and never over Add or Cancel,
              so that Role's label stays in view; where its words take two
              lines it also reaches into Role's help, never a box. It takes
              a press, so one on it opens nothing; Tab still reaches Role
              (`Callout`). */}
          <PasswordsDiffer {...confirmation.line} over={roleBox} />
          <Row
            label="Role"
            help="A new account gets the user role unless this is set to admin."
          >
            {(ids) => (
              <select
                ref={roleBox}
                id={ids.control}
                name="new-role"
                aria-label="Role"
                aria-describedby={ids.help}
                value={role}
                onChange={(e) => setRole(e.target.value)}
              >
                {ROLES.map((option) => (
                  <option key={option} value={option}>
                    {option}
                  </option>
                ))}
              </select>
            )}
          </Row>
          {/* At the box's foot, after the last field, where a form's
              submit is looked for; the refusal beneath answers it. */}
          <div className="settings-actions">
            <HeldButton
              ref={addButton}
              type="submit"
              className="keeps-wait-width"
              held={differ}
              onHeldPress={confirmation.heldPress}
              disabled={
                addingDrawn ||
                name === '' ||
                password === '' ||
                clash !== undefined
              }
            >
              <WaitWords drawn={addingDrawn} words="Add" wait={CREATING} />
            </HeldButton>
            <button
              ref={cancelButton}
              type="button"
              name="cancel"
              disabled={addingDrawn}
              onClick={() => {
                if (!adding) onCancel()
              }}
            >
              Cancel
            </button>
          </div>
          {failure !== undefined && <Refusal>{failure}</Refusal>}
          <NameClash id={clashNote} sentence={saidClash} />
        </form>
      </Section>
    </div>
  )
}
