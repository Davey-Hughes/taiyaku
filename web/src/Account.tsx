import {
  type ChangeEvent,
  type ComponentProps,
  type FocusEvent,
  type FormEvent,
  useCallback,
  useEffect,
  useId,
  useLayoutEffect,
  useRef,
  useState,
} from 'react'
import {
  changePassword,
  createKey,
  listKeys,
  listSessions,
  logout,
  revokeKey,
  revokeSession,
  type ApiKey,
  type SessionRow,
} from './api'
import { about } from './about'
import { CatalogAddress, CopyWords } from './CatalogAddress'
import { Callout } from './Callout'
import { useConfirmPassword } from './confirmPassword'
import { useCopy } from './copy'
import { usePanelFocus } from './focus'
import { HeldButton } from './HeldButton'
import { focusLeaves } from './focusLeaves'
import { useDrawnWait } from './hooks'
import { reasonOf, refusalOf } from './message'
import { DifferRoom, PasswordsDiffer } from './PasswordsDiffer'
import { Refusal } from './Refusal'
import { useFocusAfterRevoke } from './revokeFocus'
import { PageHeader, Row, Section } from './settingsLayout'

/**
 * The account screen: change a password, hold API keys, see what is signed in.
 *
 * **No user administration** (§9): creating accounts, granting roots and
 * setting capabilities are spec C4's configuration UI, and the CLI covers them
 * in between. Everything here is about *this* account, which is why no route it
 * calls names another one.
 *
 * **Two placements, one component** — O94. `'panel'` is the reader's: `App`
 * renders it over the page and `A` opens it. `'page'` is `/account`, drawn in
 * `Shell`'s `<main>` beside the library's sidebar. The panel was the only one
 * from spec A2 until then, so the one screen where an account changes its own
 * password could not be reached without opening a work, and C4's users screen
 * had to tell an administrator to "open any work and press A".
 *
 * **The two share their state, their handlers, their controls and what each
 * section holds, in one order; each frames them in its own markup.** The
 * page is a settings page, drawn on the settings components
 * (`settingsLayout.tsx`); the panel is one of the reader's dialogs.
 */

/**
 * The class each placement paints a shared part with. A part the page's
 * entry leaves out takes no class there: the settings components draw its
 * sections, headings and rows, and `base.css` draws its boxes and buttons
 * as it draws every settings page's.
 */
interface Look {
  section?: string
  field?: string
  wide?: string
  signOut?: string
  /**
   * Beside the `secret` the shared box always wears. The page's box is
   * `admin.css`'s `.account .secret`, reached by that literal alone, so the
   * page sets none here; the panel's is `reader.css`'s `.reader-secret`.
   */
  secret?: string
  failure?: string
  row?: string
  meta?: string
  button?: string
  heading?: string
}

/**
 * **Two placements, two vocabularies, one component.**
 *
 * The panel is one of the reader's dialogs and takes ③'s classes. The page
 * is a settings screen inside `.library` and takes `admin.css`'s, for the
 * parts the settings components do not draw: the alert above its sections,
 * and the one-time secret and its caption.
 *
 * **The panel's colours are the reader's and resolve dark in both themes**,
 * because `App` renders `<Account>` as a sibling of `<Settings>` inside
 * `<div className="reader">`, where `reader.css` sets `color-scheme: dark`.
 * That containment is load-bearing: the same classes *outside* `.reader`
 * would draw `--text` pale-on-paper over a fixed dark ground.
 */
const LOOKS: Record<'panel' | 'page', Look> = {
  panel: {
    section: 'reader-section',
    heading: 'reader-panel-heading',
    field: 'reader-field reader-field-block',
    wide: 'reader-button reader-button-wide',
    signOut: 'reader-button reader-button-wide',
    secret: 'reader-secret',
    failure: 'reader-failure',
    // Both halves at every site. `.reader-section-head` is ③'s row, which
    // has no margin because no panel of ③'s ever stacked two of them; the
    // modifier is the 8 px each of this panel's rows carried as an inline
    // style. See `reader.css` for why an adjacent-sibling rule cannot stand
    // in for it here.
    row: 'reader-section-head reader-section-head-stacked',
    meta: 'reader-meta',
    button: 'reader-button',
  },
  page: {
    failure: 'account-failure',
    meta: 'why',
  },
}

/** What the API keys section says keys are for, in both placements. */
const KEYS_ARE_FOR =
  'For OPDS readers like Panels or Chunky: give the key as the password, with your account name as the username.'

/** What Password's polite region says when a change works. */
const PASSWORD_CHANGED =
  'Password changed. Every other signed-in browser has been signed out; API keys still work.'

/**
 * What API keys' polite region says when a key is made: its name, and never
 * its secret, since what lands in that region is read aloud. A name of
 * spaces names nothing, as the server's own check on a key's name reads it.
 */
const keyMade = (name: string) => {
  const named = name.trim()
  return `Key made${named === '' ? '' : `: ${named}`}. Copy it now — it is not stored and cannot be shown again.`
}

/**
 * The secret's caption: the key it belongs to, among the keys listed under
 * it, and what to do with it. A name of spaces names nothing, as in
 * `keyMade`.
 */
const captionOf = (name: string) => {
  const named = name.trim()
  const what = 'it is not stored and cannot be shown again.'
  return named === ''
    ? `Copy this now — ${what}`
    : `Key “${named}”: copy this now — ${what}`
}

/**
 * The key this screen made last, while its box is shown.
 *
 * One state, so that each change to it is made against the key shown when
 * the change lands: a revoke that answers late, after a later key's box is
 * drawn, then finds that key and leaves it.
 */
interface ShownKey {
  /**
   * Its one secret. Kept in component state and nowhere else — not
   * localStorage, not a URL — because the server stores only its hash and
   * cannot answer it again. It goes when the panel closes, which the copy
   * beside it says plainly rather than leaving a reader to find out. It is
   * drawn in its box and never in a live region.
   */
  secret: string
  /**
   * Its id in the key list, where the first listing drawn after its create
   * told it: the one key in that listing the list before the create did not
   * hold.
   * Where two were new, as when another browser made one meanwhile, or that
   * listing failed, it stays unknown, and no revocation takes the box away,
   * since a box taken away by mistake loses a secret nothing can show again.
   */
  id?: number
  /** The name it was made with, which its caption names. */
  name: string
  /**
   * What API keys' visually hidden region says: that it was made, until
   * the next key is asked for.
   */
  line: string | null
}

/**
 * One list's listings: how many have been asked for, the latest of them
 * drawn, and, for each listing that has not landed, what it does alongside
 * its list.
 */
interface Listings<Row> {
  asked: number
  drawn: number
  waiting: Map<number, (listed: Row[] | undefined) => void>
}

/**
 * Lands listing `nth` of a list.
 *
 * A listing later than any drawn yet is drawn, and in the same render runs
 * what waits alongside it and alongside each earlier listing still out.
 * Each of those was asked for once its press's write had answered, so a
 * list read later answers that press as well: a made key's box comes with
 * the row it belongs to, and an alert a revocation takes away goes with
 * that row, so what is below moves once. What waits alongside a later
 * listing still out waits for it: this list was read before that press's
 * write. A listing that lands after a later one has been drawn is not
 * drawn: it was read first, so it would list again a key or a session the
 * later one no longer holds, or take a made key's row from under its box;
 * and what waited alongside it ran with the later one. A failed listing
 * draws nothing, and runs only what waits alongside it, with `undefined`;
 * what waits alongside an earlier listing still out waits for that one.
 */
function landListing<Row>(
  list: Listings<Row>,
  nth: number,
  listed: Row[] | undefined,
  draw: (listed: Row[]) => void,
) {
  if (listed !== undefined && nth > list.drawn) {
    list.drawn = nth
    draw(listed)
    for (const [each, alongside] of list.waiting) {
      if (each > nth) continue
      list.waiting.delete(each)
      alongside(listed)
    }
    return
  }
  const own = list.waiting.get(nth)
  list.waiting.delete(nth)
  own?.(listed)
}

export type AccountProps = {
  /** Called after signing out, so the gate can show the login screen. */
  onSignedOut: () => void
} & (
  | {
      /** The reader's overlay, and the default. Drawn while `visible`. */
      placement?: 'panel'
      visible: boolean
      onClose: () => void
    }
  | {
      /**
       * `/account`, inside `Shell`'s `<main>`. Drawn for as long as the route
       * is mounted, so it takes no `visible` and has nothing to close.
       */
      placement: 'page'
    }
)

export function Account(props: AccountProps) {
  const { onSignedOut } = props
  const visible = props.placement === 'page' || props.visible
  const look = LOOKS[props.placement ?? 'panel']
  const panelRef = useRef<HTMLDivElement>(null)
  usePanelFocus(panelRef, props.placement !== 'page' && visible)
  const [keys, setKeys] = useState<ApiKey[]>([])
  const [sessions, setSessions] = useState<SessionRow[]>([])
  const [keyName, setKeyName] = useState('')
  const [shown, setShown] = useState<ShownKey | null>(null)
  /** A write of the secret was refused, so it is drawn to copy by hand. */
  const [copyRefused, setCopyRefused] = useState(false)
  const { canCopy, said, shown: copied, copy } = useCopy()
  const captionId = useId()
  const secretBoxRef = useRef<HTMLDivElement>(null)
  const secretRef = useRef<HTMLSpanElement>(null)
  const [current, setCurrent] = useState('')
  const [next, setNext] = useState('')
  const confirmation = useConfirmPassword(next)
  const { setConfirm, differ } = confirmation
  /**
   * Password's visually hidden region says the last change worked, until
   * the next change is asked for.
   */
  const [passwordChanged, setPasswordChanged] = useState(false)
  /**
   * Why the last Create, or the last Change password, was refused, said in
   * a callout at its form, which moves nothing (the user's ruling of
   * 2026-10-25). Each goes when a box of its form is edited, when focus
   * leaves the form, or as the next attempt is sent.
   */
  const [keyRefusal, setKeyRefusal] = useState<string | null>(null)
  const [passwordRefusal, setPasswordRefusal] = useState<string | null>(null)
  /**
   * A revocation or a sign-out that failed, said above the sections: each
   * answers a button of a list or of the screen, with no form of its own to
   * hold a callout and no box whose edit would take it away.
   */
  const [failure, setFailure] = useState<string | null>(null)
  /**
   * A request from either form is out. One at a time: a second Create sent
   * before the first answered would make a second permanent key, and this
   * screen shows one secret, so the other could never be read. Each
   * handler's guard turns a press down from the send; `busyDrawn` draws
   * both submits held once the wait has lasted.
   */
  const [busy, setBusy] = useState(false)
  const busyDrawn = useDrawnWait(busy)
  /**
   * The OPDS catalog's path, once `/api/about` has answered with it, or
   * `null` once it has failed to.
   */
  const [catalog, setCatalog] = useState<string | null | undefined>()
  /**
   * How many times the panel has closed. The panel keeps its state while it
   * is hidden, and the close takes away what the screen drew, so an answer
   * that lands after a close draws nothing in a panel opened since: not a
   * create's one-time secret, its line or its refusal; not a password
   * change's line or its refusal, nor its emptying of boxes typed in since;
   * not a failed revocation's or sign-out's alert. Each press takes this
   * count (`openSincePress`) and asks it again when its answer lands. The
   * lists are drawn all the same, since what they say is true.
   */
  const closes = useRef(0)
  /** Taken at a press: whether the panel has not closed since. */
  const openSincePress = () => {
    const atPress = closes.current
    return () => closes.current === atPress
  }
  /**
   * How many failures have been said above the sections. A revocation's
   * list change takes away only a failure said before its press: one said
   * since is a later revocation's or sign-out's, and still true.
   */
  const failures = useRef(0)
  /** Each list's listings, as `landListing` lands them. */
  const listings = useRef<{
    keys: Listings<ApiKey>
    sessions: Listings<SessionRow>
  }>({
    keys: { asked: 0, drawn: 0, waiting: new Map() },
    sessions: { asked: 0, drawn: 0, waiting: new Map() },
  })
  const keyNameRef = useRef<HTMLInputElement>(null)
  const signedInId = useId()
  // Focus that a Revoke's row would take with it, out of the panel's
  // dialog, goes to the nearest row left in its list (`useFocusAfterRevoke`);
  // with none left, to the New key box, where a key is added, or to Signed
  // in's heading.
  const revokeFocus = useFocusAfterRevoke({ keys, sessions }, (list) =>
    list === 'keys' ? keyNameRef.current : document.getElementById(signedInId),
  )

  /**
   * Re-read both lists, each drawn as it lands, and the keys with what
   * `alongside.keys` sets, the sessions with what `alongside.sessions` sets,
   * in one render: a key made or revoked changes its box and the key list
   * together, and a password change says it worked with the sessions it
   * signed out gone, so what is below them moves once (the user's rulings
   * of 2026-10-25). Neither list waits for the other, since a create or a
   * key's revocation never changes the sessions. Each list's promise is
   * returned, for a create to hold Create until its box is drawn, and a
   * change to hold Change password until its line is said.
   *
   * One function for the effect *and* every action that changes them, rather
   * than the effect carrying its own copy: two copies of "reload the panel" is
   * how one of them comes to reload only half of it. A failure leaves the last
   * good list on screen — a reader whose keys did not reload can still change a
   * password, and an empty list would claim they have none. `alongside.keys`
   * is handed the keys' listing, or `undefined` where it failed. Each
   * `alongside` runs once, with the listing that draws its list first, which
   * may be a later one (`landListing`), since what it does answers its own
   * press: a create's box holds the only copy of its secret.
   */
  const refresh = useCallback(
    (
      alongside: {
        keys?: (listed: ApiKey[] | undefined) => void
        sessions?: () => void
      } = {},
    ) => {
      /** Asks for a listing of `list`, and lands it as it answers. */
      const ask = <Row,>(
        list: Listings<Row>,
        read: () => Promise<Row[]>,
        draw: (listed: Row[]) => void,
        also: ((listed: Row[] | undefined) => void) | undefined,
      ) => {
        list.asked += 1
        const nth = list.asked
        if (also !== undefined) list.waiting.set(nth, also)
        return read()
          .catch(() => undefined)
          .then((listed) => {
            landListing(list, nth, listed, draw)
          })
      }
      const { keys: keyList, sessions: sessionList } = listings.current
      const sessions = ask(
        sessionList,
        listSessions,
        setSessions,
        alongside.sessions,
      )
      const keys = ask(keyList, listKeys, setKeys, alongside.keys)
      return { keys, sessions }
    },
    [],
  )

  useEffect(() => {
    if (visible) {
      void refresh()
      return
    }
    // **Closing is not unmounting.** `App` renders this element
    // unconditionally, so `return null` below hides it while React keeps every
    // hook's state — and `shown` held the one-time API key secret, which
    // was redisplayed to anyone who reopened the panel for the rest of the
    // page's life. The doc comment above claimed it "goes when the panel
    // closes" and the copy beside it says the key "cannot be shown again";
    // both were false. The typed passwords survived a failed attempt the same
    // way. Found by the whole-branch review, 2026-08-18.
    closes.current += 1
    setShown(null)
    setCurrent('')
    setNext('')
    setConfirm('')
    setKeyName('')
    setPasswordChanged(false)
    setKeyRefusal(null)
    setPasswordRefusal(null)
    setFailure(null)
  }, [visible, refresh, setConfirm])

  // The box drawn in place of the one a refused write was pressed on takes
  // its focus, with the secret selected, so that it is copied by hand where
  // the press could not copy it.
  useLayoutEffect(() => {
    if (!copyRefused) return
    secretBoxRef.current?.focus()
    const secret = secretRef.current
    if (secret !== null) window.getSelection()?.selectAllChildren(secret)
  }, [copyRefused])

  useEffect(() => {
    if (!visible) return
    let cancelled = false
    // A failure is said in the address's row rather than in an alert: the
    // address helps beside the OPDS line, and this screen's alerts are for
    // what the reader did.
    about()
      .then((answer) => {
        if (!cancelled) setCatalog(answer.catalog)
      })
      .catch(() => {
        if (!cancelled) setCatalog(null)
      })
    return () => {
      cancelled = true
    }
  }, [visible])

  if (!visible) return null

  const create = (event: FormEvent) => {
    event.preventDefault()
    if (busy) return
    setBusy(true)
    setKeyRefusal(null)
    // The line is emptied as each key is asked for, so that the next key's
    // line is spoken though its words may be the last one's, and so that no
    // line stands beside a refusal.
    setShown((now) => now && { ...now, line: null })
    const known = new Set(keys.map((key) => key.id))
    const sent = keyName
    const stillOpen = openSincePress()
    // The box is drawn with the first listing drawn after the create, its
    // own or a later one, so the box and the made key's row come together,
    // and Create is held until its own has landed. Not after a close: the
    // close took this screen's secret away.
    createKey(sent)
      .then(
        (secret) =>
          refresh({
            keys: (listed) => {
              if (!stillOpen()) return
              const [only, ...more] = (listed ?? []).filter(
                (key) => !known.has(key.id),
              )
              setShown({
                secret,
                id:
                  only !== undefined && more.length === 0 ? only.id : undefined,
                name: sent,
                line: keyMade(sent),
              })
              setCopyRefused(false)
              // A name typed while the list was read is for the next key.
              setKeyName((now) => (now === sent ? '' : now))
            },
          }).keys,
      )
      .catch((why: unknown) => {
        if (stillOpen()) setKeyRefusal(refusalOf(why, 'The key was not made'))
      })
      .finally(() => {
        setBusy(false)
      })
  }

  const change = (event: FormEvent) => {
    event.preventDefault()
    if (differ) {
      confirmation.heldPress()
      return
    }
    if (busy) return
    setBusy(true)
    setPasswordRefusal(null)
    // Emptied as each change is asked for, so that the next change's line
    // is spoken though its words are the last one's.
    setPasswordChanged(false)
    const stillOpen = openSincePress()
    // The line is said with the listing of the sessions that follows the
    // change, where the sessions it signed out are gone, and Change
    // password is held until then. The lists are read after a close too,
    // since a reopen's listing may have been read before the change signed
    // those sessions out; but the close emptied this form, and what is
    // typed since is not this change's to empty.
    changePassword(current, next)
      .then(() => {
        if (stillOpen()) {
          setCurrent('')
          setNext('')
          setConfirm('')
        }
        return refresh({
          sessions: () => {
            if (stillOpen()) setPasswordChanged(true)
          },
        }).sessions
      })
      .catch((why: unknown) => {
        if (stillOpen()) {
          setPasswordRefusal(refusalOf(why, 'The password was not changed'))
        }
      })
      .finally(() => {
        setBusy(false)
      })
  }

  // What both placements draw alike. Each frames these in its own markup
  // below, in the same order, so a control or a fact changed here changes in
  // both.

  const failureAlert =
    failure === null ? null : (
      <p role="alert" className={look.failure}>
        {failure}
      </p>
    )

  // What each box is bound to. Each placement writes the box in its own
  // markup, names it there and ties it to its label.
  const currentBox = {
    type: 'password',
    autoComplete: 'current-password',
    value: current,
    onChange: (e: ChangeEvent<HTMLInputElement>) => {
      setPasswordRefusal(null)
      setCurrent(e.target.value)
    },
  } satisfies ComponentProps<'input'>

  const nextBox = {
    type: 'password',
    autoComplete: 'new-password',
    value: next,
    onChange: (e: ChangeEvent<HTMLInputElement>) => {
      setPasswordRefusal(null)
      setNext(e.target.value)
    },
  } satisfies ComponentProps<'input'>

  const confirmBox = {
    type: 'password',
    autoComplete: 'new-password',
    ...confirmation.confirmBox,
    onChange: (e: ChangeEvent<HTMLInputElement>) => {
      setPasswordRefusal(null)
      confirmation.confirmBox.onChange(e)
    },
  } satisfies ComponentProps<'input'>

  const keyNameBox = {
    ref: keyNameRef,
    placeholder: 'Which device is this for?',
    value: keyName,
    onChange: (e: ChangeEvent<HTMLInputElement>) => {
      setKeyRefusal(null)
      setKeyName(e.target.value)
    },
  } satisfies ComponentProps<'input'>

  // What each form's `onBlur` does: focus leaving the form takes its
  // refusal's callout away, so that it never lies over what is focused next.
  const leavingKeyForm = (e: FocusEvent<HTMLFormElement>) => {
    if (keyRefusal !== null && focusLeaves(e)) setKeyRefusal(null)
  }
  const leavingPasswordForm = (e: FocusEvent<HTMLFormElement>) => {
    if (passwordRefusal !== null && focusLeaves(e)) setPasswordRefusal(null)
  }

  const changeButton = (
    <HeldButton
      type="submit"
      className={look.wide}
      held={busyDrawn || differ}
      onHeldPress={confirmation.heldPress}
    >
      Change password
    </HeldButton>
  )

  // Each drawn straight after the row it answers, and laid over the rows
  // below it (`Callout`): the line and a refused Change password after
  // Confirm password's row, over Change password's; a refused Create after
  // the New key row, over the shown key's box or the first key. A refusal
  // whose words have no room below its row lies over the rows above it.
  const differLine = <PasswordsDiffer {...confirmation.line} />

  const keyRefusalCallout =
    keyRefusal === null ? null : (
      <Callout>
        <Refusal>{keyRefusal}</Refusal>
      </Callout>
    )
  const passwordRefusalCallout =
    passwordRefusal === null ? null : (
      <Callout>
        <Refusal>{passwordRefusal}</Refusal>
      </Callout>
    )

  const createButton = (
    <HeldButton type="submit" className={look.button} held={busyDrawn}>
      Create
    </HeldButton>
  )

  // A row in every state, with a button's room in each (`CatalogAddress`),
  // so that API keys holds a row above New key: with no key listed, a
  // refused Create lies over the rows above New key, and with no room there
  // it would cover the box it answers.
  const address = (
    <CatalogAddress catalog={catalog} className={look.button} keepsButtonRoom />
  )

  const secretWords = (
    <>
      <span ref={secretRef} className="secret-value">
        {shown?.secret}
      </span>
      <CopyWords
        id={captionId}
        className={`secret-caption ${look.meta ?? ''}`.trim()}
        shown={canCopy && !copyRefused ? copied : undefined}
      >
        {captionOf(shown?.name ?? '')}
      </CopyWords>
    </>
  )

  // A press copies the secret where the clipboard can be written, and the
  // box is a button then, named by its caption, which says what a press
  // does, so that its name never reads the secret out; the caption gives
  // way to "Copied" for a moment after a press that copied, as Copy
  // address's word does (`CopyWords`). Elsewhere, and once a write is
  // refused, it is text to be selected by hand (`useCopy`).
  const secretBox =
    shown === null ? null : canCopy && !copyRefused ? (
      <button
        type="button"
        className={`secret ${look.secret ?? ''}`.trim()}
        aria-labelledby={captionId}
        onClick={() => {
          void copy(shown.secret).then((result) => {
            if (result === 'Not copied') setCopyRefused(true)
          })
        }}
      >
        {secretWords}
      </button>
    ) : (
      <div
        ref={secretBoxRef}
        className={`secret ${look.secret ?? ''}`.trim()}
        tabIndex={copyRefused ? -1 : undefined}
      >
        {secretWords}
      </div>
    )

  // Beside the password form, and visually hidden, so that what it says
  // takes no room and moves nothing (the user's ruling of 2026-10-25).
  // Drawn with the screen, empty, so that the line is read when it lands in
  // it: a live region drawn together with what it says is often not read at
  // all.
  const passwordRegion = (
    <span aria-live="polite" className="visually-hidden">
      {passwordChanged ? PASSWORD_CHANGED : null}
    </span>
  )

  // In the form that made the key, beside the secret's box, and visually
  // hidden, so that what they say takes no room and moves nothing. Drawn
  // with the screen, empty, as Password's region is.
  const keyRegions = (
    <>
      <span aria-live="polite" className="visually-hidden">
        {shown?.line}
      </span>
      {canCopy ? (
        <span role="status" className="visually-hidden">
          {said}
        </span>
      ) : null}
    </>
  )

  /** Says a revocation's or a sign-out's failure above the sections. */
  const sayFailure = (message: string) => {
    failures.current += 1
    setFailure(message)
  }

  /**
   * Taken at a revocation's press: what its list change does to the failure
   * said above the sections. It takes away one said before the press, in
   * the same render as the list change, and leaves one said since. The
   * count alone decides in a panel opened since as well: the close took
   * away the failure said before it, and one said since was counted.
   */
  const failureGoesWithPress = () => {
    const atPress = failures.current
    return () => {
      if (failures.current === atPress) setFailure(null)
    }
  }

  const keyUse = (key: ApiKey) =>
    key.last_used === null ? 'never used' : `last used ${key.last_used}`

  const revokeKeyButton = (key: ApiKey) => (
    <button
      type="button"
      ref={revokeFocus.revokeRef('keys', key.id)}
      className={look.button}
      onClick={() => {
        revokeFocus.pressed('keys', key.id)
        // Said rather than swallowed, and `refresh` only on success:
        // a revocation that did not happen looked exactly like one
        // that did — the row stayed, no alert appeared, and a reader
        // believed the lost device's key was dead when it still
        // worked. Found by the third whole-branch review.
        const stillOpen = openSincePress()
        const failureGoes = failureGoesWithPress()
        void revokeKey(key.id)
          .then(
            () =>
              refresh({
                keys: () => {
                  // A revoked key's secret no longer works, so its box goes,
                  // with its row; another key's revocation leaves a secret
                  // that still does. Read against the key shown once the
                  // listing lands, which may be a later one than when it
                  // was pressed.
                  setShown((now) => (now?.id === key.id ? null : now))
                  failureGoes()
                },
              }).keys,
          )
          .catch((why: unknown) => {
            if (stillOpen()) {
              sayFailure(`That key was not revoked: ${reasonOf(why)}`)
            }
          })
      }}
    >
      Revoke
    </button>
  )

  const browserOf = (session: SessionRow) =>
    session.user_agent ?? 'unknown browser'

  const thisOne = (session: SessionRow) =>
    session.current ? <strong> (this one)</strong> : null

  const lastSeen = (session: SessionRow) => `last seen ${session.last_seen}`

  const revokeSessionButton = (session: SessionRow) => (
    <button
      type="button"
      ref={revokeFocus.revokeRef('sessions', session.id)}
      className={look.button}
      disabled={session.current}
      onClick={() => {
        revokeFocus.pressed('sessions', session.id)
        const stillOpen = openSincePress()
        const failureGoes = failureGoesWithPress()
        void revokeSession(session.id)
          .then(() => refresh({ sessions: failureGoes }).sessions)
          .catch((why: unknown) => {
            if (stillOpen()) {
              sayFailure(`That session was not ended: ${reasonOf(why)}`)
            }
          })
      }}
    >
      Revoke
    </button>
  )

  const signOutButton = (
    <button
      type="button"
      className={look.signOut}
      onClick={() => {
        const stillOpen = openSincePress()
        void logout()
          .then(onSignedOut)
          .catch((why: unknown) => {
            // Said rather than swallowed: a sign-out that failed leaves
            // the reader signed in, and a button that does nothing
            // visible is the worst way to learn that.
            if (stillOpen())
              sayFailure(`You were not signed out: ${reasonOf(why)}`)
          })
      }}
    >
      Sign out
    </button>
  )

  if (props.placement === 'page') {
    // O163: the page's title is its h1, so its sections are h2s.
    return (
      <section className="account" aria-label="Account">
        <PageHeader level={1} title="Account" />
        {failureAlert}

        <Section level={2} title="Password">
          <form
            onSubmit={change}
            onBlur={leavingPasswordForm}
            className="settings-group"
          >
            {/* O175: each `aria-label` is its row label's words as
                written, so the name keeps their case whatever case a
                stylesheet draws the label in. */}
            <Row label="Current password">
              {(ids) => (
                <input
                  id={ids.control}
                  aria-label="Current password"
                  {...currentBox}
                />
              )}
            </Row>
            <Row label="New password">
              {(ids) => (
                <input
                  id={ids.control}
                  aria-label="New password"
                  {...nextBox}
                />
              )}
            </Row>
            <Row label="Confirm password">
              {(ids) => (
                <input
                  id={ids.control}
                  aria-label="Confirm password"
                  {...confirmBox}
                />
              )}
            </Row>
            {differLine}
            {passwordRefusalCallout}
            {/* At the box's foot, after the last field, where a form's
                submit is looked for; the row the line lies over, kept as
                tall as it needs (`DifferRoom`). */}
            <div className="settings-actions keeps-differ-room">
              <DifferRoom />
              {changeButton}
            </div>
          </form>
          {passwordRegion}
        </Section>

        <Section level={2} title="API keys" description={KEYS_ARE_FOR}>
          {address}
          <form
            onSubmit={create}
            onBlur={leavingKeyForm}
            className="settings-group key-form"
          >
            <Row label="New key">
              {(ids) => (
                <>
                  <input
                    id={ids.control}
                    aria-label="New key"
                    {...keyNameBox}
                  />
                  {createButton}
                </>
              )}
            </Row>
            {keyRefusalCallout}
            {keyRegions}
          </form>
          {secretBox}
          {keys.length === 0 ? null : (
            <ul className="settings-group">
              {keys.map((key) => (
                <li key={key.id}>
                  <Row label={key.name} help={keyUse(key)}>
                    {revokeKeyButton(key)}
                  </Row>
                </li>
              ))}
            </ul>
          )}
        </Section>

        <Section level={2} title="Signed in" titleId={signedInId}>
          {sessions.length === 0 ? null : (
            <ul className="settings-group">
              {sessions.map((session) => (
                <li key={session.id}>
                  <Row
                    label={
                      <>
                        {browserOf(session)}
                        {thisOne(session)}
                      </>
                    }
                    help={lastSeen(session)}
                  >
                    {revokeSessionButton(session)}
                  </Row>
                </li>
              ))}
            </ul>
          )}
          <div className="settings-actions">{signOutButton}</div>
        </Section>
      </section>
    )
  }

  // In the reader's dialog the title is an h2, so the sections are h3s.
  return (
    <div className="reader-dialog">
      <div
        className="reader-dialog-panel"
        role="dialog"
        aria-label="Account"
        ref={panelRef}
        tabIndex={-1}
        aria-modal="true"
      >
        <div className="reader-panel-head">
          <h2 className="reader-panel-title">Account</h2>
          <button
            type="button"
            className="reader-button"
            onClick={props.onClose}
          >
            Close
          </button>
        </div>

        {failureAlert}

        <section className={look.section}>
          <h3 className={look.heading}>Password</h3>
          <form onSubmit={change} onBlur={leavingPasswordForm}>
            {/* No class on the labels: a reader panel puts the word above
                the box as plain text, one short word each, where the page
                says Current password, New password and Confirm password.
                O175: each box carries its label's words as its `aria-label`
                all the same, as the page's do. */}
            <label>
              Current
              <input
                className={look.field}
                aria-label="Current"
                {...currentBox}
              />
            </label>
            <label>
              New
              <input className={look.field} aria-label="New" {...nextBox} />
            </label>
            <label>
              Confirm
              <input
                className={look.field}
                aria-label="Confirm"
                {...confirmBox}
              />
            </label>
            {differLine}
            {passwordRefusalCallout}
            {/* The row the line lies over, kept as tall as it needs
                (`DifferRoom`). */}
            <div className="keeps-differ-room">
              <DifferRoom />
              {changeButton}
            </div>
          </form>
          {passwordRegion}
        </section>

        <section className={look.section}>
          <h3 className={look.heading}>API keys</h3>
          <p className={look.meta}>{KEYS_ARE_FOR}</p>
          {address}
          <form onSubmit={create} onBlur={leavingKeyForm} className={look.row}>
            <input className={look.field} {...keyNameBox} />
            {createButton}
            {keyRegions}
          </form>
          {/* After the form, which lays its parts out in a row, so that the
              callout lies under the whole row. */}
          {keyRefusalCallout}
          {secretBox}
          {keys.map((key) => (
            <div key={key.id} className={look.row}>
              <span>
                {key.name}
                <span className={look.meta}> — {keyUse(key)}</span>
              </span>
              {revokeKeyButton(key)}
            </div>
          ))}
        </section>

        <section className={look.section}>
          <h3 className={look.heading} id={signedInId} tabIndex={-1}>
            Signed in
          </h3>
          {sessions.map((session) => (
            <div key={session.id} className={look.row}>
              <span>
                {browserOf(session)}
                <span className={look.meta}> — {lastSeen(session)}</span>
                {thisOne(session)}
              </span>
              {revokeSessionButton(session)}
            </div>
          ))}
          {signOutButton}
        </section>
      </div>
    </div>
  )
}
