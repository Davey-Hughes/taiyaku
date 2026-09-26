# The admin follow-ups — design

**Date:** 2026-09-25. **Branch:** `admin-follow-ups`, from `main` at
`e8eecfc5`. **Closes:** O149, O153, O155, O157, O159.

Five open items the style pass ⑤ left on the admin screens, done as one PR.
Three were **decisions** (O149, O153, O155). Each was drawn on the real
library and the user chose between the drawings on 2026-09-25. The other two
were **defects** with an approach to choose (O157, O159), and the user chose
those the same day. The user decided to bundle them: the §5 checklist of
the style pass spec (2026-09-17) costs the same for one item as for five,
and each item alone is under an hour of code. The price is one squash
commit, so no single item can be reverted on its own.

---

## 0. What exists, measured

Measured on 2026-09-25 in Playwright-driven Chromium, against a copy of the
real library (`/home/davey/temp/manga-test/library.db`, 138 series, 1,896
works). The copy is at `target/o-bundle/lib/library.db`. The original's
checksum `d5ebc0c2…e59e67` was the same before and after. The drawings are in
`target/o-bundle/drawn/`, and the comparison page the user chose from is
<https://claude.ai/artifact/MT1w2HNvo2mYzkBZkGyMWT>.

| item | figure |
|---|---|
| O149, rule 1 selected | 224 rows. `edition_key`, `kind`, `language`, `title` set in **0** each; `series` 95, `volume` 61, `chapter` 20 |
| O149 at 390 px | table **614 px** in a **343 px** box today; **366 px** with the empty columns dropped |
| O153 | the role hint and the password hint: 12 px, `rgb(107, 97, 81)`. The add-account hint: **16 px**, `rgb(35, 30, 23)` |
| O155 | 35 open: 18 at score 0, 13 at 1, 3 at 2, 1 at 3. The default view shows 17, **13 chipped `weak`** |
| O159, Chromium | a `disabled` checked checkbox draws grey, and `accent-color`, `opacity: 1` and `filter: none` on `:disabled` do not bring the fill back. The same box with `aria-disabled="true"` and no `disabled` draws exactly like the enabled one (`o159-strip.png`, top to bottom: enabled, disabled, disabled plus CSS, aria-disabled) |

---

## 1. The decisions

| item | chosen | against |
|---|---|---|
| O149 | **C**: drop a column that is empty in every row, and print one quiet line **above** the table naming what was dropped | A, all eight columns; B, drop without saying so |
| O153 | **B**: mute the add-account hint to match the other two | C, un-mute the two short ones to 16 px |
| O155 | **C**: a third word. Score 0 `weak`, 1 **`maybe`**, 2 and up `strong` | B, `strength` draws its line at 1; D, `INBOX_MIN_SCORE` moves to 2 |
| O157 | **An invisible hit area**, under `(pointer: coarse)` only, with nothing drawn differently | 44 px rows drawn as such, which §12.8 of the style pass spec measured as a redesign |
| O159 | **The card's checkboxes use `aria-disabled` while saving**, not `disabled` | the same for all eleven controls; leaving it and closing the item |

**O149 moves the line from where it was first chosen.** §12.8 of the style
pass spec records the same option C, chosen on 2026-09-24, with the line
*beneath* the table. The table has 224 rows, so a line beneath it sits 224
rows down the page, and the drawing the user chose on 2026-09-25 puts it
above. This spec supersedes §12.8 on placement only.

**O155 C is the only option that keeps both closed decisions.** The
curation build spec (2026-08-26) set both lines. `strength` puts strong at 2
and up "so a two-signal `edition-of` match ranks with the two kinds §0's
`CERTAIN` places at 3". The threshold hides score 0 because score 0 is "the
suffix matches no edition word, and the volume counts differ", which is
exactly the false positive C3 §4 names. Score 1 is a single signal, which
`proposals.rs` calls "exactly the coin flip". B would reverse the first
decision and D the second. C adds a word between them. After C, "Show weak
proposals" reveals exactly the rows that say `weak`.

---

## 2. The changes

### 2.1 O149: the Provenance table shows what is set

`PreviewTabs.tsx` draws `file` plus only those `FIELDS` that at least one
shown row has in `decided_by`. A field decided by the root default counts as
set. The unset fields are computed by a pure helper in `rules.ts`. It takes
the rows and returns the unset fields in `FIELDS` order, so a `node` test
covers it without importing a `.tsx`. `FIELDS` moves into `rules.ts` with it.

When one or more fields are unset, one paragraph sits between the tab strip
and `.table-scroll`:

> edition_key, kind, language and title are not set for any of these 224 files.

- **One field:** "title is not set for any of these 224 files."
- **All seven:** the table is `file` alone, and the line names all seven.
- **One row:** "…for this file", not "any of these 1 files".
- **The 500-row cap:** when the route capped the rows, the sentence says
  "the first 500 files" and not "these 500 files". **The Provenance tab
  draws no cap notice today.** Only Matched files does (`.truncated`,
  "The first 500 files only — more matched than fit here"), which breaks
  `PreviewTabs`' own rule that "the row cap is drawn". So the same notice is
  drawn under the Provenance table too. Found while planning, on
  2026-09-25. Without it, the new line would be the only thing on the tab
  counting files, and would be easy to read as the whole library.
- **Nothing unset:** no paragraph.

The paragraph has the class `unset`, and a new `.preview .unset` rule beside
`.preview th` draws it muted at `--text-meta`, like every other explanatory
line on these screens. No existing rule on this screen draws a quiet
paragraph. Its neighbour `.truncated`, the 500-row notice, has no rule at
all (§3).

**What this changes:** the table's columns now follow the data. Selecting a
different rule can show a different set of columns. The user accepted that
with option C, and it is the reason for the line.

*Amendment, 2026-09-26: the cap notice is drawn above the table, on both
tabs.* The bullet above draws it under the Provenance table, as Matched
files drew it under its own. By the user's decision of 2026-09-26 it sits
above the table on both, for the reason §1 gives for the line: when the
notice shows, the table holds 500 rows, and beneath them it is read after
every row it qualifies. On Provenance the order is
the cap notice, then the line, then the table. The text above is left as
written.

### 2.2 O153: one quiet-text rule for every `.why` on the Users screen

`admin.css`'s `.user-row .why, .user-row .locked` becomes
`.users .why, .user-row .locked`. `.users` is the `<section>` `Users.tsx`
renders, so the add-account form's `.why` is inside it. The margin rule
`.user-row > .why` is unchanged. The comment above the 60ch measure says
"only the one in `.add-user`, which no rule here reaches, is larger. O153 has
that", which stops being true, so it is corrected.

**Not changed:** the grant box's `.unrestricted` paragraph ("Nothing ticked
means unrestricted…") stays at 16 px. It is not a `.why`, and it is the one
hint on the card that warns about access.

### 2.3 O155: `strength` has three answers

`curation.ts`: `strength(score): 'strong' | 'maybe' | 'weak'` is `strong`
at 2 and up, `maybe` at 1, and `weak` at 0. `ProposalRow.tsx` writes it on the
`<li>` and in the `.score` chip as it does now. No rule in `admin.css`
matches `li.weak` or `li.maybe`: the plain chip is the base `.proposals
.score` rule, and only `li.strong` is marked. So `maybe` draws like `weak`
with no CSS added, which is how it was drawn.

The strength-strip comment in `admin.css` says "the threshold defaults to 1 —
so a weak row on screen is one somebody asked to see". That was false until
this change, because score-1 rows showed by default and said `weak`. It is
rewritten to name all three words.

`EmptyState`'s `inbox-weak-hidden` copy says "Show weak proposals reveals the
ones below it — a weak proposal is hidden, never dropped". It becomes
exactly true and does not change.

### 2.4 O157: 44 px to the touch, nothing drawn differently

Inside `admin.css`'s existing `@media (pointer: coarse)` block:

- **`.config-nav a`, `.proposal-tabs > button`, `.preview-tabs > button`**
  take `position: relative`, plus an `::after` with no content that is
  absolutely positioned, centred on the item, and at least 44 × 44 px. The
  item's own box, the strip's height and the underline's distance from the
  word all stay as ⑤c measured them. The `:not()` exclusion that keeps the
  floor's `min-height` off the two button rows stays, because this is a
  different mechanism and not the floor.
- **`RuleList`'s rule-enable checkbox** is wrapped in a `<label>` of its own.
  Under `(pointer: coarse)` the label is an inline grid, 44 × 44, centring the
  box. The box is drawn at its current size. The `aria-label` stays on the
  `<input>`, and the wrapper adds no text.
- **`RuleEditor`'s file input leaves O157.** It has the `hidden`
  attribute, so it is not rendered and cannot be touched. The Import
  button that opens it already clears 44.

**The risk is overlap.** An extended area overlaps whatever is beside or
below the item. The inbox's tab strip sits above the "Show weak proposals"
checkbox row, and the config sub-nav sits above the screen's heading. The
plan measures every extended rectangle, in a Chromium context with
`hasTouch` and `isMobile` at 390 px, against its neighbours'
`getBoundingClientRect()`. The areas must not intersect another control.
If one does, that axis is shrunk until it doesn't, and the shortfall is
recorded. The vitest browser project cannot run this, because it does not
match `(pointer: coarse)`, as the floor's own comment records. So the
guard that runs in CI is a `node` test over the stylesheet (`testing/css.ts`'s
`rules()`), asserting each subject has the `::after` in the coarse block.

*Amendment, 2026-09-26, to this section and §1's O157 row: "nothing drawn
differently" holds for the three tab rows, not for the enable label.* Under
`(pointer: coarse)` the label takes 44 px of the row's layout, so a long
rule name can wrap one line more. The user accepted that on 2026-09-26.
Outside a coarse pointer, `.rule-row .enable { display: flex }` keeps the
box where it was. The overlap check above was run against every visible
control in the document and found no intersection (`docs/notes.md`, "Admin
follow-ups"). The text above is left as written.

### 2.5 O159: the lock stays, the grey goes

In `UserRow.tsx`, the three permission checkboxes and the per-root grant
checkboxes change `disabled={saving}` to `aria-disabled={saving}`. Nothing
else in the card changes. The role `<select>`, the password field, and the
Set password, revoke and Remove… buttons keep `disabled`.

**The lock is `begin()`'s, not `disabled`'s.** `begin` refuses through
`inFlight`, a ref, whatever `disabled` says. A tick while saving reaches
`save`, `begin` returns false, and nothing is sent. Each box is controlled
(`checked={…}` from `shown`), so React puts the box back to its real value.
`settled`'s reason for holding the card until the reload stays whole: a
second grant tick inside the window is still refused. It is refused by the
ref rather than by the UA.

**What a screen reader hears:** `aria-disabled="true"` is announced as
dimmed or unavailable, so the state is not lost.

---

## 3. Not in this PR

- **O156**, `aria-pressed` against `aria-current` on four one-of-N rows. Its
  row names the accessibility pass as what settles it, and that pass comes
  next.
- **O158**, the class-name-to-rule check. It is app-wide: 432 `className`
  sites in 46 files, 31 of them computed.
- **The add-account paragraph's copy.** It tells an administrator "C4 §4"
  and names `004_auth.sql`, which are the repository's words, not the
  reader's. Changing what a screen says is its own decision. It gets **filed
  on the branch** as a new open item, per §5's rule 6.
- **The `.unrestricted` hint's size** (§2.2).
- **The `.truncated` notice's size.** "The first 500 files only — more
  matched than fit here" has no rule and draws at body size. It says rows
  are missing, not why a control behaves as it does, so O153's reasoning
  does not settle it. It is left as it is and not filed.

---

## 4. Tests and checks

The style pass spec's §5 list applies unchanged: `scripts/check-all.sh` with
its `all green` line quoted, a browser pass at 390 × 844 and 1440 × 900 in
Paper and Ink, **every new test watched failing with its defect put back by
hand**, a whole-branch review and a review of each fix round until one finds
nothing, the pre-merge claim check, and anything found and not fixed filed.

The tests, one or more per item, each named with the defect it has to catch:

| item | test | the defect it must fail on |
|---|---|---|
| O149 | the helper, `node`: each field set in some row is kept, each set in none is returned, `FIELDS` order is kept, `root_default` counts as set | returning the set fields; dropping `root_default`; sorting |
| O149 | `PreviewTabs`, `jsdom`: the `<th>`s are `file` plus the set fields; the line's text for one, several, and no unset fields; singular for one row; "the first N" and the cap notice when capped | the old `FIELDS.map` header; the line beneath the table rather than above it; a Provenance tab that stays silent about the cap |
| O153 | `admin.css`, `node`: the quiet rule's selector reaches `.users .why` | the old `.user-row .why` |
| O155 | `strength`, `node`: 0 → `weak`, 1 → `maybe`, 2 → `strong`, 3 → `strong` | the old two-way split, which answers `weak` at 1 |
| O155 | `ProposalRow`, `jsdom`: a score-1 row's chip reads `maybe` and its `<li>` has the class `maybe` | the chip hardcoded, or class and chip out of step |
| O157 | `admin.css`, `node`: each of the three tab subjects has an `::after` inside the coarse block with a 44 px minimum on both axes | a missing subject; a 44 px `min-height` on the item itself, which is the redesign |
| O157 | `RuleList`, `jsdom`: the enable box is inside a `<label>`, and its `aria-label` is unchanged | the bare `<input>` |
| O159 | `UserRow`, `jsdom`: while a save is in flight, each checkbox has `aria-disabled="true"` and no `disabled`; a click on one sends no second `PATCH`, and the box still shows the value it had | `disabled={saving}` restored; `begin`'s ref guard removed, which a test on the attribute alone would pass |

**O159's last row is the one to watch.** A test that checks only the
attribute passes against a card with no lock at all. The watched-failing
step has to remove `begin`'s `inFlight` check and see the no-second-`PATCH`
assertion go red.

**O157's measurement is a manual step, not a test.** It is recorded in
`docs/notes.md` with the rectangles, because nothing in CI can emulate a
coarse pointer.

### 4.1 Bookkeeping

`docs/status.md` closes five rows. Each close is the five-part edit, and the
sweep runs on the way in and on the way out. The branch also opens one row
(§3's copy item). The count moves from 159 rows, 94 closed, 65 open to
**160 rows, 99 closed, 61 open**. O157's prose bullet records that the file
input was dropped from it and why. The web test counts are measured from
the two vitest runs, not added up by hand. §12.8 of the style pass spec
keeps its text as written. One line is added under its O149 bullet saying
that this spec moved the line above the table.
