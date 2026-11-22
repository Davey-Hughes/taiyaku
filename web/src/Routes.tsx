/**
 * The app's routes, and the one place `App` is mounted.
 *
 * Spec C1 §1. `react-router` is taken over a hand-rolled router because
 * scroll restoration, nested routes and navigation blocking are all things
 * this UI wants and all things that are tedious to get right twice.
 *
 * **Every route here is below `Gate.tsx`**, which `main.tsx` wraps around this
 * component — so nothing in this file has to think about a 401, and a
 * signed-out visitor never reaches any of them.
 *
 * **`key={work}` is the whole reason `App.tsx` did not have to become
 * re-entrant.** A changing key unmounts and remounts, so that file's "loads
 * exactly once" assumption stays true rather than becoming a lie maintained by
 * hand. The wasm module is a module-level ES import, so it initialises once per
 * *page* and a remount costs a sidecar fetch and a resolver construction.
 */
import { Navigate, Route, Routes, useParams } from 'react-router'
import { About } from './About'
import { Account } from './Account'
import App from './App'
import { useBack, useBackTracker } from './back'
import { BookReader } from './BookReader'
import { Config } from './Config'
import { useGate } from './gate'
import { Library } from './Library'
import { Licence } from './Licence'
import { Proposals } from './Proposals'
import { Search } from './Search'
import { Series } from './Series'
import { Shell } from './Shell'
import { useDocumentTitle } from './title'

/**
 * The reader, keyed by the slug in the URL. Its exits (the top bar's button,
 * its key, and the failed load's way back) are `back.ts`'s way back, and
 * the button and the failed load name where it goes.
 */
function Reader() {
  const { work, leave, leaveTo } = useReaderRoute()
  if (!work) return <Navigate to="/" replace />
  // `.reader-route` blacks out the page from the first frame (spec 2026-09-17
  // §8.2): `App` draws nothing until its load answers, so its own `.reader`
  // root is not there for `styles/reader.css`'s ground rule until then.
  return (
    <div className="reader-route">
      <App key={work} work={work} onLeave={leave} leaveTo={leaveTo} />
    </div>
  )
}

/** A book, keyed by the slug in the URL, leaving as the reader leaves. */
function BookRoute() {
  const { work, leave, leaveTo } = useReaderRoute()
  if (!work) return <Navigate to="/" replace />
  return <BookReader key={work} work={work} onLeave={leave} leaveTo={leaveTo} />
}

/** A reader's slug from the URL, its way back, and that way's name. */
function useReaderRoute() {
  const { work } = useParams()
  const { word, phrase, go } = useBack()
  return { work, leave: go, leaveTo: { word, phrase } }
}

/**
 * The account screen at a URL of its own — O94.
 *
 * Without a route of its own, `Account` would mount only inside `App`, behind
 * `A`: the one screen where an account changes its own password could be
 * reached only by opening a work, and not at all in a library with no work to
 * open — while `AccountPage` refuses an administrator's reset of their own
 * password and has to send them somewhere. One component draws both; here it
 * is a page in `Shell`'s `<main>` rather than an overlay over a reader.
 *
 * Sign-out is the gate's, exactly as it is for the reader's panel. The `?.` is
 * the no-gate case, which only a test mounting these routes on their own meets.
 */
function AccountScreen() {
  const gate = useGate()
  useDocumentTitle('Account')
  return <Account placement="page" onSignedOut={() => gate?.signOut()} />
}

/** About at a URL of its own, reached from the build's label above More. */
function AboutScreen() {
  useDocumentTitle('About')
  return <About />
}

/** The licence page, beneath About. */
function LicenceScreen() {
  useDocumentTitle('Licence')
  return <Licence />
}

export function AppRoutes() {
  useBackTracker()
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
      {/* A redirect to the Library view's search bar, so no shell of its own. */}
      <Route path="/search" element={<Search />} />
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
        `api.ts`'s `getJson` makes possible.
      */}
      <Route
        path="/config/*"
        element={
          <Shell>
            <Config />
          </Shell>
        }
      />
      {/*
        O94. Every signed-in account's rather than an administrator's, so the
        sidebar draws its link for everyone: `Account`'s own doc comment says
        no route it calls names another account. It needs no work in the
        library, which is the case the reader's panel could never reach.
      */}
      <Route
        path="/account"
        element={
          <Shell>
            <AccountScreen />
          </Shell>
        }
      />
      {/* Every account's, like `/account`: the label that leads here is drawn
        for everyone. */}
      <Route
        path="/about"
        element={
          <Shell>
            <AboutScreen />
          </Shell>
        }
      />
      <Route
        path="/about/licence"
        element={
          <Shell>
            <LicenceScreen />
          </Shell>
        }
      />
      <Route path="/read/:work" element={<Reader />} />
      <Route path="/book/:work" element={<BookRoute />} />
      <Route path="*" element={<Navigate to="/" replace />} />
    </Routes>
  )
}
