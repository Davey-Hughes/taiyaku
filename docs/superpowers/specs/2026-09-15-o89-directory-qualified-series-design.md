# O89 — a directory that qualifies a series, discarded by the rule that reads the filename: build design

**Status: settled in brainstorming on 2026-09-15.**

Written in the style of the rest of `docs/`: every decision records the
alternative it beat and what it cost, a figure asserted here was measured on
this machine rather than recalled, and a guess says it is one.

This closes **O89**. The product design is
[`2026-08-14-library-scanner-design.md`](2026-08-14-library-scanner-design.md);
the curation machinery this hands work back to is
[`2026-08-14-library-curation-design.md`](2026-08-14-library-curation-design.md).

## 0. The gap in one paragraph

Two releases of one volume become two members of one edition, and a reader
opening volume 1 gets the fan translation followed by the official release as
a single page sequence. Measured over a real library on 2026-09-01 — 128
series, 1,915 archives — at 25 works across three series: `[Oshi no Ko]` 12,
`Death Note` 9, `Yu Yu Hakusho` 4. Re-measured against the database on
2026-09-15: **25 editions hold more than one member, and all 25 of them draw
their members from two different parent directories. None draws from one.**

## 0.1 What the row proposed, and why none of it is what this does

O89's row names three candidate fixes — *"a rule that reads the directory, an
edition key derived from it, or only attributing the count so the proposals
inbox can offer the split"* — and calls the choice a spec B decision. All
three are answers to the wrong question, and the reason is in the seeded
rules:

```
1  regex  ^(?<series>[^/]+)/        path  series from the first directory under the root
4  regex  ^(?<series>.+?)[ _-]+v    name  series from the filename where it repeats it
```

**Rule 1 already reads the directory.** For
`[Oshi no Ko] - Fan Translated/[Oshi no Ko] - v01.cbz` it captures
`[Oshi no Ko] - Fan Translated`, which is the distinct series that would have
kept these apart. Rule 4 then matches the filename, captures `[Oshi no Ko]`,
and `Resolved::set` overwrites — *"a later rule overrides only the fields it
actually sets"*, working exactly as designed. The qualifier is discarded by
the next rule to set a series, after it was correctly read.

So O89 is not `classify` merging too eagerly, and not a missing proposal kind.
`classify` merges two entries that genuinely resolved to one `(series,
volume)`, which is its job. **The defect is upstream, in which of two correct
captures wins.**

## 0.2 The two alternatives this beat, recorded because both looked right

**An edition key derived from the directory** (the row's second option) was
the leading candidate until the seeded rules were read. It would have put the
qualifier in `edition_key` rather than in the series, giving two editions of
one work — attractive, because that is the shape taiyaku aligns. It loses on
two counts: it puts logic in `classify` that the rules layer already
expresses, and it leaves `raise_edition_of` blind, because that function
matches *series* titles and would still see one series.

**A work-scoped proposal** (the row's third option) was costed and dropped.
`proposal.subject_id` and `proposal.target_id` both
`REFERENCES series(id) ON DELETE CASCADE` (`003_scanner.sql:98-99`), so a
proposal about *which members of one work should split* has nothing to point
at. It needs a migration widening the table, a new `kind`, a new accept path,
and a new surface in the inbox — all to describe a situation the fix below
prevents arising.

## 1. What changes: the directory wins when it qualifies the filename

A reconciliation step, applied in `rules::Engine::resolve` after the rule loop
and before the root's own defaults:

> When a series set from the path or the parent directory was replaced by one a
> later rule **captured** — and the discarded value **extends** the
> replacement — the discarded value is restored.

`extends` is exact: the directory's name starts with the captured series, the
remainder begins with one of `space`, `-`, `_`, `(` or `[`, and the remainder
contains at least one letter or digit. The separator condition is what stops
`One Piece` being treated as extended by a hypothetical `One Piecey`, since a
prefix that runs straight into a word character is a different name. The
letter-or-digit condition is what stops a trailing space being a qualifier —
§1.3 has the directory in the real library that needed it. Equal strings fail
it too, having no remainder at all, so they need no case of their own.

**Literals win; captures reconcile.** Only a series a regex *capture* extracted
is reconciled. A literal `set_series`, on a rule of any `match_on`, is a
decision and is never overridden. A capture pulls text out of a name and can
drop a qualifier while doing so, which is exactly what seeded rule 4 does. A
literal was written by someone who chose the string. So a person who wants the
short name writes a literal, and that is the whole escape hatch.

**Which rule set the series is already recorded, and whether a capture did is
not.** `Resolved::provenance` maps each `Field` to the rule id that last set
it, and the engine holds every rule's `match_on`. But `apply_set_columns` and
`apply_captures` both call `Resolved::set` with the same rule id, so provenance
cannot tell a literal from a capture on one rule. The loop records that one
extra bit. The test for "this capture carries a series" moves into a helper
that `apply_captures` also calls, so the two cannot disagree about an empty
group.

### 1.1 Why this preserves the case rule 4 exists for

`003_scanner.sql:126-129` states rule 4's purpose: *"a filename does not
always repeat the series and a directory always names it"*, with rule 4
layering the filename over it. The case is a root whose first directory is not
a series — `Downloads/One Piece v01.cbz`, where rule 1 captures `Downloads`.
`Downloads` is not a prefix of `One Piece`, so the reconciliation does not
fire and rule 4 keeps the final word. **The narrowing only acts where the two
captures agree about the series and differ about its qualifier**, which is the
only shape in which discarding the directory loses information.

### 1.2 The prediction this makes, which is how it gets verified

Re-deriving the library at `/home/davey/temp/manga-test` must produce the
figures below. They are the post-repack state (O90's §4 converted twelve
archives on 2026-09-15, so the 2026-09-01 figures no longer apply). They come
from a simulation that applied seeded rules 1 and 4 and the reconciliation to
every member's path. The simulation was checked first: its "before" series
agreed with the database's actual series for **all 1,895 members, with no
disagreements**.

| | before | predicted after |
| --- | --- | --- |
| `merged` | 25 | **0** |
| works | 1870 | **1895** |
| members | 1895 | 1895 |
| distinct series holding a member | 129 | **133** |

Seven directories change the series their members resolve to, and a diff of
member → series must show these and nothing else:

| Directory | Members | Effect |
| --- | --- | --- |
| `[Oshi no Ko] - Fan Translated` | 13 | split from `[Oshi no Ko]` |
| `Death Note (Colored Council)` | 9 | split from `Death Note` |
| `Yu Yu Hakusho - Colored (Colored Council)` | 4 | split from `Yu Yu Hakusho` |
| `Hunter × Hunter - Kurapika's Memories` | 1 | split from `Hunter × Hunter` |
| `One Piece - Definitive Edition (Colored)` | 23 | renamed, nothing was merging |
| `Parasyte - Full Color (Colored Council)` | 10 | renamed, nothing was merging |
| `Shingeki no Kyojin - Kui Naki Sentaku (Complete Color Edition)` | 1 | renamed, nothing was merging |

The renames were accepted rather than narrowed away, on 2026-09-16: one
per-entry rule applied consistently gives each directory its fuller name.
Limiting the reconciliation to cases that would otherwise merge needs to know
about other entries, which `resolve` cannot, since it sees one path. That
would move the logic into `derive` and make it a different design.

`proposal` should gain `edition-of` rows for at least
`[Oshi no Ko] - Fan Translated` and `Yu Yu Hakusho - Colored (Colored
Council)`. `raise_edition_of` matches a `sort_title` that is another series'
title plus a `" - "` suffix, and these two now are. **`Death Note (Colored
Council)` is predicted *not* to be proposed**, because its qualifier is in
parentheses rather than after `" - "`. That limitation belongs to
`raise_edition_of` rather than to this change, and it is filed rather than
fixed.

The printed `series` total is recorded as measured, not predicted. It
includes series that hold no work, and whether a renamed series' old row
survives as one of those is not something this document has checked.

Any figure that comes out otherwise is the measurement, and this section is
wrong. The numbers are written down in advance so that cannot be argued
afterwards.

### 1.3 What the pre-flight scan changed, before any code

This section was first written from the seeded rules and three measured
series. A scan of the plan against the real library, before any task was
dispatched, found four things wrong with it. They are corrected above rather
than silently.

- **The blast radius was eight directories, not three.** The first draft
  predicted only the three merged series would move. A simulation over every
  entry found eight directories whose name extends what rule 4 captures from
  their files.
- **One of the eight was a regression.** The directory
  `Hunter × Hunter (Official Colored) ` has a trailing space, and its files
  capture `Hunter × Hunter (Official Colored)`. The draft's `extends` accepted
  the space as a qualifier, so 37 members would have moved from a clean series
  name to one ending in whitespace. The letter-or-digit condition in §1 closes
  it, and it has a test of its own.
- **The precedence the draft promised could not be built.** §4 said a user's
  `name` rule above 4 would still win. But the draft's mechanism only asked
  whether the final series came from a `name` rule, and seeded rule 4 is one,
  so no user `name` rule could be told apart from it. The draft's guard also
  never changed an outcome. A directory rule's value always equals the current
  series when a directory rule had the last word, so `extends` already fails
  there. The draft plan's test quietly used a `path` rule instead. Replaced on
  2026-09-16 by literals win, captures reconcile.
- **The predicted figures were taken before O90's repack**, which added twelve
  works and twelve members. §1.2 now holds the post-repack figures.

### 1.4 What execution found: this reverses O95, by decision

The pre-flight scan simulated the real library and never the test fixtures, and
that is where the conflict was. Task 1's first run passed its own tests and broke
three in `preview::tests`, two of which pinned **O95** (closed 2026-09-05). O95
recorded that rule 4 folding `Death Note (Colored Council)` into `Death Note` "is
what folds a colour edition into its main series", and it called the editor remedy
that split them "the one thing that broke it". This reconciliation *is* that split.
The two decisions never met. O95 was decided without O89's measurement that the fold
concatenates two releases whenever their volumes collide, 25 times in the real
library.

**Decided 2026-09-16: O89 governs.** A directory that qualifies the filename's
series is its own series. The curation inbox's `edition-of` proposal is how it gets
folded back, and folded *with* an edition key: accepting writes a literal
`set_series`, which §1's literals-win rule never reconciles. The alternative was to
keep O95's fold and derive an edition key from the directory's qualifier. It was
declined because it makes derivation guess that a suffix means "edition", which is
wrong for a side story such as `Hunter × Hunter - Kurapika's Memories`. It would
also have needed a key normalisation and a precedence against a root's configured
`edition_key` that nothing had designed.

What follows from it:

- **The rule editor's Overrides tab stops reporting a qualifying directory.**
  `preview::overrides`' detection is unchanged, but the final series is now rule 1's
  value, so nothing is overridden. Of O95's 178 overridden files in the real library,
  61 are reconciled and 117 still override. Those 117 span 11 (directory, capture) pairs, in 5 directories, whose name
  the filename capture does not extend, such as
  `GTO - Great Teacher Onizuka - Paradise Lost` over `GTO - Paradise Lost`. The
  simulation's 61 + 117 reproduces O95's measured 178 exactly. O95's two override
  tests moved to those GTO shapes, and the colour-edition fixture now pins that it is
  not an override.
- **A digit-only qualifier counts.** A fixture directory `Series 1` qualified
  `Series` (remainder `" 1"`). Accepted rather than excluded: a numbered directory
  over unnumbered files is more often a sequel or part than the same series. The
  fixture became `Omnibus 1`. **The cost, named by Task 1's review:** a range pack
  such as `Naruto 1-72/Naruto - v01.cbz` becomes a series called `Naruto 1-72`.
  Requiring a letter would not reliably prevent that, since `Naruto (Digital) (1-72)`
  and `Naruto v01-72` both carry letters, and it would leave a numbered sequel over
  unnumbered files merging. The real library has neither shape. Task 5 looks for them.
- **The rule preview credits a reconciled entry to rule 1**, so the Matched files and
  Provenance tabs stop listing it under seeded rule 4. Provenance is telling the truth
  about which rule's value stands.
- **`Death Note (Colored Council)` gets no proposal**, because `raise_edition_of`
  matches only a `" - "` suffix. It is correct but unlinked until that function reads
  a parenthesised qualifier. That is a new open item rather than part of this change.

## 2. What this does not change

*(Since §1.4: `preview::overrides`' code is unchanged, but what it reports for a
qualifying directory is — it is no longer an override.)*

`classify` is untouched — it still merges entries that resolve to one
`(series, volume)`, and `DeriveReport::merged` still counts them. A volume
genuinely split into several archives in one directory still merges, still
correctly. No schema change, no migration, no new proposal kind, no change to
`proposals::raise`. Rule 4 is not disabled, reordered or rewritten; what
changes is the reconciliation applied to its result.

## 3. The library fix, which needs no code and is not part of the PR

**Not written.** Implementation followed directly on 2026-09-16, so the three
temporary rules below were never created. The paragraph is kept as the option
that was available.

Three rules with ordinal above 4, one per affected directory, setting
`series` from the parent — or equivalently `edition_key` — make the three
series distinct today, before any of this ships. They are written through
`taiyaku library rules` or `RuleEditor`. They become redundant once §1 lands
and should be removed then, and this sentence is the record that they were
temporary.

**Re-deriving changes `work.content_key` for the 25 affected works**, which is
O81's condition for stranding a hand document. It costs nothing here, measured
rather than assumed: on 2026-09-15 the library at
`/home/davey/temp/manga-test/library.db` held **0 rows in `progress` and 0 in
`note`**. A library with reading history needs O81 considered first. *(Corrected 2026-09-16 by the pre-merge claim check: this conflates two things. O81's rekey cause is handled by `hand::rename_for_rekey`, and no hand document existed under the root. The reason to check `progress` and `note` first is that each work keeps its id while its edition loses a member, so a slot recorded on the concatenated sequence can name another release's page, or none.)*

## 4. Tests

- `qualifies` as a unit: the three merged shapes from the real library, every
  admitted separator, a prefix with no separator (`One Piecey` against
  `One Piece`), a remainder that is only whitespace
  (`Hunter × Hunter (Official Colored) `), equal strings, a string that is not
  a prefix at all (`Downloads` against `One Piece`), and the reverse direction.
- The headline shapes through `resolve` with the seeded four rules:
  `[Oshi no Ko] - Fan Translated/[Oshi no Ko] - v01.cbz` and
  `Death Note (Colored Council)/Death Note - v01 (Colored Council).cbz` keep
  their directory's name, and `[Oshi no Ko]/[Oshi no Ko] - v01.cbz` does not
  change.
- **The case rule 4 exists for.** `Downloads/One Piece - v01.cbz` still
  resolves `One Piece`, and provenance still names rule 4. This test fails if
  the reconciliation is written too wide.
- **The whitespace regression**, through `resolve`:
  `Hunter × Hunter (Official Colored) /Hunter × Hunter (Official Colored) - v01.cbz`
  resolves the name without the trailing space.
- **Provenance**: a restored series is credited to the rule that captured the
  directory (rule 1), not to rule 4 and not to anything the reconciliation
  invents.
- **Literals win, in both directions.** A user rule matching on `name` with a
  literal `set_series` of `[Oshi no Ko]` keeps that value over the directory.
  A user rule matching on `name` that *captures* `[Oshi no Ko]` is reconciled
  like rule 4. Both are pinned, so the decision cannot drift either way
  unnoticed.
- End to end through `derive`, on a fixture holding both releases in two
  directories: `merged` is 0, and two series and two works come out where one
  work of two members did before.

## 5. Risks

- **This changes series resolution for every library, not only for the
  merged case.** The 1,870-work re-derive in §1.2 is the evidence that it does
  not move anything else, and it must be run and diffed — not just totalled —
  before this merges. Totals can cancel: a series gained and a series lost
  reads as no change.
- **`Resolved` is `PartialEq` and `provenance` is part of it.** A
  reconciliation that restores a value must decide whether provenance names
  the rule that originally captured it (correct: it did) or the reconciliation
  (wrong: it is not a rule). Tests assert the former.
- **A test asserting the narrowing fires can pass against its own defect** if
  its fixture would resolve the same series either way. Every case above must
  be checked by hand against a mutation that removes the reconciliation, and
  the `Downloads` case against one that widens it — the two directions fail
  differently and one fixture cannot see both.
