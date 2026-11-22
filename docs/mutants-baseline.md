# Accepted mutation-test survivors

Every mutant listed here is a change to the source that **the whole test suite
still passes against**, and that somebody has looked at and accepted anyway.
`scripts/mutants.sh` diffs a run against this file; anything not listed is the
output worth reading.

The point of writing them down is that a survivor list is only actionable when a
new one is distinguishable from an old one.

**`detect.rs` and `regions.rs` are no longer swept at all**, which is different
from being clean and the distinction must not be lost. Their 45 survivors were
accepted here as deferred until L2 redesigns the detector; since 2026-08-20
`.cargo/mutants.toml` excludes both files, because they are 207 of
`taiyaku-cli`'s mutants — 18% of the longest crate — buying
notice of a regression in code slated for replacement. Their entries are gone
from the list below so the counts mean what they say. Delete the exclusion when
L2 lands and re-run with `--baseline` to rebuild them.

It is also the only memory this project has of what has already been checked.
Four whole-branch reviews of spec A2 returned fifteen findings each and the rate
never fell, because every reviewer re-sampled the whole branch from scratch.
Coverage says which lines *ran*. This says which lines have an assertion behind
them, and which are known not to.

## What belongs here, and what does not

**An accepted survivor needs a reason, and the reason has to be one of these:**

- **An equivalent mutant.** The mutation produces a program that behaves
  identically, so no test could distinguish it. `percentile`'s order-independent
  comparison is the recurring example. These are permanent.
- **Deferred behind named work.** `detect.rs` and `regions.rs` are waiting on
  L2's detector redesign, and writing fixtures against a detector about to be
  replaced is work thrown away. These carry the item they are waiting on.
- **A decision inside output nobody asserts on.** A branch that only changes what
  a command prints for a person to read. Usually the right fix is to extract the
  decision into a function that returns a value — which is what killed 11 mutants
  in `inventory` and 24 in `topology` — so this reason should be rare and should
  say why extraction was not done.

**"No test reaches it" is not a reason.** That is the finding, not an
explanation of it. Neither is "it is only reachable with a real archive": the
sweep that closed five crates found that a *decision inside a print statement* is
the most common shape of untested logic in this codebase, and extracting it is
the fix.

## What this file cannot tell you

**cargo-mutants does not mutate every expression.** It generated no mutant at all
for `run.b.map(|_| run.a)` in a `match` arm, so O38's defect was invisible to it
both before and after the fix. A file with no entries here is not a verified
file; it is a file with no *known* survivors.

**An entry is keyed on `file:line:col: description`, so moving a function
invalidates its entries** and its survivors reappear as new. That is deliberate.
Matching on the description alone would carry an accepted survivor across a
rewrite that turned it into a real defect, with the entry still explaining why
the old code was fine.

The cost is paid in whole files rather than in single entries, and spec C3 is
the first branch to pay it visibly. **Every one of the six `proposals.rs`
entries below has moved**, checked with `cargo mutants --list` against the
branch rather than inferred: `115:26` → `170:26`, `846:20` → `2549:20`,
`867:53` → `2570:53`, `889:31` → `2584:31` and `902:20` → `2597:20` all keep
their description and column and move only in line, while
`784:54: replace && with || in raise_edition_of` becomes
`2428:38: … in EditionOfSignals::volume_signal` — the same
`… == 0 && … == 0` condition, extracted into a method, so its description
changed too. All six would otherwise have been reported as new survivors by any
sweep of this branch, and **that would not have been evidence that anything got
worse**. A module-scoped sweep during the branch reported ten pre-existing
survivors in this file's scoring and `raise` code; six baseline entries having
moved accounts for at most six of them, and which of the remainder are
genuinely new was not settled — the sweep's line numbers were taken against a
mid-branch tree that has since moved again, so they cannot be re-resolved after
the fact.

**The six entries below now carry the new keys**, applied by hand against
`cargo mutants --list` at the branch head rather than left as prose for the
next reader to apply — which is what this paragraph asked for and did not do,
while `derive.rs`'s one moved entry was properly re-keyed in the same commit.
Two `interior_indices` survivors are added beside them, predicted verbatim by
that function's own doc comment and confirmed against `--list`. Without both
edits the next sweep reports eight new survivors and none of them is one. Note
the shape of the trap either way: the entries most likely to go stale are in
the file the branch worked hardest on, which is also the file where a real new
survivor matters most.

**A sweep reporting a mutant as *caught* is not proof that it is.** Measured on
this branch: `scripts/mutants.sh --since a17d442` reported
`curation.rs:125` as caught (`panicked`); applying that same mutation by hand
and running the whole suite serially shows it **survives**. The cause was not
identified — the run's own logs had been overwritten by a later sweep before
the discrepancy was noticed, and the obvious suspect, a timing-sensitive stall
test, was ruled out at 0 failures in 25 runs under load. So one of the two
readings is wrong and it is not established which. Treat a *caught* as evidence
rather than as a result, and confirm by hand before deleting a test on the
strength of one.

## Regenerating

`scripts/mutants.sh --baseline` (with whatever scope) rewrites the entries below
for the files that run examined, carrying existing reasons across and stamping
new entries with a placeholder. The placeholders are yours to replace — the
script exits non-zero while any remain, because an entry with no reason is
indistinguishable from a defect somebody stopped thinking about.

## When it was last run

**There is no schedule.** `.forgejo/workflows/mutants.yml` is dispatched by
hand, because the forge's host is an 8-core machine carrying 68 other
containers at a load average around 16, and a sweep is hours of build-and-test.
A weekly timer would degrade that box for hours every week, unattended, to
re-measure something that moves at the speed of test-writing.

**So this table is the thing that tells you a sweep is overdue.** A manual job
with no record is a job nobody can tell has lapsed. Read this before deciding
whether one is due; a long gap here is the signal a cron would have been
standing in for. Add a row every time.

`scripts/mutants.sh --since main` is the half that runs often and needs no row —
it mutates only changed code and takes minutes.

| Date | Scope | Result |
| --- | --- | --- |
| 2026-08-19 | spec A2's four modules (255 mutants) | 30 survivors; 17 killed by new tests, 13 recorded |
| 2026-08-19 | whole workspace (3,191 mutants, 3 h 12 m) | 150 survivors — the baseline below |
| 2026-08-20 | `taiyaku-library` on the forge, 3 shards | 3 survivors outside the baseline: `mtime_secs`, a catch the workstation had invented. Fixed, O59 opened |
| 2026-08-26 | spec C3's `proposals.rs`, module-scoped (220 mutants) | 168 caught, 41 unviable, 11 missed. Only 1 of the 11 belongs to this branch; the other 10 are pre-existing scoring/`raise` code, and see the note above on why the baseline could not tell moved entries from new ones |
| 2026-08-26 | spec C3, `--since` per task | 0 survivors on the web components (`Proposals.tsx` 21 of 76 before its fix round, 0 of 84 after; `AcceptForm.tsx` 0 of 29). One reported catch on `curation.rs:125` did not reproduce by hand — see "What this file cannot tell you" |
| 2026-08-20 | spec C1's browse API, `--since main` (132 mutants, 4 m) | 14 survivors, all killed rather than recorded. The one that mattered: `refuse_if_hidden` replaceable with `None` — the page-route grant check this branch added, deletable with every test passing. Two lessons kept: the first test written for it passed *with the guard deleted*, because the fixture's files do not exist and `/pages` answers 404 either way, so the assertion is on the refusal's body; and three of the fourteen were a comparison inside a `println!`, which is the third time this file has produced that shape after `sessions_ended_note` |
| 2026-08-20 | wasm, read, core, library, serve (2,049 mutants, 42 m) | re-measured after excluding `detect.rs`/`regions.rs` and capping `--jobs`; **every crate unchanged against the baseline.** `taiyaku-cli` was stopped partway and has not been re-measured since the exclusion — the numbers below for it are from the 2026-08-19 sweep |
| 2026-08-26 | `derive.rs`, file-scoped (133 mutants, 4 m) | 126 caught, 4 unviable, 2 timeouts, 1 missed — and the one missed is the entry already below, moved from `:1262` to `:1451` because `write_edition_metadata` and `in_page_order` were added above it. **0 survivors in the code this branch added**: every mutant of the new writer, of the shared sort, and of the closed-set filter was caught. The two timeouts are `nested_in_claim`'s `!` and `parent_of`'s return value, both of which make the walk non-terminating rather than wrong |
| 2026-08-26 | `proposals.rs`, file-scoped, after the whole-branch fix round (206 mutants, 8 m) | 156 caught, 42 unviable, **8 missed and every one of them already below** — the six re-keyed entries and the two `interior_indices` ones added in the same commit. `mutants-baseline: no survivor outside the baseline`, which is what the re-key was for: the same run against the un-re-keyed file would have reported all eight as new. `EditionOfSignals::vocabulary_phrase` is *not* among them any more — both its `FnValue` mutants are caught by the test added this round |
| 2026-08-21 | `library-browse-series`, `--since main` (23 mutants, 59s) | 0 survivors: 21 caught, 2 unviable (`match_for` and `search` each replaced with `Default::default()`, refused at compile time — `Match` and `SearchResults` derive no `Default`). All 21 catches are semantic, not arity-refused: none of the 23 mutants touch a placeholder count. `match_for_switches_paths_exactly_at_trigram_min` (added this branch) catches all three comparison-operator mutants at `TRIGRAM_MIN`'s use site |
| 2026-08-28 | spec C3, `--since main`, after the fresh-database fix and the seven survivors it also reported (116 mutants, 8 m) | 0 survivors: 101 caught, 15 unviable. All seven from the prior run on this branch — `hand.rs:154` and `:163` (`save`'s empty-document and non-`NotFound` guards), `metadata.rs:175` (`panicked`), `metadata.rs:272` (`worst_case`, both constant variants), `metadata.rs:462` (`series_of_work`), `metadata.rs:711` (`patch_member`'s `retain`, the one with the `PATCH {}`-shaped data-loss risk) — are now caught; none were baselined |

| 2026-08-29 | spec C3, `--since main`, after the whole-branch review's fix waves (123 mutants, 8 m) | **1 reported survivor, and it is a false one — which is the finding.** `librarydoc.rs:639`'s `apply_with_series_id -> Ok(())` was reported MISSED; hand-applying it in place fails **five** `taiyaku-serve` tests. **cargo-mutants' `--test-workspace` defaults to `false`, so it runs only the tests in the *mutated package*.** The timing says so plainly: 26 s of tests at baseline against 7 s for the mutant. `taiyaku-library` is the store and a great deal of its behaviour is asserted through `taiyaku-serve`'s routes, so **every sweep of this repository under-reports cross-crate coverage**, and some of the untriaged survivors below may be false for the same reason. The error runs one way only — a mutation caught solely by another package reads as MISSED, but nothing uncaught ever reads as caught — so a zero-survivor result stays trustworthy and a survivor list does not. Setting `test_workspace = true` in `.cargo/mutants.toml` fixes it and multiplies the per-mutant cost by the workspace's test time, which is the trade this file cannot make on its own: **O87** |
| 2026-08-29 | O85's `browse.rs`, file-scoped, after importing `HashMap` (95 mutants, 6 m) | **The import is the finding.** The same file swept as `--since main` an hour earlier reported 48 mutants, 18 caught, 30 unviable, 0 survivors — and O85's two new functions accounted for **fourteen of the thirty unviable**, every one of their whole-body mutants. `cargo-mutants` writes its replacement with the short type name (`Ok(HashMap::new())`), so in a module that spelled `std::collections::HashMap` out in full **none of them compiled**, and a zero-survivor run said nothing whatever about the code the branch added. One `use` line turned 48 mutants into 95; both new functions are now covered and every one of their mutants is caught, including `delete field missing from struct EditionCard expression in editions_by_work`. The seven new survivors are all `visible_editions` and all false, by O87 — see the entries below. This row also said **37 files in this workspace still spell those paths out in full**, `proposals.rs` among them — *wrong, and corrected by the row below: that counted files mentioning the path rather than functions returning one, and the real figure is two functions, `proposals.rs` holding none of them.* O88 |
| 2026-08-29 | O84's browse and serve changes, `--since main` (27 mutants, 2 m) | 0 survivors: 17 caught, 10 unviable. **The first sweep since O88's `use` line, and it shows the fix working**: all **seven** whole-body mutants of the new `opens_by_work` are viable and every one is caught, where before the import all seven would have been `unviable` and the clean run would have said nothing about the function. `works_of`'s new `==` — the filter deciding whether a stored position's edition is still visible — is caught too. **Of the ten unviable, three are O88's second cause rather than its first**: `browse::home` and `browse::search` have no viable whole-body mutant at all, their only generated one being `Ok(Default::default())` against types deriving no `Default`, and `routes::visible_editions_or_refuse` loses three the original way, `routes.rs` spelling `std::collections::HashSet` out in full. It keeps one viable mutant and that one is caught; `home` and `search` keep none |
| 2026-08-29 | O88's spike: `proposals.rs`, then `opds.rs`+`routes.rs`, file-scoped (206 then 126 mutants, 7 m each) | **The spike corrected O88's own scope figure and then closed half of it.** `proposals.rs` was the worst known offender at 42 unviable and has **zero** from the fully-qualified-path cause, despite writing `std::collections::BTreeSet` sixteen times — every one a local or a parameter, and only a **return type** can produce the uncompilable mutant. Its 42 are 18 `Ok(Default::default())` against types deriving no `Default`, and 24 `cargo-mutants` guessing `Offer::new`/`Offer::from` for a custom type. Measured workspace-wide, exactly **two** functions return a fully-qualified collection — `opds::visible_editions_by_work` and `routes::visible_editions_or_refuse` — against the **41 files** that merely mention the path, which is the figure O88 had quoted as its scope. Two `use` lines: all twelve of their whole-body mutants are now viable and **all twelve caught**, 0 missed, 0 unviable, and no function in the workspace still returns a fully-qualified collection. **No new survivor surfaced**, so this cause cost nothing beyond the two lines |
| 2026-08-30 | O86's change, `--since main` (10 mutants, 2 m), then `metadata.rs` file-scoped before and after (56 mutants, 4 m each) | **The `--since main` sweep was clean and could not have found the thing worth finding.** 10 mutants, 9 caught, 1 unviable, 0 survivors: every `==` and `||` in `Principal::from_account` caught, all four `admin_note` mutants caught, and the one unviable is `Principal -> Default::default()` — O88's second cause, benign because the operator-level mutants inside the function are viable and caught. But O86's rule changed what *other, unchanged* code could be reached, and `--since main` mutates only changed code. **The file-scoped sweep of `metadata.rs` is what found it: `root_grant_clause -> String::new()` MISSED — deleting the entire grant filter passed every test in the workspace.** The I8 proofs that exercised it drove a restricted *administrator*, the account O86 made unrepresentable, so closing O86 took that guard's only coverage with it while touching neither the guard nor any test. All nineteen whole-body mutants of the four resolvers are caught, so they were never unreachable — only a *non-empty* `roots` stopped reaching them. `the_grant_clause_still_filters_even_though_no_route_can_supply_a_grant` asserts the contract at the function level and fails against that mutation; the re-sweep is 53 caught, 3 unviable, **0 survivors**. **The lesson for this file: after a change to a permission or dispatch rule, sweep the code the rule governs, not the code the diff touched.** |
| 2026-08-31 | O80's per-field provenance, `--since main` (27 mutants, 2 m) | **2 survivors, one real and one O87 again.** The real one is the finding: `delete field chosen from struct SeriesInfo expression in of` — the field added on this very branch to carry "a person placed this work on this shelf" was never read back by any test, so deleting it from `of`'s own expression passed the whole suite. A field added to record a fact, and nothing asserted the fact. `chosen_reports_whether_a_person_placed_this_work_on_this_shelf` asserts both directions, since a constant `true` and a constant `false` are each half right. The second is `apply_with_series_id -> Ok(())`, false by O87 and verified by hand for the second time on the same function: 0 failures in `taiyaku-library`, **5** in `taiyaku-serve`, matching the count O87 recorded from #91 — baselined with that reason. Of the 3 unviable, `of -> Ok(Default::default())` and two `held_edition_meta` ones are O88's second cause; `held_edition_meta` keeps a viable mutant and it is caught |

| 2026-09-04 | C4 PR 2 (users and backup), `--since main`, twice (90 then 92 mutants, 7 m each) | **6 survivors on the first run, 0 on the second, and the one worth recording is a test that passed against its own mutation.** The six: `backup.rs`'s `e.kind() == NotFound` guard replaceable with `true` — an unreadable directory would have read as *no backups yet*, the silence this project keeps writing rules against; `backup_api`'s `no_file_to_back_up` and `failed` each replaceable with `Response::default()`, a 200 with an empty body, because nothing asserted either refusal; and three on `users_api::can_log_in`. Every one was killed rather than baselined. **The `can_log_in` test is the finding**: written first against `GET /api/library/users`, it passed with the comparison mutated to `!=`, because the list route reads the flag straight off `account::list`'s tuple and `users_api::can_log_in` is reached only by `patch_user`'s response. Hand-applying the mutation is what caught that, which is [[whole-branch-review-finds-what-task-review-cannot]]'s rule about readings restated for tests: a test that runs the mutated line's *file* is not a test that runs the line. All four fixes were confirmed the same way before the re-sweep, and only then did the re-sweep agree. **Of the 18 unviable, every one is O88's second cause** — `Ok(Default::default())` against `Account`, `Removal` and `Backup`, none of which derive `Default`, plus `guarded_routes`'s `MethodRouter` guesses — with two exceptions worth naming: `replace && with || in update` is unviable twice because the `&&` sits in a **let-chain**, which the mutant cannot compile. That is not a coverage report, and checking it by hand found a real hole: nothing pinned `role != current.role`, the condition that stops the self-demotion refusal firing on a form that PATCHes an unchanged role back on every save. `an_administrator_may_edit_their_own_row_while_resending_their_role` closes it. **The unviable count is not noise to skim** — O88 says it can hide untested functions, and here it hid an untested *condition*. **A third run, after the error-typing fix round, found one more: `user_panicked -> Default::default()`.** It was caught in the second run and became a survivor in the third, because that round changed the shape of the code around it — a reminder that a clean sweep is a fact about one commit and not about a branch. Tested the way its three siblings already are (`curation::panicked`, `metadata::panicked`, `config_api::root_panicked` each have a direct function-level test), not baselined: the pattern was already there and this module had simply not followed it. **A fourth run, after the credential gate, is 99 mutants / 80 caught / 19 unviable / 0 survivors** — and the four new routes' gate is caught, which matters because a gate is the one thing a sweep can prove is load-bearing and a reading cannot. Final: 99 / 80 / 19 / 0. **Across four runs this branch's sweeps found seven real gaps and zero false positives**, which is the opposite of the ratio [[O87]] leads a reader to expect — every survivor here was in code the branch itself added, where cross-crate under-reporting cannot apply. |

| 2026-09-04 | O93's credential gate, `--since main` (34 mutants, 4 m) | **34 caught, 0 missed, 0 unviable** — the first sweep in this log with nothing in any of the three columns but *caught*, and the shape is why: a gate is the one thing a mutation sweep proves better than any reading. Every mutant of `administering_or_forbidden` — the role comparison, the `Role::Admin` constant, the whole-body replacement — is killed by `every_administrative_route_refuses_an_api_key`, which drives all thirty-one administrative routes with a real key. **Zero unviable is itself worth noting** against the 19 the branch before it carried: those were all `Ok(Default::default())` against types deriving no `Default`, and this change adds no function returning such a type. The lesson from #106's run three still holds — this is a fact about this commit — but a gate is the case where a clean sweep says the most. |

| 2026-09-05 | O95's override reframing, `--since main` (9 mutants, 2 m) | **0 survivors — 7 caught, 2 unviable — and the run is worth a row because one of the two unviable was hiding a real defect.** `field_value` (the renamed `snapshot`) returns a `FieldValue`, which derives no `Default`, so its only generated mutant is the whole-body one and it is unviable: **O88's second cause, reported as `unviable` rather than as a gap**. Hand-applying `Field::Volume => FieldValue::Number(resolved.chapter)` failed **nothing at all** across the whole workspace — the two numeric arms were exchangeable and every test still passed. That is not cosmetic: `overrides` compares two snapshots to decide whether two rules genuinely disagree, so a `Volume` chain compared on `chapter` values drops real disagreements, and since O95 the row draws the value to a person as well. `the_numeric_fields_report_their_own_values_and_not_each_others` pins both arms and fails against either swap, verified by hand in both directions. The other unviable is `overrides -> Ok(vec![Default::default()])`, benign — its `Ok(vec![])` sibling is viable and caught. **This branch also re-keyed the one accepted `preview.rs` entry below**: `690:34 … in conflicts` is now `939:34 … in overrides`, the same equivalent mutant under the renamed function, re-keyed rather than left to surface as new in the next workspace sweep. **The lesson is [[O88]]'s restated with a case**: the unviable count is not noise to skim, and the second time this file has recorded a hand-check of one finding a hole a clean run could not report |
| 2026-09-07 | C3 §6 PR A (the ComicInfo write-back), `--since main`, three times (60 then 55 then 55 mutants, 5 m / 4 m / 4 m) | **4 survivors, then 1, then 0 — and all four of the first run were real.** `skipped_formats -> vec!["xyzzy".into()]` survived because the only assertion on that function was `is_array()`: the screen's whole reason for the field is being able to say "4 archives, and the rar left alone", and a shape assertion cannot see whether it can. **One was an equivalent mutant and the right answer was to delete code, not write a test.** `resolve`'s `path.is_relative()` guard survived replacement with both `true` and `false`; `Path::join` already discards the base when the joined path is absolute, so the guarded and unguarded branches computed the same value for every input and the guard was stating a distinction `join` had already made. A test pinning an equivalent mutant proves nothing, so the guard went. **The survivor after the fix round is the one worth carrying forward**: `member_paths -> Ok(vec![Default::default()])` passed `assert_eq!(paths.len(), 1)` — the count was right and the single path in it was empty, which is the whole-page `toContain` mistake in Rust. The serve-side test that *would* have caught it never ran: **a sweep tests only the mutated package** ([[O87]]), and that package is `taiyaku-library`. Asserting the paths themselves closes it. **The 24 unviable were checked rather than skimmed**, per [[O88]]: every one is `Default::default()` against a type deriving no `Default`, four of them in this branch's own code (`Planned`, `Applied`, `WireArchive`), and each would be *caught* if it were viable — `plan`'s tests assert a path and `apply`'s assert a digest, so neither would survive an empty default. Constant at 24 across all three runs, which is what makes that check cheap here |
| 2026-09-13 | O104 (sync before the rename), `--since main` (8 mutants, 2 m) | **8 caught, 0 missed, 0 unviable — and not one of the eight is a sync.** cargo-mutants mutates bodies and operators, so it generated nothing for `file.sync_all()` in `atomic::write`, the `.and_then(|out| out.sync_all())` after `rewrite_one`'s `finish`, or either directory-sync call: the fix itself is invisible to it, the blind spot #119's row names. What it did mutate was all caught — `sync_directory_of -> Ok(())`, its empty-parent guard three ways, both writers' whole bodies, and `perform`'s, reached only through a comment edit. The syncs are held instead by fifteen hand-applied mutations recorded under O104 in `docs/notes.md`, and the first version of those tests survived one of them |
| 2026-10-05 | live-works, `--since main` (88 mutants, 9 m) | **65 caught, 22 unviable, 1 missed, 0 timeouts.** The one missed is `Works::is_live -> true`, equivalent and recorded below; hand-applied, it failed nothing in any of the three crates. `Works::report -> ()`, which the plan expected to survive, is caught: `a_refresh_that_failed_is_followed_by_a_read_that_clears_its_complaint` reads `last_notice`, which only `report` sets. **The 22 unviable were read, per [[O88]]**: each is `Default::default()` against a type deriving no `Default`, or a `JoinHandle` constructor that does not exist. Twelve functions have no viable mutant at all: `ServeState::find`, `ServeState::find_blocking`, `Works::empty`, `Works::fixed`, `Works::library`, `Works::refresh_if_moved`, `Works::refresh`, `spawn_refresher`, `read_one`, `routes::work`, `download::prepare` and `taiyaku-cli`'s `serve_works`. Three were checked by hand here. `find`'s refresh on a miss, taken out, failed `a_volume_derived_after_startup_opens_on_the_first_request_for_it`, and `find_blocking`'s failed `a_route_that_looks_its_work_up_on_the_blocking_pool_finds_it_too`. `read_one` on `IMMEDIATE` failed the ordering test ("database is locked"), but **`read_one` with no transaction at all failed nothing**: `a_work_is_read_from_one_committed_state_of_the_library` was added, and fails against it. **Migration 14's SQL is a string literal, which cargo-mutants never mutates**, so the three trigger defects its first task applied by hand are that file's only mutation evidence |
| 2026-10-06 | libraries, `--since main` (249 mutants, 20 m) | **199 caught, 50 unviable, 0 missed, 0 timeouts.** Each unviable one replaces a function's whole body with a value that does not compile there. In the lines this branch changed, that leaves `user::grant_target`, `account::update`, `account::count_for`, `scan::add_root`, `scan::add_root_in`, `scan::add_root_checked`, and `proposals::acceptable_proposal`, `offered_rule`, `preview`, `evidence` and `series_reached_by` with no viable mutant (O88). **cargo-mutants never mutates a string literal, and much of the branch's scoping is SQL inside one**: migration 15, and the scoping clauses of `find_by_key`, `reconcile_series`, `held_work`, `find_series_by_title`, `roots_for`, `removal_refusal` and `raise_same_series`, and the name lookup in `libraries::id_of`. What those statements say was tested by the defects applied by hand in the branch's tasks and reviews |
| 2026-10-06 | libraries F2a, `--since main`, twice (147 then 149 mutants, 16 m each) | **2 missed and 1 timeout on the first run, none of either on the second: 122 caught, 27 unviable.** The two missed: `libraries::rename`'s guard on the UNIQUE code, replaceable with `true`, so that any failure of the update would have read as a taken name; and `libraries_api::library_panicked`, replaceable with `Default::default()`, a 200 with an empty body, because nothing read the answer a libraries route gives when its blocking work panics. Each was killed with a test and neither is baselined. **The timeout was a test, not a mutant that loops**: `serve_folders -> Ok(vec![])` let `serve_accepts_folders_and_refuses_one_with_no_directory` go on to serve, in-process and with no bound, until cargo-mutants' 239 s ran out. The test waits on a thread with a bound now, and the mutant fails it. **The 27 unviable were read, per O88**, and are the same 27 in both runs. Twenty-four replace a function's whole body with a value that does not compile there: `account::update`, `scan::add_root_in`, `add_root_checked_in`, `probed` and `create_library`, `config_api::roots_with_last_scan`, `folders_api::Prefixes::read` and `list`, `libraries_api::panicked_answer`, `added_error`, `folders_of`, `scan_folders` and `spawn_library_scan` (four), and `routes::guarded_routes` (eight). Three replace `&&` with `\|\|` inside a `let` chain, in `serve_folders`, `delete_library` and `spawn_library_scan`. **cargo-mutants makes no mutant for a string literal or a `json!` key**, which is every sentence and every wire shape these routes answer, and the SQL of `library_removal_refusal`; what those say was tested by the defects each task's review applied by hand |
| 2026-10-07 | F2a follow-ups, `--since main` (71 mutants, 9 m) | **54 caught, 17 unviable, 0 missed, 0 timeouts.** Sixteen of the unviable replace a function's whole body with a value that does not compile there: `account::ungrantable` and `account::update`, `scan::add_root_in`, `scan::add_root_checked_in`, `scan::create_library`, `Excluder::build`, `scan::scan_root`, `scan::scan_root_inner`, `scan::held_for` (two), `config_api::a_scan_reserved_on`, `folders_api::list` and `libraries_api::spawn_library_scan` (four). The seventeenth is `&&` to `\|\|` in `config_api::finish_reservation`, whose `&&` joins a `let` chain, where `\|\|` does not compile; the task reviews took the guard out by hand instead, and `a_folders_scan_lands_only_on_its_own_reservation` failed. No mutant is made of a string literal or of SQL. |
| 2026-10-08 | F2b (Move), `--since main` (225 mutants, 30 m), then `--file` `relocate.rs` and `fold.rs` (292 mutants, 33 m) | **The first run: 183 caught, 29 unviable, 13 missed, 0 timeouts; the second: 276 caught, 10 unviable, 6 missed, 0 timeouts.** The 13 were all in `relocate.rs` and `fold.rs`, and each was killed before the second run, by a test or by deleting the code it lived in (an unused `Display` impl and three sums nothing read). The second run's six are in lines of `fold.rs` this branch did not change: `fold::pair`'s body four ways and its `<=`, and a comparison in `absorb`'s sentence of conflicting rows; they are O211 and are not baselined. Every unviable mutant but two replaces a function's whole body with a value that does not compile there; the two are `&&` as `\|\|` inside a `let` chain, in `scan_jobs::finish_reservation` in the first run and `fold::unplaceable` in the second. A third run, over `relocate.rs` alone after its last change (cde926ee, 92 mutants, 12 m): 87 caught, 5 unviable, none missed. No mutant is made of SQL in a string literal, which is most of what Move's refusals, re-filing and split say; the defects put back by hand in the branch's tasks and reviews are their evidence |
| 2026-10-12 | O57, O60 and O138, `--since main` (53 mutants, 17 m), then `--file` `credential.rs` at `2db48eef`, the last commit to change that file (31 mutants, 4 m) | **`--since main`: 39 caught, 8 unviable, 2 missed, 4 timeouts. `credential.rs`: 26 caught, 4 unviable, 1 missed.** The one missed in both runs is `credential::hasher` as `Default::default()`, an equivalent whose entry was already here under the line the function used to stand on. The first run's other was `<` as `<=` where `WorkingMemory::blocks_for` decides whether to grow, which changes nothing the server does, since it is a growth by zero. The second run caught it: the fix round between the two gave the test build a count of a memory's growths, and that count reads a growth by zero as a growth. Three of the first run's timeouts are in `limit.rs`'s semaphore, where an acquire that waits while a permit is free, a permit that is never given back, or a count that never moves leaves a test waiting. The fourth is `taiyaku-cli`'s `serve_auth` replaced by a default configuration. Put back by hand under nextest it fails tests at once, and `tests::run_dispatches_to_serve` does not return, which is what the sweep waited out its 450 s bound for. Seven of the first run's unviable replace a function's whole body with a value that does not compile there: `auth::from_basic`, `routes::sidecar_as_seen`, `VerifyLimit::permit`, `Semaphore::acquire`, `WorkingMemory::blocks_for`, `credential::hash_in` and `KeptMemories::lend`. The eighth is `&&` as `\|\|` inside a `let` chain in `credential::verified_in`. The second run's four are the four of those that are in `credential.rs`. No mutant moves a statement, so none is made for where `visible_editions_or_refuse` is asked in the six handlers or where a permit is taken and given back, which is most of what the branch's tests are about; the defects put back by hand in its tasks and reviews are their evidence |
| 2026-10-17 | O220, `--since main` at `6e83021d` (257 mutants, 26 m) | **183 caught, 72 unviable, 2 missed, 0 timeouts.** Both missed were put back by hand and failed nothing in `taiyaku-serve` or `taiyaku-cli` either (O87). One was real: with `grant_library`'s `+` as `*`, granting a second library to an account already limited returns false, and the CLI prints `that account could already see …; nothing to do` over the grant it has just written. `dc480579` kills it with `granting_another_library_to_a_limited_account_is_new_and_repeating_it_is_not`, and `library_user_list_shows_what_each_account_is_granted` now reads every grant's line; both fail with it put back. The other, `Access::grant_clause`'s guard `roots.is_empty()` as `false`, is equivalent, because every closure handed to `grant_clause` writes a positive `IN (…)` and SQLite reads an empty `IN ()` as false; it is recorded below. Thirty-six of the unviable are `user::grant_target`'s, whose tuple holds a `Library`, which derives no `Default`. The rest replace a function's whole body with a value that does not compile there, but for one `&&` as `\|\|` inside a `let` chain in `browse::cover_work`. That leaves `auth::access_of` and `opds::granted`, which return the new `Access`, `browse::status`, rewritten to take one, and `grant_target` with no viable mutant, so the sweep says nothing about them: `Access` derives no `Default`, and cargo-mutants does not try `Access::Every` as a body. No mutant is made of migration 017 or of the SQL that reads and writes `every_library` |
| 2026-10-21 | dir-rename: `serve_options`, which holds the `TAIYAKU_FOLDERS` refusal; `library_line`, the one function the branch gave a new mutant; `visible_editions`; and `read_line` as a control, function-scoped (16 mutants, 5 m; 3 of them field deletions, which `--re` does not filter, all caught) | **14 caught, 2 missed, and the 2 are the control: the `read_line` entries below.** cargo-mutants makes no mutant of the refusal's `if`, so it was put wrong by hand five ways: removed, inverted, excused when empty, excused by `TAIYAKU_DIRS` beside it and excused by `--dirs`. Each failed `serve_options_refuses_the_old_dirs_variable_naming_both`, and the removal, the inversion and the `--dirs` excuse also failed `tests/cli.rs`'s `serve_refuses_the_old_dirs_variable_before_it_listens`. **`visible_editions`' seven entries are dropped**: O220's `browse::tests::only_with_no_dir_sees_nothing` kills all seven inside `taiyaku-library`. All but one of the other entries in the files this branch edits were keyed to lines that had moved before it; each is re-keyed by its description and by the source line at the commit that wrote the key, and its reason re-read |
| 2026-10-23 | config-grouping (PR 2), `--since main`, twice (48 mutants, 9 m each) | **The first run: 39 caught, 3 unviable, 6 missed; the second: 45 caught, 3 unviable, 0 missed.** All six were in `account.rs`. Five of them, `check_name` as `Ok(())`, its guard as `false` and as `true`, its `!=` as `==`, and `is_unique_violation` as `true`, were killed only by `taiyaku-cli` and `taiyaku-serve` tests, which cargo-mutants does not run for a `taiyaku-library` mutant (O87). `account/tests.rs` now kills each of them, `is_unique_violation` with a planted trigger fault. The sixth, `unclaimed` as `Ok(1)`, was real: under it, `user promote` on a claimed library asked for a password before refusing, and its CLI test accepted any error naming "password". It is now killed by a library test and by that CLI test, which runs with a password source that panics if asked. None is baselined. The three unviable each replace a function's whole body with `Ok(Default::default())` for a type with no `Default`: `account::update`, `scan::add_root_checked_in` and `scan::create_library` |
| 2026-11-17 | the paused-directory freeze (O320, O270), `--since main` at `3af6a1d4` (56 mutants, 23 m) | **48 caught, 8 unviable, 0 missed, 0 timeouts.** The eight unviable each replace a function's whole body with a value that does not compile there: `derive_watched`'s four (`Outcome` has no `new`, `from` or `from_iter` to build one), `entry_of`, `entries` and `frozen_entries` with a `Default` entry (`Entry` derives no `Default`), and `place` with a one-entry map (`GroupKey` and `Group` derive none). `entries` and `frozen_entries` as `Ok(vec![])` are viable and caught; `derive_watched`'s only viable mutant is the `!` before `fresh.contains`, caught. cargo-mutants makes no mutant of the SQL text or field initialisers the branch changed, so thirteen were put wrong by hand. Nine failed a named test: the walk's four `held_*` assignments removed, `copy_entry` without the held columns, the stamp finding its row by digest, `frozen_entries` without the held-reading arm or without the containment step, `reads_as_one_volume` without `e.held_sha256`, `CopyAt::frozen` as `false` (killed by `a_copy_at_another_path_has_its_own_rows_held_reading_dropped_or_kept` alone), and the server's and the CLI's fresh ids emptied. Three passed every test in the three crates: `drop_held`'s and `digest`'s held-reading clauses with `root_id = ?1` made vacuous, and the stamp's held clause testing the row's `sha256 IS NULL` in place of the member's digest. `0ef8a329` kills all three: `a_paused_copy_at_the_same_path_keeps_its_held_reading_when_the_others_goes` fails one case under each `root_id` mutant, and `a_stamp_moves_a_held_reading_only_where_it_is_the_digest_replaced` swaps its two rows' digests so the row's own decides nothing. The thirteenth, migration 27's `coalesce(m.format, …)` as the entry's format alone, is equivalent: nothing writes `member.format` (`stamp::plan`'s and `proposals::edition_formats`' notes), so no test is written for it. None is baselined |
| 2026-11-17 | content matching PR 2, fingerprints in the server, `--since main` (405 mutants, 3 h) | **317 caught, 37 unviable, 47 missed, 4 timeouts.** Thirty-four of the unviable replace a function's whole body with a value that does not compile there (no `Default` for `Unit`, `Job`, `PagePrint`, `BuildLabel` and the rest; no `MutexGuard::new` or `JoinHandle::new`), and three put an or in a `let` chain. The four timeouts are `flat_border`'s scan counters made to stand still (`+=` as `*=` from zero, `-=` as `/=`), which never end. Of the 47 missed, 28 lay in `taiyaku-serve`'s `fingerprint`, the code moved from `taiyaku-cli`, whose tests stayed in the CLI and are not run for a `taiyaku-serve` mutant (O87): the CLI's small-page test, now in the server too, kills 21 of them, and new tests kill the large-page bound's four and `crop`'s two. New tests kill 12 more, in the pass, its job, the store and the progress words. Three are equivalent and recorded below: `flat_border`'s guard against an empty crop, the entry the first workspace-wide sweep left UNTRIAGED at `taiyaku-cli`'s `fingerprint.rs:167:22`, and two in `walk`. Five are killed outside a sweep and survive in one by its setup, and are not recorded below: `about::this_build` as `None`, since the sweep builds a copy with no `.git` and so names no commit; and four in `fingerprint_pass::priority`, since `mutants.sh` runs at niceness 19 in the idle I/O class, where the priority tests print NOT MEASURED |
| 2026-11-17 | RAR PR 1 (`rar-libarchive`), `--since main`, twice (185 then 123 mutants) | **The first run: 18 missed; the second: 6, each accepted below.** Eleven of the first run's, and both its timeouts, were in `pages/rar_tests.rs`: test code, mutated because its module is declared under `all(test, rar)`, which cargo-mutants does not read as `#[cfg(test)]`; `.cargo/mutants.toml` now excludes it beside `test_support.rs`. One was real: `vint`'s `<<` as `>>`, which only an end block whose size or type takes two bytes shows, and no fixture's does; `a_rar5_end_block_is_known_by_its_type_and_crc` now kills it. The four in `pages.rs` sit under `#[cfg(not(rar))]`, which the sweep's every-feature build does not compile, so each entry names the test that fails against it in the default build; one of the four, the guard made `true`, needed a new test. `Libarchive::name` is a label only tests read |
| 2026-11-18 | RAR PR 1 after its rebase onto main, `--since main` (135 mutants) | **111 caught, 18 unviable, 6 missed, 0 timeouts.** The six are the entries accepted on 2026-11-17, four of them under keys the rebase moved. `rar_unread_entries` and `member_unread_entries`, which the rebase reached, have 14 mutants and all are caught |
| 2026-11-18 | RAR PR 1's checksum check, `--since main` (315 mutants) | **263 caught, 26 unviable, 26 missed, 0 timeouts.** Six of the missed are the accepted entries under moved keys. The other twenty are in the new `rar/libarchive/headers.rs`: fifteen real, each then killed by a test, and five equivalent, one of which went with the constant it mutated |
| 2026-11-18 | RAR PR 1 after the fixes to its header walk, `--since main` (316 mutants) | **280 caught, 26 unviable, 10 missed, 0 timeouts.** Six of the ten are the accepted entries, re-keyed below. The other four are the equivalents recorded under `rar/libarchive/headers.rs`, `\|` as `^` on values that share no bit; each was also applied by hand and every `taiyaku-read` test passed |
| 2026-11-17 | JPEG XL pages, `--since main` on the branch as its review fixes left it (202 mutants, 82 m), then `--file` `jxl.rs` twice after the kills (92 mutants, 38 m and 31 m) | **The sweep: 131 caught, 62 unviable, 8 missed, 1 timeout. The last re-run of `jxl.rs`: 42 caught, 50 unviable, 0 missed, 0 timeouts.** All eight missed were in code the branch wrote, and each is killed by a test the branch added for it, put back by hand and watched failing: `METADATA_CAP`'s two `*` as `+` (a cap of about 1 MiB or 17 KiB) fail `a_page_whose_metadata_is_well_under_the_cap_decodes` and `…_is_rebuilt` (two 4 MiB boxes); `metadata_fits`' `>` as `>=` fails `metadata_that_inflates_to_exactly_the_cap_is_accepted`; its `-=` as `+=` fails `two_boxes_each_under_the_cap_are_refused_together_over_it` and as `/=` the two 4 MiB tests; `boxes`' `<` as `==` and as `<=` end the walk at an empty box and fail `an_empty_box_ahead_of_a_bomb_does_not_hide_it`, the input on which they and the original differ, a box whose size is its header's; and `onto_white`'s rounding `+ 127` as `* 127`, which turns a part-transparent pixel white while leaving alpha 0 and 255 alone, fails `a_part_transparent_pixel_is_blended_onto_white`. None is baselined. **The first re-run reported both `METADATA_CAP` mutants as unviable, not caught**: the two-box test had checked its fixture against the cap in a `const` block, so a changed cap stopped the tests compiling, and cargo-mutants counts a build that fails as a mutant that cannot be built. The test now checks it at run time, and the second re-run catches all four of the cap's operator mutants. `cover.rs` was not re-swept: the script scopes by file, not by function, and of `cover.rs`'s 93 mutants 14 are `onto_white`'s; its survivor was put back by hand instead. **The timeout is a hang the tests detected**: `scan_root_inner -> Ok(None)` leaves a scan with no outcome, and a test waiting on it ran into the 1,204 s bound. **The 62 unviable were read against the branch.** Fifty are `jxl.rs`'s: `image::ImageResult` is an alias, and for it cargo-mutants writes `ImageResult::new()` and `::from(…)`, never `Ok(…)`; `ColorType` and `ImageError` derive no `Default`; and `boxes`' `iter::once` bodies do not compile. That left seven functions with no viable mutant, and `routes::page_variant` an eighth (`PageVariant` derives no `Default`), so their whole-body mutants were put in by hand, with `whole_first_frame`'s: `color_type` as `L8` failed 24 tests, `set_limits` as `Ok(())` 2, `read_image` and `read_image_boxed` as `Ok(())` 10 each, `out_of_memory_as_limits` returning its error 1, `whole_first_frame` as `Ok(())` 3, and `page_variant` as `Stored` 10 and as `JxlAsJpeg` 8. `icc_profile` and `exif_metadata` as `Ok(None)` failed nothing, and are equivalent as the code stands: nothing in the workspace, and nothing in `image` 0.25.9's decode, reads either. The other twelve replace a whole body with a value that does not compile there (`as_variant`'s `ServedPage`, `scan_root_inner`'s `Outcome`, `due`'s `Due`, `held_for`'s `Held`, `report::prepare`'s `PageHash`), and each of those functions has viable mutants that were caught. cargo-mutants makes no mutant of a string literal or SQL, so five were put wrong by hand: `"image/jxl"` in `names_jpeg_xl` failed 8 tests, among them `page_variant_reads_whether_accept_names_jpeg_xl`; `jxl_variant_at`'s `-j` failed 4, among them `the_jpeg_cache_name_is_the_page_tag`; `reader_signature` without its `zip:` failed `the_reader_signature_names_every_page_extension_for_zip`; `copy_entry` without `counted_by` in either column list failed `removing_a_nested_directory_hands_its_files_to_the_directory_around_it`; and the scan's `INSERT` without `counted_by`, its `?9` still bound, failed 3, among them `a_new_or_changed_archive_records_this_readers_signature` |
| 2026-11-19 | the colour profile cap (O350), `--since main`, twice: on the branch as its two code commits left it, with the cap at 1 MiB, then after its review fixes and its first rebase onto main (`f5fa08df`), started 11:13 UTC (26 mutants, 17 m each) | **26 caught, 0 unviable, 0 missed, 0 timeouts, both times.** The branch's later rebase onto main changed no code the sweep mutated. Each mutant of the new code, `ICC_CAP`, `icc_fits`, `codestream` and the check added at each call site, was caught both times by a colour-profile test the branch added; in the second run, `ICC_CAP`'s `*` as `+` was also caught by the test of the worst profile the cap passes, at its fixture check. cargo-mutants makes no mutant of `icc_fits` refusing a size it cannot read or a header that does not parse, nor of `codestream` keeping a `jxlp` payload's index or ending after the first box; each was put in by hand and watched failing (`docs/notes.md`, "JPEG XL pages: a cap on the embedded colour profile (O350, 2026-11-19)") |
| 2026-11-19 | RAR PR 2 (libraries read RAR), `--since main`, twice, before the whole-branch review's fixes (232 mutants each, 46 m and 44 m) | **The first run: 198 caught, 26 unviable, 7 missed, 1 timeout; the second: 203 caught, 26 unviable, 2 missed, 1 timeout.** Five of the first run's seven were real, and each is killed by a test watched failing against it put back by hand: `visit`'s `==` as `!=` drops a target's kept bytes at the first of two references to it, which no fixture had, so `rar5-two-references.rar` is one now; `pages_of`'s reflowable-EPUB guard as `true` gives a fixed-layout EPUB a count and a title; and in a passthrough's name and type, `sent_as`'s guard as `true` renames a `.zip` to `.cbz`, `type_for` without its `cbr`/`rar` arm sends a `.cbr` of unknown bytes as `application/octet-stream`, and `renamed`'s guard as `false` turns `.CBZ` into `.cbz`. The other two are equivalent `\|`-as-`^` entries below that this branch moved: they are re-keyed, as is their packed-size twin, which no hunk of the branch reaches (its key from `cargo mutants --list`), and the entry for `SPLIT \| ENCRYPTED` is dropped with the expression. The timeout is `scan_root_inner -> Ok(None)`, a scan with no outcome that a test waits on. Every unviable mutant replaces a whole body with a value that does not compile there, which left seven functions with no viable mutant: `member_page`, `rar_member_page`, `libarchive::open` and `listed`, `Walk::listed` and `bytes_of`, and `download::prepare`. A mutant put in by hand failed tests in each. **The two `Libarchive::name` entries are dropped**: `pages::tests::the_reader_signature_names_libarchives_build_time_version_for_rar` kills both (`.superpowers/sdd/rar-pr2-evidence/CC2-fix-name-mutants.log`) |
| 2026-11-19 | content matching PR 3, volume joins, `--since main` (495 mutants, "tested in 4h" as cargo-mutants reports it), with `RUST_TEST_THREADS=8` | **404 caught, 63 unviable, 27 missed, 1 timeout.** The run is at eight test threads because the unmutated baseline failed at the default thirty-two, twice: three `tests::serve_*` tests, which give a server about two seconds to answer, fail under `cargo test` at niceness 19 with thirty-two threads in one process, on `main`'s tree as on this branch's, and on this branch pass at eight; and `match_pass`'s `the_pass_derive_runs_at_the_servers_priority` could not tell a lowered thread from the main one in a process already at niceness 19, and now says NOT MEASURED there, as the fingerprint pass's priority tests do. Of the 27 missed, 21 are killed by tests added after the run, each put back by hand and watched failing: a place lost to the host counted on a move, a hand alignment whose runs carry no mark, a solver alignment `align --write` stored replaced by the pair's, a run backwards on either side refused, what a Split sets apart (five whole-body mutants of `splits_for`, whose tests were all in `taiyaku-serve`, O87), a group's mark when the work applied another pair first, a stray work's moves and refusals (four), an alignment between two editions neither of which is the hub, the match job's opening count, a kick during a pass, a lookup that panics (two in `Ending`'s drop), and the pass's debug line (two). Four are equivalent, one is not killed and one is a line on stderr, each recorded below with why. **The timeout is a hang**: `reach`'s `==` as `!=` never ends. **The 63 unviable were read against the branch.** Fifty-nine replace a whole body with a value that does not compile there (no `Default` for `Job`, `Resolved` and the rest; `MutexGuard::new`; `HashMap::from_iter`), three put an or in a `let` chain (`Engine::resolve`, `match_pass::started`, `Ending`'s drop) and one deletes `!` from a `&bool` in `move_left_behind`. That leaves nine functions with no viable mutant. Three were put wrong by hand, four ways, and failed tests: `NewPair::turned` as no turn, `OrdF64::cmp` as `Equal`, and `match_pass::started` without its job and without its libraries. The sweep says nothing about the other five, `derive::document`, `Resolved::empty`, `Engine::resolve`, `curation::deriving` and `match_pass::locked`, nor about `volume_match::slots`, which the O340 reproduction holds to every one of 12,246 scores. No mutant is made of SQL, a string literal or a field initialiser |
| 2026-11-19 | content matching PR 3's fix wave, `--since 0c0fcc56` (55 mutants, 28 m), with `RUST_TEST_THREADS=8` | **48 caught, 7 unviable, 0 missed, 0 timeouts.** Six of the unviable replace a whole body with a value that does not compile there: `derive_watched`'s four (`Outcome` has no `new`, `from` or `from_iter`), `Placed::of` as `Ok(Default::default())` (`Placed` derives no `Default`) and `joins::apply` with a one-entry map; the seventh deletes `!` from a `&bool` in `held_work`. cargo-mutants makes no mutant of the SQL or the web code the wave changed, so those, and the two unviable bodies, were put wrong by hand, each failing a named test: `held_work`'s skip made vacuous (`an_english_volume_arriving_where_a_raw_followed_takes_a_work_of_its_own`, and the join generator in 16 of its 164 seeds), the skip taken for a row a hand series or volume placed (`a_work_placed_at_an_address_takes_the_volume_that_appears_there`, `a_rekey_at_an_address_another_work_holds_is_counted_on_every_pass`), a held group following regardless of where its work stands or never (`a_hand_value_on_a_followers_work_keeps_it_where_it_followed`, `a_held_work_does_not_follow`), `matches::reach` skipping paused directories (`a_pause_keeps_every_join_and_follower`), `held`'s `any` as `all` (`one_held_file_holds_its_whole_group`), Undo restoring the named pair alone, no Split group recorded, a kept-apart pair's Split read as its own id (the curation tests of a triangle's Undo, and `split_on_a_triangle_splits_every_pair_tying_the_file_to_its_work`), `split_together` answering the pair alone (`a_splits_pairs_are_found_together_and_apart_from_another_splits`), and in `Series.tsx` the kept-apart lines drawn per pair, unmerged by volume, or landing on the named pair's Split… alone (`Series.test.tsx`, and `Series.panel.browser.test.tsx`'s grouped Undo). None is baselined |
| 2026-11-19 | content matching PR 3's fix for a volume arriving where a work was placed by hand, `--since dde930ad`, twice: after the fix (23 mutants, 13 m), then after a test for the one mutant it missed (23 mutants, 16 m), with `RUST_TEST_THREADS=8` | **The first run: 22 caught, 1 missed, 0 unviable, 0 timeouts; the second: 23 caught, 0 missed.** Thirteen replace the whole of `held_work`, whose volume branch the fix rewrote as a `match` on `Placed::claims`, and ten are on the new lines: three match guards each as `true` and as `false`, two `==` as `!=`, and `holds_a_file_of` as `Ok(true)` and as `Ok(false)`; all compile. The one missed was the fix wave's guard `derived` as `false`: that arm was held only through followers, where the claiming group stands in another series and holds none of the arriving group's files, so the hand arms below it gave the same answer. `a_volume_renumbered_by_its_name_keeps_its_work_from_the_volume_taking_its_number` holds a derived row claimed by a group in the same series, fails with the guard as `false`, and passes with the fix wave's `derive.rs`, so it pins what the fix wave already did. cargo-mutants makes no mutant that deletes an arm of that `match`, so two were put wrong by hand: the series arm deleted fails `a_rekey_at_an_address_another_work_holds_is_counted_on_every_pass`, and the whole new skip removed fails the tests of a follower given a hand volume or a hand series and both tests of a volume appearing where a work was placed from another series. None is baselined |
| 2026-11-20 | O414's fix, `crates/taiyaku-core/src/route.rs`, file-scoped (21 mutants, "tested in 10s" as cargo-mutants reports it), with `RUST_TEST_THREADS=8` | **12 caught, 7 unviable, 2 missed, 0 timeouts.** The two missed are this file's two untriaged entries below, so the script printed `no survivor outside the baseline` and exited 0: a sweep that reaches a verdict reads as it did before the fix. The same sweep with its unmutated baseline made to fail now exits 4 (`docs/notes.md`, "O414: a sweep with no verdict exits non-zero (2026-11-20)") |
| 2026-11-21 | O69, `member.format` dropped, `--since main` (13 mutants, 11 m), with `RUST_TEST_THREADS=8` | **11 caught, 2 unviable, 0 missed, 0 timeouts.** Eight test threads for the reason the 2026-11-19 content-matching row gives: at the default thirty-two the unmutated baseline failed twice on the same three `tests::serve_*` socket tests, which fail the same way under plain `cargo test` on `main`'s tree (`4b7a4f4f`). The two unviable replace a whole body with a value that does not compile there: `unchanged` with `Ok(Some(Default::default()))` (`Unchanged` derives no `Default`) and `held_members_of` with `Ok(vec![Default::default()])` (derive's `MemberRow` derives none). The eleven caught are in the functions whose lines the branch changed: four in `unchanged`'s condition (`\|\|` to `&&` and `!=` to `==` on its page-count and `root_id` lines) and its `Ok(None)`, `held_members_of`'s `Ok(vec![])`, `members_of`'s two, `move_members`, `take_over` and `curation::accept`. None is baselined |
| 2026-11-20 | O396, the fingerprint pass hashing a volume a second time, `--since main`, twice (5 mutants) | **The first run tested nothing**: its unmutated baseline failed `lease::tests::a_waiting_lease_returns_only_once_the_holder_drops_and_says_it_waits`, a timing test this branch does not touch (O429, since closed: `notes.md`, "O429: the waiting-lease test, starved (2026-11-21)"), and the script still ended "no survivor outside the baseline" with exit 0 (O414). **The second, started once the load had fallen below 30, passed its baseline: 5 caught, 0 unviable, 0 missed, 0 timeouts.** The five are `recorded_digest`'s three whole-body replacements, `Reading::one` as `()`, and the digest's field deleted from the `Unit` that `one` goes on with. No mutant is made of SQL, so `recorded_digest`'s four conditions were broken by hand (`notes.md`, "O396: a digest taken behind the listing (2026-11-20)") |
| 2026-11-21 | O399, the serve tests' first answer, `--file crates/taiyaku-serve/src/web.rs` (6 mutants, 10 m), with no `RUST_TEST_THREADS` | **6 caught, 0 unviable, 0 missed, 0 timeouts, and the unmutated baseline passed at the default thirty-two test threads.** `--since main` made no mutant, the branch changing only `taiyaku-serve`'s `tests.rs`, so it ran no baseline. `web.rs` was swept instead: its baseline is `cargo test` over `taiyaku-serve`, the configuration O399's three tests failed in, and `serve_serves_the_web_app_it_is_given` drives its code. That test failed under two of the six, `is_api` as `true` (a 404 for a deep link) and `Web::answer` as `Default::default()` (an empty body), and none of the four tests that wait for a first answer failed under any other (`notes.md`, "O399: the serve tests' first answer in one crowded process (2026-11-21)"). None is baselined |
| 2026-11-21 | O429, the waiting-lease test, `--file crates/taiyaku-library/src/lease.rs` (33 mutants, 26 m), with no `RUST_TEST_THREADS` | **14 caught, 4 unviable, 14 missed, 1 timeout, and the unmutated baseline passed at the default thirty-two test threads.** `--since main` makes no mutant of a change to test code alone, so `lease.rs` was swept instead: its baseline is `cargo test` over `taiyaku-library` at niceness 19, the configuration O429's test failed in, and it passed, 1767 passed and 6 ignored. The waiting-lease test failed under five mutants, each one that breaks the file lease: `take_file` as `Ok(None)`, its busy guard as `false`, either `\|` of its open flags as `&`, and `lock_path` as `Default::default()`. Three of the unviable replace a whole body with a value that does not compile there (`holding`, `take_flag` and `take_file`), and the fourth makes `take_flag`'s `+=` a `*=`. **The 14 missed are not triaged by this run, and none is baselined by it; the next row triages them**: the two `Debug` impls' bodies; six in `take_flag`, the in-memory library's lease (its two guards forced, both `>=` as `<`, and `+=` as `-=`); `\|` as `^` between open flags in `take_file` (two) and `held_elsewhere`; and `is_busy` forced in `take_file`'s guard to `true` and in `initialise`'s to `true` and to `false`. The timeout is `take_flag`'s `waited >= TRY` as `false`, under which a `Wait::No` never gives up. The run's tree changed no line of `lease.rs`; O436's fix, in the next row, does (`notes.md`, "O429: the waiting-lease test, starved (2026-11-21)") |
| 2026-11-21 | O436 and the triage of O429's 14 missed, `--file crates/taiyaku-library/src/lease.rs` (34 mutants, 30 m), with no `RUST_TEST_THREADS` | **24 caught, 4 unviable, 4 missed, 2 timeouts, the unmutated baseline passed, and no survivor outside the baseline: exit 0.** The 34th mutant is O436's check, `181:26`, its `!=` as `==`, and it was caught. The four missed are entries below. A fifth entry, `142:13`'s `\|` as `^`, was reported caught, which is false, the mutant being equivalent: the test that failed under it was `schema::tests::opens_arriving_while_another_builds_a_fresh_database_all_succeed_and_take_no_backup`, three of its four opens answered "database is locked", while the loads read from eight minutes into the sweep to its end ran from 84 to 134, from other work on the machine; that failure is filed as O437 (since closed: `notes.md`, "O437: the fresh-build race test, starved (2026-11-21)"). Its entry stays, though the script asked for a `--baseline` run to drop it. `FlagGuard`'s drop as `()` timed out where O429's run caught it: under it the new in-memory waiting-lease test waited for good on a flag that never cleared. That test now fails itself instead, and the file was swept again (the next row) |
| 2026-11-21 | the same, with the in-memory waiting-lease test's guard (34 mutants, 29 m), with no `RUST_TEST_THREADS` | **24 caught, 4 unviable, 5 missed, 1 timeout, the unmutated baseline passed at the default thirty-two test threads, and no survivor outside the baseline: exit 0.** Against O429's run: one mutant more, O436's `181:26`, caught; nine of its 14 missed now caught by five tests in `lease/tests.rs`, and its other five the entries below (`notes.md`, "lease.rs's survivors triaged (2026-11-21)"); the same four unviable and the same timeout, `take_flag`'s `waited >= TRY` as `false`, under which a `Wait::No` never gives up. `FlagGuard`'s drop as `()` was caught. The load read just before the sweep started was 24, and those read from seven minutes in to just after its end ran from 104 to 130, from other builds on the machine |

<!-- entries -->

### `crates/taiyaku-cli/src/duplicates.rs`

- `crates/taiyaku-cli/src/duplicates.rs:441:53: replace > with >= in contradictions`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-cli/src/duplicates.rs:443:33: replace + with * in contradictions`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.

### `crates/taiyaku-cli/src/library.rs`

- `crates/taiyaku-cli/src/library.rs:1335:20: delete ! in export_targets`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-cli/src/library.rs:264:5: replace resolve_export_user -> Result<i64, String> with Ok(1)`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.

### `crates/taiyaku-cli/src/main.rs`

- `crates/taiyaku-cli/src/main.rs:1656:8: delete ! in serve_work`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-cli/src/main.rs:1972:5: replace slot_index -> u32 with 0`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-cli/src/main.rs:1972:5: replace slot_index -> u32 with 1`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-cli/src/main.rs:1983:5: replace print_alignment_detail with ()`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-cli/src/main.rs:717:37: replace / with * in verify`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-cli/src/main.rs:717:37: replace / with % in verify`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-cli/src/main.rs:779:5: replace page_count -> u32 with 0`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-cli/src/main.rs:779:5: replace page_count -> u32 with 1`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.

### `crates/taiyaku-cli/src/sample.rs`

- `crates/taiyaku-cli/src/sample.rs:57:30: replace < with <= in dev_pages`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.

### `crates/taiyaku-cli/src/score.rs`

- `crates/taiyaku-cli/src/score.rs:120:14: replace > with < in classify_single_cover`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-cli/src/score.rs:131:29: replace < with <= in classify_single_cover`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-cli/src/score.rs:151:12: replace * with + in absorbs`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-cli/src/score.rs:152:20: replace > with >= in absorbs`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-cli/src/score.rs:153:20: replace > with >= in absorbs`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-cli/src/score.rs:156:13: replace && with || in absorbs`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-cli/src/score.rs:261:46: replace += with -= in Report::add_page`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-cli/src/score.rs:261:46: replace += with *= in Report::add_page`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-cli/src/score.rs:304:41: replace == with != in Report::render`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-cli/src/score.rs:312:40: replace / with * in Report::render`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-cli/src/score.rs:312:40: replace / with % in Report::render`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-cli/src/score.rs:506:34: replace == with != in command`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-cli/src/score.rs:673:33: replace += with -= in sample_worklist`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-cli/src/score.rs:673:33: replace += with *= in sample_worklist`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-cli/src/score.rs:691:27: replace += with -= in sample_worklist`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-cli/src/score.rs:691:27: replace += with *= in sample_worklist`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-cli/src/score.rs:77:30: replace - with + in intersection`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-cli/src/score.rs:77:9: replace - with + in intersection`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.

### `crates/taiyaku-cli/src/user.rs`

- `crates/taiyaku-cli/src/user.rs:127:5: replace read_password -> Result<String, String> with Ok("xyzzy".into())`
  The process boundary. `read_password` is a terminal prompt, two blocking reads
  from stdin and an `stty` call; nothing under `cargo test` has a tty. **The
  testable half is already extracted** — `accept(first, &second)` holds the rule
  about which inputs are refused and in what words, and it is tested; the
  commands take an injected `AskForPassword` rather than calling this at all.
  What survives here is the reading itself.
- `crates/taiyaku-cli/src/user.rs:127:5: replace read_password -> Result<String, String> with Ok(String::new())`
  The process boundary. `read_password` is a terminal prompt, two blocking reads
  from stdin and an `stty` call; nothing under `cargo test` has a tty. **The
  testable half is already extracted** — `accept(first, &second)` holds the rule
  about which inputs are refused and in what words, and it is tested; the
  commands take an injected `AskForPassword` rather than calling this at all.
  What survives here is the reading itself.
- `crates/taiyaku-cli/src/user.rs:130:8: delete ! in read_password`
  The `if !quiet` that warns when echo could not be turned off — a decision about
  what a terminal shows, reachable only with a tty. It sits above the extracted
  `accept` for the same reason the rest of this function does.
- `crates/taiyaku-cli/src/user.rs:170:5: replace read_line -> Result<String, String> with Ok("xyzzy".into())`
  `read_line` is `std::io::stdin().read_line`. There is nothing here but the read.
- `crates/taiyaku-cli/src/user.rs:170:5: replace read_line -> Result<String, String> with Ok(String::new())`
  `read_line` is `std::io::stdin().read_line`. There is nothing here but the read.
- `crates/taiyaku-cli/src/user.rs:180:5: replace set_echo -> bool with false`
  `set_echo` shells out to `stty` against an inherited stdin. Under a test harness
  stdin is not a tty, so it answers false — which would kill `-> true` and not
  `-> false`, and only while the harness captures stdin. A test whose result
  depends on whether the runner has a terminal is worse than this entry.
- `crates/taiyaku-cli/src/user.rs:180:5: replace set_echo -> bool with true`
  `set_echo` shells out to `stty` against an inherited stdin. Under a test harness
  stdin is not a tty, so it answers false — which would kill `-> true` and not
  `-> false`, and only while the harness captures stdin. A test whose result
  depends on whether the runner has a terminal is worse than this entry.

### `crates/taiyaku-core/src/route.rs`

- `crates/taiyaku-core/src/route.rs:129:61: replace - with / in route`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-core/src/route.rs:129:61: replace - with + in route`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.

### `crates/taiyaku-core/src/sidecar.rs`

- `crates/taiyaku-core/src/sidecar.rs:1311:46: replace || with && in Sidecar::trust`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.

### `crates/taiyaku-core/src/solve.rs`

- `crates/taiyaku-core/src/solve.rs:135:18: replace > with >= in calibrate`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-core/src/solve.rs:138:25: replace > with >= in calibrate`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.

### `crates/taiyaku-core/src/truth.rs`

- `crates/taiyaku-core/src/truth.rs:195:17: replace + with - in off_the_page`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-core/src/truth.rs:195:17: replace + with * in off_the_page`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-core/src/truth.rs:195:28: replace > with == in off_the_page`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-core/src/truth.rs:195:28: replace > with >= in off_the_page`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-core/src/truth.rs:196:40: replace + with - in off_the_page`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-core/src/truth.rs:196:40: replace + with * in off_the_page`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-core/src/truth.rs:198:28: replace > with >= in off_the_page`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-core/src/truth.rs:199:40: replace + with - in off_the_page`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-core/src/truth.rs:199:40: replace + with * in off_the_page`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.

### `crates/taiyaku-library/src/account.rs`

- `crates/taiyaku-library/src/account.rs:153:36: replace match guard roots.is_empty() with false in Access::grant_clause`
  **An equivalent mutant, because SQLite reads an empty `IN ()` as false.**
  With the guard false, `Only` with no directory falls through to `in_dirs`
  over no holes, and every `in_dirs` closure handed to `grant_clause`
  writes a positive `IN (…)`, so the clause becomes `AND … IN ()`. SQLite
  answers that false, which is what the guard's ` AND 0` answers, and neither
  form binds a parameter, so no statement's count moves either.
  Hand-applied on the O220 branch, it failed nothing in `taiyaku-library`,
  `taiyaku-serve` or `taiyaku-cli`.

  **The guard stays anyway.** An empty `IN ()` is SQLite's extension, not SQL,
  and ` AND 0` keeps "granted nothing matches nothing" true without leaning on
  it. It stops being equivalent the day a closure writes `NOT IN (…)`, which
  an empty list answers true: then the guard is the only thing standing
  between an account granted nothing and every library.

### `crates/taiyaku-library/src/credential.rs`

- `crates/taiyaku-library/src/credential.rs:102:5: replace hasher -> argon2::Argon2<'static> with Default::default()`
  **An equivalent mutant, verified rather than assumed.**
  `argon2::Params::DEFAULT_M_COST`, `DEFAULT_T_COST` and `DEFAULT_P_COST` are
  19456, 2 and 1 — printed from the crate, not recalled — which is exactly what
  `M_COST_KIB`, `T_COST` and `P_COST` hold, because both are OWASP's current
  recommendation. `Argon2::default()` therefore produces the same hasher.

  **It stops being equivalent the day these parameters are raised**, which §14's
  pending measurement on the NAS may well do.
  `credential::tests::the_stored_hash_carries_this_modules_parameters` is what
  fails on that day: it reads the parameters back out of the PHC string, so a
  hasher built from anything but this module's constants is caught the moment
  they differ from the crate's defaults.

### `crates/taiyaku-library/src/derive/joins.rs`

- `crates/taiyaku-library/src/derive/joins.rs:337:51: replace + with * in Linking::link`
  Not killed, and not equivalent. It adds each side to the sides it is
  checked against, which differs only where a side's own files are split from
  each other: both files of a split pair in one edition, which only names can
  do (a rule re-keying them after the Split), and where the pair already
  shares a work. There the mutant refuses the whole edge, so it changes which
  edges apply in a store a rule can make; no fixture builds one.
- `crates/taiyaku-library/src/derive/joins.rs:346:68: replace != with == in Linking::link`
  Equivalent. The names-tie union is built from parts of a losing side and
  from components holding editions left behind, and neither ever holds a held
  group: a losing side with one is refused first, a held group never moves,
  and an edition left behind leaves `left` once a pair takes it into another
  component. So `held.contains(group)` is false at that check whichever way
  the host comparison goes, and the check stands as a guard.

### `crates/taiyaku-library/src/import.rs`

- `crates/taiyaku-library/src/import.rs:445:36: replace && with || in drop_stale_spoke`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-library/src/import.rs:445:72: replace && with || in drop_stale_spoke`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-library/src/import.rs:449:74: replace && with || in drop_stale_spoke`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.

### `crates/taiyaku-library/src/lease.rs`

- `crates/taiyaku-library/src/lease.rs:142:13: replace | with ^ in take_file`
  **Equivalent: the three open flags share no bit.**
  `OpenFlags::SQLITE_OPEN_READ_WRITE`, `SQLITE_OPEN_CREATE` and
  `SQLITE_OPEN_NO_MUTEX` are 0x2, 0x4 and 0x8000 (`SQLITE_OPEN_READWRITE`,
  `SQLITE_OPEN_CREATE` and `SQLITE_OPEN_NOMUTEX` in the `sqlite3.h` of the
  SQLite 3.54.1 that `libsqlite3-sys` 0.40.0 bundles), and bitflags 2.13.1
  makes `|` the or of the bits and `^` their exclusive or, which agree where
  no bit is on both sides. `^` in place of the first `|` groups as
  `(READ_WRITE ^ CREATE) | NO_MUTEX` and gives 0x8006, as the line does. It
  stops being equivalent only if a flag sharing a bit with another joins them.
- `crates/taiyaku-library/src/lease.rs:143:13: replace | with ^ in take_file`
  Equivalent, for the reason `142:13` gives: `^` in place of the second `|`
  binds tighter, `READ_WRITE | (CREATE ^ NO_MUTEX)`, and gives 0x8006 too.
- `crates/taiyaku-library/src/lease.rs:245:43: replace | with ^ in held_elsewhere`
  Equivalent, as `take_file`'s two: `SQLITE_OPEN_READ_WRITE` (0x2) and
  `SQLITE_OPEN_NO_MUTEX` (0x8000) share no bit, so `^` gives 0x8002 as `|`
  does.
- `crates/taiyaku-library/src/lease.rs:33:9: replace <impl std::fmt::Debug for Wait<'_>>::fmt -> std::fmt::Result with Ok(Default::default())`
  **Output only a person reads, and nothing in the workspace prints it.** The
  impl is there because the workspace lints `missing_debug_implementations`
  (`Cargo.toml`). With it and `DeriveLease`'s both deleted,
  `cargo check --workspace --all-targets --all-features` checked every target
  and warned only that the two types lack `Debug`, so no derive, `{:?}`,
  `dbg!` or `unwrap_err` anywhere formats either; checked on the branch that
  closed O436. The mutant prints an empty string where `No` or `Yes` was.
- `crates/taiyaku-library/src/lease.rs:64:9: replace <impl std::fmt::Debug for DeriveLease<'_>>::fmt -> std::fmt::Result with Ok(Default::default())`
  Output only a person reads, and nothing in the workspace prints it, by the
  check `33:9` describes: the mutant prints an empty string where
  `DeriveLease { .. }` was.

### `crates/taiyaku-library/src/lib.rs`

- `crates/taiyaku-library/src/lib.rs:219:5: replace is_busy -> bool with true`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-library/src/lib.rs:255:56: replace < with <= in set_wal_within`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.

### `crates/taiyaku-library/src/librarydoc.rs`

- `crates/taiyaku-library/src/librarydoc.rs:1028:5: replace find_series_by_title -> Result<Option<i64>, String> with Ok(None)`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-library/src/librarydoc.rs:730:5: replace apply_with_series_id -> Result<(), String> with Ok(())`
  DEFERRED behind **O87**, and verified false rather than assumed so — for the
  second time, on the same function. Every suite in `taiyaku-library` passes
  with the body replaced; `taiyaku-serve` fails **five** tests. That is the
  exact case and the exact count O87's own entry records from #91, reproduced
  here by making the substitution and running both packages. `patch_series` is
  the only caller, so every assertion on this function lives across the crate
  boundary `--test-workspace = false` does not cross. It stops being reported
  the day O87 is settled.

### `crates/taiyaku-library/src/matches.rs`

- `crates/taiyaku-library/src/matches.rs:267:34: replace > with >= in store`
  Equivalent. It differs only for a pair whose two digests are equal, which is
  never scored: the lookup leaves the query out of its own candidates, and
  copies of a file share one digest and one unit.

### `crates/taiyaku-library/src/migrate.rs`

- `crates/taiyaku-library/src/migrate.rs:221:9: replace SiblingMigration::is_empty -> bool with true`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-library/src/migrate.rs:222:13: replace && with || in SiblingMigration::is_empty`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-library/src/migrate.rs:223:13: replace && with || in SiblingMigration::is_empty`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-library/src/migrate.rs:224:13: replace && with || in SiblingMigration::is_empty`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-library/src/migrate.rs:380:14: replace > with < in held`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.

### `crates/taiyaku-library/src/preview.rs`

- `crates/taiyaku-library/src/preview.rs:938:34: replace - with / in overrides`
  **An equivalent mutant, verified rather than assumed.** The expression is
  `i64::MAX - (proposed.len() as i64) + index as i64`, computed once per
  unsaved (`id == 0`) rule in `proposed`, `index` its own position. The
  relabelled id it produces is never read back by anything outside this
  function — `overrides` uses it only as the second half of the sort key
  `(relabelled[i].ordinal, relabelled[i].id)` that decides resolution order,
  never as a value compared against a real row id or against
  `crate::rules::ROOT_FALLBACK` the way `preview::rows_for`'s own `decided`
  does. So the only two properties that matter are: does it stay injective
  across `index`, and does it stay clear of every id a real row could hold.

  Mutated to `i64::MAX / (proposed.len() as i64) + index as i64`, both still
  hold. Only the first operator changed — the trailing `+ index as i64` is
  untouched — so it is still strictly increasing in `index` by exactly `1`
  per step, the same spacing the correct formula gives, just built on a
  different, smaller huge constant — so it stays injective and preserves
  the exact same relative order between any two rules, including two
  sharing one ordinal; nothing about which rule resolves before another,
  and so which one wins a tie, changes. Colliding with a real row's small
  id, or with `ROOT_FALLBACK` (`0`), would need `i64::MAX / proposed.len()`
  itself to be small — on the order of a few thousand — which needs
  `proposed.len()` on the order of `i64::MAX / 1000`, roughly 9.2×10¹⁵. A
  `Vec<Rule>` that long cannot be built by any real caller: each `Rule` is
  well over one hundred bytes, so the vector alone would need several
  hundred petabytes, long before this crate's own `taiyaku-serve` bound of
  200 (`MAX_PROPOSED_RULES`) is ever reached by the one HTTP route that
  calls this function at all.

  **The two mutants this expression's other operator produces are not
  equivalent, and are not filed here** — both are killed by
  `preview::tests::two_same_ordinal_unsaved_rules_break_their_tie_in_proposed_order`,
  which gives two unsaved rules the same ordinal and checks that the one
  listed *later* in `proposed` wins their tie, the same rule a real save
  would mint the higher id for. `replace + with -` gives
  `i64::MAX - len - index`, decreasing as `index` increases instead of
  increasing. `replace + with *` gives `i64::MAX - len * index` — `*` binds
  tighter than the leading `-`, so this is `i64::MAX - (len * index)`, not
  `(i64::MAX - len) * index`, verified against `cargo mutants --diff` rather
  than assumed from the source text alone — which is *also* decreasing as
  `index` increases, just at a different rate. Both silently reverse which
  of two same-ordinal unsaved rules resolves last and so wins their tie;
  neither is a change in scale alone the way `- with /` is.

  It stops being equivalent the day anything downstream starts reading the
  relabelled id itself rather than only its ordering, or the day a caller
  can reach `overrides` with a `proposed` list long enough to shrink
  `i64::MAX / proposed.len()` into the range a real row id occupies.

### `crates/taiyaku-library/src/proposals.rs`

- `crates/taiyaku-library/src/proposals.rs:174:26: replace * with /`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-library/src/proposals.rs:2446:40: replace - with / in interior_indices`
  Equivalent: the bound becomes `page_count / 1`, which is `page_count`.
  **The upper half of the `clamp` never fires**, so raising its bound cannot
  change a returned value: `page_count * 3 / 5 <= page_count - 1` holds for
  every `page_count >= 2`, and the function returns empty below two.
  `interior_indices`' own doc comment says the upper half never fires and why
  the clamp stays anyway.

  **Kept in the code rather than deleted, which is why these two are accepted
  rather than killed.** The clamp states an invariant of this function — an
  interior index is never 0 and never past the last page — not a property of
  whichever fractions somebody later picks. A test could only kill them by
  asserting on fractions that do not exist, and a fix that deleted the bound
  would leave the next fraction free to run off the end.
- `crates/taiyaku-library/src/proposals.rs:2446:40: replace - with + in interior_indices`
  Equivalent: the bound becomes `page_count + 1`.
  **The upper half of the `clamp` never fires**, so raising its bound cannot
  change a returned value: `page_count * 3 / 5 <= page_count - 1` holds for
  every `page_count >= 2`, and the function returns empty below two.
  `interior_indices`' own doc comment says the upper half never fires and why
  the clamp stays anyway.

  **Kept in the code rather than deleted, which is why these two are accepted
  rather than killed.** The clamp states an invariant of this function — an
  interior index is never 0 and never past the last page — not a property of
  whichever fractions somebody later picks. A test could only kill them by
  asserting on fractions that do not exist, and a fix that deleted the bound
  would leave the next fraction free to run off the end.
- `crates/taiyaku-library/src/proposals.rs:2701:38: replace && with || in EditionOfSignals::volume_signal`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-library/src/proposals.rs:2830:20: replace += with *= in raise_same_series`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-library/src/proposals.rs:2864:67: replace += with *= in raise_attach_chapters`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-library/src/proposals.rs:2878:31: replace == with != in raise_attach_chapters`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-library/src/proposals.rs:2891:20: replace += with *= in raise_attach_chapters`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.

### `crates/taiyaku-library/src/scan.rs`

- `crates/taiyaku-library/src/scan.rs:1440:17: replace > with >= in scan_root_inner`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-library/src/scan.rs:1667:28: replace += with *= in record`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-library/src/scan.rs:1772:23: replace match guard format == taiyaku_read::Format::Zip.token() with true in pages_of`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-library/src/scan.rs:2009:14: replace > with == in measure_dir`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-library/src/scan.rs:2009:14: replace > with >= in measure_dir`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-library/src/scan.rs:2063:21: replace += with -= in measure_dir`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-library/src/scan.rs:2063:21: replace += with *= in measure_dir`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.

### `crates/taiyaku-library/src/schema.rs`

- `crates/taiyaku-library/src/schema.rs:251:20: replace > with < in apply_pending`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.

### `crates/taiyaku-library/src/state.rs`

- `crates/taiyaku-library/src/state.rs:252:5: replace work_slug -> Result<String, String> with Ok("xyzzy".into())`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-library/src/state.rs:252:5: replace work_slug -> Result<String, String> with Ok(String::new())`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.

### `crates/taiyaku-library/src/watch.rs`

- `crates/taiyaku-library/src/watch.rs:135:30: replace match guard names_the_probe(&event, &probe_path) with true in probe`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-library/src/watch.rs:274:13: replace > with >= in fmt_interval`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-library/src/watch.rs:276:20: replace > with >= in fmt_interval`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-library/src/watch.rs:276:24: replace && with || in fmt_interval`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.

### `crates/taiyaku-read/src/format.rs`

- `crates/taiyaku-read/src/format.rs:154:16: replace < with <= in read_header`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.

### `crates/taiyaku-read/src/naming.rs`

- `crates/taiyaku-read/src/naming.rs:230:40: replace && with || in marked_number`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-read/src/naming.rs:230:56: replace + with - in marked_number`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.

### `crates/taiyaku-read/src/pages.rs`

- `crates/taiyaku-read/src/pages.rs:283:9: delete match arm crate::Format::Rar in unsupported_container`
  **Equivalent in the build the sweep makes.** The arm is `#[cfg(not(rar))]`
  and `.cargo/mutants.toml` sweeps with every feature, so the mutated line is
  not compiled. In the build without RAR, which the `default-features` CI job
  tests, `a_rar_header_is_refused_without_a_rar_backend` fails against it:
  `unsupported_container` gives `None` for a RAR header.
- `crates/taiyaku-read/src/pages.rs:372:23: replace match guard format == crate::Format::Rar with false in refuse_unsupported_container`
  **Equivalent in the build the sweep makes**, as `:283:9` is: the guard is
  `#[cfg(not(rar))]` too. Without RAR, `a_rar_header_is_refused_without_a_rar_backend`
  fails against it: a RAR gets the generic refusal, which does not say the
  build reads no RAR.
- `crates/taiyaku-read/src/pages.rs:372:23: replace match guard format == crate::Format::Rar with true in refuse_unsupported_container`
  **Equivalent in the build the sweep makes**, as `:283:9` is. Without RAR,
  `a_container_no_build_reads_is_not_refused_for_want_of_a_rar_backend` fails
  against it: a 7-Zip archive is told the build reads no RAR.
- `crates/taiyaku-read/src/pages.rs:372:30: replace == with != in refuse_unsupported_container`
  **Equivalent in the build the sweep makes**, as `:283:9` is. Without RAR,
  both tests above fail against it: the RAR refusal and the 7-Zip one swap
  sentences.
- `crates/taiyaku-read/src/pages.rs:389:5: replace page_count -> u32 with 0`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-read/src/pages.rs:389:5: replace page_count -> u32 with 1`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-read/src/pages.rs:56:41: replace * with /`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-read/src/pages.rs:56:41: replace * with +`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-read/src/pages.rs:56:48: replace * with /`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-read/src/pages.rs:56:48: replace * with +`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.

### `crates/taiyaku-read/src/rar/libarchive/headers.rs`

- `crates/taiyaku-read/src/rar/libarchive/headers.rs:520:55: replace | with ^ in rar4_fields`
  **Equivalent mutant.** The low half of the packed size is a `u32` widened to
  `u64`, so it is below 2^32, and the high half is shifted left past it. No bit
  is set in both, so `|` and `^` give the same value.
- `crates/taiyaku-read/src/rar/libarchive/headers.rs:525:62: replace | with ^ in rar4_fields`
  **Equivalent mutant**, as `:520:55` is, for the unpacked size.
- `crates/taiyaku-read/src/rar/libarchive/headers.rs:544:29: replace | with ^ in rar4_file`
  **Equivalent mutant.** `0x01` and `0x02`, the two flags `SPLIT` joins, share
  no bit.

### `crates/taiyaku-serve/src/auth.rs`

- `crates/taiyaku-serve/src/auth.rs:276:12: delete ! in client_of`
  The `!said.swap(true, Ordering::Relaxed)` that makes each `X-Forwarded-For`
  warning print once per process, `said` being one of two statics, one per case.
  Deleting the `!` inverts which call prints, and no in-process test can see it:
  a second test in the same binary observes an atomic the first already swapped,
  so any assertion here depends on test order.

  **The decision it guards is tested.** `ignored_forwarded_warning` was extracted
  out of this `if` for exactly that reason and
  `auth::tests::the_forwarded_warning_fires_only_when_a_header_is_being_ignored`
  covers it. What is left is the once-ness, which is the part that is genuinely
  about the process rather than about the request.
- `crates/taiyaku-serve/src/auth.rs:781:26: replace || with && in from_basic`
  **Equivalent in practice, and it is worth saying why rather than asserting it.**
  The expression is `pair.is_err() || address.is_err()` over two
  `account::note_failure` calls made back to back on one connection, against one
  table, with the same statement and different key strings. `||` and `&&` differ
  only when exactly one of them errors, and no realistic failure does that: the
  errors reachable here are `SQLITE_BUSY` on the shared connection and the table
  being gone, both of which take out both calls.

  **This became equivalent by being fixed.** It was
  `note_failure(&conn, &key).is_err() || note_failure(&conn, &wide).is_err()`,
  where `||` short-circuits — so a pair-key failure skipped the wide key
  entirely, dropping the counter that bounds the denial of service on the path
  that most needs it. Both calls are now made unconditionally and the decision is
  taken afterwards, which is what removed the difference the mutant was pointing
  at.

  It stops being equivalent if the two counters ever move to different tables or
  different connections. `session::tests::a_failure_the_limiter_cannot_record_is_answered_as_a_lockout`
  and its `Basic` twin cover the case where both fail.
- `crates/taiyaku-serve/src/auth.rs:797:26: replace || with && in from_basic`
  **Equivalent in practice, and it is worth saying why rather than asserting it.**
  The expression is `pair.is_err() || address.is_err()` over two
  `account::note_failure` calls made back to back on one connection, against one
  table, with the same statement and different key strings. `||` and `&&` differ
  only when exactly one of them errors, and no realistic failure does that: the
  errors reachable here are `SQLITE_BUSY` on the shared connection and the table
  being gone, both of which take out both calls.

  **This became equivalent by being fixed.** It was
  `note_failure(&conn, &key).is_err() || note_failure(&conn, &wide).is_err()`,
  where `||` short-circuits — so a pair-key failure skipped the wide key
  entirely, dropping the counter that bounds the denial of service on the path
  that most needs it. Both calls are now made unconditionally and the decision is
  taken afterwards, which is what removed the difference the mutant was pointing
  at.

  It stops being equivalent if the two counters ever move to different tables or
  different connections. `session::tests::a_failure_the_limiter_cannot_record_is_answered_as_a_lockout`
  and its `Basic` twin cover the case where both fail.

### `crates/taiyaku-serve/src/fingerprint_pass/read.rs`

- `crates/taiyaku-serve/src/fingerprint_pass/read.rs:229:25: replace && with || in walk`
  Equivalent for every read the stamp check keeps. The `||` adds the case of
  a walk that ended with no read error short of the listed count, which needs
  the walk and the listing before it to disagree about a member whose size
  and mtime did not move: a rewrite in place that the stamp cannot see, after
  which the row is wrong either way.
- `crates/taiyaku-serve/src/fingerprint_pass/read.rs:229:41: replace < with <= in walk`
  Equivalent. The walk stops at the listed count, so `prints.len()` never
  passes it, and `<=` adds only the case where the two are equal, where
  `resize(count, None)` changes nothing.

### `crates/taiyaku-serve/src/fingerprint.rs`

- `crates/taiyaku-serve/src/fingerprint.rs:200:22: replace || with && in flat_border`
  Equivalent. Neither half of `right <= left || bottom <= top` can hold: the
  guard above it returns for a page under 8 on either axis, and each side's
  scan stops a quarter of the way in, so `left` ends at most at `w / 4` and
  `right` at least at `w - 1 - w / 4`, which is greater for every `w` of 8 or
  more, and `top` and `bottom` likewise. No page tells `||` from `&&`; only
  deleting the guard would kill it. The first workspace-wide sweep,
  2026-08-19, recorded it as UNTRIAGED at
  `crates/taiyaku-cli/src/fingerprint.rs:167:22`; it was triaged when content
  matching's PR 2 moved the code here.

### `crates/taiyaku-serve/src/lib.rs`

- `crates/taiyaku-serve/src/lib.rs:545:8: delete ! in serve`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-serve/src/lib.rs:546:88: delete ! in serve`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-serve/src/lib.rs:546:9: replace && with || in serve`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-serve/src/lib.rs:562:13: delete match arm Ok(true) in serve`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.

### `crates/taiyaku-serve/src/match_pass.rs`

- `crates/taiyaku-serve/src/match_pass.rs:504:30: replace < with <= in Looking<'_>::pair`
  Equivalent. It differs only where the query and its leader share a digest,
  and a leader is never the query: `volume_match::leaders` votes among the
  other files, and the listing holds each digest once.
- `crates/taiyaku-serve/src/match_pass.rs:751:5: replace say with ()`
  A line on stderr. `say` writes the pass's diagnostics there, and no test
  reads stderr in-process.

### `crates/taiyaku-serve/src/registry.rs`

- `crates/taiyaku-serve/src/registry.rs:184:9: replace Works::is_live -> bool with true`
  **Equivalent in behaviour, and the reason is one line in another function.**
  `is_live` decides only whether `ServeState::find` and
  `ServeState::find_blocking` ask for a refresh on a miss, and whether `serve`
  spawns the refresher. A fixed registry asked anyway answers
  `Refresh::Unmoved` from `refresh_if_moved_with`'s own `!self.live` check,
  without opening the library, so `true` costs a trip to the blocking pool per
  `find` miss and a task that wakes every two seconds to do nothing, and changes no
  answer. Hand-applied on the live-works branch, it failed nothing in
  `taiyaku-library`, `taiyaku-serve` or `taiyaku-cli`.

  It stops being equivalent if `refresh_if_moved_with` ever stops checking
  `live` itself. That check's own `delete !` mutant is caught.

### `crates/taiyaku-serve/src/routes.rs`

- `crates/taiyaku-serve/src/routes.rs:1170:9: replace <impl IntoResponse for NoPrincipal>::into_response -> Response with Default::default()`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.

### `crates/taiyaku-serve/src/session.rs`

- `crates/taiyaku-serve/src/session.rs:541:30: replace || with && in login`
  **Equivalent in practice, and it is worth saying why rather than asserting it.**
  The expression is `pair.is_err() || address.is_err()` over two
  `account::note_failure` calls made back to back on one connection, against one
  table, with the same statement and different key strings. `||` and `&&` differ
  only when exactly one of them errors, and no realistic failure does that: the
  errors reachable here are `SQLITE_BUSY` on the shared connection and the table
  being gone, both of which take out both calls.

  **This became equivalent by being fixed.** It was
  `note_failure(&conn, &key).is_err() || note_failure(&conn, &wide).is_err()`,
  where `||` short-circuits — so a pair-key failure skipped the wide key
  entirely, dropping the counter that bounds the denial of service on the path
  that most needs it. Both calls are now made unconditionally and the decision is
  taken afterwards, which is what removed the difference the mutant was pointing
  at.

  It stops being equivalent if the two counters ever move to different tables or
  different connections. `session::tests::a_failure_the_limiter_cannot_record_is_answered_as_a_lockout`
  and its `Basic` twin cover the case where both fail.
- `crates/taiyaku-serve/src/session.rs:558:30: replace || with && in login`
  **Equivalent in practice, and it is worth saying why rather than asserting it.**
  The expression is `pair.is_err() || address.is_err()` over two
  `account::note_failure` calls made back to back on one connection, against one
  table, with the same statement and different key strings. `||` and `&&` differ
  only when exactly one of them errors, and no realistic failure does that: the
  errors reachable here are `SQLITE_BUSY` on the shared connection and the table
  being gone, both of which take out both calls.

  **This became equivalent by being fixed.** It was
  `note_failure(&conn, &key).is_err() || note_failure(&conn, &wide).is_err()`,
  where `||` short-circuits — so a pair-key failure skipped the wide key
  entirely, dropping the counter that bounds the denial of service on the path
  that most needs it. Both calls are now made unconditionally and the decision is
  taken afterwards, which is what removed the difference the mutant was pointing
  at.

  It stops being equivalent if the two counters ever move to different tables or
  different connections. `session::tests::a_failure_the_limiter_cannot_record_is_answered_as_a_lockout`
  and its `Basic` twin cover the case where both fail.

### `crates/taiyaku-serve/src/volume_match.rs`

- `crates/taiyaku-serve/src/volume_match.rs:97:43: replace < with <= in score`
  Equivalent. It differs only for two sides of equal length and equal key, one
  file scored against itself, which the server never asks: a leader is never
  its query, and a key is a digest, so equal keys hold equal slots.
