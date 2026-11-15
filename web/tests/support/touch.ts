import { cdp } from 'vitest/browser'
import { middleWhereOver } from './callout'

/**
 * A real finger's tap, at a point in the test's frame, outside `act`: a
 * touch pressed and lifted a frame apart, which Chromium turns into the
 * pointer events of a touch at the press and a tap's mouse events and click
 * after the lift, each sent to whatever is drawn topmost there then.
 *
 * The session's coordinates are the browser window's, so a place in the
 * frame is carried through the frame's own box, as `mouse` carries it.
 */
export async function tapAt(x: number, y: number) {
  const point = inWindow(x, y)
  await outsideAct(async () => {
    await cdp().send('Input.dispatchTouchEvent', {
      type: 'touchStart',
      touchPoints: [point],
    })
    await nextFrame()
    await cdp().send('Input.dispatchTouchEvent', {
      type: 'touchEnd',
      touchPoints: [],
    })
    await nextFrame()
  })
}

/**
 * A real finger's swipe, level at `y`, from `from` px past `x` to `to` px
 * past it, outside `act` as `tapAt` is: a touch pressed at the start, moved
 * in steps of 15 px a frame apart, as a hand moves, and lifted at the end.
 * Chromium scrolls whatever the finger lands on, as a phone's would.
 */
export async function swipeAlong(
  x: number,
  y: number,
  from: number,
  to: number,
): Promise<void> {
  await outsideAct(async () => {
    await cdp().send('Input.dispatchTouchEvent', {
      type: 'touchStart',
      touchPoints: [inWindow(x + from, y)],
    })
    await nextFrame()
    const steps = Math.ceil(Math.abs(to - from) / 15)
    for (let step = 1; step <= steps; step++) {
      await cdp().send('Input.dispatchTouchEvent', {
        type: 'touchMove',
        touchPoints: [inWindow(x + from + ((to - from) * step) / steps, y)],
      })
      await nextFrame()
    }
    await cdp().send('Input.dispatchTouchEvent', {
      type: 'touchEnd',
      touchPoints: [],
    })
    await nextFrame()
  })
}

/** A place in the test's frame, in the browser window's coordinates. */
function inWindow(x: number, y: number): { x: number; y: number } {
  const frame = window.frameElement
  if (!frame) throw new Error('this file is not running in a frame')
  const box = frame.getBoundingClientRect()
  return {
    x: box.left + (x * box.width) / window.innerWidth,
    y: box.top + (y * box.height) / window.innerHeight,
  }
}

function nextFrame(): Promise<unknown> {
  return new Promise((resolve) => requestAnimationFrame(resolve))
}

async function outsideAct(send: () => Promise<void>): Promise<void> {
  const global = globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }
  global.IS_REACT_ACT_ENVIRONMENT = false
  try {
    await send()
  } finally {
    global.IS_REACT_ACT_ENVIRONMENT = true
  }
}

/** A real tap at the middle of where `over` lies over `under` (`middleWhereOver`). */
export async function tapWhereOver(over: Element, under: Element) {
  const { x, y } = middleWhereOver(over, under)
  await tapAt(x, y)
}
