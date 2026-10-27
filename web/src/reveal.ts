/**
 * Whether the head of `el`, or all of it as far as the view holds (`whole`),
 * is in the window, below whatever sticks to its top, and in the box of each
 * element that scrolls it, such as a reader panel that scrolls inside
 * itself, which hides what is outside its box though the window shows that
 * place. `scroll-margin-top` is where the stylesheet says how much sticks to
 * the window's top.
 */
export function inView(el: HTMLElement, whole = false): boolean {
  const at = el.getBoundingClientRect()
  const clear = Number.parseFloat(getComputedStyle(el).scrollMarginTop) || 0
  let top = clear
  let bottom = window.innerHeight
  for (let up = el.parentElement; up !== null; up = up.parentElement) {
    const { overflowY } = getComputedStyle(up)
    if (overflowY === 'auto' || overflowY === 'scroll') {
      const box = up.getBoundingClientRect()
      top = Math.max(top, box.top)
      bottom = Math.min(bottom, box.top + up.clientTop + up.clientHeight)
    }
  }
  const seen = whole
    ? Math.min(at.height, bottom - top)
    : Math.min(at.height, 48)
  return at.top >= top - 1 && at.top + seen <= bottom
}

/**
 * Scrolls `el` to the top of the view unless its head is already in it.
 * Not `block: 'nearest'`: for a report taller than the window that lands on
 * its foot. `scrollIntoView` is optional-chained because jsdom has none.
 */
export function reveal(el: HTMLElement) {
  if (!inView(el)) el.scrollIntoView?.({ block: 'start' })
}

/**
 * Scrolls `el` just far enough to be seen whole, unless it already is or
 * there is no `el` yet (a ref not set); one taller than the view, from its
 * head. For a sentence beneath the control it answers, or laid over the
 * foot of its section: the control stays in view above it, where
 * `reveal`'s top-of-the-view would push it out, and none of its lines is
 * left under the edge of the window or of a panel that scrolls.
 */
export function bringIntoView(el: HTMLElement | null) {
  if (el !== null && !inView(el, true))
    el.scrollIntoView?.({ block: 'nearest' })
}
