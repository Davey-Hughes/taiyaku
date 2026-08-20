import { useState, type CSSProperties, type FormEvent } from 'react'
import { login } from './api'

/**
 * The sign-in screen.
 *
 * **It owns the whole viewport rather than sitting over the reader**, which is
 * the one place this differs from `Settings` and `Help`. Those are opened
 * deliberately and dismissed back to a page that is still there; this is shown
 * because there is no page to go back to yet, and drawing it as a modal over an
 * empty reader would suggest there was something underneath.
 *
 * No `Secure`/cookie handling here and nothing that touches a token: the
 * session cookie is `HttpOnly`, so this component cannot read it and does not
 * try. What it knows is whether the server accepted the credentials.
 */

const overlayStyle: CSSProperties = {
  position: 'fixed',
  inset: 0,
  display: 'flex',
  alignItems: 'center',
  justifyContent: 'center',
  background: '#111',
  zIndex: 30,
}

const panelStyle: CSSProperties = {
  background: 'rgba(24, 24, 24, 0.96)',
  color: 'rgba(255, 255, 255, 0.92)',
  borderRadius: 12,
  boxShadow: '0 10px 30px rgba(0, 0, 0, 0.45)',
  padding: '24px 28px',
  width: 'min(360px, 90vw)',
}

const fieldStyle: CSSProperties = {
  display: 'block',
  width: '100%',
  marginTop: 6,
  padding: '8px 10px',
  borderRadius: 6,
  border: '1px solid rgba(255, 255, 255, 0.2)',
  background: 'rgba(0, 0, 0, 0.3)',
  color: 'inherit',
  font: 'inherit',
}

const labelStyle: CSSProperties = { display: 'block', marginTop: 14 }

const errorStyle: CSSProperties = {
  marginTop: 14,
  color: '#ffb4a8',
  fontSize: '0.9em',
}

export interface LoginProps {
  /** Called once the server has accepted, so `App` can load the library. */
  onSignedIn: () => void
}

export function Login({ onSignedIn }: LoginProps) {
  const [name, setName] = useState('')
  const [password, setPassword] = useState('')
  const [failure, setFailure] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)

  const submit = (event: FormEvent) => {
    event.preventDefault()
    setBusy(true)
    // The previous failure is cleared on submit rather than on typing: a
    // message that vanishes as soon as a reader touches the field is gone
    // before they have finished reading it.
    setFailure(null)
    login(name, password)
      .then(() => {
        onSignedIn()
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
      <form style={panelStyle} onSubmit={submit}>
        <h2 style={{ margin: 0, fontSize: '1.1em' }}>taiyaku</h2>
        <label style={labelStyle}>
          Name
          <input
            style={fieldStyle}
            value={name}
            autoComplete="username"
            onChange={(e) => {
              setName(e.target.value)
            }}
          />
        </label>
        <label style={labelStyle}>
          Password
          <input
            style={fieldStyle}
            type="password"
            value={password}
            autoComplete="current-password"
            onChange={(e) => {
              setPassword(e.target.value)
            }}
          />
        </label>
        {failure === null ? null : (
          <p style={errorStyle} role="alert">
            {failure}
          </p>
        )}
        <button
          type="submit"
          style={{ ...fieldStyle, marginTop: 18, cursor: 'pointer' }}
          disabled={busy}
        >
          {busy ? 'Signing in…' : 'Sign in'}
        </button>
      </form>
    </div>
  )
}
