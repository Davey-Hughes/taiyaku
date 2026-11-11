# Pre-deploy lane 1: formats (O262, O263, O264, O185) — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Settle four formats a deployed store keeps, before the first real data exists:
- O262: a work's slug drops letters outside ASCII and keeps dash runs and the archive's extension. The series cover's redirect does not encode its `Location`.
- O263: an open that migrates the database takes no backup first.
- O264: `LibraryDoc`, `TruthDoc` and `RegionDoc` read a newer version, and `hand::save` and an in-place export write over a newer document.
- O185: a ComicInfo write-back cut while staging leaves a file the next scan reads.

**Architecture:**
- Task 1: one slug rule for work slugs and edition keys.
- Task 2: a non-ASCII slug through the cover redirect, OPDS and the reader route.
- Task 3: a backup name tagged with its migration, listed and never pruned.
- Task 4: the open takes that backup before it migrates, and the docs and the Backup tab say so.
- Task 5: the three readers refuse a newer document.
- Task 6: `hand::save` leaves a newer hand document alone.
- Task 7: an in-place export keeps a newer sibling.
- Task 8: the stamp stages under a dot name (O185).
- Task 9: the closes in `docs/status.md`.
- Then the Finish.

This lane adds no migration and no SQL statement the store does not already run.

**Tech Stack:** Rust (`taiyaku-core`, `taiyaku-library`, `taiyaku-serve`, `taiyaku-cli`), TypeScript/React (`web/`), vitest projects `node`, `jsdom` and `browser`.

**Spec:** `docs/superpowers/specs/2026-11-10-pre-deploy-design.md` (merged as 10ae1e74, #231): §1 for the lanes and their coordination, §2 for this lane. Its sections are cited as "spec §2 <heading>", and its decisions as "decision n". The pre-flight (`.superpowers/sdd/pre-deploy-evidence/formats-preflight.md`, local) read the code at 6ecd66e3; every location below was read again at 10ae1e74. Locations are by symbol. Line numbers are hints taken at 10ae1e74.

**The owner's rulings of 2026-11-10** (binding; do not re-open):
- **The slug rule.** Keep Unicode letters and digits, collapse each run of other characters to one `-`, drop a trailing archive extension, keep the `-2`/`-3` suffixes, and reuse `proposals::slugify`'s rule. Encode the series cover redirect's `Location` with it.
- **Told to the owner, no objection:**
  - A backup before every migration: `library-<stamp>-before-v<N>.db` in the existing backups directory, listed in the Backup tab, never deleted automatically. The migration is refused if the backup fails. It is skipped for an in-memory database, a fresh one (version 0) and one already up to date.
  - An older binary leaves a newer hand document alone and says so (`persisted: false` and a reason).
  - Readers refuse a newer version, as `Sidecar::readable_version` does.
- **Merge order.** This lane merges first. Lane 2's notes-version task builds on Task 5's sentence and Task 7's export guard once they are on `main`.

**What the plan settles** (the controller's calls; told to the owner, not rulings):
1. **The shared slug rule** lives in `import.rs` as `slug_token`. `proposals::slugify` is deleted and its one caller calls `crate::import::slug_token`. The work-slug path (`import::slugify`) drops the extension and falls back to `work`. The edition-key path does neither.
2. **The edition-key refusal test** the spec asks for already exists: `proposals::tests::accept_refuses_an_edition_of_proposal_whose_suffix_has_no_edition_key_to_give` (`proposals/tests.rs` ~1037).
3. **No existing test opens an older file database through `Library::open`** at 10ae1e74. The older-database tests in `schema/tests.rs` hold a bare `Connection` (in memory, or two on one file for the race) and call `migrate` or `migrate_with` directly. One test can now leave a backup behind: `tests::several_first_opens_of_one_fresh_library_at_once_all_succeed` (`tests.rs` ~1252). An open racing another open's first migration can read a version between 0 and `N`, and it then backs up the half-migrated database under a `-before-v<N>` name. That is harmless, and it is stated as a residual in O263's close. Task 4 runs that test twenty times to show it stays green.
4. **The tagged name.** `N` is one or more ASCII digits, the first not `0`, fitting a `u32`, so one version has one spelling. Nothing else may sit between the stamp and `.db`.
5. **The in-memory skip has no test.** An in-memory database has no path, so there is no directory a backup could be written to.
6. **One sentence for a newer document.** `taiyaku_core::refuse_newer` holds `readable_version`'s words with the key as a parameter. `Sidecar::readable_version` calls it with `"taiyaku"`, and its words stay byte for byte as they are. The three readers call it with their own keys.
7. **The words** are written once, below, where each task needs them: the refusal, `save`'s `why`, the refused open, the Backup tab's paragraph, the README's paragraphs and the CLI's kept line.
8. **`docs/sidecar.md` gets one paragraph** in "Forward compatibility" (Task 7). It covers the sibling documents' versions and the bump rule for hand documents (decision 7), where a reader of the format looks for them.
9. **The drive** serves on 127.0.0.1:4391. Other lanes' drives may hold neighbouring ports.

**New interfaces this plan names** (first written here; lane 2 consumes the ones marked):
- `crate::import::slug_token(s: &str) -> String` in `taiyaku-library`, `pub(crate)`. It is `proposals::slugify`'s body: Unicode letters and digits, lowercased, each run of other characters one `-`, none leading or trailing. It may answer `""`.
- `crate::schema::pending_migration(conn: &Connection) -> Result<Option<(u32, u32)>, String>`, `pub(crate)`. It answers `Some((current, SCHEMA_VERSION))` exactly when `0 < current < SCHEMA_VERSION`.
- `crate::backup::name_before_migration(conn: &Connection, target: u32) -> Result<String, String>`, `pub(crate)`. It answers `library-<stamp>-before-v<target>.db` from SQLite's clock.
- `crate::backup::before_migration(conn: &Connection, db: &Path, name: &str) -> Result<(), String>`, `pub(crate)`. It takes `name` into `directory_for(db)`. `TakeError::Exists` is `Ok(())`.
- `backup::Backup` gains `pub before: Option<u32>`: the `N` of a `-before-v<N>` name, `None` for a backup taken on request. It is not sent on the wire. `backup_api`'s JSON and the web's `Backup` type are unchanged.
- **(lane 2)** `taiyaku_core::refuse_newer(key: &str, declared: u32, reads: u32) -> Result<(), String>`, re-exported from `taiyaku-core`'s `lib.rs`. When `declared > reads` it answers `Err` of: `this document declares {key} format version {declared}, and this build reads {reads}. Refusing to read it rather than dropping the fields it does not understand and writing them back out as version {reads} — upgrade taiyaku instead`. Lane 2's notes document refuses a newer `taiyaku_notes` through it.
- **(lane 2)** `crate::migrate::declared_version(path: &Path, key: &str) -> Option<u64>` in `taiyaku-library`, `pub(crate)`. It reads the file at `path` as a JSON object and answers `key`'s value as a non-negative integer. It answers `None` for a file that is missing or unreadable, is not JSON, is not an object, lacks the key, or holds something other than a non-negative integer there.
- **(lane 2)** In `export.rs`, the private `const VERSIONED: [(&str, &str, u32); 4]` lists `(suffix, key, the version this build writes)` for each versioned sibling: `("json", "taiyaku", taiyaku_core::VERSION)`, `("library.json", "taiyaku_library", crate::librarydoc::VERSION)`, `("progress.json", "taiyaku_progress", crate::progressdoc::VERSION)` and `("truth.json", "taiyaku_truth", taiyaku_core::TRUTH_VERSION)`. A private `fn newer_sibling(path: &Path, suffix: &str) -> bool` looks the suffix up there and compares `declared_version`. Both of `export::work`'s loops ask it, in place only. Lane 2 adds `("notes.json", "taiyaku_notes", <its version>)` and makes the length 5. `notes.json` is always in the write loop, so that loop's ask covers it.

## Spec gaps

Each is resolved as stated here, and none silently.
1. **The CLI's kept line would be wrong.** `library export --in-place` prints every kept file as "kept … (this library has never read it, …)" (`taiyaku-cli/src/library.rs` ~1363-1369), and `tests/cli.rs` (~1066-1072) pins it. A newer sibling that the export kept may well have been read. Task 7 rewords the line to name both causes and updates the pin.
2. **"Existing tests that open an older file database through `Library::open`"**: there are none (settled item 3). The spec expected some.
3. **Stale sentences the spec did not name:**
   - `web/src/backup.ts`'s module doc repeats "`--keep` is where retention lives", and README ~348-352 says the schedule is "the cron entry above rather than anything inside the server". Task 4 changes both.
   - `taiyaku-cli/src/tests.rs` (~2606-2607) describes the old slug rule ("non-alphanumerics to `-`"). Task 1 rewords it with the spec's five comments.
4. **Decision 11 holds only where a directory keeps documents.** `save`'s early branch for a root that keeps no documents records the marker without reading the file. So after a hand edit there, a newer document beside the work stops being reported in `unreadable_hand`. The file is never written or removed there. The guard is about writes, so the plan leaves that branch as it is and O264's close states the residual.
5. **O185 has no notes bullet** in `docs/status.md`. Task 9 adds its close beside the O262-O264 bullets.

## Global Constraints

- **Five lanes in parallel.** This lane runs in its own git worktree `W` (an absolute path the controller gives each dispatch), on branch `pre-deploy-formats`, cut from main 10ae1e74. Every command runs against `$W` by absolute path, in one Bash call of the form `builtin cd "$W" && …`. Never a bare `cd` (it is zoxide here), and never a `cd` that outlives its call.
- **Evidence.** `E=/home/davey/projects/taiyaku/.superpowers/sdd/pre-deploy-evidence/lane-1-formats`, absolute, in the main checkout, so reports outlive the worktree. Logs and each task's report (`$E/T<n>-report.md`) go there, written by Bash (`cat >| … <<'EOF'`). The Write tool is refused under `.superpowers/`. Never write evidence under `target/`.
- **Builds.** Never set a shared `CARGO_TARGET_DIR`: the worktree builds into its own `$W/target`. `$W/web/node_modules` is a real copy, never a symlink, and `$W/crates/taiyaku-wasm/pkg` is present (the controller copies both). If either is missing, stop and report. Never `pnpm exec`: call `./node_modules/.bin/<tool>` directly.
- **Rust, per task.** Each into its own log, followed by `echo "exit $?" >| …` into a `.exit` file, which is what you read:
  - `builtin cd "$W" && env -u TAIYAKU_CORPUS cargo nextest run -p <crate> --test-threads=8 >| "$E/T<n>-<crate>.log" 2>&1`, one run per crate the task names;
  - `cargo clippy --workspace --all-targets --all-features -- -D warnings`;
  - `cargo fmt --all -- --check`.

  Read the summary lines from the log.
- **Web, per task that touches `web/`.** `builtin cd "$W/web" && ./node_modules/.bin/vitest run --project <node|jsdom|browser> --reporter=default >| "$E/T<n>-<project>.log" 2>&1`, then `./node_modules/.bin/tsc -b` and `./node_modules/.bin/biome check ./src ./tests` the same way. Run whole projects, never a named file. A task touching a `*.browser.test.tsx` file or CSS runs the whole browser project (CI never runs it).
- **Gate.** The repo's own script, never a copy. It `cd`s to `git rev-parse --show-toplevel`, so it must start inside `$W`: `builtin cd "$W" && env -u TAIYAKU_CORPUS NEXTEST_TEST_THREADS=8 RUST_TEST_THREADS=8 ./scripts/check-all.sh >| "$E/<name>.log" 2>&1; echo "exit $?" >| "$E/<name>.exit"`. Wait for load < 24 (`cat /proc/loadavg`) and record it. Read the `.exit` file, never the harness's exit code. Never pipe the gate through `tail`. Quote its `all green` line.
- **Browser crashes.** A browser-project failure with import or connection errors and no failed assertion is the headless Chromium crashing. Check `coredumpctl list` and `/proc/loadavg`, then rerun the whole project.
- **Pre-existing failures.** A failure in a file the task did not touch is pre-existing only if the same command fails at main 10ae1e74, not at another commit on this branch. Report it; do not fix it.
- **Watched mutations.** Every new or tightened assertion gets one. Put the named defect back by hand and see the test fail. Restore it, and check with `git diff` that the restore is complete. Record each in the task report: the defect, the failing test, and the assertion's message. A mutation `cargo nextest` run rebuilds the crate, so restore before the task's final run.
- **Sentences are literals.** A test asserts a user-facing sentence written out in the test, never one computed through the helper under test.
- **Comments.** A code comment says why: the reason, the invariant, what breaks if it changes. It never carries history or measurements, and it may point at an O-number. `scripts/check-comments.mjs` and `scripts/comment-budget.json` enforce part of this through the gate. If the gate fails on a share, follow `docs/ci.md`, "The comment guard", and say so in the report.
- **Attribution.** None anywhere: code, comments, docs, commits, PR body.
- **Commits.** One commit per task, its subject naming the O-number(s). Never `--amend`. After committing, check the new hash is an ancestor of `HEAD`.
- **Shared files.** Lane 2 edits `lib.rs` (`Note`, `Notes`), `schema/tests.rs`, `state.rs` and `export.rs`. Lane 4 edits `opds.rs` inside `works_feed`'s loop. Touch only the functions named here, so that a rebase meets text conflicts at most.
- **`docs/status.md`** changes only in Task 9, the counts included. Intermediate commits leave the Rust total stale. The gate's `status-test-count` step reads it, which is why the gate runs only after Task 9.

---

### Task 1: One slug rule for work slugs and edition keys (O262, part 1)

Spec §2 decision 1 and "The slug rule". Pre-flight §1.

**Files:**
- This plan: copy `/home/davey/projects/taiyaku/.superpowers/sdd/pre-deploy-evidence/plans/2026-11-10-pre-deploy-formats.md` to `$W/docs/superpowers/plans/2026-11-10-pre-deploy-formats.md`. It rides in this task's commit.
- `crates/taiyaku-library/src/import.rs`:
  - `slugify` (~606-618), and the new `slug_token` beside it;
  - `unique_slug` (~586-603), whose body does not change, and its doc (~578-585).
- `crates/taiyaku-library/src/proposals.rs`:
  - `slugify` (~1310-1331) is deleted, and its doc moves to `slug_token`;
  - its caller in the edition-of arm (~757) calls `crate::import::slug_token`. The comment above it (~752-756) keeps saying why an empty token is refused.
- Comments to reword (spec §2's five, plus spec gap 3's):
  - `import::unique_slug`'s doc says the rule is `slugify`'s. It also says `taiyaku-serve`'s `works::work_id` keeps its own ASCII rule for works with no store row, and the two need not agree.
  - `state::notes`' doc (`crates/taiyaku-library/src/state.rs` ~172-174): `work` is the work's slug, which is `taiyaku-serve`'s work id for a work with a store row. It drops "the same shape".
  - `opds::works_feed` (`crates/taiyaku-serve/src/opds.rs` ~424-426) and `opds::work_feed` (~535-538): a slug keeps letters outside ASCII, so the id is percent-encoded like every other segment. Drop "by construction" and "already URL-safe".
  - The `★★★` test's doc in `crates/taiyaku-library/src/import/tests.rs` (~1249-1252): "nothing alphanumeric", not "nothing ASCII-alphanumeric".
  - `crates/taiyaku-cli/src/tests.rs` (~2606-2607): "each run of other characters becomes one `-`", not "non-alphanumerics to `-`".
- Pinned literals: in `crates/taiyaku-library/src/derive/tests.rs`, `alone---v01` (~1519) becomes `alone-v01`. In `crates/taiyaku-library/src/browse/tests.rs`, `series---v01` becomes `series-v01` at ~281, ~1494, ~1515, ~1542, ~1569, ~1602 and ~1623.
- Tests: `crates/taiyaku-library/src/import/tests.rs`, beside `a_title_with_no_alphanumeric_characters_slugifies_to_work` (~1254) and `a_third_work_with_one_title_takes_the_third_slug_in_sequence` (~412).

**The rule** (spec §2 "The slug rule"):
- `slug_token(s)` is `proposals::slugify`'s body, unchanged: `char::is_alphanumeric`, lowercased with `char::to_lowercase`, each run of other characters one `-`, none leading or trailing. Its doc keeps the edition-key example and says that work slugs use it too.
- `slugify(title)` takes these steps in order:
  1. It drops one trailing archive extension, compared case-insensitively. The extensions are `.cbz`, `.cbr`, `.cb7`, `.cbt`, `.zip`, `.rar`, `.7z`, `.tar`, `.pdf` and `.epub`.
  2. It applies `slug_token`.
  3. It answers `work` when nothing is left.
- `taiyaku-serve`'s `works::work_id` and `derive::title_of` are not touched (spec §2: a title keeps its extension).

- [ ] **Step 1: Copy the plan in** (Files, first bullet).
- [ ] **Step 2: Write the failing tests** in `import/tests.rs`. Each calls `super::slugify` directly; a child module reaches the private function. Every expected value is a literal:
  - `Spy × Family - v01 (2020).cbz` gives `spy-family-v01-2020`.
  - `Kabushikigaisha Magi-Lumière - v02 (2024).cbz` gives `kabushikigaisha-magi-lumière-v02-2024`.
  - `「五等分の花嫁 第01巻」` gives `五等分の花嫁-第01巻`.
  - `V01.CBZ` gives `v01`.

  The `★★★` → `work` test and the `t`, `t-2`, `t-3` test already pin the fallback and the suffixes. Keep them as they are, apart from the comment.

  Run `-p taiyaku-library`. The four new cases fail; record the answers they got.
- [ ] **Step 3: Implement.** Add `slug_token` and change `slugify`. Delete `proposals::slugify` and point its caller at `slug_token`. Update the eight literals and reword the six comments.
- [ ] **Step 4: Run** `-p taiyaku-library`, `-p taiyaku-serve` and `-p taiyaku-cli`, then clippy and fmt.
  - Any other test that moves is listed in the report with its old and new literal. A slug can move because two titles now slug alike, and the second takes `-2`.
  - If one of those looks like a real collision rather than a fixture's choice of titles, stop and report.
- [ ] **Step 5: Watched mutations.** Restore after each:
  - The ASCII test (`is_ascii_alphanumeric`) back in `slug_token`: the Lumière and CJK cases fail.
  - Runs left uncollapsed (a `-` for every character): the Spy × Family case fails.
  - The extension kept: the table's two cases fail.
  - The extension compared case-sensitively: `V01.CBZ` fails.
  - The `work` fallback dropped: the `★★★` test fails.
  - The `work` fallback moved into `slug_token`: `accept_refuses_an_edition_of_proposal_whose_suffix_has_no_edition_key_to_give` fails.
- [ ] **Step 6: Commit** "Work slugs keep letters outside ASCII, collapse each run of other characters to one dash and drop the archive's extension, by the edition keys' rule (O262)". Write `$E/T1-report.md`.

---

### Task 2: A non-ASCII slug through the cover redirect, OPDS and the reader route (O262, part 2)

Spec §2 "Where a non-ASCII slug travels", decision 2, and Testing's "The cover redirect", "OPDS" and "The reader route".

**Files:**
- `crates/taiyaku-serve/src/library_api.rs`: `cover` (~391-425). Its `tile` encodes `slug` and `edition` with `uri_component` (~111), as `cover_address` does (~96-103).
- `crates/taiyaku-serve/src/test_support.rs`:
  - A new fixture, `router_over_two_scanned_volumes_with_a_non_ascii_title()`, beside `router_over_two_scanned_volumes_of_decodable_pages` (~1699), and written the same way: a series directory `Kabushikigaisha Magi-Lumière` holding `Kabushikigaisha Magi-Lumière - v01 (2024)` and `… - v02 (2024)`, each a directory of decodable PNG pages.
  - A rule `INSERT INTO rule (ordinal, kind, pattern, match_on, set_edition_key) VALUES (99, 'prefix', '', 'path', 'édition')` goes in before the scan; `derive/tests.rs` ~108 writes the same shape. `router_over_a_written_tree` (~1245) scans and derives inside itself, so split it so that a caller can write between `add_root` and `scan_root`. Its existing callers behave exactly as before.
  - The fixture asserts that the two works' slugs are `kabushikigaisha-magi-lumière-v01-2024` and `…-v02-2024`, that each work's one edition key is `édition`, and that both works sit in one series. A test over it then cannot pass over ASCII.
- `crates/taiyaku-serve/src/library_api/tests.rs`: the cover test, beside `a_series_cover_redirects_to_the_existing_thumbnail_route` (~174).
- `crates/taiyaku-serve/src/opds/tests.rs`: the OPDS test, beside `a_restricted_readers_works_list_counts_only_the_editions_they_may_see` (~1606), which reads `/opds/v1.2/works` through `test_support::get`. If `/opds` refuses the fixture's browser session, wrap the router with `test_support::key_authenticating`, as this file does elsewhere.
- `web/tests/Routes.test.tsx`: the reader-route test. Its `App` mock (~31-49) records one entry per mount through the hoisted `mounts`.

- [ ] **Step 1: The fixture.** Nothing calls it until Step 2. Its own assertions must pass on this branch: they show Task 1's rule reached a scan, and a failure there is a fixture fault, not the defect.
- [ ] **Step 2: Write the serve tests.**
  - **The cover.** `GET /api/library/series/{id}/cover` answers 307. Its `Location`, read with `to_str()`, equals a literal: `/api/works/kabushikigaisha-magi-lumi%C3%A8re-v0N-2024/%C3%A9dition/pages/0/thumb`, where `v0N` is the volume `browse::cover_work` picks. Find which one by reading `cover_work`, and write that one. A `GET` of that `Location` answers 200. Because the header is read with `to_str()` and compared whole, the test fails whether today's header carries raw bytes or is refused.
  - **OPDS.** The `/opds/v1.2/works` body holds `href="/opds/v1.2/works/kabushikigaisha-magi-lumi%C3%A8re-v01-2024"` (and v02's). A `GET` of that href answers 200, and its body names that work's title.
  - Run `-p taiyaku-serve`. The cover test fails on today's code. The OPDS test passes already: it pins `percent_encode`, which already encodes. Record both.
- [ ] **Step 3: Fix `cover`.** Run `-p taiyaku-serve` to green.
- [ ] **Step 4: The reader route** (jsdom).
  - Give the hoisted object in `Routes.test.tsx` an `onMount` hook, default a no-op, which the `App` mock calls with `work` once per mount.
  - The new test stubs `fetch` with `vi.stubGlobal`, as `web/tests/api.test.ts` does, and unstubs it afterwards. It sets the hook to call the real `api.getSidecarWithEtag(work)` (the file's `api` mock spreads the original) and mounts `/read/lumi%C3%A8re-v02`, the percent-encoded path a browser delivers.
  - It asserts exactly one `fetch`, whose URL is exactly `/api/works/lumi%C3%A8re-v02`.
  - It should pass on today's code, because it pins the router's decoding and `seg`. **If it fails, stop and report**: the fix (encoding the links, or decoding in `Reader`) is then a spec decision.
- [ ] **Step 5: Watched mutations.** Restore after each:
  - `cover` formats the raw slug again: the cover test fails.
  - `cover` formats the raw edition key: the cover test fails.
  - `works_feed`'s href without `percent_encode`: the OPDS test fails.
  - `seg` in `web/src/api.ts` returns its argument: the reader test fails, asking for the raw path.
  - `Reader` in `web/src/Routes.tsx` passes `encodeURIComponent(work)` to `App`: the reader test fails, asking for `%25C3`.
- [ ] **Step 6: Run** `-p taiyaku-serve`, clippy and fmt, then the jsdom project, `tsc -b` and biome. Commit "The series cover's redirect encodes a slug and an edition key outside ASCII, and tests pin OPDS and the reader route over one (O262)". Write `$E/T2-report.md`.

---

### Task 3: A backup named for its migration, listed and never pruned (O263, part 1)

Spec §2 "The backup before a migration" (the name, `stamp_of`, `Backup`, `prune`) and decision 8. Pre-flight §2.

**Files:**
- `crates/taiyaku-library/src/backup.rs`:
  - `stamp_of` (~173-187): it answers the stamp and the `N` of a tagged name.
  - `Backup` (~56-64): `before` (New interfaces).
  - `take_named` (~120-167): its `Backup` literal.
  - `list` (~201-249): it fills `before`.
  - `prune` (~265-286): it neither counts nor deletes a backup whose `before` is `Some`. Its doc says why: a cron entry runs `--keep` unattended, and decision 3 says these are never deleted automatically.
- Tests: `crates/taiyaku-library/src/backup/tests.rs`, beside the existing `stamp_of` refusals and prune tests.

**The name** (settled item 4): `library-YYYYMMDDTHHMMSSZ-before-v<N>.db`. `N` is one or more ASCII digits, the first not `0`, fitting a `u32`. `stamp_of` stays the one place that decides what a backup file is. Index the stamp by bytes, so a non-ASCII name cannot panic on a char boundary.

`name_before_migration` is not added here: it would have no caller until Task 4, and clippy refuses dead code.

- [ ] **Step 1: Write the failing tests.**
  - `stamp_of("library-20261110T120000Z-before-v19.db")` answers the stamp and `19`. The plain name answers its stamp and no `N`.
  - Refused (each its own assertion):
    - `…-before-v.db`
    - `…-before-vx.db`
    - `…-before-v+5.db`
    - `…-before-v019.db`
    - `…-before-v19-before-v19.db`
    - `…-before-v1/9.db`
    - `…-before-v4294967296.db`
    - `…-after-v19.db`
  - `list` over a directory holding a tagged file answers it with `before == Some(19)`.
  - **Prune, tagged newest.** Two plain backups and one tagged with the newest stamp; `prune(dir, 1)` deletes the older plain backup, keeps the newer one and keeps the tagged one.
  - **Prune, tagged oldest.** The same with the tagged stamp oldest; the same outcome.
  - Run `-p taiyaku-library`; they fail.
- [ ] **Step 2: Implement.** Run `-p taiyaku-library` to green.
- [ ] **Step 3: Watched mutations.** Restore after each:
  - Any tail accepted after the stamp: the refusals fail.
  - `N` read with `str::parse` alone: the `+5` and `019` refusals fail.
  - `prune` counting tagged backups: the tagged-newest case fails (both plain ones are deleted).
  - `prune` counting and deleting tagged backups: the tagged-oldest case fails.
- [ ] **Step 4: Run** clippy and fmt. Commit "A backup named for the migration it precedes is listed, and `--keep` neither counts nor deletes it (O263)". Write `$E/T3-report.md`.

---

### Task 4: The open backs up before it migrates (O263, part 2)

Spec §2 "The backup before a migration" (the rest) and decisions 3 and 9.

**Files:**
- `crates/taiyaku-library/src/schema.rs`: `pending_migration` beside `migrate` (~71). It reads the version exactly as `migrate_with` does (~87-94): `ensure_meta`, then `version`. It answers `None` for 0, for `SCHEMA_VERSION` and for anything newer, which `migrate` goes on refusing.
- `crates/taiyaku-library/src/backup.rs`:
  - `name_before_migration`, sharing `name_now`'s clock read (~85-92).
  - `before_migration`. Its doc says why it runs on the one connection there is: `take`'s rule against the server's connection is about stalling requests, and nothing is served at open time.
  - The module doc (~1-15) gains that a backup is taken before every migration, under the tagged name, and that `prune` leaves it alone.
- `crates/taiyaku-library/src/lib.rs`: `Library::from_connection` (~345-352), and `open`'s "# Errors".
- Tests: `crates/taiyaku-library/src/schema/tests.rs`, which reaches the private `migrate_with` and `MIGRATIONS`.
- Docs and the Backup tab:
  - `README.md`: the backup block (~202-213), the backup tab's paragraph (~348-352) and the rollback paragraph (~630-634);
  - `crates/taiyaku-cli/src/library.rs`: `backup`'s doc (~99-104);
  - `web/src/Backup.tsx`: the paragraph (~193-198) and the doc bullet (~19-21);
  - `web/src/backup.ts`: the module doc (~4-7);
  - `web/tests/Backup.test.tsx`: `carries all four of C4 §5 sentences` (~377).

**`from_connection`:**
1. `configure`.
2. When `path` is `Some` and `pending_migration` answers `(_, target)`, take `name_before_migration(&conn, target)` and then `before_migration(&conn, path, &name)`. Either error returns from `from_connection`.
3. Then `schema::migrate`.

**`before_migration`'s answers:**
- `take_named`'s `Ok`, and `TakeError::Exists` (spec: a second process in the same second took the same backup), are both `Ok(())`.
- `TakeError::Failed(why)` is `Err` of this sentence: `the backup {name} could not be taken before migrating {db}, so the database was not migrated: {why}`.

**The words:**
- **Backup tab.** The paragraph's text, as drawn: "There is no schedule here. Run taiyaku library backup from cron, with --keep N to prune old ones. Before each migration the server takes one of its own, whose name ends in -before-v and the schema version it migrates to; --keep leaves those alone, so delete them by hand." Keep the `<strong>` on the first sentence, and put `<code>` around `taiyaku library backup`, `--keep N`, `-before-v` and the second `--keep`.
- **Backup tab's doc bullet**, and `backup.ts`'s doc: a cron entry runs `taiyaku library backup` and `--keep` prunes what it took. The server's own backup is taken only before a migration, and `--keep` leaves it alone.
- **The CLI's `backup` doc:** `--keep` never counts or deletes a backup taken before a migration.
- **README backup block.** The `--keep 7` line ends `# ...and delete all but the seven newest, leaving those taken before a migration`. Before the `# restore is out of band` line, add:
  ```
  # an open that migrates the database backs it up first, to
  # <db parent>/backups/library-<stamp>-before-v<N>.db, N being the version it
  # migrates to, and refuses to migrate if that backup fails. --keep never
  # deletes one of those; delete them by hand.
  ```
- **README backup tab's paragraph:** "…and the schedule is the cron entry above; the only backup the server takes by itself is the one before a migration."
- **README rollback paragraph**, replacing "Go back across a migration by restoring … `docker exec taiyaku taiyaku library backup`.":

  > Go back across a migration by restoring the backup the server took before it. An open that migrates first writes `backups/library-<stamp>-before-v<N>.db` beside the database, where `N` is the version it migrated to, and refuses to migrate if that backup fails. The file holds the database as it stood, so an image whose schema version is below `N`, and at least the version the file holds, can open it. `--keep` never deletes one; delete them by hand.

- [ ] **Step 1: Write the failing tests** in `schema/tests.rs`.
  - Add a helper that builds a file database one version short: `Connection::open` on a file in `crate::tests::tempdir(…)`, then `migrate_with(&conn, &MIGRATIONS[..SCHEMA_VERSION as usize - 1])`, then drop the connection.
  - Index from `SCHEMA_VERSION`, never a literal: lanes 2 and 4 each add a migration.
  - **Backup taken.** The helper's database, then `crate::Library::open`. `backup::list(&backup::directory_for(&db))` answers one backup, whose name ends `-before-v{SCHEMA_VERSION}.db` and whose `before` is `Some(SCHEMA_VERSION)`. Opened with a plain `Connection`, it reads version `SCHEMA_VERSION - 1`. The database reads `SCHEMA_VERSION`.
  - **Skips.** A fresh file opened with `Library::open` lists no backup. Opened again (now up to date), it still lists none. Assert after each open.
  - **Newer.** A file opened once, with its `meta` version then set to `SCHEMA_VERSION + 1` through a plain connection. `Library::open` errs with a message containing "newer taiyaku", and no backup is listed.
  - **Failure.** The helper's database, with `directory_for(&db)` made a plain file. `Library::open` errs. The message contains `-before-v{SCHEMA_VERSION}.db` and "so the database was not migrated". The database still reads `SCHEMA_VERSION - 1`.
  - **Exists.** The helper's database, with a file already at `directory_for(&db)/library-20261110T120000Z-before-v{SCHEMA_VERSION}.db`.
    - `crate::backup::before_migration(&conn, &db, <that name>)` on a plain `Connection` answers `Ok(())`.
    - `migrate(&conn)` then brings it to `SCHEMA_VERSION`.
    - The existing file's bytes are unchanged.
  - Run `-p taiyaku-library`; they fail.
- [ ] **Step 2: Implement** `pending_migration`, `name_before_migration`, `before_migration` and the `from_connection` change. Run `-p taiyaku-library` to green.
- [ ] **Step 3: The race.** Run `several_first_opens_of_one_fresh_library_at_once_all_succeed` twenty times in a zsh loop, each run `env -u TAIYAKU_CORPUS cargo nextest run -p taiyaku-library -E 'test(several_first_opens_of_one_fresh_library_at_once_all_succeed)'` into its own log. Record the twenty exit codes. This is a flake probe beside the whole-crate run, not a gate. Any red run stops the task, and the report names it.
- [ ] **Step 4: The docs and the tab.**
  - Write the words above.
  - Extend `carries all four of C4 §5 sentences`:
    - `told` contains "Before each migration the server takes one of its own";
    - `told` contains "--keep leaves those alone";
    - `told` does not contain "only when asked".
  - Run the jsdom project. `Config.browser.test.tsx` draws this disclosure (~5055), so also run the whole browser project, then `tsc -b` and biome.
- [ ] **Step 5: Watched mutations.** Restore after each:
  - The backup taken after `migrate`: Backup taken fails (the copy reads `SCHEMA_VERSION`).
  - No backup taken: Backup taken fails.
  - `pending_migration` answering for `current == SCHEMA_VERSION`: Skips fails on the second open.
  - `pending_migration` answering for 0: Skips fails on the first open.
  - `pending_migration` answering for a newer version, with the backup therefore taken before the refusal: Newer fails.
  - `Failed` warned about and the migration run anyway: Failure fails.
  - `Exists` treated as a failure: Exists fails.
  - The old "only when asked" paragraph restored: the Backup test fails.
- [ ] **Step 6: Run** `-p taiyaku-library` and `-p taiyaku-cli`, then clippy and fmt. Commit "Opening a database that migrates first takes a backup named for the migration, and refuses to migrate without it (O263)". Write `$E/T4-report.md`.

---

### Task 5: The three readers refuse a newer document (O264, part 1)

Spec §2 decision 4 and "Document version guards" (the readers and the read-back); Testing's "Readers" and "Read-back". Pre-flight §3.

**Files:**
- `crates/taiyaku-core/src/sidecar.rs`: `refuse_newer` beside `readable_version` (~552-562), which now calls `refuse_newer("taiyaku", self.taiyaku, VERSION)`. Re-export `refuse_newer` in `crates/taiyaku-core/src/lib.rs`'s `pub use sidecar::{…}` (~26-30).
- `crates/taiyaku-core/src/truth.rs`: `TruthDoc::from_json` (~249-266) asks `refuse_newer("taiyaku_truth", doc.taiyaku_truth, VERSION)` after the parse and before the box check. Add it to its "# Errors".
- `crates/taiyaku-core/src/regions.rs`: `RegionDoc::from_json` (~150-152), with `"taiyaku_regions"`.
- `crates/taiyaku-library/src/librarydoc.rs`:
  - `LibraryDoc::from_json` (~218-220), with `"taiyaku_library"`;
  - `VERSION`'s doc (~38) becomes "bumped for any new field" (decision 7), saying why: an older taiyaku's `hand::save` rewrites the document from its rows and drops a field it has no column for (O264).
- Tests:
  - `crates/taiyaku-core/src/truth/tests.rs`;
  - `crates/taiyaku-core/src/regions/tests.rs`;
  - `crates/taiyaku-library/src/librarydoc/tests.rs`;
  - the read-back test in `crates/taiyaku-library/src/derive/tests.rs`, beside `a_hand_document_that_will_not_parse_is_reported_and_the_scan_goes_on` (~1088).

The words are `refuse_newer`'s (New interfaces). `readable_version`'s message must stay byte for byte as it is. The existing sidecar tests pin it, and they must stay green unedited.

- [ ] **Step 1: Write the failing tests.**
  - **Readers.** For each of the three documents, a minimal valid document at `VERSION` parses. The same at `VERSION + 1` is refused, with the error containing the literal `this document declares taiyaku_truth format version 2, and this build reads 1. Refusing to read it rather than dropping the fields it does not understand and writing them back out as version 1 — upgrade taiyaku instead`. Use the document's own key, and build the version literals from the test's own numbers, not from `refuse_newer`.
  - **Read-back.** Use the model's fixture and steps. At the volume 1 work's `hand::path_for` address, write a document `{"taiyaku_library": 2, "updated": "…", "title": "Hand title"}`, then clear the markers and derive. `unreadable_hand` has one entry, naming the file and containing "declares taiyaku_library format version 2". The work's title is not "Hand title".
  - Run `-p taiyaku-core` and `-p taiyaku-library`; they fail.
- [ ] **Step 2: Implement.** Run both crates and `-p taiyaku-cli` (which reads truth and regions documents) to green.
- [ ] **Step 3: Watched mutations.** Restore after each:
  - `refuse_newer`'s comparison made `>=`: every accept case fails.
  - The call removed from each `from_json` in turn: that document's refuse case fails.
  - The call removed from `LibraryDoc::from_json`: the read-back test also fails.
- [ ] **Step 4: Run** clippy and fmt. Commit "The library, truth and regions documents refuse a version newer than this build reads, in the sidecar's words (O264)". Write `$E/T5-report.md`.

---

### Task 6: `hand::save` leaves a newer hand document alone (O264, part 2)

Spec §2 decisions 5 and 11 and "Document version guards"; Testing's "`hand::save`", "The marker" and "Through the API".

**Files:**
- `crates/taiyaku-library/src/migrate.rs`: `declared_version`, beside `record_absorbed` (~420).
- `crates/taiyaku-library/src/hand.rs`:
  - `save` (~202-290). The guard goes after `path_for` and before the `match` on `path`, and so before both the write and the remove, and before `record_absorbed`. When `path` is `Some(path)` and `declared_version(path, "taiyaku_library")` is above `librarydoc::VERSION`, it returns `Saved { path: Some(path), persisted: false, why: Some(<the sentence>), keeps_documents }`. The no-address branch and the root-keeps-no-documents branch are unchanged (spec gap 4).
  - `save`'s doc gains the guard and why it skips the marker (decision 11): an older binary then goes on reporting the document, and a binary that can read it reads it back.
  - `Saved::why`'s doc (~141-150): it is also this reason.
- Tests:
  - `crates/taiyaku-library/src/hand/tests.rs`, beside `saving_writes_the_document_and_records_the_marker` (~125) and `a_hand_document_withdrawn_removes_the_file_it_left_behind` (~198), over `a_scanned_work_rooted_at_that_keeps_documents` (~117);
  - the marker test in `derive/tests.rs`, beside Task 5's;
  - the API test in `crates/taiyaku-serve/src/metadata/tests.rs`, beside `patching_a_title_writes_the_row_and_the_document` (~164), using `the_hand_document_path` (~52).

**The sentence** (`why`): `{path}: a newer taiyaku wrote this document (taiyaku_library {declared}; this build writes {VERSION}), so it was left as it is`.

Every caller of `save` gets the guard: the metadata routes, the series fan-out and the fold's carry in `derive`.

- [ ] **Step 1: Write the failing tests.**
  - **`save`, with content.** A newer document (`taiyaku_library: 2`) at the work's address. Set a hand title in the rows, then `hand::save`:
    - the file's bytes are unchanged;
    - `persisted` is false;
    - `why` contains "a newer taiyaku wrote this document";
    - the row holds the hand title.
  - **`save`, empty.** A newer document at the address, and rows that project empty, then `hand::save`. The same four assertions, with the rows still empty.
  - **The marker.**
    - Derive over a newer document, so the read-back refuses it.
    - Give the work a hand title in its rows, so `save` projects content and is guarded, then call `hand::save`.
    - Derive again: `unreadable_hand` still lists the document.

    With an empty projection, the mutant below would record `false` and pass, so the hand value is required.
  - **Through the API.** Write a newer document at `the_hand_document_path(&state)`, then PATCH the work's title. The answer has `persisted: false`, and its `why` array's one entry contains "a newer taiyaku wrote this document". The answer's title is the new one. The file's bytes are unchanged.
  - Run `-p taiyaku-library` and `-p taiyaku-serve`; they fail.
- [ ] **Step 2: Implement.** Run both crates to green.
- [ ] **Step 3: Watched mutations.** Restore after each:
  - The guard inside the write branch only: the empty case fails (the file is removed).
  - The guard removed: both `save` cases and the API test fail.
  - The guard calling `record_absorbed(…, true)` before it returns: the marker test fails.
- [ ] **Step 4: Run** clippy and fmt. Commit "An older taiyaku leaves a hand document a newer one wrote as it is, and says so in the edit's answer (O264)". Write `$E/T6-report.md`.

---

### Task 7: An in-place export keeps a newer sibling (O264, part 3)

Spec §2 decision 10 and "Document version guards" (the export); Testing's "Export". Spec gap 1.

**Files:**
- `crates/taiyaku-library/src/export.rs`:
  - `VERSIONED` and `newer_sibling` (New interfaces), beside `stem_of` (~463).
  - In `work`, the write loop (~318-332) and the removal loop (~339-365) each ask, in place only and for a file that is there, `newer_sibling(&path, suffix)`. Ask before the `absorbed_for` check. A newer file is pushed to `kept` and left alone. `--dir` stays exempt.
  - `Exported::kept`'s doc (~44-49): kept because this library has never had the content, or because a newer taiyaku wrote it.
  - `work`'s "What an export will not touch" (~185-217) gains the fourth shape.
- `crates/taiyaku-cli/src/library.rs`: the kept line (~1363-1369) and its comment (~1348-1362).
- `crates/taiyaku-cli/tests/cli.rs`: the kept line's pin (~1066-1072).
- `docs/sidecar.md`: "Forward compatibility" (~527-640). One paragraph after the last one, "The rows are authoritative for a document they have actually held…". It says:
  - the sibling documents carry their own numbers (`taiyaku_library`, `taiyaku_truth`, `taiyaku_regions`), and a build refuses one newer than its own, in the sidecar's words;
  - an older taiyaku neither writes over nor removes a newer hand document, and says so in the edit's answer;
  - `--in-place` keeps a newer sibling and names it among the files it kept;
  - `taiyaku_library` is bumped for any new field, because an older taiyaku rewrites that file from its own rows.
- Tests: `crates/taiyaku-library/src/export/tests.rs`, beside `clearing_the_labelling_and_exporting_removes_the_stale_truth_document` (~778) and `hand_fixed_metadata_is_written_and_clearing_it_removes_the_document` (~595), which arm the same markers.

**The CLI's kept line:** `kept {} (this library has never read it, or a newer taiyaku wrote it, so it was not overwritten; move it aside and export again to replace it with what this library holds — which is no file at all where it holds nothing)`.

- [ ] **Step 1: Write the failing test** (spec Testing, "Export").
  - Set up a work imported from a `taiyaku.json`, with a hand value and truth rows. An in-place export writes `library.json` and `truth.json`, recording both absorbed. Assert both markers through `migrate::absorbed`, so the setup cannot pass unarmed.
  - Remove the truth rows. Write newer documents in place: `taiyaku.json` declaring `taiyaku: 2`, `library.json` declaring `taiyaku_library: 2` and `truth.json` declaring `taiyaku_truth: 2`.
  - The in-place export under test: `kept` is exactly those three paths in the fixed order (the written group, then the stale group). Each file's bytes are unchanged.
  - Run `-p taiyaku-library`; it fails.
- [ ] **Step 2: Implement** the guard and the docs. Change the CLI line and its pin. Run `-p taiyaku-library` and `-p taiyaku-cli` to green.
- [ ] **Step 3: Watched mutations.** Restore after each:
  - The write loop not asking `newer_sibling`: the test fails on `taiyaku.json` and `library.json`.
  - The removal loop not asking: the test fails on `truth.json`.
  - The old CLI line restored: the `tests/cli.rs` test fails.
- [ ] **Step 4: Run** clippy and fmt. Commit "An in-place export keeps a sibling document a newer taiyaku wrote, and names it among the files it kept (O264)". Write `$E/T7-report.md`.

---

### Task 8: The stamp stages under a dot name (O185)

Spec §2 decision 6 and "O185: the staging name"; Testing's "O185". Pre-flight §4.

**Files:**
- `crates/taiyaku-library/src/stamp.rs`: `staging_path` (~443-455) and its doc. `v01.cbz` stages as `.v01.cbz.taiyaku-stamp` in the same directory. The doc says why:
  - a dot name is skipped by both places the walk asks (`scan::measure_dir`, and `Excluder::keeps_out` with `KeptOut::DotName`);
  - the same directory keeps the rename a rename, so O104's syncs stand;
  - the suffix still follows the whole name, so `v01.cbz` and `v01.cbr` do not collide.
- Tests: `crates/taiyaku-library/src/stamp/tests.rs`:
  - `a_rewrite_returns_the_new_digest_and_leaves_no_temp_behind` (~517) also asserts that `super::staging_path(&path)`'s file name starts with `.`. Its `contains("taiyaku-stamp")` filter passes with either name.
  - A new scan test: a directory with `v01.cbz` written by `crate::scan::tests::write_archive`, and a valid archive written the same way at `super::staging_path` of it, beside it. Scan it with `crate::scan::add_root` and `scan_root`. The root's `scan_entry` rows number exactly one.

- [ ] **Step 1: Write the tests.** Run `-p taiyaku-library`; both fail (two entries, and an undotted name).
- [ ] **Step 2: Implement.** Run to green.
- [ ] **Step 3: Watched mutation.** `staging_path` back to the undotted name: both tests fail. Restore.
- [ ] **Step 4: Run** clippy and fmt. Commit "A ComicInfo write-back stages under a dot name, so a scan never reads one a cut stamp left behind (O185)". Write `$E/T8-report.md`.

---

### Task 9: The closes in `docs/status.md`

Read `docs/status.md`'s Open items section: the four rows (O185 ~1794, O262-O264 ~1871-1873), the notes bullets for O262-O264 (~4181-4190) and the count sentences. Rows are long, so read them with `cut -c1-2000`.

- [ ] **Step 1: Close O262, O263, O264 and O185** with the five-part edit:
  1. The row's checkbox.
  2. The row's text, rewritten to say what was done, by symbol and with no line numbers. Each close states its residuals:
     - **O262:**
       - an NFD filename and its NFC twin slug differently, since no normalisation crate is in the workspace;
       - a freed slug can be minted again, and lane 4's removal is a new way to free one;
       - suffixes follow the order works arrive in;
       - a dev database keeps its old slugs until rebuilt;
       - a derived title keeps its extension;
       - `works::work_id` keeps its ASCII rule.
     - **O263:**
       - a racing open of a fresh database can back up a half-migrated one (settled item 3);
       - a second process in the same second may snapshot after the first migrated;
       - an open that finds the backup's name already taken migrates at once, so if the other open's copy under that name then fails, that migration ran without a good backup;
       - there is no retention rule and no restore route.
     - **O264:**
       - the read-back runs only while the marker is unset, so a newer document beside an absorbed work is reported by nothing;
       - spec gap 4's off-root branch;
       - `save` does not carry unknown keys (the bump rule covers that);
       - Part E's flush will need the guard;
       - the notes version is lane 2's.
     - **O185:**
       - a stamp cut while staging leaves a hidden full-size copy until that archive is stamped again;
       - a leftover under the old name is still scanned.
  3. The notes bullets. O262-O264's bullets change from "Open." to their closes. O185's close is added beside them (spec gap 5).
  4. The "N are now closed — leaving M" sentence (~1482): add the four O-numbers in order and move both counts.
  5. The "What remains: the … open items" line (~4713). Grep the sentence opener and read the next line too, in case the count word wraps.

  Derive both counts from the table, not by arithmetic on the old sentence:
  - `grep -a -c '^| O[0-9]' docs/status.md`;
  - `grep -a -c '^| O[0-9]* | \[x\]' docs/status.md`;
  - `grep -a -n -A1 '^What remains:' docs/status.md`.

  If this branch rebases onto a lane that moved them, redo them then.
- [ ] **Step 2: The Rust total**, in its two places: "Six crates, N tests" (~179) and "of the N are standing checks" (~256). Measure it:
  - run `builtin cd "$W" && env -u TAIYAKU_CORPUS cargo nextest run --workspace --test-threads=8 >| "$E/T9-workspace.log" 2>&1`, which writes `target/nextest/default/junit.xml`;
  - then run `node scripts/status-test-count.mjs target/nextest/default/junit.xml docs/status.md` from `$W`. It names the count it read when the figure is wrong.

  Set both places and run the script again until it passes.
- [ ] **Step 3: The web counts.** Run the node and jsdom projects. Set "plus N in `web/`'s TypeScript — X in the `node` project and Y in the `jsdom` one" from those two runs' summary lines; do not add. The browser figure is unchanged: no browser test was added.
- [ ] **Step 4: Sweep.** Grep each bare O-number (`O185`, `O262`, `O263`, `O264`) across `docs/`, `web/src/`, `crates/` and `README.md`, and read every hit. Prose elsewhere that speaks of one as open is corrected. A history document under `docs/superpowers/` stays as written.
- [ ] **Step 5:** Commit "Close O185, O262, O263 and O264 in status.md". Write `$E/T9-report.md`.

---

### Finish

1. **Whole-branch review** of `main..HEAD`, in three lanes, each against spec §2, the rulings and this plan's settled items:
   - (a) slugs and O185: Tasks 1, 2 and 8;
   - (b) the backup: Tasks 3 and 4;
   - (c) the version guards and the closes: Tasks 5, 6, 7 and 9.

   One fix wave goes back to the implementers by SendMessage. After fixing a finding, grep for its shape, not its function. Review the fixes too.
2. **SQLite 3.40.1.** Task 4's open path now runs `VACUUM INTO` before any migration. Run `-p taiyaku-library -p taiyaku-serve -p taiyaku-cli` in `rust:1-bookworm` with the recipe in `docs/superpowers/plans/2026-10-06-libraries-f2a-admin.md` (~7351-7371), from `$W`, logging to `$E/bookworm-<hash>.log` and `.exit`. `stamp::tests::a_rewrite_syncs_…` and `opds::tests::the_work_feed_stats_its_members_on_the_blocking_pool` fail there by design (no `strace`); anything else is real. `target/bookworm/` is build cache: leave it.
3. **Final gate** on the final commit, at load < 24. Read the `.exit` file.
4. **The drive.** The scratch directory `S` is outside the repository (the session scratchpad).
   - **Before:**
     - `sha256sum /home/davey/temp/manga-test/library.db`, and `touch "$S/marker"`;
     - copy two volumes each of `Volumes/Kabushikigaisha Magi-Lumière` and `Volumes/Spy × Family` into `$S/tree/`, under their series directories' own names. Use `cp -r`: real copies, never hard links. Never scan the test library itself: the walk's watch probe writes into its root.
   - **Build.** `cargo build --release -p taiyaku-cli` in `$W`, and copy the binary into `$S`. `node "$W/web/node_modules/vite/bin/vite.js" build "$W/web" --outDir "$S/dist" --emptyOutDir`.
   - **A fresh database**, so the slugs are this branch's:
     - `$S/taiyaku library scan --add "$S/tree" --db "$S/library.db"`;
     - `library user add local`, then `user promote local --role admin`. Feed the password `pw` as `printf 'pw\npw\n' |`, which is how the two prompts read it.
   - **Serve.** Check `ss -lptn 'sport = :4391'` is free. Serve with `--db "$S/library.db" --cache "$S/cache" --web "$S/dist" --addr 127.0.0.1:4391 --insecure-cookies`. Start it from the controller; an agent that starts it must never restart it.
   - **With a real browser, at 1280 and 390 px:**
     - both series' covers draw (the redirect);
     - a Lumière volume opens from its tile at its new slug (the shape of `kabushikigaisha-magi-lumière-v02-2024`; the copied files' names decide the rest) and its pages draw;
     - reloading that address from the address bar opens the same volume;
     - Spy × Family's slug holds no run of dashes.
   - **OPDS:** `curl -u local:pw http://127.0.0.1:4391/opds/v1.2/works` shows percent-encoded hrefs, and following one answers 200.
   - **The Backup tab:**
     - the new paragraph reads as Task 4 wrote it;
     - take a backup, and copy it in `$S/backups/` to the same stamp with `-before-v<SCHEMA_VERSION>` added;
     - reload: both are listed, and the tagged one downloads with the same bytes;
     - `$S/taiyaku library backup --keep 0 --db "$S/library.db"` removes the plain backups and leaves the tagged one.

     The open-time backup itself needs an older database, which needs an older build. Task 4's tests prove it; the drive does not.
   - **After:** stop the server by PID and check the port again. `find /home/davey/temp/manga-test -newer "$S/marker"` answers nothing, and the sha256 is unchanged. Then delete `$S`.
5. **Claim check** of the closes, this plan's figures and the PR body. Put the PR body in pass 1, and scope later passes to the correction commit.
6. **The PR body** at `$E/pr-body.md`, with the title as its first line, measured under 255 characters. Ask the user before pushing.
