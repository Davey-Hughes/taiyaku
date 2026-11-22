/**
 * Where a screen's way back goes, and what its button is called.
 *
 * **Back goes where you came from, else the library.** Where an entry this
 * app drew comes before the one on screen, the way back is the browser's own
 * Back: it lands where the reader came from, at the scroll kept there
 * (`scrollKeeper.ts`), and leaves no entry behind for the browser's Back to
 * walk into again. Where none does, as on a cold load, it pushes the last
 * Library view drawn, at its scroll, or `/` at the top.
 *
 * **Whether an entry comes before is `BrowserRouter`'s `idx`**, which its
 * history writes into `window.history.state`: 0 on the first entry the app
 * draws in a tab, one more on each push, the same through a replace. The
 * browser keeps it through a reload, so a reloaded reader still goes back.
 * react-router has no other way to ask.
 *
 * **The button names where it goes** from the address of the entry the one on
 * screen was pushed from, which `useBackTracker` records as each push is
 * drawn. The record is in memory for the tab, as the scroll keeper's scrolls
 * are, so after a reload the button says "Back", and still goes there; and it
 * is the signed-in account's, so the gate forgets it on signing out and in.
 * No storage: `prefs.ts` is the one writer of `localStorage`.
 */
import { useLayoutEffect, useRef } from 'react'
import { useLocation, useNavigate, useNavigationType } from 'react-router'
import { lastLibraryAddress, RESTORE_SCROLL } from './scrollKeeper'

/** A way back's name: the word its button shows, and the phrase a sentence ends with, `''` for none. */
export interface Destination {
  readonly word: string
  readonly phrase: string
}

const BACK: Destination = { word: 'Back', phrase: '' }

/** Each pushed entry's `location.key`, and the address (`pathname + search`) it was pushed from. */
const pushedFrom = new Map<string, string>()

/** Drops every record: each was kept for the account signed in when it was drawn. */
export function forgetBack(): void {
  pushedFrom.clear()
}

/** What a way back to `path`, a `pathname + search`, is called. */
export function destination(path: string): Destination {
  const cut = path.indexOf('?')
  const pathname = cut < 0 ? path : path.slice(0, cut)
  const under = (root: string) =>
    pathname === root || pathname.startsWith(`${root}/`)
  if (pathname === '/') {
    // The Library view draws its search hits for any `q` but the empty one.
    const q = new URLSearchParams(cut < 0 ? '' : path.slice(cut)).get('q')
    return (q ?? '') === ''
      ? { word: 'Library', phrase: 'the library' }
      : { word: 'Search', phrase: 'the search' }
  }
  if (pathname.startsWith('/series/'))
    return { word: 'Series', phrase: 'the series' }
  if (pathname.startsWith('/read/') || pathname.startsWith('/book/'))
    return { word: 'Book', phrase: 'the book' }
  if (pathname === '/proposals')
    return { word: 'Proposals', phrase: 'Proposals' }
  if (under('/config'))
    return { word: 'Configuration', phrase: 'Configuration' }
  if (pathname === '/account') return { word: 'Account', phrase: 'Account' }
  if (under('/about')) return { word: 'About', phrase: 'About' }
  return BACK
}

/**
 * Records where each push came from, under the new entry's key; a replace
 * hands its entry's record on to the entry taking its place. Called once, at
 * the top of `AppRoutes`, during its render rather than in an effect:
 * `AppRoutes` renders before every screen in the same pass, so a screen's
 * `useBack` finds the record in its first render. The write is keyed by the
 * entry drawn and reads only the last committed one, so a render React throws
 * away writes what the kept one will.
 */
export function useBackTracker(): void {
  const location = useLocation()
  const arrival = useNavigationType()
  const committed = useRef<{ key: string; path: string } | undefined>(undefined)
  const before = committed.current
  if (before !== undefined && before.key !== location.key) {
    if (arrival === 'PUSH') pushedFrom.set(location.key, before.path)
    else if (arrival === 'REPLACE') {
      const from = pushedFrom.get(before.key)
      if (from !== undefined) pushedFrom.set(location.key, from)
    }
  }
  useLayoutEffect(() => {
    committed.current = {
      key: location.key,
      path: `${location.pathname}${location.search}`,
    }
  }, [location])
}

/** Whether an entry the app drew comes before the one on screen. */
function entryBefore(): boolean {
  const idx = (window.history.state as { idx?: unknown } | null)?.idx
  return typeof idx === 'number' && idx > 0
}

/**
 * The screen's way back, named for where it goes: a step back, named for the
 * entry it was pushed from, or "Back" where that is not known; else a push of
 * the last Library view drawn, marked to land at its kept scroll, or of `/`.
 */
export function useBack(): Destination & { go: () => void } {
  const { key } = useLocation()
  const navigate = useNavigate()
  // The entry a press went from, until another is drawn: the browser's Back
  // lands a moment after it is asked for, and a push is drawn in a
  // transition, so this screen still takes presses after one, and a second
  // would step again.
  const left = useRef<string | undefined>(undefined)
  // biome-ignore lint/correctness/useExhaustiveDependencies: `key` is the trigger: a new entry drawn lets the next press through.
  useLayoutEffect(() => {
    left.current = undefined
  }, [key])
  const go = () => {
    if (left.current === key) return
    left.current = key
    if (entryBefore()) void navigate(-1)
    else
      void navigate(lastLibraryAddress() ?? '/', {
        state: { [RESTORE_SCROLL]: true },
      })
  }
  if (!entryBefore()) return { ...destination(lastLibraryAddress() ?? '/'), go }
  const from = pushedFrom.get(key)
  return { ...(from === undefined ? BACK : destination(from)), go }
}
