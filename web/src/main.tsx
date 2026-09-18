import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { BrowserRouter } from 'react-router'
import './index.css'
import { Gate } from './Gate.tsx'
import { AppRoutes } from './Routes.tsx'
import { applyTheme, loadTheme } from './theme.ts'

// Again here, after `index.html`'s pre-paint script has done it: a
// Content-Security-Policy that refuses inline scripts would stop that one and
// not this, and a flash of the OS's theme is a better failure than the wrong
// theme for the whole session.
applyTheme(loadTheme(), document.documentElement)

// `Gate` inside `BrowserRouter` and outside `AppRoutes`: the router stays
// mounted across a sign-in, so the URL a signed-out visitor arrived on is the
// one they land on once they have signed in — and every route is under one
// registration of `api.ts`'s `whenSignedOut`, which is a module-level slot that
// the last registration silently wins.
createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <BrowserRouter>
      <Gate>
        <AppRoutes />
      </Gate>
    </BrowserRouter>
  </StrictMode>,
)
