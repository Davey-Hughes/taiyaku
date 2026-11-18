import { expect } from 'vitest'
import { frames } from './motion'
import { mouse } from './mouse'

/** A real click at `x`, `y` in the frame, with no scroll to a target first. */
export async function clickAt(x: number, y: number) {
  await mouse.moveTo(x, y)
  await mouse.press()
  await mouse.release()
}

/**
 * A real click at the middle of the part of `el` in the window: a
 * locator's click would first scroll the page to show all of it.
 */
export async function clickShown(el: Element) {
  const box = el.getBoundingClientRect()
  const x = box.left + box.width / 2
  const y = (Math.max(0, box.top) + Math.min(innerHeight, box.bottom)) / 2
  const hit = document.elementFromPoint(x, y)
  expect(hit !== null && el.contains(hit), 'the click lands on it').toBe(true)
  await clickAt(x, y)
}

/** Frames until the phone bar rests, ten at most; whether it does. */
export async function barRested(host: ParentNode): Promise<boolean> {
  const bar = host.querySelector('.phone-bar')
  if (bar === null) throw new Error('no .phone-bar')
  for (let frame = 0; frame < 10 && !bar.hasAttribute('data-at-rest'); frame++)
    await frames(1)
  return bar.hasAttribute('data-at-rest')
}
