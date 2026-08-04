import { display, type ReadingState } from './reading'
import type { Resolver } from './resolver'

/**
 * One offer either side of a gap, already resolved against `state.viewing`.
 *
 * Carrying `page` here — not just the authority-space `slot` a caller
 * navigates to — is what lets a caller skip resolving the same slot a
 * second time. `App.tsx` used to call `session.ts`'s `resolveOffered` on
 * whichever slot this scan had just found resolving, which is one more
 * `Binding::resolve` per offer for an answer this function already had.
 * `docs/status.md`'s O14 entry has the measured cost of a redundant resolve.
 */
export interface NearestOffer {
  /** Authority-space — what `onTake`/`takeOffered` expects. */
  slot: number
  /** Zero-based page in `state.viewing`. Display converts at its own boundary, same as `resolveOffered`'s. */
  page: number
}

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
): { before?: NearestOffer; after?: NearestOffer } {
  const landingAt = (slot: number): NearestOffer | undefined => {
    const landed = display({ ...state, slot }, resolver)
    if (landed.kind !== 'at') return undefined
    return { slot, page: resolver.slotAt(landed.edition, landed.slot)!.page }
  }
  const found: { before?: NearestOffer; after?: NearestOffer } = {}
  for (let slot = state.slot - 1; slot >= 0; slot--) {
    const offer = landingAt(slot)
    if (offer) { found.before = offer; break }
  }
  for (let slot = state.slot + 1; slot < within; slot++) {
    const offer = landingAt(slot)
    if (offer) { found.after = offer; break }
  }
  return found
}
