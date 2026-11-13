import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { BrowserRouter } from 'react-router'
import './index.css'
import { Gate } from './Gate.tsx'
import { AppRoutes } from './Routes.tsx'
import { applyTheme, loadTheme } from './theme.ts'

// Again here, after the pre-paint script `/theme.js` has done it: should that
// file fail to load, what it costs is a flash of the OS's theme, not the
// wrong theme for the whole session.
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
