import type { ReaderModule } from '../module'

export interface RegroupPrefs {
  /** Break positions per edition key. */
  breaks: Record<string, number[]>
}

/**
 * Insert or remove a parity break at a slot.
 *
 * Kept sorted so the same set of breaks always produces the same layout,
 * whatever order a reader inserted them in. A break at slot 0 is refused
 * because pairing already restarts there — accepting it would store a
 * preference that cannot change anything.
 */
export function toggleBreak(breaks: number[], slot: number): number[] {
  if (slot <= 0) return breaks
  return breaks.includes(slot)
    ? breaks.filter((b) => b !== slot)
    : [...breaks, slot].sort((a, b) => a - b)
}

/** `regroupModule`'s own declared intent string — read here by `App.tsx` rather than duplicated as a literal. */
export const TOGGLE_INTENT = 'regroup:toggle'

export const regroupModule: ReaderModule<RegroupPrefs> = {
  id: 'regroup',
  label: 'Regroup pages',
  defaults: { breaks: {} },
  keys: [
    {
      key: 'b',
      intent: TOGGLE_INTENT,
      label: 'break the two-page pairing at this slot',
    },
  ],
}

/**
 * Toggle one edition's break at a slot, leaving every other edition's list
 * alone.
 *
 * Mirrors `direction.ts`'s `setOverride`: the same per-edition merge shape
 * that already bit once in this project (`App.tsx` used to inline it for
 * direction, untested, and a forgotten spread there would have silently
 * wiped every other edition's saved override). `App.tsx` is legitimately
 * Vitest-unreachable, so this is extracted here rather than inlined into the
 * `b` key handler, where nothing could catch a regression.
 */
export function toggleEditionBreak(
  prefs: RegroupPrefs,
  edition: string,
  slot: number,
): RegroupPrefs {
  return {
    ...prefs,
    breaks: {
      ...prefs.breaks,
      [edition]: toggleBreak(prefs.breaks[edition] ?? [], slot),
    },
  }
}
