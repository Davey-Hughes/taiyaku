import type { ReaderModule } from '../module'

export type Direction = 'rtl' | 'ltr'

/** Overrides are per edition, because editions of one work can differ. */
export interface DirectionPrefs {
  override: Record<string, Direction>
}

/**
 * Which way the pages read.
 *
 * `work.reading` in the sidecar is a fact about the work and is never written
 * back to. An override is a preference about the person reading it, so it lives
 * client-side with the other preferences.
 */
export function effectiveDirection(
  sidecar: Direction,
  override: Direction | undefined,
): Direction {
  return override ?? sidecar
}

/** `directionModule`'s own declared intent string — read here by `App.tsx` rather than duplicated as a literal. */
export const TOGGLE_INTENT = 'direction:toggle'

export const directionModule: ReaderModule<DirectionPrefs> = {
  id: 'direction',
  label: 'Reading direction',
  defaults: { override: {} },
  keys: [
    {
      key: 'd',
      intent: TOGGLE_INTENT,
      label: 'flip this edition between right-to-left and left-to-right',
    },
  ],
}

/**
 * Set one edition's override, leaving every other edition's alone.
 *
 * Extracted out of `App.tsx`'s `d` handler, which used to inline this same
 * merge with no test able to reach it — `App.tsx` is legitimately
 * Vitest-unreachable (see the global no-resolver-in-tests constraint), so a
 * one-line regression here (forgetting to spread `prefs.override` before
 * writing the one key that changed) would have silently wiped every other
 * edition's saved override on the next toggle, caught by nothing.
 */
export function setOverride(
  prefs: DirectionPrefs,
  edition: string,
  direction: Direction,
): DirectionPrefs {
  return { ...prefs, override: { ...prefs.override, [edition]: direction } }
}

/**
 * `d`'s flip: this edition the other way round, leaving every other edition's
 * override alone.
 *
 * **Flipped against the direction currently in *effect*, not against the
 * override.** An edition with no override yet reads the sidecar's own
 * `work.reading`, so flipping from `override[edition]` — `undefined` — would
 * have to guess a starting point, and on a right-to-left work the first press
 * of `d` would set `rtl` and appear to do nothing. `effectiveDirection` is
 * what makes the first press always visible.
 *
 * Written out in `App.tsx`'s `d` handler until this existed, which put the one
 * rule that decides what a keypress means somewhere no test could reach.
 */
export function flipped(
  sidecarReading: Direction,
  prefs: DirectionPrefs,
  edition: string,
): DirectionPrefs {
  const now = effectiveDirection(sidecarReading, prefs.override[edition])
  return setOverride(prefs, edition, now === 'rtl' ? 'ltr' : 'rtl')
}

/**
 * Which way a physical arrow key turns the page.
 *
 * The arrows are *physical* — `ArrowRight` is the key on the right of the
 * keyboard, and `keys.ts` is right to report it as `by: 1` without knowing
 * anything about the work. What that means for the reader is not physical: in
 * a right-to-left work the next page is drawn to the **left**, so pressing
 * `→` to advance would move the reader away from the page they are about to
 * read.
 *
 * This is the one place that mapping lives. `App.tsx` puts a `step` intent
 * through it before applying it, `Filmstrip` lays its rail out the same way,
 * and `modules/help.ts` asks it which arrow advances so the help cannot say
 * `→ next page` to a reader for whom it is false. Three consumers, one rule —
 * the alternative was each deciding for itself, which is how the filmstrip
 * came to run left-to-right under a right-to-left work in the first place.
 *
 * `ltr` is the identity, so a work with no override behaves exactly as it did
 * before this existed.
 */
export function stepFor(by: 1 | -1, reading: Direction): 1 | -1 {
  if (reading !== 'rtl') return by
  return by === 1 ? -1 : 1
}
