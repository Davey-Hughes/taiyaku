# Overlapping libraries, PR 1: one derive across libraries — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build §3 of `docs/superpowers/specs/2026-11-02-overlapping-libraries-design.md`: one derive over the whole store, so one work can have editions in several libraries, a byte-identical volume in two libraries is one work with one position per account, and an account is shown only the editions its grants reach. Libraries still cannot overlap: `scan::vetted` is unchanged.

**Architecture:** The readers of a work's and a series' library are cut over first, each green under today's per-library derive: Move becomes relabelling a directory and library counts go through directories (Task 1), and proposals pair across the store (Task 2). `fold::absorb` learns to carry a folded-away work's hand values (Task 3), because the store-wide derive (Task 4) folds every work that libraries kept apart. Migration 018 then reduces each content key and each series title to its oldest row in SQL, drops `work.library_id` and `series.library_id`, makes `content_key` unique and adds `member_root` (Task 5). A file held under two directories becomes one member with a `member_root` row per directory, and access, counts and removal read those rows (Tasks 6–7). The series page takes `?library=` (Task 8), Move's screen says who gains and loses sight (Task 9), and the docs close it (Task 10).

**Tech Stack:** Rust (`taiyaku-library`, `taiyaku-serve`, `taiyaku-cli`), SQLite (system library: 3.54 here, 3.40.1 in CI and the image), TypeScript/React (`web/`).

**Spec:** §3, with §1, §6, §7 and §9. What the plan settled that the spec left open, with the code read for it: `.superpowers/sdd/overlap-evidence/plan-findings.md` (§0 copies collapse into one member; §1 `member.root_id` stays; §2 hand documents of de-duplicated copies; §3 the user's rulings). Locations are by symbol; line numbers are hints taken at fdef833a.

**The user's rulings of 2026-11-02** on what the first draft left open:
- **No fold.** "There's nothing using this so a migration here doesn't have to be safe." Migration 018 keeps the lowest-id work of each `content_key` and the lowest-id series of each `sort_title`, re-points works to the kept series, and deletes the rest with everything that names it. The next scan fills in (Task 5; findings §3.1).
- **The derive's own folds carry hand values.** Where only the folded-away work has a work-level hand value (title, volume, series), it carries to the target. Where both hold different values, the fold is refused and both works stay, as a refused fold is today (Task 3; findings §3.2).
- **Copies collapse.** Candidates of one edition that share a `sha256` under different roots become one member, with a `member_root` row per root. This follows from the user's "one book" ruling (Task 6; findings §0).

## Global Constraints

- A code comment says why: the reason, the invariant, what breaks if it changes. No history and no measurements in a comment; they go in `docs/notes.md` or are dropped. A comment may point at a spec section or an O-number. `scripts/check-comments.mjs` enforces part of this in the gate.
- No AI attribution anywhere.
- **Crate runs**, whole crates and never named test files, logged and read from the log:
  ```sh
  E=.superpowers/sdd/overlap-evidence/pr1; mkdir -p "$E"
  cargo nextest run -p taiyaku-library --all-features --test-threads=8 >| "$E/T<n>-library.log" 2>&1; print -r -- "exit $?" >> "$E/T<n>-library.log"
  ```
  The same for `-p taiyaku-serve` and `-p taiyaku-cli` where a task names them. Quote the summary line and the `exit` line. Then `cargo clippy -p <crate> --all-targets --all-features -- -D warnings` and `cargo fmt --all -- --check`, each into its own log.
- **Web runs** from `web/`: `./node_modules/.bin/vitest run --reporter=default --project <node|jsdom|browser>` into a log. Never `pnpm exec`. A browser project that fails with import or connection errors and no failed assertion is the headless renderer crashing: check `coredumpctl` and rerun the whole project before suspecting the branch.
- **The baseline.** Before changing anything, a task runs its crates at its parent commit into `T<n>-before.log`. After its change, a failing test that its list below does not name is a regression until shown otherwise. A listed test is rewritten to state the new behaviour unless its entry says delete. The task report names every rewrite and deletion with its reason. Never delete a test because it fails. Establish "pre-existing" against the parent commit, not against main or another task's commit.
- **Watched mutations.** Every new or changed assertion: put the defect it names back by hand, watch the test fail, restore, and check `git diff` shows the restore. Record each in the task report.
- **Fixtures that must outlive Task 5.** From Task 5 on, two works cannot share a `content_key`. A fixture that writes `work` rows with SQL gives each its own key, as `fold/tests.rs`'s `seed_work` does by naming it. A pair built through the derive or `import` hashes its hub's digests, so it needs different hub archives. Only migration 018's own tests (Task 5) build same-key copies, on a store migrated to 17 explicitly.
- **SQL for SQLite 3.40.1**: no `RAISE()` with an expression (3.47), no `concat()`, `string_agg` or ordered aggregates (3.44). Task 5 and Task 10 run the three crates in `rust:1-bookworm` with the recipe in Task 5.
- **Stop rule.** A task that reaches about 120 tool calls stops at a green point, commits what is green, and reports what is left, so the controller can split it.
- **Test counts.** `docs/status.md`'s two Rust figures ("Six crates, N tests" ~149, "of the N are standing checks" ~226) are updated once, in Task 10, from the gate's own JUnit report. The web figures there are not gated; Task 10 measures them.
- Evidence and reports go in `.superpowers/sdd/overlap-evidence/pr1/`, never `target/`.
- Branch `overlapping-libraries`, in the main checkout. One commit per task, its subject saying what the task makes true. Nothing is pushed until Task 10.

---

### Task 1: Move relabels a directory; library counts go through directories

**Files:**
- `crates/taiyaku-library/src/relocate.rs`: rewritten around `dir`, `resolve`, `apply_move`, `move_cost` and `move_root`. `refused_before`, `refile`, `split`, `Census`, `Home`, `volumes_of`, `first_few`, `NAMED` and the verdict go, and so does the module doc's "writer in front of the derive".
- `crates/taiyaku-library/src/relocate/tests.rs`: rewritten.
- `crates/taiyaku-library/src/fold.rs`: `rehome_editions`, `Rehomed`, `Rehoming` and whatever only they use (its one caller was `relocate::split`). `fold/tests.rs` from `divided_pair` (~3254) to the end.
- `crates/taiyaku-library/src/libraries.rs`: `list` (~26) and `unfiled_works` (~292). `libraries/tests.rs`.
- `crates/taiyaku-library/src/browse.rs`: `filed_in` (~1533) and the module doc (~11-15). `browse/tests.rs`.
- `crates/taiyaku-serve/src/libraries_api.rs`: `moved_json` (~536) and `move_dir`'s answer (~670). `libraries_api/tests.rs`.
- `crates/taiyaku-serve/src/opds.rs`: `View::Filed`'s doc (~167) and `library_works_by_work` (~287). `opds/tests.rs`. `test_support.rs` (~2472-2484).
- `crates/taiyaku-cli/src/library.rs`: `unfiled_line`, and its test.

**Interfaces:**
```rust
pub struct Moved {
    pub library_id: i64, pub library: String,
    pub gains_sight: Vec<String>,   // accounts that see the directory after and not before
    pub loses_sight: Vec<String>,   // accounts that see it before and not after
    pub library_removed: Option<String>, pub grants_removed: usize,
    pub left_with_no_library: Vec<String>,
}
pub fn move_cost(conn, root_id, target) -> Result<Moved, MoveError>;  // rolled back
pub fn move_root(conn, root_id, target) -> Result<Moved, MoveError>;  // committed; derives nothing
```
`MoveError::Refused` stays, since serve builds it for a scan-reserved directory.

**The rules.**
- A move sets `root.library_id`, and makes the new library first where one is named. That is all it writes. Migration 15's `library_emptied_by_move` removes an emptied library and its grants.
- `gains_sight` and `loses_sight` name, in name order, the accounts with role `user` and `every_library = 0` granted the target and not the source, and granted the source and not the target. An administrator and an every-library account are never named, as in `scan::only_grant_holders`.
- `libraries::list`'s works count: works with a member whose `member.root_id` is one of the library's directories. `unfiled_works`: works none of whose members names a root. `filed_in(library)`: the slugs of works with a member under the library's directories. These read `member.root_id` until Task 7 moves them to `member_root`.
- The serve answer drops `derive` and carries the new fields.

**Tests to delete or rewrite:**
- `relocate/tests.rs` keeps, rewritten to the new answer: `a_dir_or_a_library_that_is_not_there_is_missing_and_its_own_library_is_a_mistake`, `a_move_makes_no_library_called_a_dot_name`, `moving_the_last_dir_of_an_accounts_only_library_names_who_is_left_with_none`, `a_new_library_is_made_for_the_dir_and_named_in_the_answer`, `a_dir_alone_in_its_library_takes_its_works_and_the_library_goes` (the library goes, every row stays as it was), `a_dir_leaving_a_library_that_keeps_another_keeps_its_works_as_they_were`, `a_rehearsal_answers_what_the_move_would_and_leaves_everything_as_it_was`, `a_move_with_documents_kept_leaves_the_works_document_where_it_was`, and `a_proposal_across_a_moved_dir_still_exists`.
- **Every other test in `relocate/tests.rs` is deleted:** they pin §6.2's re-file, split and join, and the refusals that protected a reader's place through them, or a refusal sentence naming volumes. That covers `the_same_volume_under_*`, `an_edition_with_members_on_both_sides_refuses_the_move`, `a_volume_the_target_holds_under_another_key_joins_it_as_an_edition`, every `a_divided_work_*`, every `a_move_beside_a_*`, `a_move_that_would_lose_*`, `a_work_that_cannot_be_joined_is_refused_naming_it`, `a_work_with_the_same_content_key_in_the_target_is_refused_and_not_a_store_fault`, `a_census_counts_*`, `an_edition_a_person_has_edited_is_not_replaced_by_a_split`, `a_spread_flag_on_the_edition_a_split_would_replace_refuses_the_move`, `a_tag_that_a_join_would_lose_refuses_the_move`, `the_works_of_a_series_the_target_holds_join_it`, `a_series_that_keeps_works_behind_stays_*`, `a_work_put_in_a_series_by_hand_*`, `a_move_carries_only_the_editions_*`, `a_position_on_an_edition_*`, `a_position_read_on_the_other_works_edition_*`, `moving_the_hub_carries_*`, `a_move_writes_back_a_document_that_went_missing`, `a_refused_move_changes_nothing_*`, `a_refusal_naming_four_volumes_*`, `an_edition_with_a_member_whose_dir_is_gone_*` and `first_few_names_three_and_counts_the_rest`, with their helpers (`o191`, `o191_with_a_french_edition`, `both_refused`, `a_pending_fold_that_loses`, and so on). O191's row cites three of them; Task 10 updates it.
- `fold/tests.rs`: every rehoming test from `divided_pair` to the end, because their function is deleted. Keep `store_dump` (~3491), which Task 3's tests use.
- `libraries/tests.rs`: `listing_names_each_library_with_its_dirs_and_works` (rewrite: counted through directories) and `unfiled_works_counts_only_works_in_no_library` (rewrite: works no directory holds).
- `browse/tests.rs`: `filed_in_answers_the_slugs_of_the_works_filed_in_a_library` (rewrite: through directories).
- `opds/tests.rs`: `the_by_work_feed_lists_a_work_under_the_library_it_is_filed_in_with_every_edition` (rewrite: under each library a directory of which holds one of its editions, with every edition the grant allows). Keep `an_imported_work_is_in_no_by_work_feed`.
- `libraries_api/tests.rs`: `moved_is_written_with_each_value_under_its_own_key`, `a_dir_is_rehearsed_and_then_moved_to_another_library`, `a_dir_is_moved_to_a_library_made_for_it` and `rehearsing_a_move_refuses_a_reader` are rewritten to the new fields. `a_refused_move_is_a_409_naming_its_volumes` is deleted: no move is refused for a volume now, and the scan-held 409s keep their own tests.
- `taiyaku-cli` `the_unfiled_line_appears_only_when_works_are_in_no_library`: rewrite if the line's wording changes.

- [ ] **Step 1.** Baseline: `taiyaku-library`, `taiyaku-serve` and `taiyaku-cli` at the parent.
- [ ] **Step 2: Write the failing tests.**
  - One library with two directories, `/v` and `/c`, holds a volume in `/v` and its chapter release in `/c`: one work, two editions. Moving `/c` to a new library writes no `work`, `edition`, `member`, `progress` or `note` row (compare dumps), and answers `loses_sight` as the account granted only the old library.
  - An account granted only the target is in `gains_sight`; an administrator and an every-library account are in neither list.
  - After that move, `libraries::list` counts the work in both libraries, and `filed_in` names it in both.
- [ ] **Step 3.** Implement. Delete what the lists above delete.
- [ ] **Step 4: Watched mutations.** Name an every-library account in `loses_sight`. Count works by a stale `work.library_id` in `list` (the move test must catch it). Leave `refile` in `apply_move`.
- [ ] **Step 5.** Crate runs (three crates), clippy, fmt. Commit: "Moving a directory changes only its library, and says who gains and loses sight of it; library counts go through directories".

### Task 2: Proposals pair across the store

**Files:**
- `crates/taiyaku-library/src/proposals.rs`: `SeriesRow` and `all_series` (~2511), `series_id_by_sort_title` (~2554), `series_library` (~2570), `raise_edition_of` (~2755), `raise_same_series` (~2799), `raise_attach_chapters` (~2851), `series_reached_by` (~1662), `evidence` (~2109-2140), `across_libraries` (~683) and its callers `acceptable_proposal`, `preview`, `offered_rule` and the `accept` path (~1355-1363), and their docs.
- `crates/taiyaku-library/src/derive.rs`: `rel_paths_for_series` (~2506), `rel_paths_by_series` (~2557) and `RelPathsBySeries`, keyed by title alone.
- `crates/taiyaku-library/src/proposals/tests.rs`: the block from `across_libraries_refusal` (~4784) to the end.
- `crates/taiyaku-serve/src/curation*` and its tests, where one asserts the "different libraries" refusal (grep it).

**The rules.** Pairing reads no library. `series_id_by_sort_title` finds the oldest series of that title in the store. `across_libraries` and `series_library` go. A rule is still scoped to one directory or every directory, and `common_directory_pattern` still refuses a subject spread over two directories (decision 4 of 2026-10-05; spec §3.3).

**Tests to delete or rewrite** (proposals F1 block):
- **Delete:** `across_libraries_refusal`, `one_in_no_library_refusal`, `a_proposal_across_two_libraries_is_refused_at_accept`, `a_proposal_whose_target_is_in_no_library_is_refused_in_its_own_words`, `a_proposal_whose_subject_is_in_no_library_is_refused_in_its_own_words`, `preview_refuses_a_proposal_not_within_one_library_before_the_sweep`, `a_series_in_no_library_pairs_only_with_another_in_none_and_has_no_dir_to_accept_from` (the refusal is gone). Also `the_same_title_in_two_libraries_raises_no_proposal_between_them`: from Task 4 two libraries hold one series of a title, and migration 018 (Task 5) keeps the older of an existing pair.
- **Rewrite:** `an_edition_of_is_raised_within_a_library_and_never_across_two` and `a_same_series_is_raised_within_a_library_and_never_across_two` (raised across two), `a_widened_rule_answers_another_proposal_within_a_library_and_never_across_two` (answers across two), `chapter_directories_are_counted_within_a_library_and_never_across_two` (counted across), `attach_chapters_names_the_series_in_the_chapters_own_library` (names the series of that title wherever its volumes are), `accepting_reads_only_the_subjects_own_library` (reads the subject's directories wherever they are), `a_dry_run_counts_each_librarys_series_apart`, and `chapters_are_a_question_about_the_older_of_two_series_of_one_name_in_a_library` (… in the store).

- [ ] **Step 1.** Baseline.
- [ ] **Step 2: Write the failing tests.**
  - `Foo` in library A and `Foo - Color` in library B: an `edition-of` proposal is raised.
  - Accepting it writes a rule scoped to the subject's one directory.
  - A subject whose entries sit under directories of two libraries is refused by `common_directory_pattern`, in its own words.
- [ ] **Step 3.** Implement.
- [ ] **Step 4: Watched mutations.** Restore the `library_id` inequality in `raise_edition_of`. Make `series_id_by_sort_title` answer the newest. Each must fail a test.
- [ ] **Step 5.** Crate runs (`taiyaku-library`, `taiyaku-serve`), clippy, fmt. Commit: "Proposals pair series wherever their files are; a rule is still scoped to one directory or every directory".

### Task 3: A derive's fold carries the folded work's hand values, or refuses where both differ

**Files:**
- `crates/taiyaku-library/src/fold.rs`: `absorb` (~1370) and its doc's list of refusals (~1315-1366), and `Moved` (~69).
- `crates/taiyaku-library/src/fold/tests.rs` (`seed_work`, `store_dump`).
- `crates/taiyaku-library/src/derive/tests.rs`: one test through `derive_with`'s `superseded` path.

**Interfaces:** `absorb`'s signature is unchanged. `Moved` gains `hand_carried: usize`. Nothing is added to `DeriveReport`; a refusal travels in `unfoldable`, as every refusal does.

**The rule** (the user's ruling of 2026-11-02):
- A seventh refusal, a read made with the other six before any write: `from` and `into` both hold one of `title_hand`, `volume_hand` or `series_id_hand`, and the two differ. The sentence names both works, the field and both values, and says nothing was moved or deleted. Both works stay.
- Before the final `DELETE`, each of the three that only `from` holds is written onto `into`, together with its effective column, as `librarydoc::apply` writes them: `title`, `volume` and `series_id` take the hand value. That way the target reads right before its next derive.
- Where any value carried and `from`'s `LIBRARY` marker is armed, `into`'s is armed too. `hand::reconcile` then owes the target a document and writes it where its root keeps documents. A read-back of an older document at the target's address cannot then undo the carry.
- Edition-level hand metadata, hand chapters, tags and credits keep today's rule: counted in `Moved::discarded`.

**Tests to read when they fail:** every `fold/tests.rs` and `derive/tests.rs` test whose folded-away work carries a hand title, volume or series (grep `title_hand`, `volume_hand` and `series_id_hand` in both); `folding_a_work_away_orphans_its_hand_document_on_the_same_pass` (the target now takes the folded work's values, and the document is still orphaned and named); and any proposals or preview test that accepts and rescans (O73's shape).

- [ ] **Step 1.** Baseline: `taiyaku-library`.
- [ ] **Step 2: Write the failing tests.** Fixtures through `seed_work`, two works whose editions are digest-identical and whose keys differ by name:
  - `from` holds a hand title and `into` none: after `absorb`, `into`'s `title_hand` and `title` are `from`'s, and `hand_carried == 1`. One test each for volume and series.
  - Both hold hand titles that differ: refused, nothing written (compare `store_dump` before and after), and both works stand.
  - Both hold the same hand title: folds.
  - `from`'s marker armed, `into`'s not: `into`'s is armed afterwards.
  - Through `derive_with`: a superseded work with a hand title is folded and its target carries the title. With two differing titles, `unfolded_works == 1`, both stand, and the refusal is in `unfoldable`.
- [ ] **Step 3.** Implement.
- [ ] **Step 4: Watched mutations.** Overwrite `into`'s hand title instead of refusing. Drop the carry (the first test must fail). Refuse when the two values are equal. Carry the hand column but not `title`.
- [ ] **Step 5.** Crate run, clippy, fmt. Commit: "A fold carries the folded work's hand title, volume and series, and is refused where the two works disagree".

### Task 4: The derive and a work's identity span the store

**Files:**
- `crates/taiyaku-library/src/derive.rs`: `GroupKey` (~866) and its doc, `Group` (~918), `classify` (~1175), `derive_with`'s `named_series` and `series_ids` (~606, ~655-672), `reconcile_series` (~1384), `write_work`'s import call and comments (~1554-1660), and `held_work` (~2037).
- `crates/taiyaku-library/src/import.rs`: `scanned` and `scanned_in` (~147-172), `import_with`'s `library` parameter, `find_by_key` (~467), `update_work` (~553) and `insert_work` (~605).
- `crates/taiyaku-library/src/libraries.rs`: `own_or_adoptable_work` goes. `own_or_adoptable_series` stays until Task 5: `librarydoc::find_series_by_title` still calls it, and with no library given it allows every series.
- Tests: `derive/tests.rs` from `two_releases_told_apart_only_by_their_directories_are_two_works` (~4813) to the end; `import/tests.rs` ~1733-2040 (read `a_document_lands_on_the_work_that_already_holds_its_path` too); `preview/tests.rs` `two_libraries_sharing_a_title` (~365) and its two tests.

**The rules.**
- The group key is `(series, volume, alone)`, and `Group` carries no library.
- `reconcile_series(conn, sort_title)`: the oldest series of that `sort_title`, else a new one.
- `held_work`: as now with no library. Where two rows sit at one address, a work with a member under some root comes before one with none, then the oldest.
- `find_by_key`: by `content_key`, keeping the document-path preference.
- The derive calls `import::scanned`, and `scanned_in` goes.
- `update_work`'s scan arm writes only `content_key`, and `insert_work` writes no `library_id`. The two columns stay in the schema until Task 5, written by nothing, so `librarydoc`'s library-scoped lookups see `NULL` and already behave store-wide.
- Comments that say "within a library" are rewritten.

**Tests to delete or rewrite:**
- **Delete** (adoption and library scoping no longer exist): `a_work_of_another_library_at_this_address_is_not_taken_for_this_librarys_own`, `a_volume_less_work_of_another_library_naming_this_path_is_not_taken_for_this_librarys_own`, `a_work_another_librarys_dir_holds_is_not_taken_at_this_librarys_address`, `a_volume_less_work_another_librarys_dir_holds_is_not_taken_at_this_librarys_path`, `a_series_and_a_work_in_no_library_are_adopted_by_the_next_derive`, `a_series_in_no_library_goes_to_the_first_library_whose_files_name_it`, `a_series_in_no_library_goes_with_its_works_dir_and_not_to_a_library_naming_its_title`, `a_library_takes_the_older_of_two_series_in_no_library`, `a_library_keeps_its_own_series_before_an_older_one_in_no_library`, `a_library_takes_the_older_of_two_works_in_no_library_at_its_address`, `a_library_takes_the_older_of_two_volume_less_works_in_no_library_at_its_path` (store-wide duplicates of `the_older_of_two_works_at_one_address_is_taken`), `two_works_in_no_library_sharing_a_key_each_go_with_their_own_dir` (two works cannot share a key from Task 5), `a_moved_dirs_work_is_not_taken_by_a_lower_library_scanning_a_copy_of_it`, `a_moved_dirs_work_is_not_taken_by_a_lower_library_scanning_the_same_volume`, `a_hand_document_does_not_find_a_series_in_no_library_that_another_librarys_dir_holds`, `a_hand_document_files_the_series_in_no_library_it_attaches_a_filed_work_to`, and `byte_identical_volumes_in_two_libraries_are_two_works_with_two_slugs` (Task 6 writes its replacement: one work, one member).
- **Delete in `import/tests.rs`:** `scanned_in_finds_its_own_librarys_work_and_not_anothers`, `scanned_in_adopts_a_work_in_no_library`, `scanned_in_adopts_the_older_of_two_works_in_no_library_and_only_that_one`, `scanned_in_finds_its_own_work_before_an_older_one_in_no_library`, `scanned_in_leaves_a_known_work_in_the_library_it_is_in` (the function goes); `a_document_import_prefers_a_filed_work_and_then_the_oldest`, `a_document_lands_on_its_holder_though_an_older_work_is_filed_too`, `a_documents_work_in_no_library_beside_a_filed_one` and the helper `two_unfiled_works_sharing_a_key` (they need two works of one key). Keep `a_document_import_finds_a_scanned_work_in_any_library`.
- **Rewrite:**
  - `the_same_title_in_two_libraries_makes_two_series_and_two_works` → one series, and one work with an edition from each where the directories set different edition keys (decision 4).
  - `a_work_another_librarys_dir_holds_stays_its_own` → one work.
  - `a_work_left_spanning_two_libraries_is_split_and_keeps_the_other_librarys_edition` → stays whole.
  - `a_dir_moved_to_another_library_carries_positions_and_notes_through_the_fold`, `a_dir_moved_out_of_a_library_it_was_alone_in_keeps_its_works`, `a_dir_moved_out_of_its_library_and_then_changed_keeps_its_work` and `a_moved_dirs_work_goes_with_its_dir` → one test: relabelling a directory's library, then deriving, changes no work id, slug, series, edition or position. That completes Task 1's claim.
  - `a_librarys_files_go_to_the_older_of_its_two_works_at_one_address` and `…_volume_less_works_at_one_path` → store-wide, or delete where they duplicate `the_older_of_two_works_at_one_address_is_taken`.
  - `a_work_follows_its_files_before_an_older_one_in_no_library_at_its_address`, `a_volume_less_work_follows_its_files_before_an_older_one_in_no_library` and `the_librarys_own_work_follows_its_files` → before an older one no directory holds.
  - `an_imported_work_is_taken_by_the_library_whose_files_match_it` → taken by the derive.
  - `a_hand_document_finds_its_series_in_its_own_library`, `a_hand_document_naming_a_new_series_mints_it_in_the_works_library` and `a_hand_document_resolves_one_title_by_the_works_library` → the oldest series of that title in the store.
  - `a_title_two_libraries_share_can_be_set_on_the_series_of_each` → refused across the store; `a_title_another_series_of_the_same_library_holds_is_still_refused` and `a_title_is_still_refused_where_either_series_is_in_no_library` fold into it.
  - `a_chapter_only_dir_names_its_librarys_series` → names the series of that title.
  - `preview/tests.rs`: `filing_a_series_under_a_title_only_another_library_holds_is_not_previewed_as_a_merge` and `the_unchanged_rules_count_one_title_in_two_libraries_as_two_series` → one series.
- **Keep:** `two_dirs_of_one_library_still_make_one_work_of_a_volume_and_its_other_release`, beside a new test for two libraries.

- [ ] **Step 1.** Baseline: `taiyaku-library`, `taiyaku-serve` and `taiyaku-cli`.
- [ ] **Step 2: Write the failing tests.**
  - A volume under library A's directory and its chapter release under library B's, with distinct edition keys: one work, two editions, one series.
  - The relabel test above.
  - `Foo` in two libraries is one series.
- [ ] **Step 3.** Implement. Then rewrite and delete the listed tests. The F1 blocks are contiguous: cut deleted tests by line range rather than one edit each.
- [ ] **Step 4: Watched mutations.** Put the library back in the group key. Make `reconcile_series` answer the newest. Make `held_work` prefer a rootless work.
- [ ] **Step 5.** Crate runs (three crates), clippy, fmt. Commit: "Series and works are derived across every library; a library no longer decides what a work is".

### Task 5: Migration 018

**Files:**
- New `crates/taiyaku-library/src/schema/018_overlapping_libraries.sql`.
- `crates/taiyaku-library/src/schema.rs`: `SCHEMA_VERSION` to 18, and the file appended to `MIGRATIONS`. Nothing else in the migration runner changes.
- `crates/taiyaku-library/src/librarydoc.rs`: `apply_series`'s library read and filing (~834-870), `find_series_by_title` (~1019-1045), and `set_series_titles`' collision check (~1140-1150). Each loses its library clause.
- `crates/taiyaku-library/src/libraries.rs`: `own_or_adoptable_series` and `own_or_adoptable` go.
- `crates/taiyaku-library/src/schema/tests.rs`.
- Any test or fixture still naming `library_id` on `work` or `series`. At fdef833a a single-line grep also finds such lines in `schema/tests.rs`, `scan/tests.rs`, serve's `auth/tests.rs` and `libraries_api/tests.rs`, `preview/tests.rs` and `test_support.rs`, beside the files Tasks 1–4 rewrite. Grep again here, multi-line SQL included.

**The migration** (new):
```sql
-- Spec 2026-11-02-overlapping-libraries-design.md §3.1: a library no longer
-- decides what a work or a series is, so `work` and `series` lose
-- `library_id`, and a work's content key is unique across the store again.
--
-- **Copies are reduced to the oldest row, not folded**, by the user's ruling
-- of 2026-11-02 that no store holds data worth carrying. Of each content key
-- the lowest-id work is kept, and of each `sort_title` the lowest-id series,
-- with every work re-pointed to it. The rest goes with everything that names
-- it, each table by name, because a migration runs with foreign keys off and
-- no cascade fires. The next scan writes the kept rows again from the files.
--
-- **`work` is rebuilt**, because `library_id` is part of a UNIQUE that DROP
-- COLUMN refuses. The copy keeps every id, and the index and six triggers
-- `DROP TABLE work` takes are recreated as they stood: 001's three that feed
-- `work_fts`, and 014's three that move the structure counter. `series`
-- loses its column in place.
--
-- **`member_root` is every directory holding a copy of a member**, and it is
-- what a grant, a library page and a removal read. `member.root_id` stays as
-- the directory of the copy `member.path` names, which a document's address,
-- `derive::superseded`'s key and the scan-entry joins need. The two triggers
-- keep that directory in the set, whoever writes the column. CASCADE on both
-- sides: a member's copies go with it, and a removed directory stops holding
-- anything, and neither deletes a page.
CREATE TEMP TABLE kept_series AS
SELECT s.id AS id, (SELECT min(o.id) FROM series o WHERE o.sort_title = s.sort_title) AS kept
  FROM series s;
CREATE TEMP TABLE gone_series AS SELECT id FROM kept_series WHERE id <> kept;

UPDATE work SET series_id = (SELECT kept FROM kept_series k WHERE k.id = work.series_id)
 WHERE series_id IN (SELECT id FROM gone_series);
UPDATE work SET series_id_hand = (SELECT kept FROM kept_series k WHERE k.id = work.series_id_hand)
 WHERE series_id_hand IN (SELECT id FROM gone_series);

DELETE FROM series_title    WHERE series_id IN (SELECT id FROM gone_series);
DELETE FROM series_tag      WHERE series_id IN (SELECT id FROM gone_series);
DELETE FROM external_id     WHERE series_id IN (SELECT id FROM gone_series);
DELETE FROM collection_item WHERE series_id IN (SELECT id FROM gone_series);
DELETE FROM credit          WHERE series_id IN (SELECT id FROM gone_series);
DELETE FROM proposal        WHERE subject_id IN (SELECT id FROM gone_series)
                               OR target_id IN (SELECT id FROM gone_series);
DELETE FROM series          WHERE id IN (SELECT id FROM gone_series);

CREATE TEMP TABLE gone_work AS
SELECT w.id FROM work w
 WHERE EXISTS (SELECT 1 FROM work o WHERE o.content_key = w.content_key AND o.id < w.id);
CREATE TEMP TABLE gone_edition AS
SELECT id FROM edition WHERE work_id IN (SELECT id FROM gone_work);

DELETE FROM run             WHERE alignment_id IN (SELECT id FROM alignment
                                                    WHERE work_id IN (SELECT id FROM gone_work));
DELETE FROM alignment       WHERE work_id IN (SELECT id FROM gone_work);
DELETE FROM truth_box       WHERE work_id IN (SELECT id FROM gone_work);
DELETE FROM truth_page      WHERE work_id IN (SELECT id FROM gone_work);
DELETE FROM truth_member    WHERE work_id IN (SELECT id FROM gone_work);
DELETE FROM progress        WHERE work_id IN (SELECT id FROM gone_work);
DELETE FROM note            WHERE work_id IN (SELECT id FROM gone_work);
DELETE FROM credit          WHERE work_id IN (SELECT id FROM gone_work)
                               OR edition_id IN (SELECT id FROM gone_edition);
DELETE FROM page_flag       WHERE edition_id IN (SELECT id FROM gone_edition);
DELETE FROM member          WHERE edition_id IN (SELECT id FROM gone_edition);
DELETE FROM edition         WHERE id IN (SELECT id FROM gone_edition);
DELETE FROM work_tag        WHERE work_id IN (SELECT id FROM gone_work);
DELETE FROM collection_item WHERE work_id IN (SELECT id FROM gone_work);
DELETE FROM migrated_document WHERE work_id IN (SELECT id FROM gone_work);
UPDATE series SET cover_work_id = NULL WHERE cover_work_id IN (SELECT id FROM gone_work);
DELETE FROM work            WHERE id IN (SELECT id FROM gone_work);

DROP TABLE gone_edition;
DROP TABLE gone_work;
DROP TABLE gone_series;
DROP TABLE kept_series;

CREATE TABLE work_new (
  id             INTEGER PRIMARY KEY,
  content_key    TEXT NOT NULL UNIQUE,
  slug           TEXT NOT NULL UNIQUE,
  series_id      INTEGER REFERENCES series(id) ON DELETE SET NULL,
  volume         REAL,
  title          TEXT NOT NULL,
  reading        TEXT NOT NULL,
  hub            TEXT NOT NULL,
  sidecar_path   TEXT,
  sidecar_mtime  INTEGER,
  imported       TEXT NOT NULL DEFAULT (datetime('now')),
  title_hand     TEXT,
  title_derived  TEXT,
  volume_hand    REAL,
  series_id_hand INTEGER REFERENCES series(id) ON DELETE SET NULL
);
INSERT INTO work_new (id, content_key, slug, series_id, volume, title, reading, hub,
                      sidecar_path, sidecar_mtime, imported, title_hand, title_derived,
                      volume_hand, series_id_hand)
SELECT id, content_key, slug, series_id, volume, title, reading, hub,
       sidecar_path, sidecar_mtime, imported, title_hand, title_derived,
       volume_hand, series_id_hand
  FROM work;
DROP TABLE work;
ALTER TABLE work_new RENAME TO work;

CREATE INDEX work_by_series ON work (series_id);
CREATE TRIGGER work_ai AFTER INSERT ON work BEGIN
  INSERT INTO work_fts (rowid, title) VALUES (new.id, new.title);
END;
CREATE TRIGGER work_ad AFTER DELETE ON work BEGIN
  INSERT INTO work_fts (work_fts, rowid, title) VALUES ('delete', old.id, old.title);
END;
CREATE TRIGGER work_au AFTER UPDATE ON work BEGIN
  INSERT INTO work_fts (work_fts, rowid, title) VALUES ('delete', old.id, old.title);
  INSERT INTO work_fts (rowid, title) VALUES (new.id, new.title);
END;
CREATE TRIGGER structure_work_ins AFTER INSERT ON work BEGIN
  UPDATE structure_version SET n = n + 1;
END;
CREATE TRIGGER structure_work_upd AFTER UPDATE ON work BEGIN
  UPDATE structure_version SET n = n + 1;
END;
CREATE TRIGGER structure_work_del AFTER DELETE ON work BEGIN
  UPDATE structure_version SET n = n + 1;
END;

ALTER TABLE series DROP COLUMN library_id;

CREATE TABLE member_root (
  member_id INTEGER NOT NULL REFERENCES member(id) ON DELETE CASCADE,
  root_id   INTEGER NOT NULL REFERENCES root(id) ON DELETE CASCADE,
  PRIMARY KEY (member_id, root_id)
) WITHOUT ROWID;
CREATE INDEX member_root_by_root ON member_root (root_id, member_id);

INSERT INTO member_root (member_id, root_id)
SELECT id, root_id FROM member WHERE root_id IS NOT NULL;

CREATE TRIGGER member_root_on_insert AFTER INSERT ON member
WHEN new.root_id IS NOT NULL
BEGIN
  INSERT OR IGNORE INTO member_root (member_id, root_id) VALUES (new.id, new.root_id);
END;
CREATE TRIGGER member_root_on_update AFTER UPDATE OF root_id ON member
WHEN new.root_id IS NOT old.root_id
BEGIN
  DELETE FROM member_root WHERE member_id = old.id AND root_id IS old.root_id;
  INSERT OR IGNORE INTO member_root (member_id, root_id)
  SELECT new.id, new.root_id WHERE new.root_id IS NOT NULL;
END;
```
This SQL was applied to a scratch v17 store on 3.54 and on 3.40.1 (`rust:1-bookworm`'s python3). The store held a copy pair, a same-title series pair, a position and a note on the younger copy, a page flag, and two proposals naming the younger series. In both, `foreign_key_check` was empty, the older work and series were kept, `work_fts` held only the kept works, the triggers behaved as written, and a second row of one key failed on `work.content_key` (findings §3, §4). The dependants are every foreign key naming `work`, `edition`, `alignment` or `series` at v17. Before relying on that list, check it against a v17 store's `pragma_foreign_key_list`, and check the column list against its `sqlite_schema`.

**A younger copy's hand document** stays on disk untouched; no code changes for it. `hand::reconcile` over its root reports it as an orphan while nothing addresses it, as any stranded document is reported today. `hand::load` would read it only if the kept work's address later moved to that directory while the work's marker was unarmed, and then it reads a document of that work's own content key, which is the read-back store spec §4.4 is for (findings §2).

**Tests to delete or rewrite:**
- `schema/tests.rs`: `a_content_key_is_unique_within_a_library_and_a_slug_across_all_of_them` → a content key and a slug are each unique across the store. `a_library_goes_when_its_last_root_moves_away_and_its_works_stay` → without its `work.library_id` assertion.
- The `migration_15_*` tests stop at 15 and stay.
- Every fixture or assertion naming `library_id` on `work` or `series` is rewritten to read through directories, or dropped where it only pinned the filing.

- [ ] **Step 1.** Baseline: three crates.
- [ ] **Step 2: Write the failing tests** in `schema/tests.rs`. Each starts from a store at 17, built with `migrate_with(&MIGRATIONS[..17])` and written with SQL. The fixture has libraries A and B, and series 1 and 2 both titled `Foo`; series 2 has an alternate title, a tag, an external id and a proposal naming it. Works 1 and 2 share a content key (in series 1 and 2); work 2 holds an edition, a member, a page flag, an alignment with a run, a truth page with a box and its digests, a position, a note, a tag, a credit, a collection place and a `LIBRARY` marker. Work 3 has its own key, with `series_id` and `series_id_hand` naming series 2. Series 1's `cover_work_id` names work 2. After `migrate`:
  - work 1 and its rows stand;
  - work 2 and every row naming it or its edition are gone;
  - series 2 and its titles, tags, external ids and proposals are gone;
  - work 3 names series 1 in both columns, and series 1's `cover_work_id` is NULL;
  - `PRAGMA foreign_key_check` is empty, and `work_fts` holds works 1 and 3 alone;
  - inserting a second work of work 1's key fails on `work.content_key`.

  Then, on a fresh store:
  - the index and six triggers on `work` equal a v17 store's, as `migration_15_recreates_every_index_and_trigger_work_had` compares them;
  - `member_root` holds each member's `root_id`; inserting a member with a root adds its row, changing the root replaces it, and deleting the root removes it;
  - `series` has no `library_id` column, and a fresh store reaches 18.
- [ ] **Step 3.** Implement. Then remove `librarydoc`'s and `libraries`' remaining library code, which changes no behaviour since Task 4.
- [ ] **Step 4: Watched mutations.**
  - Drop the `note` delete; the migration's own `foreign_key_check` in `apply_pending` must refuse it, and the test see it.
  - Keep the highest id instead of the lowest.
  - Drop the `series_id_hand` re-point.
  - Drop `work_au`.
- [ ] **Step 5.** Crate runs (three crates), clippy, fmt. Then CI's SQLite:
  ```sh
  H=$(git rev-parse --short=8 HEAD); mkdir -p "target/bookworm/src-$H" target/bookworm/target target/bookworm/cargo-home
  git archive HEAD | tar -x -C "target/bookworm/src-$H"
  docker run --rm --user "$(id -u):$(id -g)" \
    -v "$PWD/target/bookworm/src-$H:/src" -v "$PWD/target/bookworm/target:/t" -v "$PWD/target/bookworm/cargo-home:/c" \
    -e CARGO_TARGET_DIR=/t -e CARGO_HOME=/c -e HOME=/c -e RUSTUP_TOOLCHAIN=1.97.1-x86_64-unknown-linux-gnu \
    -w /src rust:1-bookworm \
    cargo test --locked --no-fail-fast -p taiyaku-library -p taiyaku-serve -p taiyaku-cli -- --test-threads=8 \
    >| .superpowers/sdd/overlap-evidence/pr1/T5-bookworm.log 2>&1; print -r -- "exit $?" >> .superpowers/sdd/overlap-evidence/pr1/T5-bookworm.log
  ```
  Commit first, since `git archive HEAD` reads the commit. Check the toolchain name with `rustup toolchain list` inside the image. Two tests fail there by design (`stamp::tests::a_rewrite_syncs_…` and `opds::tests::the_work_feed_stats_its_members_on_the_blocking_pool`: no `strace`). Anything else failing is real. Commit: "Migration 18: a work is unique by content across the store, a series by title, and a member's directories are a set".

### Task 6: A file held under two directories is one member

**Files:**
- `crates/taiyaku-library/src/derive.rs`:
  - `Candidate` (~877) gains `copies: Vec<Copy { root_id, path, rel_path }>`;
  - `classify` (~1240-1290): an entry whose `sha256` matches a candidate of the same edition under another root becomes a copy, and is not counted in `merged`;
  - `write_work` (~1480), after `held_work`: a candidate swaps to the copy whose root the held work's member of that `sha256` names, else keeps the lowest root id;
  - `write_roots` (~2252): it sets `root_id`, then writes the member's `member_root` rows to exactly the primary and the copies. A root removed mid-derive skips its row and counts in `root_removed_mid_derive`;
  - `DeriveReport::merged`'s doc.
- `crates/taiyaku-library/src/import.rs`:
  - `HeldMember` (~1110) gains every root held for the hash;
  - `held_member_meta` (~1190): `member_root` restores as the union of every root the hash's rows held. `root_id` restores from the held row with the same `sha256` and `path`, else the one distinct root, else NULL;
  - `replace_editions`' restore (~898) writes those rows;
  - `HeldMember::root_id`'s doc.
- `crates/taiyaku-library/src/stamp.rs`: `plan` (~55) leaves out a member with more than one `member_root` row, as it leaves out an unreadable one. The serve route that reports skipped members (grep `skipped_formats`) names it.
- Tests in `derive/tests.rs`, `import/tests.rs`, `stamp/tests.rs`, and serve's stamp route tests.

**Tests to read when they fail:** `nested_roots_covering_the_same_path_are_not_superseded` (~2917; two roots over one path may now collapse); `two_entries_joining_one_edition_are_counted_as_a_merge` (its entries may be copies); `held_member_meta`'s two-roots tests in `import/tests.rs` (grep `two roots`, `ambiguous`; rewrite: both roots restore as rows, and the column follows the held row of the same path).

- [ ] **Step 1.** Baseline.
- [ ] **Step 2: Write the failing tests.**
  - Spec §1: A (`/a/Foo/Foo v01.cbz`) and B (`/b/Foo/Foo v01.cbz`), same bytes, one work with one member and `member_root` rows for both roots, its key that of one member, one slug, and one position read through either.
  - Sticky: a store holding the work under root B, then a copy added under a root with a lower id: the next derive leaves `member.root_id`, `member.path` and `hand::path_for` as they were.
  - A document reimport over such a work restores both rows.
  - `stamp::plan` leaves the copied member out.
  - Two roots of one library holding a copy are one member, and the derive counts the rekey.
- [ ] **Step 3.** Implement.
- [ ] **Step 4: Watched mutations.** Collapse copies under the same root as well. Swap to the lowest root even when held names another. Restore `root_id` from the first held row. Stamp a copied member.
- [ ] **Step 5.** Crate runs (`taiyaku-library`, `taiyaku-serve`), clippy, fmt. Commit: "A volume held under two directories is one member, with each directory recorded".

### Task 7: Access, counts and removal read `member_root`

**Files:**
- `crates/taiyaku-library/src/browse.rs`: `root_clause_for` (~297), `member_clause` (~326), the module doc (~20) and `filed_in`.
- `crates/taiyaku-serve/src/metadata.rs`: `root_grant_clause` (~378), `edition_owner` (~410) and `member_owner` (~440).
- `crates/taiyaku-library/src/libraries.rs`: `list` and `unfiled_works`.
- `crates/taiyaku-library/src/scan.rs`: `DOOMED_WORKS` (~743), `take_root` (~804) and their docs.
- `crates/taiyaku-cli/src/user.rs`: the unrooted count (~476).
- `account::access_for` and `browse::library_scope` resolve grants to root ids through `root.library_id`, which PR 1 keeps. Read them; they do not change.

**The rules.**
- Each `… root_id IN (holes)` becomes `EXISTS (SELECT 1 FROM member_root mr WHERE mr.member_id = <member> AND mr.root_id IN (holes))`.
- `DOOMED_WORKS`: works with a member that has a `member_root` row for `?1`, where no member of the work either has no `member_root` row at all (an imported member) or has one for another root.
- `take_root` deletes the members whose every `member_root` row is `?1`'s, not `WHERE root_id = ?1`. A surviving member whose `member.path` was under `?1` is re-pointed by the derive that follows, in the same transaction.
- The counts and `filed_in` read `member_root`.
- The CLI counts members with no `member_root` row.

- [ ] **Step 1.** Baseline: three crates.
- [ ] **Step 2: Write the failing tests** (spec §7, "Access"), in `browse/tests.rs`, with one serve route test each for a page and a download:
  - a work whose one edition is under Kids' directory and whose other is under Manga's: an account granted only Kids is listed the work with the Kids edition alone, and is refused the other's page;
  - an account granted neither sees nothing;
  - Task 6's copy under A and B is visible through either grant alone.
  - Removal, in `scan/tests.rs`: removing B's directory keeps the work, its position and its notes, and counts no work; then removing A's removes it. The dry run counts the same both times.
  - `libraries::list` counts the copy in both A and B.
- [ ] **Step 3.** Implement.
- [ ] **Step 4: Watched mutations.** Read `member.root_id` at one of the five sites (each site's test must catch it; do all five, one at a time). Delete members `WHERE root_id = ?1` in `take_root`.
- [ ] **Step 5.** Crate runs (three crates), clippy, fmt. Commit: "Grants, library pages, counts and removal read every directory that holds a copy".

### Task 8: The series page carries `?library=`

**Files:**
- `crates/taiyaku-library/src/browse.rs`: `one_series` (~956) takes `library: Option<i64>` and reads its card and works through `narrowed`.
- `crates/taiyaku-serve/src/library_api.rs`: `one_series` (~241) takes a `Query` like `HomeQuery`.
- `web/src/library.ts`: `oneSeries` (~174).
- `web/src/pageReads.ts`: `seriesRead` (~58), its address through `narrowedTo`.
- `web/src/Series.tsx`: reads `libraryParam` from the URL.
- `web/src/SeriesGrid.tsx` (~29): its link carries the library, passed in by `Library.tsx` (~285, ~356).
- `web/src/reads.ts` (~369): check that the invalidation of `/api/library/series/{id}` still reaches `?library=` variants.
- `web/src/Sidebar.tsx` is unchanged: the spec does not ask it to mark a series page.
- Tests: `browse/tests.rs`, `library_api/tests.rs`, `web/tests/Series.test.tsx`, `Library.test.tsx`, `pageReads.test.ts`, `reads.test.ts`, `Routes.test.tsx`.

**The rule.** With no library the page is as now. With a library, the card and the works are those the library's directories hold within the grant. A library outside the grant, or a series with nothing in it, answers as a series that does not exist, as `narrowed` answers `None`.

- [ ] **Step 1.** Baseline: `taiyaku-library`, `taiyaku-serve`, and the web's jsdom and node projects.
- [ ] **Step 2: Write the failing tests** (spec §3.6's case). Series `Foo` has v1–v4 under Manga's directory and v5 under Kids'. For an account granted both:
  - the Kids grid's card counts 1, and `/api/library/series/{id}?library=<Kids>` lists 1 work;
  - without `library` it lists 5;
  - the Kids grid's tile links to `/series/{id}?library=<Kids>`;
  - `seriesRead(id, kids)` and `seriesRead(id)` are two cache entries.
- [ ] **Step 3.** Implement.
- [ ] **Step 4: Watched mutations.** Drop `library` from `one_series`'s card query. Drop it from the tile link. Key the cache without it.
- [ ] **Step 5.** Crate runs, clippy, fmt, the web projects. Commit: "A series opened from a library's page shows what that library holds".

### Task 9: Move's screen says who gains and loses sight

**Files:**
- `web/src/libraries.ts` (`Moved`, ~134).
- `web/src/MoveCounts.tsx`: it draws `gains_sight` and `loses_sight`, and drops kept, joined, split and series-joined.
- `web/src/MoveRoot.tsx`: it drops "what the derive would find" and the volumes-in-the-way list, which no move answers now. It keeps the 409 sentence for a scan-held move, and keeps `left_with_no_library`.
- `README.md` (~272-276): the Move sentence.
- Tests: `web/tests/MoveRoot.test.tsx`; the `Moved` fixtures in `RootRow.test.tsx` (~408, ~2197), `Config.browser.test.tsx` (~1270, ~1305) and `heldFocus.browser.test.tsx` (~380).

- [ ] **Step 1.** Baseline: the web's jsdom and browser projects.
- [ ] **Step 2: Write the failing tests.**
  - A rehearsal answering `gains_sight: ["kim"]` and `loses_sight: ["ana"]` draws both names, and no count of works.
  - An empty rehearsal says no account's sight changes.
  - A 409 still draws its sentence.
- [ ] **Step 3.** Implement. Update the fixtures.
- [ ] **Step 4: Watched mutations.** Swap the two lists. Draw a works count from a missing field.
- [ ] **Step 5.** Web projects (jsdom, node, browser). Commit: "Move's dialog names who will gain and lose sight of the directory".

### Task 10: Docs, open items and the gate

**Files:** `docs/status.md`, `docs/superpowers/specs/2026-10-05-libraries-design.md`, `README.md` (~488-491), and `docs/notes.md` (the drive).

- [ ] **Step 1: `docs/status.md`'s libraries summary** (~129-145). Rewrite "Derivation and proposals stay inside one library: the same title in two libraries is two series, and a byte-identical volume in two is two works", and OPDS's "by the library the store files each work in". The new text: one derive over the store; a library is a name and its directories, and decides what an account sees and what a library page lists; a volume in two libraries is one work; proposals pair across the store. Name the spec.
- [ ] **Step 2: The 2026-10-05 spec.** An italic note pointing at `2026-11-02-overlapping-libraries-design.md` at each of: decisions 1, 2 and 3; §2.2; §3's "a series is in exactly one library"; §4's by-work view; and §6.2. Use the shape of the O220 note already in §6.2. The text above each note stays as written.
- [ ] **Step 3: README** (~488-491): two directories are derived together whichever libraries hold them.
- [ ] **Step 4: Open items.** Run the open-items sweep first and after (`grep -c '^| O[0-9]'`, the closed count, `grep -n -A1 '^What remains:'`).
  - **O191:** narrow it to what remains a defect under decision 4. In a partly visible work, the truth route sends and replaces the hidden edition's entries, and the notes route sends a reader's notes on it (read those routes on the branch to confirm). Rename its site from symbols the branch deleted, and drop its citations of tests Task 1 deleted.
  - **O189, O190, O194, O212, O213:** re-read each against the branch. Close one only where its shape can no longer be reached, by a drive or a reading stated as such; narrow otherwise. Rename any site the branch deleted (`proposals::across_libraries`, `relocate::refile`).
  - **Open O238**, a five-part edit: the head count word (~1417, "Two hundred and thirty-seven"), a table row after O237, an intro paragraph and a bullet after O237's bullet (~3802), and the "What remains" count (~4377). The row: a symlink inside one root to a directory in it walks its files twice under two paths; the derive makes one edition of the two, one `sha256` twice, so its pages show twice. That is O47's shape with no nested root. A symlink to another library's directory is accepted the same way. The site is `scan::measure_dir` and `derive::classify`; opened by the overlapping-libraries survey, 2026-11-02. The evidence is survey-store.md's E5 and E4, run on main 6067071d. Task 6's collapse is across roots only, so E5 is unchanged by this PR; say so.
  - Recount the closed and open counts from the table rather than by arithmetic. Then grep each bare O-number touched (O189, O190, O191, O192, O194, O212, O213, O238) across `docs/`, `crates/`, `web/src/`, `web/tests/` and `scripts/`, and read every hit.
- [ ] **Step 5: The Later entry** "Libraries whose directories overlap" (~5797): say the survey is done, the user's decisions are in the spec, PR 1 is this branch, and PRs 2 and 3 follow.
- [ ] **Step 6: Counts.** Measure the web counts (`--project node`, `--project jsdom`, `--project browser`) and write them in status.md's "What is built" sentence. The Rust figures come from Step 7's JUnit report.
- [ ] **Step 7: The gate.** Wait for load below 24 and record it.
  ```sh
  env -u TAIYAKU_CORPUS NEXTEST_TEST_THREADS=8 RUST_TEST_THREADS=8 scripts/check-all.sh >| .superpowers/sdd/overlap-evidence/pr1/gate.log 2>&1
  ```
  Use the repo's own script, never a copy. Read the exit status from the log's `all green` line, and do not pipe it through `tail`. When `status-test-count` names the two Rust figures, write them in status.md (~149, ~226) and run the gate again.
- [ ] **Step 8: Bookworm**, Task 5's recipe, on the final commit.
- [ ] **Step 9: Drive it on real material.**
  - Build `main` (6067071d) in a scratch worktree with its own `CARGO_TARGET_DIR`. Scan a scratch hardlinked copy of the test library (`/home/davey/temp/manga-test`; copy, never open its `library.db`) into a scratch database at schema 17. Add a second library holding a byte-identical copy of one volume, and read one page in each.
  - Copy that database and open the copy with the branch's build (`taiyaku library libraries`). Record the work and series counts before and after, that the older copy's slug survived, and that the younger's rows are gone.
  - Scan again. The shared volume is now one work with one member and both directories in `member_root`, and the older copy's position stands.
  - Serve it at 127.0.0.1:4399 and check, as an account granted one library, that the shared work shows once with its editions narrowed. Re-check the port after every kill, and kill by PID.
  - Write the results in `docs/notes.md`, under a section the status summary points at.
- [ ] **Step 10.** Commit: "Overlapping libraries PR 1: the docs, O191 narrowed, O238 opened".

---

## Part 2: narrowing a merged edition (spec §3.8)

Added 2026-11-03, after Task 10's drive found O239's case served whole. **Spec:** §3.8, decisions 6–10, and §7's "Narrowing". **Locations:** `.superpowers/sdd/overlap-evidence/pr1/survey-narrowing.md` (part 1) and `survey-narrowing-2.md` (part 2); line numbers there are hints at 07e24b60. Task 10's Steps 6–8 (counts, gate, bookworm) move to the Finish below, and "Nothing is pushed until Task 10" now reads "until the Finish".

**Architecture.** Core narrows an edition and translates its pages and slots (Task 11). The library crate answers which members of a work an account sees, and serve builds the account's view of a work from that answer; the work document reads the view (Task 12). The page routes, OPDS's included, translate through it (Task 13); downloads and cover versions read its members (Task 14); progress and notes translate through it (Task 15); truth and alignment corrections refuse a narrowed edition, and every route declares what it does with one (Task 16). The administrators' route (Task 17) and tab (Task 18) list the cases, and the docs and a drive on real material close it (Task 19).

**Added constraints** (the Global Constraints above still bind):
- `Access::Every` (administrators and "every library" accounts) is never narrowed and pays no new query.
- One rule for which members are visible: a `member_root` row whose root is in the account's `Access::Only` set, the test `browse::member_clause` makes. No second spelling of it.
- Stored data stays in the full numbering: `progress.slot`, `note.slot`, truth's page keys, `page_flag`, alignments. Nothing is migrated.
- **Fail closed.** When the flags for an edition do not number the sidecar's members (a scan committed between the two reads), the edition is hidden for that request.
- **The fixture.** Every narrowing test uses a fixture whose hidden member is not the last, so a translation that does nothing gives a different answer; a test's first assertion states the fixture's shape. Task 12 builds the serve fixture and Tasks 13–19 use it.
- `every_route_declares_its_grant_posture` pins `26` driven and `37` grant-independent (`auth/tests.rs` ~1697, ~1718). Only a task that adds a route moves them, and it says so.
- A comment budget in `scripts/comment-budget.json` is raised only with the reason in the task report; the PR body lists every change.

### Task 11: Core narrows an edition and translates its pages and slots

**Files:**
- Create `crates/taiyaku-core/src/narrow.rs`, with its tests in `crates/taiyaku-core/src/narrow/tests.rs` (the `mod tests;` shape of `sidecar.rs` ~1670). Declare and re-export it in `crates/taiyaku-core/src/lib.rs`.
- Read: `sidecar.rs` `Edition` (~210), `locate` (~253), `slots` (~270); `slot.rs` `Slot`, `Half`, `expand`.

**Interfaces (produces):**
```rust
/// An edition as an account that sees only some of its members sees it (spec §3.8).
pub struct Narrowed { /* private */ }

impl Narrowed {
    /// `visible[i]` says whether `edition.members[i]` is visible. `None` when
    /// `visible.len() != edition.members.len()`, or when no visible member has a page.
    pub fn new(edition: &Edition, visible: &[bool]) -> Option<Narrowed>;
    /// The edition the account is sent: the visible members in order, spreads
    /// and credits kept where their page is visible, renumbered.
    pub fn edition(&self) -> &Edition;
    /// Narrowed page index to full; `None` past the end.
    pub fn page_to_full(&self, page: u32) -> Option<u32>;
    /// Full page index to narrowed; `None` for a hidden page or past the end.
    pub fn page_from_full(&self, page: u32) -> Option<u32>;
    /// Narrowed member index to its index among the full edition's members.
    pub fn member_to_full(&self, member: usize) -> Option<usize>;
    /// Narrowed slot to full, through the slot's page and half; `None` past the end.
    pub fn slot_to_full(&self, slot: u32, reading: Direction) -> Option<u32>;
    /// Full slot to narrowed.
    pub fn slot_from_full(&self, slot: u32, reading: Direction) -> FromFull;
}

pub enum FromFull {
    Visible(u32),
    /// The slot's page is hidden. `fallback` is the first slot of the nearest
    /// visible page before it, or slot 0 when no visible page comes before it.
    Hidden { fallback: u32 },
    /// Past the end of the full edition.
    Outside,
}
```
`Narrowed` holds what its methods need; how it stores it is the implementer's.

- [ ] **Step 1.** Baseline: `cargo nextest run -p taiyaku-core` into `T11-before.log`.
- [ ] **Step 2: Write the failing tests.** The fixture: members A (3 pages), B (2), C (0), D (4), visible `[true, false, true, true]`; spreads at full pages 3 (B's first) and 5 (D's first); a credit at full page 8. Full pages: A 0–2, B 3–4, D 5–8.
  - Its shape: 9 full pages, B at 3–4.
  - The narrowed edition holds A, C and D, 7 pages, spreads `[3]`, credits `[6]`.
  - `page_to_full`: 0→0, 2→2, 3→5, 6→8, 7→None. `page_from_full`: 3→None, 4→None, 5→3, 9→None.
  - `member_to_full`: 0→0, 1→2, 2→3, 3→None.
  - For both reading directions: every narrowed slot round-trips through `slot_to_full` and `slot_from_full`, and the full slot it names has the same half on the mapped page (the spread at narrowed page 3 is two slots, each keeping its half).
  - A full slot on page 3 or 4 is `Hidden` with the first slot of narrowed page 2 as fallback. With visible `[false, true, true, true]`, a full slot on page 0 is `Hidden { fallback: 0 }`. A slot past the full end is `Outside`.
  - `new` is `None` for a mask of the wrong length, and for `[false, false, true, false]` (C has no page).
- [ ] **Step 3.** Run them and watch them fail.
- [ ] **Step 4.** Implement.
- [ ] **Step 5: Watched mutations.** `page_to_full` returns its argument. Spreads kept unrenumbered. `slot_from_full` drops the half (always the page's first slot). The fallback takes the next visible page instead of the previous.
- [ ] **Step 6.** Crate run (`taiyaku-core`), `cargo check -p taiyaku-wasm`, clippy, fmt. Commit: "Core narrows an edition to the members an account sees, and translates its pages and slots".

### Task 12: An account's view of a work, and the work document

**Files:**
- `crates/taiyaku-library/src/browse.rs`: new `visible_members` beside `edition_visible` (~1456). `member_clause` (~328) hard-codes the alias `m`.
- Create `crates/taiyaku-serve/src/view.rs`.
- `crates/taiyaku-serve/src/routes.rs`: `get_sidecar` (~764, its `ETag` ~794), and `patch_alignments` (~1846): the `ETag` its `If-Match` is compared with.
- `crates/taiyaku-serve/src/test_support.rs`: the fixture, beside `work_whose_editions_sit_in_two_libraries` (~2553).
- Tests: `crates/taiyaku-library/src/browse/tests.rs` (model: `merged_edition_library`, ~755), `crates/taiyaku-serve/src/routes/tests.rs`.

**Interfaces:**
- Consumes: Task 11's `Narrowed`.
- Produces:
  ```rust
  // taiyaku-library, browse.rs
  /// For each edition key of the work (or only `edition`), one flag per member
  /// in ordinal order: whether `access` sees it. Not asked for `Access::Every`.
  pub fn visible_members(conn: &Connection, access: &Access, slug: &str, edition: Option<&str>)
      -> Result<HashMap<String, Vec<bool>>, String>;
  ```
  In `view.rs`: `pub(crate) struct View` holding the sidecar as this account is sent it and a `Narrowed` per narrowed edition, with `sidecar()`, `narrowed(edition) -> Option<&Narrowed>` and `page_to_full(edition, page) -> Option<u32>` (the page itself for an edition that is not narrowed). A pure `View::build(sidecar: Arc<Sidecar>, flags: Option<&HashMap<String, Vec<bool>>>) -> Option<View>` (`None` flags: `Access::Every`, the sidecar as it is; `None` result: nothing visible). Two loaders: `view::of_work(state, extensions, slug)` for the work routes, refusing a work it cannot see as `visible_editions_or_refuse` (~643) refuses one, and `view::of_edition(state, extensions, slug, edition)` for the page routes, one query, refusing as `refuse_if_hidden` (~447) refuses. Their parameter types are `visible_editions_or_refuse`'s. `View::build` drops hidden editions and every alignment touching a hidden or narrowed edition, and moves `hub` as `filter_sidecar_editions` (~731) does.
- `visible_editions_or_refuse` and `sidecar_as_seen` stay for their other callers until Tasks 15–16 move them.

**The fixture**, `test_support::router_over_an_edition_merged_from_two_libraries()`: library Manga holds `/m` (`Foo v01 part1.cbz`, 2 pages, and `Foo v01 part3.cbz`, 4 pages), `/m-en` (edition key `en`, `Foo v01.cbz`, 5 pages) and `/m-fr` (edition key `fr`, `Foo v01.cbz`, 6 pages, different bytes); library Kids holds `/k` (`Foo v01 part2.cbz`, 3 pages). One work, editions `raw` (part1, part2, part3), `en` and `fr`. Seed one alignment `raw`↔`en` and one `en`↔`fr`. Accounts as `Granted` offers them, adding what the tests need: Manga only, Kids only, both, and an administrator. Full `raw` pages: part1 0–1, part2 2–4, part3 5–8. For Manga only, narrowed `raw` is part1 0–1 and part3 2–5. If the derive does not give this shape, change the names until it does; never the assertions. Which edition is the hub, and so the cover, follows from the derive: a test that depends on it states it first.

- [ ] **Step 1.** Baseline: `taiyaku-library`, `taiyaku-serve`.
- [ ] **Step 2: Write the failing tests.**
  - `visible_members` over `merged_edition_library` (and the serve fixture's shape, through the store): flags in ordinal order for each grant.
  - `View::build` with flags of the wrong length for `raw` hides `raw` and keeps `en` and `fr`.
  - The fixture's shape, from the administrator's document.
  - The work document for Manga only: `raw` holds part1 and part3, 6 pages; `en` and `fr` are whole; the `raw`↔`en` alignment is absent and `en`↔`fr` is present. For Kids only: `raw` holds part2 alone, and `en` and `fr` are absent. For both: `raw` is whole.
  - The document `ETag` for Manga only differs from the administrator's; each answers 304 to its own `If-None-Match` and 200 to the other's.
  - Manga only, sending its own document `ETag`, corrects the `en`↔`fr` alignment without a 412.
- [ ] **Step 3.** Run them and watch them fail.
- [ ] **Step 4.** Implement.
- [ ] **Step 5: Watched mutations.** Serve the full edition to an `Only` account. Flags in reverse ordinal order. Treat a length mismatch as all visible. The document `ETag` over the full sidecar. `patch_alignments` comparing with the full sidecar's `ETag`.
- [ ] **Step 6.** Crate runs (`taiyaku-library`, `taiyaku-serve`), clippy, fmt. Commit: "An account that sees part of an edition is sent only that part".

### Task 13: Pages, thumbs and OPDS's pages translate through the view

**Files:**
- `crates/taiyaku-serve/src/routes.rs`: `serve_page` (~885) and `get_thumb` (~1048), with `locate_member` (~93) and the `ETag` lookup they make. `ServeState::page` (`lib.rs` ~314) locates on the full edition: translate before calling it.
- `crates/taiyaku-serve/src/opds.rs`: `work_feed`'s `pse:count` (~571, emitted ~612) and the editions it lists; `stream_page` (~695); `cover` (~784, `locate(0)` ~810).
- Tests: `routes/tests.rs`, `opds/tests.rs`.

**Interfaces:** consumes Task 12's `view::of_edition`, `view::of_work` and `View::page_to_full`.

- [ ] **Step 1.** Baseline: `taiyaku-serve`.
- [ ] **Step 2: Write the failing tests** (the Task 12 fixture, as Manga only unless named):
  - Narrowed page 2's bytes and `ETag` equal the administrator's page 5's; narrowed page 6 is refused as a page past the end; `If-None-Match` with page 5's `ETag` on narrowed page 2 answers 304.
  - The thumb of narrowed page 2 equals the administrator's thumb of page 5.
  - Kids only: page 0 is the administrator's page 2.
  - OPDS: `raw`'s `pse:count` is 6 (Kids only: 3); the PSE stream's narrowed page 2 is the administrator's page 5; for Kids only, who sees `raw` alone, the cover is part2's page 0.
  - The series cover redirect (`/api/library/series/{id}/cover`) lands on a thumb whose bytes are part2's page 0 for Kids only.
- [ ] **Step 3.** Run them and watch them fail.
- [ ] **Step 4.** Implement.
- [ ] **Step 5: Watched mutations.** Skip the translation in `serve_page`, in `get_thumb`, in `stream_page`, and in the OPDS cover, one at a time. `pse:count` from the full edition.
- [ ] **Step 6.** Crate run, clippy, fmt. Commit: "A narrowed edition's pages are numbered from its first visible page".

### Task 14: Downloads and cover versions read the visible members

**Files:**
- `crates/taiyaku-serve/src/download.rs`: `prepare` (~671) and `member_paths` (~247) for the edition download (web and OPDS, ~497); `advertised_type` (~298, called from `opds.rs` ~657-677); `member` (~1162) and `prepare_member` (~1269), whose 404 names the member count.
- `crates/taiyaku-library/src/browse.rs`: `page_zero_hash` (~489) and its three uses (~527 series grid, ~634 `editions_by_work`, ~1413 `cover_target`). Its doc says it leaves out the grant on purpose: read why, and say in the new doc why narrowing changes it (the thumb sends `COVER_CACHE_CONTROL` only when `v` names the page it located, `routes.rs` ~915-919).
- Tests: `download/tests.rs`, `opds/tests.rs`, `browse/tests.rs`, `library_api/tests.rs`.

- [ ] **Step 1.** Baseline: `taiyaku-library`, `taiyaku-serve`.
- [ ] **Step 2: Write the failing tests** (the Task 12 fixture):
  - Manga only: `raw`'s download is a cbz of part1's and part3's pages, 6 pages in order. Kids only: `raw`'s download is part2's file passed through, and OPDS advertises its type.
  - Manga only: member ordinal 1 is part3; ordinal 2 is refused, and the refusal names 2 members.
  - The series card's and the work card's cover `?v=` for Kids only names part2's page 0 (Kids sees `raw` alone); for the administrator it is what it was before this task. A thumb fetched with Kids' `?v=` as Kids carries `COVER_CACHE_CONTROL`.
- [ ] **Step 3.** Run them and watch them fail.
- [ ] **Step 4.** Implement.
- [ ] **Step 5: Watched mutations.** Zip every member. Member ordinals from the full list. The refusal's count from the full list. `page_zero_hash` without the grant, at each of its three uses, one at a time.
- [ ] **Step 6.** Crate runs, clippy, fmt. Commit: "A narrowed edition downloads, and shows a cover, from the files the account sees".

### Task 15: Progress and notes are translated, and a notes write keeps what it cannot see

**Files:**
- `crates/taiyaku-serve/src/routes.rs`: `get_progress`/`put_progress` (~1553-1676), `get_notes` (~1187), `put_notes` (~1261).
- `crates/taiyaku-serve/src/notes.rs`: `validate_against` (~106).
- Tests: `routes/tests.rs`. Rewrite `a_new_note_on_an_edition_outside_the_grant_is_refused_and_a_held_one_is_kept` (~5797): the held note is no longer sent, and a write of what was sent keeps it.

**The rules.**
- **Progress read:** a position on a narrowed edition is sent as `slot_from_full`'s `Visible` slot, or its `Hidden` fallback; an `Outside` one is left out. Hidden editions are left out, as now.
- **Progress write:** the slot is checked against the narrowed edition's slots, in today's words, and stored as `slot_to_full`.
- **Notes read:** notes on a hidden edition are left out (O191's notes half), and so are notes on a narrowed edition's hidden pages or past its end; the rest are sent with narrowed slots. The `ETag` names the account's numbering (spec §3.8, decision 11; added by this task's fix round): for an account with grant limits, over the stored JSON and its view; for `Access::Every`, `notes_etag` of the stored JSON as before.
- **Notes write:** translate incoming slots on narrowed editions to full (a slot past the narrowed end is refused in `validate_against`'s words). The stored document becomes the translated incoming notes plus every stored note this account cannot see, unchanged. An incoming note with the id of a stored note it cannot see is kept only if it equals that note (a client that read before this change), and refused otherwise; a new note on a hidden edition is refused, as now. Validate the merged document, in full numbering, against `sidecar_as_seen` (the visible editions, not narrowed).

- [ ] **Step 1.** Baseline: `taiyaku-serve`.
- [ ] **Step 2: Write the failing tests** (the Task 12 fixture, Manga only, seeding through the store):
  - Notes n1 on `raw` page 1, n2 on page 3 (part2, hidden), n3 on page 6 (narrowed 3), n4 on `en`. The read sends n1, n3 at narrowed page 3's slot, and n4, not n2, under the stored document's `ETag`.
  - Writing that body back with a new n5 on narrowed page 2 stores n1, n2 unchanged, n3 at its full slot, n4, and n5 at full page 5's slot.
  - A write giving n2's id to a note on a visible page is refused, and nothing is stored.
  - A position on full page 3 reads as narrowed page 1's first slot. Kids only: a position on full page 0 reads as slot 0.
  - A position written at narrowed page 2's slot is stored at full page 5's, and reads back at narrowed page 2's.
  - The two-library fixture (`router_over_a_work_whose_editions_sit_in_two_libraries`): the note on the hidden edition is not sent, and a write of what was sent keeps it.
- [ ] **Step 3.** Run them and watch them fail.
- [ ] **Step 4.** Implement.
- [ ] **Step 5: Watched mutations.** Send notes untranslated. Store only the incoming notes (n2 lost). The notes `ETag` over the sent body. The fallback to the next visible page. Store a written position untranslated.
- [ ] **Step 6.** Crate run, clippy, fmt. Commit: "A reader's position and notes on a narrowed edition are translated, and a notes write keeps the notes it cannot see".

### Task 16: Truth and corrections refuse a narrowed edition, and every route declares what it does with one

**Files:**
- `crates/taiyaku-serve/src/routes.rs`: `get_truth` (~1401), `put_truth` (~1460-1551), `patch_alignments` (~1846) and `visible_editions_for_a_correction` (~691); after this task nothing calls `visible_editions_or_refuse` or `sidecar_as_seen` that `View` should serve.
- `routes.rs`: a new `#[cfg(test)] fn narrowing_postures()` beside `grant_postures` (~2381), and its test beside `every_route_declares_its_grant_posture` (`auth/tests.rs` ~1645).
- Tests: `routes/tests.rs`, `auth/tests.rs`.

**The rules.**
- **Truth read** leaves out every edition the account cannot see whole, hidden or narrowed (O191's truth half). The `ETag` names the account's numbering (spec §3.8, decision 11), by the same function Task 15's fix gave notes: for an account with grant limits, over the stored document and its view; for `Access::Every`, the stored document's as before. The write's `If-Match` and its answer's tag use it too.
- **Truth write:** the body's entry for an edition the account cannot see whole is kept only if it equals the stored entry; otherwise a narrowed edition is refused with 403, "`{edition}` of `{work}` is only partly visible to this account, so its ground truth cannot be written here", and a hidden one in the words an edition the work does not have is refused in. The stored entries of every such edition the body leaves out are kept.
- **An alignment correction** naming a narrowed edition is refused with 403, "`{edition}` of `{work}` is only partly visible to this account, so its alignments cannot be corrected here".
- **`narrowing_postures()`** lists every route of `guarded_routes`, in its order, as one of: `Translates` (a route that takes a page index), `Narrows` (it sends a document, list or count built from the view), `KeepsUnseen` (a whole-document write that keeps what the account cannot see), `Refuses`, or `NoPages { reason }`. The test asserts the route set equals `guarded_routes`, and drives each route that is not `NoPages` as Manga only over the Task 12 fixture, with O60's walk filling the path: `Translates` answers narrowed page 2 as the administrator answers page 5; `Narrows` answers with neither part2's `sha256` nor its file name in the body; `KeepsUnseen` sends nothing of part2's pages, and a write of what it sent leaves the store unchanged; `Refuses` answers 403 naming `raw`.

- [ ] **Step 1.** Baseline: `taiyaku-serve`.
- [ ] **Step 2: Write the failing tests** (the Task 12 fixture; truth for `raw` and `en` seeded through the store):
  - Manga only reads truth for `en` alone, under its numbering's `ETag` (the administrator's tag is unchanged from before this task). A write read before Manga is granted Kids, sent after, is refused with 412 and stores nothing. Writing it back with `en` changed stores the change and keeps `raw`'s entry. A body with a changed `raw` entry is refused with the sentence above and nothing is stored; one with `raw`'s stored entry unchanged is accepted.
  - The two-library fixture: the truth read leaves out the hidden edition, and a write of what was read keeps it.
  - Correcting `raw`↔`en` as Manga only is refused with the sentence above; `en`↔`fr` is not.
  - The postures test.
- [ ] **Step 3.** Run them and watch them fail.
- [ ] **Step 4.** Implement.
- [ ] **Step 5: Watched mutations.** The truth read sends narrowed editions. The truth write stores the body alone (`raw` lost). A changed `raw` entry accepted. The correction not refused. Drop one route from `narrowing_postures`. Stop translating in `get_page` and watch the postures test fail too.
- [ ] **Step 6.** Crate run, clippy, fmt. Commit: "Ground truth and alignment corrections refuse a narrowed edition, and every route says what it does with one".

### Task 17: The administrators' list of split editions

**Files:**
- `crates/taiyaku-library/src/libraries.rs`: `pub fn split_editions(conn: &Connection) -> Result<Vec<SplitEdition>, String>`, built on part 1's SQL (`survey-narrowing.md` §6); `SplitEdition` and its parts carry the answer's fields below, and serialise to it.
- `crates/taiyaku-serve/src/libraries_api.rs`: the route, modelled on `list_libraries` (~120) with `administering_or_forbidden` (`session.rs` ~858). Register `GET /api/library/split-editions` in `guarded_routes` (`routes.rs` ~2065); `GrantIndependent` in `grant_postures`, which moves the pinned 37 to 38; `NoPages` in `narrowing_postures`.
- Tests: `crates/taiyaku-library/src/libraries/tests.rs`, `crates/taiyaku-serve/src/libraries_api/tests.rs` (`a_reader_is_refused`, ~148).

**The answer** (it first exists here):
```json
{"editions": [{
  "work": "foo", "title": "Foo", "edition": "raw", "pages": 9,
  "members": [{"path": "Foo v01 part1.cbz", "pages": 2,
               "directories": [{"root": 3, "path": "/m", "library": 1}]}],
  "libraries": [{"id": 1, "name": "Manga", "members": 2, "pages": 6}]
}]}
```
Editions by work title, then edition key; members in ordinal order; directories by path; libraries by name, each library whose directories hold one of the edition's members. A member's `path` is as `member.path` stores it.

- [ ] **Step 1.** Baseline: `taiyaku-library`, `taiyaku-serve`.
- [ ] **Step 2: Write the failing tests.**
  - The Task 12 fixture, as the administrator: one entry, `raw`, three members in order with their directories, Kids (1 member, 3 pages) and Manga (2, 6); `en` and `fr` are not listed.
  - A reader is refused 403.
  - `router_over_a_copy_granted_through_its_second_directory` lists nothing: a byte-identical copy is not a split.
  - A store with no split answers `{"editions": []}`.
- [ ] **Step 3.** Run them and watch them fail.
- [ ] **Step 4.** Implement.
- [ ] **Step 5: Watched mutations.** Drop the `NOT EXISTS` (every edition a library holds is listed). Count every member's pages for each library. Remove the guard.
- [ ] **Step 6.** Crate runs, clippy, fmt. Commit: "Administrators can list the editions split between libraries".

### Task 18: The Split editions tab

**Files:**
- Create `web/src/splitEditions.ts` (the answer's types and `listSplitEditions` over `getJson`, as `backup.ts` ~36) and `web/src/SplitEditions.tsx` (modelled on `Backup.tsx`).
- `web/src/pageReads.ts`: a `SPLIT_EDITIONS` read. Each time the tab opens it asks the server again: read how `usePage` and `read()` behave on remount, and if they answer from the cache without asking, invalidate `SPLIT_EDITIONS` wherever the libraries list is invalidated after a scan, an add, a move or a removal.
- `web/src/config.ts`: `{ path: 'split-editions', label: 'Split editions' }`, last in `TABS` (~55-71). `web/src/Config.tsx`: its `SCREENS` entry (~34-39).
- Tests: create `web/tests/SplitEditions.test.tsx` (jsdom, mocking `../src/splitEditions`, as `Backup.test.tsx` does); `web/tests/classNames.test.ts` where a class needs an entry.

**The words** (they first exist here):
- Header "Split editions". Under it: "An edition is split when its files sit in directories of more than one library. An account granted only some of those libraries sees only their files, with the pages renumbered. An account granted every library listed sees the whole edition, and an administrator always does."
- Each row: the work's title and the edition key; under it one line per library, "{name} sees {m} of {n} files, {p} of {total} pages", or "{name} sees every file" where `m` is `n`; then the files, each with its page count and the directories that hold it, each a link to `/config/libraries/{library}/dirs/{root}` (confirm the address from `Libraries.tsx` ~580).
- Under the list: "To split one, give one of its directories its own edition key (in the directory's settings, or with a rule), then scan that directory."
- Empty: "No edition is split between libraries."
- A refusal is shown as the other tabs show one (`Config.tsx` ~11-15).

- [ ] **Step 1.** Baseline: the web's jsdom and node projects.
- [ ] **Step 2: Write the failing tests.** Assert per element, never a whole-page `toContain`:
  - Two editions: each row's library lines, in order, with their counts; a library holding every file reads "sees every file".
  - Each directory link's `href`.
  - The empty state.
  - A 403 answer is shown as the other tabs show it.
  - Configuration lists the tab last, at `/config/split-editions`.
  - Opening the tab, leaving it and opening it again asks the server twice.
- [ ] **Step 3.** Run them and watch them fail.
- [ ] **Step 4.** Implement.
- [ ] **Step 5: Watched mutations.** Swap `m` and `n`. Link a directory under the wrong library. Show the empty state with cases present. Answer the second opening from the cache.
- [ ] **Step 6.** Web projects (jsdom, node, browser). Commit: "Configuration lists the editions split between libraries".

### Task 19: Docs, open items, and the drive

**Files:** `docs/status.md`, `README.md`, `docs/notes.md`, and the overlapping-libraries spec (the backup wording in §3.1 that the ledger's Minor list names).

- [ ] **Step 1: Open items**, with the sweep before and after (Task 10 Step 4's commands).
  - **O239:** close it by decision 6: the edition still merges, and an account sees the members its grants reach, with the Split editions tab listing each case. Name the tests of Tasks 12–17 and the drive below.
  - **O191:** close it by Tasks 15–16. Name their tests.
  - Recount from the table; grep each O-number touched across `docs/`, `crates/`, `web/src/`, `web/tests/` and `scripts/`, and read every hit.
- [ ] **Step 2: `docs/status.md`'s libraries summary:** a merged edition is narrowed for an account that sees part of it, and the Split editions tab lists each case. Name spec §3.8.
- [ ] **Step 3: README:** the same, where it describes what an account sees, and the tab where it lists Configuration's tabs.
- [ ] **Step 4: The drive**, on Task 10b's scratch store (`.superpowers/sdd/overlap-evidence/pr1/drive/`), under `task-10b-brief.md`'s isolation rules, which bind unchanged: the marker and digests before and after, archives hardlinked and nothing else, no stamp, servers through `run_in_background`, killed by PID, the port re-checked after every kill. A release build of this commit, copied into the drive directory. If the store has no edition split between libraries, build one as Task 10b's O239 confirmation did.
  - As the administrator: `/api/library/split-editions` lists the case, and the tab shows it (a screenshot in the drive directory).
  - As an account granted one library: the work document, every page of the narrowed edition (each page's bytes equal the administrator's page at the mapped index), a position written and read back, and a note written and read back. The store's position and note are in the full numbering.
  - Write the results in `docs/notes.md`, under a section the status summary points at.
- [ ] **Step 5.** Commit: "Overlapping libraries PR 1: a merged edition is narrowed, O191 and O239 closed".

### Finish

After Task 19: the whole-branch review (the most capable model) with the ledger's Minor list; one fix wave; then Task 10's Steps 6–8 on the final commit (web counts, the gate with the two Rust figures written from its JUnit report, bookworm); the claim check; the PR body.
