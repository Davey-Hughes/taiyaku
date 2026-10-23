import { useCallback, useEffect, useRef, useState, type FormEvent } from 'react'
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
import { CatalogAddress } from './CatalogAddress'
import { usePanelFocus } from './focus'
import { messageOf } from './message'

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
 */

/** The class each placement paints a part with. */
interface Look {
  section?: string
  /** On the `<label>`. The page uses `.field`; the panel wants none. */
  label?: string
  field?: string
  wide?: string
  signOut?: string
  /**
   * Beside the `secret` the call site writes for both placements, not in
   * place of it — `admin.css`'s `.account .secret` and `reader.css`'s
   * `.reader-secret` are two boxes of the same idea, and the page's is
   * reached by the literal alone. The page therefore sets none: `secret:
   * 'secret'` here concatenated to `class="secret secret"`, and collapsing
   * the literal instead would take `.secret` off the panel, which
   * `admin.css` paints the page's copy with.
   */
  secret?: string
  failure?: string
  note?: string
  row?: string
  meta?: string
  button?: string
  heading?: string
}

/**
 * **Two placements, two vocabularies, one component.**
 *
 * The panel is one of the reader's dialogs and takes ③'s classes; the page
 * is a settings screen inside `.library` and takes `admin.css`'s. This map
 * was a map of `CSSProperties` until ⑤c, and the panel's half of it was 25
 * inline `style=` props and 10 colour literals drawing a dialog that matched
 * none of the four beside it.
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
    // No label class: a reader panel puts the word above the box as plain
    // text. `.field`'s uppercase micro-label is the settings area's idiom,
    // and the page takes it in Task 6.
    field: 'reader-field reader-field-block',
    wide: 'reader-button reader-button-wide',
    signOut: 'reader-button reader-button-wide',
    secret: 'reader-secret',
    failure: 'reader-failure',
    note: 'reader-success',
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
    section: 'account-card',
    heading: 'account-heading',
    label: 'field',
    wide: 'account-wide',
    signOut: 'account-wide',
    failure: 'account-failure',
    note: 'account-note',
    row: 'account-row',
    meta: 'why',
  },
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
  /**
   * The one secret this screen will ever hold.
   *
   * Kept in component state and nowhere else — not localStorage, not a URL —
   * because the server stores only its hash and cannot answer it again. It goes
   * when the panel closes, which the copy beside it says plainly rather than
   * leaving a reader to find out.
   */
  const [freshSecret, setFreshSecret] = useState<string | null>(null)
  const [current, setCurrent] = useState('')
  const [next, setNext] = useState('')
  const [note, setNote] = useState<string | null>(null)
  const [failure, setFailure] = useState<string | null>(null)
  /* **Neither form disabled while in flight**, so a double-click on Create
  /// minted two permanent API keys and showed only the second secret — the
  /// first existed, was never displayed, and could only be found by listing
  /// keys and guessing which of two identically-named rows it was. Found by the
   second whole-branch review. */
  const [busy, setBusy] = useState(false)
  /** The OPDS catalog's path, once `/api/about` has answered with it. */
  const [catalog, setCatalog] = useState<string | undefined>()

  /**
   * Re-read both lists.
   *
   * One function for the effect *and* every action that changes them, rather
   * than the effect carrying its own copy: two copies of "reload the panel" is
   * how one of them comes to reload only half of it. A failure leaves the last
   * good list on screen — a reader whose keys did not reload can still change a
   * password, and an empty list would claim they have none.
   */
  const refresh = useCallback(() => {
    void listKeys()
      .then(setKeys)
      .catch(() => undefined)
    void listSessions()
      .then(setSessions)
      .catch(() => undefined)
  }, [])

  useEffect(() => {
    if (visible) {
      refresh()
      return
    }
    // **Closing is not unmounting.** `App` renders this element
    // unconditionally, so `return null` below hides it while React keeps every
    // hook's state — and `freshSecret` held the one-time API key secret, which
    // was redisplayed to anyone who reopened the panel for the rest of the
    // page's life. The doc comment above claimed it "goes when the panel
    // closes" and the copy beside it says the key "cannot be shown again";
    // both were false. The typed passwords survived a failed attempt the same
    // way. Found by the whole-branch review, 2026-08-18.
    setFreshSecret(null)
    setCurrent('')
    setNext('')
    setKeyName('')
    setNote(null)
    setFailure(null)
  }, [visible, refresh])

  useEffect(() => {
    if (!visible) return
    let cancelled = false
    // A failure draws the OPDS line without the address rather than an
    // alert: the address helps beside that line, and this screen's alerts
    // are for what the reader did.
    about()
      .then((answer) => {
        if (!cancelled) setCatalog(answer.catalog)
      })
      .catch(() => undefined)
    return () => {
      cancelled = true
    }
  }, [visible])

  if (!visible) return null

  const create = (event: FormEvent) => {
    event.preventDefault()
    if (busy) return
    setBusy(true)
    setFailure(null)
    createKey(keyName)
      .then((secret) => {
        setFreshSecret(secret)
        setKeyName('')
        refresh()
      })
      .catch((why: unknown) => {
        setFailure(messageOf(why))
      })
      .finally(() => {
        setBusy(false)
      })
  }

  const change = (event: FormEvent) => {
    event.preventDefault()
    if (busy) return
    setBusy(true)
    setFailure(null)
    setNote(null)
    changePassword(current, next)
      .then(() => {
        setCurrent('')
        setNext('')
        setNote(
          'Password changed. Every other signed-in browser has been signed out; API keys still work.',
        )
        refresh()
      })
      .catch((why: unknown) => {
        setFailure(messageOf(why))
      })
      .finally(() => {
        setBusy(false)
      })
  }

  // O163: the page's title is its h1, so its sections are h2s; in the
  // reader's dialog the title is an h2 and they stay h3s.
  const SectionHeading = props.placement === 'page' ? 'h2' : 'h3'

  // Everything under the heading is the same in both placements. What frames
  // it differs, and so do its colours — see `LOOKS`.
  const sections = (
    <>
      {failure === null ? null : (
        <p role="alert" className={look.failure}>
          {failure}
        </p>
      )}
      {/* Drawn with the screen, empty, so that the note is read when it
          lands in it: a live region drawn together with what it says is
          often not read at all. A bare `<div>`, so the note's margins
          collapse through it and an empty one takes no room. */}
      <div aria-live="polite">
        {note === null ? null : <p className={look.note}>{note}</p>}
      </div>

      <section className={look.section}>
        <SectionHeading className={look.heading}>Password</SectionHeading>
        <form onSubmit={change}>
          {/* O175: each `aria-label` repeats its label's words because the
              page's `.field` draws them in capitals, and Chromium's
              accessibility tree named the box in those capitals. The panel
              draws them as written and carries the same attribute. */}
          <label className={look.label}>
            Current
            <input
              className={look.field}
              type="password"
              aria-label="Current"
              autoComplete="current-password"
              value={current}
              onChange={(e) => {
                setCurrent(e.target.value)
              }}
            />
          </label>
          <label className={look.label}>
            New
            <input
              className={look.field}
              type="password"
              aria-label="New"
              autoComplete="new-password"
              value={next}
              onChange={(e) => {
                setNext(e.target.value)
              }}
            />
          </label>
          <button type="submit" className={look.wide} disabled={busy}>
            Change password
          </button>
        </form>
      </section>

      <section className={look.section}>
        <SectionHeading className={look.heading}>API keys</SectionHeading>
        <p className={look.meta}>
          For OPDS readers like Panels or Chunky: give the key as the password,
          with your account name as the username.
        </p>
        {catalog === undefined ? null : (
          <CatalogAddress catalog={catalog} className={look.button} />
        )}
        <form onSubmit={create} className={look.row}>
          <input
            className={look.field}
            placeholder="Which device is this for?"
            value={keyName}
            onChange={(e) => {
              setKeyName(e.target.value)
            }}
          />
          <button type="submit" className={look.button} disabled={busy}>
            Create
          </button>
        </form>
        {freshSecret === null ? null : (
          <div className={`secret ${look.secret ?? ''}`.trim()}>
            <div>{freshSecret}</div>
            <p className={look.meta}>
              Copy this now — it is not stored and cannot be shown again.
            </p>
          </div>
        )}
        {keys.map((key) => (
          <div key={key.id} className={look.row}>
            <span>
              {key.name}
              <span className={look.meta}>
                {' '}
                —{' '}
                {key.last_used === null
                  ? 'never used'
                  : `last used ${key.last_used}`}
              </span>
            </span>
            <button
              type="button"
              className={look.button}
              onClick={() => {
                // Said rather than swallowed, and `refresh` only on success:
                // a revocation that did not happen looked exactly like one
                // that did — the row stayed, no alert appeared, and a reader
                // believed the lost device's key was dead when it still
                // worked. Found by the third whole-branch review.
                void revokeKey(key.id)
                  .then(refresh)
                  .catch((why: unknown) => {
                    setFailure(`That key was not revoked: ${messageOf(why)}`)
                  })
              }}
            >
              Revoke
            </button>
          </div>
        ))}
      </section>

      <section className={look.section}>
        <SectionHeading className={look.heading}>Signed in</SectionHeading>
        {sessions.map((session) => (
          <div key={session.id} className={look.row}>
            <span>
              {session.user_agent ?? 'unknown browser'}
              <span className={look.meta}>
                {' '}
                — last seen {session.last_seen}
              </span>
              {session.current ? <strong> (this one)</strong> : null}
            </span>
            <button
              type="button"
              className={look.button}
              disabled={session.current}
              onClick={() => {
                void revokeSession(session.id)
                  .then(refresh)
                  .catch((why: unknown) => {
                    setFailure(`That session was not ended: ${messageOf(why)}`)
                  })
              }}
            >
              Revoke
            </button>
          </div>
        ))}
        <button
          type="button"
          className={look.signOut}
          onClick={() => {
            void logout()
              .then(onSignedOut)
              .catch((why: unknown) => {
                // Said rather than swallowed: a sign-out that failed leaves
                // the reader signed in, and a button that does nothing
                // visible is the worst way to learn that.
                setFailure(messageOf(why))
              })
          }}
        >
          Sign out
        </button>
      </section>
    </>
  )

  if (props.placement === 'page') {
    return (
      <section className="account" aria-label="Account">
        <h1>Account</h1>
        {sections}
      </section>
    )
  }

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

        {sections}
      </div>
    </div>
  )
}
