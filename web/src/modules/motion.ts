import type { CSSProperties } from 'react'
import type { ReaderModule } from '../module'

export interface MotionPrefs {
  animate: boolean
  ms: number
}

/**
 * Whether `prefs` describes a real transition, as opposed to nothing to
 * animate at all.
 *
 * `transitionStyle` and `fadeStyle` both branch on exactly this condition;
 * this exports it so a caller that only needs the yes/no answer —
 * `Chrome.tsx`'s fade-scheduling effect, for one — asks the preferences
 * directly instead of duck-typing it out of `transitionStyle`'s return shape
 * (checking for a `transition` key), which only happens to work today
 * because of that function's particular return shape and would silently
 * stop working the day it returned something else equally empty.
 */
export function isAnimating(prefs: MotionPrefs): boolean {
  return prefs.animate && prefs.ms > 0
}

/**
 * The page transition, or nothing.
 *
 * Returns an empty object rather than `{ transition: 'none' }` when there is
 * nothing to animate — not merely as a stylistic choice, but because the two
 * are not equivalent. `transition: 'none'` is an inline style, and inline
 * styles win the cascade over any stylesheet rule, including a
 * `prefers-reduced-motion` media query that also sets `transition: none`. If
 * this function ever wrote that inline value, it would silence the reader's
 * own OS-level motion preference rather than defer to it. An empty object
 * leaves the `transition` property entirely undeclared on the element, so
 * there is nothing inline to win the cascade and the stylesheet's
 * `prefers-reduced-motion` rule (or its absence) governs unopposed. A `ms` of
 * zero is treated the same way for the same reason: a `0ms` transition is
 * still a declared `transition` property, not the absence of one.
 */
export function transitionStyle(prefs: MotionPrefs): CSSProperties {
  if (!isAnimating(prefs)) return {}
  return { transition: `opacity ${prefs.ms}ms ease` }
}

/**
 * The style `Chrome.tsx` merges onto the strip and the readout row: the
 * transition declaration plus the opacity it animates between, or nothing at
 * all when `prefs` isn't animating — the same empty-object case
 * `transitionStyle` reports, so a reader with animation off never gets an
 * `opacity` written onto an element that has no `transition` to animate it.
 *
 * `shown` is `Chrome`'s own fade-settled flag — whether the frame
 * `scheduleFade` schedules has fired yet. This function only turns that flag
 * into the CSS that makes it visible; deciding *when* it flips is
 * `scheduleFade`'s job, not this one's.
 */
export function fadeStyle(prefs: MotionPrefs, shown: boolean): CSSProperties {
  if (!isAnimating(prefs)) return {}
  return { ...transitionStyle(prefs), opacity: shown ? 1 : 0 }
}

/**
 * Drive `Chrome`'s fade-in: decide whether to settle its `shown` state
 * immediately or schedule the frame that flips it, and hand back the cleanup
 * a caller's effect must run before scheduling again.
 *
 * Extracted out of `Chrome.tsx`'s own `useEffect`, which used to inline this
 * branch with nothing able to reach it — the same argument `direction.ts`'s
 * `setOverride` doc makes about `App.tsx`: a reordered dependency or a
 * flipped condition here breaks quietly, every existing test still green,
 * because none of them import a `.tsx` file. Nothing here touches the DOM —
 * only `requestAnimationFrame`/`cancelAnimationFrame`, both stubbable on
 * `globalThis` in plain Vitest — so extracting it costs nothing and buys a
 * test for exactly the kind of edit that breaks it.
 *
 * Hidden, or not animating: settle `shown` to `!animating` at once, no frame
 * scheduled — there is nothing on screen for a frame to prepare. Visible and
 * animating: schedule one frame to flip `shown` to `true`, so the caller's
 * first paint lands at `opacity: 0` and this frame is what gives the
 * already-declared `transition` something to animate from.
 *
 * The returned cleanup cancels that scheduled frame. A caller that runs it
 * from a `useEffect`'s own cleanup gets a rapid show/hide/show toggle handled
 * correctly for free: React calls the previous effect's cleanup before
 * running the next one, so a frame from a toggle the reader has already
 * moved past is cancelled rather than left to fire later and flip `shown`
 * against whatever state is current by then.
 */
export function scheduleFade(
  visible: boolean,
  animating: boolean,
  setShown: (shown: boolean) => void,
): () => void {
  if (!visible || !animating) {
    setShown(!animating)
    return () => {}
  }
  const frame = requestAnimationFrame(() => setShown(true))
  return () => cancelAnimationFrame(frame)
}

export const motionModule: ReaderModule<MotionPrefs> = {
  id: 'motion',
  label: 'Motion',
  defaults: { animate: true, ms: 120 },
}
