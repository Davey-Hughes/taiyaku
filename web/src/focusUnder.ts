/**
 * Whether the control with focus is drawn under `cover`, and is marked as
 * focused there (`:focus-visible`), so that its ring is drawn on `cover`
 * (`settings.css`): a callout, or a library page's link line, which lies
 * over what is above it as a callout does.
 */
export function focusUnder(cover: Element): boolean {
  const focused = document.activeElement
  if (focused === null || !focused.matches(':focus-visible')) return false
  const over = cover.getBoundingClientRect()
  const at = focused.getBoundingClientRect()
  const x = (at.left + at.right) / 2
  const y = (at.top + at.bottom) / 2
  return x > over.left && x < over.right && y > over.top && y < over.bottom
}
