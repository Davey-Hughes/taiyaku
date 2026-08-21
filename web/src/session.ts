import type { Intent } from './keys'
import { nearestPaired } from './nearest'
import { stepToGroup, type Group } from './pairing'
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
 * Every intent but `chrome`, `module`, `correct`, `account` and `library`.
 * Showing or hiding the strip is UI state with no bearing on `ReadingState`, a
 * module's own key (direction's `d`, currently) acts on that module's own
 * preference slice, not on `ReadingState` either, a correction is a write to
 * the sidecar's alignment — it names a `Pending` assertion, not a change to
 * where the reader is looking — the account panel is a screen over the reader
 * rather than anything about which page it is showing, and leaving the reader
 * entirely is `App.tsx`'s `onLeave`, not a change to where it is looking
 * either. None of the five has anything to hand `applyIntent`.
 *
 * `App.tsx`'s keydown handler branches on all five before `applyIntent` is
 * ever called, and this excludes them at the type level rather than leaving a
 * dead `case` here for a variant this function has no reason to touch.
 *
 * **Which makes this list the decision point for a new intent**, not a
 * formality: spec A2's `account` was added to `keys.ts` and `tsc` failed here
 * until it said where the new key is handled. A reader-state intent belongs in
 * `applyIntent`'s switch; anything else belongs in this `Exclude`.
 */
export type ReadingIntent = Exclude<
  Intent,
  | { kind: 'chrome' }
  | { kind: 'module'; intent: string }
  | { kind: 'correct' }
  | { kind: 'account' }
  | { kind: 'library' }
>

/**
 * One keypress's worth of state change.
 *
 * Never conflates `swap` and `navigate` — `notes.md`'s swap gotcha is exactly
 * what happens if a `toggle`/`view` is allowed to write `slot`. Only `step`
 * calls `navigate`, and never onto a slot it has not confirmed first — but
 * there are two confirmations now, not one. On a real page it is `display`
 * reporting `kind: 'at'` for the position the reader is standing on. On a gap
 * or a borrowed page it is `takeOffered`'s own `resolveOffered`, which asks
 * about the *offered* slot instead. The rule they share is what matters:
 * nothing navigates to a slot the viewed edition does not hold.
 *
 * **An arrow on a gap takes the offered move, and this reverses a decision
 * recorded here on purpose.** That decision read: "the offered moves … are the
 * way out of one, *not a repurposed arrow key*," and its reasoning was sound —
 * there is genuinely no slot to add `by` to. What changed is the evidence
 * about its consequence. `modules/borrow.ts` removed the screen that made an
 * inert arrow self-explanatory, and 64 of 104 unpaired runs in the corpus are
 * a single position, so the offer and the next page are the same destination
 * most of the time. The 2026-08-13 design, §3.1, carries the argument.
 *
 * The `display` called below is the *unwrapped* one — `withBorrow` is applied
 * in `App.tsx`, above this — so a borrowed page is a `gap` here and one branch
 * serves both screens, which is what keeps them from answering one key two
 * ways.
 *
 * **`skipped` is what makes that branch honour `skip`, and it is `docs/status.md`'s
 * O40.** The `groups` route below already does: `skip` is carried out by
 * handing the skipped slots to `pairing.ts`'s `groups`, so `stepToGroup` steps
 * over one without being told anything. The gap branch reaches `nearestPaired`
 * instead, which resolves candidates straight off the `Resolver` and knew
 * nothing about them — so an arrow out of a gap or a borrowed page could land
 * on a credit page the reader had asked to skip. The same set both routes need
 * is now passed to both, rather than a second module being taught an invariant
 * `skippedApartFrom`'s doc says lives in one place. In the *viewed* edition's
 * slot space, like `groups`' own, and optional: a caller with nothing skipped
 * passes nothing and the scan is what it was.
 *
 * What changed for the reader: `Gap` announced the dead end in words and put
 * both escapes on screen beside it, so an arrow that did nothing was explained
 * by the screen it did nothing on. A borrowed page announced nothing — an
 * apparently ordinary page filled the area, ← and → were silent, and the only
 * escape in the page itself was `Borrowed`'s mark, whose label read as a
 * caption. That asymmetry is what the paragraph above fixes: an arrow now
 * takes the offered move on both screens, not only the one that used to
 * explain its own silence. Recorded in `docs/status.md` under "Fall back to
 * another edition where this one has no page", and measured in
 * `docs/notes.md` under "The lender rule in a browser".
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
  groups?: Group[],
  skipped?: Set<number>,
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
      if (shown.kind !== 'at') {
        // No slot in the viewed edition to add `by` to — so the arrow takes
        // the move the screen is already offering instead. `by` arrives
        // rtl-mapped (`App.tsx` runs it through `stepFor`), so `1` is the
        // next slot in reading order and `after` is its offer.
        // Must match `App.tsx`'s `gapMoves` call exactly — both the `within`
        // bound and the skipped set — or an arrow could refuse a move the
        // screen's own offer buttons are showing, or take one they are not.
        const within = resolver.slotCount(state.authority) ?? state.slot + 1
        const { before, after } = nearestPaired(
          state,
          resolver,
          within,
          skipped,
        )
        const offer = intent.by === 1 ? after : before
        if (offer === undefined) return { state, previous }
        return {
          state: takeOffered(state, offer.slot, resolver),
          previous,
        }
      }
      // By group where the caller knows the grouping, by slot where it does
      // not. A group is one image on screen, so an arrow moves to the next
      // *image* — see `pairing.ts`'s `stepToGroup` for what that fixes and
      // what it costs. The slot fallback is what keeps this function usable
      // without a renderer: `groups` is a property of how the page is being
      // drawn right now (mode, direction override, hand-inserted breaks),
      // which nothing below the component tree knows.
      if (groups)
        return {
          state: navigate(state, stepToGroup(groups, shown.slot, intent.by)),
          previous,
        }
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
 * slot through to the viewed edition's slot and page, for `takeOffered` below
 * to commit the move to.
 *
 * **Not what `Gap`'s page numbers use.** They used to: `App.tsx`'s gap branch
 * called this on the same slot `nearestPaired` had just resolved while
 * finding it, a second `Binding::resolve` for an answer already in hand —
 * `docs/status.md`'s O14 entry has the measured cost. `nearestPaired` now
 * returns that resolution directly (see `nearest.ts`'s `NearestOffer`), so
 * this is only called once more, here, when a move is actually taken.
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
  return {
    edition: landed.edition,
    slot: landed.slot,
    page: resolver.slotAt(landed.edition, landed.slot)!.page,
  }
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
export function takeOffered(
  state: ReadingState,
  offered: number,
  resolver: Resolver,
): ReadingState {
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
export function verdictFor(
  alignments: Alignment[],
  from: string,
  to: string,
): Verdict | undefined {
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
 * Pulls `alignments` out of a freshly-fetched sidecar document, the same
 * field `App.tsx`'s load effect reads at startup (`SidecarShape.alignments`
 * there).
 *
 * `Sidecar::rewrite` recomputes the `quality` block — what `verdictFor`
 * above actually reads — on every correction, and the composed branch can
 * add an entirely new edge. Before this existed, `App.tsx` rebuilt
 * `resolver` from a correction's response body (both on the success path and
 * after a stale-`ETag` re-read) but left `alignments` at whatever loaded at
 * session start, so the swap banner's verdict, `chapterReadout` and
 * `healthReadout` could all go on describing a document the correction had
 * already changed — the M5d review's I3, and the drift its own O23 entry
 * measured directly (two corrections took a real fixture from `sound` to
 * `fragmented` while the reader kept reading "sound").
 *
 * A one-line `JSON.parse` rather than reaching for `SidecarShape` itself:
 * that interface lives in `App.tsx`, which this module cannot import (see
 * its own no-resolver-in-tests doc), and `alignments` is the only field
 * either call site needs out of the response.
 */
export function alignmentsOf(json: string): Alignment[] {
  return (JSON.parse(json) as { alignments: Alignment[] }).alignments
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
