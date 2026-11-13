# Lane 4: missing works and O184 (pre-deploy) — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Part D of the deployment spec, as the pre-deploy spec changes it, in two PRs.
- PR 1 (Part 1): a work none of whose members any directory still holds is marked missing by every derive, hidden from every listing a reader browses and from `/api/works` and OPDS, answered by the reader with a reason, and brought back with its id, positions and notes when a scan finds a member again. Closes O48; narrows O91.
- PR 2 (Part 2): an administrator lists the missing works (Configuration → Missing works, `GET /api/library/missing`, `taiyaku library missing`) and removes one or all, counted first. A live registry never mints a `bridge:` row (O184). Closes O91 and O184. *O184 was closed by go-public lane 3 (`2026-11-12-go-public-lane-3-second-accounts.md`); O91 is still open, and PR 2 closes it alone.*

**Architecture:**
- Part 1: migration 020 (provisional) adds `work.missing_since`; `derive::mark_missing` marks and clears after the fold loop (Task 1) and is reached by every way a member's files go (Task 2); `browse`'s listings take `missing_since IS NULL` (Task 3); the registry's `Work` carries the mark, `/api/works` and OPDS skip it, `get_sidecar` answers it 404 (Task 4); the reader says so and the scan report shows both counts (Task 5); the closes (Task 6); the Finish.
- Part 2: the list and the removal in `scan.rs` (Task 1); the routes, under the job lock (Task 2); the CLI (Task 3); O184 (Task 4); the Missing works tab (Tasks 5 and 6); the closes (Task 7); the Finish.

**Tech Stack:** Rust (`taiyaku-library`, `taiyaku-serve`, `taiyaku-cli`), SQLite, TypeScript/React (`web/`), vitest projects `node`, `jsdom` and `browser`.

**Spec:** `docs/superpowers/specs/2026-11-10-pre-deploy-design.md` §1 and §5 (cited "spec §5", "decision n"), which builds Part D (§5) of `docs/superpowers/specs/2026-10-04-day-to-day-deployment-design.md` (cited "Part D"). Pre-flight: `.superpowers/sdd/pre-deploy-evidence/daily-preflight.md` §2. The code was read for this plan at main 10ae1e74. Locations are by symbol; line numbers are hints at 10ae1e74.

**Rulings** (binding; do not re-open; `.superpowers/sdd/pre-deploy-evidence/rulings.md`):
- Two PRs: mark, hide and bring back; then list and remove (ruled 2026-11-10).
- O184 is folded in, in the second PR (ruled 2026-11-10).
- A removal of missing works takes the library job lock and answers 409 in O245's words (told to the owner 2026-11-10, no objection).
- Every decision in spec §5 is final, the spec's own calls included.

**What the plan settles** (the plan writer's calls, from the code at 10ae1e74; the controller tells the owner the starred ones):
- **Where the pass runs.** Digests are written in `derive::classify` through `derive::digest`, in the first loop over the roots, before `place`, the write loop and the fold loop. The pass runs right after the fold loop and before `series_holding_no_work` is counted, under both `Documents` modes (spec §5 "The mark").
- **The clear is the mark's condition negated.** One SQL fragment says "missing": the work has a member, every member has a `member_root` row, and no member's `sha256` is held by a `scan_entry` in a directory its `member_root` rows name. The mark sets it where it holds and the mark is NULL; the clear empties it where it does not hold and the mark is set. So a marked work that comes to have a member under no directory (O249's leftover) is cleared and shown, as an imported work is.
- **\* An index in the same migration**, `scan_entry (root_id, sha256)`. `scan_entry` has none on `sha256` today (`schema/003_scanner.sql`), so without it every member's probe reads every entry of its directory. Task 1 records `EXPLAIN QUERY PLAN` with and without it, as O62's close measured its join with a negative control.
- **How a live registry learns of a mark.** The pass's `UPDATE`s fire `structure_work_upd`, so the refresher reads the works again. A request that hits a held work between the derive and the next refresh still sees the old flag, which is Part A's window.
- **`Work` carries the mark in an `AtomicBool`**, set on every refresh for a held `Work` as for a new one, so a held work keeps its notes and correction locks (`Work::reproject`'s reason) across a mark.
- **The browse predicate goes in each caller's SQL**, never inside `works_of`, so each listing's predicate can be removed alone and fail its own test. The enumeration is in Part 1 Task 3.
- **Admin counts are not listings.** `libraries::list`'s `works`, `libraries::unfiled_works` and `libraries::split_editions` keep counting missing works; Part 2's tab names them.
- **\* The reader's 404 is JSON**, `{"error": <sentence>, "missing": true}`, and comes after `view::of_work`'s grant check, so a work the grant hides still answers "no work" as today. The web tells it from "no work" by `missing`.
- **`/api/works` and OPDS skip a missing work for every access** (spec §5). Under a grant two filters then stand (the skip and `visible_editions`' predicate), so the grant-case tests are watched against both removed together; each filter alone is caught by its own test (Spec gaps, 2).
- **Names.** `DeriveReport::marked_missing` and `DeriveReport::found_again` (and the same JSON keys); the scan report's labels "Works marked missing" and "Missing works found again".
- **\* A removal of one work whose slug names no work marked missing** (a scan found its files since the list was read, or no such work) removes nothing, derives nothing and answers 404. A removal of all with none marked removes nothing and still derives, so its answer has the same shape as any other.
- **The path last seen** is `hand::path_for`'s rule: the hub edition's member of lowest ordinal, its `member.path`. **The libraries that reach a work** are those whose reach (`reach::of_libraries`) holds a directory one of its members' `member_root` rows names, by name, ascending by library id. The list is ordered by title (`COLLATE NOCASE`), then slug.
- **The CLI's `--remove SLUG`** may be given more than once; each slug is its own removal, in the order given, and the command stops at the first slug naming no missing work.
- **\* O184 in `patch_alignments`**: the row is resolved before the file is written, so a 404 writes nothing. For a fixed registry the `bridge:` row is then minted before the file write rather than after it.
- **The tab is listed for every account**, as every other tab is (`Config.tsx`, O93): a non-administrator who reaches it sees the server's 403 sentence and no row (Spec gaps, 1).

**Spec gaps** (not resolved silently; the controller tells the owner):
1. Spec §5 Testing asks for the tab's "absence for an account that does not administer". `Config.tsx`'s doc rules out a client-side admin guard ("No admin guard here, deliberately", O93), and every tab is drawn for every account, a non-administrator seeing the server's own 403 sentence. This plan follows `Config.tsx`: the jsdom test asserts the 403 sentence and no row and no Remove button. The menu entry that leads to Configuration is already absent for a non-administrator (`Shell.tsx`'s `admin`).
2. Spec §5 Testing asks each listing's test to fail "on that listing's predicate removed, one at a time", the `/api/works` and OPDS tests "under `Access::Every` and under a grant". Under a grant both the registry skip and `browse::visible_editions`' predicate filter, so no single removal turns the grant case red. The plan watches the grant case against both removed, the Every case against the skip, and `visible_editions`' own browse test against its predicate.
3. `browse::unfiled` holds only imported works today (a derived work always gets a series: `write_work`'s `COALESCE(series_id_hand, ?1)`), and an imported work is never marked, so no derive reaches its predicate. It takes the predicate for the one-rule reason, and its test marks a work by hand.

**New interfaces this plan names** (the plan is where they are first written):
- Part 1:
  - `crates/taiyaku-library/src/schema/020_work_missing_since.sql` (provisional number), exactly as Part 1 Task 1 gives it.
  - `derive::mark_missing(conn: &Connection) -> Result<(usize, usize), String>`, `pub(crate)`: the works it marked, then the works it cleared.
  - `DeriveReport` gains `pub marked_missing: usize` and `pub found_again: usize`.
  - `registry::WorkRow` gains `pub(crate) missing: bool`. `Work` gains a private `missing: AtomicBool`, `pub fn is_missing(&self) -> bool` and `pub(crate) fn set_missing(&self, missing: bool)`; `Work::new` leaves it false.
  - Web: `api.ts` `export class WorkMissing extends Error { readonly work: string }`, not a subclass of `WorkNotFound`; `LoadFailure` gains `{ kind: 'missing'; work: string }`; `Derived` gains `marked_missing: number` and `found_again: number`.
- Part 2:
  - In `scan.rs`: `pub struct MissingWork { pub slug: String, pub title: String, pub libraries: Vec<String>, pub path: Option<String>, pub missing_since: String, pub positions: usize, pub notes: usize }`; `pub enum Missing<'a> { One(&'a str), All }`; `pub fn missing_works(conn) -> Result<Vec<MissingWork>, String>`; `pub fn missing_removal_cost(conn, which: Missing<'_>) -> Result<Option<(Removal, DeriveReport)>, String>`; `pub fn remove_missing(conn, which: Missing<'_>) -> Result<Option<(Removal, DeriveReport)>, String>`. `None` only for `Missing::One` naming no work marked missing, read inside the transaction. The `Removal` holds `works`, `positions` and `notes`; its other fields are their defaults.
  - Routes: `GET /api/library/missing` answers `{"works": [{"slug", "title", "libraries": [<name>], "path": <string | null>, "missing_since", "positions", "notes"}]}`. `DELETE /api/library/missing` and `DELETE /api/library/missing/{work}` take `?dry_run=true` (`config_api::DryRunQuery`) and answer `{"removed": {"works", "positions", "notes"}, "derive": <derive_report_json>, "dry_run": <bool>}`. Handlers `config_api::list_missing`, `config_api::delete_missing_all`, `config_api::delete_missing_one`. `AdminAnswer` gains `NotMissing(String)`, a 404.
  - O184, in `works.rs`: `pub(crate) enum Mint { Bridge, Never }` with `pub(crate) fn of(works: &crate::Works) -> Mint` (`Never` for a live registry), and `pub(crate) enum NoRow { Gone(String), Failed(String) }`. `row_id_for_write` and `row_id_for_document` take a `Mint` and answer `Result<i64, NoRow>`; `Gone` carries `` no work `{slug}` ``, the words `ServeState::find` refuses in. `progress::store`, `notes::store` and `truth::store` take a `Mint` and answer `Result<(), NoRow>`. `alignments::store(conn, work_id: i64, doc)` takes the row id its caller resolved.
  - Web: `web/src/missingWorks.ts` (client and types), `web/src/MissingWorks.tsx` (the tab), `web/src/RemoveMissing.tsx` (the dialog), `pageReads.ts`'s `MISSING_WORKS`.

**Sentences** (literal; tests assert them written out):
- Server, the reader's 404 (`error`): `` the files of work `{slug}` are missing from every directory that held them; it opens again once a scan finds them ``.
- Web, `ReaderFailed`'s alert for `missing`: "The files of <code>{work}</code> are missing from every directory that held them. It opens again once a scan finds them."
- Server, `AdminAnswer::NotMissing`: `` there is no missing work `{slug}`; a scan may have found its files since the list was read ``.
- Server, the lock's 409: `waits_for_the_job("remove the work")` for one, `waits_for_the_job("remove the missing works")` for all.
- CLI, the derive line's clauses, each only when non-zero: `, {n} work(s) marked missing` and `, {n} missing work(s) found again`.
- CLI, `library missing`: one line per work, `{slug}: {title}; missing since {missing_since}; in {libraries joined by ", ", or "no library"}; last at {path, or "no path"}; {positions} position(s) and {notes} note(s)`; with none, `no work is missing`. A removal prints `removed {slug}: {positions} position(s) and {notes} note(s)` (one) or `removed {works} missing work(s): {positions} position(s) and {notes} note(s)` (all), then the derive line `library scan` prints. A slug naming no missing work exits 1 with the `NotMissing` sentence.
- Web, the tab: title "Missing works"; description "A work is missing when a scan finds none of its files in any directory that held them. It is hidden from the library, the search and every OPDS feed until a scan finds its files again, and its reading positions and notes are kept. Removing it deletes the work with its positions and notes; no file on disk is changed."; empty "No work is missing."; a row's lines "In {names}" (or "In no library"), the path in a `<code>` (or "No path"), "Missing since {inUtc(missing_since)}", "{count(positions, 'reading position', 'reading positions')} and {count(notes, 'note', 'notes')}"; buttons "Remove…" (a row) and "Remove all…".
- Web, the dialog: counts "This removes {count(works, 'work', 'works')}, {count(positions, 'reading position', 'reading positions')} and {count(notes, 'note', 'notes')}."; paragraph "Nothing here can be undone from this screen. No file on disk is changed: a removed work's documents under .taiyaku/ stay where they are, and the report after the removal lists them as orphaned documents where their directory keeps documents."; confirm "Remove {title}" (one) or "Remove every missing work" (all); wait "Removing…"; after it, the page's line "Removed {count(works, 'work', 'works')}, {count(positions, 'reading position', 'reading positions')} and {count(notes, 'note', 'notes')}." with `DeriveSection` beneath.

## Global Constraints

- **Worktrees.** Five lanes run at once, each in its own git worktree. The controller gives each dispatch `W`, the worktree's absolute path. Part 1's branch is `pre-deploy-missing`, cut from main 10ae1e74; Part 2's is `pre-deploy-missing-remove`, cut from main after PR 1 merges, in a fresh worktree. Every command runs against `$W` by absolute path; never `cd` (it is zoxide here). Where a tool must run from a directory, write `builtin cd $W/... && ...`.
- **Evidence.** `E=/home/davey/projects/taiyaku/.superpowers/sdd/pre-deploy-evidence/lane-4-missing`, absolute and in the MAIN checkout, so reports survive the worktree. Logs and each task's report (`$E/P1-T<n>-report.md`, `$E/P2-T<n>-report.md`) go there, written by Bash (`cat >| … <<'EOF'`), never under `target/`. The Write tool is refused under `.superpowers/`.
- **Builds.** Never set a shared `CARGO_TARGET_DIR`: each worktree builds into its own `$W/target`. `$W/web/node_modules` is a real copy, never a symlink, and `crates/taiyaku-wasm/pkg` is copied in by the controller; never `pnpm exec`; call `./node_modules/.bin/<tool>` directly.
- **Comments.** A code comment says why: the reason, the invariant, what breaks if it changes. It never carries history or measurements, and it may point at an O-number. `scripts/check-comments.mjs` enforces part of this through the gate, including each area's share in `scripts/comment-budget.json`. If the gate fails on a share, follow `docs/ci.md`, "The comment guard", and say so in the report.
- **Attribution.** None anywhere: code, comments, docs, commits, PR body.
- **Gate.** The repo's own `scripts/check-all.sh`, never a copy. It runs from the repository it sits in (`cd "$(git rev-parse --show-toplevel)"`), so: `builtin cd $W && env -u TAIYAKU_CORPUS NEXTEST_TEST_THREADS=8 RUST_TEST_THREADS=8 ./scripts/check-all.sh >| $E/<name>.log 2>&1; echo "exit $?" >| $E/<name>.exit`. Wait for load < 24 (`cat /proc/loadavg`) and record the load. Read the `.exit` file and quote the `all green` line; never the harness's exit code, never the gate piped through `tail`.
- **Rust, per task.** `cargo nextest run --manifest-path $W/Cargo.toml -p <crate> --test-threads=8 >| $E/<task>-<crate>.log 2>&1`, for each crate the task touches; then `cargo clippy --manifest-path $W/Cargo.toml --workspace --all-targets --all-features -- -D warnings` and `cargo fmt --manifest-path $W/Cargo.toml --all -- --check`. Read the summary lines from the logs.
- **Web, per task.** `builtin cd $W/web && ./node_modules/.bin/vitest run --project <node|jsdom|browser> --reporter=default >| $E/<task>-<project>.log 2>&1`, whole projects, never a named file; then `./node_modules/.bin/tsc -b` and `./node_modules/.bin/biome check ./src ./tests` the same way. A task touching a `*.browser.test.tsx` file or CSS runs the whole browser project (CI never runs it). A browser failure with import or connection errors and no failed assertion is the headless Chromium crashing: check `coredumpctl list` and `/proc/loadavg`, then rerun the whole project.
- **Pre-existing failures.** A failure is pre-existing only if the same command fails at main 10ae1e74 (Part 1) or at the main Part 2 was cut from (Part 2), never at another commit of the branch. Report it; do not fix it.
- **`docs/status.md`** is edited only in each part's closes task (spec §1, coordination). Its `status-test-count` gate step checks the Rust total against the JUnit count, so the gate is red on the Rust total until then; the per-task checks are nextest, clippy and fmt, and the first gate runs in the closes task.
- **Existing tests that move.** A derive now marks every work whose members have `member_root` rows and no `scan_entry` of their digest: seeded fixtures (`derive/tests.rs`'s `seed_work`, which gives members root 1 and so `member_root` rows) are such works. A test whose assertion moves because of it is reported with why, never edited to the new number unread.
- **Watched mutations.** Every new or tightened assertion gets one: put the named defect back by hand, see the test fail, restore, and check with `git diff` that the restore is complete. Record each in the task report: the defect, the failing test, the assertion's message. A test that names a guard must reach it. A hand mutation's `cargo nextest` of `taiyaku-cli` rebuilds `target/debug/taiyaku`; rebuild after restoring before driving the binary.
- **Sentences are literals.** A test asserts a user-facing sentence written out in the test, never one computed through the helper under test.
- **Real interaction.** Web tests that navigate do it outside `act`, with real clicks; a real-key browser test focuses the page first.
- **Commits.** One commit per task, its subject naming the O-number(s). Never `--amend`. Parallel lanes share the machine: check each commit hash is an ancestor of `HEAD`.
- **Migrations.** Part 1's migration takes 020, the next free number at 10ae1e74. Lane 2 adds one too (provisionally 020). The second lane to merge renames its file, its `include_str!`, `SCHEMA_VERSION`, the `MIGRATIONS` length and the version test, and runs the schema tests again (spec §1). Lane 1 edits `schema.rs` elsewhere.
- **SQL on 3.40.1.** Each part adds SQL beyond a plain `SELECT`, so its closes task runs the library, serve and CLI crates' tests in `rust:1-bookworm` (SQLite 3.40.1), the recipe in Part 1 Task 6 Step 5.
- **Files other lanes edit** (conflicts are text only, in other functions; spec §1): `scan.rs` and `config_api.rs` and web `config.ts` (lane 3); `opds::works_feed`'s loop (lane 1's comment on the href, a few lines from this lane's skip); `schema.rs` and `schema/tests.rs` (lanes 1 and 2); `library_job.rs`'s `LibraryJob` doc, which gains a taker here; `README.md` (lanes 1, 3 and 5). No task adds a field to `ServeState`.

---

# Part 1 (PR 1): mark, hide, bring back

### Part 1, Task 1: The mark and its counts (O91, O48)

Spec §5 "The mark", decisions 2, 9 and 10; Part D §5.2 and §5.4. Pre-flight §2.4, §2.7 D-1.

**Files:**
- `crates/taiyaku-library/src/schema/020_work_missing_since.sql` (new) and `schema.rs`: `SCHEMA_VERSION`, `MIGRATIONS` (~27-50).
- `crates/taiyaku-library/src/schema/tests.rs`: `the_schema_version_is_nineteen` (~750), renamed for twenty; a migration test beside `migration_6_adds_member_root_id_leaving_existing_members_null` (~730), its model.
- `crates/taiyaku-library/src/derive.rs`: `DeriveReport` (~150), its two new fields with docs in the house style (why counted: a partial mount that hides hundreds of works says so); the new `mark_missing` beside `write_roots` (~2419); its call in `derive_with` right after the fold loop (`for (from_work, into_work) in superseded(...)`, ending ~767) and before `report.series_holding_no_work` is counted.
- `crates/taiyaku-serve/src/config_api.rs`: `derive_report_json` (~1429), which destructures with no `..`, gains the two keys.
- `crates/taiyaku-cli/src/library.rs`: `derive_summary` (~882) and its doc's list of conditional clauses.
- Tests: `crates/taiyaku-library/src/derive/tests.rs` (helpers `root_in` ~6363, `seed_work_without_root` ~2779, `scalar` ~5223; `write_archive` from `scan/tests.rs` ~49; `crate::state::put_progress` and `put_notes` for the reading); `crates/taiyaku-cli/src/library/tests.rs` beside `derive_summary_prints_unrecognised_kinds_when_nonzero_and_omits_them_when_zero` (~1536), with `derive_report()` (~1471) as the fixture.
- `docs/superpowers/plans/2026-11-10-pre-deploy-missing-works.md` (new): this plan, copied from `/home/davey/projects/taiyaku/.superpowers/sdd/pre-deploy-evidence/plans/2026-11-10-pre-deploy-missing-works.md`.

**The migration**, exactly:

```sql
-- A work none of whose members any directory still holds is marked rather
-- than deleted, and every listing hides it until a scan finds a member again
-- (pre-deploy spec §5). NULL is a present work; `derive::mark_missing` is the
-- one writer.
ALTER TABLE work ADD COLUMN missing_since TEXT;

-- `derive::mark_missing` asks of every member whether a directory it is under
-- holds an entry of its digest; `scan_entry_by_root` alone answers that by
-- reading every entry of the directory.
CREATE INDEX scan_entry_by_digest ON scan_entry (root_id, sha256);
```

**The pass.** One fragment over the `UPDATE`'s own `work` row, a `const` in `derive.rs`, holding three conditions: the work has a member; no member of it lacks a `member_root` row (imported, O54); and no member's `sha256` has a `scan_entry` in a directory its `member_root` rows name (the join `scan::repoint_kept_members` uses). Then:
- mark: `UPDATE work SET missing_since = datetime('now') WHERE missing_since IS NULL AND <fragment>`;
- clear: `UPDATE work SET missing_since = NULL WHERE missing_since IS NOT NULL AND NOT (<fragment>)`.

Each count is its `UPDATE`'s own `changes()` (`Connection::execute`'s answer); trigger writes are not counted in it. Both run under `Documents::Skip` as under `Reconcile`. The doc of `mark_missing` says why the `IS NULL` / `IS NOT NULL` guards matter: every `UPDATE` of `work` re-indexes its title (`work_au`) and bumps `structure_version` (`structure_work_upd`).

- [ ] **Step 1: Commit-ready plan.** Copy this plan to `$W/docs/superpowers/plans/2026-11-10-pre-deploy-missing-works.md`. It ships in this task's commit.
- [ ] **Step 2: The rejoin, before any change** (settles spec §5's "inferred; the first test settles it"). In `derive/tests.rs`, a real tree: a series of two volumes under one directory (`root_in`), derived, with a position and a note on volume 2's work. Remove volume 2's file, scan and derive; restore the same bytes at the same path, scan and derive. Assert volume 2's work has the same id and slug, its position (edition, slot) and its note. Run it on the unchanged code: it must PASS. **If it fails, stop and report**: the central claim then needs a design the spec does not have.
- [ ] **Step 3: The central claim, watched failing** (spec §5 Testing, "Part D's central claim"). The same shape, asserting in addition that between the scans volume 2's `missing_since` is set and volume 1's is NULL, and after the second scan both are NULL with id, position and note as in Step 2. Run it: it fails, at the mark, on the unchanged code (the column does not exist yet, or the mark stays NULL). Record the failure.
- [ ] **Step 4: The migration and the pass.** Add the file, `MIGRATIONS`, `SCHEMA_VERSION = 20`; `mark_missing`; its call; the two report fields; the JSON keys; the CLI clauses (Sentences above). Step 3 passes.
- [ ] **Step 5: The other tests.**
  - Migration: on a store migrated to 19 with a work row seeded (`migrate_with(&conn, &MIGRATIONS[..19])`, the file's pattern), migrating to 20 leaves `missing_since` NULL on it; the version test asserts 20. *Fails on:* a `DEFAULT` on the column.
  - Imported, and no member (decision 10): a work seeded by `seed_work_without_root`, and a work row with no edition, both stay NULL across `mark_missing`; a seeded work with a `member_root` row and no `scan_entry` beside them is marked, so the fixture tells right from wrong. *Fails on:* the `member_root` condition dropped; the has-a-member condition dropped.
  - Counts (decision 9): a real tree of three volumes; remove two files, scan, derive: `marked_missing == 2`, `found_again == 0`; restore one, scan, derive: `marked_missing == 0`, `found_again == 1`. *Fails on:* either count left at zero; the two swapped.
  - Quiet passes (spec §5 Testing), on the pass alone: a store holding one marked work and one imported work; read `crate::structure::version`, call `mark_missing`, read again: unchanged, and the answer is `(0, 0)`. *Fails on:* the mark's `missing_since IS NULL` guard dropped; the clear's `missing_since IS NOT NULL` guard dropped (the imported work then satisfies the negated fragment).
  - CLI: `derive_summary` prints each clause when non-zero and omits it at zero, as its neighbours' tests do. *Fails on:* a clause dropped; a clause printed at zero.
- [ ] **Step 6: The query plan.** On a store migrated to 20, record in the report `EXPLAIN QUERY PLAN` of both `UPDATE`s, and again with `scan_entry_by_digest` dropped (a scratch connection, not a test). With the index, the `scan_entry` probe reads `USING INDEX scan_entry_by_digest`; a plan that scans `scan_entry` per member with the index present is a finding. Run nothing into `target/`.
- [ ] **Step 7: Watched mutations**, each named in Step 5, and for Step 3: make the clear a no-op (return before it) and see the central test fail after the second scan.
- [ ] **Step 8: Run** nextest for `taiyaku-library`, `taiyaku-serve` and `taiyaku-cli`, clippy, fmt. Commit `Mark a work missing when none of its members is held, clear it when one returns, and count both (O91, O48)`. Write `$E/P1-T1-report.md`, with the existing tests that moved (Global Constraints).

---

### Part 1, Task 2: Every way a member's files go (O91, O48)

Spec §5 "The mark" (O243, the exclude pattern, both `Documents` modes) and Testing ("O243", "A partial mount", "An exclude pattern"). Part D §5.2.

**Files:**
- `crates/taiyaku-library/src/derive/tests.rs`: `removing_the_older_directory_of_a_split_volume_leaves_its_work_unfolded` (~7046) and `removing_the_directory_of_one_of_two_editions_keeps_its_stale_edition_beside_the_copys_work` (~7181).
- New tests in `derive/tests.rs` or `scan/tests.rs`, wherever the neighbouring real-tree tests are closer.
- `crates/taiyaku-library/src/scan.rs`: `ScanReport::forgotten_by_exclusion`'s doc (~2219), which says the works an exclusion covers "are left standing and stale"; they are now marked missing and hidden until the pattern is cleared.

- [ ] **Step 1: O243.** Both pins also assert every work they leave is unmarked (`missing_since IS NULL`). *Fails on:* presence read as "written this pass" (hand-apply: mark every work not in `written`, with `written` passed in).
- [ ] **Step 2: A partial mount** (O48's scenario). One directory holding two series of two volumes each; derive; move one series' directory out of the tree (rename it outside), scan, derive: that series' works are marked and the other's are not, and the scan was applied (`refused` is `None`); move it back, scan, derive: none is marked, ids unchanged. *Fails on:* the clear made a no-op.
- [ ] **Step 3: An exclude pattern** (O92). Derive; set the directory's `exclude_globs` to one series (`scan::update_root`, `RootPatch::exclude_globs`), scan, derive: that series' works are marked and the scan report's `forgotten_by_exclusion` counts their entries; clear the pattern, scan, derive: unmarked, same ids. *Fails on:* the clear made a no-op.
- [ ] **Step 4: Both `Documents` modes.** A work whose file was removed and the directory scanned with no derive after (`scan::scan_root` alone): `derive::derive_with(&conn, Documents::Skip)` marks it and reports `marked_missing == 1`. *Fails on:* the pass guarded by `documents == Documents::Reconcile`.
- [ ] **Step 5: The doc.** Rewrite `forgotten_by_exclusion`'s last paragraph to say the works are marked missing and hidden, and come back when the pattern is cleared. Grep `left standing and stale` and `left standing` across `crates/` for other comments that say an exclusion or a vanished file leaves works listed, and correct each.
- [ ] **Step 6: Watched mutations**, as named above. Run nextest for `taiyaku-library`, clippy, fmt. Commit `A partial mount, an exclude pattern and O243's unfolded works each meet the missing mark as the spec has it (O48, O91)`. Write `$E/P1-T2-report.md`.

---

### Part 1, Task 3: The hide, in every browse listing (O91)

Spec §5 "The hide", first three bullets; decision 3; Part D §5.2 ("the plan enumerates those listings from the code before the first change, and the review checks the enumeration"). Pre-flight §2.6.

**The enumeration** (from `crates/taiyaku-library/src/browse.rs` at 10ae1e74; the reviewer checks it against `rg -n 'FROM work|JOIN work' crates/taiyaku-library/src/browse.rs`):

| # | Site | Reached by | Takes the predicate | Why |
|---|---|---|---|---|
| 1 | `series_cards`' grouping join (`JOIN work w ON w.series_id = s.id`) | `series`, `one_series`' card, `search`'s series hits | yes, in the join's `ON` | a series' card counts and lists its works; a series whose every work is missing goes |
| 2 | `cover_edition`'s `JOIN work w` | `series_cards`' covers, `cover_work` | yes, in the join's `ON` | a cover from a missing work's files is a cover of nothing the series shows |
| 3 | `one_series`' `works_of` predicate (`w.series_id = ?`) | `GET /api/library/series/{id}` | yes | a series page |
| 4 | `unfiled_cards`' `works_of` predicate (`w.series_id IS NULL`) | `unfiled`, `search`'s unfiled hits | yes | one rule for every listing; unreachable by a derive today (Spec gaps, 3) |
| 5 | `home`'s `continuing` predicate | the continue strip | yes | |
| 6 | `home`'s `read` predicate | `GET /api/library/home` | yes | |
| 7 | `home`'s `added` predicate (`1 = 1`) | `GET /api/library/home` | yes | |
| 8 | `series_of_matching_works`, both arms (FTS and `LIKE`) | `search` | yes | a series must not be a hit for a missing work's title |
| 9 | `visible_editions` | `/api/works` and OPDS under a grant; OPDS library feeds | yes, as a `WHERE` after the grant's join | the registry skip covers `Access::Every`; this covers the grant's map |
| — | `matching_unfiled_works` | `search` | no | its ids are filtered through `unfiled_cards` (4) |
| — | `matching_series` | `search` | no | matches series titles; a series of missing works only is dropped by 1 |
| — | `works_of` itself, `editions_by_work`, `opens_by_work`, `alignments_of` | listings above | no | they read the ids a listing already chose |
| — | `work`, `visible_members` | the reader and the work page | no, decided | spec §5: the reader answers with a reason |
| — | `missing_by_edition` | edition cards | no, decided | goes on counting missing members |
| — | `held_in` | `opds::library_works_by_work` | no | its caller intersects it with `visible_editions` (9), and `works_feed` skips |
| — | `status`, `library_scope`, `narrowed` | the sidebar, narrowing | no | read libraries and directories, no work row |

**Files:**
- `crates/taiyaku-library/src/browse.rs`: the sites in the table.
- `crates/taiyaku-library/src/browse/tests.rs`: one test per row 1-9. Fixtures from `two_volume_library` (~204), `library_with_two_works` (~618) and the grant fixtures (`Access::Only` at ~1275); mark a work with `UPDATE work SET missing_since = datetime('now') WHERE id = ?`, since the mark itself is Task 1's.

- [ ] **Step 1: Write the nine failing tests.** Each fixture holds a present work beside the missing one and asserts the present one listed and the missing one not, so a listing that drops everything fails too:
  1. `series`: a series of a present and a missing volume counts `works == 1`; a series of one missing volume is not listed.
  2. `cover_work`: the series' `cover_work_id` set to the missing work (the model is `a_hand_set_cover_wins_over_the_fallback` ~523) answers the present work; and `series`' card cover agrees.
  3. `one_series`: lists the present work only.
  4. `unfiled`: two imported works (`import_a_work` ~182), one marked by hand: only the other is listed.
  5. `home().continuing`, 6. `home().read`, 7. `home().added`: a position (open, then completed) on each of a present and a missing work; the missing one is in none of the three lists.
  8. `search`: series S holds present "Alpha" and missing "Zebra"; searching "Zebra" (FTS) and "Ze" (`LIKE`) finds no series; "Alpha" still finds S.
  9. `visible_editions` under `Access::Only` and `Access::Every`: the missing work's slug is not a key.
  Run the library crate; all nine fail.
- [ ] **Step 2: Add the predicate** at each site in the table, and one sentence in the module doc's header saying a missing work is left out of every listing and why `work` and `visible_members` are not. Green.
- [ ] **Step 3: Watched mutations.** Remove each site's predicate in turn (both arms of 8 separately) and see exactly its test fail. Record which test went red for each; a site whose removal turns no test red is a finding.
- [ ] **Step 4:** Run nextest for `taiyaku-library` and `taiyaku-serve` (whose browse-backed route tests read these), clippy, fmt. Commit `Every listing a reader browses leaves a missing work out (O91)`. Write `$E/P1-T3-report.md` with the table as built.

---

### Part 1, Task 4: The registry's mark, `/api/works`, OPDS and the reader's 404 (O91)

Spec §5 "The hide", last two bullets. Pre-flight §2.4 ("The access-filter trap").

**Files:**
- `crates/taiyaku-serve/src/registry.rs`: `WorkRow` (~51) and `work_rows` (~73) read `missing_since IS NOT NULL`; `Works::read_all` (~266) calls `set_missing` on every work it keeps or makes from a row, the held one after `reproject`, the new one after `Work::new`, and the previous one it keeps when a row will not project.
- `crates/taiyaku-serve/src/works.rs`: `Work` (~30) and its doc (the new field and why it is atomic); `from_library` (~485) sets the flag too.
- `crates/taiyaku-serve/src/routes.rs`: `list_works` (~354) skips `w.is_missing()` for every access, before the `visible` match; `get_sidecar` (~596) answers a missing work after `view::of_work` succeeds: 404, `Content-Type: application/json`, `{"error": <sentence>, "missing": true}` (Sentences above). Its doc and `list_works`' say so.
- `crates/taiyaku-serve/src/opds.rs`: `works_feed`'s loop (~410) skips a missing work first, for every caller (lane 1 comments the href a few lines below). Its doc says so.
- Tests: `crates/taiyaku-serve/src/registry/tests.rs` (model ~86 `a_live_registry_serves_a_work_another_process_added_once_refreshed`); `routes/tests.rs` or `library_api/tests.rs` beside the `/api/works` grant tests; `opds/tests.rs` beside the works feed's grant tests. Fixtures: `test_support::router_over_a_live_library` (~174, an administrator) and a reader's (`router_over_a_scanned_work_as_a_reader` ~1168, or a live one built the same way). Mark by SQL on `state.library`, then `state.works.refresh_if_moved(&state.library)`.

- [ ] **Step 1: Write the failing tests.**
  - Registry: a held work is marked, refreshed, `is_missing()`; cleared, refreshed, not; and it is the same `Arc` throughout (`Arc::ptr_eq`). *Fails on:* the flag set only on a new `Work`.
  - `/api/works` under `Access::Every`: the missing work's slug is absent and the present one's present. *Fails on:* `list_works`' skip removed.
  - `/api/works` under a grant: the same. *Fails on:* the skip and `visible_editions`' predicate removed together (Spec gaps, 2).
  - `/opds/v1.2/works` under `Access::Every` and under a grant: the same two, by entry id. *Fails on:* `works_feed`'s skip removed (Every); it and `visible_editions`' predicate together (grant).
  - `GET /api/works/{work}` of a missing work by an administrator: 404, JSON, `missing: true`, `error` the sentence as a literal; of a present work: 200 as today; of the missing work by an account whose grant hides it: today's `` no work `{slug}` `` 404 with no `missing` key. *Fails on:* the check removed (200); the check placed before the grant check (the hidden work answers the missing body).
  - Run the serve crate; they fail.
- [ ] **Step 2: Implement.** Green.
- [ ] **Step 3: Watched mutations**, as named. Run nextest for `taiyaku-serve`, clippy, fmt. Commit `The registry carries the missing mark: /api/works and OPDS leave the work out, and the reader's read of it says its files are missing (O91)`. Write `$E/P1-T4-report.md`.

---

### Part 1, Task 5: The reader says why, and the scan report counts (O91)

Spec §5 "The hide", last bullet; decision 9. Part D §5.2.

**Files:**
- `web/src/api.ts`: `WorkMissing` beside `WorkNotFound` (~391); `getSidecarWithEtag` (~420): on a 404 whose body parses as a JSON object with `missing === true`, throw `WorkMissing(work, message)`, the message as today; any other 404 a `WorkNotFound` as today.
- `web/src/ReaderFailed.tsx`: `LoadFailure` and the alert (Sentences above), the slug in a `<code>` as `not-found` draws it; its doc's paragraph on the 404's three facts gains the fourth.
- `web/src/App.tsx`: the load's `catch` (~800) maps `WorkMissing` to `{ kind: 'missing', work }` before the `WorkNotFound` arm.
- `web/src/config.ts`: `Derived` (~217) gains the two keys. `web/src/ScanReport.tsx`: `DeriveSection` (~318) draws `marked_missing` ("Works marked missing") among what it did not do and `found_again` ("Missing works found again") among what it did.
- Fixtures that build a whole `Derived` gain both keys: `web/tests/support/derive.ts`, `ScanReport.test.tsx`, `RemoveRoot.test.tsx`, `RuleEditor.test.tsx`, `RootRow.test.tsx`, `Config.browser.test.tsx` (grep `root_removed_mid_derive: 0` under `web/tests`).
- Tests: `web/tests/api.test.ts` (model ~148), `web/tests/App.test.tsx` (model ~785), `web/tests/App.browser.test.tsx` (model ~1270), `web/tests/ScanReport.test.tsx` (the label case ~430).

- [ ] **Step 1: Write the failing tests.**
  - api: a 404 with body `{"error": "…", "missing": true}` rejects as a `WorkMissing` carrying the slug and not a `WorkNotFound`; a 404 with a plain body is still a `WorkNotFound`; a 404 whose JSON has `missing: false` is a `WorkNotFound`. *Fails on:* the `missing` check removed (every 404 a `WorkMissing`, or none).
  - App (jsdom): a `WorkMissing` draws the alert's sentence as a literal, the slug in a `<code>`, and not the not-found sentence. *Fails on:* `App`'s arm removed (draws the thrown message); `ReaderFailed` drawing the not-found words for `missing`.
  - App (browser): a slug of 160 characters in the missing sentence stays inside the screen, as the not-found case at ~1270 measures.
  - ScanReport: both rows are drawn with their labels as literals, the zero marked `data-zero`, and a non-zero count drawn. *Fails on:* a row dropped; the two labels swapped.
  - Run node, jsdom and the whole browser project; the new cases fail.
- [ ] **Step 2: Implement.** Green on all three projects.
- [ ] **Step 3: Watched mutations**, as named. Run `tsc -b` and biome. Commit `The reader says a missing work's files are missing, and the scan report counts the works marked missing and found again (O91)`. Write `$E/P1-T5-report.md`.

---

### Part 1, Task 6: The closes in `docs/status.md`, the 3.40.1 run and the gate

Read `docs/status.md`'s Open items section, rows O48 (~1656) and O91 (~1699), and the paragraphs that introduce each PR's closes (the newest, ~4123-4140, are the model).

- [ ] **Step 1: Close O48** with the five-part edit:
  1. The row's checkbox.
  2. The row's text: "**Closed <date>: …**" saying a partial scan is still applied and now hides what it did not find rather than stranding it, and the next good scan brings it back (Part D §5.2, "O48 closes by consequence"), with no threshold; then the tests by name; then "*As filed:*" and the original text, as the closed rows near it do. Sites by symbol: `derive::mark_missing`, `derive::derive_with`, `scan::scan_root`'s wholly-empty refusal (unchanged).
  3. A notes paragraph introducing this PR, then O48's bullet ("… Closed."), after the newest such paragraph.
  4. The "N are now closed — leaving M" sentence (~1482): regenerate the closed list from the table (`rg -o '^\| (O[0-9]+) \| \[x\]' -r '$1' docs/status.md | paste -sd, - | sed 's/,/, /g'`), its count word and "leaving M".
  5. "What remains: the M open items" (~4713); its count word can wrap onto the next line (`rg -n -A1 '^What remains:' docs/status.md`).
- [ ] **Step 2: Narrow O91.** Its row's text gains "**Narrowed <date>:**" saying the works are now marked, hidden and brought back, and what remains: nothing removes them until Part 2. Its notes bullet, if the sweep finds none, is written under the same paragraph. Counts do not move.
- [ ] **Step 3: The Rust total.** Measure the JUnit total from the gate's run and write it in both sites ("Six crates, N tests" ~179, "of the N are standing checks" ~256). **Web counts** in the same sentence: measure the `node` and `jsdom` totals from their logs, never add.
- [ ] **Step 4: Sweep.** Grep `O48` and `O91` across `docs/` (outside `docs/superpowers/`), `crates/`, `web/src/` and `scripts/`, and read every hit. A pointer that presents O48 as open is corrected; narrative in the past tense stays.
- [ ] **Step 5: SQLite 3.40.1.** From `$W`, with `H` the short hash of `HEAD`: `git -C $W archive HEAD | tar -x -C $W/target/bookworm/src-$H` (make the three directories first), then `docker run --rm --user "$(id -u):$(id -g)" -v "$W/target/bookworm/src-$H:/src" -v "$W/target/bookworm/target:/t" -v "$W/target/bookworm/cargo-home:/c" -e CARGO_TARGET_DIR=/t -e CARGO_HOME=/c -e HOME=/c -e RUSTUP_TOOLCHAIN=<the image's toolchain, from rustup toolchain list inside it> -w /src rust:1-bookworm cargo test --locked --no-fail-fast -p taiyaku-library -p taiyaku-serve -p taiyaku-cli -- --test-threads=6 >| $E/P1-bookworm.log 2>&1`. Two tests fail there by design (`stamp::tests::a_rewrite_syncs_…` and `opds::tests::the_work_feed_stats_its_members_on_the_blocking_pool`, which need `strace`); any other failure is real.
- [ ] **Step 6: The gate** into `$E/P1-gate-1.log`. Quote `all green`, the `.exit` file and the load.
- [ ] **Step 7:** Commit `O48 closes and O91 narrows in docs/status.md`. Write `$E/P1-T6-report.md`.

---

### Part 1, Finish (the controller)

1. **Whole-branch review** of `main..HEAD`, in two lanes, against spec §5 PR 1, Part D §5, the rulings and this plan's settled calls:
   - (a) the library: Tasks 1-3. Ask in particular whether the enumeration in Task 3 misses a reader of `work` that lists (check it against the grep it names), whether the pass's two `UPDATE`s can ever both touch one work, and whether any test passes against its named defect.
   - (b) serve, web and docs: Tasks 4-6. Ask whether any route that lists works reads the registry without the skip, whether the 404's order leaks a hidden work, and whether the closes' figures match their sources.
2. **One fix wave** by SendMessage to the implementers. Review the fixes too; after fixing a finding, grep for its shape, not its function.
3. **The final gate** on the final commit, as in Task 6 Step 6, into `$E/P1-gate-final.log`; re-read the Rust and web counts against that run.
4. **The drive** on a live server, at 1280 and 390 px. Never scan the test library itself (a scan's watch probe writes into its root). Instead:
   - record `sha256sum /home/davey/temp/manga-test/library.db` and `touch` a marker; build a scratch library in the session scratchpad from two series directories copied with `cp -r` (real copies, never `cp -l`) out of `/home/davey/temp/manga-test/Volumes`; a fresh database there; build the release binary and the web into the scratchpad; set a password; serve on a free port (check `ss -lptn` first; 127.0.0.1:4399 if free).
   - Add the directory and scan from Configuration. Read two pages of a volume and add a note.
   - Move that volume's file out of the scratch tree; scan. The volume is gone from the grid, its series page, search, the continue strip, `/api/works` and `/opds/v1.2/works`; the scan report shows "Works marked missing 1"; `/read/<slug>` shows the missing sentence.
   - Move it back; scan. The volume is back under the same slug, the reader opens at the page read, the note is there, and the report shows "Missing works found again 1".
   - Set an exclude pattern over the other series; scan; it is hidden; clear the pattern; scan; it is back.
   - Stop the server by PID, check the port again, delete the scratchpad's copy, and prove the test library unchanged: the same `library.db` digest, and `find /home/davey/temp/manga-test -newer <marker>` empty.
5. **The claim check** of every factual claim in the closed and narrowed rows, the notes and the PR body. Put the PR body in pass 1; scope later passes to the correction commit and the PR body; grep a corrected sentence's shape everywhere.
6. **The PR body** at `$E/P1-pr-body.md`, for the gitea MCP, never hard-wrapped, its first line the title, under 255 characters (measure it). It names O48 closed and O91 narrowed, the rulings and the plan's settled calls, the migration's provisional number, and no attribution. Ask the user before pushing.

---

# Part 2 (PR 2): list and remove, and O184

Part 2's branch `pre-deploy-missing-remove` is cut from main after PR 1 merges, in a fresh worktree `W`. Its line numbers are those of Part 1's code; re-grep each symbol before using a hint.

### Part 2, Task 1: The list and the removal, in the library (O91)

Spec §5 PR 2, "What a removal deletes", "Its derives", "Documents"; decisions 6 and 7. Pre-flight §2.4 "Removal machinery to reuse".

**Files:**
- `crates/taiyaku-library/src/scan.rs`, beside the removal code (`Removal` ~1072, `take_root` ~1570, `removal_cost` ~1806, `remove_root` ~1864): the new types and functions (New interfaces). Shape:
  - `missing_works`: every work with `missing_since` set, its title, slug and mark; its positions (`progress` rows) and notes (`note` rows), counted as `take_root` counts them; its path by `hand::path_for`'s rule; its libraries by reach (What the plan settles).
  - The removal: inside a `crate::Nested` transaction, read the ids (`slug = ? AND missing_since IS NOT NULL`, or every marked work); `None` for `One` with no such work, nothing changed; count positions and notes; `DELETE FROM work WHERE id IN (…)`; `derive::derive_with(…, Documents::Skip)`. `missing_removal_cost` drops the transaction (rollback, `removal_cost`'s shape); `remove_missing` commits, then runs `derive::derive` (`Reconcile`) and answers that report (`remove_root`'s shape). No document is carried (decision 7).
  - The comments in `derive.rs` and `scan.rs` that name the places a `work` row is deleted (grep `DELETE FROM work` in comments: `derive_with`'s fold comment "The only place `derive` itself reaches a `DELETE FROM work`", the "After everything" comment, `apply_removal`'s doc) name the missing removal too.
- Tests: `crates/taiyaku-library/src/scan/tests.rs`. The model for the documents test is `a_removal_leaves_unlisted_a_document_under_a_directory_that_stays` (~5166), with `keep_documents` (~6904).

- [ ] **Step 1: Write the failing tests.**
  - The list (spec §5 Testing, "The list"): two libraries, one directory each and a third directory nested so two libraries reach it; works missing under each; a position and two notes on one. Each row carries slug, title, libraries (names, by library id), path, mark, positions and notes; a present work is not listed. *Fails on:* the libraries read by link rather than reach (a work under the nested directory names one library); the counts read for another work.
  - Dry run (spec §5 Testing, "Dry run"): `missing_removal_cost(All)`'s counts equal what `remove_missing(All)` then answers, and after the dry run the store is unchanged (compare a census of `work`, `progress`, `note` and `missing_since`). *Fails on:* the dry run committing.
  - A returned work (spec §5 Testing): list, then restore the file, scan and derive, then `remove_missing(One(slug))`: `None`, and the work, its position and notes stand. `remove_missing(All)` in the same state removes nothing. *Fails on:* the mark not checked inside the transaction (delete by slug alone).
  - One and all: `One` removes that work only and answers its counts; `All` removes every marked work and no present one.
  - Documents (decision 7): a missing work with a hand document under a directory that keeps documents; `remove_missing(One)`: the file is still on disk and the answer's report lists it in `orphan_documents`. *Fails on:* the removal answering its `Skip` report instead of running the `Reconcile` derive.
  - Run the library crate; they fail.
- [ ] **Step 2: Implement.** Green.
- [ ] **Step 3: Watched mutations**, as named. Run nextest for `taiyaku-library`, clippy, fmt. Commit `The missing works are listed with their libraries, and removed one or all, counted first (O91)`. Write `$E/P2-T1-report.md`.

---

### Part 2, Task 2: The routes, under the job lock (O91)

Spec §5 PR 2, "Routes" and "The lock"; decision 4. Model: `config_api::delete_root` (~1925) and `DryRunQuery` (~1856).

**Files:**
- `crates/taiyaku-serve/src/config_api.rs`: `list_missing`, `delete_missing_all`, `delete_missing_one`, each first calling `session::administering_or_forbidden`; `AdminAnswer::NotMissing` and its arm in `into_response`; a small JSON writer for the list and one for the three counts (destructure what they read with no `..` where they read a whole struct). The real removal takes `state.library_job.try_lock()`: `WouldBlock` answers `AdminAnswer::Refused(waits_for_the_job(…))` (Sentences above); a poisoned lock is taken (`into_inner`), as `libraries_api::relinking` (~813) does. The dry run takes no lock. Both run on `state.library.working()` in `spawn_blocking`.
- `crates/taiyaku-serve/src/library_job.rs`: `LibraryJob`'s doc names the missing removal among the takers that refuse.
- `crates/taiyaku-serve/src/routes.rs`: `guarded_routes` (~1944), two entries beside `/api/library/split-editions`: `/api/library/missing` (`get(list_missing).delete(delete_missing_all)`) and `/api/library/missing/{work}` (`delete(delete_missing_one)`). Every list that names `/api/library/split-editions` gains both paths: `grant_postures` (`GrantIndependent`, with a reason), `narrowing_postures` (`ADMIN`), and `auth/tests.rs`' `expected` array and `REFUSE_A_DEVICE_CREDENTIAL` (grep `"/api/library/split-editions"` across `crates/taiyaku-serve/src` and read every hit).
- Tests: `crates/taiyaku-serve/src/config_api/tests.rs`; `every_config_api_route_refuses_a_reader` (~126) gains the three method-and-path pairs. Fixture: a live library with a work made missing by moving its file and scanning, or by SQL plus a derive.

- [ ] **Step 1: Write the failing tests.**
  - The list answers each field, as a literal object for one row.
  - `DELETE /api/library/missing/{work}?dry_run=true` answers the counts and changes nothing; without `dry_run` it removes the work and answers the same counts and `"dry_run": false`; `DELETE /api/library/missing` removes them all.
  - The lock (spec §5 Testing): hold `state.library_job.lock()` in the test; the removal answers 409 with `{"error": "a scan or another library job is running; remove the work once it has finished"}` as a literal (and the all route its own sentence), and nothing is deleted; the dry run, with the lock still held, answers its counts. *Fails on:* the lock not taken (the removal answers 200); the dry run taking it (409).
  - A slug naming no missing work: 404 with the `NotMissing` sentence as a literal, for the dry run and the removal.
  - A reader is refused all three (403), and an API key is refused, through the shared lists.
  - Run the serve crate; they fail.
- [ ] **Step 2: Implement.** Green.
- [ ] **Step 3: Watched mutations**, as named, plus: answer `NotMissing` as 200 and see its test fail. Run nextest for `taiyaku-serve`, clippy, fmt. Commit `The missing works routes, administrator-gated, and a removal refuses while the library job lock is held (O91)`. Write `$E/P2-T2-report.md`.

---

### Part 2, Task 3: `taiyaku library missing` (O91)

Spec §5 PR 2, "The CLI"; decision 6. Model: `proposals` (`crates/taiyaku-cli/src/library.rs` ~1154), which prints by default and acts on a flag.

**Files:**
- `crates/taiyaku-cli/src/library.rs`: `command`'s match (~66) and both subcommand-list sentences; a new `missing` with `reject_unknown_flags` and `reject_positional_arguments` for `--remove`, `--all`, `--db`; `--remove` and `--all` together refused; line formatters as pure functions, for the reason `proposal_line`'s doc gives; the removal prints its line and then `derive_summary`'s.
- `crates/taiyaku-cli/src/main.rs`: the usage line beside `library proposals`' (~214): `taiyaku library missing [--remove SLUG]... [--all] [--db PATH]`.
- `README.md`: a short block beside `library proposals`' (~236-245), in its style.
- Tests: the formatters in `crates/taiyaku-cli/src/library/tests.rs`; end to end in `crates/taiyaku-cli/tests/cli.rs` (model `library_proposals_lists_and_accepts_over_real_stdout` ~1203), over a real tree whose volume file was moved out and scanned.

- [ ] **Step 1: Write the failing tests.**
  - The line formatters, with the Sentences as literals, libraries joined and "no library" and "no path".
  - `library missing` prints the rows and deletes nothing (the store's `work` count unchanged); `--remove SLUG` removes that work and prints its line; `--all` removes every missing work; `--remove` of a slug naming no missing work exits 1 with the `NotMissing` sentence and removes nothing; `--remove` with `--all` is refused.
  - *Fails on:* the list path calling the removal; `--remove` ignoring its slug (removing all); the exit status 0 on a refusal.
- [ ] **Step 2: Implement.** Green.
- [ ] **Step 3: Watched mutations**, as named; rebuild `target/debug/taiyaku` after restoring. Run nextest for `taiyaku-cli`, clippy, fmt. Commit `taiyaku library missing lists the missing works, and --remove and --all remove them (O91)`. Write `$E/P2-T3-report.md`.

---

### Part 2, Task 4: A live registry never mints a `bridge:` row (O184)

*Done in go-public lane 3, as that lane's plan's Task 6
(`2026-11-12-go-public-lane-3-second-accounts.md`), which closed O184.*

Spec §5 PR 2, "O184"; decision 5. O184's row (~1793).

**Files:**
- `crates/taiyaku-serve/src/works.rs`: `Mint` and `NoRow` (New interfaces); `row_id_for_write` (~401) and `row_id_for_document` (~424): under `Mint::Never` a missing row is `NoRow::Gone`, and nothing is inserted; their docs.
- `crates/taiyaku-serve/src/progress.rs`, `notes.rs`, `truth.rs`: `store` takes the `Mint` and answers `NoRow`; their docs.
- `crates/taiyaku-serve/src/alignments.rs`: `store(conn, work_id, doc)`; its doc and the module doc say who resolves the row and that a fixed registry still mints (spec §5: `test_support::state`'s fixtures and `corpus.rs` pair a fixed registry with an empty library).
- `crates/taiyaku-serve/src/routes.rs`: the four writers (`put_progress` ~1509, notes ~1167, truth ~1364, `patch_alignments` ~1897) pass `Mint::of(&state.works)` and answer `Gone` 404 through `NoProgress::NotFound`, `NoNotes::NotFound`, `NoTruth::NotFound` and `(StatusCode::NOT_FOUND, …)`; `patch_alignments` resolves the row inside its `what != AlreadyTrue` branch before the file write.
- `crates/taiyaku-serve/src/lib.rs`: `ServeState::library`'s doc (~92) on the row `row_id_for_write` "might still mint".
- Tests: `crates/taiyaku-serve/src/works/tests.rs` (the bridge tests ~251-390 pass `Mint::Bridge`); `routes/tests.rs`. Fixtures: `router_over_a_live_library` for progress, notes and truth; `router_over_a_live_library_with_an_edge` (~189) for the alignments `PATCH`.

- [ ] **Step 1: Write the failing tests** (spec §5 Testing, "O184"). Each: delete the work's row by SQL with no refresh after (the registry still holds the work; a lookup hit does not refresh), then send the write as the administrator, then read `SELECT count(*) FROM work`. The administrator matters: under a grant, `view::flags_or_refuse` finds no visible member for a deleted row and answers `` no work `{slug}` `` before the store is reached, so only `Access::Every` reaches the mint.
  - progress `PUT`, notes `PUT`, truth `PUT`: 404 `` no work `{slug}` `` as a literal, and no work row.
  - alignments `PATCH`: 404, no work row, and the work's file (where it has one) unchanged.
  - A fixed registry still mints: an existing bridge test passes with `Mint::Bridge`; keep one that asserts the `bridge:` row.
  - *Fails on:* the mint restored under `Mint::Never` (each answers 2xx and a row appears); `Mint::of` answering `Bridge` for a live registry; the `PATCH`'s row resolved after the file write (the file changes).
  - Run the serve crate; they fail.
- [ ] **Step 2: Implement.** Green.
- [ ] **Step 3: Watched mutations**, as named. Run nextest for `taiyaku-serve` and `taiyaku-cli`, clippy, fmt. Commit `A write to a work whose row is gone answers 404 from a live registry instead of minting a bridge row (O184)`. Write `$E/P2-T4-report.md`.

---

### Part 2, Task 5: The Missing works tab: its list (O91)

Spec §5 PR 2, "A Missing works tab"; decision 6. Models: `web/src/SplitEditions.tsx` (a list tab), `web/src/splitEditions.ts`, `pageReads.ts`' `SPLIT_EDITIONS` (~116).

**Files:**
- `web/src/missingWorks.ts` (new): the types and `listMissing`, `missingRemovalCost(which)`, `removeMissing(which)`, with `?dry_run=true` spelled exactly as `config.ts`' `removalCost` explains; `which` a slug or `'all'`.
- `web/src/pageReads.ts`: `MISSING_WORKS`; `invalidateLibraryViews` marks it too (a scan changes which works are missing), and its doc says so.
- `web/src/config.ts`: `TABS` gains `{ path: 'missing-works', label: 'Missing works' }` after Split editions.
- `web/src/Config.tsx`: `SCREENS['missing-works']`.
- `web/src/MissingWorks.tsx` (new): `usePage([MISSING_WORKS])`, `PageHeader` with the title and description, the empty sentence, one `<li>` per work in a `settings-group` list with its lines (Sentences above), "Remove…" per row and "Remove all…" above the list (each opens nothing yet beyond a stub the next task replaces, or is left out until Task 6: the implementer's call, said in the report).
- CSS in `web/src/styles/settings.css`, where `split-edition-*` is, only if the list needs it.
- Tests: `web/tests/MissingWorks.test.tsx` (new), `web/tests/Config.test.tsx` (it drives every tab in `TABS` and must mock this one's read), `web/tests/pageReads.test.ts` if it pins `invalidateLibraryViews`.

- [ ] **Step 1: Write the failing jsdom tests.**
  - Rows: two works, one in two libraries with a path and one in none with no path; each line asserted as a literal, per row (not one whole-page `toContain`).
  - Empty: the sentence; no "Remove all…".
  - A non-administrator (Spec gaps, 1): the read rejects with `/api/library/missing: 403 only an administrator may do this` (the model is `SplitEditions.test.tsx` ~301); the page draws that sentence, no row and no Remove button.
  - `Config.test.tsx` reaches the tab.
  - *Fails on:* a line's field swapped for another's; the empty sentence drawn beside rows; the tab left out of `TABS` or `SCREENS`.
  - Run node and jsdom; they fail.
- [ ] **Step 2: Implement.** Green. If CSS changed, run the whole browser project.
- [ ] **Step 3: Watched mutations**, as named. Run `tsc -b` and biome. Commit `Configuration lists the missing works, each with its libraries, path, mark and reading (O91)`. Write `$E/P2-T5-report.md`.

---

### Part 2, Task 6: The Missing works tab: removing, counted first (O91)

Spec §5 PR 2 ("Remove one, or remove all, each behind a dialog that shows the counts first"). Model: `web/src/RemoveRoot.tsx` (dry run on mount, counts, confirm held until the counts exist, the in-flight ref, the 409 drawn in the dialog, `DeriveSection`).

**Files:**
- `web/src/RemoveMissing.tsx` (new): the dialog, for one work or all (Sentences above). On the removal's answer it hands the answer to the page; on a 409 it draws the sentence and stays; on a 404 (`NotMissing`) it draws the sentence and the page reads the list again.
- `web/src/MissingWorks.tsx`: opens the dialog from "Remove…" and "Remove all…"; after a removal draws the page's line and `DeriveSection` beneath until the page is left; marks `MISSING_WORKS` stale and calls `invalidateLibraryViews`.
- Tests: `web/tests/RemoveMissing.test.tsx` (new), `web/tests/MissingWorks.test.tsx`; one browser case in a new `web/tests/MissingWorks.browser.test.tsx` or the browser file that already lays out Configuration tabs at 390 px (grep `390` in `web/tests/*.browser.test.tsx`).

- [ ] **Step 1: Write the failing tests.**
  - The dialog's counts from the dry run, as literals, singular and plural (one work, two positions, one note); the confirm absent until the counts arrive; zero drawn ("0 notes").
  - Confirm sends the removal once on two quick presses (the in-flight ref); the page then draws its line and the derive report, and the row is gone after the list is read again.
  - A 409 from the removal: the dialog draws the lock sentence as a literal and offers the confirm again once the counts are read again.
  - A 404 from the dry run: the dialog draws the `NotMissing` sentence and the page reads the list again.
  - "Remove all…": the dialog's counts are the dry run of all; its confirm reads "Remove every missing work".
  - Browser, at 390 px: the list with a long path, and the open dialog, stay inside the page (no horizontal overflow), measured as the neighbouring cases do.
  - *Fails on:* the confirm drawn before the counts; the in-flight ref removed (two `DELETE`s); the 404 not re-reading the list; `which` sent as `all` from a row.
  - Run node, jsdom and the whole browser project; the new cases fail.
- [ ] **Step 2: Implement.** Green on all three.
- [ ] **Step 3: Watched mutations**, as named. Run `tsc -b` and biome. Commit `A missing work is removed from Configuration one at a time or all together, its counts shown first (O91)`. Write `$E/P2-T6-report.md`.

---

### Part 2, Task 7: The closes in `docs/status.md`, the 3.40.1 run and the gate

*O184 was closed by go-public lane 3 (`2026-11-12-go-public-lane-3-second-accounts.md`); O91 is still open, so this task closes O91 alone and leaves O184's row as that lane wrote it.*

As Part 1 Task 6, for O91 and O184 (row O184 ~1793; O91 as Part 1 left it).

- [ ] **Step 1: Close O91 and O184** with the five-part edit. O91's text says the works are marked, hidden, brought back (PR 1) and listed and removed by an administrator, counted first (this PR), sites `derive::mark_missing`, `scan::missing_works`, `scan::remove_missing`, `config_api::delete_missing_one`. O184's says a live registry never mints and answers 404, a fixed registry still mints, and why; sites `works::row_id_for_document`, `works::Mint`, `alignments::store`, `routes::patch_alignments`. A notes paragraph introducing this PR, then each bullet. The count sentences regenerated from the table, as Part 1 Task 6 Step 1 says.
- [ ] **Step 2: The Rust total and the web counts**, measured, as Part 1 Task 6 Step 3.
- [ ] **Step 3: Sweep** `O91` and `O184` across `docs/` (outside `docs/superpowers/`), `crates/`, `web/src/` and `scripts/`, and read every hit; `crates/taiyaku-cli/src/library.rs`' module doc on the bridge row stays true for a fixed registry, and says so if it does not.
- [ ] **Step 4: SQLite 3.40.1**, Part 1 Task 6 Step 5's recipe, into `$E/P2-bookworm.log`.
- [ ] **Step 5: The gate** into `$E/P2-gate-1.log`; quote `all green`, the `.exit` file and the load.
- [ ] **Step 6:** Commit `O91 and O184 close in docs/status.md`. Write `$E/P2-T7-report.md`.

---

### Part 2, Finish (the controller)

1. **Whole-branch review** of `main..HEAD`, in three lanes, against spec §5 PR 2, the rulings and this plan's settled calls:
   - (a) the library and the CLI: Tasks 1 and 3. Ask whether anything deletes a work not marked inside the transaction, whether the dry run can commit, and whether every comment that names the places a work row is deleted names this one.
   - (b) serve: Tasks 2 and 4. Ask whether any `library_job` taker answers other words (`rg -n 'library_job' crates/taiyaku-serve/src`), whether any write path can still mint from a live registry (`rg -n 'bridge:' crates/taiyaku-serve/src`), and whether the new routes are in every route list.
   - (c) the web and the docs: Tasks 5-7.
2. **One fix wave** by SendMessage to the implementers; review the fixes; grep each finding's shape.
3. **The final gate** on the final commit into `$E/P2-gate-final.log`; re-read the counts against it.
4. **The drive**, the scratch library as in Part 1's Finish, at 1280 and 390 px: two volumes made missing in two series; the tab lists both with their library, path, mark and reading; remove one through its dialog (counts first); remove all; the grid and the Libraries cards after; `taiyaku library missing` and `--remove` against a copy of the scratch database. Stop the server by PID, check the port, delete the scratch copy, prove the test library unchanged by its `library.db` digest and the marker.
5. **The claim check**, as Part 1's, PR body in pass 1.
6. **The PR body** at `$E/P2-pr-body.md`, never hard-wrapped, title first and under 255 characters (measure it); it names O91 and O184 closed, the rulings and the settled calls, and no attribution. Ask the user before pushing.
