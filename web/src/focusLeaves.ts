import type { FocusEvent } from 'react'

/**
 * Whether focus leaving the element `e` is on goes somewhere outside it: to
 * another control, or to the page. Not when the window itself loses focus,
 * as when the person turns to another window, since focus comes back where
 * it was.
 */
export function focusLeaves(e: FocusEvent<Element>): boolean {
  const to = e.relatedTarget
  if (to === null) return document.hasFocus()
  return !e.currentTarget.contains(to)
}
