/**
 * The library's chrome: the sidebar, and the `<main>` a screen draws into.
 *
 * **The status fetch lives here rather than in each screen**, because three
 * screens need the same roots list and three fetches is three chances for them
 * to disagree. `Library` also reads it for §8's empty states, through
 * `shell.ts`'s context rather than through a prop, so a screen that does not
 * care about it says nothing about it.
 *
 * The theme's tokens throughout — see `styles/tokens.css`, and `Login.tsx`,
 * which once shipped a hardcoded `#111` on this app's front door.
 */
import { useCallback, useEffect, useRef, useState } from 'react'
import { Link, useLocation } from 'react-router'
import { BrowseSettings } from './BrowseSettings'
import {
  BrowsePrefsContext,
  browseModule,
  browseRegistry,
} from './browseModule'
import { INBOX_MIN_SCORE, inboxTotal, listProposals } from './curation'
import { useGate } from './gate'
import { usePrefsMap } from './hooks'
import { type LibraryStatus, status as fetchStatus } from './library'
import { fieldChanged, sliceFor, sliceReset } from './module'
import { loadPrefs, resetPrefs, savePrefs } from './prefs'
import { InboxRefreshContext, StatusContext } from './shell'
import { Sidebar } from './Sidebar'
import { ThemeControl } from './ThemeControl'
import { Wordmark } from './Wordmark'
import { messageOf } from './message'

/**
 * `browseRegistry.all()`, hoisted once. `usePrefsMap`'s persistence effect
 * depends on `modules` by identity (`[modules, map, save]`, `hooks.ts`), and
 * `browseRegistry.all()` returns a new array on every call — handed in
 * inline, that would re-fire the effect, and re-`savePrefs`, on every
 * render. `App.tsx` hoists `registry.all()` at module scope for its own
 * `usePrefsMap` call for the same reason.
 */
const BROWSE_MODULES = browseRegistry.all()

/** Spec 2026-09-17 §2's breakpoint: the sidebar at or above it, the drawer below. */
const WIDE = '(width >= 900px)'

/**
 * What makes the sidebar column a modal dialog, while — and only while — the
 * drawer is open. Spread rather than written as three conditional props:
 * biome's `useAriaPropsSupportedByRole` refuses `aria-modal` beside a `role`
 * it cannot read statically.
 */
const AS_DIALOG = {
  role: 'dialog',
  'aria-modal': true,
  'aria-label': 'Menu',
} as const

export function Shell({ children }: { children: React.ReactNode }) {
  // **The roots list is an admin's, and only the gate knows who is asking.**
  // `undefined` — no gate above, which means a test mounting one screen — reads
  // as "no account known" and so as not an admin: what an account may *see* is
  // the server's to enforce, and this chooses only what to draw.
  //
  // A default rather than the last word: §11 step 8's `browse` module makes it
  // a preference, and a preference has to start somewhere.
  const admin = useGate()?.who.admin ?? false
  const [status, setStatus] = useState<LibraryStatus | undefined>()
  const [failure, setFailure] = useState<string | undefined>()

  // **The reader's own preference machinery, via the reader's own hook.**
  // `hooks.ts`'s `usePrefsMap` is what `App.tsx` uses to load every reader
  // module's slice once and persist it from an effect after every commit; this
  // is that same hook, over `browseRegistry`'s one module instead of the
  // reader's several. `fieldChanged` and `sliceReset` (`module.ts`) are still
  // generic over whatever registry they are handed — which is what §4 means
  // by "a second registry, not a second implementation" — but the state and
  // its persistence are the hook's now, not a hand-rolled `useState` that
  // persisted from inside `setMap`'s own updater. That mattered: an updater
  // must stay pure (React may invoke it more than once), and a persist done
  // there could not survive `sliceReset` — see `reset` below.
  const [map, setMap] = usePrefsMap(
    BROWSE_MODULES,
    (id, defaults) =>
      // **`admin` folded into the defaults, not into the stored value.**
      // `loadPrefs` merges what is stored over what it is given, so an account
      // that has never opened settings gets the roots list Plan A gave it, and
      // one that switched the list off keeps it off. The gate resolves before
      // any route mounts, so `admin` is known on this first render.
      loadPrefs(id, { ...defaults, showRoots: admin }),
    savePrefs,
  )
  const prefs = sliceFor(map, browseModule)

  const change = (key: string, raw: unknown) => {
    setMap((prev) =>
      fieldChanged(prev, browseRegistry, browseModule.id, key, raw),
    )
  }

  // **Both halves of the reset are needed and neither is enough** —
  // `module.ts`'s `sliceReset` says so: `resetPrefs` alone satisfies "reload,
  // confirm it persisted" and silently fails "change a setting, confirm it
  // takes effect".
  //
  // **`sliceReset` restores `browseModule.defaults`, so `showRoots` resets to
  // `false` even for an admin — and stays `false` through a reload.** That is
  // the module's declared default and the right answer for a control the
  // reader just asked to reset. The admin fold above only ever applies when
  // storage is *absent*; `usePrefsMap`'s effect re-persists this map on every
  // change, `sliceReset`'s included, so the next mount finds `showRoots:
  // false` stored explicitly and the fold has nothing left to override. The
  // admin fold is a genuine first-run default, and this reset is a floor.
  const reset = () => {
    resetPrefs(browseModule.id)
    setMap((prev) => sliceReset(prev, browseRegistry, browseModule.id))
  }

  useEffect(() => {
    let cancelled = false
    fetchStatus()
      .then((next) => {
        if (cancelled) return
        setStatus(next)
        setFailure(undefined)
      })
      .catch((err: unknown) => {
        if (cancelled) return
        setFailure(messageOf(err))
      })
    return () => {
      cancelled = true
    }
  }, [])

  // **The open count on the sidebar's Proposals entry — C3 §6, and the build
  // design's file-structure table.** The entry shipped without it, on the
  // ground that this component mounts around every screen and so a count is a
  // request on every page load. Measured, that request is the cheap one:
  // `GET /api/library/proposals` reads `proposal` — joined to `series` twice
  // for the two titles a row draws, and grouped for the counts — and it
  // neither compiles the rule set nor walks `scan_entry`, which is what the
  // detail and dry-run routes both do. So it is flat where those two cost
  // 1.1–1.2 µs per entry: 0.10 ms at 8 rows and 0.23 ms at 25,008
  // (`notes.md`, 2026-08-26). It is admin-only on top of that, so a reader
  // pays nothing.
  //
  // **It uses `counts` and throws the rows away**, which is worth saying
  // rather than leaving to be discovered: there is no counts-only route, and
  // adding one for a badge would be a route to keep in step with the list's
  // own filters for no measured gain.
  //
  // **A failure is swallowed on purpose, unlike the status fetch above.** A
  // count is an ornament on a link that works without it; the link is drawn
  // either way, and putting a second `role="alert"` over every screen in the
  // library because a badge could not be fetched would be the failure
  // reporting more than the failure costs. `undefined` — never fetched, still
  // in flight, or failed — draws no badge, which is `StatusContext`'s own
  // rule for the same three states.
  const [openProposals, setOpenProposals] = useState<number | undefined>()
  const [inboxReads, setInboxReads] = useState(0)
  const refreshInbox = useCallback(() => setInboxReads((n) => n + 1), [])

  // `Proposals.tsx` carries the same suppression for the same shape, and
  // `hooks.ts`'s `useScrollFollow` is where it is written down: a dependency
  // the body never reads, listed because its *change* is the signal.
  // Removing it — which is what this rule suggests — would leave the badge at
  // whatever it read when the shell mounted, however many proposals the
  // screen beside it has since answered.
  // biome-ignore lint/correctness/useExhaustiveDependencies: `inboxReads` is a trigger, not a closure read — the body reads only `admin`.
  useEffect(() => {
    if (!admin) return
    let cancelled = false
    listProposals({ state: 'open', min_score: INBOX_MIN_SCORE })
      .then((inbox) => {
        if (cancelled) return
        setOpenProposals(inboxTotal(inbox.counts))
      })
      .catch(() => {})
    return () => {
      cancelled = true
    }
  }, [admin, inboxReads])

  // **The drawer — spec 2026-09-17 §7.1.** Below 900 px the sidebar column is
  // off-canvas and ☰ slides it in; at or above, ☰ is not drawn, so this never
  // becomes true there. While it is open the bar and `<main>` are `inert`,
  // which is what keeps focus and taps inside the column: a browser skips an
  // inert subtree for both. A native `<dialog>` would have done the same, but
  // this column is also the permanent sidebar at 900 px and up, so its content
  // would have had to move between the two. The column carries its own Close
  // button because the scrim sits outside the dialog and ☰ is inert while it is
  // open, so for a screen reader inside the `aria-modal` region neither is a
  // way out of it — a pointer can still tap the scrim.
  const [menuOpen, setMenuOpen] = useState(false)
  const menuButton = useRef<HTMLButtonElement>(null)
  const column = useRef<HTMLDivElement>(null)
  /** Where the skip link sends focus (O174). */
  const mainRef = useRef<HTMLElement>(null)
  const wasOpen = useRef(false)
  const location = useLocation()

  // Focus follows the drawer: to its first link on opening — not the Close
  // button above it, which is one Shift+Tab away — back to ☰ on closing, and
  // only on a close, so the first render moves nothing.
  useEffect(() => {
    if (menuOpen) {
      column.current?.querySelector<HTMLElement>('a[href]')?.focus()
    } else if (wasOpen.current) {
      menuButton.current?.focus()
    }
    wasOpen.current = menuOpen
  }, [menuOpen])

  // Escape closes it, and so does the window widening past the breakpoint,
  // where the column is the sidebar again and nothing would be left to close
  // it with. `matchMedia` is checked for because jsdom has none.
  useEffect(() => {
    if (!menuOpen) return
    const onKey = (event: KeyboardEvent) => {
      if (event.key === 'Escape') setMenuOpen(false)
    }
    const wide =
      typeof window.matchMedia === 'function'
        ? window.matchMedia(WIDE)
        : undefined
    const onWide = () => {
      if (wide?.matches) setMenuOpen(false)
    }
    document.addEventListener('keydown', onKey)
    wide?.addEventListener('change', onWide)
    return () => {
      document.removeEventListener('keydown', onKey)
      wide?.removeEventListener('change', onWide)
    }
  }, [menuOpen])

  // Any navigation closes it: a link inside it, and the browser's own back
  // button alike.
  // biome-ignore lint/correctness/useExhaustiveDependencies: `location` is a trigger, not a closure read — the body reads nothing from it.
  useEffect(() => {
    setMenuOpen(false)
  }, [location])

  const classes = [
    'library',
    prefs.density === 'compact' ? 'compact' : '',
    menuOpen ? 'menu-open' : '',
  ]
    .filter(Boolean)
    .join(' ')

  return (
    <BrowsePrefsContext.Provider value={prefs}>
      <InboxRefreshContext.Provider value={refreshInbox}>
        <StatusContext.Provider value={status}>
          <div className={classes}>
            {/* O174: a sighted keyboard user's way past the sidebar. Focus
                moves by hand rather than by following `#content`, which
                would put a fragment in the URL and a history entry for Back
                to walk through. `preventScroll`, because at phone width the
                bar is sticky and would cover `main`'s top edge. `inert` with
                the bar and `main`, so the drawer's trap stays sealed.

                **`main` is focusable only while it holds this focus.** A
                standing `tabIndex` would make it take focus on every click
                on plain text inside it, and the next `Tab` would start from
                its top rather than from the click. So the attribute goes on
                here and comes off when focus leaves. While `main` holds that
                focus a click on plain text inside it still lands on `main`,
                until focus next moves. */}
            {/* biome-ignore lint/a11y/useValidAnchor: a link and not a button — it has a destination, `#content`, and the handler goes there without leaving the fragment in the URL. */}
            <a
              className="skip-link"
              href="#content"
              inert={menuOpen}
              onClick={(event) => {
                event.preventDefault()
                const main = mainRef.current
                if (!main) return
                main.tabIndex = -1
                main.addEventListener(
                  'blur',
                  () => {
                    main.removeAttribute('tabindex')
                  },
                  { once: true },
                )
                main.focus({ preventScroll: true })
              }}
            >
              Skip to content
            </a>
            <header className="phone-bar" inert={menuOpen}>
              <button
                ref={menuButton}
                type="button"
                className="menu-button"
                aria-label="Menu"
                aria-expanded={menuOpen}
                aria-controls="library-menu"
                onClick={() => {
                  setMenuOpen(true)
                }}
              >
                <svg viewBox="0 0 24 24" aria-hidden="true">
                  <path d="M4 7h16M4 12h16M4 17h16" />
                </svg>
              </button>
              <Link to="/" className="home">
                <Wordmark />
              </Link>
              <Link to="/search" className="search" aria-label="Search">
                <svg viewBox="0 0 24 24" aria-hidden="true">
                  <circle cx="11" cy="11" r="6" />
                  <path d="M16 16l4 4" />
                </svg>
              </Link>
            </header>
            <div
              id="library-menu"
              ref={column}
              className="sidebar-column"
              {...(menuOpen ? AS_DIALOG : {})}
            >
              <Wordmark />
              <button
                type="button"
                className="menu-close"
                aria-label="Close menu"
                onClick={() => {
                  setMenuOpen(false)
                }}
              >
                <svg viewBox="0 0 24 24" aria-hidden="true">
                  <path d="M6 6l12 12M18 6L6 18" />
                </svg>
              </button>
              <Sidebar
                roots={status?.roots ?? []}
                showRoots={prefs.showRoots}
                admin={admin}
                openProposals={openProposals}
              />
              <BrowseSettings prefs={prefs} onChange={change} onReset={reset} />
              <ThemeControl />
            </div>
            {menuOpen && (
              <button
                type="button"
                className="scrim"
                aria-label="Close menu"
                tabIndex={-1}
                onClick={() => {
                  setMenuOpen(false)
                }}
              />
            )}
            <main id="content" ref={mainRef} inert={menuOpen}>
              {failure !== undefined && <p role="alert">{failure}</p>}
              {children}
            </main>
          </div>
        </StatusContext.Provider>
      </InboxRefreshContext.Provider>
    </BrowsePrefsContext.Provider>
  )
}
