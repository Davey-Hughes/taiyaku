/**
 * The app's routes, and the one place `App` is mounted.
 *
 * Spec C1 §1. `react-router` is the first runtime dependency beyond React
 * since the project began, taken over a hand-rolled fifty-line router because
 * scroll restoration, nested routes and navigation blocking are all things
 * this UI wants and all things that are tedious to get right twice.
 *
 * **Every route here is below `Gate.tsx`**, which `main.tsx` wraps around this
 * component — so nothing in this file has to think about a 401, and a
 * signed-out visitor never reaches any of them. That was not true when this
 * file first shipped: sign-in lived inside `App`, `App` mounts on `/read/:work`
 * alone, and `/` therefore had no way to sign in at all.
 *
 * **`key={work}` is the whole reason `App.tsx` did not have to become
 * re-entrant.** A changing key unmounts and remounts, so that file's "loads
 * exactly once" assumption stays true rather than becoming a lie maintained by
 * hand. The wasm module is a module-level ES import, so it initialises once per
 * *page* and a remount costs a sidecar fetch and a resolver construction.
 */
import { Navigate, Route, Routes, useNavigate, useParams } from 'react-router'
import App from './App'
import { Config } from './Config'
import { Library } from './Library'
import { Proposals } from './Proposals'
import { Search } from './Search'
import { Series } from './Series'
import { Shell } from './Shell'

/** The reader, keyed by the slug in the URL. */
function Reader() {
  const { work } = useParams()
  const navigate = useNavigate()
  if (!work) return <Navigate to="/" replace />
  return <App key={work} work={work} onLeave={() => void navigate('/')} />
}

export function AppRoutes() {
  return (
    <Routes>
      <Route
        path="/"
        element={
          <Shell>
            <Library />
          </Shell>
        }
      />
      <Route
        path="/series/:id"
        element={
          <Shell>
            <Series />
          </Shell>
        }
      />
      <Route
        path="/search"
        element={
          <Shell>
            <Search />
          </Shell>
        }
      />
      <Route
        path="/proposals"
        element={
          <Shell>
            <Proposals />
          </Shell>
        }
      />
      {/*
        A splat, because `Config` declares its own child routes from `TABS` —
        see that file's own doc comment for why the list and the routes it
        implies live together. Every other route here is a leaf.

        No admin guard, like `/proposals` above: a non-admin who types this
        reaches it and is shown the server's own 403 sentence, which is what
        `api.ts`'s `getJson` was changed during C3 to make possible.
      */}
      <Route
        path="/config/*"
        element={
          <Shell>
            <Config />
          </Shell>
        }
      />
      <Route path="/read/:work" element={<Reader />} />
      <Route path="*" element={<Navigate to="/" replace />} />
    </Routes>
  )
}
