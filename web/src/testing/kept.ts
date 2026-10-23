import { expect } from 'vitest'

/** `el`'s left, top, width and height, as drawn. */
export function boxOf(el: Element): number[] {
  const at = el.getBoundingClientRect()
  return [at.left, at.top, at.width, at.height]
}

/**
 * Holds, softly, that `after` is `before` to within half a pixel on each of
 * left, top, width and height: what `name` names kept its place and its
 * size.
 */
export function expectKept(name: string, before: number[], after: number[]) {
  expect
    .soft(
      after.map((value, i) => Math.abs(value - before[i]) <= 0.5),
      `${name}: left, top, width, height`,
    )
    .toEqual([true, true, true, true])
}
