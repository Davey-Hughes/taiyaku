import type { ReaderModule } from '../module'
import type { Display, ReadingState } from '../reading'
import type { Resolver } from '../resolver'

/** Whether a gap is filled from another edition, or left as a gap. */
export interface BorrowPrefs {
  enabled: boolean
}

/**
 * Drawing another edition's page where the viewed edition has none.
 *
 * Claims no keys: a reader decides once whether they want gaps filled, rather
 * than toggling it mid-read — `readouts`' and `prefetch`'s reasoning, against
 * `direction`'s `d` and `regroup`'s `b`.
 *
 * Defaults on, matching `zoom`, `prefetch` and `help`. The switch exists
 * because a reader comparing translations may want to *see* that a page is
 * missing; that is served by it existing, not by it defaulting off.
 */
export const borrowModule: ReaderModule<BorrowPrefs> = {
  id: 'borrow',
  label: 'Borrowed pages',
  defaults: { enabled: true },
}

/**
 * Which edition's page to draw where the viewed one has none, and which slot
 * of it.
 *
 * Candidates are `order` — `App.tsx` passes `Session.editions`, which
 * `startup.ts` builds with `orderedEditions`, so the hub is first — minus the
 * viewed edition and minus the authority. The first that resolves wins; if
 * none does, the authority lends.
 *
 * **The authority can always lend**, which is why the fallback needs no
 * resolution: `from` below is its own slot, so there is nothing to route. The
 * one case with no lender at all is `from` being undefined, meaning the
 * authority slot does not exist — and that is the only gap that survives this
 * module being enabled.
 *
 * **The authority is tried last rather than first, and that rule is
 * imperfect.** It is right when the authority is the raw and a third edition
 * is another English release, which is the case the feature exists for. It is
 * wrong in the mirror case — authority `en-fan`, viewed `en-official`, third
 * `raw-mono` — where it draws the raw over the other English edition. Nothing
 * available distinguishes them: the sidecar's `editions` block carries a
 * free-text `label` and no language or kind field. See the design's §2.
 */
export function lenderFor(
  state: ReadingState,
  resolver: Resolver,
  order: string[],
): { edition: string; slot: number } | undefined {
  const from = resolver.slotAt(state.authority, state.slot)
  if (from === undefined) return undefined
  for (const edition of order) {
    if (edition === state.viewing || edition === state.authority) continue
    const landed = resolver.resolve(state.authority, edition, from)
    const slot = landed && resolver.indexOf(edition, landed)
    if (slot !== undefined) return { edition, slot }
  }
  return { edition: state.authority, slot: state.slot }
}

/**
 * A `gap` with a lender becomes `borrowed`; everything else passes through
 * unchanged, by identity, so a caller can compare against what it passed in.
 *
 * **Layered on top of `display` rather than built into it.** `nearest.ts`'s
 * `landingAt` calls `display` and treats anything that is not `at` as
 * not-landing, which is exactly right for the offers either side of a gap:
 * they must land on a *real* page in the edition the reader chose. A `display`
 * that returned `borrowed` would silently turn each of those offers into a
 * jump to a third edition's page instead — the opposite of what they are for.
 * Keeping the decision here means `nearest.ts` needs no change at all.
 */
export function withBorrow(
  shown: Display,
  state: ReadingState,
  resolver: Resolver,
  order: string[],
  prefs: BorrowPrefs,
): Display {
  if (!prefs.enabled || shown.kind !== 'gap') return shown
  const lender = lenderFor(state, resolver, order)
  if (lender === undefined) return shown
  return {
    kind: 'borrowed',
    edition: shown.edition,
    from: shown.from,
    lender,
  }
}
