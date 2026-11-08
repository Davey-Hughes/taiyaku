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
  const frame = window.frameElement
  if (!frame) throw new Error('this file is not running in a frame')
  const box = frame.getBoundingClientRect()
  const point = {
    x: box.left + (x * box.width) / window.innerWidth,
    y: box.top + (y * box.height) / window.innerHeight,
  }
  const global = globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }
  global.IS_REACT_ACT_ENVIRONMENT = false
  try {
    await cdp().send('Input.dispatchTouchEvent', {
      type: 'touchStart',
      touchPoints: [point],
    })
    await new Promise((resolve) => requestAnimationFrame(resolve))
    await cdp().send('Input.dispatchTouchEvent', {
      type: 'touchEnd',
      touchPoints: [],
    })
    await new Promise((resolve) => requestAnimationFrame(resolve))
  } finally {
    global.IS_REACT_ACT_ENVIRONMENT = true
  }
}

/** A real tap at the middle of where `over` lies over `under` (`middleWhereOver`). */
export async function tapWhereOver(over: Element, under: Element) {
  const { x, y } = middleWhereOver(over, under)
  await tapAt(x, y)
}
