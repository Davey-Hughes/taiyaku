import type { Intent } from './keys'
import { display, navigate, swap, type ReadingState } from './reading'
import type { Resolver } from './resolver'

/** A sidecar alignment's verdict — see `taiyaku-core::solve::Verdict`. */
export type Verdict = 'refused' | 'fragmented' | 'sound'

/**
 * The slice of a sidecar's `alignments[]` entry `verdictFor` needs. A real
 * entry carries `by`, `solver` and `runs` too; those are irrelevant here and
 * TypeScript's structural typing lets a caller pass the wider object straight
 * through from `JSON.parse` without picking it apart first.
 */
export interface Alignment {
  from: string
  to: string
  quality?: { verdict: Verdict }
}

/**
 * Every intent but `chrome`. Showing or hiding the strip is UI state with no
 * bearing on `ReadingState` — `App.tsx`'s keydown handler branches on it
 * before `applyIntent` is ever called, and this excludes it at the type
 * level rather than leaving a dead `case` here for a variant this function
 * has no reason to touch.
 */
export type ReadingIntent = Exclude<Intent, { kind: 'chrome' }>

/**
 * One keypress's worth of state change.
 *
 * Never conflates `swap` and `navigate` — `notes.md`'s swap gotcha is exactly
 * what happens if a `toggle`/`view` is allowed to write `slot`. Only `step`
 * calls `navigate`, and only after confirming `display` actually landed
 * somewhere (`kind: 'at'`) in the edition being stepped through — there is no
 * slot to add `by` to while showing a gap, so arrows are deliberately inert
 * there. `Gap`'s offered moves (`before`/`after`, taken via `takeOffered`) are
 * the way out of one, not a repurposed arrow key.
 *
 * `previous` is MRU, not "the authority": it means "the edition being left,"
 * so it is captured on every change to `viewing` — `toggle` and `view` alike —
 * not only on `toggle`. Missing that on `view` is exactly how two consecutive
 * digit presses lose track of the edition in between: press a digit, press
 * another digit, and `Space` must return to the first one, not to whatever
 * `previous` happened to hold before either press.
 */
export function applyIntent(
  intent: ReadingIntent,
  state: ReadingState,
  previous: string,
  resolver: Resolver,
): { state: ReadingState; previous: string } {
  switch (intent.kind) {
    case 'toggle': {
      const to = state.viewing === state.authority ? previous : state.authority
      return { state: swap(state, to), previous: state.viewing }
    }
    case 'view':
      return { state: swap(state, intent.edition), previous: state.viewing }
    case 'step': {
      const shown = display(state, resolver)
      if (shown.kind !== 'at') return { state, previous }
      const count = resolver.slotCount(shown.edition) ?? shown.slot + 1
      const slot = Math.min(Math.max(shown.slot + intent.by, 0), count - 1)
      return { state: navigate(state, slot), previous }
    }
  }
}

/**
 * Where one of `nearest.ts`'s `nearestPaired` offers actually lands.
 *
 * `offered` is expressed in the authority edition's space — the same space
 * `nearestPaired` scanned it in, since it varies `state.slot` while holding
 * `state.authority`/`state.viewing` fixed. This resolves that authority-space
 * slot through to the viewed edition's slot and page, which is everything
 * both a caller committing to the move (`takeOffered`) and a view merely
 * displaying it (`Gap`'s page numbers) need — one resolution, shared, rather
 * than the gap view re-deriving it from `state` a second time.
 *
 * `undefined` if `offered` does not actually resolve in the viewed edition —
 * defensive, since a caller is expected to only ever pass a value
 * `nearestPaired` itself returned.
 */
export function resolveOffered(
  state: ReadingState,
  offered: number,
  resolver: Resolver,
): { edition: string; slot: number; page: number } | undefined {
  const landed = display({ ...state, slot: offered }, resolver)
  if (landed.kind !== 'at') return undefined
  return { edition: landed.edition, slot: landed.slot, page: resolver.slotAt(landed.edition, landed.slot)!.page }
}

/**
 * The state after taking one of `nearest.ts`'s `nearestPaired` offers.
 *
 * Real navigation rather than another swap: `resolveOffered` resolves where
 * `offered` lands in the viewed edition, and `navigate` commits authority
 * there, exactly as `step` does. The difference from `step` is only where
 * the destination slot comes from — a gap search found it outright here,
 * rather than an offset from the current one.
 *
 * A no-op (returns `state` unchanged) if `offered` does not resolve — see
 * `resolveOffered`.
 */
export function takeOffered(state: ReadingState, offered: number, resolver: Resolver): ReadingState {
  const resolved = resolveOffered(state, offered, resolver)
  return resolved ? navigate(state, resolved.slot) : state
}

/**
 * The verdict on the pairing between two editions, straight off the
 * sidecar's own `alignments` list.
 *
 * A field lookup, not the composition `Sidecar::trust` does in Rust:
 * `alignments` stores one direction per hub-and-spoke edge (a verdict is a
 * property of the pairing, not of which side asked, so either orientation
 * answers — same as Rust's `edge_between`), but this does not walk through
 * the hub when neither orientation is stored directly. A spoke-to-spoke pair
 * with no direct edge answers `undefined` rather than guessing at a composed
 * verdict, which is the one-implementation rule: reimplementing run
 * composition here would be a second copy of logic `taiyaku-core` already
 * owns, where a plain field lookup is not. `Resolver` has no verdict
 * accessor and the binding has no `trust` yet — a follow-up for M5b, where
 * `readouts` wants this same value and a binding accessor earns its keep.
 */
export function verdictFor(alignments: Alignment[], from: string, to: string): Verdict | undefined {
  const edge = alignments.find(
    (a) => (a.from === from && a.to === to) || (a.from === to && a.to === from),
  )
  return edge?.quality?.verdict
}

/**
 * A deterministic order for the `editions` list `intentOf` indexes into, hub
 * first.
 *
 * The sidecar's edition map, and `Resolver.editions()` derived from it, both
 * serialise from a Rust `BTreeMap` — alphabetical, not the hub. A digit that
 * means a different edition between reloads is worse than no digit at all,
 * so this is sorted here rather than left to whatever order the source
 * happened to produce.
 *
 * Lives alongside `applyIntent` rather than in `keys.ts`: both are session
 * bookkeeping (an edition list and what a keypress does to it), not
 * keybinding policy, and `keys.ts` already has one concern of its own
 * (`intentOf`) that changes for its own reason. Pure and taking plain strings
 * rather than a `Resolver` is what keeps it testable without a `resolver.ts`
 * import.
 */
export function orderedEditions(hub: string, editions: string[]): string[] {
  return [hub, ...editions.filter((edition) => edition !== hub).sort()]
}

/**
 * What `previous` should start as, before any toggle or view has happened.
 *
 * `applyIntent`'s toggle arm computes its target as
 * `state.viewing === state.authority ? previous : state.authority` — so
 * seeding `previous` with `authority` itself makes the very first `Space`
 * compute a target equal to the edition already shown: `swap` returns an
 * identical state, and `previous` is reassigned to that same value, making
 * every *later* `Space` the identical no-op too. That was the actual bug: not
 * a one-off missed keypress, but the toggle going permanently inert the
 * moment a session starts already viewing its authority — which `restore` (see
 * `reading.ts`) does whenever nothing is stored, and whenever the reader last
 * read the edition they last stored progress against.
 *
 * The fix seeds `previous` one step ahead in `orderedEditions`' order instead
 * — the same edition a `2` keypress would already jump to from the hub — so
 * the first `Space` and the first `2` land in the same place, and wraps past
 * the last edition back to the first rather than leaving a "no next edition"
 * gap in the order.
 *
 * `editions.length <= 1` (or `authority` absent from `editions` entirely,
 * which should not happen but is checked rather than assumed) has no other
 * edition to seed with, so this returns `authority` unchanged and the toggle
 * stays an honest no-op — there is nothing else to toggle to.
 */
export function initialPrevious(editions: string[], authority: string): string {
  const index = editions.indexOf(authority)
  if (index === -1 || editions.length <= 1) return authority
  return editions[(index + 1) % editions.length]
}
