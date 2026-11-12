# Lane 2 of the pre-deploy design: anchors (O244) and the notes document's version — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A reading position and a note name the page they were written on, by the member's digest, beside the slot they already hold. Every write fills that anchor and every read answers the slot the anchor names in the edition's current layout. A position whose page is gone falls back to its stored slot, clamped; a note keeps its stored slot and is answered `stale`. The notes document gains a version, `taiyaku_notes: 1`. O244 narrows to the removal rehearsal's count. No client behaviour changes.

**Architecture:**
- Task 1: migration 020 adds four nullable columns to `progress` and `note`, and this plan goes into the branch.
- Task 2: `taiyaku-core` maps a slot to the member page it names and back.
- Task 3: the store's layout query, the resolution rules, positions written with their anchor and read resolved.
- Task 4: `Note` gains `stale`, and `same_anchor` moves into the library.
- Task 5: notes read resolved, and `put_notes` keeps the anchor of a note that has not moved (trap 1).
- Task 6: the fold carries the anchor columns (trap 2) and plans from resolved slots.
- Task 7: `stale` at the edges: the notes `PUT`, export and import.
- Task 8: the notes document's version. **Waits for lane 1 on `main`; rebase first.**
- Task 9: O244 narrowed in `docs/status.md`.
- Then the Finish.

**Tech Stack:** Rust (`taiyaku-core`, `taiyaku-library`, `taiyaku-serve`; `taiyaku-cli` for struct literals in its tests), SQLite through `rusqlite` (system library: 3.53 here, 3.40.1 in CI and the image).

**Spec:** `docs/superpowers/specs/2026-11-10-pre-deploy-design.md` (#231, main 10ae1e74): §1 (lanes, merge order, shared files) and §3 "Lane 2: anchors (O244)" in full. Sections are cited as "spec §3 <heading>" and its decisions as "decision n". Pre-flight: `.superpowers/sdd/pre-deploy-evidence/o244-survey.md` (local, read at 6ecd66e3). Locations are by symbol; line numbers are hints taken at 10ae1e74.

**The owner's rulings of 2026-11-10** (binding; do not re-open; `.superpowers/sdd/pre-deploy-evidence/rulings.md`):
- **Approach A.** `progress` and `note` keep `slot` and gain an anchor beside it (digest, occurrence, page in the member, half), filled on every write. Reads resolve the anchor to a slot and fall back to the stored slot.
- **A lost file.** A position falls back to its stored slot clamped into range. A note is kept at its stored slot and the API marks it `stale`, with no new UI now. The row is never rewritten, so the anchor resolves again if the file returns.
- **A moved file.** A row stays with its own edition and resolves as a lost file. The fold keeps its explicit carry.
- **The web sends anchors in a follow-up PR**, which closes O240. This lane changes no client behaviour.
- **The notes document gets a version**, `taiyaku_notes: 1`, in this lane.
- The spec's own calls (decisions 6 to 11) are final as written.

**What the plan settles** (the controller's calls; told to the owner, not rulings):
1. **Names.** The core's page anchor is `MemberPage`. Mapping a slot to it is `Edition::member_page`, and back is `Edition::slot_of`, which answers `SlotOf::Exact` or `SlotOf::PageOnly` (decision 6's case), so the store can treat a position and a note differently there.
2. **Where the rules live.** The core is the one place a slot and an anchor are mapped to each other. `state::resolve_position` and `state::resolve_note` apply decisions 2, 6 and 10 and the NULL rule (the table in Task 3). `state::progress`, `state::notes`, `state::put_notes` and the fold call them. Serve resolves nothing.
3. **The layout** is one query, `project::layout`: one edition's members (`sha256` and `pages` in ordinal order), its spread flags and the work's reading direction. Credit flags are not read; they do not change slots.
4. **The edge arms.** Decision 10 is asked first: a row on an edition key the work no longer has answers its stored slot, unclamped, and a note there is `stale`, whatever its anchor. A NULL anchor answers the stored slot, unclamped, and a note with one is not `stale`. An unresolved position on an edition with no slots answers its stored slot, since there is nothing to clamp into.
5. **A partial set of anchor columns is refused on read**, as `state::region_from_columns` refuses a partial region. The one writer cannot make one.
6. **No all-or-none CHECK in the migration.** The spec puts the rule on the one writer, `state::at_columns`; a CHECK across columns would need both tables rebuilt.
7. **The notes `PUT`'s tag and answer.** The tag is taken from the stored document before `stale` is cleared, as the `GET` takes it. The answer is read again from the store after the write. See Spec gaps 1.
8. **The fold resolves against the layouts `plan_slots` read**, held in `SlotPlan`. See Spec gaps 3.
9. **O78's edge** (spec §3 "Edges the plan settles"). The anchor columns do not join the identical-note comparison. Two copies are compared at their resolved, re-placed slots, which is what their anchors say. A target copy with a NULL anchor and a source copy with one, naming the same page, are one note; the target's copy stays, as `drop_identical_notes` already decides.
10. **`stale` is cleared by `Notes::clear_stale`**, called by the notes `PUT` (on the incoming document and on the stored one it compares against) and by export. `library import` needs no call (Spec gaps 2).
11. **A consequence, stated.** A layout change that moves a note's resolved slot changes that work's notes `ETag` for every account, so a tab open from before is answered 412 and takes O240's merge path, which this lane leaves as it is.
12. **The notes version.** `taiyaku_notes` is `Notes`' first field and is always written; `NOTES_VERSION` lives in `taiyaku-library`'s `lib.rs`; the refusal's words follow `Sidecar::readable_version`'s.

**Spec gaps** (the spec is silent or would be wrong against the code; settled here, told to the owner):
1. **The notes `PUT`'s answer while a note is stale.** `routes::put_notes` answers `notes::tagged_for(&merged, &view)`, built from the document it wrote, and its doc comment promises that tag is the next read's. Once `stale` is cleared on the way in, `merged` carries no `stale`, while the next `GET` does, so the tags differ and the client's next write is answered 412. Also, the tag the `If-Match` is checked against must be taken before the clearing, or every `PUT` over a stale note is answered 412. Settled: item 7 above, with a test.
2. **"`library import` clears it too."** No column holds `stale`, so `state::put_notes` cannot store it and the import has nothing to clear; a test of a clear there could not fail. No code is added to `migrate::apply_notes`.
3. **"through the resolution `state::` uses" for the fold.** `fold::absorb` runs `write_spreads` before `resequence_rows(into_work, …)`, so a row of the target resolved by a fresh store read there would be resolved against the flags the fold just wrote. Settled: item 8 above. The existing `the_twins_own_rows_are_placed_under_the_flags_it_takes` should turn red against that defect once `put_progress` writes anchors (inferred from reading; Task 6 Step 4 watches it).

**New interfaces this plan names** (first written here):
- `taiyaku-core`, in `sidecar.rs`, exported from `lib.rs` beside `Edition`:
  ```rust
  /// Where a reading position or a note stands: a page of one member of an
  /// edition, held by the member's digest so the members around it can change.
  #[derive(Debug, Clone, PartialEq, Eq)]
  pub struct MemberPage {
      pub sha256: String,
      /// Which of the edition's members with this digest, from 0 in member order.
      pub nth: u32,
      /// The page within that member, from 0.
      pub page: u32,
      /// The half of a spread; `None` for a whole page.
      pub half: Option<Half>,
  }

  /// Where a `MemberPage` stands in an edition's slots now.
  #[derive(Debug, Clone, Copy, PartialEq, Eq)]
  pub enum SlotOf {
      /// The slot naming that page and half.
      Exact(u32),
      /// The page is there and its half is not: a half on a page that is no
      /// longer a spread, or a whole page that now is one. The page's first
      /// slot in reading order.
      PageOnly(u32),
  }

  impl Edition {
      pub fn member_page(&self, slot: u32, reading: Direction) -> Option<MemberPage>;
      pub fn slot_of(&self, at: &MemberPage, reading: Direction) -> Option<SlotOf>;
  }
  ```
- `taiyaku-library`:
  - `project::layout(conn: &Connection, work_id: i64, key: &str) -> Result<Option<(Edition, Direction)>, String>`, `pub(crate)`; `None` where the work holds no edition `key`.
  - `state::resolve_position(layout: Option<(&Edition, Direction)>, stored: u32, at: Option<&MemberPage>) -> u32` and `state::resolve_note(…same…) -> (u32, bool)` (the bool is `stale`), both `pub(crate)`.
  - `state::at_columns`, private: the one function that turns an `Option<&MemberPage>` into the four column values. `state::at_from_columns`, `pub(crate)`: the four columns back into `Option<MemberPage>`, refusing a partial set.
  - `Note::same_anchor(&self, other: &Note) -> bool` (edition, slot and region equal), `pub`.
  - `Note.stale: bool`, `#[serde(default, skip_serializing_if = …)]`: read as `false` when absent, written only when true.
  - `Notes::clear_stale(&mut self)`.
  - Task 8: `pub const NOTES_VERSION: u32 = 1;`, `Notes.taiyaku_notes: u32` (first field, `#[serde(default = …)]` giving `NOTES_VERSION`, always written), `Notes::readable_version(&self) -> Result<(), String>`.
- The schema: `at_sha256 TEXT`, `at_nth INTEGER`, `at_page INTEGER`, `at_half TEXT CHECK (at_half IN ('left','right'))` on `progress` and on `note` (Task 1 has the SQL).

## Global Constraints

- **Worktree and paths.** This lane runs in its own git worktree, `W` (an absolute path the controller gives each dispatch), on branch `pre-deploy-anchors` cut from main 10ae1e74. Four other lanes run at once in their own worktrees. Evidence: `E=/home/davey/projects/taiyaku/.superpowers/sdd/pre-deploy-evidence/lane-2-anchors`, in the main checkout, so reports survive the worktree. Every command runs against `$W` by absolute path: `git -C $W …`, `cargo … --manifest-path $W/Cargo.toml`. Never `cd` (it is zoxide here). A command that must run from the checkout is written `builtin cd $W && …` inside the one call.
- **Builds.** Never set `CARGO_TARGET_DIR`: each worktree builds into its own `$W/target`. `$W/web/node_modules` is a real copy, never a symlink. Never `pnpm exec`; call `./node_modules/.bin/<tool>` directly. No task here changes a web file; only the gate runs the web job.
- **Commits.** One commit per task, its subject naming O244 (Task 8 has no row and names the spec's decision 5). Never `git commit --amend`. Check each new hash is an ancestor of `HEAD`.
- **Comments.** A code comment says why: the reason, the invariant, what breaks if it changes. It never carries history or measurements, and it may point at an O-number. `scripts/check-comments.mjs` enforces part of this in the gate, including each area's comment share in `scripts/comment-budget.json`. If the gate fails on a share, follow `docs/ci.md`, "The comment guard", and say so in the report. The migration's SQL comments follow the same rule.
- **Attribution.** None anywhere: code, comments, docs, commits, PR body.
- **Gate.** The repo's own `scripts/check-all.sh`, never a copy or a modified one. It runs from the checkout's top level (`git rev-parse --show-toplevel`), so:
  `builtin cd $W && env -u TAIYAKU_CORPUS NEXTEST_TEST_THREADS=8 RUST_TEST_THREADS=8 ./scripts/check-all.sh >| $E/<name>.log 2>&1; echo "exit $?" >| $E/<name>.exit`
  Wait for load < 24 (`cat /proc/loadavg`) and record it. Read the `.exit` file, never the harness's exit code. Quote the `all green` line. Never pipe the gate through `tail`.
- **Rust, per task.**
  - `cargo nextest run --manifest-path $W/Cargo.toml -p <crate> --test-threads=8 >| $E/T<n>-<crate>.log 2>&1; echo "exit $?" >| $E/T<n>-<crate>.exit` for every crate the task touches, and `taiyaku-library` and `taiyaku-serve` from Task 3 on (both read `state::`). Tasks 4 and 8 also run `taiyaku-cli`: its tests build `Note` and `Notes`.
  - `cargo clippy --manifest-path $W/Cargo.toml --workspace --all-targets --all-features -- -D warnings`, then `cargo fmt --manifest-path $W/Cargo.toml --all -- --check`.
  - Read the summary lines from the logs. Never trust a figure the log does not show.
- **The gate also checks** rustdoc intra-doc links (`broken_intra_doc_links` denied), so every new ``[`link`]`` must resolve; a per-file coverage floor, so every new branch needs a test that reaches it; and, as an advisory pass, clippy pedantic, where a new bare `as` cast between the fold's `i64` slots and `u32` shows. Convert with `try_from`.
- **SQLite 3.40.1.** This lane adds SQL beyond a plain `SELECT` (a migration, an upsert naming new columns, an `INSERT … SELECT` naming them). Run the crates in `rust:1-bookworm` at the two steps that name it (Task 1, Finish):
  ```sh
  builtin cd $W && H=$(git rev-parse --short=8 HEAD) && mkdir -p "target/bookworm/src-$H" target/bookworm/target target/bookworm/cargo-home && git archive HEAD | tar -x -C "target/bookworm/src-$H" && docker run --rm --user "$(id -u):$(id -g)" -v "$PWD/target/bookworm/src-$H:/src" -v "$PWD/target/bookworm/target:/t" -v "$PWD/target/bookworm/cargo-home:/c" -e CARGO_TARGET_DIR=/t -e CARGO_HOME=/c -e HOME=/c -e RUSTUP_TOOLCHAIN=1.97.1-x86_64-unknown-linux-gnu -w /src rust:1-bookworm cargo test --locked --no-fail-fast -p taiyaku-library -p taiyaku-serve -p taiyaku-cli -- --test-threads=6 >| $E/<name>.log 2>&1; echo "exit $?" >| $E/<name>.exit
  ```
  Check the toolchain name inside the image first (`rustup toolchain list`). Two tests fail there by design, `stamp::tests::a_rewrite_syncs_…` and `opds::tests::the_work_feed_stats_its_members_on_the_blocking_pool` (they need `strace`); anything else failing is real. `target/bookworm/` is gitignored build cache; ask before deleting it.
- **Watched mutations.** Every new or tightened assertion gets one:
  - Put the named defect back by hand and watch the test fail.
  - Restore, and check with `git -C $W diff` that the restore is complete.
  - Record each in the task report: the defect, the failing test, and the assertion's message.
  - Check that the fixture can tell right from wrong before trusting the test: a clamp test whose slot is in range, or a held-anchor test whose recomputed anchor is NULL, proves nothing. The steps below name each fixture's constraint.
- **Reads go through `state::` and the routes.** A test reads a position's or a note's slot through `state::progress`, `state::notes` or a route, never by raw SQL: a raw read of `slot` passes whether resolution works or not. Only the checks of the written columns themselves (the four anchor columns, and the fold's re-placed fallback in Task 6) read columns. A fixture may set up a layout or an anchor by SQL.
- **Existing tests that move.** An existing test going red under this lane is a finding first. Read it against decisions 1 to 11. Change its expectation only where a decision makes the new answer right, never to make it pass, and list each one in the report with the decision. Rows written by a raw `INSERT INTO progress|note` leave the anchor NULL and answer as before.
- **Sentences are literals.** A test asserts a user-facing sentence written out in the test, never one computed through the helper under test.
- **Pre-existing failures.** A failure is pre-existing only if the same command fails at main 10ae1e74, not at another commit of this branch. Report it; do not fix it.
- **Shared files** (spec §1, "Coordination"). Lane 1 edits `crates/taiyaku-library/src/lib.rs` (`Library::from_connection`), `schema.rs` and `schema/tests.rs` (its backup tests), `state.rs` (a doc comment) and `export.rs` (its guard). Lane 4 adds a migration. Keep this lane's edits inside its own functions so conflicts stay textual. No field is added to `ServeState`.
- **The migration's number is provisional.** 020 is the next free number at 10ae1e74. At rebase, if another lane's migration landed first, take the next free one: rename the file, its `include_str!`, `SCHEMA_VERSION`, the version test's name and literal, and the migration test's `MIGRATIONS[..N]` prefix, its name (`migration_20_gives_progress_and_note_a_page_beside_the_slot`), its `version(&conn)` literal `20`, its doc line ("Migration 20"), its seed `SCHEMA_19_POSITION_AND_NOTE` and the `"migrates to 19"`, `"seeds a schema-19 store"` and `"migrates to 20"` messages, then run the schema tests again.
- **`docs/status.md` is edited only in Task 9.**
- **Reports.** `$E/T<n>-report.md`, written by Bash (`cat >| … <<'EOF'`), never inside the worktree and never under `target/`. The Write tool is refused under `.superpowers/`. Every count in a report quotes its log line.

---

### Task 1: The plan in the branch, and migration 020 (O244)

Spec §3 "The schema". Survey §6.

**Files:**
- `docs/superpowers/plans/2026-11-10-pre-deploy-anchors.md`: a copy of `/home/davey/projects/taiyaku/.superpowers/sdd/pre-deploy-evidence/plans/2026-11-10-pre-deploy-anchors.md`.
- New `crates/taiyaku-library/src/schema/020_anchors.sql`.
- `crates/taiyaku-library/src/schema.rs`: `SCHEMA_VERSION` (~28), `MIGRATIONS` (~36), appended.
- `crates/taiyaku-library/src/schema/tests.rs`:
  - `the_schema_version_is_nineteen` (~750) becomes `…_twenty` and pins 20.
  - `migration_19_links_each_directory_to_its_library_and_drops_the_column` (~2319) calls `migrate(&conn)` and pins version 19 (~2329-2333). It migrates with `migrate_with(&conn, &MIGRATIONS[..19])` instead.
  - The new test goes after it. `rows_of` (~1229) and `columns_of` (~2257) are the helpers.

**The migration** (first written here):
```sql
-- O244: a reading position and a note name the page they stand on, beside
-- the slot, so a change to the edition's members or spread flags does not
-- move them to another page. `at_sha256` is the member's digest, `at_nth`
-- which of the edition's members with that digest it is (from 0), `at_page`
-- the page within it, and `at_half` the half of a spread, NULL for a whole
-- page. `state::` writes all four on every write and resolves them on every
-- read; `slot` stays the answer for a row whose page is gone.
--
-- **The first three are all set or all NULL.** NULL means the row named no
-- page when it was written (past the end) or was written before this
-- migration. `state::at_columns` is the one place the four are made, which
-- is what keeps that rule: a CHECK across columns needs the table rebuilt,
-- and the fold copies the four as they are.
--
-- ADD COLUMN rebuilds neither table, so `note_by_work` stands.
ALTER TABLE progress ADD COLUMN at_sha256 TEXT;
ALTER TABLE progress ADD COLUMN at_nth INTEGER;
ALTER TABLE progress ADD COLUMN at_page INTEGER;
ALTER TABLE progress ADD COLUMN at_half TEXT CHECK (at_half IN ('left', 'right'));
ALTER TABLE note ADD COLUMN at_sha256 TEXT;
ALTER TABLE note ADD COLUMN at_nth INTEGER;
ALTER TABLE note ADD COLUMN at_page INTEGER;
ALTER TABLE note ADD COLUMN at_half TEXT CHECK (at_half IN ('left', 'right'));
```

- [ ] **Step 1: The plan.** Copy this file to `$W/docs/superpowers/plans/2026-11-10-pre-deploy-anchors.md`.
- [ ] **Step 2: Write the failing schema test** (e.g. `migration_20_gives_progress_and_note_a_page_beside_the_slot`).
  - Build a store at 19 with `migrate_with(&conn, &MIGRATIONS[..19])`, foreign keys on as the migration 19 test sets them.
  - Seed what a `progress` row and a `note` row need (a user, a work), then one row of each by raw `INSERT`.
  - Run `migrate(&conn)` and assert:
    - the version is 20;
    - each table's columns end with `at_sha256, at_nth, at_page, at_half`;
    - the seeded rows read as they were, with the four NULL;
    - `note_by_work` is still in `sqlite_master`;
    - on each table, an `UPDATE` setting `at_half = 'middle'` is refused and one setting `'left'` is accepted.
  - It fails before the SQL exists. Record how.
- [ ] **Step 3: Add the migration**, append it to `MIGRATIONS`, set `SCHEMA_VERSION` to 20, and fix the two pins above.
- [ ] **Step 4: Watched mutations.**
  - Drop the CHECK from `note`'s `at_half`: the refusal assertion fails on `note`.
  - Drop `progress`'s `at_page` line: the columns assertion fails.
- [ ] **Step 5: Run** the `taiyaku-library` tests, clippy and fmt.
- [ ] **Step 6: SQLite 3.40.1.** Run the bookworm recipe into `$E/T1-bookworm.log`. An `ADD COLUMN` with a CHECK on `progress`, a `WITHOUT ROWID` table, is what 3.53 and 3.40.1 could disagree on. Quote the `test result` lines.
- [ ] **Step 7: Commit** the plan and the migration: "Migration 020 gives a reading position and a note a page beside the slot (O244)". Write `$E/T1-report.md`.

---

### Task 2: The core maps a slot to its member page and back (O244)

Spec §3 "The core", decision 6, decision 11.

**Files:**
- `crates/taiyaku-core/src/sidecar.rs`: `MemberPage` and `SlotOf` beside `Edition` (~211); the two methods beside `Edition::locate` (~253) and `Edition::slots` (~270).
- `crates/taiyaku-core/src/lib.rs`: the `pub use sidecar::{…}` list (~26).
- `crates/taiyaku-core/src/sidecar/tests.rs`: beside the `locate` test (~1939).

**Behaviour:**
- `member_page(slot, reading)`: `None` past the end. Otherwise the slot's page and half from `slots(reading)`, the member and the page within it from `locate(page)`, the member's `sha256`, and `nth`, the count of earlier members with that digest.
- `slot_of(at, reading)`:
  - `None` where the edition holds fewer than `at.nth + 1` members with `at.sha256`, or where `at.page` is at or past that member's `pages`.
  - Otherwise the flattened page is the earlier members' pages plus `at.page`.
  - `Exact` with the slot that is `Slot { page, half: at.half }` in `slots(reading)`.
  - Failing that, `PageOnly` with the page's first slot in reading order (decision 6).
- Both doc comments say why the occurrence is needed: one archive can stand twice in an edition (derive keeps two such files as two members). Decision 11 (an occurrence renumbers when an earlier copy goes) is accepted and stated there.

- [ ] **Step 1: Write the failing tests.**
  - **Round trip.** An edition of three members with one spread in the middle member, read both `rtl` and `ltr`. Every slot goes to a `MemberPage` and back to `Exact` of itself, and the slot one past the end answers `None`. *Fails on:* the page within the member off by one; the half dropped from the anchor (the two spread slots).
  - **Occurrence.** Members `[a, b, a]`. A slot on the second `a`'s page names `nth: 1`, and `slot_of` answers that slot, not the first `a`'s. *Fails on:* the occurrence ignored (the first match taken).
  - **A half the layout no longer holds** (decision 6). A half anchor taken from a spread, asked of the same edition with that flag cleared, answers `PageOnly` of the page's one slot. A whole-page anchor, asked of the edition with that page flagged, answers `PageOnly` of its first half in reading order: right under `rtl`, left under `ltr`. *Fails on:* `Exact` answered (assert the variant); the second half chosen.
  - **Gone.** An anchor with `nth: 1` against an edition holding one copy answers `None`. A member that shrank, so `at.page` is past its end while the next member holds that flattened page, answers `None`. *Fails on:* the page bound dropped (a slot in the next member is answered).
- [ ] **Step 2: Implement.** Run `taiyaku-core`'s tests, clippy and fmt.
- [ ] **Step 3: Watched mutations**, each named above.
- [ ] **Step 4: Commit** "taiyaku-core maps a slot to the member page it names and back (O244)". Write `$E/T2-report.md`.

---

### Task 3: Positions resolve, and every write names its page (O244)

Spec §3 "Writes" (the first two bullets), "Reads", decisions 1, 2, 6 and 10; spec §3 Testing: "Round trip" is Task 2's; "O244's own case", "A lost file" (the position), "A half" (the position), "A vanished edition key" (the position), "Writes" (the positions).

**Files:**
- `crates/taiyaku-library/src/project.rs`: `layout`, beside `sidecar` (~40). It reuses `work_row`'s direction parse, `members_for` (~95) and `flags_for(…, "spread")` (~115). Make `parse_half` (~329) `pub(crate)` if `state` reuses it.
- `crates/taiyaku-library/src/state.rs`:
  - New: `at_columns`, `at_from_columns`, `resolve_position`, `resolve_note` (the note rules are in the same table, so both are written and unit-tested here; Task 5 wires the note one).
  - `put_progress_at` (~132): its `INSERT` and its `DO UPDATE SET` both write the four. Its doc comment's account of the columns it names changes with it.
  - `progress` (~55): reads the four with the slot and answers each row's resolved slot. Each edition key's layout is read once per call.
  - The module doc gains a short paragraph: where the anchor is written, where it is resolved, and that the slot is the fallback.
- `crates/taiyaku-library/src/tests.rs`: a new fixture `a_volume_across_two_directories`, beside `library_with_two_roots_and_a_reading_position` (~350).
  - One library holds directory A with `Foo/Foo - v01 - c001.cbz` (3 pages) and directory B with `Foo/Foo - v01 - c002.cbz` (4 pages), written with `crate::scan::tests::write_archive`, each added with `scan::add_root_in(…, InLibrary::Named(…))`, scanned and derived.
  - It asserts one work whose `raw` edition's members are c001 then c002, so slots 0-2 are c001's and 3-6 c002's.
  - It returns the library, both `scan::Root`s and the work id, and offers a way to put a removed directory back: add it to the same library again, scan it, derive.
- Tests: `crates/taiyaku-library/src/state/tests.rs`. Its `a_work` (~8) shows the `import::sidecar` document fixture the half and vanished-key cases use. The route test goes in `crates/taiyaku-serve/src/routes/tests.rs`, in the module over `test_support::router_over_an_edition_merged_from_two_libraries` (helpers `hold_position`, `positions`, `progress_uri` ~7690-7730).

**The resolution rules** (decisions 2, 6, 10 and the NULL rule; the plan's statement of them):

| The row | A position answers | A note answers |
| --- | --- | --- |
| Its edition key is gone (no layout) | the stored slot, unclamped | the stored slot, `stale` |
| NULL anchor | the stored slot, unclamped | the stored slot |
| `SlotOf::Exact(s)` | `s` | `s` |
| `SlotOf::PageOnly(s)` | `s` | the stored slot, `stale` |
| `None` (the page is gone) | the stored slot clamped to the last slot; the stored slot where the edition has none | the stored slot, `stale` |

- [ ] **Step 1: The rules, unit-tested.** One case per row of the table, for each function. *Fails on:* the clamp removed (the position answers its stored slot past the end); the note clamped; `PageOnly` treated as gone for a position; the no-layout arm clamped against nothing (0 answered); `stale` not set on each `stale` row.
- [ ] **Step 2: Writes** (the four columns, read by SQL):
  - `put_progress` on one page, then on another: the four name the second page. *Fails on:* the anchor written on insert only (`DO UPDATE SET` without the four).
  - A position past the end is stored with all four NULL. *Fails on:* an anchor made from a clamped slot.
  - After both, every row's first three columns are all set or all NULL.
  - A row whose `at_sha256` is set and `at_nth` NULL (set by SQL) is refused by `state::progress`, naming the row. *Fails on:* a partial set read as NULL.
- [ ] **Step 3: O244's own case** (library). On the fixture, a position at slot 4 (c002's second page); `scan::remove_root` of A. `state::progress` answers slot 1. *Fails on:* resolution disabled (4 is answered).
- [ ] **Step 4: A lost position.** A position at slot 6 (c002's last page); remove B, leaving c001's three slots. It answers slot 2, the clamp. Put B back: it answers 6. A position on c001 cannot test the clamp: its stored slot stays in range. *Fails on:* the clamp dropped (6 while lost); a read that rewrites the row from its fallback (2 after B is back).
- [ ] **Step 5: A half** (the position). A work imported from a document read `rtl`, with a spread flag on page `p` and a page after `p`. A position on `p`'s left half, which is second in that order. Delete the flag (`DELETE FROM page_flag`). It answers `p`'s one slot, one below its stored slot. *Fails on:* the position treated as gone (the stored slot, now the next page's, is answered).
- [ ] **Step 6: A vanished edition key** (the position). Import a document with editions `raw` and `en` at a path, put a position on `en` at slot 1 or above, and import again at the same path a document with `raw` alone: `en` is deleted (`Scope::Document`), as in `import/tests.rs`'s `reimporting_with_fewer_editions_removes_the_ones_that_went` (~532, with `two_edition_doc` ~20 and `hub_only_doc` ~104). It answers its stored slot. *Fails on:* the position clamped against an edition with no slots (0 is answered).
- [ ] **Step 7: Through the route.** On `router_over_an_edition_merged_from_two_libraries`, `raw` is part1 (2 pages), part2 (3) and part3 (4). As the administrator, `PUT` a position at raw slot 6 (part3's second page). Remove Kids' directory `k` (part2) with `taiyaku_library::scan::remove_root`, finding its id in `root`. `GET /api/works/{work}/progress` answers raw slot 3. *Fails on:* resolution disabled (6 is answered).
- [ ] **Step 8: Implement**, run, and watch each mutation above. Steps 1-7 fail, or do not compile, before it.
- [ ] **Step 9: Commit** "A reading position answers the page it was written on, and every write names that page (O244)". Write `$E/T3-report.md`.

---

### Task 4: `Note` gains `stale`, and `same_anchor` moves to the library (O244)

Spec §3 "Writes" ("That comparison moves into `taiyaku-library`"), "The notes document" (`stale`).

**Files:**
- `crates/taiyaku-library/src/lib.rs`: `Note` (~153) gains `stale` and the method `same_anchor`; its doc comment moves from serve's.
- `crates/taiyaku-serve/src/notes.rs`: `same_anchor` (~57) goes; `validate_against` (~138) calls `held.same_anchor(note)`.
- Every `Note {` struct literal in `crates/` gains `stale: false` (grep `Note {`, word-bounded; `crates/taiyaku-serve/src/corpus.rs`'s corpus test included).
- Tests: `crates/taiyaku-library/src/tests.rs`.

- [ ] **Step 1: Write the failing tests.**
  - A note body with no `stale` key reads `stale: false`, and one with `"stale": true` reads true. *Fails on:* the `default` removed (every body today has no key and is refused).
  - A note with `stale: false` is written with no `stale` key, and one with `true` with `"stale": true`. *Fails on:* `skip_serializing_if` removed.
  - `same_anchor` is true for two notes that differ only in body, colour, times or `stale`, and false when the edition, the slot or the region differs. *Fails on:* the region dropped from the comparison.
- [ ] **Step 2: Implement**, and add `stale: false` to every literal. Serve's grandfathering tests (`a_stale_anchor_already_stored_does_not_refuse_the_writes_that_follow_it`, `moving_a_stale_note_is_refused_even_though_it_was_already_stored`, in `notes/tests.rs`) stay green on the moved method.
- [ ] **Step 3: Run** `taiyaku-library`, `taiyaku-serve` and `taiyaku-cli`, clippy and fmt. Watch each mutation.
- [ ] **Step 4: Commit** "A note can say it is stale, and the library decides when a note has not moved (O244)". Write `$E/T4-report.md`.

---

### Task 5: Notes resolve, and `put_notes` keeps the anchor of a note that has not moved (O244)

Spec §3 "Writes" (`put_notes`), "Reads", decisions 2, 3, 6 and 10; spec §3 Testing: "A lost file" (the note), "A moved file", "A half" (the note), "A vanished edition key" (the note), "A held anchor", "Writes" (the notes).

**Files:**
- `crates/taiyaku-library/src/state.rs`:
  - `NoteRow` (~156) carries the four columns; `note_from_row` (~217) builds the anchor through `at_from_columns`.
  - `notes` (~182) answers each note's resolved slot and sets `stale` through `resolve_note`, against its own edition's layout. Resolution searches that edition only (decision 3).
  - `put_notes` (~272) reads the stored rows resolved, before its `DELETE`. An incoming note whose id has a stored note that `same_anchor`s it (edition, resolved slot, region) is written with that stored note's four columns unchanged. Any other note gets the anchor its slot names in the current layout. Its `INSERT` names the four. Its doc comment says why the held anchor is kept.
- Tests: `crates/taiyaku-library/src/state/tests.rs`, on Task 3's fixture and document fixtures.

- [ ] **Step 1: A lost note.** On the fixture, a note at slot 6 (c002's last page); remove B. `state::notes` answers it at slot 6 with `stale: true`. Put B back: slot 6, not stale. *Fails on:* `stale` not set; the note clamped (2 is answered); a read that rewrites the row.
- [ ] **Step 2: A moved file** (decision 3). On the fixture, a note on c001's second page (slot 1). A rule made with `rules::create` matches c001's file and sets `edition_key` to `colour`; derive. The note answers on `raw` at slot 1, `stale`, and no note is answered on `colour`. *Fails on:* resolution searching the work's other editions (the note is not `stale`).
- [ ] **Step 3: A half** (the note). Task 3's half fixture, with a note on `p`'s left half; delete the flag. The note answers its stored slot, `stale`. *Fails on:* the note placed on the page (one below its stored slot, not `stale`).
- [ ] **Step 4: A vanished edition key** (the note). Task 3's fixture, with a note on `en` at slot 1 or above. It answers its stored slot, `stale`. *Fails on:* `stale` not set where no layout is found.
- [ ] **Step 5: Trap 1, `put_notes` keeps the stored anchor of an unchanged note.**
  - Notes n1 on c001's second page (slot 1) and n2 on c002. Remove **A**: n1 answers slot 1, `stale`.
  - Read the document with `state::notes`, change n2's body, and write it back with `state::put_notes`.
  - Put A back: n1 answers slot 1, not stale.
  - The fixture's constraint: the lost note's stored slot must lie inside the shrunken edition, so that a recomputed anchor names one of c002's pages rather than nothing. With B removed instead, a recomputed anchor is NULL and answers the same slot, and the test cannot fail. That is why A is removed here.
  - *Fails on:* every note's anchor recomputed on write (n1 answers 4 once A is back).
- [ ] **Step 6: A moved note is anchored anew** (the columns read by SQL). Write the same document with n2 moved to another page: n2's four columns name the new page. After both writes, every `note` row's first three columns are all set or all NULL. *Fails on:* the held anchor kept for a note matched by id alone.
- [ ] **Step 7: Implement**, run, and watch each mutation.
- [ ] **Step 8: Commit** "A note answers the page it was written on, a lost one as stale, and a write keeps an untouched note's page (O244)". Write `$E/T5-report.md`.

---

### Task 6: The fold carries anchors and plans from resolved slots (O244)

Spec §3 "Writes" (the fold bullets), decision 9, "Edges the plan settles" (O78); spec §3 Testing: "The fold carries anchors", "The fold plans from resolved slots".

**Files:**
- `crates/taiyaku-library/src/fold.rs`:
  - `move_positions` (~1799): its `INSERT … SELECT` column list and its `DO UPDATE SET` both name the four. Its doc comment's "all four columns" says what the newer row carries now.
  - `SlotPlan` (~832) and `plan_slots` (~904): the plan keeps both works' editions and reading directions as `plan_slots` read them, before any write, with one helper that resolves a row (table, side, edition key, stored slot, anchor) against them through `state::resolve_position` / `state::resolve_note`.
  - `unplaceable` (~1179), `shared_note_ids` (~570) and `resequence_rows` (~1247) read the four columns with `slot` and use the resolved slot wherever they used the stored one. `resequence_rows` writes the placed number into `slot` where it differs from the stored number, and never writes the four.
  - `NoteContent`'s doc (~465-478): "A new column on `note` is not covered until it is added here" gains why the four anchor columns are left out (item 9 of What the plan settles).
  - `move_notes` (~1859) is an `UPDATE` and keeps the four; it does not change.
- Tests: `crates/taiyaku-library/src/fold/tests.rs`, with `seed_paged_work` (~2019), `place` (~2232), `folded` (~96), `insert_note` (~2989) and the models `where_both_works_hold_the_same_edition_the_newer_position_wins` (~294), `a_twin_holding_other_spread_flags_keeps_them_and_places_what_it_can` (~2619) and `note_copies_are_one_note_by_the_page_they_name_not_the_number_they_hold` (~2888).

- [ ] **Step 1: Trap 2, `fold::move_positions` lists its columns.** "The fold carries anchors", run twice:
  - **The insert.** A position written by `place` on page `p` of the source; no position on the target's edition. Fold. The moved row's four columns equal the source's (read by SQL). Then add a spread flag on an earlier page of the target's edition (by SQL): `state::progress` answers `p`'s slot in the new layout. *Fails on:* the four dropped from the `INSERT … SELECT` (the old number, another page, is answered).
  - **The update.** The target holds an older position on page `q`, not `p` (`put_progress_at` with an earlier `updated`); the source's newer one is on `p`. Fold, then change the layout as above: it answers `p`'s slot. *Fails on:* the four dropped from `DO UPDATE SET` (the target's old anchor answers `q`'s page).
- [ ] **Step 2: The fold plans from resolved slots** (decision 9).
  - Source: edition `raw`, one member `x` of 6 pages or more, no flags; `place` on page 3 (slot 3). Then flag page 2 of the source (by SQL). Read `rtl`, slot 3 now names page 2's left half, and the anchor still names page 3 (resolved slot 4).
  - Target: the same member, holding a spread flag on page 4 only, so it keeps its own flags and its page 2 is whole.
  - The fold is not refused, and on the target `state::progress` answers page 3's slot there (3).
  - *Fails on:* `unplaceable` reading the stored slot (it names a half the twin does not hold, and the fold is refused).
- [ ] **Step 3: Shared notes by resolved slot, anchors not compared** (O78's edge, item 9).
  - Source and target both hold member `x` (6 pages), no flags.
  - The source's note is written by `put_notes` at slot 3 (page 3). Then flag page 1 of the source (by SQL), so its stored 3 names page 2 and its anchor resolves to 4. The target holds the same id and content by `insert_note` at slot 3, with a NULL anchor.
  - The target takes the source's flag, so its row is re-placed from 3 to 4.
  - The fold goes ahead with the source's copy dropped (`Moved::dropped_notes`), and the target's note answers page 3's slot.
  - *Fails on:* `shared_note_ids` comparing stored slots (3 against 4: refused); the four anchor columns added to `NoteContent` (NULL against set: refused).
- [ ] **Step 4: The plan-time layout.** `the_twins_own_rows_are_placed_under_the_flags_it_takes` (~2715) writes the twin's place through `put_progress`, anchored since Task 3, and reads the fallback slot the fold writes.
  - *Fails on:* `resequence_rows` resolving `into_work`'s rows against the store read inside it, after `write_spreads`.
  - Watch that mutation turn it red. If it does not, add a case that does, and say why.
- [ ] **Step 5: Implement**, run `taiyaku-library` (the fold also runs under derive and removal tests) and `taiyaku-serve`, clippy and fmt, and watch each mutation.
- [ ] **Step 6: Commit** "A fold carries the page a position names, and plans from the page each row names (O244)". Write `$E/T6-report.md`.

---

### Task 7: `stale` at the edges: the notes `PUT`, export and import (O244)

Spec §3 "The notes document" (`stale` cleared on the way in), decision 7; spec §3 Testing: "`stale` on the way in", "Import"; Spec gaps 1 and 2.

**Files:**
- `crates/taiyaku-library/src/lib.rs`: `Notes::clear_stale`.
- `crates/taiyaku-serve/src/routes.rs` `put_notes` (~1081-1170), in this order:
  1. Parse the body, then clear `stale` on the incoming notes.
  2. Load `current`, and take the tag from it uncleared. The `GET` tags this same document, so the `If-Match` compares like with like.
  3. Check `If-Match`.
  4. Clear `stale` on the stored document `as_stored` and `validate_against` compare against, then run both.
  5. Sort and store.
  6. Answer from the store read again (`notes::load`, then `tagged_for`), not from `merged`. The answer's doc comment says why.
- `crates/taiyaku-library/src/export.rs` `work` (~247-253): clear `stale` on the notes before serialising them.
- `crates/taiyaku-library/src/migrate.rs` `apply_notes` (~605): no change (Spec gaps 2).
- Tests:
  - `crates/taiyaku-serve/src/routes/tests.rs`, the merged-libraries module (`hold_notes`, `held_notes`, `fetched`, `put`, `notes_uri`, `note` ~7138-7230; model `manga_alones_notes_write_is_stored_translated_and_keeps_the_note_it_is_not_sent` ~7613).
  - `crates/taiyaku-library/src/export/tests.rs` (model `exporting_writes_five_documents_beside_the_sidecar` ~410).
  - `crates/taiyaku-library/src/migrate/tests.rs` (models ~66 and ~263; `a_work` ~34 writes the sidecar at a real path).

- [ ] **Step 1: `stale` on the way in.**
  - Kids alone is narrowed. Hold, for Kids, its notes on `raw` and one note on `en`, an edition Kids is not sent. Make that note lost by setting its `at_sha256` to a digest no member holds (by SQL).
  - Read Kids' document and tag. `PUT` it with the `en` note added as stored, carrying `"stale": true`: 200. Read the tag again and `PUT` the same with no `stale` key: 200.
  - After each, the store answers the `en` note as before: its slot, `stale`.
  - Why `en`: a note on a hidden page of a narrowed edition cannot be sent back as it was, since Kids' numbering cannot name that page, so `as_stored` places it elsewhere and refuses it whatever `stale` says. A note on an edition the account is not sent reaches `as_stored`'s comparison unchanged.
  - *Fails on:* `stale` left on the stored side of the comparison (both `PUT`s are refused, the incoming side being cleared at the parse); left on the incoming side (the first is refused).
- [ ] **Step 2: The `PUT`'s tag and answer while a note is stale** (Spec gaps 1).
  - As the administrator, hold a note on part2's page, then remove Kids' directory `k`: the note is lost.
  - `GET`: the note is `stale: true`.
  - `PUT` the body back, with one new note, under that tag: 200. The answer's `ETag` and body equal the next `GET`'s, and its lost note is `"stale": true`.
  - *Fails on:* the tag taken after `stale` is cleared (412); the answer built from the document written (the tags differ).
- [ ] **Step 3: Export.** A work imported from a document with members `a` and `b`, a note on `b` written through `put_notes`, then the document imported again without `b`: the note is lost. `export::work` to a directory writes a `notes.json` with no `stale` key, and the note at its stored slot. *Fails on:* export writing `state::notes`' answer as it stands.
- [ ] **Step 4: Import** (spec §3 Testing).
  - A work imported at a real path from a document with members `a` (3 pages) and `b` (4), with a `progress.json` holding a position at slot 4 and a `notes.json` holding a note at slot 4 beside it. Run `migrate::work`.
  - Then import at the same path the document without `a`.
  - `state::progress` and `state::notes` answer slot 1.
  - *Fails on:* `apply_progress` or `apply_notes` writing the rows around `state::` (a raw insert, anchor NULL: 4 is answered).
- [ ] **Step 5: Implement**, run `taiyaku-library` and `taiyaku-serve`, clippy and fmt, and watch each mutation.
- [ ] **Step 6: Commit** "A notes write and an export leave stale out of what they compare and write, and an import anchors what it reads (O244)". Write `$E/T7-report.md`.

---

### Task 8: The notes document's version (decision 5). Waits for lane 1 on `main`; rebase first

Spec §3 "The notes document", decision 5; spec §2 "Document version guards" (lane 1's helper and in-place export guard) and decision 10 there.

**Files:**
- `crates/taiyaku-library/src/lib.rs`: `NOTES_VERSION`; `Notes` (~179) gains `taiyaku_notes` as its first field; `Notes::readable_version`. The field's doc comment gives the bump rule: only a newer version is refused, and whoever bumps it owns what reading a 1 means (`docs/sidecar.md`, "Forward compatibility").
- Every `Notes {` struct literal in `crates/` gains `taiyaku_notes: NOTES_VERSION` (grep `Notes {`, word-bounded).
- `crates/taiyaku-serve/src/routes.rs` `put_notes`: after the body is parsed, a version above 1 is answered 400 with the sentence below (`bad_request`), and nothing is written.
- `crates/taiyaku-library/src/migrate.rs` `apply_notes`: after the parse, the same refusal, so the import reports `NotesOutcome::Malformed` with the sentence and writes nothing.
- `crates/taiyaku-library/src/export.rs`: lane 1's guard (spec §2, "An in-place `library export`") gains `notes.json` with the key `taiyaku_notes` and `NOTES_VERSION`, in whichever loop and whatever form lane 1 gave it; `notes.json` is always in the write loop.
- If lane 1 lists the versioned siblings anywhere (`docs/sidecar.md`, `README.md`, a doc comment on its helper), `notes.json` joins that list.

**The sentence** (literal; it follows `Sidecar::readable_version`'s): `this document declares taiyaku_notes version 2, and this build reads 1. Refusing to read it rather than dropping the fields it does not understand and writing them back out as version 1 — upgrade taiyaku instead`. If lane 1 put the words of its three readers in one function, call it, and keep this literal in the tests only if that function produces it; otherwise follow lane 1's shape and record the difference.

- [ ] **Step 0: Rebase.** Lane 1's PR must be merged on `main`. `git -C $W fetch`, then `git -C $W rebase origin/main`.
  - Conflicts are expected to be textual, in `lib.rs`, `schema.rs`, `schema/tests.rs`, `state.rs` and `export.rs`.
  - If lane 4's migration landed first, renumber this lane's (Global Constraints).
  - Run `taiyaku-library`, `taiyaku-serve` and `taiyaku-cli`, and report what conflicted and how it was resolved.
  - Read lane 1's version helper and export guard, and its export test, before writing anything below.
- [ ] **Step 1: Write the failing tests.**
  - `GET /api/works/{work}/notes` answers `"taiyaku_notes": 1`. *Fails on:* the field not written.
  - A `PUT` whose body is a JSON literal with no `taiyaku_notes` key is accepted. *Fails on:* the `default` removed.
  - A `PUT` declaring 2 is answered 400 with the sentence, and the stored notes are unchanged. *Fails on:* the check removed.
  - `migrate::work` over a `notes.json` declaring 2 reports `Malformed` with the sentence, writes no `note` row, and leaves the file as it was. *Fails on:* the check removed from `apply_notes`.
  - An in-place export first writes `notes.json`, arming its marker. A `notes.json` declaring 2 then put in its place is kept by the next in-place export, its bytes unchanged, and listed in `Exported::kept`. *Fails on:* `notes.json` left out of lane 1's guard (the file is overwritten).
- [ ] **Step 2: Implement**, add the field to every literal, and run `taiyaku-library`, `taiyaku-serve` and `taiyaku-cli`, clippy and fmt. Watch each mutation.
- [ ] **Step 3: Commit** "The notes document declares taiyaku_notes 1, and a newer one is refused and kept (pre-deploy decision 5)". Write `$E/T8-report.md`.

---

### Task 9: O244 narrowed in `docs/status.md`

Read `docs/status.md`'s Open items section, O244's row (~1853) and its notes bullet (~3981), and O56's row (~1664).

- [ ] **Step 1: O244's row.** The checkbox stays `[ ]`. Rewrite the row's text, with symbols and no line numbers:
  - what was done: the anchor written beside the slot on every write and resolved on every read; a lost file's position clamped and its note `stale`; a moved file resolving as a lost one; the fold carrying the anchor and planning from resolved slots;
  - what remains: the removal rehearsal's count of rows whose anchor would stop resolving, which goes with the web half (O240's follow-up), where a screen can show it;
  - what is not anchored: `page_flag`, `run` and `truth_page`.
  The row keeps exactly six `|` outside code spans.
- [ ] **Step 2: Its notes bullet.** Rewrite it in place as "… Narrowed.", saying the same in prose.
- [ ] **Step 3: O56's row** gains one sentence: an anchor's page within an image-folder member drifts with the member's contents in the same way, and is left open with O56 (spec §3 "Edges the plan settles").
- [ ] **Step 4: Counts.** No row closes or opens, so the closed list and its "leaving N", the section head's total and "What remains … N open items" do not change. Check them against the table:
  ```sh
  grep -a -c '^| O[0-9]' docs/status.md
  grep -a -c '^| O[0-9]* | \[x\]' docs/status.md
  grep -a -n -A1 '^What remains:' docs/status.md
  ```
  If the rebase brought another lane's counts, they stand.
- [ ] **Step 5: Sweep.** Grep `O244` across `docs/`, `crates/`, `web/src/` and `web/tests/`, and read every hit. Prose that speaks of O244 as wholly open, or of positions as stored in a numbering that drifts, is corrected or marked. Historical narrative stays as written.
- [ ] **Step 6: The Rust test total**, in its two places ("Six crates, N tests" ~179 and "of the N are standing checks" ~256). Run the gate into `$E/gate-0.log`: its `status-test-count` step names the JUnit total when the sentence is out of date. Write that total in both places. No web file changed, so the web figures stay unless the rebase moved them; then measure the node and jsdom projects with `--reporter=default` and take the figures from those runs, never by adding.
- [ ] **Step 7: The gate** into `$E/gate-1.log`. Quote `all green`, the exit status and the load.
- [ ] **Step 8: Commit** "O244 narrows in docs/status.md to the rehearsal's count". Write `$E/T9-report.md`.

---

### Finish

1. **Whole-branch review** of `main..HEAD`, in two lanes, each against the spec, the rulings above and this plan's settled calls and Spec gaps:
   - (a) the core, the store and the fold (Tasks 2, 3, 5, 6). Ask in particular:
     - Does any reader of `progress.slot` or `note.slot` still read the stored number where it should resolve? (`rg -n 'slot' crates/taiyaku-library/src --glob '!**/tests.rs'` around every `FROM progress` and `FROM note`.)
     - Does any writer drop the four columns? (`rg -n 'INTO (progress|note)|UPDATE (progress|note)' crates`)
     - Does each row of Task 3's table have a test that fails against its neighbour's answer?
     - Does every new test fail against its named defect, on a fixture that tells right from wrong?
   - (b) the schema, `Note` and `Notes`, the serve edges, export, import and the docs (Tasks 1, 4, 7, 8, 9). Ask in particular:
     - Is the `PUT`'s tag still taken before `stale` is cleared, and its answer read after the write?
     - Is `stale` in any document written to disk or any value compared?
     - Does the version refusal use the sentence everywhere?
     - Are the status.md claims true?
2. **One fix wave**, by SendMessage to the implementers. Review the fix commits too. After fixing a finding, grep for its shape, not its function.
3. **SQLite 3.40.1** on the final head: the bookworm recipe into `$E/bookworm-final.log`.
4. **The final gate** on the final commit into `$E/gate-final.log`, at load < 24. Read the `.exit` file. Re-read the Rust total in `docs/status.md` against that run.
5. **The drive.** A position and a note survive a member going and coming back, on a live server.
   - Setup, in the session scratchpad:
     - `sha256sum` and `stat` `/home/davey/temp/manga-test/library.db`, then `touch` a marker.
     - Copy that `library.db` into the scratch directory. Copy only; never open the original.
     - Build a tree there of one volume split into two chapter archives, copied with `cp` (never `ln`) from the test library, or written fresh if no volume there is split.
     - Add the tree as a new directory of the copy with `taiyaku library scan --add <tree> --db <copy>`. Never scan a directory of the test library.
     - Build a release binary and copy it into the scratch directory. Build the web there.
     - Check the port is free (`ss -lptn 'sport = :<port>'`). Serve on `127.0.0.1:<port>`, logged in as `local`/`pw` (the copy's account; set its password on the copy as the recipe in the project's memory does).
   - In the reader, read to a page of the second chapter and add a note there.
   - Scan the tree's directory after each move below (Configuration's Scan directory, or `library scan --root <id> --db <copy>`). Never move both chapters out at once: a work left with no member is lane 4's case, not this one.
   - Move the first chapter's archive out: reopening the work resumes on the same page, with the note on it (O244's own case). Put it back: the same.
   - Move the second chapter out: `GET …/notes` answers the note `"stale": true` at its stored slot, and `GET …/progress` the clamped slot (curl, with the session cookie and an `origin` header).
   - Put it back: the page and the note answer as they did, without `stale`.
   - Stop the server by PID, check the port again, compare `find /home/davey/temp/manga-test -newer <marker>` (empty) and the digest (unchanged), and delete the scratch directory last.
6. **The claim check** of O244's row and notes bullet, O56's sentence, the test counts and the PR body. The PR body is in pass 1; later passes are scoped to the correction commit plus the PR body. Grep a corrected sentence's shape everywhere.
7. **The PR body** at `$E/pr-body.md`, for the gitea MCP and never hard-wrapped. Its first line is the title, measured under 255 characters. It says O244 is narrowed, gives the owner's rulings of 2026-11-10, this plan's settled calls and its Spec gaps as resolved, says the web half follows (closing O240), and has no attribution. This lane merges after lane 1. Ask the user before pushing.
