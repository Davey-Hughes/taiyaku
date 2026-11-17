/**
 * Back to the Library view or a series page lands at the scroll it was left
 * at, and the reader's way out returns to the last Library view at its
 * scroll (spec `2026-10-28-loading-library-design.md` §5.2). The two views call
 * `useKeptScroll`; the reader's exits read `lastLibraryAddress`.
 *
 * **Kept per history entry**, by react-router's `location.key`, in memory
 * for the tab: a reload starts at the top. A position is saved as the
 * reader scrolls and as the view goes.
 *
 * **Where an arrival lands**, once its body is drawn, and once only, so a
 * re-read landing later moves nothing:
 * - Back or Forward (`POP`): the entry's kept scroll, clamped to the room
 *   there is now; the top if none is kept.
 * - A push marked `RESTORE_SCROLL` to the last Library view's address: that
 *   view's kept scroll.
 * - Any other push, a replace that draws the view, and a replace marked
 *   `SCROLL_TO_TOP`: the top. A view's first render draws kept answers at
 *   once, so without this a link would land at whatever scroll the page
 *   before it left. The sidebar's links and the wordmarks carry the mark
 *   (`TO_TOP`): a link to the address shown is a replace, so a click adds
 *   no entry, and unmarked it would stay where the view is.
 * - Any other replace while the view is drawn (a chip, the sort, an
 *   edition, a key typed into the search box) leaves the scroll where it
 *   is, and saving goes on under the entry it makes.
 *
 * **The scroller is read from the layout**: `<main>` where the stylesheet
 * lets it scroll, beside the sidebar, else the document.
 */
import { useEffect, useLayoutEffect, useRef } from 'react'
import { useLocation, useNavigationType } from 'react-router'

/** The navigation-state key `onLeave` sets: a key of its own, so the search box, which reads only its tag, takes the arrival as a change it did not make. */
export const RESTORE_SCROLL = 'restoreScroll'

/** The navigation-state key `TO_TOP` sets: a replace with it lands at the top, as a push does. */
export const SCROLL_TO_TOP = 'scrollToTop'

/** The navigation state of a link that lands at the top of what it opens, the view already drawn included. */
export const TO_TOP = { [SCROLL_TO_TOP]: true }

/** Each history entry's scroll, by `location.key`. */
const kept = new Map<string, number>()
/** The last Library view drawn: its address, and the entry it was drawn on. */
let library: { address: string; key: string } | undefined

/** The whole address of the last Library view drawn in this tab, `?q=` included. */
export function lastLibraryAddress(): string | undefined {
  return library?.address
}

/** Drops every kept scroll and the Library view's address: they were kept for the account signed in when they were drawn. */
export function forgetScroll(): void {
  kept.clear()
  library = undefined
}

/** The element that scrolls the shell's pages in the layout drawn. */
export function scroller(): Element {
  const main = document.getElementById('content')
  if (main !== null) {
    const { overflowY } = getComputedStyle(main)
    if (overflowY === 'auto' || overflowY === 'scroll') return main
  }
  return document.scrollingElement ?? document.documentElement
}

function scrollTo(y: number) {
  const at = scroller()
  at.scrollTop = Math.max(0, Math.min(y, at.scrollHeight - at.clientHeight))
}

function marked(state: unknown, mark: string): boolean {
  return (state as Record<string, unknown> | null)?.[mark] === true
}

/** The entry a view draws, where its arrival lands (`null`: where it is), and whether it has. */
interface Drawn {
  readonly key: string
  readonly to: number | null
  landed: boolean
}

/**
 * Restores the kept scroll once `ready` on a Back/Forward arrival or a
 * RESTORE_SCROLL one; saves it while drawn. `library` marks the Library
 * view, whose address `lastLibraryAddress` gives and whose scroll a
 * RESTORE_SCROLL arrival lands at. `ready` says the body for this location
 * is drawn, so the room to scroll is there.
 */
export function useKeptScroll(
  ready: boolean,
  options?: { library?: boolean },
): void {
  const location = useLocation()
  const arrival = useNavigationType()
  const { key } = location
  const isLibrary = options?.library === true
  const drawn = useRef<Drawn | undefined>(undefined)

  // biome-ignore lint/correctness/useExhaustiveDependencies: `key` stands for the location, read once per entry; `ready` is the other trigger.
  useLayoutEffect(() => {
    let entry = drawn.current
    if (entry?.key !== key) {
      const address = `${location.pathname}${location.search}`
      let to: number | null = 0
      if (arrival === 'POP') to = kept.get(key) ?? 0
      else if (arrival === 'REPLACE')
        to =
          entry === undefined || marked(location.state, SCROLL_TO_TOP)
            ? 0
            : null
      else if (
        isLibrary &&
        marked(location.state, RESTORE_SCROLL) &&
        library?.address === address
      )
        to = kept.get(library.key) ?? 0
      entry = { key, to, landed: false }
      drawn.current = entry
      if (isLibrary) library = { address, key }
    }
    if (!ready || entry.landed) return
    entry.landed = true
    if (entry.to !== null) scrollTo(entry.to)
    kept.set(key, scroller().scrollTop)
  }, [key, ready])

  // Saved as the reader scrolls, which the browser reports once a frame at
  // most for each element that scrolls, and as the view goes. Both are this
  // layout effect's: its cleanup runs in the commit that takes the view
  // away, before the view's nodes are taken out, so the scroller still has
  // the view's height to report, and before the next view lands, so the
  // listener cannot hear that landing. A passive effect's cleanup can run
  // after a frame has passed, as it can after a commit in a transition,
  // which is how `BrowserRouter` commits a navigation, and its listener
  // would save the next view's landing under the entry left.
  useLayoutEffect(() => {
    const scrolled = () => {
      const entry = drawn.current
      if (entry?.landed) kept.set(entry.key, scroller().scrollTop)
    }
    document.addEventListener('scroll', scrolled, {
      capture: true,
      passive: true,
    })
    return () => {
      document.removeEventListener('scroll', scrolled, { capture: true })
      scrolled()
    }
  }, [])

  // The browser's own restoration would put a scroll back on Back after the
  // effect above has put the view in place. The setting is the history
  // entry's, and an entry a push makes starts with the one it is made from,
  // so the entry left for is set back to the browser's default as the view
  // goes: a view drawn there turns it off again, after this.
  useEffect(() => {
    history.scrollRestoration = 'manual'
    return () => {
      history.scrollRestoration = 'auto'
    }
  }, [])
}
