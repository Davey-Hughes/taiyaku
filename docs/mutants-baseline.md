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

<!-- entries -->### `crates/taiyaku-cli/src/duplicates.rs`

- `crates/taiyaku-cli/src/duplicates.rs:453:53: replace > with >= in contradictions`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-cli/src/duplicates.rs:455:33: replace + with * in contradictions`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.

### `crates/taiyaku-cli/src/fingerprint.rs`

- `crates/taiyaku-cli/src/fingerprint.rs:170:22: replace || with && in flat_border`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.

### `crates/taiyaku-cli/src/library.rs`

- `crates/taiyaku-cli/src/library.rs:202:5: replace resolve_export_user -> Result<i64, String> with Ok(1)`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-cli/src/library.rs:720:20: delete ! in export_targets`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.

### `crates/taiyaku-cli/src/main.rs`

- `crates/taiyaku-cli/src/main.rs:1479:8: delete ! in serve_work`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-cli/src/main.rs:1808:5: replace slot_index -> u32 with 0`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-cli/src/main.rs:1808:5: replace slot_index -> u32 with 1`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-cli/src/main.rs:1828:5: replace print_alignment_detail with ()`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-cli/src/main.rs:726:37: replace / with * in verify`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-cli/src/main.rs:726:37: replace / with % in verify`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-cli/src/main.rs:788:5: replace page_count -> u32 with 0`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-cli/src/main.rs:788:5: replace page_count -> u32 with 1`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.

### `crates/taiyaku-cli/src/sample.rs`

- `crates/taiyaku-cli/src/sample.rs:58:30: replace < with <= in dev_pages`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.

### `crates/taiyaku-cli/src/score.rs`

- `crates/taiyaku-cli/src/score.rs:119:14: replace > with < in classify_single_cover`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-cli/src/score.rs:130:29: replace < with <= in classify_single_cover`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-cli/src/score.rs:150:12: replace * with + in absorbs`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-cli/src/score.rs:151:20: replace > with >= in absorbs`
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
- `crates/taiyaku-cli/src/score.rs:155:13: replace && with || in absorbs`
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
- `crates/taiyaku-cli/src/score.rs:507:34: replace == with != in command`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-cli/src/score.rs:675:33: replace += with -= in sample_worklist`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-cli/src/score.rs:675:33: replace += with *= in sample_worklist`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-cli/src/score.rs:693:27: replace += with -= in sample_worklist`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-cli/src/score.rs:693:27: replace += with *= in sample_worklist`
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

- `crates/taiyaku-cli/src/user.rs:116:5: replace read_password -> Result<String, String> with Ok("xyzzy".into())`
  The process boundary. `read_password` is a terminal prompt, two blocking reads
  from stdin and an `stty` call; nothing under `cargo test` has a tty. **The
  testable half is already extracted** — `accept(first, &second)` holds the rule
  about which inputs are refused and in what words, and it is tested; the
  commands take an injected `AskForPassword` rather than calling this at all.
  What survives here is the reading itself.
- `crates/taiyaku-cli/src/user.rs:116:5: replace read_password -> Result<String, String> with Ok(String::new())`
  The process boundary. `read_password` is a terminal prompt, two blocking reads
  from stdin and an `stty` call; nothing under `cargo test` has a tty. **The
  testable half is already extracted** — `accept(first, &second)` holds the rule
  about which inputs are refused and in what words, and it is tested; the
  commands take an injected `AskForPassword` rather than calling this at all.
  What survives here is the reading itself.
- `crates/taiyaku-cli/src/user.rs:119:8: delete ! in read_password`
  The `if !quiet` that warns when echo could not be turned off — a decision about
  what a terminal shows, reachable only with a tty. It sits above the extracted
  `accept` for the same reason the rest of this function does.
- `crates/taiyaku-cli/src/user.rs:159:5: replace read_line -> Result<String, String> with Ok("xyzzy".into())`
  `read_line` is `std::io::stdin().read_line`. There is nothing here but the read.
- `crates/taiyaku-cli/src/user.rs:159:5: replace read_line -> Result<String, String> with Ok(String::new())`
  `read_line` is `std::io::stdin().read_line`. There is nothing here but the read.
- `crates/taiyaku-cli/src/user.rs:169:5: replace set_echo -> bool with false`
  `set_echo` shells out to `stty` against an inherited stdin. Under a test harness
  stdin is not a tty, so it answers false — which would kill `-> true` and not
  `-> false`, and only while the harness captures stdin. A test whose result
  depends on whether the runner has a terminal is worse than this entry.
- `crates/taiyaku-cli/src/user.rs:169:5: replace set_echo -> bool with true`
  `set_echo` shells out to `stty` against an inherited stdin. Under a test harness
  stdin is not a tty, so it answers false — which would kill `-> true` and not
  `-> false`, and only while the harness captures stdin. A test whose result
  depends on whether the runner has a terminal is worse than this entry.

### `crates/taiyaku-core/src/route.rs`

- `crates/taiyaku-core/src/route.rs:131:61: replace - with / in route`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-core/src/route.rs:131:61: replace - with + in route`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.

### `crates/taiyaku-core/src/sidecar.rs`

- `crates/taiyaku-core/src/sidecar.rs:1326:46: replace || with && in Sidecar::trust`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.

### `crates/taiyaku-core/src/solve.rs`

- `crates/taiyaku-core/src/solve.rs:143:18: replace > with >= in calibrate`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-core/src/solve.rs:146:25: replace > with >= in calibrate`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.

### `crates/taiyaku-core/src/truth.rs`

- `crates/taiyaku-core/src/truth.rs:200:17: replace + with - in off_the_page`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-core/src/truth.rs:200:17: replace + with * in off_the_page`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-core/src/truth.rs:200:28: replace > with == in off_the_page`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-core/src/truth.rs:200:28: replace > with >= in off_the_page`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-core/src/truth.rs:201:40: replace + with - in off_the_page`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-core/src/truth.rs:201:40: replace + with * in off_the_page`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-core/src/truth.rs:203:28: replace > with >= in off_the_page`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-core/src/truth.rs:204:40: replace + with - in off_the_page`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-core/src/truth.rs:204:40: replace + with * in off_the_page`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.

### `crates/taiyaku-library/src/browse.rs`

- `crates/taiyaku-library/src/browse.rs:1320:5: replace visible_editions -> Result<HashMap<String, HashSet<String>>, String> with Ok(HashMap::new())`
  DEFERRED behind **O87**, and verified false rather than assumed so. Every test
  in `taiyaku-library` — 557 of them — passes with `visible_editions`' body
  replaced, while `taiyaku-serve` fails **five**, among them
  `opds::tests::the_works_feed_does_not_name_a_work_outside_the_grant` and
  `routes::tests::the_work_list_does_not_name_a_work_outside_the_grant`. Checked
  by hand on 2026-08-29 by making the substitution and running both packages,
  not read off a timing. This function exists *for* `taiyaku-serve` — its own
  doc says so — so every assertion on it lives across the crate boundary that
  `--test-workspace = false` does not cross. These stop being reported at all
  the day O87 is settled; they are not a gap in the tests.

- `crates/taiyaku-library/src/browse.rs:1320:5: replace visible_editions -> Result<HashMap<String, HashSet<String>>, String> with Ok(HashMap::from_iter([(String::new(), HashSet::new())]))`
  DEFERRED behind **O87**, and verified false rather than assumed so. Every test
  in `taiyaku-library` — 557 of them — passes with `visible_editions`' body
  replaced, while `taiyaku-serve` fails **five**, among them
  `opds::tests::the_works_feed_does_not_name_a_work_outside_the_grant` and
  `routes::tests::the_work_list_does_not_name_a_work_outside_the_grant`. Checked
  by hand on 2026-08-29 by making the substitution and running both packages,
  not read off a timing. This function exists *for* `taiyaku-serve` — its own
  doc says so — so every assertion on it lives across the crate boundary that
  `--test-workspace = false` does not cross. These stop being reported at all
  the day O87 is settled; they are not a gap in the tests.

- `crates/taiyaku-library/src/browse.rs:1320:5: replace visible_editions -> Result<HashMap<String, HashSet<String>>, String> with Ok(HashMap::from_iter([(String::new(), HashSet::from_iter([String::new()]))]))`
  DEFERRED behind **O87**, and verified false rather than assumed so. Every test
  in `taiyaku-library` — 557 of them — passes with `visible_editions`' body
  replaced, while `taiyaku-serve` fails **five**, among them
  `opds::tests::the_works_feed_does_not_name_a_work_outside_the_grant` and
  `routes::tests::the_work_list_does_not_name_a_work_outside_the_grant`. Checked
  by hand on 2026-08-29 by making the substitution and running both packages,
  not read off a timing. This function exists *for* `taiyaku-serve` — its own
  doc says so — so every assertion on it lives across the crate boundary that
  `--test-workspace = false` does not cross. These stop being reported at all
  the day O87 is settled; they are not a gap in the tests.

- `crates/taiyaku-library/src/browse.rs:1320:5: replace visible_editions -> Result<HashMap<String, HashSet<String>>, String> with Ok(HashMap::from_iter([(String::new(), HashSet::from_iter(["xyzzy".into()]))]))`
  DEFERRED behind **O87**, and verified false rather than assumed so. Every test
  in `taiyaku-library` — 557 of them — passes with `visible_editions`' body
  replaced, while `taiyaku-serve` fails **five**, among them
  `opds::tests::the_works_feed_does_not_name_a_work_outside_the_grant` and
  `routes::tests::the_work_list_does_not_name_a_work_outside_the_grant`. Checked
  by hand on 2026-08-29 by making the substitution and running both packages,
  not read off a timing. This function exists *for* `taiyaku-serve` — its own
  doc says so — so every assertion on it lives across the crate boundary that
  `--test-workspace = false` does not cross. These stop being reported at all
  the day O87 is settled; they are not a gap in the tests.

- `crates/taiyaku-library/src/browse.rs:1320:5: replace visible_editions -> Result<HashMap<String, HashSet<String>>, String> with Ok(HashMap::from_iter([("xyzzy".into(), HashSet::new())]))`
  DEFERRED behind **O87**, and verified false rather than assumed so. Every test
  in `taiyaku-library` — 557 of them — passes with `visible_editions`' body
  replaced, while `taiyaku-serve` fails **five**, among them
  `opds::tests::the_works_feed_does_not_name_a_work_outside_the_grant` and
  `routes::tests::the_work_list_does_not_name_a_work_outside_the_grant`. Checked
  by hand on 2026-08-29 by making the substitution and running both packages,
  not read off a timing. This function exists *for* `taiyaku-serve` — its own
  doc says so — so every assertion on it lives across the crate boundary that
  `--test-workspace = false` does not cross. These stop being reported at all
  the day O87 is settled; they are not a gap in the tests.

- `crates/taiyaku-library/src/browse.rs:1320:5: replace visible_editions -> Result<HashMap<String, HashSet<String>>, String> with Ok(HashMap::from_iter([("xyzzy".into(), HashSet::from_iter([String::new()]))]))`
  DEFERRED behind **O87**, and verified false rather than assumed so. Every test
  in `taiyaku-library` — 557 of them — passes with `visible_editions`' body
  replaced, while `taiyaku-serve` fails **five**, among them
  `opds::tests::the_works_feed_does_not_name_a_work_outside_the_grant` and
  `routes::tests::the_work_list_does_not_name_a_work_outside_the_grant`. Checked
  by hand on 2026-08-29 by making the substitution and running both packages,
  not read off a timing. This function exists *for* `taiyaku-serve` — its own
  doc says so — so every assertion on it lives across the crate boundary that
  `--test-workspace = false` does not cross. These stop being reported at all
  the day O87 is settled; they are not a gap in the tests.

- `crates/taiyaku-library/src/browse.rs:1320:5: replace visible_editions -> Result<HashMap<String, HashSet<String>>, String> with Ok(HashMap::from_iter([("xyzzy".into(), HashSet::from_iter(["xyzzy".into()]))]))`
  DEFERRED behind **O87**, and verified false rather than assumed so. Every test
  in `taiyaku-library` — 557 of them — passes with `visible_editions`' body
  replaced, while `taiyaku-serve` fails **five**, among them
  `opds::tests::the_works_feed_does_not_name_a_work_outside_the_grant` and
  `routes::tests::the_work_list_does_not_name_a_work_outside_the_grant`. Checked
  by hand on 2026-08-29 by making the substitution and running both packages,
  not read off a timing. This function exists *for* `taiyaku-serve` — its own
  doc says so — so every assertion on it lives across the crate boundary that
  `--test-workspace = false` does not cross. These stop being reported at all
  the day O87 is settled; they are not a gap in the tests.

### `crates/taiyaku-library/src/credential.rs`

- `crates/taiyaku-library/src/credential.rs:99:5: replace hasher -> argon2::Argon2<'static> with Default::default()`
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

### `crates/taiyaku-library/src/import.rs`

- `crates/taiyaku-library/src/import.rs:437:36: replace && with || in drop_stale_spoke`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-library/src/import.rs:437:72: replace && with || in drop_stale_spoke`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-library/src/import.rs:441:74: replace && with || in drop_stale_spoke`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.

### `crates/taiyaku-library/src/lib.rs`

- `crates/taiyaku-library/src/lib.rs:142:5: replace is_busy -> bool with true`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-library/src/lib.rs:180:56: replace < with <= in set_wal_within`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.

### `crates/taiyaku-library/src/librarydoc.rs`

- `crates/taiyaku-library/src/librarydoc.rs:697:5: replace find_series_by_title -> Result<Option<i64>, String> with Ok(None)`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.

### `crates/taiyaku-library/src/librarydoc.rs`

- `crates/taiyaku-library/src/librarydoc.rs:700:5: replace apply_with_series_id -> Result<(), String> with Ok(())`
  DEFERRED behind **O87**, and verified false rather than assumed so — for the
  second time, on the same function. Every suite in `taiyaku-library` passes
  with the body replaced; `taiyaku-serve` fails **five** tests. That is the
  exact case and the exact count O87's own entry records from #91, reproduced
  here by making the substitution and running both packages. `patch_series` is
  the only caller, so every assertion on this function lives across the crate
  boundary `--test-workspace = false` does not cross. It stops being reported
  the day O87 is settled.

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
- `crates/taiyaku-library/src/migrate.rs:382:14: replace > with < in held`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.

### `crates/taiyaku-library/src/preview.rs`

- `crates/taiyaku-library/src/preview.rs:939:34: replace - with / in overrides`
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

- `crates/taiyaku-library/src/proposals.rs:170:26: replace * with /`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-library/src/proposals.rs:2193:40: replace - with + in interior_indices`
  Equivalent: the bound becomes `page_count + 1`.
  **The upper half of the `clamp` never fires**, so raising its bound cannot
  change a returned value: `page_count * 3 / 5 <= page_count - 1` holds for
  every `page_count >= 2`, and the function returns empty below two. Predicted
  in the source before it was measured — `interior_indices`' own doc comment
  says "A mutation raising it to `page_count` survives the suite for exactly
  that reason".

  **Kept in the code rather than deleted, which is why these two are accepted
  rather than killed.** The clamp states an invariant of this function — an
  interior index is never 0 and never past the last page — not a property of
  whichever fractions somebody later picks. A test could only kill them by
  asserting on fractions that do not exist, and a fix that deleted the bound
  would leave the next fraction free to run off the end.
- `crates/taiyaku-library/src/proposals.rs:2193:40: replace - with / in interior_indices`
  Equivalent: the bound becomes `page_count / 1`, which is the mutation the
  code comment names verbatim.
  **The upper half of the `clamp` never fires**, so raising its bound cannot
  change a returned value: `page_count * 3 / 5 <= page_count - 1` holds for
  every `page_count >= 2`, and the function returns empty below two. Predicted
  in the source before it was measured — `interior_indices`' own doc comment
  says "A mutation raising it to `page_count` survives the suite for exactly
  that reason".

  **Kept in the code rather than deleted, which is why these two are accepted
  rather than killed.** The clamp states an invariant of this function — an
  interior index is never 0 and never past the last page — not a property of
  whichever fractions somebody later picks. A test could only kill them by
  asserting on fractions that do not exist, and a fix that deleted the bound
  would leave the next fraction free to run off the end.
- `crates/taiyaku-library/src/proposals.rs:2428:38: replace && with || in EditionOfSignals::volume_signal`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-library/src/proposals.rs:2549:20: replace += with *= in raise_same_series`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-library/src/proposals.rs:2570:53: replace += with *= in raise_attach_chapters`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-library/src/proposals.rs:2584:31: replace == with != in raise_attach_chapters`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-library/src/proposals.rs:2597:20: replace += with *= in raise_attach_chapters`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.

### `crates/taiyaku-library/src/scan.rs`

- `crates/taiyaku-library/src/scan.rs:200:17: replace > with >= in scan_root`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-library/src/scan.rs:357:28: replace += with *= in record`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-library/src/scan.rs:462:23: replace match guard format == taiyaku_read::Format::Zip.token() with true in pages_of`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-library/src/scan.rs:652:14: replace > with == in measure_dir`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-library/src/scan.rs:652:14: replace > with >= in measure_dir`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-library/src/scan.rs:682:21: replace += with -= in measure_dir`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-library/src/scan.rs:682:21: replace += with *= in measure_dir`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.

### `crates/taiyaku-library/src/schema.rs`

- `crates/taiyaku-library/src/schema.rs:242:20: replace > with < in apply_pending`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.

### `crates/taiyaku-library/src/state.rs`

- `crates/taiyaku-library/src/state.rs:197:5: replace work_slug -> Result<String, String> with Ok("xyzzy".into())`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-library/src/state.rs:197:5: replace work_slug -> Result<String, String> with Ok(String::new())`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.

### `crates/taiyaku-library/src/watch.rs`

- `crates/taiyaku-library/src/watch.rs:142:30: replace match guard names_the_probe(&event, &probe_path) with true in probe`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-library/src/watch.rs:283:13: replace > with >= in fmt_interval`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-library/src/watch.rs:285:20: replace > with >= in fmt_interval`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-library/src/watch.rs:285:24: replace && with || in fmt_interval`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.

### `crates/taiyaku-read/src/format.rs`

- `crates/taiyaku-read/src/format.rs:138:16: replace < with <= in read_header`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.

### `crates/taiyaku-read/src/naming.rs`

- `crates/taiyaku-read/src/naming.rs:238:40: replace && with || in marked_number`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-read/src/naming.rs:238:56: replace + with - in marked_number`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.

### `crates/taiyaku-read/src/pages.rs`

- `crates/taiyaku-read/src/pages.rs:390:5: replace page_count -> u32 with 0`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-read/src/pages.rs:390:5: replace page_count -> u32 with 1`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-read/src/pages.rs:63:41: replace * with /`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-read/src/pages.rs:63:41: replace * with +`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-read/src/pages.rs:63:48: replace * with /`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-read/src/pages.rs:63:48: replace * with +`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.

### `crates/taiyaku-serve/src/auth.rs`

- `crates/taiyaku-serve/src/auth.rs:283:12: delete ! in client_of`
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
- `crates/taiyaku-serve/src/auth.rs:647:26: replace || with && in from_basic`
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
- `crates/taiyaku-serve/src/auth.rs:663:26: replace || with && in from_basic`
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

### `crates/taiyaku-serve/src/lib.rs`

- `crates/taiyaku-serve/src/lib.rs:352:8: delete ! in serve`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-serve/src/lib.rs:353:88: delete ! in serve`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-serve/src/lib.rs:353:9: replace && with || in serve`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.
- `crates/taiyaku-serve/src/lib.rs:370:13: delete match arm Ok(true) in serve`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.

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

- `crates/taiyaku-serve/src/routes.rs:657:9: replace <impl IntoResponse for NoPrincipal>::into_response -> Response with Default::default()`
  UNTRIAGED: recorded by the first workspace-wide sweep, 2026-08-19, and not
  examined. Nothing is known about this mutant beyond the fact that it survived.

  It is here so a dispatched run's diff stays readable, **not** because anybody has
  judged it acceptable — `scripts/mutants-baseline.mjs` counts these separately
  and prints the total on every run for exactly that reason. Triaging one means
  deciding which of three it is: an equivalent mutant, a decision inside output
  nobody asserts on, or a missing assertion. Only the third is work on the tests.

### `crates/taiyaku-serve/src/session.rs`

- `crates/taiyaku-serve/src/session.rs:355:30: replace || with && in login`
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
- `crates/taiyaku-serve/src/session.rs:371:30: replace || with && in login`
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
