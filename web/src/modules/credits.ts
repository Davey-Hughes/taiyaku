import type { ReaderModule } from '../module'
import type { Position } from '../resolver'

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
 * **Both halves now do something.** `C` opens `CreditsPanel`, which lists this
 * edition's credit pages; `skip` keeps them out of the reading flow. The panel
 * was blocked on O25 until 2026-08-07 — `App.tsx`'s `shown.kind === 'gap'`
 * branch returned the `Gap` view *instead of* the reader's tree, so anything
 * mounted below it went unrendered, and a gap is precisely the state a reader
 * asks "what did I skip?" from. That branch is gone and there is one mount
 * site a gap does not hide.
 *
 * **`skip` is carried out by `pairing.ts`'s `groups`, which is handed the
 * skipped slots**, not by filtering the groups it returns. The difference is
 * not academic: a filter can only drop a group whose every slot is skipped,
 * and measured on the primary corpus that is none of them, since each of
 * `en-fan`'s four credit pages sits at a chapter's end and so pairs with an
 * ordinary page — 122 tiles in two-page mode with the setting on and 122 with
 * it off. Deciding while pairing lets the partner search step past a skipped
 * slot so its neighbour pairs forward. See `docs/notes.md`, "The credit-pages
 * panel, and where skipping had to be decided".
 *
 * `creditSlots` and `nextReadable` are both gone. The first is `creditSlotsIn`
 * below, filtering the already-expanded slot table instead of deriving slot
 * expansion a second time; the second walked slots in a reader that has
 * stepped by group since #12, and its job is now the grouping's.
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

/**
 * Every slot of `slots` that one of `pages` presents.
 *
 * The sidecar's `credits` is in pages and the reader addresses slots, so this
 * is the one place the two meet. A joined spread presents two slots and both
 * are returned: half a credits card is not a page anybody wants, and both
 * halves carry the same `page`, so filtering on `page` gets that without
 * needing to know which pages are spreads.
 *
 * **`slots` is `App.tsx`'s `allSlots(resolver, edition)`, deliberately.** The
 * function this replaced took a `{ pages, spreads }` and walked it, which was
 * a second implementation of slot expansion — `taiyaku_core`'s `slot::expand`
 * owns that derivation and `docs/sidecar.md` is normative on the page
 * sequence; `docs/status.md` names two consumers deriving it separately as the
 * defect `taiyaku-read` exists to prevent, because the format cannot detect
 * the disagreement. Taking the already-expanded table instead means the
 * browser reads core's answer rather than recomputing it in a third language.
 * It also removes the reason the old signature was uncallable: nothing in
 * `web/` could build a `{ pages, spreads }`, since `Resolver` exposes neither
 * a page count nor a spread list.
 *
 * A page the edition does not have contributes nothing rather than throwing.
 * `credits` is a heuristic's output written by a person, while the slot table
 * is re-derived from the archives on every load — same file, different
 * provenance — so the two can disagree after a re-solve or a replaced member.
 * Returning `undefined` on an ordinary out-of-range index is what `Resolver`'s
 * own `slotAt`, `indexOf` and `chapterAt` all do, reserving a throw for what
 * `resolver.ts` calls "a real defect, not an ordinary gap"; degrading to
 * "fewer skipped pages" is the safe
 * direction, where a thrown error would take the whole reader down over a
 * stale list.
 */
export function creditSlotsIn(slots: Position[], pages: number[]): number[] {
  const wanted = new Set(pages)
  const out: number[] = []
  slots.forEach((position, slot) => {
    if (wanted.has(position.page)) out.push(slot)
  })
  return out
}

/**
 * The slots to set aside, minus the page the reader is standing on.
 *
 * **A reader on a credit page is there for a reason, and there are four.**
 * Taking an entry from the panel is the deliberate one, and `CreditsPanel`
 * promises in as many words that it goes there. The others are switching
 * `skip` on while already standing on one, restoring saved progress that
 * points at one, and a swap or a confirmed correction resolving onto the other
 * edition's credit page. *An earlier version of this paragraph said the panel
 * was the only way in. It is not, and `pairing.ts` listed two of the others
 * one module away.*
 *
 * Leaving the slot skipped breaks the promise in a way that reads as a bug
 * rather than a refusal: the page area draws whatever group survives at the
 * reader's position instead, the filmstrip marks that group, and only the
 * readout still says where they really are. Measured before this existed:
 * taking `page 96` on `en-fan` drew the volume *cover*, while the row beneath
 * read `slot 98 / 240 · p. 96 · 41%`.
 *
 * **The toggle route has a consequence worth knowing.** A reader who switches
 * `skip` on while standing on a credit page sees the setting appear to do
 * nothing: their own page stays drawn, and only the rest of the volume's
 * credit pages leave the flow. One arrow proves it worked. Moving them off
 * instead would be a navigation nobody asked for, which is the same argument
 * `nearest.ts` makes about not jumping a reader out of a gap.
 *
 * Exempting the slot rather than special-casing the renderer means the page is
 * drawn in all three modes, appears in the filmstrip, and is stepped off by
 * the ordinary arrow path — there is no stranded state left to handle. The
 * exemption travels with the reader, so the page goes back to being set aside
 * the moment they leave it.
 *
 * **The invariant this establishes is load-bearing elsewhere and stated
 * nowhere else**, so it is stated here: *every position the reader can hold
 * belongs to some group.* `App.tsx` relies on it to draw a page at all —
 * without it `grouped.findIndex` answers `-1` and the page area draws nothing,
 * which is what "every page skipped drew a blank page area" was. `pairing.ts`'s
 * `nearestGroup` is the backstop for its violation and is unreachable while it
 * holds. **Matching by slot instead of by page would break it silently**, on a
 * joined spread: the reader's own half would be exempted and the other still
 * skipped, so the spread would stay out of the grouping and nothing would be
 * drawn. `credits.test.ts` pins the whole-spread behaviour for that reason.
 *
 * **Both halves of a joined spread go together**, matched by page rather than
 * by slot: exempting one half would draw half a credits card, which is the
 * same rule `creditSlotsIn` above follows in the other direction.
 */
export function skippedApartFrom(
  skipped: number[],
  slots: Position[],
  standing: number | undefined,
): Set<number> {
  const out = new Set(skipped)
  const here = standing === undefined ? undefined : slots[standing]
  if (here === undefined) return out
  slots.forEach((position, slot) => {
    if (position.page === here.page) out.delete(slot)
  })
  return out
}
