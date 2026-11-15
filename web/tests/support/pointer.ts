import { expect } from 'vitest'

// Shared by the browser tests that draw the page as a device's pointers
// would have it.

/** A device by its pointers, as `pointerRulesAs` draws the page for it. */
export type Device = 'phone' | 'mouse' | 'touch laptop'

/**
 * Whether each pointer or hover condition the stylesheets ask holds on each
 * device, keyed on the condition as Chromium serialises it, its whitespace
 * removed. A phone's one pointer is coarse and cannot hover. A mouse is fine
 * and hovers. A touch laptop's primary pointer is fine and hovers, and its
 * screen adds a coarse one: `(pointer: coarse)` asks only the primary, while
 * the hiding rule over a volume tile's `i` (`library.css`) asks every one.
 * Beside it, the progress line's press band and its wake (`reader.css`) ask
 * whether any pointer is fine, which a touch laptop's trackpad is.
 */
const HOLDS = new Map<string, Record<Device, boolean>>([
  ['(pointer:coarse)', { phone: true, mouse: false, 'touch laptop': false }],
  [
    '(hover:hover)and(not(any-pointer:coarse))',
    { phone: false, mouse: true, 'touch laptop': false },
  ],
  ['(any-pointer:fine)', { phone: false, mouse: true, 'touch laptop': true }],
])

/**
 * Every `@media` block in the page's stylesheets that asks about the pointer
 * or hover, switched in place to `all` or `not all` as `HOLDS` says it holds
 * on `device`, so the cascade keeps its order; the function returned puts
 * each block's condition back as it was written. This project's Chromium has
 * a fine pointer, and its touch emulation, once turned off again, leaves the
 * page reporting no pointer at all to every file that runs after.
 *
 * A condition `HOLDS` does not name throws before anything is switched: left
 * alone, it would draw as this Chromium's pointer does while the case names
 * another device.
 */
export function pointerRulesAs(device: Device): () => void {
  const found: CSSMediaRule[] = []
  const walk = (list: CSSRuleList) => {
    for (const rule of list) {
      if (
        rule instanceof CSSMediaRule &&
        /pointer|hover/.test(rule.media.mediaText)
      ) {
        found.push(rule)
      }
      if (rule instanceof CSSGroupingRule) walk(rule.cssRules)
    }
  }
  for (const sheet of document.styleSheets) walk(sheet.cssRules)
  const key = (text: string) => text.replace(/\s/g, '')
  const other = found
    .map((rule) => rule.media.mediaText)
    .filter((text) => !HOLDS.has(key(text)))
  if (other.length > 0) {
    throw new Error(
      `cannot switch as a ${device}: @media ${other.join('; @media ')}`,
    )
  }
  const switched = found.map(({ media }) => ({
    media,
    written: media.mediaText,
  }))
  for (const { media, written } of switched) {
    media.mediaText = HOLDS.get(key(written))?.[device] ? 'all' : 'not all'
  }
  return () => {
    for (const { media, written } of switched) media.mediaText = written
  }
}

/** A phone's pointer rules, for the cases that ask only for a coarse one. */
export function coarseRulesOn(): () => void {
  return pointerRulesAs('phone')
}

/** A rectangle in the window's coordinates: where a press lands, or should. */
export type PressArea = {
  top: number
  bottom: number
  left: number
  right: number
  width: number
  height: number
}

/** `rect` as an area, for comparing with one. */
export function areaOf(rect: DOMRectReadOnly): PressArea {
  const { top, bottom, left, right, width, height } = rect
  return { top, bottom, left, right, width, height }
}

/**
 * How far out from the middle of `el`'s first box a press still lands on
 * `el`, walked along its middle row and its middle column by hit testing.
 * So it counts what a pseudo-element adds, since a press on one is a press
 * on its element, and it stops wherever anything else is drawn over the
 * target on those two lines. `el` must be in view.
 *
 * **`elementFromPoint` answers for a pixel, not a point**: what the 1 px
 * square right and down of the point meets, the topmost where it meets two.
 * So a box from `a` to `b` is found from just after `a - 1` to just before
 * `b`, and each edge here is that run's, put back by the pixel. That is
 * checked, once per file, against a box drawn for the purpose (`calibrate`)
 * before anything is measured, so a browser that answers otherwise throws
 * rather than measuring wrong. Edges are found to 1/64 px, the unit
 * Chromium lays out in: a whole pixel at a time out to the first miss, then
 * halved back, so every point asked stays on that grid.
 */
export function pressArea(el: Element): PressArea {
  calibrate()
  return walk(el)
}

let calibrated = false

/**
 * Throws unless `walk` finds a box of known size and place, set at
 * fractions of a pixel, exactly where `getBoundingClientRect` says it is.
 * Drawn fixed and above everything, so nothing on the page covers it.
 */
function calibrate(): void {
  if (calibrated) return
  const probe = document.createElement('div')
  probe.style.cssText =
    'position: fixed; left: 3.3px; top: 5.7px; width: 30.2px; height: 20.6px; z-index: 2147483647'
  document.body.append(probe)
  try {
    const found = walk(probe)
    const box = areaOf(probe.getBoundingClientRect())
    if (JSON.stringify(found) !== JSON.stringify(box)) {
      throw new Error(
        `hit testing finds ${JSON.stringify(found)} for a box at ${JSON.stringify(box)}`,
      )
    }
  } finally {
    probe.remove()
  }
  calibrated = true
}

function walk(el: Element): PressArea {
  const first = el.getClientRects()[0]
  if (first === undefined) throw new Error('nothing drawn to press')
  const unit = 1 / 64
  const snap = (v: number) => Math.round(v / unit) * unit
  const x = snap(first.left + first.width / 2)
  const y = snap(first.top + first.height / 2)
  const lands = (px: number, py: number) => {
    const hit = document.elementFromPoint(px, py)
    return hit !== null && el.contains(hit)
  }
  if (!lands(x, y)) throw new Error('a press at its middle lands elsewhere')
  // The last offset in `dir` from the middle at which `at` still lands.
  const reach = (at: (offset: number) => boolean, dir: 1 | -1) => {
    let inside = 0
    let outside: number = dir
    while (at(outside)) {
      inside = outside
      outside += dir
      if (Math.abs(outside) > 400) throw new Error('a press lands too far out')
    }
    while (Math.abs(outside - inside) > unit) {
      const middle = (inside + outside) / 2
      if (at(middle)) inside = middle
      else outside = middle
    }
    return inside
  }
  const vertical = (offset: number) => lands(x, y + offset)
  const horizontal = (offset: number) => lands(x + offset, y)
  const top = y + reach(vertical, -1) + 1 - unit
  const bottom = y + reach(vertical, 1) + unit
  const left = x + reach(horizontal, -1) + 1 - unit
  const right = x + reach(horizontal, 1) + unit
  return {
    top,
    bottom,
    left,
    right,
    width: right - left,
    height: bottom - top,
  }
}

/**
 * The target a finger should have on `el`: its first box, grown about its
 * middle to 44 px in each direction it is short of that (spec 2026-09-17
 * §7.1). That is both a box floored at 44, which it leaves as it is, and
 * O157's `::after`, which is drawn that way.
 */
export function fingerArea(el: Element): PressArea {
  const first = el.getClientRects()[0]
  if (first === undefined) throw new Error('nothing drawn to press')
  const width = Math.max(first.width, 44)
  const height = Math.max(first.height, 44)
  const left = first.left + (first.width - width) / 2
  const top = first.top + (first.height - height) / 2
  return {
    top,
    bottom: top + height,
    left,
    right: left + width,
    width,
    height,
  }
}

/**
 * A matcher for an area whose every edge is within 0.05 px of `area`'s.
 * That covers, with room to spare, the 1/128 px by which a box centred on
 * an odd number of Chromium's 1/64 px layout units is placed, and is far
 * less than any cover a press could find.
 */
export function near(area: PressArea) {
  return {
    top: expect.closeTo(area.top, 1),
    bottom: expect.closeTo(area.bottom, 1),
    left: expect.closeTo(area.left, 1),
    right: expect.closeTo(area.right, 1),
    width: expect.closeTo(area.width, 1),
    height: expect.closeTo(area.height, 1),
  }
}

/** The area two areas share, 0 where they only meet at an edge. */
export function overlapOf(a: PressArea, b: PressArea): number {
  const across = Math.min(a.right, b.right) - Math.max(a.left, b.left)
  const down = Math.min(a.bottom, b.bottom) - Math.max(a.top, b.top)
  return across > 0 && down > 0 ? across * down : 0
}
