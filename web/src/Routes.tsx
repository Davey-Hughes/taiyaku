/**
 * The four routes, and the one place `App` is mounted.
 *
 * Spec C1 §1. `react-router` is the first runtime dependency beyond React
 * since the project began, taken over a hand-rolled fifty-line router because
 * scroll restoration, nested routes and navigation blocking are all things
 * this UI wants and all things that are tedious to get right twice.
 *
 * **Every route here is below `Gate.tsx`**, which `main.tsx` wraps around this
 * component — so nothing in this file has to think about a 401, and a
 * signed-out visitor never reaches any of the four. That was not true when this
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
import { Library } from './Library'

/** The reader, keyed by the slug in the URL. */
function Reader() {
  const { work } = useParams()
  const navigate = useNavigate()
  if (!work) return <Navigate to="/" replace />
  return <App key={work} work={work} onLeave={() => void navigate('/')} />
}

/** Plan B builds these two. Named rather than blank, so the seam is visible. */
function NotBuiltYet({ what }: { what: string }) {
  return (
    <main>
      <h1>{what}</h1>
      <p>Not built yet.</p>
    </main>
  )
}

export function AppRoutes() {
  return (
    <Routes>
      <Route path="/" element={<Library />} />
      <Route path="/series/:id" element={<NotBuiltYet what="Series" />} />
      <Route path="/search" element={<NotBuiltYet what="Search" />} />
      <Route path="/read/:work" element={<Reader />} />
      <Route path="*" element={<Navigate to="/" replace />} />
    </Routes>
  )
}
