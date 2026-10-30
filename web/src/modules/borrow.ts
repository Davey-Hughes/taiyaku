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
 * Defaults on, matching `zoom`, `prefetch` and `help`. It governs only
 * borrowing from a *third* edition, so on a work with two editions it changes
 * nothing — the switch is honest rather than useless, and `Gap` is what a
 * reader sees either way. That honesty is on screen and not just in this
 * comment: `borrowNote` below says so under the toggle itself.
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
 * viewed edition and minus the authority. The first that resolves wins.
 *
 * **Nothing lends where no third edition does.** The authority is skipped like
 * the viewed edition, and there is no fallback to it: `applyIntent`'s toggle
 * goes to `state.authority` whenever `viewing !== authority` — true of every
 * gap — so lending it would draw a page one keypress away, and draw it
 * without arrows, annotations, zoom or prefetch, in place of the `Gap` view
 * that explains the state. See §2 of
 * `docs/superpowers/specs/2026-08-13-borrow-lender-and-arrows-design.md`. A
 * two-edition work therefore never borrows at all.
 *
 * **Which third edition lends, where more than one could, is imperfect.** The
 * order among candidates is `order`'s own — hub first, then alphabetical —
 * and nothing distinguishes a same-language edition from a raw: the sidecar's
 * `editions` block carries a free-text `label` and no language or kind field.
 * So an alphabetically-earlier raw can lend over a same-language edition that
 * would have been the more useful page, decided by nothing meaningful. See
 * the design's §2.
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
  return undefined
}

const EDITION_COUNT_WORDS: Record<number, string> = {
  0: 'no',
  1: 'one',
  2: 'two',
}

/**
 * The sentence `App.tsx` hands `Settings` (keyed by `borrowModule.id` in its
 * `notes` map) to say why the toggle above it can do nothing on this work —
 * `lenderFor`'s own limit, worded for a reader who has never heard "authority"
 * and does not need to: its candidates are `order` minus the edition being
 * viewed minus the authority, which is empty whenever `order` holds fewer
 * than three editions in the first place, regardless of which page a reader
 * is looking at or which edition they are viewing it in.
 *
 * `undefined` at three editions or more, where a third candidate exists for
 * `lenderFor` to try — whether or not it happens to resolve for the slot in
 * front of a reader right now, which is a fact about one gap and not about
 * the work, and not this function's job to know. `Settings` renders no note
 * at all for `undefined` rather than one that would need to flicker on and
 * off as a reader turns pages.
 *
 * A property of the work, so this takes the edition count alone — not a
 * `ReadingState`, not the edition being viewed — the same reasoning the
 * design calls out by name: the note must not depend on which edition is on
 * screen.
 */
export function borrowNote(editionCount: number): string | undefined {
  if (editionCount >= 3) return undefined
  const word = EDITION_COUNT_WORDS[editionCount] ?? String(editionCount)
  const plural = editionCount === 1 ? 'edition' : 'editions'
  return `This work has ${word} ${plural}, so there is no third edition to borrow a page from.`
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
