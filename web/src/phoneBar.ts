/**
 * The phone bar is sticky inside a track (`.phone-bar-track`), a box at the
 * page's top `E` px tall, in a clip as tall as the page (`shell.css`), so
 * the page's scroll range is always its own. A sticky box cannot leave its
 * containing block, so the bar is pinned while the scroll is at most `E - H`
 * and past that goes up with the page, moved by the compositor in step with
 * the scroll, where a transform written from script would trail iOS's
 * scroll. Script moves only `E` (`trackEnd`); it also rests the bar in the
 * page whenever the page is still, holds the share it shows through a turn,
 * and holds the bar whole while the drawer is open or keyboard focus is in
 * it.
 */
import { type RefObject, useEffect, useRef } from 'react'
import { PHONE_LAYOUT } from './phoneLayout'
import { onScrollPutBack } from './reseatTiming'
import { onLanded } from './scrollKeeper'

/**
 * No scroll for this long ends one, where `scrollend` is missing, and after a
 * wheel or a touch that moves no page.
 */
export const REST_IDLE_MS = 150
/** A turn holds the bar's shown share until this long after its last event. */
export const TURN_HOLD_MS = 600

/** Set while the track runs, in the phone layout: the line `barLine` gives. */
let lineNow: (() => number | null) | null = null

/**
 * The view's top line for the keeper (`scrollerSwap.ts`): the bar's shown
 * part as of the last scroll seen here, or, at rest or through a turn, the
 * share held, or more near the page's top, where the bar is whole at the
 * scroll as it is now; 0 out of view, and null while the track is stopped:
 * beside the sidebar, and where the hook is not running. The bar's box can
 * be stale by then: iOS moves the scroll at a turn before any event here.
 */
export function barLine(): number | null {
  return lineNow === null ? null : lineNow()
}

const clamp = (v: number, lo: number, hi: number) =>
  Math.min(Math.max(v, lo), hi)

/**
 * The track's end after a scroll to `y`: held while the bar slides out or
 * in, then following `y` once the bar is out of view and `y + height` once
 * it is fully in, never under `height`, so the bar is whole at the top. `y`
 * is clamped first, so iOS's rubber band past either end counts for nothing.
 */
export function trackEnd(
  end: number,
  y: number,
  height: number,
  maxScroll: number,
): number {
  const at = clamp(y, 0, maxScroll)
  return Math.max(height, clamp(end, at, at + height))
}

/** The share of the bar in view, from 0 to 1, with the track ending at `end`. */
export function shownShare(end: number, y: number, height: number): number {
  return clamp((end - y) / height, 0, 1)
}

function maxScroll(): number {
  return Math.max(0, document.documentElement.scrollHeight - innerHeight)
}

/** The clamped scroll: iOS's rubber band at either end reads as none. */
function yNow(): number {
  return clamp(scrollY, 0, maxScroll())
}

/**
 * Calls `turn` at each turn, and `resized` after it at each resize; the
 * answer stops it.
 */
function watchTurns(
  turn: () => void,
  resized: () => void,
): {
  widthChanged: () => boolean
  stop: () => void
} {
  let width = innerWidth
  const widthChanged = () => {
    if (innerWidth === width) return false
    width = innerWidth
    return true
  }
  const onResize = () => {
    if (widthChanged()) turn()
    resized()
  }
  const onOrientation = () => {
    width = innerWidth
    turn()
  }
  addEventListener('resize', onResize)
  addEventListener('orientationchange', onOrientation)
  return {
    widthChanged,
    stop: () => {
      removeEventListener('resize', onResize)
      removeEventListener('orientationchange', onOrientation)
    },
  }
}

/**
 * Runs the track while `phone` matches, starting with the bar shown. A
 * window leaving the phone layout resizes before the list's `change` stops
 * this, and there the bar has no height to take a share of, so every path
 * that reads one checks `phone` first.
 */
function trackBar(
  el: HTMLElement,
  trackEl: HTMLElement,
  phone: MediaQueryList,
  open: RefObject<boolean>,
  reveal: RefObject<(() => void) | null>,
): () => void {
  const H = () => el.offsetHeight
  const hasScrollEnd = 'onscrollend' in window
  let lastY = yNow()
  let E = Math.max(H(), lastY + H())
  let atRest = false
  /** The shown share kept through a turn, or at rest. */
  let heldF = 1
  let turning = false
  let turnTimer = 0
  let idle = 0
  /** When a wheel, a touch's move or a key last came: `performance.now()`. */
  let inputAt = Number.NEGATIVE_INFINITY
  let touching = false
  /** Whether a wheel, a touch or a key began the scroll under way. */
  let gesture = false

  const shareAt = (y: number) => shownShare(E, y, H())
  // Keyboard focus only: the drawer hands focus back to ☰ on each close,
  // a tap's close included, so any focus would hold the bar for good once
  // a touch reader had used the drawer.
  const keyboardFocusIn = () => {
    const focused = document.activeElement
    return (
      focused !== null &&
      el.contains(focused) &&
      focused.matches(':focus-visible')
    )
  }
  const writeE = () => {
    trackEl.style.height = `${E}px`
  }
  lineNow = () => {
    if (!phone.matches) return null
    // The bar is never above the page's top, so near the top more of it is
    // in view than the share held: the line is its foot at the scroll as it
    // is now, which the keeper's put-back may have just moved.
    if (atRest || turning) return Math.max(heldF * H(), H() - yNow())
    return shareAt(lastY) * H()
  }

  /** Puts the bar's held share back against the scroll as it is now. */
  const reapply = () => {
    const y = yNow()
    const h = H()
    E = Math.max(h, y + heldF * h)
    writeE()
    if (atRest) el.style.setProperty('--rest-top', `${E - h}px`)
    lastY = y
  }

  /**
   * Makes the bar part of the page at the pixels it is drawn at, wherever
   * the page is still: a scroll's end, each show, the track's start. The
   * scroll it rests at counts as the last seen, so the scroll event that a
   * landing's write brings after it leaves the bar at rest.
   */
  const toRest = () => {
    if (atRest || turning || !phone.matches) return
    const y = yNow()
    heldF = shareAt(y)
    el.style.setProperty('--rest-top', `${Math.min(y, E - H())}px`)
    el.setAttribute('data-at-rest', '')
    atRest = true
    lastY = y
  }
  const fromRest = () => {
    if (!atRest || turning) return
    const h = H()
    const top = Number.parseFloat(el.style.getPropertyValue('--rest-top'))
    E = trackEnd(top + h, scrollY, h, maxScroll())
    writeE()
    el.removeAttribute('data-at-rest')
    atRest = false
  }

  const turn = () => {
    if (!phone.matches) return
    if (!turning) {
      if (!atRest) heldF = shareAt(lastY)
      turning = true
    }
    clearTimeout(turnTimer)
    turnTimer = window.setTimeout(() => {
      reapply()
      turning = false
      toRest()
    }, TURN_HOLD_MS)
  }
  // iOS sets its own scroll with a turn's new size, before any scroll event
  // and ahead of the frame's observers, and can paint a frame there, so the
  // bar is re-placed at each resize while turning: one whose width the
  // `orientationchange` before it has already taken too.
  const turns = watchTurns(turn, () => {
    if (phone.matches && turning) reapply()
  })

  const onScroll = () => {
    if (!phone.matches) return
    if (turns.widthChanged()) turn()
    if (turning) {
      reapply()
      return
    }
    if (atRest) {
      // A scroll that left the page where it was is not the reader's.
      if (yNow() === lastY) return
      fromRest()
    }
    const h = H()
    const max = maxScroll()
    const y = clamp(scrollY, 0, max)
    E = open.current || keyboardFocusIn() ? y + h : trackEnd(E, y, h, max)
    writeE()
    lastY = y
    // A scroll a wheel, a touch or a key began is the reader's until its
    // `scrollend`, the glide after a finger lifts included, whose events can
    // come further apart than the wait. One nothing began, such as the
    // browser's clamp of a page grown shorter, ends with no `scrollend` in
    // Chromium, so the bar rests once the page has been still for the wait;
    // so does every scroll where `scrollend` is missing.
    clearTimeout(idle)
    if (touching || performance.now() - inputAt <= REST_IDLE_MS) gesture = true
    if (!hasScrollEnd || !gesture)
      idle = window.setTimeout(toRest, REST_IDLE_MS)
  }
  const onScrollEnd = () => {
    gesture = false
    toRest()
  }
  const onTouch = (event: TouchEvent) => {
    touching = event.touches.length > 0
  }
  // Leaves rest before the first scrolled frame. A wheel or a touch can move
  // only something inside the page, such as the Continue row sideways, and
  // then no page scroll ends to rest the bar again: the wait does.
  const onInput = () => {
    inputAt = performance.now()
    fromRest()
    clearTimeout(idle)
    idle = window.setTimeout(toRest, REST_IDLE_MS)
  }
  // A show moves the bar with no scroll, so no scroll's end rests it: it
  // rests here, under the open drawer too, and stays so once that closes.
  // During a turn the bar stays as it is, at rest or not, and the whole bar
  // becomes the share the turn holds and re-places.
  const show = () => {
    if (atRest) fromRest()
    heldF = 1
    reapply()
    toRest()
  }
  reveal.current = show
  // Chromium scrolls a control Tab reaches out of view to the window's
  // middle before it says `focusin`: the reader moved focus, not the page,
  // so the page goes back to where the key found it and the bar comes in.
  // Kept only until the key's focus move is over, the next frame, so no
  // other focus puts anything back, a navigation's landing above all.
  let tabFrom: number | null = null
  let tabFrame = 0
  const onKeyDown = (event: KeyboardEvent) => {
    inputAt = performance.now()
    if (event.key !== 'Tab') return
    tabFrom = yNow()
    cancelAnimationFrame(tabFrame)
    tabFrame = requestAnimationFrame(() => {
      tabFrom = null
    })
  }
  const onFocusIn = () => {
    if (!keyboardFocusIn()) return
    if (tabFrom !== null && yNow() !== tabFrom) scrollTo(scrollX, tabFrom)
    tabFrom = null
    show()
  }
  // A kept scroll can land after its page is drawn, once a wait is over, and
  // would carry out a bar the page's change had shown.
  const offLanded = onLanded(show)

  // After the keeper's own observer, which was made first: a turn's size is
  // seen there and the bar re-placed here, before the frame is painted.
  const resized = new ResizeObserver(() => {
    if (!phone.matches) return
    if (turns.widthChanged()) turn()
    if (turning || atRest) reapply()
  })
  // A page that shrinks in place has its scroll clamped in layout, and the
  // clamp's scroll event comes after that frame is drawn, which would draw a
  // resting bar where the page moved from, or cut off by the clip. The
  // track's clip is as tall as the page (`shell.css`), so for a shrink laid
  // out before the frame's observers run, the scroll's handling runs here
  // before the paint; through a turn it re-places the share held. Shrinks
  // heard late or not at all: O337's known limits in `docs/notes.md`. After
  // the observer above, so a turn is seen first.
  const pageResized = new ResizeObserver(() => {
    if (yNow() !== lastY) onScroll()
  })
  // From the next frame: a start from the keeper's put-back is inside the
  // keeper's own delivery, where a new observation of the root cannot be
  // delivered and the browser reports a loop.
  const observing = requestAnimationFrame(() => {
    resized.observe(document.documentElement)
    const clip = trackEl.parentElement
    if (clip !== null) pageResized.observe(clip)
  })
  const offPutBack = onScrollPutBack(() => {
    if (!phone.matches) return
    // The keeper moved the scroll; the bar keeps what it showed.
    if (!turning && !atRest) heldF = shareAt(lastY)
    reapply()
  })

  writeE()
  toRest()
  addEventListener('scroll', onScroll, { passive: true })
  addEventListener('scrollend', onScrollEnd)
  addEventListener('wheel', onInput, { passive: true })
  addEventListener('touchmove', onInput, { passive: true })
  for (const type of ['touchstart', 'touchend', 'touchcancel'] as const)
    addEventListener(type, onTouch, { passive: true })
  addEventListener('keydown', onKeyDown, { capture: true })
  el.addEventListener('focusin', onFocusIn)
  return () => {
    removeEventListener('scroll', onScroll)
    removeEventListener('scrollend', onScrollEnd)
    removeEventListener('wheel', onInput)
    removeEventListener('touchmove', onInput)
    for (const type of ['touchstart', 'touchend', 'touchcancel'] as const)
      removeEventListener(type, onTouch)
    removeEventListener('keydown', onKeyDown, { capture: true })
    el.removeEventListener('focusin', onFocusIn)
    cancelAnimationFrame(tabFrame)
    turns.stop()
    cancelAnimationFrame(observing)
    resized.disconnect()
    pageResized.disconnect()
    offPutBack()
    offLanded()
    clearTimeout(turnTimer)
    clearTimeout(idle)
    reveal.current = null
    lineNow = null
    el.removeAttribute('data-at-rest')
    el.style.removeProperty('--rest-top')
    trackEl.style.removeProperty('height')
  }
}

/** Where the shell's page is drawn, and whether a replace took it there. */
export interface PageAt {
  readonly path: string
  readonly search: string
  readonly replace: boolean
}

/**
 * Moves the phone bar with the page, for the bar and the track around it;
 * called once, by the shell. It writes to the DOM from its listeners,
 * observers, timers and effects, never through a render, and does nothing
 * where `matchMedia` or `ResizeObserver` is missing. Beside the sidebar the
 * bar is not drawn, so the track runs only in the phone layout, shown again
 * on each return to it. An open drawer shows the bar whole, and so does a
 * new page: a new address without its fragment, as a jump to an anchor is a
 * scroll, and not a replace on the same path, which refines the page shown.
 */
export function usePhoneBar(
  bar: RefObject<HTMLElement | null>,
  track: RefObject<HTMLElement | null>,
  drawerOpen: boolean,
  page: PageAt,
): void {
  const { path, search, replace } = page
  const open = useRef(drawerOpen)
  open.current = drawerOpen
  /** Set while the track runs: shows the bar whole. */
  const reveal = useRef<(() => void) | null>(null)

  useEffect(() => {
    if (
      typeof matchMedia === 'undefined' ||
      typeof ResizeObserver === 'undefined'
    ) {
      return
    }
    const el = bar.current
    const trackEl = track.current
    if (el === null || trackEl === null) return
    const phone = matchMedia(PHONE_LAYOUT)
    let stop: (() => void) | null = null
    // The list's `change` can come a frame after the keeper's put-back,
    // whose scroll would carry away a bar whose track has only the CSS's
    // height, none set here yet, so the put-back looks at the list too.
    const follow = () => {
      if (phone.matches === (stop !== null)) return
      stop?.()
      stop = phone.matches ? trackBar(el, trackEl, phone, open, reveal) : null
    }
    follow()
    phone.addEventListener('change', follow)
    const offPutBack = onScrollPutBack(follow)
    return () => {
      phone.removeEventListener('change', follow)
      offPutBack()
      stop?.()
    }
  }, [bar, track])

  useEffect(() => {
    if (drawerOpen) reveal.current?.()
  }, [drawerOpen])

  const lastPath = useRef<string | null>(null)
  // biome-ignore lint/correctness/useExhaustiveDependencies: the address is the trigger; `replace` is read with the address it came with.
  useEffect(() => {
    const before = lastPath.current
    lastPath.current = path
    // A key typed into the search box, a chip, the sort or an edition: the
    // reader is still on the page, where the bar is the scroll's to move.
    if (replace && before === path) return
    reveal.current?.()
  }, [path, search])
}
