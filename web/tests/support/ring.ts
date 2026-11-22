import { expect } from 'vitest'

/**
 * Where `el`'s focus ring is drawn: its border box grown by the ring's
 * reach, `outline-offset` plus `outline-width`, read off the element.
 * Asserts first that `el` is keyboard-focused and the ring drawn: an
 * unfocused element computes a width with no ring.
 */
export function ringOf(el: Element): {
  left: number
  top: number
  right: number
  bottom: number
} {
  expect(el.matches(':focus-visible'), 'the element shows keyboard focus').toBe(
    true,
  )
  const style = getComputedStyle(el)
  expect(style.outlineStyle, 'the ring is drawn').toBe('solid')
  const reach =
    Number.parseFloat(style.outlineOffset) +
    Number.parseFloat(style.outlineWidth)
  const box = el.getBoundingClientRect()
  return {
    left: box.left - reach,
    top: box.top - reach,
    right: box.right + reach,
    bottom: box.bottom + reach,
  }
}
