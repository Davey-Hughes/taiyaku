// Shared by the browser tests that drive the page with the real mouse
// outside `act`, as the app meets its inputs.
import { mouse } from './mouse'

/**
 * Real inputs, and the page's answer to them, as the app meets them:
 * outside `act`, which would draw what each update set only as it ended,
 * and with React told so, which otherwise warns of every update outside it.
 * Two frames let the last update's effects run before `act` is the rule
 * again.
 */
export async function byHand(run: () => Promise<unknown>) {
  globalThis.IS_REACT_ACT_ENVIRONMENT = false
  try {
    await run()
    for (let frame = 0; frame < 2; frame++) {
      await new Promise((resolve) => requestAnimationFrame(resolve))
    }
  } finally {
    globalThis.IS_REACT_ACT_ENVIRONMENT = true
  }
}

/**
 * Waits until `holds`, outside `act` (`byHand`), where `settleUntil`'s
 * own `act` would be warned of; throws naming `what` after 2 s.
 */
export async function until(what: string, holds: () => boolean) {
  const end = performance.now() + 2000
  while (!holds()) {
    if (performance.now() > end) {
      throw new Error(`${what}: not drawn within 2000 ms`)
    }
    await new Promise((resolve) => setTimeout(resolve, 20))
  }
}

/** The real mouse to the middle of `el`. */
export function pointAt(el: Element) {
  const box = el.getBoundingClientRect()
  return mouse.moveTo(box.left + box.width / 2, box.top + box.height / 2)
}

/** A real press and release of the mouse's button at the middle of `el`. */
export async function clickAt(el: Element) {
  await pointAt(el)
  await mouse.press()
  await mouse.release()
}
