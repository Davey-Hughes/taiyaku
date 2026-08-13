# A page borrowed from another edition, in place of a gap — design

**Status: settled in brainstorming on 2026-08-12. Built, and shipped on
`gap-fallback`.**

Written in the style of the rest of `docs/`: every decision records the
alternative it beat and what it cost, a figure asserted here was measured on
this machine rather than recalled, and a guess says it is one.

This closes the **"Fall back to another edition where this one has no page,
marked as such"** entry under `status.md`'s "Later". It is not an O-number: it
was filed as a future decision rather than as a defect found by a review, so
closing it is an edit to that entry rather than the five-part open-items
ritual.

## Where it came from

The reader shows `Gap` when the edition being read has no counterpart for the
slot the reader is standing on. That is an honest answer and a missed
opportunity: the alignment already knows which *other* editions hold that page.
The "Later" entry states the case for filling it, and states the hazard in the
same breath — a page silently drawn from another edition would be worse than a
gap — so the marking is not decoration here, it is the thing that makes the
feature admissible at all.

The entry also calls this "arguably the whole point of the alignment": a reader
following a story should not lose a page because the edition they happen to be
in dropped it, and no single-edition reader can offer that.

## 1. Two findings that shape everything below, both from reading the code

Neither was in the "Later" entry, and each removes a design option the entry
left open.

**The authority edition can always lend.** `display` (`web/src/reading.ts:96`)
reaches its gap branch three ways: `from` undefined, no route, or a landed
position that is not indexable in the viewed edition. `from` is
`resolver.slotAt(state.authority, state.slot)` — so whenever it is defined, the
authority holds that exact position by construction, and is therefore always a
valid lender. A gap also only ever arises when `viewing !== authority`, because
the identity branch above returns `at` first. **So the only gap that survives
this feature is the one where the authority's own slot is invalid**, which
`restore`'s validation and `navigate`'s resolver-derived slots already prevent.

**A gap is a single-position state, not a per-page state.** `App.tsx:2243`
reads `shown.kind === 'gap' ? <Gap/> : mode === 'scroll' ? <column> : …` — the
gap replaces the whole page area in *every* mode, before the mode is consulted.
Scroll and two-page mode draw `grouped` (`App.tsx:1605`), built from the viewed
edition's own slots, every one of which exists. There is no scroll column with
a hole in it, and no half-borrowed pair.

Together these mean a borrowed page is **always drawn solo, in place of the
`Gap` view**. An earlier sketch during brainstorming showed one woven into a
scroll column between two real pages; that state is not reachable, and the
sketch was wrong.

## 2. The rule: which edition lends

A pure function in a new `web/src/modules/borrow.ts`:

```ts
export function lenderFor(
  state: ReadingState,
  resolver: Resolver,
  order: string[],
): { edition: string; slot: number } | undefined
```

`order` is `session.ts`'s `orderedEditions(hub, resolver.editions())` — hub
first, then the rest alphabetically. Candidates are that order minus the viewed
edition and minus the authority; the first that resolves from the authority
wins, resolved the same way `display` does it (`resolver.resolve` then
`resolver.indexOf`). If none resolves, the authority lends, at `state.slot`.

**It returns `undefined` in exactly one case**: `resolver.slotAt(authority,
slot)` is undefined, so there is no position at all and nothing can lend. That
is §1's surviving gap, and the only path by which `Gap` still reaches the
screen with this feature enabled.

**The alternative it beat: authority always.** Simpler, zero extra resolver
calls, nothing to order. It loses because on a work with three editions it
answers a dropped `en-official` page with the raw the reader just swapped away
from, when an `en-fan` page exists and is the thing they actually wanted. On
`gotoubun-v01` — `raw-mono`, `en-mono`, `en-fan` — that is the whole case.

**The alternative it also beat: a reader-set order per work.** Most control,
and it needs a stored per-work list, an editor for it, and a default anyway —
which is one of the two rules above. Deferred until the rule below is measured
against real reading.

**What the rule gets wrong, stated rather than discovered later.** Preferring a
third edition over the authority is right when the authority is the raw and the
third is another English release. It is wrong in the mirror case — authority
`en-fan`, viewed `en-official`, third `raw-mono` — where it draws the raw
instead of the other English edition. **Nothing available distinguishes them**:
the sidecar's `editions` block carries a free-text `label` and no language,
script or kind field (`docs/sidecar.md`), so "prefer a same-language edition"
is not expressible. The per-work order above is the escape hatch if this bites;
building it now would be guessing at a case no reading has produced.

## 3. The type: `Display` gains a third arm

```ts
export type Display =
  | { kind: 'at'; edition: string; slot: number; via: Landed['via'] }
  | { kind: 'borrowed'
      edition: string
      from: { edition: string; slot: number }
      lender: { edition: string; slot: number } }
  | { kind: 'gap'; edition: string; from: { edition: string; slot: number } }
```

**`edition` keeps one meaning across all three arms — the edition being
viewed.** Making it name the lender in the borrowed arm would have been the
obvious shape and is a trap: `App.tsx:1596` passes `shown.edition` into
`alignedBreaksCached`, so the grouping would be computed against the lender.
Nothing is drawn from that grouping while a gap is shown, so it would not be
visible — which is exactly what makes it worth refusing here rather than
finding later. `lender` carries both the edition and its slot, so no caller has
to re-resolve what this already knows; that is `nearest.ts`'s `NearestOffer`
reasoning, and O14's measured cost of a redundant resolve, applied again.

## 4. `display()` itself does not change

The borrow decision layers on top, applied once in `App.tsx` where `shown` is
computed:

```ts
export function withBorrow(
  shown: Display, state: ReadingState, resolver: Resolver,
  order: string[], prefs: BorrowPrefs,
): Display
```

It maps a `gap` to a `borrowed` when `prefs.enabled` and a lender exists, and
returns `shown` untouched otherwise.

**Teaching `display()` about borrowing directly was the obvious alternative and
is wrong.** `nearest.ts`'s `landingAt`, inside `nearestPaired`, calls `display`
and treats anything that is not `at` as not-landing. That is precisely right for
its purpose — an offer either side of a gap must land on a **real** page in the
edition the reader chose, not on another borrowed one. A `display` that
returned `borrowed` would silently turn every one of those offers into a jump
to yet another edition's page, which is the opposite of what the offers are
for. Keeping the borrow decision out of `display` means `nearestPaired` needs
no change at all.

## 5. The module and the setting

```ts
export interface BorrowPrefs { enabled: boolean }

export const borrowModule: ReaderModule<BorrowPrefs> = {
  id: 'borrow',
  label: 'Borrowed pages',
  defaults: { enabled: true },
}
```

Claims no keys. Nothing here is toggled mid-read — a reader decides once
whether they want gaps filled — which is `readouts`' and `prefetch`'s reasoning
verbatim, against `direction`'s `d` and `regroup`'s `b`.

Registration is one import and one array entry in `web/src/modules/registry.ts`.
Per that file's own doc, a module with a `defaults` shape and nothing else is
fully wired the moment it appears there: `App.tsx`'s preference state is a
single map built from `registry.all()`, and `Settings.tsx` generates the
checkbox from `fieldKind` reporting `'boolean'`. No hand-written panel entry,
and no edit to `settingsPanel.ts`.

**Defaults on**, matching `zoom`, `prefetch` and `help`, each of which declares
its own `enabled: boolean`. The "Later" entry's reason for wanting a setting —
a reader comparing translations may want to *see* that a page is missing — is
served by the switch existing, not by it defaulting off. A reader who never
opens settings is better served by the page than by the gap, which is the whole
premise of the entry.

## 6. The view, and the mark that is also the way back

`web/src/Borrowed.tsx`, mounted where `<Gap>` is today, wrapping a bare
`PageView`. That component works standalone — `decodeQueue`, `cohort`,
`makeImage` and `fit` all default (`web/src/PageView.tsx:66`) — so this needs
none of `App.tsx`'s decode machinery threaded through it. `buffered` is
`false`: the component is remounted per position rather than having its `src`
swapped under it, which is the only state `buffered` exists for.

The source is `pageUrl(work, lender.edition, page, longEdge)` — `pageUrl`
already takes an edition (`web/src/api.ts:37`), so drawing another edition's
page needs no new route and no server change. The page index is
`resolver.slotAt(lender.edition, lender.slot)!.page`, not the viewed edition's
`slots` array that `App.tsx`'s `pageAt` reads.

`alt` is `"page N, borrowed from <lender>"`, **unconditionally**. The
substitution must not be a purely visual signal; this is the one part of the
marking that is not a style choice.

The mark itself is a tinted leading edge plus a `<button>` naming the lender
and its page. Pressing it renders the existing `<Gap>` for that slot, offers
intact.

**Why the mark is the control rather than a fourth key.** The one thing a
reader might want instead of the borrowed page is a real page in the edition
they chose, and `Gap`'s before/after offers already are that — RTL-aware, and
the subject of the O25 fix. Reaching them from the thing that says the page is
borrowed makes them discoverable at the moment the question arises. A new
keybinding would be a fifth thing to learn for a screen most readers meet
rarely, and `borrow` claims no keys for the reason in §5. Keyboard reach is the
button's place in the tab order.

The toggle is `useState` inside `Borrowed`. `App.tsx` passes
``key={`${authority}:${slot}:${shown.edition}`}`` so React remounts it on any
real move — no effect to clear stale state, and no way for the Gap view to
persist across a page turn.

## 7. What this does not touch

`at` stays `undefined` in the borrowed case, exactly as in a gap today. So
annotations do not draw, zoom does not engage, and the page is not a prefetch
target. **Every existing rule that guards on `at` is unchanged**, which is what
keeps this off the correction UI, note anchoring, `pairing.ts`'s grouping, the
filmstrip and the readouts.

That is a real limitation and not a claim that it is ideal: a borrowed page is
a page, and a reader may well want to zoom it or read the lender's notes on it.
Building that means answering which edition's slot a note on a borrowed page
belongs to, and giving `mapRect` a case where the drawn page is neither the
viewed nor the authority edition — each of which has a measured design behind
it. **Recorded in `docs/notes.md` as something we may want later, deliberately
not as a `status.md` "Later" entry or an O-number**, so it is a note rather
than a commitment.

## 8. Testing

- **`borrow.test.ts`** (the `node` project, no DOM, against `fixtures.ts`'s
  `fakeResolver`): `lenderFor` prefers a third edition; falls back to the
  authority when there is no third; skips a third that does not resolve;
  respects `order`. `withBorrow` returns `shown` untouched when disabled, when
  `shown.kind !== 'gap'`, and when the authority slot is invalid — the one case
  where `Gap` still shows.
- **`Borrowed.test.tsx`** (the `jsdom` project, beside `Gap.test.tsx`): the
  mark names lender and page; `alt` says borrowed; the mark is a `button`;
  pressing it reveals the Gap offers.

`App.tsx`'s wiring is not reachable from Vitest — it value-imports
`resolver.ts` (see that file's own comment) — so that half is verified in the
browser against the corpus, which is this project's existing practice for every
`App.tsx` change.

**To be verified against real material on `gotoubun-v01`** — nothing here has
been run yet, per the status line at the top. That volume is the only corpus
material with three editions, and therefore the only place where §2's rule does
anything the rejected "authority always" rule would not.

## 9. What is deliberately not done

- **No per-work lender order.** §2 says why, and what would open it.
- **No annotations, zoom or prefetch on a borrowed page.** §7.
- **No borrowed pages woven into scroll or two-page layout.** §1 shows the
  state is not reachable; building for it would be building against a shape
  the reader does not have.
- **No new keybinding.** §6.
- **No server change.** `pageUrl` already takes an edition.

## 10. What could not be measured

**How often a gap actually arises in real reading**, which is the number that
says whether this feature is felt or theoretical. It is computable — for each
ordered pair of editions of a work, count the authority slots that do not
resolve — but it needs a harness that does not exist and the corpus mounted,
and no figure here should be quoted for it until one is run. The one adjacent
figure that *is* measured is `alignedPairs.ts`'s: `gotoubun-v01` needs 4 solo
groups in `en-mono`, 2 in `en-fan` and 2 in `raw-mono`, which counts pairing
boundaries rather than gaps and must not be read as a proxy.

**Whether the two-edition degeneration is annoying in practice.** On a work
with two editions the rule has no third to prefer, so it draws the page the
reader just swapped away from, marked. That is most of the corpus. It is
honest, and the mark is what distinguishes it from the swap having done
nothing — but whether a reader finds it useful or irritating is a question only
reading answers, and this design does not claim to know.
