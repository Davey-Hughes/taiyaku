import {
  useCallback,
  useEffect,
  useState,
  type CSSProperties,
  type FormEvent,
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

/**
 * The account screen: change a password, hold API keys, see what is signed in.
 *
 * **No user administration** (§9): creating accounts, granting roots and
 * setting capabilities are spec C4's configuration UI, and the CLI covers them
 * in between. Everything here is about *this* account, which is why no route it
 * calls names another one.
 */

const overlayStyle: CSSProperties = {
  position: 'fixed',
  inset: 0,
  display: 'flex',
  alignItems: 'center',
  justifyContent: 'center',
  background: 'rgba(0, 0, 0, 0.6)',
  zIndex: 20,
}

const panelStyle: CSSProperties = {
  background: 'rgba(24, 24, 24, 0.96)',
  color: 'rgba(255, 255, 255, 0.92)',
  borderRadius: 12,
  boxShadow: '0 10px 30px rgba(0, 0, 0, 0.45)',
  padding: '20px 24px',
  width: 'min(560px, 92vw)',
  maxHeight: '80vh',
  overflowY: 'auto',
}

const sectionStyle: CSSProperties = {
  marginTop: 16,
  paddingTop: 16,
  borderTop: '1px solid rgba(255, 255, 255, 0.15)',
}

const fieldStyle: CSSProperties = {
  display: 'block',
  width: '100%',
  marginTop: 6,
  padding: '6px 8px',
  borderRadius: 6,
  border: '1px solid rgba(255, 255, 255, 0.2)',
  background: 'rgba(0, 0, 0, 0.3)',
  color: 'inherit',
  font: 'inherit',
}

const secretStyle: CSSProperties = {
  marginTop: 10,
  padding: '10px 12px',
  borderRadius: 6,
  background: 'rgba(255, 255, 255, 0.08)',
  fontFamily: 'ui-monospace, monospace',
  wordBreak: 'break-all',
}

const rowStyle: CSSProperties = {
  display: 'flex',
  alignItems: 'baseline',
  justifyContent: 'space-between',
  gap: 12,
  marginTop: 8,
}

export interface AccountProps {
  visible: boolean
  onClose: () => void
  /** Called after signing out, so `App` can show the login screen. */
  onSignedOut: () => void
}

export function Account({ visible, onClose, onSignedOut }: AccountProps) {
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
        setFailure(why instanceof Error ? why.message : String(why))
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
        setFailure(why instanceof Error ? why.message : String(why))
      })
      .finally(() => {
        setBusy(false)
      })
  }

  return (
    <div style={overlayStyle}>
      <div style={panelStyle} role="dialog" aria-label="Account">
        <div style={rowStyle}>
          <h2 style={{ margin: 0, fontSize: '1.1em' }}>Account</h2>
          <button type="button" onClick={onClose}>
            Close
          </button>
        </div>

        {failure === null ? null : (
          <p role="alert" style={{ color: '#ffb4a8' }}>
            {failure}
          </p>
        )}
        {note === null ? null : <p style={{ color: '#a8ffb4' }}>{note}</p>}

        <section style={sectionStyle}>
          <h3 style={{ margin: 0, fontSize: '1em' }}>Password</h3>
          <form onSubmit={change}>
            <label>
              Current
              <input
                style={fieldStyle}
                type="password"
                autoComplete="current-password"
                value={current}
                onChange={(e) => {
                  setCurrent(e.target.value)
                }}
              />
            </label>
            <label>
              New
              <input
                style={fieldStyle}
                type="password"
                autoComplete="new-password"
                value={next}
                onChange={(e) => {
                  setNext(e.target.value)
                }}
              />
            </label>
            <button
              type="submit"
              style={{ ...fieldStyle, cursor: 'pointer' }}
              disabled={busy}
            >
              Change password
            </button>
          </form>
        </section>

        <section style={sectionStyle}>
          <h3 style={{ margin: 0, fontSize: '1em' }}>API keys</h3>
          <p style={{ fontSize: '0.9em', opacity: 0.8 }}>
            For OPDS readers like Panels or Chunky: give the key as the
            password, with your account name as the username.
          </p>
          <form onSubmit={create} style={rowStyle}>
            <input
              style={fieldStyle}
              placeholder="Which device is this for?"
              value={keyName}
              onChange={(e) => {
                setKeyName(e.target.value)
              }}
            />
            <button type="submit" disabled={busy}>
              Create
            </button>
          </form>
          {freshSecret === null ? null : (
            <div style={secretStyle}>
              <div>{freshSecret}</div>
              <p
                style={{ margin: '8px 0 0', fontSize: '0.85em', opacity: 0.85 }}
              >
                Copy this now — it is not stored and cannot be shown again.
              </p>
            </div>
          )}
          {keys.map((key) => (
            <div key={key.id} style={rowStyle}>
              <span>
                {key.name}
                <span style={{ opacity: 0.6 }}>
                  {' '}
                  —{' '}
                  {key.last_used === null
                    ? 'never used'
                    : `last used ${key.last_used}`}
                </span>
              </span>
              <button
                type="button"
                onClick={() => {
                  // Said rather than swallowed, and `refresh` only on success:
                  // a revocation that did not happen looked exactly like one
                  // that did — the row stayed, no alert appeared, and a reader
                  // believed the lost device's key was dead when it still
                  // worked. Found by the third whole-branch review.
                  void revokeKey(key.id)
                    .then(refresh)
                    .catch((why: unknown) => {
                      setFailure(
                        why instanceof Error
                          ? `That key was not revoked: ${why.message}`
                          : String(why),
                      )
                    })
                }}
              >
                Revoke
              </button>
            </div>
          ))}
        </section>

        <section style={sectionStyle}>
          <h3 style={{ margin: 0, fontSize: '1em' }}>Signed in</h3>
          {sessions.map((session) => (
            <div key={session.id} style={rowStyle}>
              <span>
                {session.user_agent ?? 'unknown browser'}
                <span style={{ opacity: 0.6 }}>
                  {' '}
                  — last seen {session.last_seen}
                </span>
                {session.current ? <strong> (this one)</strong> : null}
              </span>
              <button
                type="button"
                disabled={session.current}
                onClick={() => {
                  void revokeSession(session.id)
                    .then(refresh)
                    .catch((why: unknown) => {
                      setFailure(
                        why instanceof Error
                          ? `That session was not ended: ${why.message}`
                          : String(why),
                      )
                    })
                }}
              >
                Revoke
              </button>
            </div>
          ))}
          <button
            type="button"
            style={{ ...fieldStyle, marginTop: 14, cursor: 'pointer' }}
            onClick={() => {
              void logout()
                .then(onSignedOut)
                .catch((why: unknown) => {
                  // Said rather than swallowed: a sign-out that failed leaves
                  // the reader signed in, and a button that does nothing
                  // visible is the worst way to learn that.
                  setFailure(why instanceof Error ? why.message : String(why))
                })
            }}
          >
            Sign out
          </button>
        </section>
      </div>
    </div>
  )
}
