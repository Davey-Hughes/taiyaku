/**
 * The library's chrome: the sidebar, and the `<main>` a screen draws into.
 *
 * **The status fetch lives here rather than in each screen**, because three
 * screens need the same roots list and three fetches is three chances for them
 * to disagree. `Library` also reads it for §8's empty states, through
 * `shell.ts`'s context rather than through a prop, so a screen that does not
 * care about it says nothing about it.
 *
 * System colours throughout — see `index.css`'s `.library` block, and
 * `Login.tsx`, which shipped a hardcoded `#111` on this app's front door.
 */
import { useCallback, useEffect, useState } from 'react'
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

/**
 * `browseRegistry.all()`, hoisted once. `usePrefsMap`'s persistence effect
 * depends on `modules` by identity (`[modules, map, save]`, `hooks.ts`), and
 * `browseRegistry.all()` returns a new array on every call — handed in
 * inline, that would re-fire the effect, and re-`savePrefs`, on every
 * render. `App.tsx` hoists `registry.all()` at module scope for its own
 * `usePrefsMap` call for the same reason.
 */
const BROWSE_MODULES = browseRegistry.all()

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
        setFailure(err instanceof Error ? err.message : String(err))
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

  return (
    <BrowsePrefsContext.Provider value={prefs}>
      <InboxRefreshContext.Provider value={refreshInbox}>
        <StatusContext.Provider value={status}>
          <div
            className={`library ${prefs.density === 'compact' ? 'compact' : ''}`.trim()}
          >
            <div className="sidebar-column">
              <Sidebar
                roots={status?.roots ?? []}
                showRoots={prefs.showRoots}
                admin={admin}
                openProposals={openProposals}
              />
              <BrowseSettings prefs={prefs} onChange={change} onReset={reset} />
            </div>
            <main>
              {failure !== undefined && <p role="alert">{failure}</p>}
              {children}
            </main>
          </div>
        </StatusContext.Provider>
      </InboxRefreshContext.Provider>
    </BrowsePrefsContext.Provider>
  )
}
