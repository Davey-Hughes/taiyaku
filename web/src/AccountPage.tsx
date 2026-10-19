/**
 * One account's page: what it is, what it may do, what is signed in as it,
 * and what removing it would cost — spec C4 §4 and the build design's §6.2.
 *
 * **The page owns its own writes, the list owns who exists.** `RootRow`'s
 * split and its stated reason: a patch, a revocation and a dry run all belong
 * to the account being administered, and the list reloads on `onChanged`.
 *
 * **The server is the authority; this disables and explains.** Every refusal
 * here is enforced in `taiyaku-library` and answered as a 409 with a sentence,
 * which this page renders when it arrives. Where it can tell in advance it
 * *also* disables the control and says why — and the duplication is
 * one-directional: this may refuse what the server would refuse, and may never
 * permit what the server would not. That is why `onlyAdminWhoCanLogIn` mirrors
 * `another_admin_can_log_in` rather than counting administrators.
 *
 * **One reason covers two controls.** The last-administrator rule reaches the
 * role dropdown and Remove through different routes and is one invariant; a
 * page that disabled only Remove would leave the identical accident one
 * dropdown away.
 *
 * **The self-password refusal links to the screen that gets it right.**
 * `/api/password` is the route that keeps the session making the change, and
 * the screen that calls it is `Account.tsx`, at `/account`. A link, because
 * any account can follow one, where directions through a work would fail an
 * administrator of a library with no readable work (O94).
 *
 * **Grants are libraries, sent whole, and "every library" is a fact of its
 * own.** `user.every_library` alone says whether the account sees every
 * library; `libraries` is its limit, and an empty limit sees nothing. A patch
 * sends `{every_library: true}` to lift the limit or the whole list to set
 * it, `[]` included. The page draws that as a choice between "Every library"
 * and "Only these". Each tick and each cleared box saves at once, the last
 * one too, and so does choosing "Only these", which saves as no library.
 * Lifting the limit to every library, "Every library" on an account with a
 * limit, asks first (O195), since an arrow key chooses on the radios as a
 * click does.
 *
 * **The confirm does not exist until the counts do**, and the real `DELETE` is
 * sent from here and nowhere else. `RemoveRoot`'s reasoning, on the route
 * `users_api` calls the second-most destructive in the product.
 */
import { type ReactNode, useEffect, useId, useRef, useState } from 'react'
import { Link } from 'react-router'
import { useDrawn } from './hooks'
import type { Library } from './libraries'
import { sentenceOf } from './message'
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

export interface AccountPageProps {
  user: User
  /**
   * Every account, so the last-administrator rule is answered from the list
   * the screen already holds rather than by a request of its own.
   */
  users: User[]
  /**
   * Every library, for the grant editor — `undefined` where that list has
   * not been read.
   *
   * **Three states rather than two.** Reading an unread list as `[]` would
   * draw "there are no libraries yet" while the read was still in flight and
   * for ever after one that failed: a positive false claim, on the control
   * that decides what an account sees. What the page can always say is what
   * the account sees, because `user.every_library` says so and
   * `user.libraries` carries the names; what it cannot do without the list is
   * offer a library nobody granted yet.
   */
  libraries?: Library[]
  /**
   * The account this browser is signed in as, or `undefined` where no gate
   * sits above — which `gate.ts` defines as "no account known", never "an
   * admin". Both readings are safe here: the server refuses either way.
   */
  actingAs?: number
  /**
   * Re-read the screen's list of accounts.
   *
   * **Its answer is awaited when it gives one.** `Users.reload` returns the
   * `listUsers` promise, and this page holds its controls until that settles
   * — see `settled`. A caller that returns nothing is not wrong, only
   * unheld.
   *
   * Typed `() => unknown` rather than as a union with `void`, which biome
   * refuses: what matters here is that a returned promise is awaited, not
   * what a caller that returns nothing is called.
   */
  onChanged: () => unknown
  /**
   * The account is gone, and nothing is left here to edit. The screen drops
   * it, and goes back to its table while the person is still on the page
   * that asked.
   */
  onRemoved?: (name: string) => void
  /**
   * A write sent from this page was refused after the page had gone, with
   * the sentence that says so and names the account.
   */
  onLateRefusal?: (sentence: string) => void
  /**
   * The person was sent here by an action whose button went with the page
   * it was on, so the heading takes focus.
   */
  arrived?: boolean
  /**
   * What the screen says above the page: a list that could not be read, a
   * removal's line, or an answer that came once the page it was asked from
   * had gone.
   */
  notices?: ReactNode
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

export function AccountPage({
  user,
  users,
  libraries,
  actingAs,
  onChanged,
  onRemoved,
  onLateRefusal,
  arrived = false,
  notices,
}: AccountPageProps) {
  const heading = useRef<HTMLHeadingElement>(null)
  useEffect(() => {
    if (arrived) heading.current?.focus()
  }, [arrived])
  const [failure, setFailure] = useState<string | undefined>()
  /**
   * What the last write on this page was answered with.
   *
   * **The hold alone was not enough, and this is the half that closes it.**
   * `settled` waits for the list read a write triggers, but that read can fail
   * or be superseded by a later one — and either way it resolves without
   * bringing fresh props, so the controls came back live over pre-write
   * values. `patchUser` answers the account whole, so the page can keep that
   * answer and stop guessing: it is what is drawn, and what `toggleGrant`
   * computes the next grant list from. Retired the moment a new `user` prop
   * arrives, because the list is the truth as soon as it catches up.
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
   * the dialog back to "working out" for ever, with nothing in flight.
   * `Backup.tsx` keeps the same two slots for its own pair, and the users
   * screen keeps the list's failure in `Users.tsx` apart from the add's
   * refusal in `NewAccount.tsx`.
   */
  const [costFailure, setCostFailure] = useState<string | undefined>()
  const [removing, setRemoving] = useState(false)
  /**
   * Whether this account is gone and the list has not caught up.
   *
   * **A terminal state rather than a closed dialog.** A closed dialog alone
   * would put the plain *Remove…* back over a deleted account, and leave the
   * role dropdown, all three capability boxes, the grants, the password form
   * and every Revoke live against an id that answers "no account with id 2".
   * It is the page's own guard, for a caller that leaves the page drawn once
   * its account is gone. No production caller does: `Users` passes
   * `onRemoved`, and takes the page away in the removal's own answer.
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
  /** False once the page has gone, and a refusal has no page here to say it. */
  const onScreen = useDrawn()
  /** The `user` prop this page last drew — see `applied`. */
  const drawn = useRef(user)
  /** The newest dry run — see `askCost`. */
  const latestCost = useRef(0)

  // Adjusted during render rather than in an effect: a new prop *is* the newer
  // fact, so there is no frame in which both are held.
  if (drawn.current !== user) {
    drawn.current = user
    if (applied !== undefined) setApplied(undefined)
    // **The tombstone goes with it.** A prop arriving for this page means an
    // account of this name exists, and `Users` keys the page by name — so
    // where the page outlives a removal, an account re-created under that
    // name is handed this instance, and must not be drawn as removed, with no
    // controls. That is the recycled-id hazard `Users.tsx`'s own doc comment
    // describes, reached through the name instead.
    if (removed) setRemoved(false)
  }

  /** The account as this page knows it: the last answer, else the list's row. */
  const shown = applied ?? user

  const isSelf = actingAs === user.id
  const lastAdmin = onlyAdminWhoCanLogIn(users, shown)

  /**
   * Claim the page for one write, or refuse because it is already claimed.
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
    // about one write, and nothing else on this page was clearing it — so it
    // stood over every later toggle, grant and refusal, still saying somebody
    // had just been signed out everywhere. O96's `heldNotice` finding, which
    // `RuleList` closed the same way: the mutation that follows a notice is
    // what retires it.
    setWasReset(false)
    return true
  }

  /**
   * Release the page once the list this write triggered has come back.
   *
   * **Not when the write answers — when the *reload* does.** `user` is a prop,
   * so between a 200 and the list landing this page is still drawing what it
   * held before the write, with its controls live again. That window is a
   * defect rather than a cosmetic lag on the one control that sends a *whole
   * list*: ticking a second library inside it computes the new grants from
   * the pre-patch `user.libraries` and so silently revokes the first. Holding
   * until `onChanged` settles keeps this page's props and its controls in
   * step, and costs the reload's few hundred milliseconds on a screen touched
   * rarely.
   */
  const settled = (after?: () => void) => {
    after?.()
    Promise.resolve(onChanged()).finally(() => {
      inFlight.current = false
      setSaving(false)
      setRemoving(false)
    })
  }

  /**
   * A write was refused. `late` says what was not done, for a refusal that
   * comes once this page has gone: the screen says it on the page the person
   * is on, where nothing else names the account.
   */
  const failed = (late: string) => (err: unknown) => {
    inFlight.current = false
    setSaving(false)
    setRemoving(false)
    const sentence = sentenceOf(err)
    if (onScreen.current) setFailure(sentence)
    else onLateRefusal?.(`${late} (${sentence}).`)
  }

  const save = (patch: UserPatch, late: string, after?: () => void) => {
    if (!begin()) return
    patchUser(user.id, patch)
      .then((answer) => {
        setApplied(answer)
        settled(after)
      })
      .catch(failed(late))
  }

  /** What a refused setting says once its page has gone. */
  const notSaved = (what: string) => `${what} for ${shown.name} was not saved`
  const grantsNotSaved = `Libraries for ${shown.name} were not saved`

  const granted = shown.libraries.map((g) => g.id)
  /**
   * Whether the account has a limit, read from `every_library` alone. The
   * list's length cannot say it: an empty list is a limit to no library.
   */
  const restricted = !shown.every_library
  /**
   * "Every library" was chosen on an account with a limit, and the page is
   * asking before it lifts it.
   *
   * **Asked, not saved at once (O195).** An arrow key on the radios chooses
   * as a click does, so one key pressed on "Only these" would lift the limit
   * with nothing asked — on an account limited to no library as much as on
   * one with ticks.
   */
  const [lifting, setLifting] = useState(false)
  // Retired during render, as `applied` is: a list read that brings the account
  // lifted elsewhere must not leave the question waiting for its next limit.
  if (lifting && !restricted) setLifting(false)
  const everyRadio = useRef<HTMLInputElement>(null)
  const onlyRadio = useRef<HTMLInputElement>(null)
  const keepButton = useRef<HTMLButtonElement>(null)
  const liftId = useId()
  // The arrow key that opened the question left focus on the radio it moved
  // to, which still reads as unchosen; the answer that changes nothing is the
  // safe place to land, and the group's description is read as focus enters.
  useEffect(() => {
    if (lifting) keepButton.current?.focus()
  }, [lifting])

  /**
   * Grants whose library the list does not hold, as when the list was read
   * before the library was made. No box is drawn for them, so they are named:
   * an account holding only these would otherwise look as if it saw nothing.
   */
  const unlisted =
    libraries === undefined
      ? []
      : shown.libraries.filter((g) => !libraries.some((l) => l.id === g.id))

  /**
   * Said whenever the list has been read, since a grant the list does not
   * hold is drawn nowhere else. Where they are all there is, it does not say "also".
   */
  const called = unlisted.map((g) => g.name)
  const names =
    called.length < 2
      ? called.join('')
      : `${called.slice(0, -1).join(', ')} and ${called[called.length - 1]}`
  /**
   * Whether the account is left seeing no library: limited, holding no grant
   * at all, and not an administrator, who sees every library whatever its
   * rows say. `granted` rather than the boxes ticked, because a grant the
   * list does not hold is drawn by no box and is still a library it sees.
   * One condition for every branch of the fieldset, so none can say it of
   * an account another branch would not.
   */
  const seesNothing =
    restricted && granted.length === 0 && shown.role !== 'admin'
  const noLibraryLine = seesNothing ? (
    <p className="why" data-field="no-library">
      This account sees no library.
    </p>
  ) : null

  const unlistedLine =
    unlisted.length === 0 ? null : (
      <p className="why" data-field="unlisted-grants">
        {unlisted.length === shown.libraries.length
          ? `This account is granted ${names}, which the list does not hold.`
          : `Also granted, though the list does not hold ${
              unlisted.length === 1 ? 'it' : 'them'
            }: ${names}.`}
      </p>
    )

  // The last tick clears like any other: an empty list limits the account to
  // no library, which narrows it and widens nothing.
  const toggleGrant = (id: number, on: boolean) => {
    const next = on ? [...granted, id] : granted.filter((held) => held !== id)
    save({ libraries: next }, grantsNotSaved)
  }

  /**
   * **Neither radio acts while a write is in flight.** "Only these" saves, so
   * `begin` refuses it through `inFlight`. "Every library" only opens the
   * question, which changes local state and so would redraw the page mid-save
   * — it reads the ref itself.
   */
  const seeEvery = () => {
    if (inFlight.current) return
    setLifting(true)
  }

  /** Saved at once as no library, for the reason the last tick clears. */
  const seeOnly = () => save({ libraries: [] }, grantsNotSaved)

  // `every_library` rather than an empty list, which would limit the account
  // to no library instead.
  const lift = () => {
    save({ every_library: true }, grantsNotSaved, () =>
      everyRadio.current?.focus(),
    )
  }

  /** The question goes, and focus goes back to the choice that stands. */
  const keepLimit = () => {
    setLifting(false)
    onlyRadio.current?.focus()
  }

  const submitPassword = (e: React.FormEvent) => {
    e.preventDefault()
    if (password === '') return
    save({ password }, `A new password for ${shown.name} was not set`, () => {
      setPassword('')
      setWasReset(true)
    })
  }

  /**
   * Revoking a session or a key, on the same guarded path as a patch.
   *
   * **Two clicks must not draw a 404 over a revocation that worked.** The
   * first `DELETE` answers 204 and the second finds no row — and
   * `revoke_session` answers 404 for a session that is not there, the same
   * answer as for one that never existed — so an unguarded second press
   * would report a failure for the thing the first had just done.
   */
  const revoke = (
    send: () => Promise<void>,
    without: (held: User) => User,
    late: string,
  ) => {
    if (!begin()) return
    send()
      // **The row goes locally too, for `save`'s reason one path over.**
      // `settled` waits for the list read, and that read can fail or be
      // superseded without bringing fresh props — leaving the revoked session
      // listed with a live Revoke, whose next press answers 404 for a session
      // that is already gone. Third review pass, in the fix the second one
      // produced for exactly this shape.
      .then(() => {
        // **Applied to whatever the page holds *now*, not to the snapshot the
        // click was made from.** A list read landing during the `DELETE`
        // clears `applied` and brings a newer row; writing back a click-time
        // copy would drop whatever that read brought — a session opened since,
        // or a role another administrator changed.
        setApplied((held) => without(held ?? drawn.current))
        settled()
      })
      .catch(failed(late))
  }

  const askCost = () => {
    // **The newest dry run wins.** Two are routinely in flight — Remove…,
    // Cancel, Remove… again — and nothing makes them answer in order, so a
    // slow first answer landing after a failed second drew five counts and an
    // enabled *Remove sam* underneath an alert saying the count could not be
    // taken. `Users`, `Backup` and `Libraries` all carry the same counter. Fourth
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
        setCostFailure(sentenceOf(err))
      })
  }

  const confirmRemoval = () => {
    if (!begin()) return
    setRemoving(true)
    removeUser(user.id)
      // **The dialog closes itself rather than trusting the page to go.**
      // While `removed` holds, the tombstone draws no dialog. But where the
      // page outlives the removal (no `onRemoved` takes it away), a prop of
      // the same name drops the tombstone — an account re-created under the
      // removed one's name — and a dialog left open would come back over
      // that account, "working out what this would remove…" with nothing in
      // flight and no *Remove…* button.
      .then(() => {
        settled(() => {
          setRemoved(true)
          setConfirming(false)
          setCost(undefined)
          setCostFailure(undefined)
        })
        onRemoved?.(shown.name)
      })
      .catch(failed(`${shown.name} was not removed`))
  }

  const roleWhy = isSelf
    ? 'An administrator cannot take their own admin role away; another administrator has to do it.'
    : lastAdmin
      ? `${shown.name} is the only administrator who can log in, so neither the role nor Remove can change that. Give another account the admin role first.`
      : undefined

  const back = (
    <p className="back">
      <Link to="/config/users">‹ Users</Link>
    </p>
  )

  if (removed) {
    return (
      <div className="user-row removed">
        {back}
        <h2>{shown.name}</h2>
        <p className="gone">
          Removed. The list has not caught up — this page is what is left of it,
          and nothing here can be edited.
        </p>
      </div>
    )
  }

  return (
    <div className="user-row">
      {back}
      <h2 ref={heading} tabIndex={-1}>
        {shown.name}
        {isSelf && <span className="you"> — this is you</span>}
      </h2>
      {notices}
      {failure !== undefined && <p role="alert">{failure}</p>}

      {!shown.can_log_in && (
        <p className="locked">
          Nobody can sign in as this account: it holds no usable password.
          Setting one below gives it a way in.
        </p>
      )}

      <section className="account-section">
        <h3>Role &amp; permissions</h3>
        <div className="fields">
          {/* O175: the `aria-label` repeats the label's words because `.field`
            draws them in capitals, and Chromium's accessibility tree named
            the control in those capitals. The password box below carries one
            for the same reason. */}
          <label className="field">
            <span>Role</span>
            <select
              name="role"
              aria-label="Role"
              value={shown.role}
              disabled={saving || isSelf || lastAdmin}
              onChange={(e) =>
                save(
                  { role: e.target.value },
                  `The role for ${shown.name} was not changed to ${e.target.value}`,
                )
              }
            >
              {ROLES.map((role) => (
                <option key={role} value={role}>
                  {role}
                </option>
              ))}
            </select>
          </label>
        </div>
        {roleWhy !== undefined && <p className="why">{roleWhy}</p>}

        {/* **`aria-disabled`, not `disabled`, on every checkbox on this page —
          here and in the grant list below. O159.** `disabled` draws a
          checked box in the UA's grey and takes its `accent-color` fill
          with it, so one tick would flash every box on the page.
          `accent-color`, `opacity` and `filter` on `:disabled` do not bring
          the fill back in Chromium; only redrawing the box with
          `appearance: none` can, which is a redesign of every checkbox.
          The lock never lived in the attribute: `begin` refuses a second
          write through `inFlight`, and each box is controlled, so React
          puts a refused tick back. The role select, the password field
          and the buttons keep `disabled`.
          With `aria-disabled`, a tick refused mid-save does nothing and
          shows no cue: the box keeps its real value. `aria-disabled` also
          leaves keyboard focus on the box, where in Chromium a control
          that becomes `disabled` loses it (`docs/notes.md`, "Admin
          follow-ups"). */}
        {/* O154: one named group, as the grants below are one named group.
          A `role` and a name rather than a `<fieldset>`: nothing on screen
          changes, and a fieldset's box model has cost these screens width
          before (`admin.css`, above `.rule-form .rule-sets`). */}
        <div className="checks" role="group" aria-label="Permissions">
          <label>
            <input
              type="checkbox"
              name="can_download"
              checked={shown.can_download}
              aria-disabled={saving}
              onChange={(e) =>
                save(
                  { can_download: e.target.checked },
                  notSaved('May download whole editions'),
                )
              }
            />
            <span>May download whole editions</span>
          </label>
          <label>
            <input
              type="checkbox"
              name="can_stream"
              checked={shown.can_stream}
              aria-disabled={saving}
              onChange={(e) =>
                save(
                  { can_stream: e.target.checked },
                  notSaved('May stream pages'),
                )
              }
            />
            <span>May stream pages</span>
          </label>
          <label>
            <input
              type="checkbox"
              name="can_stamp"
              checked={shown.can_stamp}
              aria-disabled={saving}
              onChange={(e) =>
                save(
                  { can_stamp: e.target.checked },
                  notSaved('May write ComicInfo.xml into archives'),
                )
              }
            />
            <span>May write ComicInfo.xml into archives</span>
          </label>
        </div>
      </section>

      <section className="account-section">
        <h3>Libraries</h3>
        <fieldset className="grants">
          <legend className="visually-hidden">
            Libraries this account may see
          </legend>
          {libraries === undefined ? (
            <p className="unknown">
              The libraries list has not been read, so there is nothing to offer
              here.
              {shown.every_library
                ? ' This account sees every library.'
                : shown.libraries.length > 0
                  ? ` This account is granted ${shown.libraries
                      .map((g) => g.name)
                      .join(', ')}.`
                  : seesNothing && ' This account sees no library.'}
            </p>
          ) : libraries.length === 0 ? (
            <>
              <p className="empty">There are no libraries yet.</p>
              {unlistedLine}
              {noLibraryLine}
            </>
          ) : (
            <>
              <label>
                <input
                  type="radio"
                  name={`sees-${user.id}`}
                  ref={everyRadio}
                  value="every"
                  checked={!restricted}
                  aria-disabled={saving}
                  onChange={seeEvery}
                />
                <span>Every library, including any added later</span>
              </label>
              <label>
                <input
                  type="radio"
                  name={`sees-${user.id}`}
                  ref={onlyRadio}
                  value="only"
                  checked={restricted}
                  aria-disabled={saving}
                  onChange={seeOnly}
                  // A radio already chosen fires no change, and "Only these"
                  // stands while the question is up, so choosing it again by
                  // click or by an arrow key is heard here.
                  onClick={() => {
                    if (inFlight.current) return
                    setLifting(false)
                  }}
                />
                <span>Only these</span>
              </label>
              {lifting && (
                <div
                  className="lift-limit"
                  role="group"
                  aria-label="Lift the limit"
                  aria-describedby={liftId}
                >
                  {/* No ticks are named where there are none to clear. */}
                  <p id={liftId}>
                    {shown.name} would see every library, including any added
                    later
                    {granted.length === 0
                      ? ''
                      : ', and the ticks below would be cleared'}
                    .
                  </p>
                  <button
                    type="button"
                    name="lift-limit"
                    disabled={saving}
                    onClick={lift}
                  >
                    See every library
                  </button>
                  <button
                    type="button"
                    name="keep-limit"
                    ref={keepButton}
                    disabled={saving}
                    onClick={keepLimit}
                  >
                    Keep the limit
                  </button>
                </div>
              )}
              {restricted && (
                <ul className="grant-list">
                  {libraries.map((library) => (
                    <li key={library.id}>
                      <label>
                        <input
                          type="checkbox"
                          name={`library-${library.id}`}
                          checked={granted.includes(library.id)}
                          aria-disabled={saving}
                          onChange={(e) =>
                            toggleGrant(library.id, e.target.checked)
                          }
                        />
                        <span>{library.name}</span>
                      </label>
                      <ul className="grant-folders">
                        {library.folders.map((folder) => (
                          <li key={folder.path}>
                            <code>{folder.path}</code>
                          </li>
                        ))}
                      </ul>
                    </li>
                  ))}
                </ul>
              )}
              {unlistedLine}
              {noLibraryLine}
              {shown.role === 'admin' && (
                <p className="why">
                  An administrator sees every library whatever is chosen here.
                  The choice takes effect if the role becomes user.
                </p>
              )}
            </>
          )}
        </fieldset>
      </section>

      <section className="account-section">
        <h3>Password</h3>
        <form className="reset" onSubmit={submitPassword}>
          <label className="field">
            <span>Set a new password</span>
            <input
              type="password"
              name="password"
              aria-label="Set a new password"
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
              You cannot reset your own password here: this ends every session
              the account has, and yours is the session making the change. The
              server refuses it and names <code>/api/password</code>, which
              keeps the session making the change. Change your own on{' '}
              <Link to="/account">your account screen</Link>.
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
      </section>

      <section className="account-section credentials">
        <h3>Sessions &amp; keys</h3>
        <p className="tally">
          {count(shown.sessions.length, 'session', 'sessions')} ·{' '}
          {count(shown.keys.length, 'API key', 'API keys')}
        </p>
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
                      `A session for ${shown.name} was not revoked`,
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
                      `The API key ${key.name} for ${shown.name} was not revoked`,
                    )
                  }
                >
                  Revoke
                </button>
              </li>
            ))}
          </ul>
        )}
      </section>

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
                  {count(
                    cost.removed.grants,
                    'library grant',
                    'library grants',
                  )}
                  .
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
                  className="confirm"
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
    </div>
  )
}
