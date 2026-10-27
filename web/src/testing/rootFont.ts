import { onTestFinished } from 'vitest'

/**
 * The root's font at `size` for this test, the browser's own where empty:
 * what a larger default text size does to every rem, while a `px`
 * breakpoint stays where it is.
 */
export function rootFontAt(size: string) {
  const root = document.documentElement
  onTestFinished(() => {
    root.style.fontSize = ''
  })
  root.style.fontSize = size
}
