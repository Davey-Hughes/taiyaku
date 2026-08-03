import type { StoredProgress } from './api'
import type { Landed, Resolver } from './resolver'

/**
 * One authority, not a per-edition cursor.
 *
 * `notes.md` records the trap: swap onto a page with no counterpart, land on
 * the nearest, swap back, and you have silently moved one page — compounding
 * on every swap. A swap here never writes position, so there is nothing to
 * drift, and round-tripping is lossless by construction rather than by care.
 */
export interface ReadingState {
  /** The edition the position is expressed in. */
  authority: string
  /** A flat slot index within `authority`. Never a page number. */
  slot: number
  /** The edition being shown. */
  viewing: string
}

/**
 * Where to start: at stored progress if there is any, or slot 0 of the hub
 * if there is none.
 *
 * `progress` is `undefined` exactly when the server answered its `GET` with
 * 204 — nothing recorded — not when a fetch failed; `App.tsx`'s load effect
 * is what tells those apart. `authority` and `viewing` both become the
 * stored edition, matching every other place in this module: a position is
 * always read while looking at the edition it authorises, never at the hub
 * on its behalf.
 *
 * Validated against `resolver` before being trusted — a stored position is
 * data written on one load and read back on a later one, and the sidecar in
 * between is free to have changed (re-solved, an edition shrunk or renamed).
 * `display`'s identity branch (`viewing === authority`) returns `state.slot`
 * with no resolver check at all, on the assumption that whatever produced
 * `ReadingState` already put a valid slot in it — true of every other
 * producer (`navigate`, `takeOffered`) because each derives its slot from
 * the resolver in the same call. `restore` is the one producer that instead
 * reads a slot out of storage, so it is the one place that has to check.
 * Doing it here rather than in `display` costs one check per page load
 * instead of one per render, and it means every downstream consumer keeps
 * getting a `ReadingState` it can trust, rather than `display` growing a new
 * "invalid" case that nothing else needs to know about.
 *
 * Two distinct failures, two distinct honest answers — neither of which is
 * "guess a slot near wherever the new boundary happens to fall," which would
 * just be a different silent wrong page:
 * - The stored edition no longer exists in this sidecar at all (renamed or
 *   dropped) — there is no slot space to place the reader in, so this is
 *   exactly the "nothing recorded" case: start at slot 0 of the hub.
 * - The stored edition still exists but the slot is out of its range (the
 *   edition shrunk, or the slot was never valid) — stay on that edition,
 *   since it still exists, but at slot 0 rather than clamped to whatever its
 *   new last slot is, which would be just as silent a move as the one this
 *   fixes.
 */
export function restore(
  progress: StoredProgress | undefined,
  hub: string,
  resolver: Resolver,
): ReadingState {
  if (!progress) return { authority: hub, slot: 0, viewing: hub }
  const count = resolver.slotCount(progress.edition)
  if (count === undefined) return { authority: hub, slot: 0, viewing: hub }
  if (progress.slot < 0 || progress.slot >= count) {
    return { authority: progress.edition, slot: 0, viewing: progress.edition }
  }
  return { authority: progress.edition, slot: progress.slot, viewing: progress.edition }
}

export type Display =
  | { kind: 'at'; edition: string; slot: number; via: Landed['via'] }
  | { kind: 'gap'; edition: string; from: { edition: string; slot: number } }

/** Show a different edition. Authority is untouched — that is the whole rule. */
export function swap(state: ReadingState, to: string): ReadingState {
  return { ...state, viewing: to }
}

/** Real navigation: the edition being viewed becomes the authority. */
export function navigate(state: ReadingState, slot: number): ReadingState {
  return { authority: state.viewing, slot, viewing: state.viewing }
}

/**
 * Which slot of the viewed edition to draw, or that there is no such slot.
 *
 * A gap is a view, not a position: it names what was being looked for and
 * where from, and one press of the toggle returns exactly where you were.
 */
export function display(state: ReadingState, resolver: Resolver): Display {
  if (state.viewing === state.authority) {
    return { kind: 'at', edition: state.viewing, slot: state.slot, via: 'identity' }
  }
  const from = resolver.slotAt(state.authority, state.slot)
  const landed = from && resolver.resolve(state.authority, state.viewing, from)
  const slot = landed && resolver.indexOf(state.viewing, landed)
  if (landed === undefined || slot === undefined) {
    return {
      kind: 'gap',
      edition: state.viewing,
      from: { edition: state.authority, slot: state.slot },
    }
  }
  return { kind: 'at', edition: state.viewing, slot, via: landed.via }
}
