# The credit-pages panel, and the skip that governs something — design

**Status: designed, not built.** Settled in brainstorming on 2026-08-07. This
builds the remaining half of §4 of [`../../status.md`](../../status.md),
"Skipping scanlation credit pages", whose sidecar half shipped in `#15` and
whose reader half has never been wired at all.

Every figure quoted below is measured or read out of the corpus. Where this
document expects a result it has not yet produced, it says so and names the run
that would establish it.

## What §4 asks for, and what is actually in the tree

§4 says the reader's half is missing and that "the 'Credit pages' settings
section holds two toggles that persist and govern nothing". That is exactly
true, and stronger than it sounds — **nothing** of `modules/credits.ts` is
reachable from the reader:

| piece | state |
| --- | --- |
| `creditsModule` | registered; `defaults` render a settings section |
| `PANEL_INTENT` (`C`) | no `case` in `App.tsx`'s module switch — swallowed by its `preventDefault()` |
| `creditSlots` | exported, tested, called by nothing |
| `nextReadable` | exported, tested (9 references), called by nothing |

O25 was the stated blocker and is closed, so the panel now has a mount site.
What remains is that both toggles start governing something.

## 1. The panel

`C` opens a list of the **viewed** edition's credit pages. `Resolver.credits`
already exposes them — `credits(edition): number[]`, zero-based page indices,
straight off the sidecar's per-edition `credits` field — so nothing new crosses
the wasm boundary.

**A new `web/src/CreditsPanel.tsx`**, following `NotePanel`'s shape:

```ts
export interface CreditsPanelProps {
  visible: boolean
  work: string
  /** The edition being read. The list is its credit pages, not the work's. */
  edition: string
  /** Zero-based page indices, `Resolver.credits`' own unit. */
  pages: number[]
  /** Whether `skip` is on, so the panel can say what the list currently means. */
  skipping: boolean
  /** Take the reader to a page. Zero-based; `App.tsx` converts to a slot. */
  onGo: (page: number) => void
  onClose: () => void
}
```

Each entry is a `thumbUrl(work, edition, page)` thumbnail plus `page N`,
1-based at the display boundary the way `altAt` and `Gap`'s numbers already
are. **No labels.** The sidecar carries page indices and nothing else; which
page is a donation card and which is a staff-credits page is something §4
established by *looking at them*, not something the format records. A panel
that named them would be inventing the distinction.

The empty case says so in words — `No credit pages found in raw-mono.` —
following `chapterLine`'s precedent, where "an empty readout reads as a bug, a
labelled one reads as a fact about this particular work". It is the common
case: of the five editions across the two corpus sidecars, none has a `credits`
field at all today, and `raw-mono` would have none even once they are written.

**Taking an entry navigates there even while `skip` is on.** The module's own
doc is normative here: "nothing is deleted and nothing is meant to become
unreachable". The reader lands on a slot that is filtered out of the groups,
which is precisely the stranded state §2 defines a rule for — so the next arrow
carries them back out.

**`panel: false` makes `C` inert and mounts nothing.** That is what the
preference means ("Offer the panel that lists them"), and it is checked in the
`PANEL_INTENT` case rather than inside the component, so a reader who has
switched the panel off pays no render.

## 2. Skip filters groups, not slots

**Where it bites.** `App.tsx` computes `grouped = groups(slots, mode, …)` and
hands the same array to the page area, the arrow keys (via `groupedRef`) and
the filmstrip. Filtering *there* makes one change govern all three:

```ts
export function readableGroups(all: Group[], skipped: Set<number>): Group[]
```

**A group is dropped only when every one of its slots is skipped.** That matters
in `two` mode, where a pairing can put a credit page beside an ordinary one —
dropping that group would hide a page the reader wants. A joined spread is
always both-or-neither, because a credit card's two halves are one card, which
is the behaviour `creditSlots`' tests already pin and which §3 preserves.

**Slot numbers are untouched.** `Group.slots` holds indices into the unfiltered
`slots` array, so removing groups changes what is drawn and never what anything
is called. This is §4's "skipping never renumbers" and the swap precedent
behind it: the position readout still reads `slot 89 / 195` on a volume with
four skipped pages, and jumps rather than counting differently.

**The stranded case.** `stepToGroup` currently returns the slot unchanged when
`findIndex` gives `-1`. That is unreachable today and becomes ordinary here: a
reader can stand on a skipped slot by taking a panel entry, by clicking a
filmstrip tile before switching `skip` on, or by switching it on where they
already are. It gains a fallback — step to the nearest surviving group in the
direction of travel:

```
slots   0  1  2 [3] 4  5        [3] skipped
groups  □  □  □     □  □

on 2, → lands on 4          (ordinary: one group along)
on 3, → lands on 4, ← on 2  (stranded: nearest either side)
on 3 with 4,5 also skipped, → returns 3   (nothing ahead; the boundary answer)
```

**The fallback needs no knowledge of which slots are skipped.** It reads the
filtered group list, which already encodes the answer. That is the argument for
doing this in group space rather than slot space, and it is why `nextReadable`
does not survive — see §3.

## 3. Two functions go, and one document sentence with them

**`creditSlots` is replaced, not adopted.** Its own doc already says why, in
two parts, and both are answered by the same replacement:

```ts
/** The slots of `edition` that `pages` presents. */
export function creditSlotsIn(slots: Position[], pages: number[]): number[]
```

`slots` is what `App.tsx`'s `allSlots(resolver, edition)` already produces, so
slot expansion comes from `taiyaku_core::slot::expand` through the wasm binding
— one derivation, read rather than repeated — instead of the TypeScript loop
that was a second consumer in a third language. And it needs no `Pagination`,
which was the other objection: nothing in `web/` could build one, since
`Resolver` exposes no page count and no spread list. `Pagination` goes with it.

A joined spread still contributes both slots, because both carry the same
`page` and the filter is on `page`. That is the property `creditSlots`' tests
pin, and §5 carries them over rather than dropping them.

**`nextReadable` goes.** It walks slots; arrows have stepped by *group* since
`c11c1e9` ("An arrow moves to the next image, not the next slot"), three
commits before `nextReadable` was written. Keeping it as the primary mechanism
would mean either stepping by slot again — reintroducing the two-presses-for-a-
spread bug that commit fixed — or mapping its answer back to a group, which is
the group filter with extra steps. It also governs arrows only: under it a
skipped page would still be a filmstrip tile and still render in scroll mode,
so "skipped" would mean *skipped by the arrow keys and by nothing else*, which
is not §4's "out of the reading flow".

Its one unique capability is that it works when the starting slot is itself
skipped. The group-space fallback in §2 covers that case without needing the
skipped set threaded into the step path.

**So `status.md` §4's sentence naming it is wrong and must be corrected**: "the
readout jumps rather than counting differently … carried out by `nextReadable`,
which walks to the next unskipped slot without renumbering." The first half
stands. The mechanism named in the second half does not, and the correction
should say what replaced it and why, the way this branch's other corrections do.

## 4. Where it wires in

All within `App.tsx`, and all at the single mount site O25 left:

- `const [creditsVisible, setCreditsVisible] = useState(false)` beside
  `settingsVisible`.
- `case PANEL_INTENT:` in the module switch, flipping it — gated on the
  `panel` preference. The switch's own comment invites this: "The next module
  to claim a key adds the next case."
- `const creditPages = resolver.credits(shown.edition)` and
  `const skippedSlots = creditsPrefs.skip ? new Set(creditSlotsIn(slots, creditPages)) : EMPTY`.
- `const grouped = readableGroups(groups(slots, mode, …), skippedSlots)` — one
  line, and the page area, `groupedRef` and `Chrome` all inherit it.
- `<CreditsPanel …/>` beside `<Settings/>`.

`onGo` converts a page to a slot with the same `slots.findIndex` the panel's
own list already implies, then calls `goToSlot`.

**In a gap the panel still works**: the viewed edition has credit pages whether
or not the authority slot resolves into it, and `shown.edition` is defined in
both states. It mounts under `at` for nothing.

## 5. Verification

**Unit, in `vitest`, against the pure functions:**

- `readableGroups` — a group all of whose slots are skipped is dropped; a `two`
  mode group with one skipped slot and one ordinary slot is **kept**; a joined
  spread with both slots skipped is dropped whole; an empty skipped set returns
  the input unchanged (identity, so `skip: false` costs nothing).
- `stepToGroup`'s stranded case — forward, backward, and the boundary where
  nothing survives in the direction of travel, which must return the slot
  unchanged rather than jumping to the far end.
- `creditSlotsIn` — carried over from `creditSlots`' existing tests, including
  the joined spread contributing two slots, restated against a `Position[]`
  instead of a `Pagination`.

**A browser pass — and it needs a fixture built first.**

> **No corpus sidecar has a `credits` field today.** Read directly: all five
> editions across `corpus/gotoubun-v01` and `corpus/kaguya-v01` report
> `credits: None`. So `Resolver.credits` returns `[]` everywhere, the panel
> would render its empty state, and `skip` would skip nothing. **A pass against
> the corpus as it stands cannot fail** — the same trap the O25 plan fell into
> when it named a gap whose anchor check could not distinguish pass from fail.

The pass therefore begins by producing one, on a **copy** of the sidecar:

```sh
cp corpus/gotoubun-v01/taiyaku.json "$SCRATCH/credits-fixture.json"
./target/release/taiyaku credits \
  'en-fan/Go-Toubun no Hanayome - Fan Translated - c00'{0,1,2,3,4,5}'.cbz' \
  --sidecar "$SCRATCH/credits-fixture.json" --write
```

(The member paths are the sidecar's own `editions['en-fan'].members`; read them
from it rather than retyping. `c005.5` is the seventh and contributes nothing.)

`taiyaku credits` proposes and "writes nothing without `--write`", and the
gated corpus check `gotoubun_fan_edition_classifies_four_credit_pages` pins
exactly what it should find:

```rust
assert_eq!(pages, vec![95, 168, 198, 225],
    "the classified set must be exactly these four pages, not merely four of six");
```

**Those are zero-based page indices**, not display numbers — `credit_pages`
reads run geometry, which is internal throughout this project — so the panel
renders them as **pages 96, 169, 199 and 226**. `status.md` §4's prose says "at
pages 95, 168, 198 and 225" without saying which base it means; the assertion
above is what settles it, and the pass should confirm the panel's four numbers
are the display forms rather than the raw ones. None of the four is a joined
spread (`en-fan`'s spreads are 45, 91, 133, 164, 196), so each is one slot.

What the pass then confirms:

1. `C` opens the panel; it lists exactly four entries, each with a thumbnail,
   reading `page 96`, `page 169`, `page 199`, `page 226`.
2. Viewing `raw-mono`, the panel says `No credit pages found in raw-mono.`
3. With `skip` off, the filmstrip shows all 235 `en-fan` tiles and an arrow
   through page 95 lands on it.
4. With `skip` on, the strip shows **231** in `single` mode — four one-slot
   groups dropped — and an arrow that would have landed on page index 95 lands
   on the next readable page instead. In `two` mode the count differs, and the
   check to run there is the mixed group: a credit page paired with an ordinary
   one keeps the group, so the ordinary page stays reachable.
5. The position readout is **unchanged** by the toggle — still `/ 240` — which
   is "skipping never renumbers" on screen.
6. Taking the panel's `page 96` entry lands on page index 95 anyway, with
   `skip` still on; the next arrow leaves, in either direction.
7. `panel: false` makes `C` do nothing.

Then the copy is discarded and the corpus's own sidecar is untouched. `corpus/`
is gitignored, so git cannot undo a write there — the copy is the only undo.

## What this does not do

- **No labels or classification in the panel.** The sidecar records page
  indices; anything more would be invented.
- **No renumbering, and no change to the position readout's total.** §4 and
  `docs/sidecar.md` are normative: the slot is the addressed unit, and a
  per-reader toggle that changed what an address meant would make one device's
  progress and another's notes wrong.
- **No change to `taiyaku credits` or to the rule behind it.** Recall is bounded
  above rather than known — `c000`'s page 43 is a credits page the solver
  *paired*, so no unpaired-based signal can reach it — and that is a CLI
  question this document does not reopen.
- **Nothing about the false positive.** Kaguya's page 212 is a colour
  announcement the rule classifies and a corpus check pins deliberately. The
  panel will list it if that sidecar is ever written, which is the honest
  outcome: the panel shows what the sidecar says.
