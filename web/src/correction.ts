/**
 * What a reader is about to assert, and what it will cost.
 *
 * Pure, and deliberately free of any `fetch`: the consequence of a correction
 * is arithmetic over the alignment the reader is already holding, and it has
 * to be shown *before* anything is written, because nothing undoes a
 * correction short of making another one.
 */
import type { Position, Resolver } from './resolver'

export interface Pending {
  /** The edition being read — the one whose slot `a` names. */
  from: string
  /** The edition being corrected against. */
  to: string
  a: Position
  /**
   * Absent until a tile is picked — not "this slot pairs with nothing".
   * `api.ts`'s `CorrectionRequest.b` is `string | null`, and `null` is the
   * wire's way to say "unpair `a`" — but nothing in this reader ever builds
   * that request with `b: null`; `confirmCorrection` (`App.tsx`) only ever
   * fires once `pending.b` is set to a real `Position`. So the unpair path
   * has no client here, and "not yet picked" is the only meaning "absent"
   * carries on this side of the boundary.
   */
  b?: Position
}

/**
 * A slot, in the sidecar's own span notation — `7`, `45R`, `46L`.
 *
 * The wire format for a correction, so the request, the document and the
 * contact sheet all name a slot the same way. A bare index would be a fourth
 * spelling of the same thing and the one nobody could read.
 */
export function spanOf(at: Position): string {
  return at.half ? `${at.page}${at.half}` : `${at.page}`
}

/**
 * Which slot of `from` will lose its counterpart if this correction lands.
 *
 * The alignment is strictly 1:1 in slots, so asserting that `a` pairs with `b`
 * is also asserting that whatever holds `b` today does not. That slot becomes
 * a gap, and a reader who is not shown this before committing finds out by
 * noticing a page stopped resolving.
 *
 * Answered by resolving in the direction the reader is *not* looking — `b`, in
 * `to`, back into `from` — which is one ordinary `Resolver.resolve` and works
 * for a composed pair exactly as it does for a stored edge. Nothing new has to
 * cross the WASM boundary for this.
 */
export function displacedBy(
  pending: Pending,
  resolver: Resolver,
): Position | undefined {
  if (!pending.b) return undefined
  const holder = resolver.resolve(pending.to, pending.from, pending.b)
  if (!holder) return undefined
  const same = holder.page === pending.a.page && holder.half === pending.a.half
  return same ? undefined : { page: holder.page, half: holder.half }
}

/**
 * A `Position` read out loud — 1-based, with a half suffixed when there is one.
 *
 * The counterpart to `spanOf`, and deliberately separate from it: `spanOf`
 * writes the sidecar's own zero-based notation for the wire, this writes what
 * a person reads. Sharing one function between the two would mean the wire
 * format and the banner drifting together the next time either is changed.
 *
 * Lives here rather than in `App.tsx` so it can be tested: `App.tsx` is
 * deliberately unreachable from Vitest (it imports `resolver.ts` as a value),
 * and this is a boundary conversion — the 1-based rule, and the half suffix —
 * that is exactly the kind of thing worth pinning.
 */
export function describePosition(at: Position): string {
  const displayPage = at.page + 1
  return at.half ? `page ${displayPage} ${at.half}` : `page ${displayPage}`
}
