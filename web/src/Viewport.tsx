import type { ReactNode } from 'react'
import type { Rect } from './viewport'

/**
 * The group, scaled and translated so `rect` fills the box around it.
 *
 * One CSS transform on one wrapper, deliberately, rather than resizing the
 * images underneath. Two things fall out of that choice and both are
 * load-bearing:
 *
 * 1. **`PageView` is untouched.** Its sizing — `modules/fit.ts`'s `pageStyle`
 *    since the `fit` module shipped, a fixed `fitWhole` constant before it —
 *    along with its decode gate and its buffer-swap race fix all keep
 *    working, because none of them is aware of an ancestor's transform.
 * 2. **Every existing measurement stays correct.** `App.tsx`'s `pointOn` and
 *    `PageAnnotations` both measure through `getBoundingClientRect()`, which
 *    reports post-transform geometry, so a region drawn under zoom lands where
 *    the pointer was and a marker sits over the art — with no change to
 *    `paintedBox`, `pointInSlot` or anything else in `notes.ts`.
 *
 * The second point holds only because this transform is pure scale and
 * translate. Nothing here rotates or skews, and nothing should start:
 * `paintedBox`'s arithmetic assumes an axis-aligned box.
 *
 * This is why `Rect` carries the `w === h` invariant documented on that
 * type: `scale(1/rect.w, 1/rect.h)` is only ever a uniform scale — never a
 * stretch — when the two match.
 *
 * `transform-origin: 0 0` so the translate is in the untransformed box's own
 * units and the two terms compose in the obvious order. With the default
 * centre origin, the translate needed to bring `rect.x` to the left edge
 * depends on the scale, which is one more thing to get wrong for no gain.
 *
 * A zero-sized rect would divide by zero. Nothing in the current call graph
 * feeds one here — all five producers of that state are safe: `WHOLE` is 1×1,
 * `zoomAt` floors both axes above zero for any finite `maxScale`, `panBy` does
 * not touch the sizes, `clampRect` on a turn cannot grow a zero it was not
 * given, and `mapView` ends in `fitRect` guarded on its aspect. But that is a
 * property of today's callers, not of `clampRect`, which sanitises nothing:
 * hand it a zero-sized rect and it hands one back. So this guards rather than
 * relying on an induction that any new caller could break, and the guard is
 * what stops a page being painted at infinite scale on whichever path reaches
 * it first.
 */
export function Viewport({
  rect,
  children,
}: {
  rect: Rect
  children: ReactNode
}) {
  const scaleX = rect.w > 0 ? 1 / rect.w : 1
  const scaleY = rect.h > 0 ? 1 / rect.h : 1
  return (
    <div
      style={{
        width: '100%',
        height: '100%',
        transformOrigin: '0 0',
        transform: `scale(${scaleX}, ${scaleY}) translate(${-rect.x * 100}%, ${-rect.y * 100}%)`,
        // A compositing hint only. What keeps a scaled page off the chrome is
        // the page area's own `overflow: hidden`, already set in `App.tsx` for
        // every mode but scroll.
        willChange: 'transform',
      }}
    >
      {children}
    </div>
  )
}
