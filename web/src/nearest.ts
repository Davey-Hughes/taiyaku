import { display, type ReadingState } from './reading'
import type { Resolver } from './resolver'

/**
 * One offer either side of a gap, already resolved against `state.viewing`.
 *
 * Carrying `page` here — not just the authority-space `slot` a caller
 * navigates to — is what lets a caller skip resolving the same slot a
 * second time: `session.ts`'s `resolveOffered` on whichever slot this scan
 * found resolving would be one more `Binding::resolve` per offer for an answer
 * this function already has. `docs/status.md`'s O14 entry has the measured cost
 * of a redundant resolve.
 */
export interface NearestOffer {
  /** Authority-space — what `onTake`/`takeOffered` expects. */
  slot: number
  /** Zero-based page in `state.viewing`. Display converts at its own boundary, same as `resolveOffered`'s. */
  page: number
  /**
   * Zero-based *slot* in `state.viewing` — not the authority-space `slot`
   * above, and not derivable from `page` where the edition holds joined
   * spreads (one page, two slots).
   *
   * Carried for the same reason `page` is: `landingAt` below already has it
   * as `landed.slot` and would otherwise discard it, leaving a caller to
   * resolve the same offer a second time. `App.tsx` centres the filmstrip on
   * this, since a gap has no slot of its own in the viewed edition and the
   * nearest one that resolves is the closest thing to "where the reader is".
   */
  at: number
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
 *
 * `skipped` is the `credits` module's set of slots the reader has asked to
 * keep out of the reading flow, in the **viewed** edition's slot space — the
 * same set and the same space `pairing.ts`'s `groups` is handed, which is why
 * it is passed in rather than derived here. A candidate landing in it is
 * passed over and the scan carries on, exactly as `groups`' partner search
 * steps past one; it is not merely rejected at the end, or a skipped page
 * between the reader and a real one would leave them with no offer at all.
 *
 * **This is `docs/status.md`'s O40.** `skip` is carried out by handing the
 * skipped slots to `groups`, so every path through `groups`/`stepToGroup`
 * honours it; this one — the arrow out of a gap or a borrowed page, which
 * reaches here instead — takes the set, or a reader whose nearest resolving
 * neighbour is a credit page would land on it anyway. `App.tsx`'s call sites
 * pass the same set, so the arrow and the `Gap` screen's offer buttons cannot
 * name different destinations. Empty by default, so a caller that does not skip
 * is unaffected and pays nothing — the same default `groups` takes.
 */
export function nearestPaired(
  state: ReadingState,
  resolver: Resolver,
  within: number,
  skipped: Set<number> = NOTHING_SKIPPED,
): { before?: NearestOffer; after?: NearestOffer } {
  const landingAt = (slot: number): NearestOffer | undefined => {
    const landed = display({ ...state, slot }, resolver)
    if (landed.kind !== 'at') return undefined
    if (skipped.has(landed.slot)) return undefined
    return {
      slot,
      page: resolver.slotAt(landed.edition, landed.slot)!.page,
      at: landed.slot,
    }
  }
  const found: { before?: NearestOffer; after?: NearestOffer } = {}
  for (let slot = state.slot - 1; slot >= 0; slot--) {
    const offer = landingAt(slot)
    if (offer) {
      found.before = offer
      break
    }
  }
  for (let slot = state.slot + 1; slot < within; slot++) {
    const offer = landingAt(slot)
    if (offer) {
      found.after = offer
      break
    }
  }
  return found
}

/**
 * One empty set, so the default argument does not allocate per call — the same
 * constant `pairing.ts` keeps for `groups`' own `skipped` default, kept
 * separately rather than exported across so neither module has to import the
 * other for a value that is `new Set()`.
 */
const NOTHING_SKIPPED: Set<number> = new Set()
