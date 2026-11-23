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
 * **A reader's way back goes to its work's series page** (O431), once the
 * reader knows which: a step back where that page is the entry under the
 * reader's, else that page in the reader's place. The reader puts the page
 * under itself, so that the browser's Back and a phone's back gesture land
 * there too (`docs/notes.md`, "The reader's way back and the browser's
 * history (O431)", for what each browser lets a page add). Its route can ask
 * instead for the rule above wherever that page is not already under the
 * reader, which then puts nothing under itself (the other value of O431's
 * setting, in the same section). A work in no series or whose series is not
 * yet known, and every other screen, the series page's Back among them
 * (O423), keep the rule above.
 *
 * **Whether an entry comes before is `BrowserRouter`'s `idx`**, which its
 * history writes into `window.history.state`: 0 on the first entry the app
 * draws in a tab, one more on each push, the same through a replace. The
 * browser keeps it through a reload, so after one the rule above still steps
 * back. react-router has no other way to ask.
 *
 * **The button names where it goes** from the address of the entry the one on
 * screen was pushed from, which `useBackTracker` records as each push is
 * drawn. The record is in memory for the tab, as the scroll keeper's scrolls
 * are, so after a reload the button says "Back", and still goes there, unless
 * the entry's own navigation state names where it goes, as a reader's marked
 * entry does once its work's answer has named the series, and the series page
 * it put under itself until an edition is chosen there (O431); and the record
 * is the signed-in account's, so the gate forgets it on signing out and in.
 * No storage: `prefs.ts` is the one writer of `localStorage`.
 */
import { useEffect, useLayoutEffect, useRef } from 'react'
import { useLocation, useNavigate, useNavigationType } from 'react-router'
import { lastLibraryAddress, RESTORE_SCROLL } from './scrollKeeper'

/** A way back's name: the word its button shows, and the phrase a sentence ends with, `''` for none. */
export interface Destination {
  readonly word: string
  readonly phrase: string
}

const BACK: Destination = { word: 'Back', phrase: '' }

/**
 * The navigation-state key of a reader's entry that has its series page, the
 * value, as the entry under it (O431). Unlike the record below, it outlives a
 * reload.
 */
export const BENEATH = 'seriesBeneath'

/**
 * The navigation-state key of a series page's entry that a reader put under
 * itself: the address the reader was pushed from, which names the page's way
 * back where no record is kept for the entry (O423, O431).
 */
export const CAME_FROM = 'cameFrom'

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
 * hands its entry's record, or else its `CAME_FROM`, on to the entry taking
 * its place. Called once, at the top of `AppRoutes`, during its render
 * rather than in an effect: `AppRoutes` renders before every screen in the
 * same pass, so a screen's `useBack` finds the record in its first render.
 * The write is keyed by the entry drawn and reads only the last committed
 * one, so a render React throws away writes what the kept one will.
 */
export function useBackTracker(): void {
  const location = useLocation()
  const arrival = useNavigationType()
  const committed = useRef<
    { key: string; path: string; cameFrom: string | undefined } | undefined
  >(undefined)
  const before = committed.current
  if (before !== undefined && before.key !== location.key) {
    // A push that carries the mark is the reader putting its series page
    // under itself: the entry under it is that page, not the reader (O431).
    if (arrival === 'PUSH')
      pushedFrom.set(
        location.key,
        stateText(location.state, BENEATH) ?? before.path,
      )
    else if (arrival === 'REPLACE') {
      // A series page a reader put under itself has no record, only its
      // `CAME_FROM`, which a replace (an edition chosen) does not carry.
      const from = pushedFrom.get(before.key) ?? before.cameFrom
      if (from !== undefined) pushedFrom.set(location.key, from)
    }
  }
  useLayoutEffect(() => {
    committed.current = {
      key: location.key,
      path: `${location.pathname}${location.search}`,
      cameFrom: stateText(location.state, CAME_FROM),
    }
  }, [location])
}

/** The text an entry's navigation state holds under `name`, if it holds text there. */
function stateText(state: unknown, name: string): string | undefined {
  const value = (state as Record<string, unknown> | null)?.[name]
  return typeof value === 'string' ? value : undefined
}

/** Whether an entry the app drew comes before the one on screen. */
function entryBefore(): boolean {
  const idx = (window.history.state as { idx?: unknown } | null)?.idx
  return typeof idx === 'number' && idx > 0
}

/** `navigator.userActivation`, which not every browser has. */
interface Activation {
  readonly isActive: boolean
}

/** The events a reader listens to for the first input. */
const INPUTS = ['keydown', 'pointerdown', 'pointerup', 'touchend'] as const

/**
 * Whether `event` is an input that activates the page, by the HTML spec's
 * list: a key but `Escape`, a mouse's press, any other pointer's lift, a
 * touch's end. Only after one does every browser keep, on its Back, an entry
 * the page pushes (O431). Where the browser has `userActivation`, it must
 * also say active, so an event a script made counts only while a real
 * input's activation is still live.
 */
export function countsAsInput(
  event: {
    readonly type: string
    readonly key?: string
    readonly pointerType?: string
  },
  activation: Activation | undefined,
): boolean {
  if (activation?.isActive === false) return false
  switch (event.type) {
    case 'keydown':
      return event.key !== 'Escape'
    case 'pointerdown':
      return event.pointerType === 'mouse'
    case 'pointerup':
      return event.pointerType !== 'mouse'
    case 'touchend':
      return true
    default:
      return false
  }
}

/**
 * Whether a reader may put its series page under itself as its answer lands,
 * not at the first input: where the click that opened it is still the page's
 * activation (`userActivation` active, the reader drawn `drawnFor` ms ago,
 * under 5 s), and the browser is not Firefox, whose Back skips the series
 * page unless the reader had an input first (O431).
 */
export function rewritesAtAnswer(
  activation: Activation | undefined,
  userAgent: string,
  drawnFor: number,
): boolean {
  return (
    activation?.isActive === true &&
    drawnFor < 5000 &&
    !userAgent.includes('Firefox/')
  )
}

/** What `useBack` keeps for the reader on screen, the one at `path`. */
interface Reading {
  readonly path: string
  /** When it was first drawn, by `performance.now()`. */
  readonly drewAt: number
  /** Whether its series page is put under it, or may no longer be. */
  settled: boolean
}

/**
 * The screen's way back, named for where it goes. Given a reader's series
 * page (O431): a step back where that page is under the reader, else the page
 * in the reader's place; and the page put under the reader, once, as it is
 * given or at the first input. Given `putsUnder` false too, the other value
 * of O431's setting, nothing is put there, and the rule below holds where the
 * page is not under. Otherwise a step back, named for the entry it was pushed
 * from, else its own entry's `CAME_FROM`, or "Back"; else a push of the last
 * Library view drawn, marked to land at its kept scroll, or of `/`.
 */
export function useBack(
  seriesPage?: string,
  putsUnder = true,
): Destination & { go: () => void } {
  const { key, pathname, search, hash, state } = useLocation()
  const arrival = useNavigationType()
  const navigate = useNavigate()
  // Whether a press has gone, until another entry is drawn: the browser's
  // Back lands a moment after it is asked for, and a push is drawn in a
  // transition, so this screen still takes presses after one, and a second
  // would step again.
  const left = useRef(false)
  // Whether this hook has put `seriesPage` under the reader: true from that
  // moment, before the entry it pushes is drawn with the mark that says so.
  const rewritten = useRef(false)
  // biome-ignore lint/correctness/useExhaustiveDependencies: `key` is the trigger: a new entry drawn lets the next press through.
  useLayoutEffect(() => {
    // The reader's stamp or rewrite draws its own entry again, with the mark:
    // no other entry, so a press gone from it before then still holds (O431).
    if (arrival === 'POP' || stateText(state, BENEATH) === undefined)
      left.current = false
  }, [key])
  const reading = useRef<Reading>({ path: '', drewAt: 0, settled: false })
  // The reader on screen: its path and when it was drawn, set as it is
  // drawn. `Routes.tsx` mounts this hook afresh for each slug; the reset on
  // a new path also keeps a caller that holds the hook across readers from
  // handing one reader's state to the next. Kept through StrictMode's second
  // run, which keeps the path.
  useLayoutEffect(() => {
    if (reading.current.path === pathname) return
    reading.current = {
      path: pathname,
      drewAt: performance.now(),
      settled: false,
    }
    rewritten.current = false
  }, [pathname])
  useEffect(() => {
    // A step through the history has begun: an entry pushed now would land
    // over the one it goes to.
    const stop = () => {
      reading.current.settled = true
    }
    window.addEventListener('popstate', stop)
    return () => window.removeEventListener('popstate', stop)
  }, [])
  useEffect(() => {
    const now = reading.current
    if (seriesPage === undefined || now.settled) return
    if (stateText(state, BENEATH) === seriesPage) return
    const own = { pathname, search, hash }
    const marked = {
      ...(state as Record<string, unknown> | null),
      [BENEATH]: seriesPage,
    }
    const from = pushedFrom.get(key)
    if (from?.split('?')[0] === seriesPage) {
      // A replace: no browser skips one on its Back, input or none.
      now.settled = true
      void navigate(own, { replace: true, state: marked })
      return
    }
    // The route asked for O422's rule where the page is not under the reader:
    // nothing is put there, as the answer lands or at an input.
    if (!putsUnder) return
    // In one step, so the series page is never drawn: React draws only the
    // last of the two.
    const rewrite = () => {
      now.settled = true
      rewritten.current = true
      void navigate(seriesPage, { replace: true, state: { [CAME_FROM]: from } })
      void navigate(own, { state: marked })
    }
    const activation = () =>
      (navigator as { userActivation?: Activation }).userActivation
    const drawnFor = performance.now() - now.drewAt
    if (rewritesAtAnswer(activation(), navigator.userAgent, drawnFor)) {
      rewrite()
      return
    }
    // On `window` as it is captured, before the way out's handlers, which
    // bubble: `L`'s way out then finds the series page under the reader.
    const hear = (event: Event) => {
      if (!now.settled && countsAsInput(event, activation())) rewrite()
    }
    for (const type of INPUTS) window.addEventListener(type, hear, true)
    return () => {
      for (const type of INPUTS) window.removeEventListener(type, hear, true)
    }
  }, [seriesPage, putsUnder, key, pathname, search, hash, state, navigate])
  // Whether the series page is the entry under the reader, by the mark or the
  // record. A series page's address is its path: `?edition=` and `?library=`
  // pick what it shows, and the step back keeps them.
  const under = () =>
    seriesPage !== undefined &&
    (stateText(state, BENEATH) === seriesPage ||
      pushedFrom.get(key)?.split('?')[0] === seriesPage)
  const go = () => {
    // A way out taken: a push now would land over where it goes.
    reading.current.settled = true
    if (left.current) return
    left.current = true
    if (seriesPage !== undefined && (under() || rewritten.current))
      void navigate(-1)
    // A replace even with no entry before: a push would leave the reader
    // under its series page, for the browser's Back to reopen.
    else if (seriesPage !== undefined && putsUnder)
      void navigate(seriesPage, { replace: true })
    else if (entryBefore()) void navigate(-1)
    else
      void navigate(lastLibraryAddress() ?? '/', {
        state: { [RESTORE_SCROLL]: true },
      })
  }
  if (seriesPage !== undefined && (putsUnder || under()))
    return { ...destination(seriesPage), go }
  if (!entryBefore()) return { ...destination(lastLibraryAddress() ?? '/'), go }
  const from = pushedFrom.get(key) ?? stateText(state, CAME_FROM)
  return { ...(from === undefined ? BACK : destination(from)), go }
}
