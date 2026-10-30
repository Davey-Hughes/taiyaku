import { groups } from './pairing'
import type { Position, Resolver } from './resolver'

/**
 * Where an edition has to restart its two-page pairing so that every edition of
 * a work draws the same pairs as every other.
 *
 * **The defect this closes.** `pairing.ts`'s `groups` anchors `two` mode's
 * pairing at slot 0 of whatever edition it is handed, so without this nothing
 * carries that choice across a swap. Editions disagree about how many pages
 * precede the story, and every joined spread present in one and not another
 * flips the relative parity again from that point on — so the same reading
 * position would draw pages `(n-1, n)` in one edition and `(n, n+1)` in the
 * next. One page on screen would be right and the other its neighbour.
 *
 * **Pair boundaries are a property of the work, not of an edition.** That is
 * the whole idea here. A joined spread is one image and `groups` gives it its
 * own group unconditionally — a break cannot halve it — so wherever *any*
 * edition joins a spread, that is a boundary the work itself has, and every
 * other edition has to honour it or it cannot draw the counterpart pair.
 * `forcedBreaks` finds those, they are mapped into the hub's slot space and
 * unioned, the hub is grouped with them, and every edition (the hub included)
 * takes its breaks from that one grouping. The result is one boundary sequence
 * that all editions share.
 *
 * **Deriving each *spoke's* parity from the hub's own natural grouping, and
 * leaving the hub alone, is not enough.** It fails where a spoke's spreads
 * force boundaries the hub has no way to follow: `en-mono` joins pages 45 and
 * 46 back to back, which raw splits into four ordinary pages. Unioning the
 * forced boundaries fixes that case and, because the hub can move too, needs
 * *fewer* solo groups to do it.
 *
 * **The hub, and not the authority**, as the space the boundaries are unioned
 * in: `ReadingState.authority` moves to whichever edition the reader last
 * navigated in (`reading.ts`'s `navigate`), so using it would re-group every
 * edition whenever the reader turned a page somewhere new. The hub is fixed by
 * the sidecar, every alignment is stored against it, and a route to it is
 * direct rather than composed.
 *
 * **What it costs, stated rather than hidden.** Each position where the parity
 * genuinely has to change costs one group drawn alone, because that is what
 * restarting a pair means, and a reader who never swaps pays it too. It is the
 * honest arrangement: a solo page says "there is nothing here to pair this
 * with", where a silently mismatched pair says nothing at all.
 *
 * Returns `[]` for any edition the resolver cannot describe, which leaves that
 * edition grouping as it would without this function.
 *
 * **Costly enough to memoise** — it walks every slot of every edition, each a
 * call across the WASM boundary. `alignedBreaksCached` is the memo, and
 * `App.tsx` holds its cache in a ref.
 */
export function alignedBreaks(
  resolver: Resolver,
  hub: string,
  edition: string,
  reading: 'rtl' | 'ltr',
): number[] {
  const hubSlots = readSlots(resolver, hub)
  if (hubSlots === undefined) return []

  const hubBreaks = workBoundaries(resolver, hub, hubSlots)
  if (edition === hub) return hubBreaks

  const editionSlots = readSlots(resolver, edition)
  if (editionSlots === undefined) return []

  // `two` regardless of the mode on screen, so switching modes does not
  // re-derive a different set. Nothing skipped and no reader break: this asks
  // what the alignment says, not what the reader has since adjusted by hand.
  // `reading` is threaded because `groups` takes it, though it does not
  // currently read it — see that function's doc on why direction does not
  // appear in the grouping.
  const reference = groups(hubSlots, 'two', reading, hubBreaks)

  const breaks: number[] = []
  for (const group of reference) {
    let opens: number | undefined
    for (const slot of group.slots) {
      const at = hubSlots[slot]
      const index = at && landing(resolver, hub, edition, at)
      // The *lowest* slot the group lands on opens the pair. An alignment is
      // monotonic so this is the first in practice, but a group whose first
      // slot is unpaired and whose second is not would otherwise contribute
      // nothing, and that slot still has to open a group for the pages after
      // it to stay in phase.
      if (index !== undefined && (opens === undefined || index < opens)) {
        opens = index
      }
    }
    if (opens !== undefined && opens > 0) breaks.push(opens)
  }
  return tidy(breaks)
}

/**
 * Every slot of `edition`, or `undefined` if any of them cannot be read.
 *
 * All-or-nothing deliberately: a partial walk would place boundaries from the
 * part that resolved and none from the rest, which is a third grouping rather
 * than either of the two this module chooses between.
 */
function readSlots(
  resolver: Resolver,
  edition: string,
): Position[] | undefined {
  const count = resolver.slotCount(edition)
  if (count === undefined) return undefined
  const slots: Position[] = []
  for (let index = 0; index < count; index += 1) {
    const at = resolver.slotAt(edition, index)
    if (at === undefined) return undefined
    slots.push(at)
  }
  return slots
}

/** Where `at`, a slot of `from`, lands in `to` — as a slot index. */
function landing(
  resolver: Resolver,
  from: string,
  to: string,
  at: Position,
): number | undefined {
  const landed = resolver.resolve(from, to, at)
  return landed && resolver.indexOf(to, landed)
}

/**
 * The positions where pairing must restart in `slots`, whatever any break list
 * says: a joined spread's own first half, and the slot after it.
 *
 * `groups` emits a joined spread as its own two-slot group unconditionally and
 * steps past both halves, so the pair before it has to stop short and the pair
 * after it starts fresh. Recording both ends is what lets those boundaries be
 * carried into an edition that splits the same spread across two ordinary
 * pages, where nothing about the pages themselves would reveal them.
 *
 * Sorted and de-duplicated, because two spreads sitting back to back name the
 * slot between them twice — once as the end of the first pair and once as the
 * start of the second. That is precisely the `en-mono` shape this module exists
 * to handle, so it is the ordinary case rather than a corner one. Slot 0 is
 * *not* dropped here, unlike `tidy`: this function answers where the boundaries
 * are, and one at the very start is a true answer even though it changes
 * nothing downstream.
 *
 * Exported for its tests; `alignedBreaks` is the caller.
 */
export function forcedBreaks(slots: Position[]): number[] {
  const out: number[] = []
  for (let index = 0; index < slots.length; index += 1) {
    const here = slots[index]
    const next = slots[index + 1]
    if (here?.half === undefined || next?.page !== here.page) continue
    out.push(index)
    if (index + 2 < slots.length) out.push(index + 2)
  }
  return [...new Set(out)].sort((a, b) => a - b)
}

/**
 * Every edition's forced boundaries, mapped into the hub's slot space and
 * unioned — the one boundary sequence the whole work shares.
 *
 * The hub's own are included by walking it like any other edition. They are
 * inert there, since `groups` already forces them from the slots themselves,
 * and including them costs nothing while leaving one rule rather than a rule
 * plus an exception.
 */
function workBoundaries(
  resolver: Resolver,
  hub: string,
  hubSlots: Position[],
): number[] {
  const boundaries: number[] = []
  for (const edition of resolver.editions()) {
    const slots = edition === hub ? hubSlots : readSlots(resolver, edition)
    // One unreadable edition drops its own evidence and no one else's — unlike
    // a partial walk *within* an edition, a missing edition leaves the
    // boundaries that were found still correct, just fewer.
    if (slots === undefined) continue
    for (const slot of forcedBreaks(slots)) {
      const at = slots[slot]
      if (at === undefined) continue
      const index = edition === hub ? slot : landing(resolver, edition, hub, at)
      if (index !== undefined) boundaries.push(index)
    }
  }
  return tidy(boundaries)
}

/**
 * Sorted, de-duplicated, and with slot 0 dropped — the shape `groups` reads and
 * `regroup.ts`'s `toggleBreak` maintains, so the two lists concatenate without
 * either caring which produced a given position.
 *
 * Slot 0 goes for the reason `toggleBreak` refuses it: pairing already restarts
 * there, so recording it would be a break that cannot change anything.
 * Duplicates are ordinary here — two editions joining the same spread name the
 * same boundary twice.
 */
function tidy(slots: number[]): number[] {
  return [...new Set(slots)].filter((slot) => slot > 0).sort((a, b) => a - b)
}

/**
 * What `alignedBreaksCached` remembers between renders. A plain object rather
 * than a closure so `App.tsx` can hold it in a `useRef` and the caching rule
 * itself stays a pure function with tests.
 */
export interface DerivedBreaksCache {
  resolver?: Resolver
  hub?: string
  edition?: string
  reading?: 'rtl' | 'ltr'
  breaks: number[]
}

/** A fresh, empty cache — the initial value `App.tsx`'s ref takes. */
export function emptyBreaksCache(): DerivedBreaksCache {
  return { breaks: [] }
}

/**
 * `alignedBreaks`, computed once per distinct set of inputs.
 *
 * The walk is a resolver call per slot of *every* edition, each a crossing of
 * the wasm boundary, and `App.tsx` re-renders on every keypress, every hover
 * over the chrome and every settings change, none of which move the hub or the
 * edition. `useMemo` would be the obvious tool and is unavailable: every hook
 * in that component has to sit above its `if (!session) return null`, and the
 * hub, the viewed edition and the effective reading direction are all bound
 * below it. So the memo is written out, over a cache the caller keeps in a ref.
 *
 * The resolver is compared by identity rather than by name, deliberately: a
 * correction or a stale-`ETag` re-read replaces the resolver while the work,
 * the hub and the edition all keep their names, and a cache keyed on names
 * alone would go on answering with the pairing the old alignment implied.
 */
export function alignedBreaksCached(
  cache: DerivedBreaksCache,
  resolver: Resolver,
  hub: string,
  edition: string,
  reading: 'rtl' | 'ltr',
): number[] {
  if (
    cache.resolver === resolver &&
    cache.hub === hub &&
    cache.edition === edition &&
    cache.reading === reading
  ) {
    return cache.breaks
  }
  const breaks = alignedBreaks(resolver, hub, edition, reading)
  cache.resolver = resolver
  cache.hub = hub
  cache.edition = edition
  cache.reading = reading
  cache.breaks = breaks
  return breaks
}

/**
 * The derived breaks and the reader's own, as one list.
 *
 * A reader's hand-placed break is kept rather than overridden: `groups` treats
 * the list as a set of positions to restart at, so a reader who marked one the
 * alignment did not know about still gets it. `b` on a position the derivation
 * already found is the one gesture this makes inert — `toggleBreak` would
 * remove it from the reader's own list while it stayed in the derived one — and
 * that is left alone deliberately, because the alternative is a stored
 * preference that silently fights the alignment on every render.
 */
export function pairingBreaks(own: number[], derived: number[]): number[] {
  if (derived.length === 0) return own
  if (own.length === 0) return derived
  return [...new Set([...own, ...derived])].sort((a, b) => a - b)
}
