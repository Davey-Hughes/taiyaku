# A paused directory is frozen until Scan (O320, O270) — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Until Scan is pressed, no derive hashes a paused directory's files or changes its works, and no stamp leaves a derive an old digest to write back.

**Architecture:** A stamp writes each new digest onto its archive's `scan_entry` row, found by path (O270). Migration 027 gives `scan_entry` a held reading that the walk fills, once, when it finds an entry changed. `derive_watched` takes the roots its caller reads fresh. Every other paused root is frozen: it contributes its chapter-only entries, the entries whose digest a work of that root holds, and the entries whose held reading one holds, and nothing else. A chapter-only entry inside an entry that waits waits with it. `digest_held_members` passes a frozen root over, so the folder-member refusal also reads a held digest. A derive that is not frozen for an entry's directory drops the entry's held reading when it writes the work that holds the entry.

**Tech Stack:** Rust; rusqlite against the system SQLite (3.53 here, 3.40.1 in CI and the image); axum and tokio in `taiyaku-serve`. No web change.

**Spec:** docs/superpowers/specs/2026-11-16-paused-directory-frozen-design.md

## Global Constraints

- **Where.** Worktree `W=/home/davey/projects/taiyaku/.claude/worktrees/paused-frozen`, branch `paused-frozen`, based on main `b1c842e0`. Never `cd` into the main checkout or another worktree; use absolute paths, `env -C $W` or `git -C $W`. Line numbers below are hints at `8a9186be`; find code by symbol.
- **Evidence.** `E=/home/davey/projects/taiyaku/.superpowers/sdd/paused-frozen-evidence`, set in every Bash call that uses it (gitignored in the main checkout). Logs and reports go there by Bash (`cat >| $E/T<n>-report.md <<'EOF'`); the Write tool is refused under `.superpowers/`.
- **Build** into `CARGO_TARGET_DIR=$W/target` only, never a target shared with another worktree.
- **Shell** is zsh with noclobber: overwrite with `>|`, start a log with `>|` or `tee`, never `>>` first. `grep -a` on every log. Never wait in a loop on `grep -q`.
- **Rust per task:** for each crate the task touches, `env -C $W cargo test -p <crate> -- --test-threads=8 >| $E/T<n>-<crate>.log 2>&1`, then on its own line `echo "exit $?" >| $E/T<n>-<crate>.exit`, then `grep -a 'test result:' $E/T<n>-<crate>.log`. Run whole crates; a named test only to watch one failure. Then `cargo fmt --all -- --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`, and `node scripts/check-comments.mjs`, each from `W`.
- **Comments** say why: the reason, the invariant, what breaks if it changes. No history and no measurements; an O-number pointer is fine. A new block holds at most 8 lines. If the comment guard reports a raised budget, report it; never raise a budget.
- **Attribution:** none, anywhere.
- **Watched mutations.** Every new test is watched failing against the defect its step names: put the defect in by hand, see the test fail on its assertion (not on a setup `expect`), restore, check `git -C $W diff`. A test with several assertions is watched once per assertion with a defect the earlier ones tolerate. Record defect, test and failure message in the task report. If no named defect makes a test fail, report it; do not weaken the test or drop the step.
- **Fixtures assert themselves.** Each test asserts its fixture's precondition (`"the fixture: …"`) before acting, as the neighbours do, so a fixture that changed under it fails loudly rather than passing.
- **Pre-existing failures** count only if the same command fails at main `b1c842e0` (Task 0's baseline is that code).
- **Commits:** one per task, never `--amend`, no push. Before committing, check the previous task's hash is an ancestor of `HEAD`. Read `git -C $W show --stat HEAD` after each commit.
- **Pinned names** (the plan is where they first exist): migration `027_held_reading.sql`; columns `scan_entry.held_sha256`, `held_format`, `held_pages`, `held_title`; `derive::derive_watched(conn, documents, fresh: &[i64], watch)`; `derive::frozen_entries`; `derive::drop_held`; `Candidate::frozen`; `CopyAt::rel_path` and `CopyAt::frozen`; test helpers `scan::tests::held_reading` and `derive::tests::standing`.
- **Not in this plan** (spec §4): freezing removals, any change to who sets or clears a pause, O318, O314, words on a page, web code.

## Decisions this plan makes

The spec leaves these to the builder (§10) or reads two ways; each is decided here so no task has to.

1. **The held reading is four columns**: `held_sha256`, `held_format`, `held_pages`, `held_title`. `derive::Entry` reads `sha256`, `format`, `pages`, `title` and `size`; `size` is read only to size a hash, and a held reading is never hashed. The derive reads no `kind`.
2. **The four move together.** The spec's "`COALESCE(held_sha256, sha256)` and the same for the others", read column by column, can pair a digest with an earlier walk's page count: a file changed twice before it was first hashed leaves `held_pages` set while `held_sha256` is still NULL. So the walk copies all four only where `held_sha256 IS NULL` and the row has a digest, and otherwise keeps all four (Task 2's SQL).
3. **The drop is per written work, not one statement per root.** A cancel between two works must leave the readings of the work not yet written and drop those of the work written, or a second cancelled rescan freezes the written work on a reading it no longer holds. `drop_held` runs inside `write_work`'s first scope, after `write_roots`. A work `unchanged` leaves standing drops nothing, so a derive over unchanged files still takes no write lock (O272); its entries' held readings are then the digests its works hold, so keeping them changes nothing.
4. **The stamp writes the digest, not the size and mtime.** The next walk of the stamped file's directory then holds the new digest and the derive hashes the file once more and finds its work unchanged. Writing size and mtime would need `scan::mtime_secs` and a stat taken after the rename, to save that one hash.
5. **Fresh roots are root ids.** A pressed job's are its `asked` directories (its walk already ignores the pause), a scheduled batch's none, the CLI's the directories it selected, and every library caller's (`derive`, `derive_with`, removals, preview, save) none.
6. **The held set** is one query per frozen root over `member_root` (the primary's directory and every copy's): `SELECT DISTINCT m.sha256 FROM member m JOIN member_root mr ON mr.member_id = m.id WHERE mr.root_id = ?1`, served by `member_root_by_root`.
7. **The refused unpause in §9 is made with a trigger**, not a write lock held past the busy timeout. Both of the job's unpauses must be refused and its walk must then write; a held lock would have to be let go between them by timing. A trigger refuses every unpause and nothing else, at once.
8. **§9's "Frozen equals unchanged" variant is paired with a different defect.** The spec pairs the hashed variant with "the walk overwrites the held reading", which one walk cannot show. The variant is paired with "the frozen input takes a current digest no work holds over the held reading" (the spec's second anti-goal), and the walk-overwrite defect is left to "A held reading survives a second cancelled scan", which has the second walk.

## Folded in as defects the freeze creates (the controller's call, 2026-11-16, not an owner ruling), both in Task 3

- **D1. The member refusal reads a held digest too.** `scan::reads_as_one_volume` refuses a directory added inside a folder member by finding the member's entry by digest (O91). `digest_held_members` is what gave such an entry its digest back, and it now passes over a paused root. So the refusal's join matches the entry's `held_sha256` as well as its `sha256`, or a paused folder volume whose walk cleared its digest would let a directory be added inside it until Scan.
- **D2. A chapter-only entry inside a waiting entry waits with it.** A waiting entry is one the walk-saved view says speaks for its subtree (`claims_of` over the root's rows as `entries` gives them) and that the freeze leaves out: a new folder volume, say. Its chapter subdirectories would otherwise sit under no claim in the frozen input, count as unattached and name a series no work holds. A chapter-only entry under no waiting entry passes as §7 says.
  - `proposals::raise` needs no change. It reads `derive::unattached_chapter_dirs`, which goes through `entries`, where the waiting volume still claims its chapters, so it raises nothing for them already. §7 keeps proposals reading the directory as the walk saved it, and this keeps that.

## Review Focus

1. **Another directory changes a paused directory's work** (a file of the same volume added there): the work is written with the paused files' held readings, and they stay held until Scan. Task 4 (e).
2. **A cancel between two works of a pressed Scan's derive, then another cancelled rescan:** the written work's reading is dropped and the unwritten one's kept, so no freeze later reads a reading no work holds. Task 4 (c).
3. **A pressed Scan whose unpause the store refuses** (a rules save holding the write lock): the job still reads its directories fresh, and they stay paused. Task 5 (b).
4. **A file changed twice, the first time before it was ever hashed:** its held reading is its digest's own, never an earlier walk's page count. Task 2 (e).
5. **A paused directory's rows handed to another directory** (a nested directory removed): the held reading goes with the row. Task 2 (f).

---

### Task 0: Setup and baseline

- [ ] **Step 1:** `mkdir -p $E`. `git -C /home/davey/projects/taiyaku check-ignore -v .superpowers/sdd/paused-frozen-evidence/x` prints the rule. `git -C $W log -1 --format=%h` is `8a9186be` and `git -C $W status --porcelain` is empty.
- [ ] **Step 2:** For Task 7's gate, copy (`cp -a`, never a symlink) `/home/davey/projects/taiyaku/web/node_modules` to `$W/web/` and `/home/davey/projects/taiyaku/crates/taiyaku-wasm/pkg` to `$W/crates/taiyaku-wasm/`.
- [ ] **Step 3:** Baseline `taiyaku-library`, `taiyaku-serve` and `taiyaku-cli` by the Rust-per-task commands, as `T0-<crate>`. Expected: each exit 0, no `FAILED`. A failure is re-run once; one that persists is recorded by name as pre-existing and reported before Task 1.
- [ ] **Step 4:** `$E/T0-report.md`: the three `test result:` lines.

---

### Task 1: The stamp writes its digest to the archive's row, by path (O270)

Spec: §7 "The stamp (O270), first"; §9's first two lines.

**Files:**
- Modify `crates/taiyaku-library/src/stamp.rs`:
  - `record_rows` (391-431): per written member, **before** its `UPDATE member SET sha256`, set its archive's `scan_entry.sha256` to the new digest.
  - `rekey_anchors`' doc (481-489): drop the sentence that a derive before the next scan puts the old digest back.
- Modify `crates/taiyaku-serve/src/works.rs`: `past_stamps`' doc (391-398). An `at` that still resolves stays for a member that shows a stamped-away digest again, as a file put back from before the stamp does once walked; not for O270's derive.
- Modify `crates/taiyaku-serve/src/routes/tests.rs`:
  - `stamp_part3` (~10443) also sets part3's `scan_entry.sha256`, as `record_rows` now does.
  - The docs of `an_untouched_note_after_a_stamp_and_a_renumbering_saves` (~10629) and `a_digest_the_member_still_shows_is_not_forwarded` (~10728) stop citing O270 as a derive that puts the digest back. The second keeps its body: the state it sets up is still reachable.
- Test `crates/taiyaku-library/src/stamp/tests.rs`, beside `two_copies` (2427) and its tests.

**Interfaces (Produces):** the row is found by the member's `root_id` and its `path` relative to that root's `path`, cut as `browse::held_by` cuts it (`substr` after `root.path || '/'`), **never by digest**: two copies under one directory share one. A member with no root, or a path outside its root, writes nothing. Task 2 adds a held-reading clause to this same statement and reads the member's digest from before the stamp, which is why the statement runs before the member's own update.

- [ ] **Step 1: Failing tests.**
  - (a) `a_derive_before_the_next_walk_keeps_a_stamps_digest_and_the_rows_written_meanwhile`. `one_work()`; `apply` its plan; `derive`; `new = digest_of(&fixture.archive("Series - v01.cbz"))`.
    ```rust
    assert_eq!(member_digests(&conn, work_id), vec![("raw".to_string(), new.clone())], "after the derive");
    ```
    Then, in the window, `put_position(&conn, work_id, "raw", 1)` and `hold_notes(&conn, work_id, vec![a_note("n1", "raw", 1)])`. Then the next walk and derive: `scan_root` (the root from `scan::roots`), `derive`.
    ```rust
    assert_eq!(
        (position_anchors(&conn, work_id), notes_on(&conn, work_id)),
        (vec![("raw".to_string(), new)], vec![on("n1", "raw", 1, false)]),
        "the rows written in the window stay on the new digest"
    );
    ```
  - (b) `a_stamp_writes_its_digest_to_the_row_at_the_stamped_path_alone`. `two_copies()`; `old` = the fixture's one digest; `record_digests(&conn, work_id, &[(copies.c001, "1".repeat(64))])`. With a new helper `entry_digests(conn)` (`SELECT rel_path, sha256 FROM scan_entry WHERE rel_path LIKE '%.cbz' ORDER BY rel_path`):
    ```rust
    assert_eq!(entry_digests(&conn), vec![
        ("Series/Series - v01 - c001.cbz".to_string(), Some("1".repeat(64))),
        ("Series/Series - v01 - c002.cbz".to_string(), Some(old)),
    ]);
    ```
- [ ] **Step 2: Watch both fail at the base** (`-p taiyaku-library stamp::tests::<name>`). (a) fails on its first assertion with the old digest back on the member: that is O270, watched at the base. (b) fails with c001's row on the old digest. Record both messages.
- [ ] **Step 3: Implement**, with the doc and test edits above. Run `taiyaku-library` and `taiyaku-serve` whole. If any `routes` test changes outcome after `stamp_part3`'s edit, stop and report rather than adjust it.
- [ ] **Step 4: Mutations.**
  - The row found by `sha256 = <the member's old digest>` instead of by path: (b) fails.
  - The statement removed: (a) fails.
- [ ] **Step 5:** Task checks. Commit `A stamp writes each new digest to its archive's directory row, so no derive puts the old one back (O270)`. Report.

---

### Task 2: The held reading — migration 027, the walk, the hand-back, the stamp

Spec: §7 "The held reading (schema and walk)", "The stamp (O270)" (the held-reading sentence); §9's last line.

**Files:**
- Create `crates/taiyaku-library/src/schema/027_held_reading.sql`:
  ```sql
  -- The reading the walk replaced when it found an entry changed, kept until a
  -- derive not frozen for the entry's directory writes the work that holds it
  -- (`derive::drop_held`). A paused directory's derive reads an entry by it, so
  -- a changed file keeps the reading its works were written from until Scan is
  -- pressed (O320). The four move together, so a digest is never paired with
  -- another reading's pages. NULL where nothing is held: every existing entry.
  ALTER TABLE scan_entry ADD COLUMN held_sha256 TEXT;
  ALTER TABLE scan_entry ADD COLUMN held_format TEXT;
  ALTER TABLE scan_entry ADD COLUMN held_pages INTEGER;
  ALTER TABLE scan_entry ADD COLUMN held_title TEXT;
  ```
- Modify `crates/taiyaku-library/src/schema.rs`: `SCHEMA_VERSION` 27, `MIGRATIONS` (53-80) grows to 27.
- Modify `crates/taiyaku-library/src/schema/tests.rs`:
  - `the_migration_gives_every_directory_a_paused_at_of_null` (3313) migrates with `migrate_with(&conn, &MIGRATIONS[..26])` and asserts version 26.
  - `the_schema_version_is_twenty_six` (1468) becomes `the_schema_version_is_twenty_seven`, asserting 27.
- Modify `crates/taiyaku-library/src/scan.rs`:
  - `record`'s `(true, Some(resolved))` arm (3352-3371) adds, in the same `UPDATE` that sets `sha256 = NULL`:
    ```sql
    held_sha256 = COALESCE(held_sha256, sha256),
    held_format = CASE WHEN held_sha256 IS NULL AND sha256 IS NOT NULL THEN format ELSE held_format END,
    held_pages  = CASE WHEN held_sha256 IS NULL AND sha256 IS NOT NULL THEN pages  ELSE held_pages  END,
    held_title  = CASE WHEN held_sha256 IS NULL AND sha256 IS NOT NULL THEN title  ELSE held_title  END,
    ```
    Every right-hand side reads the row as it was before the statement, so these see the digest it clears. The `(true, None)` arm is untouched.
  - `copy_entry` (1628-1647) copies the four columns with the rest.
- Modify `crates/taiyaku-library/src/stamp.rs`, `record_rows`' `scan_entry` statement from Task 1: `held_sha256 = CASE WHEN held_sha256 = <the member's digest before this stamp> THEN <new> ELSE held_sha256 END`. The other three stay: a stamp keeps the format (zip), the pages (`ComicInfo.xml` is no page) and the title (a zip declares none).
- Tests: `schema/tests.rs`, `scan/tests.rs`, `stamp/tests.rs`.

**Interfaces (Produces):** the four columns. `pub(crate) fn held_reading(conn: &Connection, root_id: i64, rel_path: &str) -> (Option<String>, Option<String>, Option<i64>, Option<String>)` in `scan/tests.rs` beside `write_archive`: the row's `held_sha256, held_format, held_pages, held_title`. Tasks 3 to 5 use it.

- [ ] **Step 1: Failing tests.** In `scan/tests.rs` unless said. "Rewrite" means `write_archive` again at the same path with the page count given, which changes size and bytes.
  - (a) `schema/tests.rs` `migration_27_holds_no_reading_for_an_entry_already_scanned`, shaped like `migration_24_…` (3240). Migrate to 26. Seed root 7, an entry `a.cbz` (`zip`, 3 pages, `sha256` `'d'` × 64) and `b.cbz` (`zip`, 2 pages, no digest). `migrate`. Version 27; `SELECT rel_path, sha256, pages, held_sha256, held_format, held_pages, held_title … ORDER BY rel_path` gives each row its digest and pages as seeded and four `Null`s.
  - (b) `a_changed_entry_holds_the_reading_its_digest_was_taken_from`. `Alpha/Alpha v01.cbz` at 3 pages; add, scan, `derive`; `d` its digest. Rewrite at 4; scan.
    ```rust
    assert_eq!(held_reading(&conn, root.id, REL), (Some(d), Some("zip".into()), Some(3), None));
    ```
    The row's own `(sha256, pages)` is `(None, Some(4))`.
  - (c) `a_held_reading_survives_a_second_change`. As (b), then `UPDATE scan_entry SET sha256 = <'e' × 64>` on that row, the digest a cancelled derive leaves. Rewrite at 5; scan. `held_reading` is still `(Some(d), Some("zip"), Some(3), None)`.
  - (d) `an_unchanged_entry_keeps_its_held_reading`. As (b), then scan again with nothing changed. `held_reading` is unchanged.
  - (e) `a_reading_is_held_with_its_own_page_count`. A new `Alpha/Alpha v01.cbz` at 3 pages; add and scan, no derive. Rewrite at 4; scan.
    - `held_reading` is `(None, None, None, None)`.
    - `UPDATE … SET sha256 = <'e' × 64>`, the digest taken at 4 pages. Rewrite at 5; scan.
    - `held_reading` is `(Some('e' × 64), Some("zip"), Some(4), None)`.
  - (f) `a_hand_back_carries_an_entrys_held_reading`, beside `removing_a_nested_directory_hands_its_files_to_the_directory_around_it` (9757), over `handed_over`. Set Kids' `Bleach/v01.cbz` to `('h' × 64, 'zip', 7, 'held title')` by SQL. `hand_back` in a `Nested` scope. Then:
    ```rust
    assert_eq!(held_reading(&tx, h.manga.id, "kids/Bleach/v01.cbz"),
               (Some("h".repeat(64)), Some("zip".into()), Some(7), Some("held title".into())));
    ```
  - (g) `stamp/tests.rs` `a_stamp_moves_a_held_reading_only_where_it_is_the_digest_replaced`. `two_copies()`; `old` = their digest. By SQL, c001's row gets `held_sha256 = old` and c002's `held_sha256 = '7' × 64`. `record_digests` with c001 → `'1' × 64` and c002 → `'2' × 64`.
    ```rust
    assert_eq!((held_of(c001), held_of(c002)), (Some("1".repeat(64)), Some("7".repeat(64))));
    ```
    `held_of` is `held_reading(...).0`.
- [ ] **Step 2:** Run: none compiles, then each fails on its assertion.
- [ ] **Step 3: Implement.** Run `taiyaku-library`, `taiyaku-serve` and `taiyaku-cli` whole.
- [ ] **Step 4: Mutations.**
  - The four `held_*` assignments removed from the walk: (b) fails.
  - `held_sha256 = sha256`, and the three `CASE`s without `held_sha256 IS NULL`: (c) fails.
  - The three `CASE`s as plain `COALESCE(held_x, x)`: (e) fails on its last assertion, after passing the first.
  - `held_sha256 = NULL` (and the three) added to the `(true, None)` arm: (d) fails.
  - `copy_entry` without the four columns: (f) fails.
  - The stamp's held clause removed: (g) fails on c001.
  - The stamp's held clause made unconditional: (g) fails on c002.
  - The migration's `held_pages` column renamed: (a) and (b) fail.
- [ ] **Step 5:** Task checks. Commit `Keep the reading the walk replaced on each changed entry (migration 27)`. Report.

---

### Task 3: The frozen input, and who reads fresh

Spec: §7 "The frozen input (derive)", "Who is frozen"; §8 calls 2, 4, 5, 7; §9's lines on another library's scan, frozen equals unchanged, the second cancelled scan, a cancel while writing, chapter-only entries, `digest_held_members`, and a stamped file in a paused directory. D1 and D2 above.

**Files:**
- Modify `crates/taiyaku-library/src/scan.rs`: `reads_as_one_volume` (843-852) joins `member m ON m.sha256 IN (e.sha256, e.held_sha256)` (D1). Its doc says why: a paused directory's entry keeps its digest only as the held reading.
- Modify `crates/taiyaku-library/src/derive.rs`:
  - `derive_watched` (628) takes `fresh`. It reads the frozen set once, `crate::scan::paused(conn)?` less `fresh`, on `conn`, so a caller's transaction sees its own pauses.
  - In the roots loop (664), a frozen root's entries come from `frozen_entries`, every other root's from `entries` as now. `entries` (1137) itself is unchanged: the proposals read it (3376-3500).
  - `digest_held_members` (fn 3082, called at 904) passes over frozen roots.
  - `derive_with` (607) passes `&[]`.
  - Doc comments say why: a frozen root is read as its works last held it, and a fresh root never is.
- Modify `crates/taiyaku-serve/src/scan_jobs.rs`:
  - `run_watched` (452): its `derive` parameter takes the fresh ids. It hands over `asked`'s ids where `reread.pressed()` and none for a batch; the call is ~586.
  - `run` (404-420) passes them to `derive_watched`.
  - `run_with` (425-447) ignores them.
- Modify `crates/taiyaku-cli/src/library.rs` (~459): the derive is `derive_watched(&conn, Documents::Reconcile, &<selected ids>, &Unwatched)`. Its `Cancelled`, which `Unwatched` never reaches, is an `Err` worded as `derive_with`'s.
- Update every other call to `derive_watched` and every `run_watched` derive closure (the compiler lists them; all of them, at `8a9186be`):
  - `derive/tests.rs` 9755, 9757, 9815, 9847, 9875, 9920, 9966, 9994: add `&[]`.
  - `scan_jobs/tests.rs` closures at 656, 844, 895, 1066, 1098, 1175, 1398.
  - `scheduler/tests.rs` 364 and 553; `config_api/tests.rs` 5019.
  - A closure that calls `derive_watched` passes the ids it is handed; one that calls `derive::derive` ignores them.
- Unchanged, and must stay so: every `derive::derive` and `derive_with` caller. That is the tests, `taiyaku-serve`'s `test_support`, `corpus.rs`, `scan::apply_removal` (1988), `scan::apply_library_removal` (2207), `scan::remove_root`'s second derive (2106) and `preview` (488, 645).
- Tests: `derive/tests.rs`; `stamp/tests.rs`.

**Interfaces:**
- Consumes: Task 1's stamp; Task 2's columns and `held_reading`.
- Produces:
  - `pub fn derive_watched(conn: &Connection, documents: Documents, fresh: &[i64], watch: &dyn Watch) -> Result<Outcome<DeriveReport>, String>`.
  - The frozen set as a `HashSet<i64>` local to `derive_watched`; Task 4 hands it to `classify`.
  - `fn frozen_entries(conn: &Connection, engine: &Engine, root: &Root) -> Result<Vec<Entry>, String>`. It makes one query for the root's held set (Decision 6) and one for its rows with the held columns, in path order. A row comes back:
    1. as it is, where `engine.resolve(root, rel_path)` makes it chapter-only (`volume` none and `chapter` some, `hashes`' own test);
    2. else as it is, where its `sha256` is in the held set;
    3. else with `held_sha256/format/pages/title` as its `sha256/format/pages/title`, where `held_sha256` is in the held set.

    Every other row is left out. Then (D2) a row kept by arm 1 is left out too where it lies strictly beneath a waiting entry. The waiting entries are the `claims_of` the root's rows as `entries` gives them (admitted with a throwaway `DeriveReport`, as `unattached_chapter_dirs` does) that arms 1 to 3 left out. "Beneath" is `nested_in_claim` over that set.
  - `run_watched`'s `derive: impl FnOnce(&rusqlite::Connection, &[i64], &dyn Watch) -> Result<Outcome<derive::DeriveReport>, String>`.

- [ ] **Step 1: Helpers** in `derive/tests.rs`:
  - `standing(conn) -> Standing`: three vectors, each sorted. Works as `(id, content_key)`. Members as `(work_id, edition key, ordinal, path, sha256, pages)`. `member_root` rows as `(member path, root_id)`.
  - `two_file_volume(dir)`: writes `Alpha/Alpha - v01 - c001.cbz` (2 pages) and `Alpha/Alpha - v01 - c002.cbz` (3 pages), one work of two members.
  - "Pause R" is `scan::pause(&conn, &[r.clone()])`. "A derive elsewhere" is `derive::derive(&conn)` with R paused.
- [ ] **Step 2: Failing tests** (`derive/tests.rs` unless said):
  - (a) `a_derive_of_another_directory_hashes_none_of_a_paused_one`, shaped like `the_bytes_to_hash_leave_out_what_classify_does_not_hash` (9785).
    - R is `three_undigested_archives`, `scanned_root`. Derive it watched with `Recording::cancelling(|(step, done, _)| step == Step::Hashing && done > 0)`, which leaves 1 digest; pause R.
    - S holds `Delta/Delta v01.cbz` (5 pages), `scanned_root`; `s` its size. Derive watched with `fresh = &[]` and `Recording::default()`.
    ```rust
    assert_eq!(reports_at(&watch, Step::Hashing),
               vec![(Step::Hashing, 0, None), (Step::Hashing, 0, Some(s)), (Step::Hashing, s, Some(s))]);
    assert_eq!((digests_under(r), member_roots_under(r)), (1, 0), "R's digest stays and none of its works is written");
    ```
    Defect: the frozen set empty (the pause ignored).
  - (b) `a_paused_directorys_works_stand_as_they_were_after_a_cancelled_rescan`, run for `hashed in [false, true]`.
    - `let (s_dir, r_dir) = crate::tests::a_copy_of("Foo v01.cbz")`; `two_file_volume(r_dir)`.
    - `scanned_root` S **first**, so S's copy is the primary and R's the copy; then R. `derive`; `before = standing(&conn)`.
    - Rewrite R's c002 at 4 pages and R's `Foo/Foo v01.cbz` at 5; scan R. Fixture: `held_reading(R, c002).0` is c002's old digest.
    - If `hashed`, derive watched with `Recording::cancelling(|(step, _, _)| step == Step::Writing)`, which hashes both files and stops before the first work.
    - Pause R; a derive elsewhere.
    ```rust
    assert_eq!(standing(&conn), before, "hashed: {hashed}");
    ```
    Defects:
    - `frozen_entries` without arm 3 (skip without the held reading): c002's member drops, both cases.
    - `frozen_entries` taking any non-NULL `sha256` before the held set: the `hashed` case fails.
    - The held set read from `member.root_id` alone: R's `member_root` row drops, both cases.
  - (c) `a_held_reading_survives_a_second_cancelled_scan`. R by `two_file_volume`; `derive`; `before = standing`.
    - The first cancelled scan: rewrite c002 at 4; scan R; derive watched, cancelled at `Step::Writing`, which hashes c002; pause R.
    - Scan pressed and cancelled after its walk: `scan::unpause`; rewrite c002 at 5; scan R; no derive; pause R.
    - A derive elsewhere. `standing` equals `before`, and c002's member holds c002's first digest.

    Defect: Task 2's walk writing `held_sha256 = sha256`; c002's member drops.
  - (d) `a_cancel_while_writing_leaves_exactly_the_works_written`. R is `three_undigested_archives`, `scanned_root`. Derive watched, cancelled at `step == Step::Writing && done > 0`, which writes Alpha's work. Pause R; a derive elsewhere.
    ```rust
    assert_eq!((count("work"), member_paths()), (1, vec![alpha_c001, alpha_c002]));
    ```
    R still has 3 digests. Defect: the pause ignored (2 works).
  - (e) `a_paused_directory_keeps_its_chapter_only_entries`. R holds `Kaguya/Kaguya - c001.cbz` (2 pages) and `Alpha/Alpha v01.cbz` (3); `scanned_root`. `first = derive`; pause R; `second = derive`.
    ```rust
    assert_eq!((second.series, second.unattached_chapter_dirs), (2, 1));
    assert_eq!((second.series, second.unattached_chapter_dirs), (first.series, first.unattached_chapter_dirs));
    ```
    Defect: `frozen_entries` without arm 1, which gives `(1, 0)`.
  - (f) `digest_held_members_passes_over_a_paused_directory`. R holds `Alpha/Alpha v01.cbz` (3); `scanned_root`; `derive`. Rewrite at 4; scan R; pause R; a derive elsewhere. The entry's `sha256` is `None`. Defect: `digest_held_members` over every root, which hashes it.
  - (g) `a_fresh_directory_is_read_as_the_walk_saved_it_though_paused`. R is `three_undigested_archives`, `scanned_root`; pause R.
    - Derive watched with `fresh = &[]`; count works.
    - Derive watched with `fresh = &[r.id]`; count works.
    - The two counts are `(0, 2)`.

    Defect: `fresh` not subtracted, which gives `(0, 0)`.
  - (h) `stamp/tests.rs` `a_stamped_file_in_a_paused_directory_keeps_its_member`.
    - A fixture beside `two_copies`: `Series/Series - v01 - c001.cbz` (2 pages) and `c002.cbz` (3 pages), one work of two members (asserted).
    - `record_digests` with c001 → `'1' × 64`; pause the root; derive.
    - `member_digests` is `[("raw", '1' × 64), ("raw", <c002's digest>)]`.

    Defect: Task 1's `scan_entry` write removed; c001's member drops.
  - (i) `scan/tests.rs` `a_dir_added_inside_a_paused_folder_volume_is_refused`, beside `a_dir_inside_a_directory_member_of_any_root_is_refused` (1554) and built as it is.
    - R is `add_root_checked_in(&conn, &r, InLibrary::Named("Manga"))`, holding the folder `S/S - v01` (`000.jpg`, `001.jpg`); scan; derive; `d` the member's digest.
    - The rescan: write `S/S - v01/extra/000.jpg`, then set the folder's mtime apart from the recorded one with `std::fs::FileTimes`, as `an_unchanged_container_is_not_reopened` (~632) sets one. Scan R.
    - The cancel: pause R; a derive elsewhere.
    - Fixture: the folder's row `(sha256, held_sha256)` is `(None, Some(d))`.
    ```rust
    assert_eq!(
        add_root_checked_in(&conn, &extra, InLibrary::Named("Kids")).map(|a| a.root),
        Err(AddRootError::Unusable(format!(
            "{} is inside {}, which the library `Manga` reads as the pages of one volume",
            extra.display(), member.display()))),
        "refused as when the folder held its digest"
    );
    ```
    Defect: `e.held_sha256` taken out of the join. The add is then accepted and answers `Ok` with a new root for `extra`, the volume's pages split between two directories.
  - (j) `a_paused_directorys_waiting_folder_volume_keeps_its_chapters_with_it`, shaped like `the_chapters_inside_a_volume_are_not_an_unattached_directory` (~2783).
    - R holds `Alpha/Alpha v01.cbz` (3 pages); `scanned_root`; derive.
    - Then the new folder volume: `write_pages` into `Series/Series - v01/c001`, `c002` and `c003` (2 pages each), where each `cNNN` resolves chapter-only and the volume claims them. Scan R; pause R. `report` is a derive elsewhere.
    ```rust
    assert_eq!(
        (report.unattached_chapter_dirs, report.series, count("series"), count("proposal")),
        (0, 1, 1, 0),
        "the volume's chapters wait with it: none counted, no series named for them, nothing raised"
    );
    ```
    Defect: D2's containment step removed, giving `(1, 2, 2, 0)`. The proposal count is 0 either way, because `raise` reads `entries`, where the volume still claims its chapters. That assertion guards against a fix that routes `raise` through the freeze.
    - Task 3 (e) stays green with the step in, since Kaguya's chapter lies under no waiting entry.
- [ ] **Step 3:** Run: each fails on its assertion (after compiling against the new signature).
- [ ] **Step 4: Implement**, callers included. Run all three crates whole.
- [ ] **Step 5: Mutations:** each test's named defect, by hand; record each. Then `run_watched` handing `&[]` to a pressed job compiles and passes everything here. That is expected: Task 5 (b) is its test.
- [ ] **Step 6:** Task checks. Commit `A paused directory is read as its works last held it, by every derive but one that reads it fresh (O320)`. Report.

---

### Task 4: The held reading is dropped when its work is written

Spec: §7 "The held reading" (its last paragraph); §9's lines on a pressed Scan, a cancelled derive, and a file gone in a paused directory.

**Files:**
- Modify `crates/taiyaku-library/src/derive.rs`:
  - `Candidate` (1048) gains `frozen: bool`.
  - `CopyAt` (1095) gains `rel_path: String` and `frozen: bool`.
  - `classify` (1400) sets `frozen` from Task 3's frozen set, handed in through `Sorting` (1129). `place` (~1577) copies `rel_path` and `frozen` onto each `CopyAt`.
  - New `drop_held`, called in `write_work` right after `write_roots` (2164), in the same scope and before any rekey commit. `unchanged`'s early return (~2084) drops nothing; its comment says why (Decision 3).
- Tests: `derive/tests.rs`.

**Interfaces (Produces):** `fn drop_held(conn: &Connection, group: &Group) -> Result<(), String>`. For each candidate and each copy whose `frozen` is false, it runs `UPDATE scan_entry SET held_sha256 = NULL, held_format = NULL, held_pages = NULL, held_title = NULL WHERE root_id = ?1 AND rel_path = ?2 AND held_sha256 IS NOT NULL` through `prepare_cached`.

- [ ] **Step 1: Failing tests:**
  - (a) `a_pressed_scan_drops_the_held_readings_of_what_it_writes`. R by `two_file_volume`; `derive`; `a2` c002's digest. Rewrite c002 at 4; scan R; pause R. Derive watched with `fresh = &[r.id]`.
    ```rust
    assert_eq!(held_reading(&conn, r.id, C002), (None, None, None, None));
    ```
    c002's member and entry hold one digest, and it is not `a2`. Defect: `drop_held` not called.
  - (b) `a_derive_cancelled_before_writing_keeps_the_held_reading`. R by `two_file_volume`; `derive`. Rewrite c002; scan R. Derive watched, cancelled at `Step::Writing`. `held_reading(c002).0` is `Some(a2)`. Defect: the drop moved to `classify`, as each entry of a root that is not frozen is staged.
  - (c) `a_cancel_between_works_drops_only_the_written_works_readings`. R by `two_file_volume` plus `Beta/Beta v01.cbz` (4); `derive`; `b1` Beta's digest. Rewrite c002 at 5 and Beta at 6; scan R. Derive watched, cancelled at `step == Step::Writing && done > 0`, which writes Alpha and not Beta.
    ```rust
    assert_eq!((held_of(C002), held_of(BETA)), (None, Some(b1)));
    ```
    Defects:
    - One statement per root after the write loop: `(Some(a2), Some(b1))`.
    - The drop at `classify`: `(None, None)`.
  - (d) `writing_a_copy_drops_its_entrys_held_reading`. `a_copy_of("Foo v01.cbz")`; `scanned_root` S then R; `derive`. Rewrite both files with identical new bytes, `write_archive_labelled(path, 4, "jpg", "same")`; scan both; `derive`.
    ```rust
    assert_eq!((held_of(s, FOO), held_of(r, FOO), crate::tests::member_root_ids(&conn)), (None, None, vec![s.id, r.id]));
    ```
    Defect: `drop_held` over candidates alone, not copies; R's stays.
  - (e) `a_paused_directorys_files_keep_their_held_reading_when_another_directory_changes_their_work`.
    - R by `two_file_volume`; `derive` (`a1`, `a2`). Rewrite c002 at 4; scan R; pause R.
    - S holds `Alpha/Alpha - v01 - c003.cbz` (4 pages); `scanned_root`; a derive elsewhere. S's file joins R's work.
    ```rust
    assert_eq!(members_by_ordinal(work), vec![(r_c001, a1), (r_c002, a2), (s_c003, c3)]);
    assert_eq!(held_of(r, C002), Some(a2));
    ```
    Defects:
    - `drop_held` ignoring `frozen`: the second assertion fails.
    - Task 3's frozen set empty: the first fails, with c002's new digest.
  - (f) `a_file_gone_from_a_paused_directory_is_marked_missing_and_returns_with_its_work_position_and_note`, shaped like `a_volume_whose_file_goes_is_marked_missing_and_returns_with_its_work_position_and_note` (9045).
    - Its fixture, position and note on volume 2. Remove volume 2's file; scan; pause the root; a derive elsewhere. `(marked(1), marked(2))` is `(false, true)`.
    - Write the bytes back; Scan pressed: `scan::unpause`, scan, derive watched with `fresh = &[root.id]`. The marks are `(false, false)`, the same work, its position `[("raw", 2)]`, its note `["n1"]`.

    Defect: the tail's `mark_missing` skipped while any root is frozen.
- [ ] **Step 2:** Run: each fails on its assertion.
- [ ] **Step 3: Implement.** Run `taiyaku-library` and `taiyaku-serve` whole.
- [ ] **Step 4: Mutations:** each named defect, by hand. Also `held_sha256 IS NOT NULL` removed from `drop_held`'s statement. That one is expected to pass, since it only changes which rows are written with NULL; record it as equivalent if it does.
- [ ] **Step 5:** Task checks. Commit `A derive drops an entry's held reading when it writes the work that holds it`. Report.

---

### Task 5: Every caller freezes, and a pressed Scan reads fresh

Spec: §7 "Who is frozen"; §9's lines on another library's scan through `scan_jobs`, rules preview and save, the scheduled batch and a removal, and the refused unpause. No production code is expected to change. If a test cannot be made to fail by its named defect, stop and report.

**Files (tests only):** `crates/taiyaku-serve/src/scan_jobs/tests.rs`, `crates/taiyaku-serve/src/scheduler/tests.rs`, `crates/taiyaku-cli/tests/cli.rs`, `crates/taiyaku-library/src/preview/tests.rs`, `crates/taiyaku-library/src/scan/tests.rs`.

**Interfaces (Consumes):** Task 3's `run_watched` derive closure and `derive_watched`; `taiyaku_library::test_support::Recording`.

- [ ] **Step 1: Tests:**
  - (a) `scan_jobs` `a_cancelled_first_import_is_hashed_by_no_other_librarys_scan_and_whole_by_its_own`, beside `a_cancelled_first_scan_reads_never_cancelled_and_paused_with_its_digests_kept` (1299).
    - `two_dirs`; add `Alpha/Alpha - v02` (3 pages) to `a` with `test_support::write_pages`.
    - `cancelled_after_one_digest(&library, &[a.clone()], Reread::Dir)`. Fixture: a has 1 digest, 0 works and is paused.
    - b's library's scan: `super::run_watched` over `[b]`, `Reread::Library(b.libraries[0])`, `scan::scan_root_watched`, and a derive closure `|conn, fresh, _| derive::derive_watched(conn, derive::Documents::Reconcile, fresh, &watch)` with `watch = Recording::default()`. `b_bytes` is the sum of the sizes of the two page files under `b/Beta/Beta - v01`.
    ```rust
    assert_eq!(reports_at(&watch, Step::Hashing).last(), Some(&(Step::Hashing, b_bytes, Some(b_bytes))));
    assert_eq!((digests_in(&a), member_roots_in(&a)), (1, 0));
    ```
    - Then a pressed Scan of a through `super::run(&library, &[a.clone()], Reread::Dir, …)`: `(member_roots_in(&a), paused(&conn, &a))` is `(2, false)`.

    Defect: the pause ignored in `derive_watched`.
  - (b) `scan_jobs` `a_pressed_scan_whose_unpause_is_refused_still_reads_its_directory_fresh`.
    - `two_dirs`; add `Alpha - v02` to `a` as in (a); `cancelled_after_one_digest` over `[a]`.
    - Then refuse every unpause:
      ```sql
      CREATE TRIGGER refuse_unpause BEFORE UPDATE OF paused_at ON root
      WHEN NEW.paused_at IS NULL
      BEGIN SELECT RAISE(ABORT, 'the store refuses the write, as one a rules save holds'); END;
      ```
    - Run `super::run(&library, &[a.clone()], Reread::Dir, &crate::LibraryJob::default(), &super::Running::new(&job, None))`; its one slot has no `error`.
    ```rust
    assert_eq!((paused(&conn, &a), member_roots_in(&a), digests_in(&a)), (true, 2, 2),
               "(still paused, so the unpause was refused; all of a written and hashed)");
    ```
    Defects:
    - `run_watched` handing a pressed job no ids.
    - `run` passing `&[]` instead of its ids.
  - (c) `scheduler` `a_batch_hashes_none_of_a_directory_paused_after_its_tick_read_the_schedule`, shaped like `a_batch_does_not_walk_a_directory_paused_after_its_tick_read_the_schedule`.
    - `state_with_two_scanned_roots`; both due; the tick takes `[a, b]`.
    - Write `Gamma/Gamma - v01` (2 pages) under `a`; `scan::scan_root` of `a` (walk only); pause `a`.
    - `scan_jobs::run_watched` over `taken`, `Reread::Batch`, its derive closure passing the ids it is handed to `derive_watched`.
    - a's `Gamma/Gamma - v01` entry has no digest and no member has that path.

    Defects:
    - `run_watched` handing a batch `asked`'s ids.
    - The pause ignored.
  - (d) `cli.rs` `library_scan_of_another_directory_hashes_none_of_a_paused_one`.
    - `add_dir` `Comics` and `Volumes`. `cbz_pages` writes `Volumes/Series/Series v01.cbz` and `Comics/Other/Other v01.cbz` (3 pages, distinct seeds).
    - `library scan --root <Volumes> --no-derive`; `pause(&db, &[&volumes])`; `library scan --root <Comics>`.
    - By `rusqlite` on `db`: no `scan_entry` under Volumes has a digest, and `SELECT count(*) FROM work` is 1.

    Defect: the pause ignored.
  - (e) `cli.rs` `library_scan_reads_its_directory_fresh_though_paused_while_it_waits_for_the_lease`, shaped like `library_scan_waits_for_another_derive_and_says_so` (404).
    - `add_dir` Volumes with `Series/Series v01.cbz`. Hold the derive lease from another connection.
    - Spawn `library scan --root <Volumes>`; read its stderr until the waiting line; `pause(&db, &[&volumes])`; drop the lease; wait.
    - `(paused_at(&db, &volumes).is_some(), <works>)` is `(true, 1)`.

    Defect: the CLI passing `&[]` as fresh.
  - (f) `preview/tests.rs` `a_preview_hashes_none_of_a_paused_directory`. A helper `a_paused_rescan(label) -> (TempDir, crate::Library, scan::Root, (String, String))`:
    - `Series/Series - v01 - c001.cbz` (2) and `c002.cbz` (3), added, scanned and derived; `(a1, a2)` their digests.
    - c002 rewritten at 4 and scanned; the root paused.

    Here, also delete c002 from disk, so any attempt to hash it finds it gone.
    ```rust
    assert_eq!(preview(&conn, &crate::rules::read(&conn).unwrap().0).unwrap(),
               Impact { series_before: 1, series_after: 1, ..Impact::default() });
    ```
    Defect: the pause ignored (`works_orphaned: 1`).
  - (g) `preview/tests.rs` `a_save_regroups_a_paused_directorys_files_by_their_held_readings`. `a_paused_rescan`, c002 left on disk. `crate::preview::apply` with the rules plus `crate::tests::a_rule_setting_series("Moved")`.
    - The members as `(path, sha256)` by path are `[(c001, a1), (c002, a2)]`.
    - `grouping(&conn)` places both under `"Moved"`.

    Defects:
    - The pause ignored: c002 holds its new digest.
    - `frozen_entries` without arm 3: c002 is absent.
  - (h) `scan/tests.rs` `removing_another_directory_hashes_none_of_a_paused_one`.
    - T holds `Beta/Beta v01.cbz`; added, scanned, derived.
    - Then R holds `Alpha/Alpha - v01 - c001.cbz` and `c002.cbz`; added and scanned, no derive; paused.
    - `remove_root(&conn, t.id)`. R has 0 digests and `SELECT count(*) FROM work` is 0.

    Defect: the pause ignored (1 work, 2 digests).
- [ ] **Step 2:** Run each crate whole; every new test passes.
- [ ] **Step 3: Mutations:** each named defect by hand, in the production code it names; each test fails. Rebuild `target/debug/taiyaku` (`cargo build -p taiyaku-cli --bin taiyaku`) after restoring a CLI mutation.
- [ ] **Step 4:** Task checks. Commit `Pin that every caller's derive freezes a paused directory and a pressed Scan reads its own fresh`. Report.

---

### Task 6: docs/status.md and the README

**Files:** `docs/status.md`; `README.md` (~329-333).

- [ ] **Step 1: Sweep on the way in**, by the commands in memory `taiyaku-open-items-ritual`: rows, closed rows, the count words, the `What remains:` line and the line after it. The highest O-number is O323 at `8a9186be`, so new rows start at O324.
- [ ] **Step 2: Close O270 and O320** in five parts each:
  1. The checkbox.
  2. The row's text opens `**Closed <date> by the paused-directory freeze: …**` with what was done and the tests that pin it, then `*As filed:*` and the text as it was, as O323's row does.
  3. The prose bullet, rewritten to "Closed.": O270's ~4587, O320's ~5338.
  4. The Rust test count at ~183, measured. Run `env -C $W -u TAIYAKU_CORPUS cargo nextest run --workspace --all-features >| $E/T6-nextest.log 2>&1`, then `node scripts/status-test-count.mjs target/nextest/default/junit.xml docs/status.md` from `W`, and write the figure it names. The web figures are unchanged (no web change).
  5. "Two hundred and eight are now closed" (append O270 and O320 to its list in order) and "What remains: the one hundred and eight".

  Each "Where" names symbols that still exist.
- [ ] **Step 3: Open what the build found**, from O324. Each new item gets a row, a prose bullet, the head count word ("Three hundred and sixteen") and the What-remains count. D1 and D2 are built, not rows.
  - O91's row (open) says `derive::digest_held_members` hashes an entry "which the refusal of a directory added inside a folder member reads by digest". Add that while the entry's directory is paused, the refusal reads its held digest (`scan::reads_as_one_volume`), since `digest_held_members` passes a paused directory over. Narrowing nothing else, its checkbox stays.
- [ ] **Step 4: The branch's paragraph**, after the first scan's (~5236): the paused-directory freeze, its spec, what it closed and opened; "their bullets follow" where it opened any.
- [ ] **Step 5:** `grep -n 'O270\|O320'` across `docs/` and `web/src/`, and read every hit. Narrative sections are kept as written and marked (`*O270 has since closed — see its entry above.*`); closed rows (O240's "O270's window") are left as written. Every row has exactly six `|`.
- [ ] **Step 6: README** (~329-333): a pause also keeps every other scan, rules preview or save and removal from hashing the directory's files or changing its works until Scan is pressed, and nothing it held is lost meanwhile.
- [ ] **Step 7:** The sweep again, on the way out. Commit `Close O270 and O320 in the status, and say what a pause keeps`. Report.

---

### Task 7: Finish — gate, SQLite 3.40.1, mutation sweep

- [ ] **Step 1: Gate**, at `/proc/loadavg` < 24, in the background:
  - `env -C $W -u TAIYAKU_CORPUS NEXTEST_TEST_THREADS=8 RUST_TEST_THREADS=8 scripts/check-all.sh >| $E/gate.log 2>&1`, then on its own line `echo "exit $?" >| $E/gate.exit`.
  - Read `gate.exit`; quote the `all green` line from `grep -a 'all green' $E/gate.log`.
  - Never pipe the gate, and never run a copy of the script. A red first failure is the report.
  - A browser-project failure with no failed assertion: read memory `taiyaku-browser-gate-chromium-crash` before suspecting the branch.
- [ ] **Step 2: SQLite 3.40.1.** Run `-p taiyaku-library -p taiyaku-serve -p taiyaku-cli` in `rust:1-bookworm` by the recipe in `/home/davey/.config/.claude/projects/-home-davey-projects-taiyaku/memory/taiyaku-sqlite-340-in-ci.md`, with the host toolchain mounted read-only as its note says. Only that memory's three `strace` tests may fail; anything else is real.
- [ ] **Step 3: Mutation sweep.** `env -C $W scripts/mutants.sh --since main >| $E/mutants.log 2>&1` in the background, after every code commit.
  - `grep -a -n 'ERROR\|Worker thread failed' $E/mutants.log` must print nothing; if it does, the verdict is void: remove `$W/target/mutants.out` (literal path, its own call) and re-run.
  - Read `target/mutants.out/outcomes.json` and `unviable.txt` against what changed: `derive_watched`, `frozen_entries`, `drop_held`, `digest_held_members`, `classify`, `place`, `write_work`, `record_rows`, `record`, `copy_entry`, `reads_as_one_volume`, `run_watched`, the CLI's `scan`.
  - A survivor is first hand-applied in place and run across all three crates (O87). Then it is killed by a test, or reported.
- [ ] **Step 4: Hand mutations on the final head** that cargo-mutants cannot make (SQL text and field initialisers). Each named test fails; restore after each:
  - the walk's four `held_*` assignments → Task 2 (b);
  - `copy_entry`'s held columns → Task 2 (f);
  - the stamp finding its row by digest → Task 1 (b);
  - `frozen_entries`' arm 3 → Task 3 (b);
  - `reads_as_one_volume`'s `e.held_sha256` → Task 3 (i);
  - `frozen_entries`' containment step → Task 3 (j);
  - `CopyAt::frozen` initialised `false` → Task 4 (e);
  - `run`'s fresh ids → Task 5 (b).
- [ ] **Step 5:** Add a row to `docs/mutants-baseline.md`'s run log ("When it was last run"), as recent `--since main` rows do: date, scope and mutant count, the caught/unviable/missed/timeout counts, and what the unviable ones were. Commit `Record the paused-directory freeze's mutation sweep`. Report with the gate's `all green` line, the bookworm summary and the sweep's counts. The claim check and the PR are the controller's.
