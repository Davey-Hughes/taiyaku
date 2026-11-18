/**
 * Where the window crosses into or out of the phone layout the scroller
 * swaps (`scroller` in `scrollKeeper.ts`: `<main>` beside the sidebar, the
 * document in the phone layout), and the new one starts at its top; where a
 * phone turns, iOS sets its own scroll. This puts back what was at the top
 * of the view, the top as far down it as a share of its height, before the
 * new layout's first paint. Inside a cover list that is the first cover on
 * the top row seen, elsewhere the element the top edge crosses; in the
 * phone layout the view starts under the bar.
 */
import { type RefObject, useEffect } from 'react'
import { barLine } from './phoneBar'
import { frameTime, onRowLaid, scrollPutBack, uprightNow } from './reseatTiming'
import { scroller } from './scrollKeeper'

/** The lists whose children are covers laid row by row. */
const COVER_LISTS = '.grid, .volumes'

/** What the view holds at its top, kept until the reader scrolls. */
interface Place {
  /** The element that scrolled when the place was last drawn. */
  scroller: Element
  /** Null where `<main>` draws nothing below the top. */
  anchor: HTMLElement | null
  /**
   * How far down the anchor the view's top falls, as a share of its height,
   * so that the anchor still crosses the top where its height changes: 0
   * where its top is at or below the view's top.
   */
  share: number
  /**
   * How far below the view's top the anchor's top stood where the top
   * crossed no anchor, in px: a turn puts it back as far below, so a turn
   * and its return land where they started; a swap puts it at the top.
   */
  below: number
  /** How far below the view's top the anchor was last drawn, in px. */
  drawn: number
  /** A page at its top is put back at its top. */
  atTop: boolean
}

/**
 * Where `el` is laid out in the window: its box. A cover moving to its new
 * column is its tile's content moved by a transform (`reseat.ts`), and the
 * tile, which is what is read here, stays where layout puts it.
 */
function laid(el: Element): { top: number; bottom: number } {
  const { top, bottom } = el.getBoundingClientRect()
  return { top, bottom }
}

/**
 * The first cover of `list` on the top row seen from `line`. Covers are
 * laid row by row, so their tops never fall and the row is found by
 * halving: a frame's reads grow with the rows' count's logarithm and the
 * columns, not the covers.
 */
function coverAt(list: HTMLElement, line: number): HTMLElement {
  const covers = list.children
  let low = 0
  let high = covers.length
  while (low < high) {
    const mid = (low + high) >> 1
    if (laid(covers[mid]).top > line) high = mid
    else low = mid + 1
  }
  if (low > 0) {
    const crossed = laid(covers[low - 1])
    if (crossed.bottom > line) {
      let first = low - 1
      while (
        first > 0 &&
        Math.abs(laid(covers[first - 1]).top - crossed.top) < 0.5
      ) {
        first -= 1
      }
      return covers[first] as HTMLElement
    }
  }
  return (covers[low] as HTMLElement | undefined) ?? list
}

/** Where a child is laid in its parent's flow, and so moves as the page scrolls. */
const IN_FLOW = new Set(['static', 'relative'])

/**
 * The first child of `parent` in the flow and not wholly above `line`. A
 * sticky, fixed or absolute child is passed over: it stands where the view
 * or a box outside the scroller puts it (a visually hidden heading stays
 * put while `<main>` scrolls), so putting it back would move nothing.
 */
function childAt(parent: Element, line: number): HTMLElement | null {
  for (const child of parent.children) {
    if (!(child instanceof HTMLElement) || laid(child).bottom <= line) continue
    if (IN_FLOW.has(getComputedStyle(child).position)) return child
  }
  return null
}

/** The element `main` keeps at `line`: the deepest the line crosses, or the first below it. */
function anchorAt(main: HTMLElement, line: number): HTMLElement | null {
  let within = main
  for (;;) {
    if (within.matches(COVER_LISTS)) return coverAt(within, line)
    const next = childAt(within, line)
    if (next === null) return within === main ? null : within
    if (laid(next).top >= line) return next
    within = next
  }
}

/**
 * How the screen faces, which a turn changes and a desktop window dragged
 * across square does not: there the browser anchors the page, and a place
 * put back as at a turn would drift a little each way. Where the screen's
 * orientation cannot be asked, whether the view stands upright.
 */
function facingNow(): string | boolean | undefined {
  return globalThis.screen?.orientation?.type ?? uprightNow()
}

/**
 * Keeps what is at the top of the view there when the scroller swaps or a
 * phone turns, for the pages `main` draws; called once, by the shell. The
 * old layout is gone by the time either is known, so the place is kept
 * ahead: at the frame after a scroll or a change in `main`, unless the
 * window has resized and its observers have not run, when a read would lay
 * the page out early. The observer puts it back after layout, before paint;
 * a resize that neither swaps nor turns is the browser's to anchor.
 */
export function useScrollerSwap(main: RefObject<HTMLElement | null>): void {
  useEffect(() => {
    const content = main.current
    if (content === null || typeof ResizeObserver === 'undefined') return
    const bar = content.closest('.library')?.querySelector('.phone-bar')
    /**
     * Where the view starts in the window, for the element that scrolls. The
     * bar's box can be stale at a turn, before its script re-places it, so
     * the line is the one the script last showed (`barLine`).
     */
    const lineOf = (at: Element) =>
      at === content
        ? content.getBoundingClientRect().top + content.clientTop
        : (barLine() ?? bar?.getBoundingClientRect().bottom ?? 0)
    const offsetOf = (anchor: HTMLElement | null, at: Element) =>
      anchor === null ? 0 : laid(anchor).top - lineOf(at)
    const placeIn = (at: Element): Place => {
      const line = lineOf(at)
      const anchor = anchorAt(content, line)
      const atTop = at.scrollTop < 1
      if (anchor === null) {
        return { scroller: at, anchor, share: 0, below: 0, drawn: 0, atTop }
      }
      const { top, bottom } = laid(anchor)
      const share = line > top ? (line - top) / (bottom - top) : 0
      const below = line > top ? 0 : top - line
      return { scroller: at, anchor, share, below, drawn: top - line, atTop }
    }
    const drawnAsKept = (place: Place, at: Element) =>
      place.anchor?.isConnected === true &&
      Math.abs(offsetOf(place.anchor, at) - place.drawn) < 0.5

    let place: Place | undefined
    /** Set from a resize the observer has not seen, until it runs. */
    let resizing = false
    /** The frame that clears `resizing` where the observer does not. */
    let settle = 0
    const size = () => `${innerWidth}x${innerHeight}`
    /** The window's size as the observer last saw it. */
    let seen = size()
    /** How the screen faced when the observer last saw it. */
    let facing = facingNow()

    /** The frame the place was last put back in. */
    let putIn = Number.NaN
    const putBack = (kept: Place, at: Element) => {
      if (kept.atTop) at.scrollTop = 0
      else if (kept.anchor?.isConnected) {
        const { top, bottom } = laid(kept.anchor)
        const line = lineOf(at)
        at.scrollTop += top - line - kept.below + kept.share * (bottom - top)
        // Near the page's top in a turn the line the bar's script gives moves
        // with the scroll (`barLine`): the place goes under it where this
        // scroll left it. Before the script starts the line is the bar's box,
        // which this scroll carries off with the page: never followed.
        if (barLine() !== null) {
          const moved = line - lineOf(at)
          if (moved !== 0) at.scrollTop += moved
        }
      } else {
        place = placeIn(at)
        return
      }
      kept.scroller = at
      putIn = frameTime()
      scrollPutBack()
      // Read once the bar's script has heard the put-back: into the phone
      // layout it starts there, and before it the line is the bar's box,
      // which this scroll carried off with the page.
      kept.drawn = offsetOf(kept.anchor, at)
    }
    // The Continue row lays a new column in its own observer, after this
    // one, and its new height moves what was put back below it: put back
    // again, before the frame is painted, where the browser does not
    // anchor the page.
    const offRowLaid = onRowLaid(() => {
      if (place !== undefined && frameTime() === putIn) {
        putBack(place, scroller())
      }
    })

    // On the root, sized to the viewport by `base.css` in both layouts and
    // never by the page: an observed box the page resizes reports, from
    // inside another observer's callback, a loop the browser cannot deliver.
    const observer = new ResizeObserver(() => {
      resizing = false
      seen = size()
      // A turn keeps the scroller, but iOS sets a scroll of its own for it
      // by the resize event, and the browser may anchor the page as well.
      // The put-back moves the scroll by how far the anchor stands from its
      // place, so it lands the same from whatever scroll those leave.
      const now = facingNow()
      const turned = now !== facing
      facing = now
      const at = scroller()
      if (place === undefined) place = placeIn(at)
      else if (place.scroller !== at || turned) {
        if (place.scroller !== at) place.below = 0
        putBack(place, at)
      }
    })
    // In a frame the resize event can come after the observer, which has
    // seen the size by then. A height alone, as a toolbar's, can leave the
    // root's box as it was, so no observer runs: the next frame clears the
    // flag, after every callback of this one has run, unless it resizes too.
    const resized = () => {
      if (size() === seen) return
      resizing = true
      cancelAnimationFrame(settle)
      settle = requestAnimationFrame(() => {
        settle = requestAnimationFrame(() => {
          settle = 0
          resizing = false
        })
      })
    }

    let frame = 0
    /** Set by a step in the covers' size, until the place is read again. */
    let stepped = false
    const mark = () => {
      frame = 0
      if (resizing) return
      const at = scroller()
      // A swap is the observer's to put back before a place is taken in it.
      if (place !== undefined && place.scroller !== at) return
      // Drawn where it was kept, the place is not read again: read in the
      // other layout it can be another cover of its row, or a clamped end,
      // and the way back would land elsewhere. A step in the covers' size
      // is the reader's, and what it leaves at the top is the place.
      if (place === undefined || stepped || !drawnAsKept(place, at)) {
        place = placeIn(at)
      }
      stepped = false
    }
    const later = () => {
      if (frame === 0) frame = requestAnimationFrame(mark)
    }
    const changed = new MutationObserver((records) => {
      if (records.some(({ type }) => type === 'attributes')) stepped = true
      later()
    })

    observer.observe(document.documentElement)
    changed.observe(content, { childList: true, subtree: true })
    // The covers' size, which the shell sets on `main`'s parent's style
    // (`--cover-n`): a step moves no scroll and draws no child where the
    // page does not anchor, so nothing else would end the pairing.
    if (content.parentElement !== null) {
      changed.observe(content.parentElement, {
        attributes: true,
        attributeFilter: ['style'],
      })
    }
    document.addEventListener('scroll', later, { capture: true, passive: true })
    window.addEventListener('resize', resized)
    return () => {
      observer.disconnect()
      changed.disconnect()
      document.removeEventListener('scroll', later, { capture: true })
      window.removeEventListener('resize', resized)
      offRowLaid()
      cancelAnimationFrame(frame)
      cancelAnimationFrame(settle)
    }
  }, [main])
}
