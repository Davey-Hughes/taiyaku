import { display, type ReadingState } from './reading'
import type { Resolver } from './resolver'

/**
 * The nearest slots either side of a gap that do resolve.
 *
 * Offered as explicit moves rather than jumped to. `notes.md` leans this way
 * and the design took it: landing on the nearest silently is the behaviour
 * that makes a swap lose your place, and the whole authority model exists to
 * prevent it. Taking one of these is real navigation, so authority moves —
 * visibly, and only because it was asked for.
 *
 * `within` bounds the search so a wholly unpaired edition costs one scan
 * rather than one per render; it is the authority edition's slot count.
 */
export function nearestPaired(
  state: ReadingState,
  resolver: Resolver,
  within: number,
): { before?: number; after?: number } {
  const resolves = (slot: number) => display({ ...state, slot }, resolver).kind === 'at'
  const found: { before?: number; after?: number } = {}
  for (let slot = state.slot - 1; slot >= 0; slot--) {
    if (resolves(slot)) { found.before = slot; break }
  }
  for (let slot = state.slot + 1; slot < within; slot++) {
    if (resolves(slot)) { found.after = slot; break }
  }
  return found
}
