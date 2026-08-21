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

// This screen used to be reachable only from inside the reader, which is
// deliberately dark because it draws over artwork — so a fixed dark palette
// was correct here too. It now fronts the library instead (`Gate.tsx`), which
// follows `color-scheme: light dark` via CSS system colours, so a hardcoded
// dark palette here would hand a light-preference reader a dark login screen
// immediately ahead of a light library. These styles follow suit, the same
// way `index.css`'s library rules do: `Canvas`/`CanvasText` for the backdrop
// and card text, `ButtonFace`/`ButtonBorder` for the card and field surfaces.
// (Not `--box-*` — that palette is fixed on purpose, for artwork this screen
// never draws over.)
const overlayStyle: CSSProperties = {
  position: 'fixed',
  inset: 0,
  display: 'flex',
  alignItems: 'center',
  justifyContent: 'center',
  background: 'Canvas',
  zIndex: 30,
}

const panelStyle: CSSProperties = {
  background: 'ButtonFace',
  color: 'CanvasText',
  border: '1px solid ButtonBorder',
  borderRadius: 12,
  // Tuned to read as a soft lift rather than a hard edge in both schemes —
  // the original `0 10px 30px rgba(0, 0, 0, 0.45)` was sized for a dark
  // backdrop and reads as a bruise on a white one.
  boxShadow: '0 8px 24px rgba(0, 0, 0, 0.2)',
  padding: '24px 28px',
  width: 'min(360px, 90vw)',
}

const fieldStyle: CSSProperties = {
  display: 'block',
  width: '100%',
  marginTop: 6,
  padding: '8px 10px',
  borderRadius: 6,
  // `rgba(255, 255, 255, 0.2)` was invisible on a light background; a system
  // colour gives the field a boundary that holds in both schemes, the same
  // way `.sidebar`'s and `.chips button`'s borders do in `index.css`.
  border: '1px solid ButtonBorder',
  background: 'Canvas',
  color: 'inherit',
  font: 'inherit',
}

const labelStyle: CSSProperties = { display: 'block', marginTop: 14 }

// There is no system colour for "error". `#ffb4a8` was tuned for the old
// dark-only backdrop (11.01:1 against `rgb(18, 18, 18)`) but falls to 1.70:1
// on white — nowhere near WCAG AA's 4.5:1. The reverse is also true: a red
// dark enough to clear AA on white (e.g. `#b00020`, 7.33:1) clears only
// 2.56:1 on `rgb(18, 18, 18)`. The two requirements don't overlap for any
// single fixed colour — the luminance a colour needs to read on white
// (<=0.183) and the luminance it needs to read on near-black (>=0.202) are
// disjoint ranges, not a coincidence of these two particular reds. `light-
// dark()` picks between two literals using the same `color-scheme: light
// dark` signal that resolves the system colours above, so each one only ever
// has to clear AA against the canvas it is actually shown on.
const errorStyle: CSSProperties = {
  marginTop: 14,
  color: 'light-dark(#b00020, #ffb4a8)',
  fontSize: '0.9em',
}

export interface LoginProps {
  /** Called once the server has accepted, so `Gate` can ask `me()` again. */
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
