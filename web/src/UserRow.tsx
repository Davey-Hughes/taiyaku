/**
 * One account: what it is, what it may do, what is signed in as it, and what
 * removing it would cost — spec C4 §4 and the build design's §6.2.
 *
 * **The card owns its own writes, the list owns who exists.** `RootRow`'s
 * split and its stated reason: a patch, a revocation and a dry run all belong
 * to the account being administered, and the list reloads on `onChanged`.
 *
 * **The server is the authority; this disables and explains.** Every refusal
 * here is enforced in `taiyaku-library` and answered as a 409 with a sentence,
 * which this card renders when it arrives. Where it can tell in advance it
 * *also* disables the control and says why — and the duplication is
 * one-directional: this may refuse what the server would refuse, and may never
 * permit what the server would not. That is why `onlyAdminWhoCanLogIn` mirrors
 * `another_admin_can_log_in` rather than counting administrators.
 *
 * **One reason covers two controls.** The last-administrator rule reaches the
 * role dropdown and Remove through different routes and is one invariant; a
 * card that disabled only Remove leaves the identical accident one dropdown
 * away.
 *
 * **The self-password refusal names a screen with no URL of its own.**
 * `/api/password` is the route that keeps the session making the change, and
 * the screen that calls it is `Account.tsx` — mounted from `App.tsx` on
 * `/read/:work` and reachable only by opening a work and pressing `A`. That is
 * O94, and it is stated rather than worked around: the fix is a route, and it
 * belongs to the reader's chrome rather than to this branch.
 *
 * **Grants are sent whole.** There is no route that grants or revokes one
 * root; `account::Patch::roots` replaces the list, an absent key leaves it
 * alone, and `[]` means unrestricted rather than blind — spec A2 and O54,
 * which is why the empty case is a sentence on screen rather than a silence.
 *
 * **The confirm does not exist until the counts do**, and the real `DELETE` is
 * sent from here and nowhere else. `RemoveRoot`'s reasoning, on the route
 * `users_api` calls the second-most destructive in the product.
 */
import { useRef, useState } from 'react'
import type { Root } from './config'
import {
  onlyAdminWhoCanLogIn,
  patchUser,
  type RemovalAnswer,
  removalCost,
  removeUser,
  revokeUserKey,
  revokeUserSession,
  ROLES,
  type User,
  type UserPatch,
} from './users'
import { messageOf } from './message'

export interface UserRowProps {
  user: User
  /**
   * Every account, so the last-administrator rule is answered from the list
   * the screen already holds rather than by a second request per card.
   */
  users: User[]
  /**
   * This library's roots, for the grant editor — `undefined` where that list
   * has not been read.
   *
   * **Three states rather than two.** `roots ?? []` would draw "this library
   * has no roots yet" while the read was still in flight and for ever after
   * one that failed: a positive false claim about the library, on the one
   * control whose *empty* state means unrestricted. What the card can always
   * say is what this account is granted, because `user.roots` carries the
   * paths as well as the ids; what it cannot do without the list is offer a
   * root nobody granted yet.
   */
  roots?: Root[]
  /**
   * The account this browser is signed in as, or `undefined` where no gate
   * sits above — which `gate.ts` defines as "no account known", never "an
   * admin". Both readings are safe here: the server refuses either way.
   */
  actingAs?: number
  /**
   * Re-read the list this card is one of.
   *
   * **Its answer is awaited when it gives one.** `Users.reload` returns the
   * `listUsers` promise, and this card holds its controls until that settles
   * — see `settled`. A caller that returns nothing is not wrong, only
   * unheld.
   *
   * Typed `() => unknown` rather than as a union with `void`, which biome
   * refuses: what matters here is that a returned promise is awaited, not
   * what a caller that returns nothing is called.
   */
  onChanged: () => unknown
}

/**
 * `n` of a thing, in the number the count actually is.
 *
 * Zero takes the plural — "0 API keys" — which is English rather than an
 * oversight. `RemoveRoot` carries the same helper and the same note; it was
 * found by rendering that dialog in a real browser, where it read "1 works".
 */
function count(n: number, one: string, many: string): string {
  return `${n} ${n === 1 ? one : many}`
}

export function UserRow({
  user,
  users,
  roots,
  actingAs,
  onChanged,
}: UserRowProps) {
  const [failure, setFailure] = useState<string | undefined>()
  /**
   * What the last write on this card was answered with.
   *
   * **The hold alone was not enough, and this is the half that closes it.**
   * `settled` waits for the list read a write triggers, but that read can fail
   * or be superseded by another card's — and either way it resolves without
   * bringing fresh props, so the controls came back live over pre-write
   * values. `patchUser` answers the account whole, so the card can keep that
   * answer and stop guessing: it is what is drawn, and what `toggleGrant`
   * computes the next grant list from. Retired the moment a new `user` prop
   * arrives, because the list is the truth as soon as it catches up. Found by
   * the second whole-branch review pass, in the fix the first one produced.
   */
  const [applied, setApplied] = useState<User | undefined>()
  const [saving, setSaving] = useState(false)
  const [password, setPassword] = useState('')
  const [wasReset, setWasReset] = useState(false)
  const [confirming, setConfirming] = useState(false)
  const [cost, setCost] = useState<RemovalAnswer | undefined>()
  /**
   * Why the dry run could not answer. **A slot of its own**, because the
   * dialog reads it: "working out what this would remove…" is drawn while
   * there is neither a cost nor a reason, and `begin()` clears the shared slot
   * on *every* write — so ticking a capability box after a failed dry run put
   * the dialog back to "working out" for ever, with nothing in flight. Third
   * review pass; `Users.tsx` and `Backup.tsx` each reached the same two-slot
   * answer for their own pair.
   */
  const [costFailure, setCostFailure] = useState<string | undefined>()
  const [removing, setRemoving] = useState(false)
  /**
   * Whether this account is gone and the list has not caught up.
   *
   * **A terminal state rather than a closed dialog.** Closing the dialog put
   * the plain *Remove…* back over a deleted account, and left the role
   * dropdown, all three capability boxes, the grants, the password form and
   * every Revoke live against an id that answers "no account with id 2". The
   * card normally unmounts the moment the reload lands; this is what it draws
   * when that read fails instead. Third review pass.
   */
  const [removed, setRemoved] = useState(false)
  /**
   * Whether the real `DELETE` is already on its way.
   *
   * **A ref, for the reason `RemoveRoot`'s and `RootRow`'s are**: React
   * batches state, so `disabled={removing}` alone lets two presses dispatched
   * before the next render both send.
   */
  const inFlight = useRef(false)
  /** The `user` prop this card last drew — see `applied`. */
  const drawn = useRef(user)
  /** The newest dry run — see `askCost`. */
  const latestCost = useRef(0)

  // Adjusted during render rather than in an effect: a new prop *is* the newer
  // fact, so there is no frame in which both are held.
  if (drawn.current !== user) {
    drawn.current = user
    if (applied !== undefined) setApplied(undefined)
    // **The tombstone goes with it.** A prop arriving for this card means an
    // account of this name exists, and `Users` keys cards by name — so a
    // removal whose reload failed, followed by re-creating the same name,
    // handed the new account the old card's instance and drew it as removed,
    // with no controls, until a page reload. That is the recycled-id hazard
    // `Users.tsx`'s own doc comment describes, reached through the name
    // instead. Fourth review pass.
    if (removed) setRemoved(false)
  }

  /** The account as this card knows it: the last answer, else the list's row. */
  const shown = applied ?? user

  const isSelf = actingAs === user.id
  const lastAdmin = onlyAdminWhoCanLogIn(users, shown)

  /**
   * Claim the card for one write, or refuse because it is already claimed.
   *
   * **A ref as well as `saving`, for the reason every other guarded press in
   * this codebase carries one**: React batches state, so two presses
   * dispatched before the next render both read `saving === false`.
   */
  const begin = () => {
    if (inFlight.current) return false
    inFlight.current = true
    setSaving(true)
    setFailure(undefined)
    // **Cleared here rather than left to fade.** "Password set" is a notice
    // about one write, and nothing else on this card was clearing it — so it
    // stood over every later toggle, grant and refusal, still saying somebody
    // had just been signed out everywhere. O96's `heldNotice` finding, which
    // `RuleList` closed the same way: the mutation that follows a notice is
    // what retires it.
    setWasReset(false)
    return true
  }

  /**
   * Release the card once the list this write triggered has come back.
   *
   * **Not when the write answers — when the *reload* does.** `user` is a prop,
   * so between a 200 and the list landing this card is still drawing what it
   * held before the write, with its controls live again. That window is a
   * defect rather than a cosmetic lag on the one control that sends a *whole
   * list*: ticking a second root inside it computes the new grants from the
   * pre-patch `user.roots` and so silently revokes the first. Holding until
   * `onChanged` settles keeps this card's props and its controls in step, and
   * costs the reload's few hundred milliseconds on a screen touched rarely.
   */
  const settled = (after?: () => void) => {
    after?.()
    Promise.resolve(onChanged()).finally(() => {
      inFlight.current = false
      setSaving(false)
      setRemoving(false)
    })
  }

  const failed = (err: unknown) => {
    inFlight.current = false
    setSaving(false)
    setRemoving(false)
    setFailure(messageOf(err))
  }

  const save = (patch: UserPatch, after?: () => void) => {
    if (!begin()) return
    patchUser(user.id, patch)
      .then((answer) => {
        setApplied(answer)
        settled(after)
      })
      .catch(failed)
  }

  const granted = shown.roots.map((g) => g.id)

  const toggleGrant = (id: number, on: boolean) =>
    save({
      roots: on ? [...granted, id] : granted.filter((held) => held !== id),
    })

  const submitPassword = (e: React.FormEvent) => {
    e.preventDefault()
    if (password === '') return
    save({ password }, () => {
      setPassword('')
      setWasReset(true)
    })
  }

  /**
   * Revoking a session or a key, on the same guarded path as a patch.
   *
   * **Two clicks used to draw a 404 over a revocation that had worked.** The
   * first `DELETE` answers 204 and the second finds no row — and
   * `revoke_session` answers 404 for a session that is not there, the same
   * answer as for one that never existed — so the card reported a failure for
   * the thing it had just done.
   */
  const revoke = (send: () => Promise<void>, without: (held: User) => User) => {
    if (!begin()) return
    send()
      // **The row goes locally too, for `save`'s reason one path over.**
      // `settled` waits for the list read, and that read can fail or be
      // superseded without bringing fresh props — leaving the revoked session
      // listed with a live Revoke, whose next press answers 404 for a session
      // that is already gone. Third review pass, in the fix the second one
      // produced for exactly this shape.
      .then(() => {
        // **Applied to whatever the card holds *now*, not to the snapshot the
        // click was made from.** A list read landing during the `DELETE`
        // clears `applied` and brings a newer row; writing back a click-time
        // copy would drop whatever that read brought — a session opened since,
        // or a role another administrator changed. Fourth review pass.
        setApplied((held) => without(held ?? drawn.current))
        settled()
      })
      .catch(failed)
  }

  const askCost = () => {
    // **The newest dry run wins.** Two are routinely in flight — Remove…,
    // Cancel, Remove… again — and nothing makes them answer in order, so a
    // slow first answer landing after a failed second drew five counts and an
    // enabled *Remove sam* underneath an alert saying the count could not be
    // taken. `Users`, `Backup` and `Roots` all carry the same counter. Fourth
    // review pass.
    const seq = ++latestCost.current
    setConfirming(true)
    setCost(undefined)
    setCostFailure(undefined)
    removalCost(user.id)
      .then((answer) => {
        if (seq !== latestCost.current) return
        setCost(answer)
      })
      .catch((err: unknown) => {
        if (seq !== latestCost.current) return
        setCostFailure(messageOf(err))
      })
  }

  const confirmRemoval = () => {
    if (!begin()) return
    setRemoving(true)
    removeUser(user.id)
      // **The dialog closes itself rather than trusting the unmount.** The
      // card goes away when the reload drops the account — unless that read
      // fails, and then a live *Remove sam* button and five counts stood over
      // an account that no longer existed, whose next press answers "no
      // account with id 2". That is the double-click 404 this branch fixed for
      // the revocations, one route along. Second review pass.
      .then(() =>
        settled(() => {
          setRemoved(true)
          setConfirming(false)
          setCost(undefined)
          setCostFailure(undefined)
        }),
      )
      .catch(failed)
  }

  const roleWhy = isSelf
    ? 'An administrator cannot take their own admin role away; another administrator has to do it.'
    : lastAdmin
      ? `${shown.name} is the only administrator who can log in, so neither the role nor Remove can change that. Give another account the admin role first.`
      : undefined

  if (removed) {
    return (
      <li className="user-row removed">
        <h3>{shown.name}</h3>
        <p className="gone">
          Removed. The list has not caught up — this card is what is left of it,
          and nothing here can be edited.
        </p>
      </li>
    )
  }

  return (
    <li className="user-row">
      <h3>
        {shown.name}
        {isSelf && <span className="you"> — this is you</span>}
      </h3>
      {failure !== undefined && <p role="alert">{failure}</p>}

      {!shown.can_log_in && (
        <p className="locked">
          Nobody can sign in as this account: it holds no usable password.
          Setting one below gives it a way in.
        </p>
      )}

      <div className="fields">
        <label>
          <span>Role</span>
          <select
            name="role"
            value={shown.role}
            disabled={saving || isSelf || lastAdmin}
            onChange={(e) => save({ role: e.target.value })}
          >
            {ROLES.map((role) => (
              <option key={role} value={role}>
                {role}
              </option>
            ))}
          </select>
        </label>
        {roleWhy !== undefined && <p className="why">{roleWhy}</p>}

        <label>
          <input
            type="checkbox"
            name="can_download"
            checked={shown.can_download}
            disabled={saving}
            onChange={(e) => save({ can_download: e.target.checked })}
          />
          <span>May download whole editions</span>
        </label>
        <label>
          <input
            type="checkbox"
            name="can_stream"
            checked={shown.can_stream}
            disabled={saving}
            onChange={(e) => save({ can_stream: e.target.checked })}
          />
          <span>May stream pages</span>
        </label>
        <label>
          <input
            type="checkbox"
            name="can_stamp"
            checked={shown.can_stamp}
            disabled={saving}
            onChange={(e) => save({ can_stamp: e.target.checked })}
          />
          <span>May write ComicInfo.xml into archives</span>
        </label>
      </div>

      <fieldset className="grants">
        <legend>Roots this account may see</legend>
        {roots === undefined ? (
          <p className="unknown">
            The roots list has not been read, so there is nothing to offer here.{' '}
            {shown.roots.length === 0
              ? 'This account is granted none, which is unrestricted.'
              : `This account is granted ${shown.roots
                  .map((g) => g.path)
                  .join(', ')}.`}
          </p>
        ) : roots.length === 0 ? (
          <p className="empty">This library has no roots yet.</p>
        ) : (
          <ul className="grant-list">
            {roots.map((root) => (
              // Keyed by path rather than id: SQLite reissues a freed
              // `root.id`, and `Roots.tsx` carries the same note for the same
              // reason. Two roots may not overlap, so a path cannot collide.
              <li key={root.path}>
                <label>
                  <input
                    type="checkbox"
                    name={`root-${root.id}`}
                    checked={granted.includes(root.id)}
                    disabled={saving}
                    onChange={(e) => toggleGrant(root.id, e.target.checked)}
                  />
                  <span>{root.path}</span>
                </label>
              </li>
            ))}
          </ul>
        )}
        {roots !== undefined && (
          <p className="unrestricted">
            Nothing ticked means unrestricted — every root, including any added
            later. It does not mean no roots at all, which is why an account
            with an empty list sees the whole library.
          </p>
        )}
      </fieldset>

      <form className="reset" onSubmit={submitPassword}>
        <label>
          <span>Set a new password</span>
          <input
            type="password"
            name="password"
            value={password}
            disabled={isSelf || saving}
            onChange={(e) => setPassword(e.target.value)}
          />
        </label>
        <button
          type="submit"
          name="reset"
          disabled={isSelf || saving || password === ''}
        >
          Set password
        </button>
        {isSelf ? (
          <p className="why">
            You cannot reset your own password here: this ends every session the
            account has, and yours is the session making the change. The server
            refuses it and names <code>/api/password</code>, which keeps the
            session making the change. That is the account panel — open any work
            and press A. It has no URL of its own yet (O94).
          </p>
        ) : (
          <p className="why">
            This ends every session {shown.name} has open. Their API keys are
            untouched — independent credentials with their own revocation.
          </p>
        )}
        {wasReset && (
          <p className="done">
            Password set. {shown.name} is signed out everywhere.
          </p>
        )}
      </form>

      <details className="credentials">
        <summary>
          {count(shown.sessions.length, 'session', 'sessions')} ·{' '}
          {count(shown.keys.length, 'API key', 'API keys')}
        </summary>
        {shown.sessions.length === 0 ? (
          <p className="empty">No sessions open.</p>
        ) : (
          <ul className="session-list">
            {shown.sessions.map((session) => (
              <li key={session.id}>
                <span className="what">
                  {session.user_agent ?? 'no user-agent recorded'} ·{' '}
                  {session.ip ?? 'no address recorded'}
                </span>
                <span className="when">
                  started {session.created} · last seen {session.last_seen}
                </span>
                {session.current && (
                  <span className="current">this browser</span>
                )}
                <button
                  type="button"
                  name={`revoke-session-${session.id}`}
                  disabled={saving}
                  onClick={() =>
                    revoke(
                      () => revokeUserSession(user.id, session.id),
                      (held) => ({
                        ...held,
                        sessions: held.sessions.filter(
                          (s) => s.id !== session.id,
                        ),
                      }),
                    )
                  }
                >
                  Revoke
                </button>
              </li>
            ))}
          </ul>
        )}
        {shown.keys.length > 0 && (
          <ul className="key-list">
            {shown.keys.map((key) => (
              <li key={key.id}>
                <span className="what">{key.name}</span>
                <span className="when">
                  made {key.created} ·{' '}
                  {key.last_used === null
                    ? 'never used'
                    : `last used ${key.last_used}`}
                </span>
                <button
                  type="button"
                  name={`revoke-key-${key.id}`}
                  disabled={saving}
                  onClick={() =>
                    revoke(
                      () => revokeUserKey(user.id, key.id),
                      (held) => ({
                        ...held,
                        keys: held.keys.filter((k) => k.id !== key.id),
                      }),
                    )
                  }
                >
                  Revoke
                </button>
              </li>
            ))}
          </ul>
        )}
      </details>

      <div className="actions">
        {!confirming && (
          <button
            type="button"
            name="remove"
            disabled={lastAdmin || saving}
            onClick={askCost}
          >
            Remove…
          </button>
        )}
        {confirming && (
          <div
            className="remove-user"
            role="group"
            aria-label={`Remove the account ${shown.name}`}
          >
            {costFailure !== undefined && <p role="alert">{costFailure}</p>}
            {cost === undefined && costFailure === undefined && (
              <p>Working out what this would remove…</p>
            )}
            {cost !== undefined && (
              <>
                <p className="counts">
                  Removing {cost.removed.name} takes{' '}
                  {count(
                    cost.removed.positions,
                    'in-progress position',
                    'in-progress positions',
                  )}
                  , {count(cost.removed.notes, 'note', 'notes')},{' '}
                  {count(
                    cost.removed.sessions,
                    'open session',
                    'open sessions',
                  )}
                  , {count(cost.removed.keys, 'API key', 'API keys')} and{' '}
                  {count(cost.removed.grants, 'root grant', 'root grants')}.
                </p>
                <p className="irreversible">
                  No work, edition or member goes with it — the library itself
                  is untouched. What goes is everything this person recorded
                  against it.
                  {isSelf &&
                    ' This is the account you are signed in as; removing it signs you out.'}
                </p>
                <button
                  type="button"
                  name="confirm-remove"
                  onClick={confirmRemoval}
                  disabled={removing || saving}
                >
                  {removing ? 'Removing…' : `Remove ${shown.name}`}
                </button>
              </>
            )}
            <button
              type="button"
              name="cancel-remove"
              onClick={() => {
                setConfirming(false)
                setCost(undefined)
                setCostFailure(undefined)
              }}
              disabled={removing}
            >
              Cancel
            </button>
          </div>
        )}
      </div>
    </li>
  )
}
