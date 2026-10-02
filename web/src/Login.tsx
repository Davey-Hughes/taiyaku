import { useState, type CSSProperties, type FormEvent } from 'react'
import { login } from './api'
import { messageOf } from './message'
import { useDocumentTitle } from './title'
import { Wordmark } from './Wordmark'

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

// The sign-in screen takes the theme like every screen inside `.library`
// (spec 2026-09-17 §2): paper or ink by the reader's choice, the title face
// for the wordmark, `--danger` for a refusal. Its controls are drawn by
// `styles/base.css`, whose rules reach this screen through the `login` class
// on the overlay, so what stays inline here is layout — and the accent on the
// one button that submits.
//
// History worth keeping: this screen shipped a hardcoded dark palette (`#111`,
// `#ffb4a8`) when it only ever appeared over the reader, and handed a
// light-preference reader a dark screen once it fronted the library.
// `colourLiterals.test.ts` now refuses any colour literal or system colour
// the guard counts in this file — a colour named in words would pass it.
const overlayStyle: CSSProperties = {
  position: 'fixed',
  inset: 0,
  display: 'flex',
  alignItems: 'center',
  justifyContent: 'center',
  background: 'var(--bg)',
  zIndex: 30,
}

const panelStyle: CSSProperties = {
  width: 'min(360px, 90vw)',
  padding: '32px 28px',
  borderTop: '1px solid var(--line)',
  borderBottom: '1px solid var(--line)',
}

// The wordmark draws itself (`Wordmark.tsx`, `styles/shell.css`); the
// heading around it drops its margin, and since O163 made it the screen's
// h1, holds the h2 size `base.css` gave it before.
const headingStyle: CSSProperties = {
  margin: 0,
  fontSize: 'var(--text-h2)',
}

const fieldStyle: CSSProperties = {
  display: 'block',
  boxSizing: 'border-box',
  width: '100%',
  marginTop: 6,
  padding: '8px 10px',
}

const labelStyle: CSSProperties = {
  display: 'block',
  marginTop: 16,
  fontSize: 'var(--text-ui)',
}

const errorStyle: CSSProperties = {
  marginTop: 16,
  color: 'var(--danger)',
  fontSize: 'var(--text-ui)',
}

const submitStyle: CSSProperties = {
  ...fieldStyle,
  marginTop: 24,
  borderColor: 'var(--accent)',
  background: 'var(--accent)',
  color: 'var(--on-accent)',
}

export interface LoginProps {
  /** Called once the server has accepted, so `Gate` can ask `me()` again. */
  onSignedIn: () => void
}

export function Login({ onSignedIn }: LoginProps) {
  useDocumentTitle('Sign in')
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
        setFailure(messageOf(why))
      })
      .finally(() => {
        setBusy(false)
      })
  }

  return (
    <main className="login" style={overlayStyle}>
      <form style={panelStyle} onSubmit={submit}>
        <h1 style={headingStyle}>
          <Wordmark />
        </h1>
        <label style={labelStyle}>
          Name
          {/*
            **`autoCapitalize` and `autoCorrect`, because an account name is
            compared byte for byte.** `user.name` is `TEXT NOT NULL UNIQUE`
            with no `COLLATE NOCASE`, so SQLite matches it under the default
            BINARY collation and `account::by_name`'s `WHERE name = ?1` finds
            nothing for `Local` when the account is `local` — verified
            against a real library, 1 row against 0.

            iOS defaults a text input to `autocapitalize="sentences"`, which
            capitalises the first letter of an empty field. So every iPhone
            typed a name that could not match, on the one screen a reader
            cannot get past. Reported from a device pass, 2026-09-23.
          */}
          <input
            style={fieldStyle}
            value={name}
            autoComplete="username"
            autoCapitalize="none"
            autoCorrect="off"
            spellCheck={false}
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
        <button type="submit" style={submitStyle} disabled={busy}>
          {busy ? 'Signing in…' : 'Sign in'}
        </button>
      </form>
    </main>
  )
}
