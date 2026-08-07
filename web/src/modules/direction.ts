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
  keys: [{ key: 'd', intent: TOGGLE_INTENT }],
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
