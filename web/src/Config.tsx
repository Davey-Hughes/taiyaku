/**
 * The settings area — spec C4, and the build design's §2.
 *
 * **One sidebar entry and a sub-nav, not four sidebar entries.** C4's opening
 * line settles the shape: configuration is "touched rarely and from a settings
 * area", so the always-visible chrome grows by one. Four entries would also
 * have put a second "Roots" in a sidebar that already has one meaning
 * something else — a browse filter, not an administrative screen.
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
import { Navigate, NavLink, Route, Routes } from 'react-router'
import { Backup } from './Backup'
import { TABS } from './config'
import { RuleEditor } from './RuleEditor'
import { Roots } from './Roots'
import { Users } from './Users'

/** Which component draws each tab. Keyed by `Tab.path`. */
const SCREENS: Record<string, React.ReactNode> = {
  rules: <RuleEditor />,
  roots: <Roots />,
  users: <Users />,
  backup: <Backup />,
}

export function Config() {
  return (
    <div className="config">
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
          <Route key={tab.path} path={tab.path} element={SCREENS[tab.path]} />
        ))}
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
