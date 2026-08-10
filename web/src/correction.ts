/**
 * What a reader is about to assert, what it will cost, and the write itself.
 *
 * The arithmetic here is pure: the consequence of a correction is derived from
 * the alignment the reader is already holding, and it has to be shown *before*
 * anything is written, because nothing undoes a correction short of making
 * another one.
 *
 * The write is not pure and takes its three calls as ports. It was `App.tsx`'s
 * `confirmCorrection` — 102 lines with a nested `try`/`catch`, three distinct
 * outcomes and a `dispose` ordering that a parse failure must not break — and
 * being in `App.tsx` it was covered by typecheck alone. `resolve` is
 * `wasmResolver` injected, for the reason `startup.ts`'s own doc gives: this
 * module must never value-import `resolver.ts`.
 */
import type { CorrectionRequest, FetchedSidecar } from './api'
import { correctionTiles, type Group } from './pairing'
import { navigate, type ReadingState } from './reading'
import { alignmentsOf, type Alignment } from './session'
import { allSlots, type Session } from './startup'
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
 * The filmstrip's click job while a correction is being asserted: `slot` is a
 * slot of `pending.to` — the edition the strip is showing while correcting —
 * converted to a `Position` here because that is what `Pending.b`, `spanOf`
 * and `displacedBy` all deal in.
 *
 * Not a write itself — `submitCorrection` below is that, and it only fires
 * once this has set `pending.b`. `slot` resolving to nothing (a gap in `to`)
 * leaves `pending` unchanged, which is what lets a reader click a gap without
 * the banner reacting.
 */
export function counterpartPick(
  pending: Pending,
  resolver: Resolver,
  slot: number,
): Pending {
  const b = resolver.slotAt(pending.to, slot)
  return b ? { ...pending, b } : pending
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

/**
 * Which edition to correct against: the authority where the reader has swapped
 * away from it — that swap is what they are comparing against — otherwise
 * `previous`, the edition a `Space` would return to and so the most recent
 * "other" edition in view.
 *
 * `undefined` where that lands back on the edition already on screen, which is
 * a correction with nothing on the other side of it.
 */
export function counterpartEdition(
  state: ReadingState,
  previous: string,
  shownEdition: string,
): string | undefined {
  const to = state.authority !== shownEdition ? state.authority : previous
  return to === shownEdition ? undefined : to
}

/** `api.ts`'s two writes and `resolver.ts`'s parse, injected — see this file's doc. */
export interface CorrectionPorts {
  patch(
    work: string,
    etag: string,
    correction: CorrectionRequest,
  ): Promise<FetchedSidecar>
  sidecar(work: string): Promise<FetchedSidecar>
  resolve(json: string): Promise<Resolver>
}

export type CorrectionOutcome =
  /** The write landed. The reader moves to the page they picked, if it resolves to a slot. */
  | {
      kind: 'landed'
      resolver: Resolver
      alignments: Alignment[]
      etag: string | undefined
      /** Where to land the reader, if `b` resolves to a slot. */
      slot: number | undefined
      to: string
    }
  /**
   * Somebody else's correction is in the document now, and it may be the one
   * this reader was about to make. The assertion survives untouched: a
   * re-read gets them a current validator and resolver to confirm against,
   * never a silently retried write.
   */
  | {
      kind: 'stale'
      /** Absent where the re-read itself failed; the assertion survives either way. */
      resolver?: Resolver
      alignments?: Alignment[]
      etag?: string
      notice: string
      /** The re-read's own failure, for the caller to log. */
      error?: unknown
    }
  | { kind: 'refused'; notice: string }

/**
 * The server's own wording for a `412` (`routes.rs`'s `patch_alignments`),
 * matched by substring rather than by re-parsing the status out of the thrown
 * message — `api.ts` only promises the status is somewhere in there.
 */
const STALE = 'changed since you read it'

const STALE_NOTICE =
  'This work changed since it was read — check the pairing below and confirm again.'

/**
 * Write one correction.
 *
 * **Never disposes anything.** The caller frees the outgoing resolver only
 * once a replacement exists, which is the ordering `resolver.ts`'s `dispose`
 * doc requires — and a parse failure here must leave the reader with a working
 * resolver rather than none, which is exactly what disposing from inside this
 * function would break.
 *
 * A throw from the *success* path — `resolve` refusing the document the server
 * just returned — is `refused` carrying that message, not `landed`. That falls
 * out of the single `try` rather than being a case, and it is deliberate: the
 * write did land on the server, but this build cannot show its result, and
 * telling the reader it worked while the screen still shows the old pairing is
 * the worse of the two wrong answers.
 */
export async function submitCorrection(
  work: string,
  etag: string,
  pending: Pending & { b: Position },
  ports: CorrectionPorts,
): Promise<CorrectionOutcome> {
  const { from, to, a, b } = pending
  try {
    const fresh = await ports.patch(work, etag, {
      from,
      to,
      a: spanOf(a),
      b: spanOf(b),
    })
    const resolver = await ports.resolve(fresh.json)
    return {
      kind: 'landed',
      resolver,
      // `alignments` alongside the resolver swap — `Sidecar::rewrite`
      // recomputes `quality` on every correction, so leaving this at whatever
      // loaded at session start would show a verdict the document no longer
      // states.
      alignments: alignmentsOf(fresh.json),
      etag: fresh.etag,
      slot: resolver.indexOf(to, b),
      to,
    }
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err)
    if (!message.includes(STALE)) return { kind: 'refused', notice: message }
    try {
      const fresh = await ports.sidecar(work)
      const resolver = await ports.resolve(fresh.json)
      // Same alignments-refresh reasoning as the success path: a 412 re-read
      // swaps the resolver too, and the document it re-reads can carry a
      // fresher `quality` block than the one this session loaded with.
      return {
        kind: 'stale',
        resolver,
        alignments: alignmentsOf(fresh.json),
        etag: fresh.etag,
        notice: STALE_NOTICE,
      }
    } catch (rereadErr) {
      // The reader is still told the pairing moved under them — that is the
      // fact they need — and the re-read's own failure goes to the caller to
      // log rather than replacing a notice about the correction with one
      // about a fetch.
      return { kind: 'stale', notice: STALE_NOTICE, error: rereadErr }
    }
  }
}

/**
 * Install a landed correction, and move the reader onto the page they picked.
 *
 * `previous` moves to the edition being left, as it must on *every* change to
 * `viewing` — see `session.ts`'s own doc on `previous` for the drift skipping
 * it produces. Reached through a correction rather than a keypress is still a
 * change to `viewing`, and this is the one place that is easy to forget.
 */
export function applyLanded(
  session: Session,
  landed: Extract<CorrectionOutcome, { kind: 'landed' }>,
): Session {
  const state =
    landed.slot === undefined
      ? session.state
      : navigate({ ...session.state, viewing: landed.to }, landed.slot)
  return {
    ...session,
    resolver: landed.resolver,
    alignments: landed.alignments,
    state,
    previous: session.state.viewing,
  }
}

/** What the filmstrip and the banner are drawn from while a correction is being asserted. */
export interface CorrectionView {
  slots: Position[]
  /**
   * `correctionTiles`, not `groups` — one tile per slot, so a spread's half
   * can be named on its own. See that function's own doc: `groups` would fold
   * a joined spread into one two-slot tile the reader could only ever assert
   * `slots[0]` of, and in `two` mode would do the same to every ordinary pair.
   */
  tiles: Group[]
  /**
   * Where the alignment currently resolves `pending.a` to, in `to` — the tile
   * a reader disagrees with when they open correction mode, so it has to be
   * visible to disagree with. `undefined` where `a` pairs with nothing there,
   * a real state `Resolver.resolve` answers and this must not crash on.
   */
  pairedIndex: number | undefined
  /** Named before the write, never after it — see `displacedBy`. */
  displaced: Position | undefined
}

/**
 * The four things a pending correction puts on screen, from the one resolver
 * they all have to agree with.
 *
 * One call rather than four derivations in a render body: they are read
 * together, they are wrong together, and the dashed marker landing on a tile
 * that names a different slot than `displaced` warns about is the failure this
 * shape exists to make impossible.
 */
export function correctionView(
  pending: Pending,
  resolver: Resolver,
): CorrectionView {
  const slots = allSlots(resolver, pending.to)
  const landedAt = resolver.resolve(pending.from, pending.to, pending.a)
  return {
    slots,
    tiles: correctionTiles(slots),
    pairedIndex: landedAt ? resolver.indexOf(pending.to, landedAt) : undefined,
    displaced: displacedBy(pending, resolver),
  }
}
