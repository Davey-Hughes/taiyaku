import type { ReaderModule } from '../module'

export const PANEL_INTENT = 'credits:panel'

/** What a reader has decided about a scanlation group's own pages. */
export interface CreditsPrefs {
  /** Keep them out of the reading flow. Off by default — see the module doc. */
  skip: boolean
  /** Offer the panel that lists them. */
  panel: boolean
}

/**
 * Scanlation credit pages: kept out of the reading flow, never out of reach.
 *
 * A group's credit is what it asked for in exchange for the work, so this
 * module sets those pages aside rather than hiding them: nothing is deleted
 * and nothing is meant to become unreachable. `C` is claimed for the panel
 * that will list every one, and taking an entry will go there.
 *
 * **The panel is not mounted, and neither half of this module does anything
 * in the shipped build.** Say what shipped, not what the design sketched: this
 * file declares the contract — an id, a label, `defaults`, and the `C` claim —
 * and `App.tsx` renders no panel for it and has no `case PANEL_INTENT` in its
 * module switch, so a `C` press is consumed by `preventDefault()` and falls
 * out of the switch in silence. Pressing `s` does show a "Credit pages"
 * settings section, generated from `defaults` like every other module's, and
 * its two toggles persist across a reload and govern nothing.
 *
 * **Why it is registered anyway.** The panel was blocked on O25 — `App.tsx`'s
 * `shown.kind === 'gap'` branch returned the `Gap` view *instead of* the
 * reader's tree, so anything mounted below it was not rendered, and a gap is
 * precisely the state a reader asks "what did I skip?" from. Mounting a third
 * panel that vanished in that state would have been the partial answer O25
 * existed to prevent. Registration is kept because `modules/registry.ts`'s
 * property — adding a module to one array is the only hand-edit its existence
 * needs — is worth more than hiding an inert settings section, provided the
 * docs are honest about what the section currently does, which is what this
 * paragraph is for. Do not strip `keys` or `defaults` to quiet the section.
 *
 * **O25 is closed and this is no longer blocked, only unwritten.** `App.tsx`
 * now narrows `Display` once rather than returning early, so every panel has
 * one mount site that a gap does not hide — mounting this one is an ordinary
 * addition to it plus a `PANEL_INTENT` case in the module switch. What is
 * still genuinely unfinished is `creditSlots` below, for the reason its own
 * doc gives.
 *
 * **`C`, not `c`.** Lower-case `c` is `keys.ts`'s own reserved built-in for
 * correction mode, checked before any module's declared keys are — a module
 * claiming it collision-checks clean against every other module (that check
 * only compares modules to each other) and then simply never fires. Shifted
 * keys reach the module loop fine, and `annotations.ts` already claims `z`
 * and `Z` as separate bindings, so this is the established way to keep a
 * mnemonic letter once its bare form is taken.
 *
 * **`skip` is off by default.** The list is a heuristic's output — see
 * `crates/taiyaku-cli/src/credits.rs` — and a reader who has not asked for
 * pages to be skipped should not silently lose one to a false positive. The
 * panel is on, because a list nobody can find is not a section of its own.
 *
 * **Skipping never renumbers.** A slot keeps its number whether or not it is
 * skipped, so the readout jumps rather than counting differently. The slot is
 * the addressed unit — `docs/sidecar.md` is normative — and a per-reader toggle
 * that changed what an address meant would make one device's progress and one
 * reader's notes wrong on another. This is the swap precedent: what is shown
 * changes, what is written does not.
 */
export const creditsModule: ReaderModule<CreditsPrefs> = {
  id: 'credits',
  label: 'Credit pages',
  defaults: { skip: false, panel: true },
  keys: [{ key: 'C', intent: PANEL_INTENT }],
}

/** An edition's page count and its joined spreads — what slot expansion needs. */
export interface Pagination {
  pages: number
  spreads: number[]
}

/**
 * Every slot the given pages present.
 *
 * The sidecar's `credits` is in pages and the reader addresses slots, so this
 * is the one place the two meet. A joined spread presents two slots and both
 * are skipped together: half a credits card is not a page anybody wants.
 *
 * **This must be replaced when the panel is mounted, not adopted as it
 * stands.** Two things are wrong with it and neither is a bug in the
 * arithmetic, which is why the tests below pass:
 *
 * 1. **It is a second implementation of slot expansion.** `taiyaku_core`'s
 *    `slot::expand` owns that derivation and the page sequence is normative —
 *    `docs/sidecar.md`, and `docs/status.md` names two consumers deriving it
 *    separately as the defect `taiyaku-read` was created to prevent, because
 *    the format cannot detect the disagreement. The loop below is that second
 *    consumer, in a third language. The replacement is a filter over
 *    `App.tsx`'s existing `allSlots(resolver, edition)`, keeping the page of
 *    each slot: `allSlots` goes through the `Resolver`, which is
 *    `taiyaku-wasm`'s `Binding`, which is core's own expansion — so there is
 *    one derivation and the browser reads it rather than repeating it.
 * 2. **Nothing in `web/` can build the `Pagination` it asks for.** The
 *    `Resolver` interface exposes no page count and no spread list, only
 *    `slotCount` and `slotAt`. So this signature cannot be satisfied from what
 *    the reader has; it is only callable from a test that writes the numbers
 *    down itself, which is exactly what the tests below do.
 *
 * Kept rather than deleted because the behaviour it states is the behaviour
 * the panel wants — both halves of a joined spread go together — and the tests
 * pin that. What has to change is where the expansion comes from.
 */
export function creditSlots(pages: number[], edition: Pagination): number[] {
  const wanted = new Set(pages)
  const out: number[] = []
  let slot = 0
  for (let page = 0; page < edition.pages; page++) {
    const width = edition.spreads.includes(page) ? 2 : 1
    if (wanted.has(page)) {
      for (let i = 0; i < width; i++) out.push(slot + i)
    }
    slot += width
  }
  return out
}

/**
 * The next slot in `step`'s direction that is not skipped.
 *
 * Returns `from` unchanged when every slot ahead is skipped or the edition
 * ends, so an arrow at the boundary does nothing rather than landing somewhere
 * surprising — the same answer the reader already gives at the first and last
 * slot.
 */
export function nextReadable(
  from: number,
  /**
   * The direction to walk, never a distance — narrowed to match `keys.ts`'s
   * own `Intent`'s `{ kind: 'step'; by: 1 | -1 }`. This is what makes the
   * `while` below obviously terminating: `at` moves by a nonzero amount every
   * iteration, so it cannot stall on a skipped slot the way a wider `number`
   * (in particular `0`) would.
   */
  step: 1 | -1,
  skipped: number[],
  slotCount: number,
): number {
  const hidden = new Set(skipped)
  let at = from + step
  while (at >= 0 && at < slotCount && hidden.has(at)) at += step
  return at >= 0 && at < slotCount ? at : from
}
