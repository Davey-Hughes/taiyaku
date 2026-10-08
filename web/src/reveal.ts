/**
 * Whether the head of `el` is in the window, below whatever sticks to its top.
 * `scroll-margin-top` is where the stylesheet says how much that is.
 */
export function inView(el: HTMLElement): boolean {
  const at = el.getBoundingClientRect()
  const clear = Number.parseFloat(getComputedStyle(el).scrollMarginTop) || 0
  return (
    at.top >= clear - 1 &&
    at.top + Math.min(at.height, 48) <= window.innerHeight
  )
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
 * Scrolls `el` just far enough to be seen, unless its head already is or
 * there is no `el` yet (a ref not set). For a sentence beneath the control
 * it answers: the control stays in view above it, where `reveal`'s
 * top-of-the-view would push it out.
 */
export function bringIntoView(el: HTMLElement | null) {
  if (el !== null && !inView(el)) el.scrollIntoView?.({ block: 'nearest' })
}
