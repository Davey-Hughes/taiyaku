# O95 — the Conflicts tab becomes the Overrides tab: build design

**Status: settled in brainstorming on 2026-09-05, and built the same day.**

Written in the style of the rest of `docs/`: every decision records the
alternative it beat and what it cost, a figure asserted here was measured on
this machine rather than recalled, and a guess says it is one.

This closes **O95**, opened 2026-09-05 by driving C4 PR B's rule editor against
the real library. The item's own entry in `docs/status.md` carries the
measurement and the argument; this document does not restate them, and settles
only what the entry left open — how far the reframing reaches into the names,
what the row draws, and what it offers instead of its one button.

The product design is
[`2026-08-14-library-configuration-design.md`](2026-08-14-library-configuration-design.md)
(`C4 §1`) and the build design is
[`2026-09-02-library-configuration-build-design.md`](2026-09-02-library-configuration-build-design.md)
(`build §2.1`). Section references below name which.

## 0. The defect in one paragraph

On a library holding nothing but the four rules migration 3 seeds, the
Conflicts tab opens reading **1 conflict, 178 examples**. Seeded rule 1 reads
the series from the directory and seeded rule 4 reads it from the filename;
rule 4 wins on ordinal, and **that override is what folds a colour edition into
its main series** — rule 4's own seeded note says so. The row's sole action
reads *"Move rule 4 above rule 1"*, and pressing it makes rule 1 win, the series
becomes `Death Note (Colored Council)`, and the fold stops. **The detection is
right and the remedy is wrong.** The screen states a correct observation in the
grammar of a correction, and then offers the correction.

`build §2.1` already narrowed C4 §1's letter once, from "two rules set the same
field" to "two rules decide it differently", *"because taken literally it flags
the shipped seeded defaults"*. That narrowing does not reach this case, because
producing a **different, normalised value** is exactly what the pairing is for.
A second narrowing cannot reach it either: the thing that distinguishes this
pair from a genuine mistake is **intent**, and intent is not a column.

## 1. What ships: one word, end to end

**Decided by the owner 2026-09-05.** `Conflict` becomes `Override` in the
store, `conflicts` becomes `overrides` on the wire, and the tab reads
**Overrides**. The rename is mechanical and about 140 mentions, most of them
call sites in `preview/tests.rs` and `config_api/tests.rs`.

**Rejected: renaming in the web layer only** — the tab and the row's copy,
leaving the store and the API saying `conflicts`. It is a tenth of the diff and
it buys a permanent, documentable drift: the screen and the route it calls would
use two words for one thing, and every future reader of either would have to be
told why. This repository has taken that trade before and recorded it; there is
no reason to here, because the store's own doc comment *already* describes the
thing as an override in as many words — *"one field where a later enabled rule
silently overrides an earlier one's genuinely different answer"*. Only the type
name disagreed with its own documentation.

**Rejected: leaving the names alone and changing only the row's sentence.** The
badge on the tab strip and the `N conflicts` count in `RuleList` are the two
places a person meets this feature before they ever open the tab, and both would
keep the alarm word. O95 says the badge is not the defect — it does not say the
word on it is right.

**Not renamed: `409 Conflict`.** `users_api.rs` and `config_api.rs` both use the
HTTP status by its own name, which is unrelated to this feature and stays.

## 2. The row carries the two values

**Decided by the owner 2026-09-05.** `Override` gains `first_value` and
`wins_value`, and the row draws them.

Without them the row can say *which* rules disagreed but not *what about*, and
that is exactly the judgement it is now asking a person to make. `Death Note
(Colored Council)` → `Death Note` reads instantly as a deliberate fold; a row
that says only "rule 4 overrides rule 1 on 178 files" reads as neither, and the
person has to reconstruct both values by hand from two regexes and a path.

`conflicts` already computes both — the `Snapshot` chain is how it tells a
disagreement from two rules agreeing in the first place — so this is plumbing an
existing value out, not a second pass.

**The values belong to the file the row names, not to all `examples` of it.**
The row's `rel_path` is the first entry to produce its `(field, first, wins)`
triple, and the two values are captured in the same `or_insert_with` that
captures the path. Two entries sharing one triple can hold different values —
that is a per-`scan_entry` fact, the same one `others` already takes a maximum
over — so the row's example and its values are always one entry's, and the doc
comment says so rather than leaving the row to imply the 178 files all read
alike.

**`Snapshot` becomes `pub` and is renamed `FieldValue`.** It was private because
nothing outside the module needed it; it is now half of what a row means.
`Text(Option<String>)` and `Number(Option<f64>)` keep the typed distinction all
the way to the wire, so `volume` serialises as a JSON number and `series` as a
string, matching how `Placement` already crosses that boundary.

**`Override` loses its `Eq` derive and keeps `PartialEq`**, because an `f64` is
now inside it. Nothing needed `Eq` — the type is never a map key or a set
member; `grouped`'s key is the `(Field, usize, usize)` triple, not the struct.

**`grouped`'s value becomes a named struct, `PartialOverride`.** It was a
three-tuple and clippy's `type_complexity` refuses the five-tuple the two
values make of it — which is the right refusal for a second reason: `examples`
and `others` are both counts and `first_value` and `wins_value` are both
values, so a positional read of either pair is one transposition away from a
row that says something untrue.

## 3. Two options, under a lead that is not an instruction

The row draws:

```
series — rule 4 overrides rule 1 on 178 files
Death Note (Colored Council)/Death Note v01.cbz
    rule 1   ^(?<series>[^/]+)/        →  Death Note (Colored Council)
    rule 4   ^(?<series>.+?)[ _-]+v    →  Death Note            (wins)
Options:  [ Move rule 4 above rule 1 ]  [ Edit rule 4 ]
```

and the tab opens with one line above the list:

> A later rule deciding a field differently is not always a mistake — layering a
> normalised value over a default is what the seeded rules do. These are the
> places one rule's value replaces another's.

**"Options:" is doing the work, not the buttons.** A single button under a
sentence in the grammar of a warning is an instruction whatever it says. Two,
under a word that names them as alternatives, is the shape that reads right for
a deliberate layering and for a genuine mistake both — which is the whole of
what O95 asks for.

**"Edit rule 4" is the narrow half, and it does not narrow anything.** Narrowing
a rule means editing a regex, and no button can guess how. What it can do is put
the winning rule in the form — which is `RuleList`'s own `onSelect`, so the
gesture costs exactly one derivation, the same as clicking rule 4 in the list.
`PreviewTabs` gains `onSelectRule: (index: number) => void` for it.

**Rejected: a "Narrow rule 4" button that appends a negative lookahead** or
otherwise edits the pattern for you. It would have to guess which part of the
match to exclude, it would write a regex nobody asked for into a saved rule, and
on this very case the right narrowing is *none* — the pairing is correct.

**Rejected: no buttons at all.** Reorder is a real remedy when the override *is*
a mistake, and it is one click here against a drag in the list. Removing it
would fix the framing by removing the function.

**`others` keeps its sentence, reworded**: `2 other rules also disagree here.`
Nothing about the chain-of-three decision recorded on `Conflict` 2026-09-03
changes; only the noun does.

## 4. What this does not change

- **Detection.** `overrides()` finds exactly what `conflicts()` found. No
  exemption, no suppression, no special case for the seeded rule ids — O95 rules
  all three out, and the second goes stale the moment somebody edits a seeded
  rule.
- **The badge.** The count stays on the tab strip, for the reason
  `PreviewTabs.tsx`'s own module doc gives: a warning-that-is-a-tab has the
  failure mode a warning does not.
- **Cost.** No new pass, no new engine, no new query. The two values were
  already computed and thrown away.

## 5. Testing

Renames aside, three tests are new:

- **The row's values are the row's example's.** Two `scan_entry`s share one
  `(field, first, wins)` triple and disagree on the values; the row's
  `first_value`/`wins_value` are the ones belonging to the `rel_path` it names.
  This is the assertion that would catch a future refactor capturing the values
  in the wrong place — a `.max()` or a last-write-wins, either of which produces
  a row whose file and values contradict each other.
- **The seeded pairing, executed.** Seeded rules 1 and 4 over a
  `X (Colored Council)/X v01.cbz` layout produce one `Override` whose
  `first_value` is the directory's series and whose `wins_value` is the
  filename's. The existing test asserts the count; this asserts the two values,
  which is what the screen now draws.
- **"Edit rule 4" selects the winner**, not the loser and not the selected rule
  — `onSelectRule` called with `wins`.
- **The two numeric arms report their own field.** Added after the sweep:
  `field_value`'s `Volume` and `Chapter` could be exchanged with every test in
  the workspace passing, because `FieldValue` derives no `Default` and so the
  function has no viable mutant at all — O88's second cause. A `Volume` chain
  compared on `chapter` values drops real disagreements, and the row now draws
  the value as well. `docs/mutants-baseline.md`'s run log carries the
  measurement.

**Every new test gets its own defect hand-applied and watched to fail** before
it is called done, per the eleven found that way on C4 PR B.

The web tests that name the tab, the badge and the row's sentence all move to
the new wording; `RuleList.test.tsx`'s badge assertion moves with them.

## 6. Documentation

- `docs/status.md`: O95's five-part close, plus the architecture paragraph that
  describes the tab by its old name, plus the test count where it appears twice.
- `build §2.1` gains this decision, since that section is where the first
  narrowing was recorded and a reader arriving at it should not have to find out
  elsewhere that the noun changed.
- **`2026-09-04-c4-screens-design.md` §5.2 and §5.4**, which are the design for
  the screen itself and specify the tab by name, its wire key, its row's
  contents and its two actions. This list originally named only the two above
  and the second whole-branch review found the omission: §5.2 still said
  *"**Conflicts** — `conflicts` … C4 §1's two offered fixes … are the two
  actions on the row"*, so a reader building against it would code to a JSON
  key the route no longer sends and to the single-fix framing O95 exists to
  remove.
