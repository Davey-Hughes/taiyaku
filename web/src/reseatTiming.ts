/**
 * What the library lists' reseat (`reseat.ts`) and the Continue row's
 * (`continueRow.ts`) share, so that a column of covers and the card above
 * it move as one: the same length, the same curve and the same clock;
 * whether a phone turned on its side moves them; and what tells the lists
 * they have moved without resizing.
 */

/** How long a reseat lasts. */
export const RESEAT_MS = 180

/**
 * A fast-out curve (a quartic ease-out): nine tenths of the way in under
 * half the time, so a cover reads as following the window rather than
 * trailing it, and a soft landing so it does not stop dead.
 * `reseatEased` below evaluates the same curve for the Continue row, which
 * is laid frame by frame rather than animated, so the two must change
 * together.
 */
export const RESEAT_EASING = 'cubic-bezier(0.25, 1, 0.5, 1)'

/**
 * How far past the viewport a cover still moves: one that starts or ends
 * there is seen arriving or leaving. Beyond it a cover snaps.
 */
export const NEAR_PX = 200

/** A move smaller than this, in px, is not read as a move, and snaps. */
export const SNAP_PX = 2

function cubicBezier(x1: number, y1: number, x2: number, y2: number) {
  const cx = 3 * x1
  const bx = 3 * (x2 - x1) - cx
  const ax = 1 - cx - bx
  const cy = 3 * y1
  const by = 3 * (y2 - y1) - cy
  const ay = 1 - cy - by
  const xAt = (t: number) => ((ax * t + bx) * t + cx) * t
  const yAt = (t: number) => ((ay * t + by) * t + cy) * t
  return (x: number): number => {
    if (x <= 0) return 0
    if (x >= 1) return 1
    // x(t) rises monotonically on [0, 1] for any x1, x2 in [0, 1], so
    // halving the interval always closes on the one t.
    let low = 0
    let high = 1
    let t = x
    for (let step = 0; step < 30; step++) {
      if (xAt(t) < x) low = t
      else high = t
      t = (low + high) / 2
    }
    return yAt(t)
  }
}

/** `RESEAT_EASING` at progress `progress` (0 to 1). */
export const reseatEased: (progress: number) => number = cubicBezier(
  0.25,
  1,
  0.5,
  1,
)

/**
 * Whether a change may move its covers: not where the system asks for less
 * motion, and not where it cannot be asked (jsdom and node have no
 * `matchMedia`). Asked at each change rather than once at import, so that
 * importing this needs no browser and a setting changed mid-session holds
 * from the next change.
 */
export function mayReseat(): boolean {
  const still = globalThis.matchMedia?.('(prefers-reduced-motion: reduce)')
  return still !== undefined && !still.matches
}

/**
 * Whether a phone turned on its side moves its covers and the Continue row
 * as any other column change does (true), or snaps them into place
 * (false). The one place that is decided, so the build that tries the
 * other way flips only this.
 */
export const TURN_RESEATS: boolean = false

/**
 * Whether the view stands taller than wide, by the `(orientation: …)`
 * media feature; undefined where it cannot be asked. Read by the observer
 * that lays a change, so it is the orientation of the size laid: neither
 * the query's change event nor `screen.orientation`'s need come before
 * that observer in the frame that lays the new size.
 */
export function uprightNow(): boolean | undefined {
  return globalThis.matchMedia?.('(orientation: portrait)').matches
}

/**
 * Whether a change laid `now` upright or not, from a layout laid `was`, is
 * a turn, which snaps unless `TURN_RESEATS`. A desktop window dragged
 * across square turns too, and snaps once.
 */
export function turnSnaps(
  was: boolean | undefined,
  now: boolean | undefined,
): boolean {
  return !TURN_RESEATS && was !== now
}

/**
 * The time this frame is drawn for. The document timeline's time holds
 * still through a frame's work, from `requestAnimationFrame` to the resize
 * observers, so a reseat started anywhere in a frame starts at that frame,
 * and its Web Animations, started at the same time, keep step with it.
 */
export function frameTime(): number {
  const time = globalThis.document?.timeline?.currentTime
  return typeof time === 'number' ? time : performance.now()
}

/**
 * A signal: `on` calls a listener at each `say` until the answer `on` gave
 * is called. A set, not one slot: each attachment adds its own, and one
 * leaving must not take another's with it.
 */
function signal() {
  const listeners = new Set<() => void>()
  return {
    on(listener: () => void): () => void {
      listeners.add(listener)
      return () => {
        listeners.delete(listener)
      }
    },
    say(): void {
      for (const listener of [...listeners]) listener()
    },
  }
}

const laidRow = signal()
const putBack = signal()

/** Calls `listener` at each `rowLaid` until the answer is called. */
export function onRowLaid(listener: () => void): () => void {
  return laidRow.on(listener)
}

/**
 * Said by the Continue row once it has laid itself at a new column, so a
 * new height: the lists below it move without resizing, and their next
 * column change must start from where they are drawn now. Its observer
 * runs after the lists' in a frame, so what they read there is stale.
 */
export function rowLaid(): void {
  laidRow.say()
}

/** Calls `listener` at each `scrollPutBack` until the answer is called. */
export function onScrollPutBack(listener: () => void): () => void {
  return putBack.on(listener)
}

/**
 * Said by the scroller swap (`scrollerSwap.ts`) once it has written the
 * scroll back, from its observer: the lists moved in the view without
 * resizing, and their next column change must start from where they are
 * drawn now.
 */
export function scrollPutBack(): void {
  putBack.say()
}
