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
 * Mirrors `direction.ts`'s `setOverride`: the same per-edition merge shape,
 * where a forgotten spread would silently wipe every other edition's saved
 * override. This is extracted here rather than inlined into the `b` key
 * handler so that a test can pin it without mounting `App.tsx`.
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
