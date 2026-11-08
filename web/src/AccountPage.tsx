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
import {
  Fragment,
  type ReactNode,
  useCallback,
  useEffect,
  useId,
  useRef,
  useState,
} from 'react'
import { Link, useLocation } from 'react-router'
import { accountPath, nameAt, USERS } from './configAddress'
import { useConfirmPassword } from './confirmPassword'
import { HeldButton } from './HeldButton'
import { useDrawn, useDrawnWait, useFocusLeavesHeld } from './hooks'
import type { Library } from './libraries'
import { reasonOf, refusalOf } from './message'
import { KEYS, SESSIONS } from './pageReads'
import { DifferRoom, PasswordsDiffer } from './PasswordsDiffer'
import { invalidate } from './reads'
import { useFocusAfterRevoke } from './revokeFocus'
import {
  PageHeader,
  PageLoading,
  RemovalSection,
  Row,
  Section,
} from './settingsLayout'
import {
  lastSeen,
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
import { WaitWords } from './WaitWords'
import { JOINING_DOT } from './joiningDot'
import { inUtc } from './utc'
import { count } from './count'

export interface AccountPageProps {
  /**
   * The account, or `undefined` while the screen reads the lists the page is
   * drawn from, or where no read of the list of accounts has answered: the
   * page then draws its header, named as its address spells it, and the
   * screen's notices, and nothing of an account.
   */
  user: User | undefined
  /**
   * Every account, so the last-administrator rule is answered from the list
   * the screen already holds rather than by a request of its own.
   * `undefined` with `user`.
   */
  users: User[] | undefined
  /**
   * The screen's wait for its lists has held long enough to draw "Loading…"
   * for it: drawn while `user` is `undefined`.
   */
  loadingDrawn?: boolean
  /**
   * Every library, for the grant editor — `undefined` where that list could
   * not be read. The screen hands the page its account only once this list
   * has been read or has failed.
   *
   * **Not read as `[]`.** That would draw "there are no libraries yet" after
   * a read that failed: a positive false claim, on the control that decides
   * what an account sees. What the page can always say is what the account
   * sees, because `user.every_library` says so and `user.libraries` carries
   * the names; what it cannot do without the list is offer a library nobody
   * granted yet.
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
   * read store's promise for the re-read, and this page holds its controls
   * until that settles — see `settled`. A caller that returns nothing is
   * not wrong, only unheld.
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

/** The confirm's words while the removal is out. */
const REMOVING = 'Removing…'

/**
 * The page's frame, which is drawn from the moment the screen is, whether or
 * not the account is known yet: its header and the screen's notices, keyed,
 * so that each is the same element, with whatever has focus in it, once the
 * account's body comes. What the header says of the account is held here
 * too, since a write's answer changes it before the list does.
 */
export function AccountPage({
  user,
  users,
  loadingDrawn = false,
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
  /** The name the address spells, which heads the page until it is drawn. */
  const named = nameAt(useLocation().pathname, USERS)?.name
  /**
   * What the last write on this page was answered with.
   *
   * **The hold alone is not enough, and this is the half that closes it.**
   * `settled` waits for the list read a write triggers, but that read can fail
   * or be superseded by a later one — and either way it resolves without
   * bringing fresh props, so the controls would come back live over pre-write
   * values. `patchUser` answers the account whole, so the page can keep that
   * answer and stop guessing: it is what is drawn, and what `toggleGrant`
   * computes the next grant list from. Retired the moment a new `user` prop
   * arrives, because the list is the truth as soon as it catches up.
   */
  const [applied, setApplied] = useState<User | undefined>()
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
  /** The `user` prop this page last drew — see `applied`. */
  const drawn = useRef(user)

  // Adjusted during render rather than in an effect: a new prop *is* the newer
  // fact, so there is no frame in which both are held.
  if (drawn.current !== user) {
    drawn.current = user
    if (applied !== undefined) setApplied(undefined)
    // **The tombstone goes with it.** A prop other than `undefined` arriving
    // for this page means an account of this name exists, and `Users` keys
    // the page by name — so where the page outlives a removal, an account
    // re-created under that name is handed this instance, and must not be
    // drawn as removed, with no controls. That is the recycled-id hazard
    // `Users.tsx`'s own doc comment describes, reached through the name
    // instead.
    if (removed) setRemoved(false)
  }

  /**
   * Draws a write's answer as the account: `change` of the account as the
   * page holds it when the answer comes, the last answer or else the newest
   * prop, not as it was when the press was made. A list read landing during
   * the write clears `applied` and brings a newer row, and a change made to
   * a press-time copy would drop whatever that read brought.
   */
  const apply = useCallback((change: (held: User) => User) => {
    setApplied((held) => {
      const now = held ?? drawn.current
      return now === undefined ? held : change(now)
    })
  }, [])

  if (user === undefined || users === undefined) {
    return (
      <div className="settings-page account-page">
        {/* Keyed, as in the drawn page, so that each is the same element
            there whatever the drawn page puts between them. */}
        <PageHeader
          key="header"
          back={{ to: USERS, label: 'Users' }}
          title={named}
          titleRef={heading}
        />
        <Fragment key="notices">{notices}</Fragment>
        <PageLoading drawn={loadingDrawn} />
      </div>
    )
  }

  /** The account as this page knows it: the last answer, else the list's row. */
  const shown = applied ?? user
  const isSelf = actingAs === user.id
  const seen = lastSeen(shown)
  const summary = `${
    shown.role === 'admin'
      ? 'Administrator'
      : shown.role === 'user'
        ? 'User'
        : shown.role
  }${JOINING_DOT}${seen === undefined ? 'no session open' : `last seen ${inUtc(seen)}`}`

  const back = { to: USERS, label: 'Users', closes: accountPath(user.name) }

  if (removed) {
    return (
      <div className="settings-page account-page">
        <PageHeader key="header" back={back} title={shown.name} />
        <p className="gone">
          Removed. The list has not caught up — this page is what is left of it,
          and nothing here can be edited.
        </p>
      </div>
    )
  }

  return (
    <div className="settings-page account-page">
      <PageHeader
        key="header"
        back={back}
        title={
          <>
            {shown.name}
            {isSelf && <span className="you"> — this is you</span>}
          </>
        }
        description={summary}
        titleRef={heading}
      />
      <Fragment key="notices">{notices}</Fragment>
      <AccountBody
        user={user}
        shown={shown}
        users={users}
        libraries={libraries}
        isSelf={isSelf}
        onChanged={onChanged}
        onRemoved={onRemoved}
        onLateRefusal={onLateRefusal}
        apply={apply}
        onGone={() => setRemoved(true)}
      />
    </div>
  )
}

/** What the page's body is drawn from, once the page knows its account. */
interface AccountBodyProps
  extends Pick<
    AccountPageProps,
    'libraries' | 'onChanged' | 'onRemoved' | 'onLateRefusal'
  > {
  user: User
  /** The account as the page knows it (`applied`). */
  shown: User
  users: User[]
  /** The account is the one this browser is signed in as. */
  isSelf: boolean
  /** Draws a write's answer as the account (`AccountPage`'s `apply`). */
  apply: (change: (held: User) => User) => void
  /** The account is gone: the page draws what is left of it instead. */
  onGone: () => void
}

/** Everything under the page's header and notices. */
function AccountBody({
  user,
  shown,
  users,
  libraries,
  isSelf,
  onChanged,
  onRemoved,
  onLateRefusal,
  apply,
  onGone,
}: AccountBodyProps) {
  const [failure, setFailure] = useState<string | undefined>()
  const [saving, setSaving] = useState(false)
  /**
   * `saving`, once it has gone on long enough to be drawn: the held look of
   * every control a write holds. What holds is `inFlight`, from the send.
   */
  const savingDrawn = useDrawnWait(saving)
  const [password, setPassword] = useState('')
  const [wasReset, setWasReset] = useState(false)
  const [confirming, setConfirming] = useState(false)
  const removeButton = useRef<HTMLButtonElement>(null)
  /**
   * Set when the removal's confirm is cancelled: focus goes back to the
   * Remove… that opened it, not to the document, where the confirm's Cancel
   * leaves it as it goes.
   */
  const backToRemove = useRef(false)
  useEffect(() => {
    if (confirming || !backToRemove.current) return
    backToRemove.current = false
    removeButton.current?.focus()
  }, [confirming])
  const [cost, setCost] = useState<RemovalAnswer | undefined>()
  /**
   * Why the dry run could not answer. **A slot of its own**, because the
   * dialog reads it: "working out what this would remove…" is drawn while
   * there is neither a cost nor a reason, and `begin()` clears the shared slot
   * on *every* write — so ticking a capability box after a failed dry run
   * would put the dialog back to "working out" for ever, with nothing in
   * flight. `Backup.tsx` keeps the same two slots for its own pair, and the
   * users screen keeps the list's failure in `Users.tsx` apart from the add's
   * refusal in `NewAccount.tsx`.
   */
  const [costFailure, setCostFailure] = useState<string | undefined>()
  /** The dry run's wait, drawn once it has lasted; until then it keeps its room. */
  const costDrawn = useDrawnWait(
    confirming && cost === undefined && costFailure === undefined,
  )
  const [removing, setRemoving] = useState(false)
  const removingDrawn = useDrawnWait(removing)
  /**
   * Whether the real `DELETE` is already on its way.
   *
   * **A ref, for the reason `RemoveRoot`'s and `RootRow`'s are**: React
   * batches state, so two presses dispatched before the next render both
   * read `removing` false, and no control is drawn disabled until the wait
   * has lasted.
   */
  const inFlight = useRef(false)
  /** False once the page has gone, and a refusal has no page here to say it. */
  const onScreen = useDrawn()
  /** The newest dry run — see `askCost`. */
  const latestCost = useRef(0)

  const sessionsId = useId()
  // Focus that a Revoke's row would take with it to the page goes to the
  // nearest row left in its list (`useFocusAfterRevoke`); with none left,
  // to the heading of Sessions & keys.
  const revokeFocus = useFocusAfterRevoke(
    { sessions: shown.sessions, keys: shown.keys },
    () => document.getElementById(sessionsId),
  )

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
    // about one write, and nothing else on this page clears it — so it would
    // stand over every later toggle, grant and refusal, still saying somebody
    // had just been signed out everywhere. O96: the mutation that follows a
    // notice is what retires it.
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
   * step, at the cost of the reload's wait on a screen touched rarely.
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
   * is on, where nothing else names the account. On this page it is said
   * too where the answer carries no sentence of the server's (`refusalOf`).
   */
  const failed = (late: string) => (err: unknown) => {
    inFlight.current = false
    setSaving(false)
    setRemoving(false)
    if (onScreen.current) setFailure(refusalOf(err, late))
    else onLateRefusal?.(`${late} (${reasonOf(err)}).`)
  }

  const save = (patch: UserPatch, late: string, after?: () => void) => {
    if (!begin()) return
    patchUser(user.id, patch)
      .then((answer) => {
        apply(() => answer)
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
    if (inFlight.current) return
    setLifting(false)
    onlyRadio.current?.focus()
  }

  const passwordBox = useRef<HTMLInputElement>(null)
  const setButton = useRef<HTMLButtonElement>(null)
  /**
   * The box is empty, as a set that worked leaves it, or this is the
   * person's own account.
   */
  const nothingToSet = isSelf || password === ''
  // A set that worked empties the box, which takes Set password away from a
  // person who pressed it: focus goes to the box, where the next one is
  // typed, and not to the page.
  useFocusLeavesHeld(nothingToSet, setButton, passwordBox)
  const confirmation = useConfirmPassword(password)
  const { differ, setConfirm } = confirmation

  const submitPassword = (e: React.FormEvent) => {
    e.preventDefault()
    if (password === '') return
    if (differ) {
      confirmation.heldPress()
      return
    }
    save({ password }, `A new password for ${shown.name} was not set`, () => {
      setPassword('')
      setConfirm('')
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
    row: { list: 'sessions' | 'keys'; id: number | string },
    send: () => Promise<void>,
    without: (held: User) => User,
    late: string,
  ) => {
    if (!begin()) return
    revokeFocus.pressed(row.list, row.id)
    send()
      // **The row goes locally too, for `save`'s reason one path over.**
      // `settled` waits for the list read, and that read can fail or be
      // superseded without bringing fresh props — leaving the revoked session
      // listed with a live Revoke, whose next press answers 404 for a session
      // that is already gone.
      .then(() => {
        // The caller's own keys and sessions are also `/account`'s and the
        // reader panel's lists, which keep the revoked row until read again.
        if (isSelf) void invalidate(row.list === 'keys' ? KEYS : SESSIONS)
        // To the account as the page holds it *now* (`apply`): a session
        // opened since the click, or a role another administrator changed,
        // is kept.
        apply(without)
        settled()
      })
      .catch(failed(late))
  }

  const askCost = () => {
    if (inFlight.current) return
    // **The newest dry run wins.** Two are routinely in flight — Remove…,
    // Cancel, Remove… again — and nothing makes them answer in order, so a
    // slow first answer landing after a failed second would draw five counts
    // and an enabled *Remove sam* underneath an alert saying the count could
    // not be taken.
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
        setCostFailure(
          refusalOf(
            err,
            `What removing ${shown.name} would take could not be counted`,
          ),
        )
      })
  }

  const confirmRemoval = () => {
    if (!begin()) return
    setRemoving(true)
    removeUser(user.id)
      // The body goes with the tombstone the page then draws, and its open
      // dialog with it: where the page outlives the removal (no `onRemoved`
      // takes it away), an account re-created under the removed one's name
      // is drawn with a body of its own, and no dialog comes back over it.
      .then(() => {
        settled(onGone)
        onRemoved?.(shown.name)
      })
      .catch(failed(`${shown.name} was not removed`))
  }

  const roleWhy = isSelf
    ? 'You cannot take away your own admin role; another administrator has to.'
    : lastAdmin
      ? `${shown.name} is the only administrator who can log in, so the role cannot change. Give another account the admin role first.`
      : undefined

  // The last-administrator rule reaches Remove as well as the role, and the
  // reason is said beside each control it holds.
  const removeWhy = lastAdmin
    ? `${shown.name} is the only administrator who can log in, so it cannot be removed. Give another account the admin role first.`
    : 'Asks first, with what it would take.'

  return (
    <>
      {failure !== undefined && <p role="alert">{failure}</p>}

      {!shown.can_log_in && (
        <p className="locked">
          Nobody can sign in as this account: it holds no usable password.
          Setting one below gives it a way in.
        </p>
      )}

      <Section title="Role & permissions">
        {/* O175: each `aria-label` on this page is its row label's words as
          written, so the name keeps their case whatever case a stylesheet
          draws the label in. */}
        <Row label="Role" help={roleWhy}>
          {(ids) => (
            // Held by `aria-disabled` once a write's wait is drawn, so that
            // it keeps focus, as a `HeldButton` does: `begin` refuses a
            // choice made then, and React puts the controlled value back.
            // `disabled` only where the role cannot change at all.
            <select
              id={ids.control}
              name="role"
              aria-label="Role"
              aria-describedby={ids.help}
              value={shown.role}
              disabled={isSelf || lastAdmin}
              aria-disabled={savingDrawn}
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
          )}
        </Row>

        {/* **`aria-disabled`, not `disabled`, on every checkbox on this page —
          here and in the grant list below. O159.** `disabled` draws a
          checked box in the UA's grey and takes its `accent-color` fill
          with it, so one tick would flash every box on the page.
          `accent-color`, `opacity` and `filter` on `:disabled` do not bring
          the fill back in Chromium; only redrawing the box with
          `appearance: none` can, which is a redesign of every checkbox.
          The lock never lived in the attribute: `begin` refuses a second
          write through `inFlight`, and each box is controlled, so React
          puts a refused tick back. On a box `aria-disabled` draws
          nothing, so it says the hold from the send. The role select,
          the password field and the buttons are held by it too, but from
          `savingDrawn`: on them it draws `:disabled`'s look (`base.css`),
          which waits until the write has lasted, and `inFlight` holds
          them from the send. With `aria-disabled`, a tick refused
          mid-save does nothing and shows no cue: the box keeps its real
          value. `aria-disabled` also leaves keyboard focus on the box,
          where in Chromium a control that becomes `disabled` loses it
          (`docs/notes.md`, "Admin follow-ups"). */}
        {/* O154: one named group, as the grants below are one named group. */}
        <div className="settings-group" role="group" aria-label="Permissions">
          <Row check label="Can download" help="Save whole editions as files.">
            {(ids) => (
              <input
                type="checkbox"
                id={ids.control}
                name="can_download"
                aria-describedby={ids.help}
                checked={shown.can_download}
                aria-disabled={saving}
                onChange={(e) =>
                  save(
                    { can_download: e.target.checked },
                    notSaved('Can download'),
                  )
                }
              />
            )}
          </Row>
          <Row
            check
            label="Can stream"
            help="Read pages, in the browser or an OPDS app."
          >
            {(ids) => (
              <input
                type="checkbox"
                id={ids.control}
                name="can_stream"
                aria-describedby={ids.help}
                checked={shown.can_stream}
                aria-disabled={saving}
                onChange={(e) =>
                  save({ can_stream: e.target.checked }, notSaved('Can stream'))
                }
              />
            )}
          </Row>
          <Row
            check
            label="Can stamp"
            help="Write ComicInfo.xml into archives."
          >
            {(ids) => (
              <input
                type="checkbox"
                id={ids.control}
                name="can_stamp"
                aria-describedby={ids.help}
                checked={shown.can_stamp}
                aria-disabled={saving}
                onChange={(e) =>
                  save({ can_stamp: e.target.checked }, notSaved('Can stamp'))
                }
              />
            )}
          </Row>
        </div>
      </Section>

      <Section
        title="Libraries"
        description="Which libraries this account may see."
      >
        <fieldset className="settings-group">
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
              {/* A radio group, so the row's word and the administrator's
                help are read as focus enters the radios, which each keep
                their own label. */}
              <Row
                label="Sees"
                help={
                  shown.role === 'admin'
                    ? 'An administrator sees every library whatever is chosen here; the choice applies if the role becomes user.'
                    : undefined
                }
                group="radiogroup"
              >
                <div className="settings-choices">
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
                      // A radio already chosen fires no change, and "Only
                      // these" stands while the question is up, so choosing
                      // it again by click or by an arrow key is heard here.
                      onClick={() => {
                        if (inFlight.current) return
                        setLifting(false)
                      }}
                    />
                    <span>Only these</span>
                  </label>
                </div>
              </Row>
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
                  <HeldButton
                    type="button"
                    name="lift-limit"
                    held={savingDrawn}
                    onClick={lift}
                  >
                    See every library
                  </HeldButton>
                  <HeldButton
                    type="button"
                    name="keep-limit"
                    ref={keepButton}
                    held={savingDrawn}
                    onClick={keepLimit}
                  >
                    Keep the limit
                  </HeldButton>
                </div>
              )}
              {restricted && (
                <ul className="grant-list settings-group">
                  {libraries.map((library) => (
                    <li key={library.id}>
                      <Row
                        check
                        label={library.name}
                        help={
                          <span className="grant-dirs">
                            {library.dirs.map((dir) => (
                              <code key={dir.path}>{dir.path}</code>
                            ))}
                          </span>
                        }
                      >
                        {(ids) => (
                          <input
                            type="checkbox"
                            id={ids.control}
                            name={`library-${library.id}`}
                            aria-describedby={ids.help}
                            checked={granted.includes(library.id)}
                            aria-disabled={saving}
                            onChange={(e) =>
                              toggleGrant(library.id, e.target.checked)
                            }
                          />
                        )}
                      </Row>
                    </li>
                  ))}
                </ul>
              )}
              {unlistedLine}
              {noLibraryLine}
            </>
          )}
        </fieldset>
      </Section>

      <Section title="Password">
        <form className="reset settings-group" onSubmit={submitPassword}>
          <Row
            label="New password"
            help={
              isSelf ? (
                <>
                  Not here: this ends every session, the one making the change
                  included, so the server refuses it and names{' '}
                  <code>/api/password</code>, which keeps that session. Change
                  yours on <Link to="/account">your account screen</Link>.
                </>
              ) : (
                `Ends every session ${shown.name} has open. API keys are untouched: they are separate credentials, revoked on their own.`
              )
            }
          >
            {(ids) => (
              <>
                {/* Held as the role is, and read-only from the send, so
                  typing changes nothing while a write is out. Enter in the
                  box presses Set password, which turns the press down while
                  it is held; before then, and for a submit that reaches the
                  form another way, `begin` refuses `submitPassword`'s
                  write. `disabled` only where nothing can be set here. */}
                <input
                  ref={passwordBox}
                  type="password"
                  id={ids.control}
                  name="password"
                  aria-label="New password"
                  aria-describedby={ids.help}
                  value={password}
                  disabled={isSelf}
                  readOnly={saving}
                  aria-disabled={savingDrawn}
                  onChange={(e) => {
                    if (inFlight.current) return
                    setPassword(e.target.value)
                  }}
                />
              </>
            )}
          </Row>
          <Row label="Confirm password">
            {(ids) => (
              /* Held as New password is. */
              <input
                type="password"
                id={ids.control}
                name="confirm-password"
                aria-label="Confirm password"
                {...confirmation.confirmBox}
                disabled={isSelf}
                readOnly={saving}
                aria-disabled={savingDrawn}
                onChange={(e) => {
                  if (inFlight.current) return
                  confirmation.confirmBox.onChange(e)
                }}
              />
            )}
          </Row>
          {/* Laid over the row below, Set password's, which it covers
              whole, so drawing it moves nothing (`Callout`). */}
          <PasswordsDiffer {...confirmation.line} />
          {/* At the box's foot, after both boxes, where a form's submit is
              looked for, as Change password is on the account screen. Not
              beside Confirm password: that row would be the section's last,
              with no row under it for the line saying the two differ to lie
              over, which would then lie over the box being typed in. Kept as
              tall as that line needs (`DifferRoom`). */}
          <div className="settings-actions keeps-differ-room">
            <DifferRoom />
            <HeldButton
              ref={setButton}
              type="submit"
              name="reset"
              held={savingDrawn || differ}
              onHeldPress={confirmation.heldPress}
              disabled={nothingToSet}
            >
              Set password
            </HeldButton>
          </div>
          {/* Drawn with the form, empty, so that the sentence is read when
              it lands in it: a live region drawn together with what it says
              is often not read at all, and focus goes to the box, not here.
              Empty, it takes no room and no line (`settings.css`). */}
          <p className="done" aria-live="polite">
            {wasReset &&
              `Password set. ${shown.name} is signed out everywhere.`}
          </p>
        </form>
      </Section>

      <Section
        title="Sessions & keys"
        titleId={sessionsId}
        description={`${count(shown.sessions.length, 'session', 'sessions')}${JOINING_DOT}${count(shown.keys.length, 'API key', 'API keys')}`}
      >
        {shown.sessions.length === 0 ? (
          <p className="empty">No sessions open.</p>
        ) : (
          <ul className="session-list settings-group">
            {shown.sessions.map((session) => (
              <li key={session.id}>
                <Row
                  label={
                    <>
                      <span className="what">
                        {session.user_agent ?? 'no user-agent recorded'}
                        {JOINING_DOT}
                        {session.ip ?? 'no address recorded'}
                      </span>
                      {session.current && (
                        <span className="current">this browser</span>
                      )}
                    </>
                  }
                  help={
                    <span className="when">
                      started {inUtc(session.created)}
                      {JOINING_DOT}last seen {inUtc(session.last_seen)}
                    </span>
                  }
                >
                  <HeldButton
                    type="button"
                    ref={revokeFocus.revokeRef('sessions', session.id)}
                    name={`revoke-session-${session.id}`}
                    held={savingDrawn}
                    onClick={() =>
                      revoke(
                        { list: 'sessions', id: session.id },
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
                  </HeldButton>
                </Row>
              </li>
            ))}
          </ul>
        )}
        {shown.keys.length > 0 && (
          <ul className="key-list settings-group">
            {shown.keys.map((key) => (
              <li key={key.id}>
                <Row
                  label={<span className="what">{key.name}</span>}
                  help={
                    <span className="when">
                      API key{JOINING_DOT}made {inUtc(key.created)}
                      {JOINING_DOT}
                      {key.last_used === null
                        ? 'never used'
                        : `last used ${inUtc(key.last_used)}`}
                    </span>
                  }
                >
                  <HeldButton
                    type="button"
                    ref={revokeFocus.revokeRef('keys', key.id)}
                    name={`revoke-key-${key.id}`}
                    held={savingDrawn}
                    onClick={() =>
                      revoke(
                        { list: 'keys', id: key.id },
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
                  </HeldButton>
                </Row>
              </li>
            ))}
          </ul>
        )}
      </Section>

      <RemovalSection title="Removal">
        <Row label="Remove this account" help={removeWhy}>
          {/* Held, not taken away, while the confirm its press opens is
            open, as the library's and a directory's Remove… are, so focus
            stays on it; and held by a write once its wait is drawn, as the
            page's other buttons are. `disabled` only where the account
            cannot be removed at all. */}
          <HeldButton
            ref={removeButton}
            type="button"
            name="remove"
            held={confirming || savingDrawn}
            disabled={lastAdmin}
            onClick={askCost}
          >
            Remove…
          </HeldButton>
        </Row>
        {confirming && (
          <div
            className="remove-user"
            role="group"
            aria-label={`Remove the account ${shown.name}`}
          >
            {costFailure !== undefined && <p role="alert">{costFailure}</p>}
            {cost === undefined && costFailure === undefined && (
              <p className={costDrawn ? undefined : 'wait-unseen'}>
                Working out what this would remove…
              </p>
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
              </>
            )}
            {/* The confirm and Cancel, a form foot's pair, as a library's
                and a directory's removals draw them. */}
            <div className="settings-actions">
              {cost !== undefined && (
                <HeldButton
                  type="button"
                  className="confirm keeps-wait-width"
                  name="confirm-remove"
                  onClick={confirmRemoval}
                  held={removingDrawn || savingDrawn}
                >
                  <WaitWords
                    drawn={removingDrawn}
                    words={`Remove ${shown.name}`}
                    wait={REMOVING}
                  />
                </HeldButton>
              )}
              <HeldButton
                type="button"
                name="cancel-remove"
                onClick={() => {
                  if (removing) return
                  backToRemove.current = true
                  setConfirming(false)
                  setCost(undefined)
                  setCostFailure(undefined)
                }}
                held={removingDrawn}
              >
                Cancel
              </HeldButton>
            </div>
          </div>
        )}
      </RemovalSection>
    </>
  )
}
