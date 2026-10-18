/**
 * The settings area — spec C4, and the build design's §2.
 *
 * **One sidebar entry and a sub-nav, not a sidebar entry per tab.** C4's
 * opening line settles the shape: configuration is "touched rarely and from a
 * settings area", so the always-visible chrome grows by one. An entry per tab
 * would also have put a second "Libraries" in a sidebar that already has one
 * meaning something else — the libraries to browse, not an administrative
 * screen.
 *
 * **No admin guard here, deliberately.** `Routes.tsx` puts none on
 * `/proposals` either, and `api.ts`'s `getJson` was changed during C3
 * precisely so a non-admin who types that URL is shown *the server's own
 * sentence* rather than a bare status. A `<RequireAdmin>` wrapper would put a
 * second opinion about who may administer into the client, months after O93
 * closed by making `session::administering_or_forbidden` the only function in
 * `taiyaku-serve` that holds one. A client that draws a screen it should not
 * is cosmetic; a client that *decides* is the shape O93 was.
 *
 * **The child routes are declared here rather than in `Routes.tsx`.** `TABS`
 * is the list, and a tab whose route lived in another file could exist in the
 * sub-nav with nothing behind it — which is exactly the drift keeping the list
 * in one place is for. `Config.test.tsx` drives every tab in `TABS` and
 * asserts each one draws something, so a tab added to the list without a
 * screen is a red test rather than a dead link.
 */
import { Navigate, NavLink, Route, Routes, useLocation } from 'react-router'
import { Backup } from './Backup'
import { TABS } from './config'
import { RuleEditor } from './RuleEditor'
import { Libraries } from './Libraries'
import { Users } from './Users'
import { useDocumentTitle } from './title'

/** Which component draws each tab. Keyed by `Tab.path`. */
const SCREENS: Record<string, React.ReactNode> = {
  rules: <RuleEditor />,
  libraries: <Libraries />,
  users: <Users />,
  backup: <Backup />,
}

export function Config() {
  // O173: the tab names itself after the configuration tab it shows. A path
  // naming no tab is redirected below, and the title says the app's name
  // alone until then.
  const { pathname } = useLocation()
  const shown = TABS.find(
    (tab) =>
      pathname === `/config/${tab.path}` ||
      pathname.startsWith(`/config/${tab.path}/`),
  )
  useDocumentTitle(shown?.label)
  return (
    <div className="config">
      {/* O163: one h1 for the tabs, each of which heads itself with an h2
          beneath it. Hidden: the nav already says where you are. */}
      <h1 className="visually-hidden">Configuration</h1>
      <nav className="config-nav" aria-label="Configuration">
        <ul>
          {TABS.map((tab) => (
            <li key={tab.path}>
              <NavLink to={`/config/${tab.path}`}>{tab.label}</NavLink>
            </li>
          ))}
        </ul>
      </nav>
      <Routes>
        <Route
          index
          element={<Navigate to={`/config/${TABS[0].path}`} replace />}
        />
        {TABS.map((tab) => (
          <Route
            key={tab.path}
            path={tab.pages ? `${tab.path}/*` : tab.path}
            element={SCREENS[tab.path]}
          />
        ))}
        {/* A bookmark to the old Roots tab goes to the Libraries screen, not
            to the leading tab. */}
        <Route
          path="roots"
          element={<Navigate to="/config/libraries" replace />}
        />
        {/*
          `Routes.tsx` matches `/config/*` before its own catch-all, so a bad
          sub-path never reaches that one — `/config/rules` drew the sub-nav
          over a blank pane with nothing said, and PR B adding that tab is
          exactly when somebody types it. Whole-branch review, 2026-09-04.
        */}
        <Route
          path="*"
          element={<Navigate to={`/config/${TABS[0].path}`} replace />}
        />
      </Routes>
    </div>
  )
}
