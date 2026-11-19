import { cdp } from 'vitest/browser'
import { middleWhereOver } from './callout'

/**
 * A real finger's tap at a point in the test's frame, outside `act`: Chromium
 * sends its pointer events at the press and its mouse events and click after
 * the lift, each to whatever is topmost there then. The point is carried
 * through the frame's own box, as `mouse` carries it.
 *
 * `at`, in seconds since the epoch, stamps the press and the lift 20 ms after:
 * unstamped, the gap between two taps is how long this machine took over them.
 */
export async function tapAt(x: number, y: number, at?: number) {
  await touchDown(x, y, at)
  await touchUp(at === undefined ? undefined : at + 0.02)
}

/** The press of `tapAt`, for a case about what happens before the lift. */
export async function touchDown(x: number, y: number, at?: number) {
  const point = inWindow(x, y)
  await outsideAct(async () => {
    await cdp().send('Input.dispatchTouchEvent', {
      type: 'touchStart',
      touchPoints: [point],
      ...(at === undefined ? {} : { timestamp: at }),
    })
    await nextFrame()
  })
}

/** The lift of `tapAt`. */
export async function touchUp(at?: number) {
  await outsideAct(async () => {
    await cdp().send('Input.dispatchTouchEvent', {
      type: 'touchEnd',
      touchPoints: [],
      ...(at === undefined ? {} : { timestamp: at }),
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
  await swipeBetween([x + from, y], [x + to, y])
}

/** The swipe of `swipeAlong`, from one point in the test's frame to another. */
export async function swipeBetween(
  [fromX, fromY]: [number, number],
  [toX, toY]: [number, number],
): Promise<void> {
  await outsideAct(async () => {
    await cdp().send('Input.dispatchTouchEvent', {
      type: 'touchStart',
      touchPoints: [inWindow(fromX, fromY)],
    })
    await nextFrame()
    const steps = Math.ceil(
      Math.max(Math.abs(toX - fromX), Math.abs(toY - fromY)) / 15,
    )
    for (let step = 1; step <= steps; step++) {
      await cdp().send('Input.dispatchTouchEvent', {
        type: 'touchMove',
        touchPoints: [
          inWindow(
            fromX + ((toX - fromX) * step) / steps,
            fromY + ((toY - fromY) * step) / steps,
          ),
        ],
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
