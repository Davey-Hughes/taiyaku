import { afterEach } from 'vitest'
import { cdp } from 'vitest/browser'

/**
 * A real mouse for the browser project, its button held across moves.
 *
 * `userEvent` has a click, a hover and a drag of one element onto another,
 * each a whole gesture, and a drag of the strip has to be stopped part way
 * and read. So these are the three inputs Playwright's own mouse sends
 * Chromium, over the DevTools session Vitest hands a test.
 *
 * The session's coordinates are the browser window's. A test file runs in a
 * frame inside that window, which is drawn smaller than its own viewport
 * where the window is the smaller of the two, so a place in the frame is
 * carried through the frame's own box.
 */
const pointer = { x: 0, y: 0, down: false }

/**
 * The time the next inputs are stamped with, where a case keeps the time
 * itself: `since` is when it began to keep it, in seconds since the epoch
 * as the session takes it, and `ms` how long after that. See `mouse.clock`.
 */
let clock: { since: number; ms: number } | undefined

async function send(type: 'mouseMoved' | 'mousePressed' | 'mouseReleased') {
  const frame = window.frameElement
  if (!frame) throw new Error('this file is not running in a frame')
  const box = frame.getBoundingClientRect()
  await cdp().send('Input.dispatchMouseEvent', {
    type,
    x: box.left + (pointer.x * box.width) / window.innerWidth,
    y: box.top + (pointer.y * box.height) / window.innerHeight,
    button: type === 'mouseMoved' && !pointer.down ? 'none' : 'left',
    buttons: pointer.down ? 1 : 0,
    clickCount: type === 'mouseMoved' ? 0 : 1,
    ...(clock && { timestamp: clock.since + clock.ms / 1000 }),
  })
}

export const mouse = {
  /** To a place in the test's own frame, in its own pixels. */
  async moveTo(x: number, y: number) {
    pointer.x = x
    pointer.y = y
    await send('mouseMoved')
  },
  /** The primary button, where the pointer is. */
  async press() {
    pointer.down = true
    await send('mousePressed')
  },
  async release() {
    pointer.down = false
    await send('mouseReleased')
  },
  /**
   * From `from` pixels past `x` to `to` pixels past it, level at `y`, in
   * steps, as a hand moves: one move would not say whether what is dragged
   * follows all the way or only arrives.
   */
  async along(x: number, y: number, from: number, to: number) {
    const steps = Math.ceil(Math.abs(to - from) / 15)
    for (let step = 1; step <= steps; step++) {
      await mouse.moveTo(x + from + ((to - from) * step) / steps, y)
    }
  },
  /**
   * Stamps the inputs sent from here on with a time the case gives, and not
   * with the time each arrives: `ms` milliseconds after the moment this was
   * first called. With no `ms`, back to the time each arrives, and the next
   * call with one counts from its own moment.
   *
   * For a case about how fast the pointer was going, or how long it had
   * rested. An input's `timeStamp` in the page is otherwise the moment the
   * browser was sent it, so the pointer's speed would be how long this
   * machine took between two sends. Chromium gives an event the time it is
   * handed here, whenever it arrives.
   */
  clock(ms?: number) {
    clock =
      ms === undefined
        ? undefined
        : { since: clock?.since ?? Date.now() / 1000, ms }
  },
}

// A case that fails with the button down would leave it down for the next,
// and one that fails with its own clock set would leave the next stamped by
// it.
afterEach(async () => {
  clock = undefined
  if (pointer.down) await mouse.release()
})
