/**
 * Which of several finished decodes is allowed to be drawn.
 *
 * **Why this is a module rather than two lines inside `PageView`.** A flick
 * shows intermediate frames — `decode.ts` presents a superseded decode when
 * nothing newer is queued, because it is still the freshest picture that
 * exists — and that is only safe if a decode which finished *late* can never
 * paint over one that finished after it. `presentWhenDecoded` cannot enforce
 * that: it sees opaque `src` strings and can tell "current" from "not current"
 * but never "newer" from "older". Only the component issuing the requests knows
 * their order.
 *
 * Inside `PageView`, a component Vitest cannot reach, the one property keeping
 * the flick safe would rest on a browser measurement and nothing else. A
 * module of its own is what makes it testable, which is the whole reason this
 * file exists.
 *
 * **The hazard it closes is real, not theoretical.** `createDecodeQueue` runs
 * up to `maxConcurrent` jobs at once; three decodes in flight settle in
 * whatever order the browser finishes them, not the order they were asked for.
 * Without this, a flick could land on page 29 and then have page 21's decode
 * arrive and overwrite it.
 */
export interface Ordering {
  /** The number the next request takes. Strictly increasing, never reused. */
  next(): number
  /**
   * Draw `value` through `show`, unless something newer has already been drawn.
   *
   * Returns whether it drew, which is what a test asserts on and what a caller
   * can use to tell "dropped as stale" from "shown".
   */
  inOrder<T>(attempt: number, value: T, show: (value: T) => void): boolean
}

/**
 * A fresh ordering, starting before any attempt.
 *
 * Zero is "nothing drawn yet" and `next` begins at 1, so the first attempt is
 * always newer than the initial state and can never be dropped — a reader whose
 * very first page decoded slowly must still see it.
 */
export function createOrdering(): Ordering {
  let issued = 0
  let drawn = 0

  return {
    next() {
      issued += 1
      return issued
    },
    inOrder(attempt, value, show) {
      // Strictly older is dropped. Equal cannot arise from `next`, which never
      // repeats a number; treating it as drawable rather than dropped means a
      // caller that supplies its own numbering gets the intuitive answer rather
      // than a silent no-op.
      if (attempt < drawn) return false
      drawn = attempt
      show(value)
      return true
    },
  }
}
