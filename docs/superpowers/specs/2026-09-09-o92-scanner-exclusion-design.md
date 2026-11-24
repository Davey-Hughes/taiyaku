# O92 — the scanner half: `exclude_globs` applied: build design

**Status: settled in brainstorming on 2026-09-09.**

Written in the style of the rest of `docs/`: every decision records the
alternative it beat and what it cost, a figure asserted here was measured on
this machine rather than recalled, and a guess says it is one.

This closes **O92**, the last of the three columns migration 1 shipped against
a feature nobody had built. Its readable half closed on 2026-09-08 in #119 —
`scan::Root` carries the column, `scan::roots` selects it,
`config_api::root_json` sends it and `RootRow.tsx` paints the box from it.
What is left is the half the
item opened with and the only half that was ever a decision: **`scan::scan_root`
never consults the column, so nothing is excluded from a walk**, and C4 §3's
fourth confession — *"directories skipped by `exclude_globs`"* — is
uncoverable.

The product design is
[`2026-08-14-library-configuration-design.md`](2026-08-14-library-configuration-design.md)
(`C4 §2`, `C4 §3`); the walk is
[`2026-08-14-library-scanner-design.md`](2026-08-14-library-scanner-design.md)
(`B §2`, `B §8.1`); the screen is
[`2026-09-04-c4-screens-design.md`](2026-09-04-c4-screens-design.md), whose §1
put this out of scope in as many words — *"Making it work is a scanner change
with its own blast radius"* — and whose §3.4 states what the report must not
hide.

## 0. The gap in one paragraph

`measure_dir` walks every child `read_dir` returns that does not begin with a
dot. `root.exclude_globs` is read into `scan::Root` and consulted by nothing.
`ScanReport` has no field a skipped directory could land in, so the fourth of
C4 §3's four confessions has nowhere to go — the build design's claim that
*"every one of those is already a field"* is true of the other three and false
of this one.

## 0.1 What O92's own entry got wrong, corrected here rather than quietly

**The entry's implied blast radius is not there, and the correction changes
what this work has to guard against.** The reasoning that kept this half
deferred ran: an exclusion makes `forget_unseen` delete `scan_entry` rows,
derive then produces fewer works, and *progress, notes and read history hang
off works* — C4 §2's sentence about removing a root, arriving through a text
box with no confirmation.

Checked against the schema and the code on 2026-09-09, three links of that chain
do not exist:

- **Nothing references `scan_entry`.** `schema/004_auth.sql:112` says it
  outright — *"`member` has no `scan_entry_id`"*. `member` references
  `edition`; `edition` references `work`. Deleting a scan entry cascades into
  nothing.
- **`derive` does not delete a work whose material vanished.** Its only
  `DELETE FROM work` is `fold::absorb`, reached through `superseded`, which
  requires the material to have a home in *another* work written this pass.
  Material that is simply gone is not superseded by anything, so no fold
  happens. The second `DELETE FROM work` in the tree is
  `scan::apply_removal`'s, which belongs to removing a root and is confirmed
  by its own screen.
- **`import::scanned` deletes only the editions the scan named.**
  `import.rs:788`, `Scope::Scan`, with its own comment giving the reason:
  *"The scan never saw this edition" and "a rule edit removed it" are
  different facts and nothing here can tell them apart.*

**So the real consequence is the opposite one, and it is mild.** An exclusion
leaves the works it covers **standing and stale** — still listed, still readable
off disk because `member.path` is still valid, and never refreshed by a scan
again. Clearing the pattern and rescanning restores the facts and re-derives
onto the same rows. No work, edition, member, progress row or note is destroyed
by an exclusion at any point.

That is not nothing: a stale work is O92's own complaint one layer down, an
admin excluding a directory and watching the library go on listing it. It is
answered by the report saying what happened (§4) rather than by a confirmation
gesture, and **the design deliberately does not add a pruning path for stale
works** — see §10.

The passage in `docs/status.md` that reasons from the destructive chain is left
as written and marked, per that document's own rule for a claim a later look
overturns.

## 1. The dependency: `ignore`, not a hand-rolled matcher

`exclude_globs` is a text box an admin types patterns into. Three ways to read
those patterns were weighed.

**Chosen: `ignore` 0.4.33** (`ignore::gitignore::GitignoreBuilder`), the
matcher behind ripgrep. It takes lines exactly as a `.gitignore` file holds
them — `add_line` per line — and answers `Match::None | Ignore | Whitelist`.
Negation, anchoring, directory-only patterns, comments and blank lines all come
with it and none of them is written here.

**Rejected: `globset` alone, plus ~50 lines of ordering.** `globset` has no
negation. Reaching gitignore semantics on top of it means reimplementing
last-match-wins, leading-`/` anchoring, the implicit `**/` on a pattern with no
slash, and trailing-`/` meaning directory-only — each with its own tests. This
codebase already argues that line against itself: `rules.rs` refuses *"a second,
drifting reimplementation as a regex default"* and hands the string to
`taiyaku_read` instead.

**Rejected: reuse `RuleKind::Regex`.** Zero new matching code, and consistent
with the rule editor. But the column, C4 §2's list and the screen's label all
say *globs*, and a regex typed where a glob is advertised is the sort of quiet
mismatch this project keeps writing rules against.

**Measured cost, on this machine on 2026-09-09**, by resolving `ignore` in a
scratch crate and diffing its lock against this workspace's:

| crate | new to this workspace? |
|---|---|
| `ignore` 0.4.33 | **new** |
| `globset` 0.4.19 | **new** |
| `bstr` 1.13.1 | **new** |
| `crossbeam-deque`, `crossbeam-epoch`, `crossbeam-utils` | already here |
| `same-file`, `walkdir`, `winapi-util` | already here |
| `windows-link`, `windows-sys` | already here |
| `aho-corasick`, `log`, `memchr`, `regex-automata`, `regex-syntax` | already here |

**Three crates, not the ten a first look at `cargo add ignore` suggests** —
`notify` already supplies the walker and the crossbeam machinery. `globset`
alone would have cost two of the same three, so the negation and anchoring the
design wanted cost exactly **one crate** over the option that could not provide
them.

**User-supplied patterns are safe here for `regex`'s reason, restated because
it is not obvious it transfers.** `globset` compiles to `regex-automata`, which
does not backtrack; a pattern typed into C4's box cannot produce catastrophic
matching behaviour. `taiyaku-library/Cargo.toml` already carries that paragraph
for `regex` and gains its parallel for `ignore`.

## 2. The pattern language

**One pattern per line**, `\n`-separated, stored in the one `TEXT` column
migration 1 defined. Blank lines and `#` comments are ignored — gitignore's own
rules, not this design's.

**Newline rather than comma or whitespace.** A path may legally contain a comma
or a space and a manga library has plenty of both; it may not contain a
newline. That is the whole argument: the other two separators make some real
pattern unwritable, silently.

Consequence for the screen: `RootRow.tsx`'s single-line `<input>` for this
column becomes a `<textarea>`. The other three text columns keep their inputs.

**Semantics verified against the crate on 2026-09-09**, not recalled — pattern
set `**/extras/**`, `extras`, `*.nfo`, `!keep.nfo`, `/toplevel`, `build/`:

| path | is dir | result |
|---|---|---|
| `Naruto/extras` | yes | excluded |
| `Naruto/extras/art.jpg` | no | excluded |
| `extras` | yes | excluded |
| `a/b/c.nfo` | no | excluded |
| `a/keep.nfo` | no | **whitelisted** — negation works |
| `toplevel` | yes | excluded |
| `deep/toplevel` | yes | not excluded — leading `/` anchors |
| `build` | yes | excluded |
| `build` | no | not excluded — trailing `/` is directory-only |
| `Naruto/v01.cbz` | no | not excluded |

**Read that table as one combined pattern set, not row by row.** Every verdict
is the verdict of all six lines together. `Naruto/extras → excluded` is the
bare `extras` line's doing, not `**/extras/**`'s — and the difference is the
whole feature, not a footnote.

**`**/extras/**` does not exclude the directory. Measured 2026-09-09:**

| pattern | `Naruto/extras` as a dir | `Naruto/extras/art.jpg` |
|---|---|---|
| `**/extras/**` | **not excluded** | excluded |
| `extras/` | excluded | never reached — no descent |
| `extras` | excluded | never reached — no descent |

So a root carrying only `**/extras/**` still descends into every `extras`
directory, still records one as a `scan_entry` row, excludes its files one at
a time, and reports **`excluded_dirs: 0`** — the field C4 §3 asks for and this
branch exists to create. None of §3's descent saving happens.

**This is not hypothetical and it is the shape this project already stored.**
`docs/notes.md` records `**/extras/**` as the value set on the real library on
2026-09-08, and it is the first row of the table above. An admin who copies it
will be told their pattern excluded no directories, which reads as "it did
nothing".

The matcher is faithful gitignore and is not the thing to change. What was
missing is that nothing — not the label, not this design, not the report —
said that **a trailing-slash or bare name excludes the directory and saves the
walk, while a `/**` suffix excludes only the contents.** Found by the
whole-branch review, which asked which of the shapes §2 measured its own
leading example falls into. §7's label should carry the distinction.

## 3. Where the exclusion applies

**In `measure_dir`'s entry loop, immediately after the dotfile check, before
`entry_count += 1` and before the dir / page-image / file split.**

- A **directory** that matches is recorded and **never `read_dir`'d**. Its
  whole subtree costs nothing — no listing, no `stat`, no `sniff`, no
  `member_pages`. Skipping at descent is the only placement that actually saves
  the walk, which is half of what an exclusion is for.
- A **file** that matches is counted and skipped, whether or not
  `taiyaku_read::is_page_name` would have made it a page. A pattern that
  excludes page images changes the directory's page total, which is what
  excluding them means.

**Before `entry_count`, deliberately.** `dir_state.entry_count` exists to say
whether a directory changed. An excluded file appearing or vanishing is not a
change this library has any interest in, and counting it would make one show
up. The cost is that `entry_count` no longer equals what `read_dir` returned —
which is consistent, since it is only ever compared against a number computed
the same way, and a pattern edit *should* read as a change.

*Corrected 2026-09-09 by the whole-branch review: **nothing reads
`dir_state.entry_count` at all.** `grep -rn entry_count` over `crates/` finds
one write — `record`'s `dir_state` upsert — and no read anywhere. The
placement decision stands on its own terms; the justification above describes
a comparison that does not exist, which is the same shape as the column this
whole branch was written about.*

**This matches git, for free.** Git cannot re-include a file whose parent
directory is excluded; not descending produces exactly that behaviour without
the design having to state it as a rule.

**`measure_dir` takes one accumulator, not three out-params.** It already
threads `unreadable: &mut Vec<String>`; exclusion adds two more collections.
The three collapse into a single `&mut Walk { unreadable, excluded_dirs,
excluded_files }`. This is the one refactor the change takes in passing, and it
improves a signature that was already at its limit.

## 4. What the report says

`ScanReport` gains four fields. **All four are drawn** — C4 §3.4's contract is
that every field is rendered including a collection that is empty this time,
and that a `Vec` which is empty renders as `0` rather than as a section that
vanishes.

| field | means |
|---|---|
| `excluded_dirs: Vec<String>` | rel_paths not descended into. **C4 §3's fourth confession**, finally a field. |
| `excluded_files: usize` | files skipped by a pattern, page images included. A count, not a list. |
| `forgotten_by_exclusion: usize` | rows deleted from `scan_entry` because a pattern now covers them. |
| `bad_exclude_patterns: Vec<String>` | lines that would not compile, named. |

**Directories are a list and files are a count**, and the asymmetry is the
point. A library excludes a handful of subtrees and may exclude thousands of
`*.nfo` files; a report that renders whole cannot carry the second as a list.
Naming the directories is what C4 §3 actually asks for.

**`forgotten_by_exclusion` is split out of `gone` rather than folded into it.**
`gone` means the file disappeared from disk. "You excluded it" and "it
disappeared" are different facts, and not conflating those is the whole of this
report's discipline. The rejected alternative — let it count in `gone` and let
the operator work it out from `excluded_dirs` sitting beside it — saves one
field and costs exactly the ambiguity the section exists to prevent.

## 5. `forget_unseen`, and how a stale entry is classified

An excluded entry is not in `seen`, so `forget_unseen` deletes its row. Which
counter it lands in is decided by `Gitignore::matched_path_or_any_parents`,
which answers for a path whose *parent* is the thing the pattern named —
verified on 2026-09-09: with `extras` alone as the pattern,
`matched("Naruto/extras/art.jpg", false)` is `None` while
`matched_path_or_any_parents` is `Ignore`.

That needs `is_dir`, which `held_for` does not currently read. So **`held_for`
selects `kind` too** and its `(i64, i64)` tuple becomes a named
`Held { size, mtime, is_dir }` — the shape `Seen`, `Resolved` and `Measured`
already use in this module. Four call sites.

**One known imprecision, named rather than fixed.**
`matched_path_or_any_parents` tests the path itself before its parents, so a
whitelisted child of an excluded parent (`!Naruto/extras/keep` under an
excluded `extras`) reads as not-excluded there — while the walk, correctly,
never reaches it. It decides only which of two counters a number lands in,
never whether a row is deleted. A comment at the call site, not a workaround.

## 6. A pattern that will not compile

**Refused in both places**, which is what the two places are each good for:

- **`PATCH /api/library/roots/{id}` answers 400**, naming the offending line,
  so an admin hears about it at save rather than after a scan.
- **A stored pattern that will not build disables itself** and is named in
  `bad_exclude_patterns`. `rules.rs`'s rule exactly: *"a malformed pattern
  disables its own rule and nothing else"*, because a scan of a real library
  must not die over one bad line. Migration 1's column predates any validation,
  so a row can already hold anything; treating a build failure as impossible
  would make it a crash instead of a complaint.

**What the 400 actually catches is narrow, and the design says so rather than
implying otherwise.** Measured against `ignore` 0.4.33 on 2026-09-09,
`add_line` rejects two shapes:

| line | result |
|---|---|
| `\` | error — dangling `\` |
| `a{b` | error — unclosed alternate group |
| `a[`, `[a-`, `[[:bogus:]]`, `[]`, `[!]` | **accepted** |
| `**`, `***`, `a**b`, `a/**b`, `{a,b}`, `!` | **accepted** |

So the validation is a guard against two specific typos, not a syntax check.
`RootRow.tsx`'s label must not suggest a pattern is verified to do what its
author meant.

*O523: `add_line` rejects more than these two: an unopened `}` and an
inverted range such as `[z-a]` are refused too. `check_exclude_globs`' doc
comment says it refuses exactly what `ignore` will not compile, and names those
shapes.*

## 7. The surfaces

```
crates/taiyaku-library/Cargo.toml   the `ignore` line and its paragraph
crates/taiyaku-library/src/scan.rs  Excluder, the walk, ScanReport, forget_unseen, held_for
crates/taiyaku-serve/src/config_api.rs  400 on PATCH; four keys in the report JSON
crates/taiyaku-cli/src/library.rs   scan_summary prints each, non-zero only
web/src/config.ts                   four fields on the ScanReport type
web/src/ScanReport.tsx              four rows, among the did-not-do fields that lead
web/src/RootRow.tsx                 <textarea>; the label says what it now does
```

**`Excluder` lives inline in `scan.rs`**, in its own `// --- exclusion ---`
section beside `// --- roots ---` and `// --- the scan ---`. A separate module
was considered and rejected on the file's own convention: its single submodule
is `tests.rs`, as with `derive/` and `fold/`.

**The label becomes `Exclude globs (one gitignore pattern per line)`.** It
loses `(recorded, not yet applied — O92)`, and it does not gain a claim that a
pattern is checked against anything — §6 measured how little the validation
actually catches.

**`config_api.rs`'s report JSON keeps destructuring with no `..`**, which is
what forces the four new fields to be handled rather than silently dropped —
C4 §3.4 names that as the reason the route is written that way.

## 8. Testing

Per this project's rule, **every test named here is checked by hand-applying
the defect it names and watching it fail** before the fix lands. A test that
passes against its own defect has been this branch's most common finding on
every recent PR.

Rust, in `scan/tests.rs` unless noted:

1. A directory matching a pattern is not walked — its children never reach
   `scan_entry`, and its rel_path appears in `excluded_dirs`.
2. A file matching a pattern is skipped and counted in `excluded_files`.
3. A negation re-includes a file its own pattern excluded.
4. A leading `/` anchors: a pattern `/extras` skips the root's own `extras`
   and not `Naruto/extras`.
5. A trailing `/` excludes the directory and not a file of the same name.
6. An entry already held that a new pattern covers is deleted and counted in
   `forgotten_by_exclusion`, **not** in `gone`; a file genuinely removed from
   disk in the same scan is counted in `gone` and not the other. One test, both
   assertions — the pairing is the claim.
7. An excluded file appearing does not move `dir_state.entry_count`.
8. A `\` line is named in `bad_exclude_patterns` and every other line still
   applies.
9. An empty or NULL column excludes nothing — the no-pattern path is the one
   every existing scan takes.
10. `config_api/tests.rs`: `PATCH` with a `\` line answers 400 naming it, and
    the column is unchanged afterwards.
11. `config_api/tests.rs`: the scan report JSON carries all four keys, present
    and not absent, when nothing was excluded.

Web:

12. `ScanReport.test.tsx`: all four fields render when empty — asserted per
    element, never with a whole-page `toContain`, which cannot see a pairing.
13. `RootRow.test.tsx`: the box is a `<textarea>`; a two-line value round-trips
    with its newline intact; the label no longer says "not yet applied".

**After changing what a test stages, ask which guard it used to be the only
witness for.** #119 lost a guard exactly that way.

## 9. Verification against real material

A green suite is not the claim. Against a copy of the real library at
`/home/davey/temp/manga-test` — never the library itself, and the isolation
proved afterwards rather than assumed:

1. Scan clean, record entry count and wall time.
2. Set `exclude_globs` to a pattern covering a real subtree. Scan. Record
   `excluded_dirs`, `excluded_files`, `forgotten_by_exclusion`, the new entry
   count and the wall time — **the walk should get faster, and by how much is
   the figure this work is worth judging on.**
3. Confirm the works under that subtree are still listed and still open — §0.1's
   claim, tested rather than asserted.
4. Clear the column. Scan. Confirm the entry count returns to (1) and no work,
   progress row or note moved.

All four runs go in `docs/notes.md`, which is where measurements live.

## 10. Out of scope

- **Pruning stale works.** An exclusion leaves the works it covers standing
  (§0.1). Deleting them means a new `DELETE FROM work` path in `derive` that
  does not exist today, cascading progress and notes, and it belongs behind a
  screen that states its counts the way root removal already does via
  `scan::removal_cost`. If it is wanted, it is its own item. *It already is
  one, and this paragraph did not know: **O91**, open since 2026-09-03 — "a
  root that shrinks partially leaves its vanished works standing forever, and
  nothing can clean them up". That row reasons from files deleted on disk;
  after this branch the same state is reachable by typing in a text box, 242
  works at a time. O91's entry has been marked; this one is left as written.*
- **Client-side pattern validation.** C4 §8's standing rule: the server owns
  it, and a matcher that runs in JS is not the matcher the store compiles.
- **Excluding by content rather than path.** Nothing has asked for it.
- **A per-root override of the dotfile skip.** Unrelated, and `watch.rs`'s
  probe depends on it.

## 11. What this invalidates

- `docs/status.md`: O92's checkbox, its annotation, its prose bullet, the
  closed/open counts in both sentences, the "What remains" line, and the test
  count at **both** of its sites. The dependency-list claim gains `ignore`.
- `docs/superpowers/specs/2026-09-04-c4-screens-design.md` §1 and §8, which
  record "no `exclude_globs` behaviour" as this screen's deliberate gap.
- `docs/superpowers/specs/2026-09-02-library-configuration-build-design.md`,
  whose *"every one of those is already a field"* becomes true.
- `RootRow.tsx`'s file-level comment, three paragraphs of which describe the
  column as applied nowhere.
