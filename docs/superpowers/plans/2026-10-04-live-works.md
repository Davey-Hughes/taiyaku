# Live works (day-to-day deployment, Part A): Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The reader, OPDS and `/api/works` serve what the library holds now,
not what it held when the process started. A volume a scan has just derived
opens on the first request for it, with no restart. This opens and closes a
new row, O183.

**Architecture:** Migration 14 adds a one-row `structure_version` counter and
eighteen triggers that bump it on every write to the six tables a `Work` is
projected from. In `taiyaku-serve`, `ServeState.works` stops being a
`Vec<Work>` and becomes a `Works` registry. The registry is *live* for
`serve` with no paths and *fixed* for `serve foo/taiyaku.json` and for test
fixtures. A live registry re-reads the library when the counter has moved:
every two seconds from a background task, and before answering a slug it does
not hold. Each surviving `Work` keeps its `Arc` and has its document swapped
under its own write lock, so a correction in flight and a refresh run one
after the other.

**Tech Stack:** Rust 2024 (workspace `rust-version` 1.85, toolchain floating on
stable), rusqlite 0.38 against the system SQLite, axum 0.8, tokio 1 (`time`
feature already on in `taiyaku-serve`), cargo-nextest in the gate. Web:
TypeScript, React 19, Vitest (projects `node`, `jsdom`, `browser`), Biome,
pnpm.

## Global Constraints

- **Branch:** `live-works`, cut from the head of `deployment-spec` (the
  commit that adds this plan). If it has merged by the time you start, cut
  from `main` instead. Task 7's PR body says which.
- **The spec is `docs/superpowers/specs/2026-10-04-day-to-day-deployment-design.md`,
  §1 and §2.** Read both before Task 1. Where this plan and the spec disagree,
  stop and ask; do not pick one. The plan's own disagreements are listed under
  "Spec corrections needed" below and are for the user, not for you.
- **Part B (deployable) may be running in another lane** and also edits
  `taiyaku_serve::serve`. This part changes the *type* of `serve`'s `works`
  parameter and adds a spawn inside it, not the parameter count. Whichever
  lands second rebases onto the other.
- **No attribution lines anywhere**: not in commit messages, code comments or
  documentation.
- **Every command runs from `/home/davey/projects/taiyaku`.** Never `cd`: this
  shell's `cd` is zoxide and has landed in another repository. The Bash tool
  runs zsh with `noclobber`, so write scratch output to a new file name or use
  `>|`. Quote globs (`--include='*.rs'`). Never name a shell variable `path`
  (zsh ties it to `PATH`).
- **Throttle every Rust test run**: `cargo test … -- --test-threads=8`, and
  `NEXTEST_TEST_THREADS=8 RUST_TEST_THREADS=8` for the gate. An unthrottled
  run on this 32-core machine has been killed for memory.
- **Every new test is watched failing with its defect put back by hand.** Edit
  the production code as the step says, run the test, read the failure,
  restore, run it green. Put the failure's first line in the task's report. A
  test that passes against its own defect is this repository's most-recorded
  failure. If the named defect does *not* make the test fail, stop and report:
  do not weaken the defect or change the test to make it fail.
- **A task is done only when whole crates pass**, not the files it touched:
  `cargo test -p taiyaku-library -p taiyaku-serve -p taiyaku-cli --
  --test-threads=8`, plus `cargo clippy --workspace --all-targets
  --all-features -- -D warnings` and `cargo fmt --all -- --check`. A web task
  also runs `pnpm --dir web typecheck`, `pnpm --dir web lint`, and
  `pnpm --dir web exec vitest run --project node --project jsdom
  --reporter=default`, plus `--project browser` when a `*.browser.test.tsx`
  changed. A failure is "pre-existing" only if it fails the same way on `main`
  at `8f8cdba2`, checked in a worktree, never against another commit on this
  branch.
- **`--reporter=default` on every vitest run.** Under Claude Code vitest
  otherwise picks its `agent` reporter, which hides passing tests' output.
- **Never `std::thread::sleep` or a blocking call on a tokio runtime worker in
  production code.** Anything that reads the database from an async handler
  goes through `tokio::task::spawn_blocking`. This part adds one async lookup
  that does that (`ServeState::find`); keep it that way.
- **No figure in a code comment that a later change would have to correct**:
  no counts of handlers, tests, call sites or works. "Two seconds" is the
  constant's own value and may be written beside it.
- **Evidence goes in `.superpowers/sdd/live-works-evidence/`**, which is
  gitignored, never under `target/`. `target/` has been emptied mid-session
  before, and everything under it went.
- **Never touch `/home/davey/temp/manga-test/`.** Task 6 copies its
  `library.db` into your session scratchpad and proves afterwards that the
  original is byte-identical.
- **Comment anchors quoted in this plan may wrap across `///` lines.** Find
  the words, not the line number. Line numbers given are at `8f8cdba2` and are
  hints only. If the words are not there at all, stop and report.
- **Nothing outside Part A changes.** A defect found on the way goes in the
  task's report for Task 7's `docs/status.md` edit. Do not fix it unless it
  blocks a step here.

## Spec corrections needed

The user should rule on these before or during execution. The plan is written
to the reading given here.

1. **§2.2 says a slug the registry already holds "keeps its `Arc<Work>`".**
   That holds unless the work's `sidecar_path` changed. `Work.path` is a
   public field fixed for a `Work`'s life and read directly at many sites
   (`state.page`, the alignment route's file write, error messages). So a
   moved document gets a new `Work`, and in that case only, its notes lock
   and correction lock are new ones. Only `library import` of a moved
   `taiyaku.json` changes `sidecar_path`; a scanned work's is always NULL.
   The plan replaces the `Work` in that case. Proposed spec wording: "keeps
   its `Arc<Work>` unless its document's path changed, which only a `library
   import` of a moved `taiyaku.json` does".
2. **§2.3 says `finished_scan_json`'s `note` is deleted, and is silent on the
   web.** `web/src/ScanReport.tsx` draws that field, `web/src/config.ts`
   types it as required, three test fixtures carry it, and
   `ScanReport.test.tsx` asserts it is drawn. Removing it is a web change too
   (Task 5). The spec's part table and §2 should say so.

Both corrections are folded into the spec (§2.2 and §2.3) as of 2026-10-04.

## Decided by the user, 2026-10-04

- **Startup is lenient too** (spec §2.3). A work whose rows will not project
  is left out and named on stderr, and the rest is served. Under WUD
  auto-deploy (Part B), a refused startup would be a restart loop until
  someone fixed the row. `Works::library` errors only when the library cannot
  be read at all. `works::from_library` (fixtures and `corpus.rs` only) keeps
  refusing, since nothing deployed reaches it.

## Decided while planning

- **Two lookups, and the old names are deleted.** `ServeState::work` and
  `ServeState::work_or_missing` go. `ServeState::find` (async) is for code on
  a runtime worker: a hit is a read lock and an `Arc` clone, and a miss asks
  the library on the blocking pool. `ServeState::find_blocking` is for code
  already inside `spawn_blocking`. Deleting the old names makes the compiler
  list every call site, and each one has to choose. O122 names, among
  others, these handlers that look works up on the async worker:
  `get_sidecar`, `serve_page`, `opds::work_feed`, `opds::stream_page`.
- **The counter is read *before* the works are projected.** Read after, a
  write that lands mid-projection would be recorded as seen without having
  been read. Task 2 has a test that moves the read late and fails.
- **Each work is read inside its own `BEGIN DEFERRED` transaction.**
  `Library::configure` sets `TransactionBehavior::Immediate` on every
  connection, so `unchecked_transaction()` would take the write lock for a
  read. `rusqlite::Transaction::new_unchecked(conn,
  TransactionBehavior::Deferred)` exists in 0.38 (checked in the registry
  source) and does not.
- **A running server keeps the previous document of a work that will not
  project**, names it in `Refresh::Refreshed { kept }`, and serves the rest.
  Startup does the same, leaving a new work out (see "Decided by the user").
- **Complaints are printed once per change, not once per tick.**
  `Works::report` prints a failing refresh the first time, nothing while the
  same failure repeats, and one line when it clears. An `Unmoved` tick neither
  raises nor clears one.
- **Existing fixtures stay fixed** (`Works::fixed`), so every existing route
  test runs exactly as it does today. Only the new tests build a live
  registry, and they all use a **file-backed** library. A live registry over
  an in-memory library reads through the shared connection, so a miss inside
  a handler already holding `state.library.conn()` would deadlock. No
  production path does that: the CLI always opens a file.
- **`works::from_library` stays** for those fixtures, rewritten onto the
  registry's `work_rows` so there is one place that turns a `work` row into a
  path. Like `load_works`, it is now reached only from tests and `corpus.rs`.
- **`Snapshot` derefs to `[Arc<Work>]`**, so `state.works.snapshot()[0]`,
  `.iter()` and `.len()` read the way `state.works[0]` did.

## File structure

| File | Change | Responsibility |
| --- | --- | --- |
| `crates/taiyaku-library/src/schema/014_structure_version.sql` | create | the counter table and its eighteen triggers |
| `crates/taiyaku-library/src/schema.rs` | modify | `SCHEMA_VERSION` 13 → 14, the `MIGRATIONS` array |
| `crates/taiyaku-library/src/schema/tests.rs` | modify | `the_schema_version_is_thirteen` → `…_fourteen` |
| `crates/taiyaku-library/src/structure.rs` | create | `structure::version(conn)` |
| `crates/taiyaku-library/src/structure/tests.rs` | create | the eighteen cases, and the two writes that must not move it |
| `crates/taiyaku-library/src/lib.rs` | modify | `pub mod structure;` |
| `crates/taiyaku-serve/src/registry.rs` | create | `Works`, `Snapshot`, `Refresh`, `work_rows`, `spawn_refresher`, `report` |
| `crates/taiyaku-serve/src/registry/tests.rs` | create | registry-level, router-level and refresher tests |
| `crates/taiyaku-serve/src/works.rs` | modify | `Work::reproject`; `from_library` onto `work_rows` |
| `crates/taiyaku-serve/src/lib.rs` | modify | `ServeState.works: Works`, `find`/`find_blocking`, `serve()` |
| `crates/taiyaku-serve/src/routes.rs`, `opds.rs`, `download.rs` | modify | every lookup onto `find`/`find_blocking`; listings onto `snapshot()` |
| `crates/taiyaku-serve/src/config_api.rs` | modify | `finished_scan_json`'s `note` field goes |
| `crates/taiyaku-serve/src/test_support.rs` | modify | `Works::fixed` in fixtures; `scanned_file_backed_library`, `router_over_a_live_library`, `add_a_volume_elsewhere` |
| every other `crates/taiyaku-serve/src/**/tests.rs` building a `ServeState`, and `corpus.rs` | modify | `Works::fixed(…)`, `.snapshot()[…]` |
| `crates/taiyaku-serve/src/tests.rs` | modify | the two `serve(…)` calls; the new serve-level test |
| `crates/taiyaku-cli/src/main.rs`, `crates/taiyaku-cli/src/tests.rs` | modify | `serve_works` returns `Works` |
| `web/src/config.ts`, `ScanReport.tsx`, `RootRow.tsx`, `styles/admin.css`, and three test files | modify | the scan report's `note` goes |
| `docs/status.md`, `docs/notes.md`, `README.md`, `docs/mutants-baseline.md` | modify | O183, the evidence, one README line, the sweep's log row |

---

## Task 1: Migration 14, the structure counter

**Files:**
- Create: `crates/taiyaku-library/src/schema/014_structure_version.sql`
- Create: `crates/taiyaku-library/src/structure.rs`
- Create: `crates/taiyaku-library/src/structure/tests.rs`
- Modify: `crates/taiyaku-library/src/schema.rs` (`SCHEMA_VERSION`, `MIGRATIONS`)
- Modify: `crates/taiyaku-library/src/schema/tests.rs` (`the_schema_version_is_thirteen`)
- Modify: `crates/taiyaku-library/src/lib.rs` (module list, after `pub mod stamp;`)

**Interfaces:**
- Produces: `taiyaku_library::structure::version(conn: &rusqlite::Connection) -> Result<i64, String>`;
  table `structure_version (id INTEGER PRIMARY KEY CHECK (id = 1), n INTEGER NOT NULL)`
  holding one row `(1, 0)` on a fresh database; `SCHEMA_VERSION == 14`.

- [ ] **Step 1: Write the failing tests**

Create `crates/taiyaku-library/src/structure/tests.rs`:

```rust
//! Migration 14's triggers: one case per table and statement kind, and the
//! two per-reader writes that must not move the counter.

use rusqlite::Connection;

use super::version;

/// One row in each of the six tables `project::sidecar` reads, each
/// satisfying the foreign keys of the rows before it.
const SEED: &str = "
    INSERT INTO work (id, content_key, slug, title, reading, hub)
         VALUES (1, 'k1', 's1', 'T', 'rtl', 'raw');
    INSERT INTO edition (id, work_id, key, label) VALUES (1, 1, 'raw', 'Raw');
    INSERT INTO member (id, edition_id, ordinal, path, sha256, pages)
         VALUES (1, 1, 0, '/x/a.cbz', 'aa', 10);
    INSERT INTO page_flag (edition_id, kind, page) VALUES (1, 'spread', 3);
    INSERT INTO alignment (id, work_id, from_key, to_key, by)
         VALUES (1, 1, 'raw', 'en', 'solver');
    INSERT INTO run (id, alignment_id, ordinal, a_start_page, a_end_page)
         VALUES (1, 1, 0, 0, 9);
";

/// Per table, an insert, an update and a delete that each change exactly one
/// row of [`SEED`]'s database.
const CASES: [(&str, [&str; 3]); 6] = [
    (
        "work",
        [
            "INSERT INTO work (id, content_key, slug, title, reading, hub) \
             VALUES (2, 'k2', 's2', 'U', 'ltr', 'raw')",
            "UPDATE work SET title = 'Retitled' WHERE id = 1",
            "DELETE FROM work WHERE id = 1",
        ],
    ),
    (
        "edition",
        [
            "INSERT INTO edition (id, work_id, key, label) VALUES (2, 1, 'en', 'English')",
            "UPDATE edition SET label = 'Relabelled' WHERE id = 1",
            "DELETE FROM edition WHERE id = 1",
        ],
    ),
    (
        "member",
        [
            "INSERT INTO member (id, edition_id, ordinal, path, sha256, pages) \
             VALUES (2, 1, 1, '/x/b.cbz', 'bb', 12)",
            "UPDATE member SET pages = 11 WHERE id = 1",
            "DELETE FROM member WHERE id = 1",
        ],
    ),
    (
        "page_flag",
        [
            "INSERT INTO page_flag (edition_id, kind, page) VALUES (1, 'credit', 9)",
            "UPDATE page_flag SET page = 4 WHERE edition_id = 1 AND kind = 'spread'",
            "DELETE FROM page_flag WHERE edition_id = 1",
        ],
    ),
    (
        "alignment",
        [
            "INSERT INTO alignment (id, work_id, from_key, to_key, by) \
             VALUES (2, 1, 'raw', 'fan', 'hand')",
            "UPDATE alignment SET verdict = 'sound' WHERE id = 1",
            "DELETE FROM alignment WHERE id = 1",
        ],
    ),
    (
        "run",
        [
            "INSERT INTO run (id, alignment_id, ordinal, a_start_page, a_end_page) \
             VALUES (2, 1, 1, 10, 19)",
            "UPDATE run SET margin = 0.5 WHERE id = 1",
            "DELETE FROM run WHERE id = 1",
        ],
    ),
];

/// A migrated database holding [`SEED`]. A bare connection, so foreign keys
/// are off and a delete removes exactly the row it names, with no cascade
/// moving the counter on its behalf.
fn seeded() -> Connection {
    let conn = Connection::open_in_memory().expect("in-memory connection");
    crate::schema::migrate(&conn).expect("migrates");
    conn.execute_batch(SEED).expect("seeds one row per table");
    conn
}

/// **Every statement kind on every table a served work is built from moves
/// the counter.** One fresh database per case, so no case can pass on a
/// movement an earlier one caused, and every miss is listed at once rather
/// than the first one only.
#[test]
fn every_write_to_a_table_a_work_is_built_from_moves_the_counter() {
    let mut unmoved = Vec::new();
    for (table, statements) in CASES {
        for (kind, sql) in ["insert", "update", "delete"].into_iter().zip(statements) {
            let conn = seeded();
            let before = version(&conn).expect("reads the counter");
            let changed = conn.execute(sql, []).expect(sql);
            assert_eq!(
                changed, 1,
                "`{sql}` must change exactly one row, or the case tests nothing"
            );
            if version(&conn).expect("reads the counter") <= before {
                unmoved.push(format!("{kind} on {table}"));
            }
        }
    }
    assert!(
        unmoved.is_empty(),
        "the counter did not move for: {unmoved:?}"
    );
}

/// **A reading position or a note does not move it.** Both are written on a
/// page turn or a keystroke, and a counter that moved for them would make a
/// server re-read every work of the library over and over while somebody
/// read.
#[test]
fn a_reading_position_or_a_note_does_not_move_the_counter() {
    let conn = seeded();
    let before = version(&conn).expect("reads the counter");
    for sql in [
        "INSERT INTO progress (user_id, work_id, edition, slot) VALUES (1, 1, 'raw', 5)",
        "UPDATE progress SET slot = 6 WHERE user_id = 1 AND work_id = 1",
        "INSERT INTO note (user_id, id, work_id, edition, slot, body, created, updated) \
         VALUES (1, 'n1', 1, 'raw', 2, 'hello', 'now', 'now')",
        "UPDATE note SET body = 'edited' WHERE id = 'n1'",
        "DELETE FROM note WHERE id = 'n1'",
        "DELETE FROM progress WHERE user_id = 1",
    ] {
        assert_eq!(conn.execute(sql, []).expect(sql), 1, "`{sql}` must change one row");
    }
    assert_eq!(version(&conn).expect("reads the counter"), before);
}
```

Create `crates/taiyaku-library/src/structure.rs`:

```rust
//! The counter migration 14 keeps: a number that moves whenever a row a
//! served work is built from is written, by any connection in any process.
//!
//! `schema/014_structure_version.sql` says which tables move it, and why
//! triggers do the counting rather than the writers.

use rusqlite::Connection;

/// The counter's current value.
///
/// # Errors
///
/// Returns an error if the row cannot be read, as on a database migration 14
/// never reached.
pub fn version(conn: &Connection) -> Result<i64, String> {
    conn.query_row("SELECT n FROM structure_version WHERE id = 1", [], |row| {
        row.get(0)
    })
    .map_err(|e| format!("reading the structure version: {e}"))
}

#[cfg(test)]
mod tests;
```

In `crates/taiyaku-library/src/lib.rs`, add after `pub mod stamp;`:

```rust
pub mod structure;
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p taiyaku-library structure:: -- --test-threads=8`
Expected: both FAIL, with "reading the structure version: no such table: structure_version".

- [ ] **Step 3: Write the migration**

Create `crates/taiyaku-library/src/schema/014_structure_version.sql`:

```sql
-- Part A of docs/superpowers/specs/2026-10-04-day-to-day-deployment-design.md
-- (§2.2): a counter that moves whenever anything a served `Work` is built
-- from is written, so `taiyaku serve` can tell its in-memory works are stale
-- without every job that changes them having to say so.
--
-- **The six tables are exactly the ones `project::sidecar` reads**: `work`,
-- `edition`, `member`, `page_flag`, `alignment` and `run`. A table added to
-- that projection later needs its three triggers too, in a migration of its
-- own. `structure::tests` lists the six by name and would not know about a
-- seventh.
--
-- **Triggers, not a call in each writer**, because a trigger fires for every
-- connection and every process: a `taiyaku library scan` run from a shell
-- beside the server is seen exactly as a scan the server ran itself.
--
-- **`progress` and `note` have none, deliberately.** They are written on a
-- page turn and a keystroke, and nothing a `Work` holds comes from them.
--
-- One row, pinned by `CHECK (id = 1)`, so "the counter" names one value.
CREATE TABLE structure_version (
  id INTEGER PRIMARY KEY CHECK (id = 1),
  n  INTEGER NOT NULL
);
INSERT INTO structure_version (id, n) VALUES (1, 0);

CREATE TRIGGER structure_work_ins AFTER INSERT ON work BEGIN
  UPDATE structure_version SET n = n + 1;
END;
CREATE TRIGGER structure_work_upd AFTER UPDATE ON work BEGIN
  UPDATE structure_version SET n = n + 1;
END;
CREATE TRIGGER structure_work_del AFTER DELETE ON work BEGIN
  UPDATE structure_version SET n = n + 1;
END;

CREATE TRIGGER structure_edition_ins AFTER INSERT ON edition BEGIN
  UPDATE structure_version SET n = n + 1;
END;
CREATE TRIGGER structure_edition_upd AFTER UPDATE ON edition BEGIN
  UPDATE structure_version SET n = n + 1;
END;
CREATE TRIGGER structure_edition_del AFTER DELETE ON edition BEGIN
  UPDATE structure_version SET n = n + 1;
END;

CREATE TRIGGER structure_member_ins AFTER INSERT ON member BEGIN
  UPDATE structure_version SET n = n + 1;
END;
CREATE TRIGGER structure_member_upd AFTER UPDATE ON member BEGIN
  UPDATE structure_version SET n = n + 1;
END;
CREATE TRIGGER structure_member_del AFTER DELETE ON member BEGIN
  UPDATE structure_version SET n = n + 1;
END;

CREATE TRIGGER structure_page_flag_ins AFTER INSERT ON page_flag BEGIN
  UPDATE structure_version SET n = n + 1;
END;
CREATE TRIGGER structure_page_flag_upd AFTER UPDATE ON page_flag BEGIN
  UPDATE structure_version SET n = n + 1;
END;
CREATE TRIGGER structure_page_flag_del AFTER DELETE ON page_flag BEGIN
  UPDATE structure_version SET n = n + 1;
END;

CREATE TRIGGER structure_alignment_ins AFTER INSERT ON alignment BEGIN
  UPDATE structure_version SET n = n + 1;
END;
CREATE TRIGGER structure_alignment_upd AFTER UPDATE ON alignment BEGIN
  UPDATE structure_version SET n = n + 1;
END;
CREATE TRIGGER structure_alignment_del AFTER DELETE ON alignment BEGIN
  UPDATE structure_version SET n = n + 1;
END;

CREATE TRIGGER structure_run_ins AFTER INSERT ON run BEGIN
  UPDATE structure_version SET n = n + 1;
END;
CREATE TRIGGER structure_run_upd AFTER UPDATE ON run BEGIN
  UPDATE structure_version SET n = n + 1;
END;
CREATE TRIGGER structure_run_del AFTER DELETE ON run BEGIN
  UPDATE structure_version SET n = n + 1;
END;
```

In `crates/taiyaku-library/src/schema.rs`, set `pub const SCHEMA_VERSION: u32 = 14;`, change `const MIGRATIONS: [&str; 13]` to `[&str; 14]`, and append after the `013_can_stamp.sql` line:

```rust
    include_str!("schema/014_structure_version.sql"),
```

In `crates/taiyaku-library/src/schema/tests.rs`, rename `the_schema_version_is_thirteen` to `the_schema_version_is_fourteen` and change `assert_eq!(SCHEMA_VERSION, 13);` to `assert_eq!(SCHEMA_VERSION, 14);`.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p taiyaku-library structure:: schema:: -- --test-threads=8`
Expected: PASS.

- [ ] **Step 5: Watch each test fail against its own defect**

1. Delete the `structure_run_upd` trigger from the migration. Run the first test. Expected FAIL: `the counter did not move for: ["update on run"]`. Restore.
2. Change `structure_member_ins`'s body to `UPDATE structure_version SET n = n;`. Expected FAIL naming `"insert on member"`. Restore.
3. Add `CREATE TRIGGER structure_progress_ins AFTER INSERT ON progress BEGIN UPDATE structure_version SET n = n + 1; END;` to the migration. Run the second test. Expected FAIL: `assertion `left == right` failed` (the counter moved). Restore.

Report each failure's first line.

- [ ] **Step 6: Whole crates, lint, format**

```bash
cargo fmt --all
cargo test -p taiyaku-library -p taiyaku-serve -p taiyaku-cli -- --test-threads=8
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```

Expected: all pass. Report any other test that pins the schema version or the migration count (grep `SCHEMA_VERSION` and `MIGRATIONS` across `crates/`). At `8f8cdba2` there is only `the_schema_version_is_thirteen`.

- [ ] **Step 7: Commit**

```bash
git add crates/taiyaku-library/src/schema/014_structure_version.sql crates/taiyaku-library/src/schema.rs crates/taiyaku-library/src/schema/tests.rs crates/taiyaku-library/src/structure.rs crates/taiyaku-library/src/structure/tests.rs crates/taiyaku-library/src/lib.rs
git commit -m "Migration 14: a counter that moves with every write a served work is built from"
```

---

## Task 2: The registry

**Files:**
- Create: `crates/taiyaku-serve/src/registry.rs`
- Create: `crates/taiyaku-serve/src/registry/tests.rs`
- Modify: `crates/taiyaku-serve/src/works.rs` (`Work::reproject` after `Work::update`; `from_library`'s body and doc)
- Modify: `crates/taiyaku-serve/src/lib.rs` (module list and re-exports only; `ServeState` is untouched until Task 3)

**Interfaces:**
- Consumes: `taiyaku_library::structure::version` (Task 1).
- Produces (all re-exported at the crate root as `taiyaku_serve::{Works, Snapshot, Refresh}`):
  - `pub struct Works` with `pub fn fixed(works: Vec<Work>) -> Self`,
    `pub fn library(library: &taiyaku_library::Library) -> Result<Self, String>`,
    `pub fn is_live(&self) -> bool`, `pub fn get(&self, slug: &str) -> Option<Arc<Work>>`,
    `pub fn snapshot(&self) -> Arc<Snapshot>`, `pub fn len(&self) -> usize`,
    `pub fn is_empty(&self) -> bool`,
    `pub fn refresh_if_moved(&self, library: &Library) -> Result<Refresh, String>`,
    `pub(crate) fn refresh_if_moved_with(&self, library: &Library, project: &Project<'_>) -> Result<Refresh, String>`,
    `pub fn report(&self, outcome: &Result<Refresh, String>)`.
  - `pub struct Snapshot`: `Deref<Target = [Arc<Work>]>`.
  - `pub enum Refresh { Unmoved, Refreshed { kept: Vec<String> } }` (`Debug, PartialEq, Eq`).
  - `pub(crate) type Project<'a> = dyn Fn(&rusqlite::Connection, i64) -> Result<taiyaku_core::Sidecar, String> + 'a;`
  - `pub(crate) struct WorkRow { pub(crate) id: i64, pub(crate) slug: String, .. }` with `pub(crate) fn path(&self) -> PathBuf`, and `pub(crate) fn work_rows(conn: &Connection) -> Result<Vec<WorkRow>, String>`.
  - `Work::reproject(&self, read: impl FnOnce() -> Result<Sidecar, String>) -> Result<(), String>` (`pub(crate)`).

- [ ] **Step 1: Write the failing tests**

Create `crates/taiyaku-serve/src/registry/tests.rs`:

```rust
//! The registry on its own, through the router, and kept current by its
//! refresher.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::time::Duration;

use taiyaku_library::Library;

use super::{Refresh, Works, complaint, notice};
use crate::test_support::Fixture;

/// A file-backed library. Only a file gives a second connection that is a
/// real second connection; an in-memory library's "working" connection is
/// its shared one.
fn file_library(fixture: &Fixture) -> (Arc<Library>, PathBuf) {
    let db = fixture.path().join("library.db");
    let library = Library::open(&db).expect("opens a file-backed library");
    (Arc::new(library), db)
}

/// Import one scanned work through a *separate* `Library` over the same
/// file, which is another process as far as the registry's own connections
/// can tell, and answer its row id and slug.
fn import_elsewhere(db: &Path, title: &str, sha: &str) -> (i64, String) {
    let other = Library::open(db).expect("a second library over the same file");
    let conn = other.conn();
    let doc = serde_json::json!({
        "taiyaku": 1,
        "work": { "title": title, "reading": "rtl" },
        "editions": { "raw": { "label": "raw", "members": [
            { "path": format!("/abs/shelf/{title} - v01"), "sha256": sha, "pages": 4 }
        ] } },
        "hub": "raw",
        "alignments": []
    });
    let doc = taiyaku_core::Sidecar::from_json(&doc.to_string()).expect("parses");
    let id = taiyaku_library::import::scanned(&conn, &doc, None).expect("imports as a scan");
    let slug = conn
        .query_row("SELECT slug FROM work WHERE id = ?1", [id], |row| row.get(0))
        .expect("reads the slug");
    (id, slug)
}

/// Run `sql`, binding `id` as `?1`, through a separate `Library` over the
/// same file.
fn write_elsewhere(db: &Path, sql: &str, id: i64) {
    let other = Library::open(db).expect("a second library over the same file");
    let changed = other.conn().execute(sql, [id]).expect(sql);
    assert_eq!(changed, 1, "`{sql}` must change one row");
}

#[test]
fn a_fixed_registry_never_reads_the_library_again() {
    let fixture = Fixture::new("registry-fixed");
    let (library, db) = file_library(&fixture);
    let works = Works::fixed(Vec::new());
    let (_, slug) = import_elsewhere(&db, "Arrived", "a1");

    assert_eq!(
        works.refresh_if_moved(&library).expect("answers"),
        Refresh::Unmoved
    );
    assert!(
        works.get(&slug).is_none(),
        "`serve foo/taiyaku.json` serves what it was handed, not what the library gained since"
    );
}

#[test]
fn len_and_is_empty_count_the_works_held() {
    let fixture = Fixture::new("registry-len");
    let (library, db) = file_library(&fixture);
    assert!(Works::fixed(Vec::new()).is_empty());
    import_elsewhere(&db, "One", "o1");
    import_elsewhere(&db, "Two", "t2");
    let works = Works::library(&library).expect("projects the library");
    assert_eq!(works.len(), 2);
    assert!(!works.is_empty());
}

#[test]
fn a_live_registry_serves_a_work_another_process_added_once_refreshed() {
    let fixture = Fixture::new("registry-added");
    let (library, db) = file_library(&fixture);
    let (_, first) = import_elsewhere(&db, "First", "f1");
    let works = Works::library(&library).expect("projects the library");
    assert!(works.get(&first).is_some());

    let (_, second) = import_elsewhere(&db, "Second", "s2");
    assert!(
        works.get(&second).is_none(),
        "`get` never reads the library; only a refresh does"
    );
    assert!(matches!(
        works.refresh_if_moved(&library).expect("refreshes"),
        Refresh::Refreshed { .. }
    ));
    assert_eq!(
        works.get(&second).expect("served after the refresh").sidecar().work.title,
        "Second"
    );
    assert_eq!(
        works.refresh_if_moved(&library).expect("answers"),
        Refresh::Unmoved,
        "a second refresh with nothing written in between reads nothing"
    );
}

#[test]
fn a_work_deleted_from_the_library_leaves_the_registry_on_refresh() {
    let fixture = Fixture::new("registry-deleted");
    let (library, db) = file_library(&fixture);
    let (_, first) = import_elsewhere(&db, "Kept", "k1");
    let (gone_id, gone) = import_elsewhere(&db, "Gone", "g1");
    let works = Works::library(&library).expect("projects the library");
    assert!(works.get(&gone).is_some());

    write_elsewhere(&db, "DELETE FROM work WHERE id = ?1", gone_id);
    works.refresh_if_moved(&library).expect("refreshes");

    assert!(works.get(&gone).is_none(), "a deleted work is still served");
    assert!(works.get(&first).is_some(), "and the one left must stay");
}

#[test]
fn a_held_work_keeps_its_identity_and_takes_the_new_document() {
    let fixture = Fixture::new("registry-identity");
    let (library, db) = file_library(&fixture);
    let (id, slug) = import_elsewhere(&db, "Before", "b1");
    let works = Works::library(&library).expect("projects the library");
    let held = works.get(&slug).expect("served");

    write_elsewhere(&db, "UPDATE work SET title = 'Retitled' WHERE id = ?1", id);
    works.refresh_if_moved(&library).expect("refreshes");

    let now = works.get(&slug).expect("still served");
    assert!(
        Arc::ptr_eq(&held, &now),
        "a refresh must not mint a second `Work` over one row: its notes lock and \
         its correction lock would then be two locks"
    );
    assert_eq!(held.sidecar().work.title, "Retitled");
}

#[test]
fn a_write_that_lands_while_a_refresh_reads_is_read_by_the_next_one() {
    let fixture = Fixture::new("registry-ordering");
    let (library, db) = file_library(&fixture);
    let (id, slug) = import_elsewhere(&db, "Ordered", "o1");
    let works = Works::library(&library).expect("projects the library");
    write_elsewhere(&db, "UPDATE work SET title = 'Before' WHERE id = ?1", id);

    // The first time it is asked, this reads the work and *then* writes, so
    // the write commits after the refresh has read what it serves.
    let wrote = AtomicBool::new(false);
    let project = |conn: &rusqlite::Connection, work_id: i64| {
        let read = taiyaku_library::project::sidecar(conn, work_id);
        if !wrote.swap(true, Ordering::SeqCst) {
            write_elsewhere(&db, "UPDATE work SET title = 'During' WHERE id = ?1", id);
        }
        read
    };
    works
        .refresh_if_moved_with(&library, &project)
        .expect("refreshes");
    assert_eq!(works.get(&slug).expect("served").sidecar().work.title, "Before");

    assert!(
        matches!(
            works.refresh_if_moved(&library).expect("refreshes"),
            Refresh::Refreshed { .. }
        ),
        "the write that landed during the last refresh must still be ahead of what it recorded"
    );
    assert_eq!(works.get(&slug).expect("served").sidecar().work.title, "During");
}

/// **A correction racing a refresh survives it.** The correction holds the
/// work's write lock across its own write, as `patch_alignments` does. The
/// refresh starts while it is held, and has a reason to read, because an
/// unrelated write has already moved the counter.
///
/// A correct refresh waits on that work's lock *before* reading it, so it
/// reads after the correction commits. A refresh that read first would read
/// the work before the correction's row existed, then swap that stale
/// document in once the lock came free, undoing the correction in memory
/// while the rows still held it.
///
/// `projected` reports when the refresh reads. The 500 ms wait only decides
/// how long a correct refresh is given to *not* read early. The correct path
/// passes whatever the timing, and the defect is caught whenever it reads
/// inside that window.
#[test]
fn a_correction_racing_a_refresh_survives_it() {
    let fixture = Fixture::new("registry-race");
    let (library, db) = file_library(&fixture);
    let (id, slug) = import_elsewhere(&db, "Raced", "r1");
    let works = Arc::new(Works::library(&library).expect("projects the library"));
    let held = works.get(&slug).expect("served");

    let (holding_tx, holding_rx) = mpsc::channel::<()>();
    let (go_tx, go_rx) = mpsc::channel::<()>();
    let (projected_tx, projected_rx) = mpsc::channel::<()>();

    let correction = {
        let held = Arc::clone(&held);
        let db = db.clone();
        std::thread::spawn(move || {
            held.update(|_current| {
                holding_tx.send(()).expect("signals that it holds the lock");
                go_rx.recv().expect("waits to be let go");
                let other = Library::open(&db).expect("a second library");
                let conn = other.conn();
                conn.execute(
                    "INSERT INTO page_flag (edition_id, kind, page) \
                     SELECT id, 'spread', 1 FROM edition WHERE work_id = ?1",
                    [id],
                )
                .expect("writes the correction's row");
                let next = taiyaku_library::project::sidecar(&conn, id)
                    .expect("projects the corrected work");
                Ok((Arc::new(next), ()))
            })
            .expect("the correction lands");
        })
    };
    holding_rx.recv().expect("the correction holds the work's lock");

    write_elsewhere(&db, "UPDATE work SET title = title WHERE id = ?1", id);

    let refresh = {
        let works = Arc::clone(&works);
        let library = Arc::clone(&library);
        std::thread::spawn(move || {
            let project = |conn: &rusqlite::Connection, work_id: i64| {
                let read = taiyaku_library::project::sidecar(conn, work_id);
                let _ = projected_tx.send(());
                read
            };
            works
                .refresh_if_moved_with(&library, &project)
                .expect("refreshes")
        })
    };

    let read_early = projected_rx
        .recv_timeout(Duration::from_millis(500))
        .is_ok();
    go_tx.send(()).expect("lets the correction go");
    correction.join().expect("the correction thread");
    refresh.join().expect("the refresh thread");

    let now = works.get(&slug).expect("still served");
    assert!(Arc::ptr_eq(&held, &now), "the refresh replaced the work it raced");
    assert_eq!(
        now.sidecar().editions["raw"].spreads,
        vec![1],
        "the refresh undid a correction that committed after it began{}",
        if read_early {
            ": it read the work before taking the work's lock"
        } else {
            ""
        }
    );
}

#[test]
fn a_work_whose_rows_will_not_project_keeps_the_document_it_had() {
    let fixture = Fixture::new("registry-unprojectable");
    let (library, db) = file_library(&fixture);
    let (id, slug) = import_elsewhere(&db, "Steady", "s1");
    let works = Works::library(&library).expect("projects the library");

    write_elsewhere(&db, "UPDATE work SET reading = 'sideways' WHERE id = ?1", id);
    let refreshed = works.refresh_if_moved(&library).expect("refreshes the rest");

    let Refresh::Refreshed { kept } = refreshed else {
        panic!("the counter moved, so the refresh must have read: {refreshed:?}");
    };
    assert_eq!(kept.len(), 1, "{kept:?}");
    assert!(kept[0].contains(&slug), "{kept:?}");
    assert_eq!(
        works.get(&slug).expect("still served").sidecar().work.title,
        "Steady",
        "a running server keeps serving the document it had"
    );
}

#[test]
fn a_work_that_will_not_project_is_left_out_at_startup_and_the_rest_is_served() {
    let fixture = Fixture::new("registry-startup-lenient");
    let (library, db) = file_library(&fixture);
    let (broken, broken_slug) = import_elsewhere(&db, "Broken", "b1");
    let (_, steady_slug) = import_elsewhere(&db, "Steady", "s1");
    write_elsewhere(&db, "UPDATE work SET reading = 'sideways' WHERE id = ?1", broken);

    let works = Works::library(&library).expect("startup serves the rest");
    assert!(works.get(&broken_slug).is_none(), "the broken work is left out");
    assert_eq!(
        works.get(&steady_slug).expect("the rest is served").sidecar().work.title,
        "Steady"
    );
}

#[test]
fn a_complaint_is_printed_once_and_its_recovery_once() {
    let mut last = None;
    assert_eq!(
        notice(&mut last, Some("disk gone".to_string())).as_deref(),
        Some("taiyaku: could not refresh the served works: disk gone")
    );
    assert_eq!(notice(&mut last, Some("disk gone".to_string())), None);
    assert_eq!(
        notice(&mut last, None).as_deref(),
        Some("taiyaku: the served works are being refreshed again")
    );
    assert_eq!(notice(&mut last, None), None);
}

#[test]
fn a_refresh_that_read_nothing_neither_raises_nor_clears_a_complaint() {
    assert_eq!(complaint(&Ok(Refresh::Unmoved)), None);
    assert_eq!(
        complaint(&Ok(Refresh::Refreshed { kept: Vec::new() })),
        Some(None)
    );
    assert_eq!(
        complaint(&Err("locked".to_string())),
        Some(Some("locked".to_string()))
    );
    let kept = complaint(&Ok(Refresh::Refreshed {
        kept: vec!["`odd`: bad reading".to_string()],
    }));
    assert!(
        matches!(&kept, Some(Some(text)) if text.contains("`odd`")),
        "{kept:?}"
    );
}
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p taiyaku-serve registry:: -- --test-threads=8`
Expected: does not compile (`registry` does not exist).

- [ ] **Step 3: Write `Work::reproject`**

In `crates/taiyaku-serve/src/works.rs`, after `Work::update`'s closing brace, inside `impl Work`:

```rust
    /// Replace the document with one read back from the library, holding the
    /// same exclusive lock [`Self::update`] holds for a correction.
    ///
    /// **The read happens inside the lock, not before it.** A refresh that
    /// read first and swapped under the lock afterwards could swap in a
    /// document read before a correction committed, after that correction had
    /// already replaced it, undoing the correction in memory while the rows
    /// still held it. Holding the lock across the read makes a correction and
    /// a refresh run one after the other.
    ///
    /// # Errors
    ///
    /// Returns whatever `read` returns, and leaves the held document as it was.
    pub(crate) fn reproject(
        &self,
        read: impl FnOnce() -> Result<Sidecar, String>,
    ) -> Result<(), String> {
        let mut held = match self.sidecar.write() {
            Ok(held) => held,
            Err(poisoned) => poisoned.into_inner(),
        };
        *held = std::sync::Arc::new(read()?);
        Ok(())
    }
```

- [ ] **Step 4: Write `registry.rs`**

Create `crates/taiyaku-serve/src/registry.rs`:

```rust
//! The works a server serves, kept in step with the library it serves them
//! from.
//!
//! **Why this is not a `Vec<Work>` built once.** It was, until Part A of
//! `docs/superpowers/specs/2026-10-04-day-to-day-deployment-design.md`. Every
//! `/api/library/*` route read the database and saw a scan the moment it
//! committed, while `/api/works`, the reader's routes and OPDS read a list
//! built at startup. So a volume a scan had just derived was listed by the
//! library and refused by the reader until the process restarted.
//!
//! **How it knows the library moved.** Migration 14 keeps a counter that
//! triggers bump on every write to a table a `Work` is projected from
//! ([`taiyaku_library::structure`]). A live registry remembers the value it
//! last read at and reads again when the counter differs. Triggers fire for
//! every connection in every process, so nothing that changes the library
//! has to remember to say so.
//!
//! **Two kinds.** [`Works::library`] is live: `serve` with no paths, the
//! whole library. [`Works::fixed`] never reads the library again: `serve
//! foo/taiyaku.json` serves the works it was handed (O61 is that mode's own
//! row), and so does every fixture that builds its works by hand.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex, PoisonError, RwLock};

use rusqlite::Connection;
use taiyaku_core::Sidecar;
use taiyaku_library::Library;

use crate::Work;

/// Reads one work's document out of the library. `project::sidecar` in
/// production; a wrapper around it in the tests that need to see when a
/// refresh reads.
pub(crate) type Project<'a> = dyn Fn(&Connection, i64) -> Result<Sidecar, String> + 'a;

/// What [`Works::seen`] holds before anything has been read. No value of the
/// counter equals it, so the first refresh always reads.
const NEVER: i64 = i64::MIN;

/// One `work` row: as much of it as a [`Work`] is keyed and located by.
#[derive(Debug)]
pub(crate) struct WorkRow {
    pub(crate) id: i64,
    pub(crate) slug: String,
    sidecar_path: Option<String>,
}

impl WorkRow {
    /// Where the work's `taiyaku.json` is, or an empty path for a scanned
    /// work, which has none. [`Work::has_document`] reads that as "no
    /// document".
    pub(crate) fn path(&self) -> PathBuf {
        self.sidecar_path
            .as_deref()
            .map_or_else(PathBuf::new, PathBuf::from)
    }
}

/// Every `work` row, in id order.
///
/// # Errors
///
/// Returns an error if the rows cannot be read.
pub(crate) fn work_rows(conn: &Connection) -> Result<Vec<WorkRow>, String> {
    let mut stmt = conn
        .prepare("SELECT id, slug, sidecar_path FROM work ORDER BY id")
        .map_err(|e| format!("listing the library's works: {e}"))?;
    stmt.query_map([], |row| {
        Ok(WorkRow {
            id: row.get(0)?,
            slug: row.get(1)?,
            sidecar_path: row.get(2)?,
        })
    })
    .map_err(|e| format!("listing the library's works: {e}"))?
    .collect::<rusqlite::Result<_>>()
    .map_err(|e| format!("reading the library's works: {e}"))
}

/// The works at one moment: in id order for a listing, and by slug for a
/// lookup.
#[derive(Debug, Default)]
pub struct Snapshot {
    order: Vec<Arc<Work>>,
    by_slug: HashMap<String, Arc<Work>>,
}

impl Snapshot {
    fn new(order: Vec<Arc<Work>>) -> Self {
        let by_slug = order
            .iter()
            .map(|work| (work.id.clone(), Arc::clone(work)))
            .collect();
        Self { order, by_slug }
    }
}

impl std::ops::Deref for Snapshot {
    type Target = [Arc<Work>];

    fn deref(&self) -> &[Arc<Work>] {
        &self.order
    }
}

/// What one call to [`Works::refresh_if_moved`] did.
#[derive(Debug, PartialEq, Eq)]
pub enum Refresh {
    /// The counter had not moved, or the registry is fixed: nothing was read.
    Unmoved,
    /// Every work was read again. `kept` names each one whose rows would not
    /// project. It keeps the document it had or, if it is new, is not served
    /// until its rows do project.
    Refreshed { kept: Vec<String> },
}

/// The works a server serves. See the module's own doc comment.
#[derive(Debug)]
pub struct Works {
    held: RwLock<Arc<Snapshot>>,
    live: bool,
    /// The counter's value when [`Self::held`] was last read, or [`NEVER`].
    seen: AtomicI64,
    /// One refresh at a time. The refresher's tick and a lookup's miss can
    /// both ask at once, and the second only has to find the counter
    /// already caught up with.
    refreshing: Mutex<()>,
    /// The last complaint [`Self::report`] printed, so a failure is said once
    /// rather than every two seconds.
    last_notice: Mutex<Option<String>>,
}

impl Works {
    fn empty(live: bool) -> Self {
        Self {
            held: RwLock::new(Arc::default()),
            live,
            seen: AtomicI64::new(NEVER),
            refreshing: Mutex::new(()),
            last_notice: Mutex::new(None),
        }
    }

    /// A registry that serves `works` and never reads the library again.
    #[must_use]
    pub fn fixed(works: Vec<Work>) -> Self {
        let registry = Self::empty(false);
        *registry.held.write().unwrap_or_else(PoisonError::into_inner) =
            Arc::new(Snapshot::new(works.into_iter().map(Arc::new).collect()));
        registry
    }

    /// A live registry over every work in `library`.
    ///
    /// **A work whose rows will not project is left out and named on stderr**,
    /// the way a running refresh names one. Refusing the whole library would
    /// turn one bad row into a restart loop under an auto-deploy (spec §2.3).
    ///
    /// # Errors
    ///
    /// Returns an error only if the library cannot be read at all.
    pub fn library(library: &Library) -> Result<Self, String> {
        let works = Self::empty(true);
        let outcome = works.refresh(library, &taiyaku_library::project::sidecar);
        works.report(&outcome);
        outcome?;
        Ok(works)
    }

    /// Whether this registry reads the library again when it moves.
    #[must_use]
    pub fn is_live(&self) -> bool {
        self.live
    }

    /// The works as they stand. Never reads the library: a read lock held
    /// only long enough to clone an `Arc`.
    #[must_use]
    pub fn snapshot(&self) -> Arc<Snapshot> {
        Arc::clone(&self.held.read().unwrap_or_else(PoisonError::into_inner))
    }

    /// The work `slug` names, if this registry holds it now. Never reads the
    /// library; `ServeState::find` is the lookup that refreshes on a miss.
    #[must_use]
    pub fn get(&self, slug: &str) -> Option<Arc<Work>> {
        self.snapshot().by_slug.get(slug).cloned()
    }

    /// How many works are held.
    #[must_use]
    pub fn len(&self) -> usize {
        self.snapshot().len()
    }

    /// Whether none are.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.snapshot().is_empty()
    }

    /// Read every work again if the library's structure has moved since the
    /// last read. A fixed registry answers [`Refresh::Unmoved`] at once.
    ///
    /// **Blocks: it reads the database.** Call it from the blocking pool or
    /// a thread of its own, never from a runtime worker.
    ///
    /// # Errors
    ///
    /// Returns an error if the library cannot be read at all. A work whose
    /// rows will not project is not an error here: it is named in
    /// [`Refresh::Refreshed`]'s `kept` and keeps the document it had.
    pub fn refresh_if_moved(&self, library: &Library) -> Result<Refresh, String> {
        self.refresh_if_moved_with(library, &taiyaku_library::project::sidecar)
    }

    /// [`Self::refresh_if_moved`], reading each work through `project`.
    pub(crate) fn refresh_if_moved_with(
        &self,
        library: &Library,
        project: &Project<'_>,
    ) -> Result<Refresh, String> {
        if !self.live {
            return Ok(Refresh::Unmoved);
        }
        self.refresh(library, project)
    }

    fn refresh(&self, library: &Library, project: &Project<'_>) -> Result<Refresh, String> {
        let _one_at_a_time = self
            .refreshing
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        let conn = library.working()?;
        // **Read before the works, never after.** A write that commits while
        // the works below are being read leaves the counter ahead of what is
        // recorded as seen, and the next refresh reads again. Read
        // afterwards, and that write would be recorded as seen without ever
        // having been read.
        let now = taiyaku_library::structure::version(&conn)?;
        if self.seen.load(Ordering::Acquire) == now {
            return Ok(Refresh::Unmoved);
        }
        let before = self.snapshot();
        let mut order = Vec::new();
        let mut kept = Vec::new();
        for row in work_rows(&conn)? {
            let path = row.path();
            let held = before
                .by_slug
                .get(&row.slug)
                .filter(|held| held.path == path);
            let read = match held {
                // **The same `Work`, read again under its own lock**, so its
                // notes lock and its correction lock stay the ones every
                // request in flight already holds (`Work::reproject`).
                Some(held) => held
                    .reproject(|| read_one(&conn, project, row.id))
                    .map(|()| Arc::clone(held)),
                // New, or its document moved. `Work::path` is fixed for a
                // `Work`'s life, so a moved document is a new `Work`.
                None => read_one(&conn, project, row.id)
                    .map(|sidecar| Arc::new(Work::new(row.slug.clone(), path, sidecar))),
            };
            match read {
                Ok(work) => order.push(work),
                Err(why) => {
                    kept.push(format!("`{}`: {why}", row.slug));
                    if let Some(previous) = before.by_slug.get(&row.slug) {
                        order.push(Arc::clone(previous));
                    }
                }
            }
        }
        *self.held.write().unwrap_or_else(PoisonError::into_inner) =
            Arc::new(Snapshot::new(order));
        self.seen.store(now, Ordering::Release);
        Ok(Refresh::Refreshed { kept })
    }

    /// Print what one refresh means to whoever is watching the server's
    /// stderr: once per change, never once per tick.
    pub fn report(&self, outcome: &Result<Refresh, String>) {
        let Some(now) = complaint(outcome) else {
            return;
        };
        let mut last = self
            .last_notice
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if let Some(line) = notice(&mut last, now) {
            eprintln!("{line}");
        }
    }
}

/// One work's document, read inside a read transaction of its own, so it
/// comes from one committed state of the library rather than from both
/// sides of a write that landed between two of `project`'s queries.
///
/// **`BEGIN DEFERRED`, spelled out.** `Library::configure` makes every
/// `unchecked_transaction` on its connections `IMMEDIATE`, which takes the
/// write lock. That is right for the writers it was set for; for a read it
/// would make every refresh wait out, and hold up, whichever writer was
/// running.
fn read_one(conn: &Connection, project: &Project<'_>, id: i64) -> Result<Sidecar, String> {
    let read = rusqlite::Transaction::new_unchecked(conn, rusqlite::TransactionBehavior::Deferred)
        .map_err(|e| format!("starting a read of work {id}: {e}"))?;
    let reading: &Connection = &read;
    project(reading, id)
}

/// What a refresh's outcome says: `None` when it read nothing, so it can
/// neither raise a complaint nor clear one; `Some(None)` when it read every
/// work cleanly; `Some(Some(why))` otherwise.
fn complaint(outcome: &Result<Refresh, String>) -> Option<Option<String>> {
    match outcome {
        Ok(Refresh::Unmoved) => None,
        Ok(Refresh::Refreshed { kept }) if kept.is_empty() => Some(None),
        Ok(Refresh::Refreshed { kept }) => Some(Some(format!(
            "these works could not be read, and each is served as it last was, or not at all \
             if it never was: {}",
            kept.join("; ")
        ))),
        Err(why) => Some(Some(why.clone())),
    }
}

/// The line to print for `now`, given that `last` was the last thing
/// printed, or nothing when it would only repeat it.
fn notice(last: &mut Option<String>, now: Option<String>) -> Option<String> {
    if *last == now {
        return None;
    }
    let line = match &now {
        Some(why) => format!("taiyaku: could not refresh the served works: {why}"),
        None => "taiyaku: the served works are being refreshed again".to_string(),
    };
    *last = now;
    Some(line)
}

#[cfg(test)]
mod tests;
```

In `crates/taiyaku-serve/src/lib.rs`: add `mod registry;` to the module list (alphabetical, after `mod progress;`), and after the existing `pub use works::{…};` line add:

```rust
pub use registry::{Refresh, Snapshot, Works};
```

- [ ] **Step 5: Put `from_library` onto `work_rows`**

Replace `works::from_library`'s body (keep its signature) with:

```rust
pub fn from_library(library: &taiyaku_library::Library) -> Result<Vec<Work>, String> {
    let conn = library.conn();
    crate::registry::work_rows(&conn)?
        .into_iter()
        .map(|row| {
            let sidecar = taiyaku_library::project::sidecar(&conn, row.id)
                .map_err(|e| format!("projecting work {} (`{}`): {e}", row.id, row.slug))?;
            Ok(Work::new(row.slug.clone(), row.path(), sidecar))
        })
        .collect()
}
```

Add one paragraph to its doc comment, after the "**A NULL `sidecar_path` becomes an empty `PathBuf`**" paragraph:

```rust
/// **No longer what `serve` itself runs.** `serve` with no paths builds a
/// live [`crate::Works`] through `Works::library`, which reads the same rows
/// through the same [`crate::registry::work_rows`]. This stays for the
/// fixtures that build a fixed registry over a library, the way `load_works`
/// stays for the ones built from files.
```

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cargo test -p taiyaku-serve registry:: works:: -- --test-threads=8`
Expected: PASS.

- [ ] **Step 7: Watch each test fail against its own defect**

Apply one at a time, run the named test, report the failure's first line, restore:

1. `fixed`: construct with `Self::empty(true)` → `a_fixed_registry_never_reads_the_library_again` fails (the refresh reads).
2. `refresh`: record a counter read *after* the works rather than before. Replace `self.seen.store(now, Ordering::Release);` with `self.seen.store(taiyaku_library::structure::version(&conn)?, Ordering::Release);` → `a_write_that_lands_while_a_refresh_reads_is_read_by_the_next_one` fails at the `matches!`.
3. `refresh`: in the `Some(held)` arm, replace `held.reproject(|| read_one(&conn, project, row.id)).map(|()| Arc::clone(held))` with `{ let doc = read_one(&conn, project, row.id); doc.and_then(|d| held.reproject(|| Ok(d))).map(|()| Arc::clone(held)) }` (read first, swap under the lock) → `a_correction_racing_a_refresh_survives_it` fails, its message ending ": it read the work before taking the work's lock".
4. `refresh`: always take the `None` arm (`let held: Option<&Arc<Work>> = None;`) → `a_held_work_keeps_its_identity_and_takes_the_new_document` fails on `ptr_eq`. Report what else fails too.
5. `refresh`: before installing, append every `before` work not in `order` (`for w in before.iter() { if !order.iter().any(|o| o.id == w.id) { order.push(Arc::clone(w)); } }`) → `a_work_deleted_from_the_library_leaves_the_registry_on_refresh` fails.
6. `refresh`: in the `Err(why)` arm, drop the `if let Some(previous)` block → `a_work_whose_rows_will_not_project_keeps_the_document_it_had` fails.
7. `library`: refuse a kept work. After `works.report(&outcome);` insert `if let Ok(Refresh::Refreshed { kept }) = &outcome { if !kept.is_empty() { return Err(kept.join("; ")); } }` → `a_work_that_will_not_project_is_left_out_at_startup_and_the_rest_is_served` fails at its `expect`.
8. `refresh`: delete `self.seen.store(now, Ordering::Release);` → `a_live_registry_serves_a_work_another_process_added_once_refreshed` fails at its last assertion.
9. `notice`: delete the `if *last == now { return None; }` guard → `a_complaint_is_printed_once_and_its_recovery_once` fails.
10. `complaint`: make `Ok(Refresh::Unmoved)` answer `Some(None)` → `a_refresh_that_read_nothing_neither_raises_nor_clears_a_complaint` fails.
11. `len`: return `0` → `len_and_is_empty_count_the_works_held` fails.

If defect 3 does not fail the race test, stop and report: the test's interleaving is wrong, and the fix is to the test's design, not to the defect.

- [ ] **Step 8: Whole crates, lint, format**

As in Task 1, Step 6. `ServeState` still holds a `Vec<Work>` at this point. Nothing outside the registry tests uses `Works` yet; it is public, so nothing warns as dead.

- [ ] **Step 9: Commit**

```bash
git add crates/taiyaku-serve/src/registry.rs crates/taiyaku-serve/src/registry/tests.rs crates/taiyaku-serve/src/works.rs crates/taiyaku-serve/src/lib.rs
git commit -m "A works registry that reads the library again when its structure moves"
```

---

## Task 3: `ServeState` onto the registry

**Files:**
- Modify: `crates/taiyaku-serve/src/lib.rs` (`ServeState.works`, its doc, `find`, `find_blocking`; `serve()`'s `works` parameter, `forget_all_works` and `resolve_all_works` call sites, `resolve_all_works`'s signature)
- Modify: `crates/taiyaku-serve/src/routes.rs`, `opds.rs`, `download.rs` (every lookup and listing)
- Modify: `crates/taiyaku-serve/src/test_support.rs` and every test file below
- Modify: `crates/taiyaku-cli/src/main.rs` (`serve_works`, the `serve` command's print loop), `crates/taiyaku-cli/src/tests.rs`
- Test: `crates/taiyaku-serve/src/registry/tests.rs` (append)

**Interfaces:**
- Consumes: `Works`, `Snapshot`, `Refresh` (Task 2).
- Produces:
  - `ServeState { pub works: Works, … }`.
  - `pub async fn find(self: &Arc<Self>, id: &str) -> Result<Arc<Work>, String>`.
  - `pub fn find_blocking(&self, id: &str) -> Result<Arc<Work>, String>`.
  - Both refuse with `` no work `{id}` ``, unchanged from `work_or_missing`.
  - `ServeState::work` and `ServeState::work_or_missing` are **deleted**.
  - `taiyaku_serve::serve(library, works: Works, …)`.
  - `test_support::{scanned_file_backed_library, router_over_a_live_library, add_a_volume_elsewhere}` (`pub(crate)`), used by Task 4.

- [ ] **Step 1: Write the failing router tests**

Append to `crates/taiyaku-serve/src/registry/tests.rs`:

```rust
// --- through the router ----------------------------------------------------

#[tokio::test]
async fn a_volume_derived_after_startup_opens_on_the_first_request_for_it() {
    let (router, state, fixture) = crate::test_support::router_over_a_live_library();
    let slug = crate::test_support::add_a_volume_elsewhere(&fixture, "Scanned - v02");
    assert!(
        state.works.get(&slug).is_none(),
        "no refresh has run, so only the lookup's own miss path can find it"
    );

    let response = crate::test_support::get(&router, &format!("/api/works/{slug}")).await;
    let status = response.status();
    let body = crate::test_support::body_string(response).await;
    assert_eq!(status, axum::http::StatusCode::OK, "{body}");
    assert!(body.contains("\"editions\""), "{body}");
}

#[tokio::test]
async fn a_route_that_looks_its_work_up_on_the_blocking_pool_finds_it_too() {
    let (router, state, fixture) = crate::test_support::router_over_a_live_library();
    let slug = crate::test_support::add_a_volume_elsewhere(&fixture, "Scanned - v02");
    assert!(state.works.get(&slug).is_none());

    let response =
        crate::test_support::get(&router, &format!("/api/works/{slug}/progress")).await;
    let status = response.status();
    let body = crate::test_support::body_string(response).await;
    assert_eq!(
        status,
        axum::http::StatusCode::NO_CONTENT,
        "a known work with no position answers 204, an unknown one 404: {body}"
    );
}

#[tokio::test]
async fn a_work_deleted_after_startup_stops_answering_once_refreshed() {
    let (router, state, fixture) = crate::test_support::router_over_a_live_library();
    let slug = state.works.snapshot()[0].id.clone();
    let other = Library::open(&fixture.path().join("db").join("library.db"))
        .expect("a second library over the same file");
    let deleted = other
        .conn()
        .execute("DELETE FROM work WHERE slug = ?1", [&slug])
        .expect("deletes the work");
    assert_eq!(deleted, 1);

    state
        .works
        .refresh_if_moved(&state.library)
        .expect("refreshes");
    let response = crate::test_support::get(&router, &format!("/api/works/{slug}")).await;
    assert_eq!(response.status(), axum::http::StatusCode::NOT_FOUND);
}
```

- [ ] **Step 2: The fixtures' mechanical edits**

These run before any production change, so the compiler afterwards points only at real call sites.

1. Wrap every `ServeState` literal's `works` field, and `state.works[…]`, in the test files:

```bash
perl -0pi -e 's/^(\s+)works,\n/$1works: crate::Works::fixed(works),\n/mg' \
  crates/taiyaku-serve/src/test_support.rs crates/taiyaku-serve/src/corpus.rs \
  crates/taiyaku-serve/src/auth/tests.rs crates/taiyaku-serve/src/opds/tests.rs \
  crates/taiyaku-serve/src/routes/tests.rs
perl -0pi -e 's/^(\s+)works: (vec!\[[^\n]*\]|Vec::new\(\)),\n/$1works: crate::Works::fixed($2),\n/mg' \
  crates/taiyaku-serve/src/test_support.rs crates/taiyaku-serve/src/tests.rs \
  crates/taiyaku-serve/src/download/tests.rs crates/taiyaku-serve/src/config_api/tests.rs \
  crates/taiyaku-serve/src/opds/tests.rs
perl -pi -e 's/\b(\w+)\.works\[/$1.works.snapshot()[/g' \
  crates/taiyaku-serve/src/test_support.rs crates/taiyaku-serve/src/corpus.rs \
  crates/taiyaku-serve/src/source/tests.rs crates/taiyaku-serve/src/metadata/tests.rs \
  crates/taiyaku-serve/src/library_api/tests.rs crates/taiyaku-serve/src/routes/tests.rs \
  crates/taiyaku-serve/src/auth/tests.rs crates/taiyaku-serve/src/opds/tests.rs
```

`tests.rs` is left out of the first command on purpose: its one bare `works,` line is an argument to `crate::serve(…)`, not a field.

2. By hand:
   - `test_support.rs`'s `state_with_two_editions` and `opds/tests.rs` (near line 938): `state.works = vec![crate::Work::new(…)];` becomes `state.works = crate::Works::fixed(vec![crate::Work::new(…)]);`.
   - `opds/tests.rs` (near line 571): the multi-line `works: vec![crate::test_support::work_with_edition_key(…)],` becomes `works: crate::Works::fixed(vec![…]),`.
   - `tests.rs`'s two `crate::serve(` calls: the argument `works,` becomes `crate::Works::fixed(works),` and `vec![work],` becomes `crate::Works::fixed(vec![work]),`.
   - `corpus.rs` (near line 120) and `source/tests.rs` (near line 39): `let work = &state.works.snapshot()[0];` borrows a temporary, so split it into `let held = state.works.snapshot();` and `let work = &held[0];`.

3. Check that none is left:

```bash
grep -rnE "^\s+works(: vec!\[|: Vec::new\(\)|,$)" crates/taiyaku-serve/src
```

Expected: only `lib.rs` (the `serve()` literal, where `works` is already a `Works` after Step 3) and `config_api.rs` (an unrelated report field). Report any other hit.

4. Replace `test_support.rs`'s `router_over_a_file_backed_library` (the whole function, doc comment kept) with these, and add the two new helpers after it. The perl pass above has already wrapped its `works,`; this replacement supersedes that.

```rust
/// One root, one volume, scanned and derived from a real tree on disk into a
/// file-backed library: what both file-backed routers below serve.
///
/// Laid out as `{fixture}/tree/Scanned/Scanned - v01/` with the database at
/// `{fixture}/db/library.db`, which is where [`add_a_volume_elsewhere`]
/// expects to find both.
pub(crate) fn scanned_file_backed_library(
    fixture: &Fixture,
) -> std::sync::Arc<taiyaku_library::Library> {
    let tree = fixture.path().join("tree");
    let db_dir = fixture.path().join("db");
    write_pages(&tree.join("Scanned").join("Scanned - v01"), 2);
    std::fs::create_dir_all(&db_dir).expect("creates the database directory");

    let library = taiyaku_library::Library::open(&db_dir.join("library.db"))
        .expect("opens a file-backed library");
    seed_fixture_account(&library);
    {
        let conn = library.conn();
        let root = taiyaku_library::scan::add_root(&conn, &tree).expect("adds the root");
        taiyaku_library::scan::scan_root(&conn, &root).expect("scans the tree");
        taiyaku_library::derive::derive(&conn).expect("derives the library");
    }
    std::sync::Arc::new(library)
}

fn router_over(
    library: std::sync::Arc<taiyaku_library::Library>,
    works: crate::Works,
    fixture: &Fixture,
) -> (axum::Router, std::sync::Arc<ServeState>) {
    let state = std::sync::Arc::new(ServeState {
        library,
        works,
        cache_dir: fixture.path().join("covers"),
        source: std::sync::Arc::new(crate::LocalLibrary),
        log: None,
        auth: crate::session::AuthConfig::default(),
        scans: Default::default(),
        library_job: Default::default(),
    });
    let router = authenticating(crate::build_router(state.clone()), &state.library);
    (router, state)
}

pub(crate) fn router_over_a_file_backed_library()
-> (axum::Router, std::sync::Arc<ServeState>, Fixture) {
    let fixture = Fixture::new("file-backed-library");
    let library = scanned_file_backed_library(&fixture);
    let works = crate::Works::fixed(
        crate::works::from_library(&library).expect("projects the scanned works"),
    );
    let (router, state) = router_over(library, works, &fixture);
    (router, state, fixture)
}

/// [`router_over_a_file_backed_library`], over a **live** registry: the one
/// `serve` with no paths builds.
pub(crate) fn router_over_a_live_library()
-> (axum::Router, std::sync::Arc<ServeState>, Fixture) {
    let fixture = Fixture::new("live-library");
    let library = scanned_file_backed_library(&fixture);
    let works = crate::Works::library(&library).expect("projects the scanned works");
    let (router, state) = router_over(library, works, &fixture);
    (router, state, fixture)
}

/// Write `volume` beside the fixture's first volume, then scan and derive it
/// through a **separate** `Library` over the same file, which is another
/// process as far as the server's own connections can tell. Answers the
/// slug the new volume derived to.
pub(crate) fn add_a_volume_elsewhere(fixture: &Fixture, volume: &str) -> String {
    let tree = fixture.path().join("tree");
    write_pages(&tree.join("Scanned").join(volume), 3);
    let other = taiyaku_library::Library::open(&fixture.path().join("db").join("library.db"))
        .expect("a second library over the same file");
    let conn = other.conn();
    let root = taiyaku_library::scan::roots(&conn)
        .expect("lists the roots")
        .into_iter()
        .next()
        .expect("the fixture's one root");
    taiyaku_library::scan::scan_root(&conn, &root).expect("scans the tree again");
    taiyaku_library::derive::derive(&conn).expect("derives the new volume");
    let slug: String = conn
        .query_row(
            "SELECT w.slug FROM work w JOIN edition e ON e.work_id = w.id \
             JOIN member m ON m.edition_id = e.id WHERE m.path LIKE ?1",
            [format!("%{volume}%")],
            |row| row.get(0),
        )
        .expect("the new volume derived to a work");
    let first: String = conn
        .query_row(
            "SELECT w.slug FROM work w JOIN edition e ON e.work_id = w.id \
             JOIN member m ON m.edition_id = e.id WHERE m.path LIKE '%Scanned - v01%'",
            [],
            |row| row.get(0),
        )
        .expect("the first volume's work");
    assert_ne!(
        slug, first,
        "the new volume must derive to a work of its own, not join the first"
    );
    slug
}
```

If `add_a_volume_elsewhere`'s `assert_ne!` fails (the default rules fold `Scanned - v02` into `Scanned - v01`'s work), stop and report. Do not change the volume names to make it pass without saying so.

- [ ] **Step 3: The production change**

1. `lib.rs`, `ServeState.works`: replace the field's whole doc comment (from "The works this process serves through `/api/works` and the reader" through "so a caller does not have to find this comment to learn it.") and the field with:

```rust
    /// The works this process serves through `/api/works`, the reader's
    /// routes and OPDS: a [`Works`] registry, live when the server was
    /// started with no paths and fixed when it was handed sidecar paths.
    ///
    /// **Look a work up with [`Self::find`] on a runtime worker and with
    /// [`Self::find_blocking`] on the blocking pool.** A miss in a live
    /// registry asks the library whether it has moved before answering "no
    /// work", and that question reads the database.
    pub works: Works,
```

2. `lib.rs`, `impl ServeState`: delete `work` and `work_or_missing` (with their doc comments) and add:

```rust
    /// The work a route names, for code running on a runtime worker.
    ///
    /// A hit is a read lock held long enough to clone an `Arc`. A miss in a
    /// live registry first asks, on the blocking pool and never here,
    /// whether the library has moved since its works were last read, and
    /// reads them again if it has. So a volume a scan has just derived opens
    /// on the first request for it rather than after the next refresh tick.
    ///
    /// **One wording for a work that does not exist**, shared with
    /// [`Self::find_blocking`]. Every route that is addressed by work
    /// answers it the same way.
    ///
    /// # Errors
    ///
    /// Returns an error, worded `` no work `{id}` ``, if `id` names no work
    /// this server serves, after that refresh for a live registry.
    pub async fn find(self: &Arc<Self>, id: &str) -> Result<Arc<Work>, String> {
        if let Some(found) = self.works.get(id) {
            return Ok(found);
        }
        if self.works.is_live() {
            let asking = Arc::clone(self);
            let outcome =
                tokio::task::spawn_blocking(move || asking.works.refresh_if_moved(&asking.library))
                    .await
                    .unwrap_or_else(|panicked| Err(format!("the refresh panicked: {panicked}")));
            self.works.report(&outcome);
            if let Some(found) = self.works.get(id) {
                return Ok(found);
            }
        }
        Err(no_work(id))
    }

    /// [`Self::find`], for code already on the blocking pool.
    ///
    /// **Reads the database on a miss**, so call it only where blocking is
    /// allowed: inside `spawn_blocking`, or a test.
    ///
    /// # Errors
    ///
    /// As [`Self::find`].
    pub fn find_blocking(&self, id: &str) -> Result<Arc<Work>, String> {
        if let Some(found) = self.works.get(id) {
            return Ok(found);
        }
        if self.works.is_live() {
            let outcome = self.works.refresh_if_moved(&self.library);
            self.works.report(&outcome);
            if let Some(found) = self.works.get(id) {
                return Ok(found);
            }
        }
        Err(no_work(id))
    }
```

and, outside the `impl`, beside `skipping_notice`:

```rust
/// How every route addressed by work refuses one this server does not serve.
fn no_work(id: &str) -> String {
    format!("no work `{id}`")
}
```

3. `lib.rs`, `serve()`: change the parameter to `works: Works`.
   - In the `refresh_stump` branch, `forget_all_works(&works);` becomes:

     ```rust
     let held = works.snapshot();
     forget_all_works(held.iter().map(|work| &**work));
     ```

   - In the Stump resolution's `spawn_blocking` closure, replace the `resolve_all_works(transport.as_ref(), &resolving.works, …)` call's second argument:

     ```rust
     // The works present at startup, and only those. Stump survives only as
     // a migration path (the store spec's §10), so a work a later scan adds
     // is served from disk.
     let held = resolving.works.snapshot();
     resolve_all_works(
         transport.as_ref(),
         &held[..],
     ```

   - Make `resolve_all_works` generic: `fn resolve_all_works<W: std::borrow::Borrow<Work>>(transport: &dyn stump::http::Transport, works: &[W], base: &str, learned: &mut dyn FnMut(stump::cache::Resolutions))`, with `let work: &Work = std::borrow::Borrow::borrow(work);` as the loop body's first line (the fully qualified call, because `work.borrow()` needs the trait in scope and can be ambiguous between `Borrow<W>` and `Borrow<Work>`). `tests.rs`'s two calls, which pass `&[first, second]` of `Work`, compile unchanged.

4. `routes.rs`: replace the private helper `work` and its doc with:

```rust
/// The work a route names, or the refusal to send instead, for a handler on
/// a runtime worker. `ServeState::find` decides the wording and does the
/// refresh a miss needs off the worker. This only wraps it in the
/// `Response`-carrying type `get_sidecar` and `serve_page` want.
async fn work(state: &Arc<ServeState>, id: &str) -> Result<Arc<Work>, NoSuchWork> {
    state.find(id).await.map_err(NoSuchWork)
}
```

In `NoSuchWork`'s doc, `Result<&Work, Response>` becomes `Result<Arc<Work>, Response>`.

5. Every call site. The old names are gone, so `cargo check -p taiyaku-serve --tests` lists them; this table says which lookup each one takes:

| File | Function | Runs on | Becomes |
| --- | --- | --- | --- |
| `routes.rs` | `get_sidecar` | runtime worker | `match work(&state, &id).await` |
| `routes.rs` | `serve_page`, first lookup | runtime worker | `match work(&state, &id).await`; then `locate_member(&found, …)` |
| `routes.rs` | `serve_page`, inside its `spawn_blocking` | blocking pool | `state.find_blocking(&id)?`; `state.page(&found, …)` in both places |
| `routes.rs` | `get_thumb`, inside its `spawn_blocking` | blocking pool | `state.find_blocking(&id)?`; `state.page(&found, …)` |
| `routes.rs` | `get_notes`, `put_notes`, `get_truth`, `put_truth`, `get_progress`, `put_progress`, `patch_alignments` | blocking pool | `state.find_blocking(&id)`; where `found` was passed on as `&Work`, pass `&found` |
| `routes.rs` | `list_works` | blocking pool | `let held = state.works.snapshot();` then `held.iter()` where `state.works.iter()` was |
| `opds.rs` | `works` | runtime worker | `for work in state.works.snapshot().iter()` |
| `opds.rs` | `work_feed` | runtime worker | `match state.find(&id).await` |
| `opds.rs` | `advertised_types`, inside its `spawn_blocking` | blocking pool | `state.find_blocking(&id)?` |
| `opds.rs` | `stream_page`, first lookup | runtime worker | `match state.find(&id).await`; `locate_member(&found, …)` |
| `opds.rs` | `stream_page`, inside its `spawn_blocking` | blocking pool | `state.find_blocking(&id)?`; `state.page(&found, …)` |
| `opds.rs` | `cover`, inside its `spawn_blocking` | blocking pool | `state.find_blocking(&id)?` |
| `download.rs` | `prepare`, `prepare_member` (each only ever called inside `spawn_blocking`) | blocking pool | `state.find_blocking(slug).map_err(crate::routes::not_found)?` |

If the compiler names a site this table does not, stop and report it with its function name and whether it runs inside `spawn_blocking`.

6. Delete `finished_scan_json`'s `note` here? **No**: that is Task 5. Leave `config_api.rs` alone in this task.

7. `crates/taiyaku-cli/src/main.rs`:
   - `serve_works` returns `Result<(taiyaku_library::Library, taiyaku_serve::Works), String>`.
   - Its no-paths branch: `let works = taiyaku_serve::Works::library(&db_library)?;`.
   - Its last line: `Ok((db_library, taiyaku_serve::Works::fixed(works)))`.
   - Its doc comment's `# Errors` gains: "…or, with no paths, if any work in the library will not project (see `Works::library`)".
   - In the `serve` command, `for work in &works {` becomes `for work in works.snapshot().iter() {`.

8. `crates/taiyaku-cli/src/tests.rs`: in each of `serve_works_imports_the_given_path_and_the_work_carries_the_librarys_slug`, `serve_works_serves_one_work_when_the_same_path_is_given_twice`, `serve_works_skips_a_work_it_cannot_import_and_serves_the_rest`, `serve_works_serves_two_sidecars_both_named_taiyaku_json` and both calls in `serve_works_survives_a_hub_that_gained_a_member`, add `let works = works.snapshot();` on the line after the `let (…, works) = serve_works(…)…;` statement. `serve_works_refuses_when_no_work_could_be_imported` needs nothing (`Works` derives `Debug`, which its `expect_err` needs).

- [ ] **Step 4: Run the tests to verify they pass**

```bash
cargo test -p taiyaku-serve registry:: -- --test-threads=8
cargo test -p taiyaku-library -p taiyaku-serve -p taiyaku-cli -- --test-threads=8
```

Expected: PASS. If an existing route test now fails, it is a real change in behaviour (every existing fixture is fixed). Stop and report it rather than editing the test.

- [ ] **Step 5: Watch each new test fail against its own defect**

1. In `ServeState::find`, delete the `if self.works.is_live() { … }` block → `a_volume_derived_after_startup_opens_on_the_first_request_for_it` fails (404). The blocking-pool test must still pass. Restore.
2. The same in `find_blocking` → `a_route_that_looks_its_work_up_on_the_blocking_pool_finds_it_too` fails (404), and the first still passes. Restore.
3. Task 2's defect 5 (keep vanished works) → `a_work_deleted_after_startup_stops_answering_once_refreshed` fails (200). Restore.
4. Point `get_sidecar` at `find_blocking` (`state.find_blocking(&id).map_err(NoSuchWork)` without `.await`) and run the first test. **It is expected to pass**: no test here can tell a blocking call on the worker from one off it. Record in the report that this site's choice is held by the table above and by review, not by a test.

- [ ] **Step 6: Whole crates, lint, format, and the leftovers check**

As in Task 1, Step 6, then:

```bash
grep -rn "work_or_missing\|fn work(&self" crates/
```

Expected: nothing in `crates/` (`docs/notes.md`'s historical table may still name `work_or_missing`; leave it).

- [ ] **Step 7: Commit**

```bash
git add -A crates/
git commit -m "Serve works from the registry: find on the worker, find_blocking on the pool"
```

Check `git status --porcelain` shows nothing unexpected before `git add -A`.

---

## Task 4: The refresher

**Files:**
- Modify: `crates/taiyaku-serve/src/registry.rs` (`REFRESH_EVERY`, `spawn_refresher`)
- Modify: `crates/taiyaku-serve/src/lib.rs` (`serve()` spawns it)
- Test: `crates/taiyaku-serve/src/registry/tests.rs` (append), `crates/taiyaku-serve/src/tests.rs` (append)

**Interfaces:**
- Consumes: `ServeState` with `works: Works` (Task 3); `test_support::{router_over_a_live_library, add_a_volume_elsewhere, scanned_file_backed_library}` (Task 3).
- Produces: `pub(crate) const REFRESH_EVERY: Duration`;
  `pub(crate) fn spawn_refresher(state: Arc<crate::ServeState>, every: Duration) -> tokio::task::JoinHandle<()>`.

- [ ] **Step 1: Write the failing tests**

Append to `crates/taiyaku-serve/src/registry/tests.rs`:

```rust
// --- the refresher ---------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_refresher_serves_a_new_volume_before_anything_asks_for_it() {
    let (_router, state, fixture) = crate::test_support::router_over_a_live_library();
    let refresher = super::spawn_refresher(Arc::clone(&state), Duration::from_millis(20));
    let slug = crate::test_support::add_a_volume_elsewhere(&fixture, "Scanned - v02");

    // `get` never refreshes, so only the refresher can make this true.
    let mut served = false;
    for _ in 0..250 {
        if state.works.get(&slug).is_some() {
            served = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    refresher.abort();
    assert!(
        served,
        "the refresher never picked up a volume another process derived"
    );
}
```

Append to `crates/taiyaku-serve/src/tests.rs`, beside `serve_binds_the_address_and_answers_on_it`:

```rust
/// **`serve` keeps a live registry current on its own.** `/api/works` is a
/// listing, so it never takes a lookup's miss path; only the refresher
/// `serve` spawns can make a volume derived after startup appear in it.
///
/// `multi_thread` for the reason the bind test above gives: `ureq` blocks.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn serve_keeps_a_live_library_current_without_a_request_naming_the_work() {
    let fixture = crate::test_support::Fixture::new("serve-live");
    let library = crate::test_support::scanned_file_backed_library(&fixture);
    let works = crate::Works::library(&library).expect("projects the scanned works");
    let probe = std::net::TcpListener::bind("127.0.0.1:0").expect("a free port");
    let addr = probe.local_addr().unwrap().to_string();
    drop(probe);
    let cache = fixture.path().join("cache");

    let serving = tokio::spawn({
        let addr = addr.clone();
        async move {
            crate::serve(
                library,
                works,
                &addr,
                None,
                None,
                false,
                Some(cache),
                crate::session::AuthConfig::default(),
            )
            .await
        }
    });
    let slug = crate::test_support::add_a_volume_elsewhere(&fixture, "Scanned - v02");

    let url = format!("http://{addr}/api/works");
    let wanted = format!("\"{slug}\"");
    let mut listed = false;
    for _ in 0..250 {
        if serving.is_finished() {
            break;
        }
        if let Ok(mut r) = ureq::get(&url)
            .header(
                "authorization",
                crate::test_support::fixture_authorization(),
            )
            .call()
            && r.body_mut().read_to_string().unwrap_or_default().contains(&wanted)
        {
            listed = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    serving.abort();
    assert!(listed, "`/api/works` never listed {slug}");
}
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p taiyaku-serve refresher serve_keeps_a_live -- --test-threads=8`
Expected: the first does not compile (`spawn_refresher` does not exist). Comment it out to see the second fail: "`/api/works` never listed …" after about five seconds. Restore it.

- [ ] **Step 3: Write the refresher**

In `crates/taiyaku-serve/src/registry.rs`, add `use std::time::Duration;` and, after `impl Works`:

```rust
/// How often a live registry asks whether the library has moved. The
/// question is one connection opened and a one-row read, and in two seconds
/// a volume a scan derives is listed even by the routes that never name a
/// work: `/api/works` and the OPDS feed.
pub(crate) const REFRESH_EVERY: Duration = Duration::from_secs(2);

/// Every `every`, ask whether `state`'s library has moved, and read its works
/// again when it has, on the blocking pool, because the question reads the
/// database.
///
/// `serve` spawns this for a live registry only and lets it run for the
/// runtime's life. It answers the task so that a test can stop it.
pub(crate) fn spawn_refresher(
    state: Arc<crate::ServeState>,
    every: Duration,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut ticks = tokio::time::interval(every);
        ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            ticks.tick().await;
            let asking = Arc::clone(&state);
            let outcome =
                tokio::task::spawn_blocking(move || asking.works.refresh_if_moved(&asking.library))
                    .await
                    .unwrap_or_else(|panicked| Err(format!("the refresh panicked: {panicked}")));
            state.works.report(&outcome);
        }
    })
}
```

In `crates/taiyaku-serve/src/lib.rs`'s `serve()`, after the Stump resolution's `if let Some((transport, side, base)) = pending { … }` block and before `axum::serve(`:

```rust
    // A live registry reads the library again whenever its structure moves.
    // A fixed one, `serve` handed sidecar paths, never does.
    if state.works.is_live() {
        registry::spawn_refresher(Arc::clone(&state), registry::REFRESH_EVERY);
    }
```

- [ ] **Step 4: Run to verify they pass**

Run: `cargo test -p taiyaku-serve refresher serve_keeps_a_live -- --test-threads=8`
Expected: PASS.

- [ ] **Step 5: Watch each fail against its own defect**

1. In `spawn_refresher`'s loop, replace the `spawn_blocking(…)` expression with `Ok(Refresh::Unmoved)` (import `Refresh` in scope as needed) → the refresher test fails. Restore.
2. Delete the `if state.works.is_live() { … }` block from `serve()` → `serve_keeps_a_live_library_current_without_a_request_naming_the_work` fails. Restore.

- [ ] **Step 6: Whole crates, lint, format**

As in Task 1, Step 6.

- [ ] **Step 7: Commit**

```bash
git add crates/taiyaku-serve/src/registry.rs crates/taiyaku-serve/src/registry/tests.rs crates/taiyaku-serve/src/lib.rs crates/taiyaku-serve/src/tests.rs
git commit -m "Refresh a live registry every two seconds, from serve"
```

---

## Task 5: The scan report stops calling `/api/works` a startup snapshot

**Files:**
- Modify: `crates/taiyaku-serve/src/config_api.rs` (`finished_scan_json` and its doc)
- Modify: `web/src/config.ts` (`FinishedScan`), `web/src/ScanReport.tsx` (`ScanReport`), `web/src/RootRow.tsx` (the `removalDerive` comment), `web/src/styles/admin.css` (`.scan-report .note`)
- Modify: `web/src/ScanReport.test.tsx`, `web/src/Config.browser.test.tsx`, `web/src/RootRow.test.tsx`

**Interfaces:**
- Produces: a finished scan's JSON is `{ "scan": …, "derive": … }`, with no `note`.

- [ ] **Step 1: The server**

In `config_api.rs`, delete from `finished_scan_json`'s doc comment the paragraph beginning "**`note` — the one thing this body says about what it is not.**" through "…this same response just wrote.", and delete the `"note": …` entry (its three continuation lines included) from the `json!`. No server test asserts the field (checked at `8f8cdba2`: `grep -rn '"note"' crates/taiyaku-serve/src/config_api/tests.rs` finds only rule notes).

- [ ] **Step 2: The web**

- `web/src/config.ts`: delete the two lines `/** The server's own note about \`/api/works\` being a startup snapshot. */` and `note: string` from `FinishedScan`.
- `web/src/ScanReport.tsx`: delete `<p className="note">{outcome.note}</p>` from `ScanReport`.
- `web/src/styles/admin.css`: delete the `.scan-report .note { … }` rule. Nothing else draws that class.
- `web/src/RootRow.tsx`: in the comment above `removalDerive`, "would need a cast asserting a `scan` and a `note` that do not exist" becomes "would need a cast asserting a `scan` that does not exist".
- `web/src/ScanReport.test.tsx`: delete the `note: …` line from the `finished` helper, and delete the whole `it('draws the note about /api/works being a startup snapshot', …)` case. The behaviour it pinned is the one this part removes.
- `web/src/Config.browser.test.tsx` and `web/src/RootRow.test.tsx`: delete each fixture's `note: …` line.

- [ ] **Step 3: Check nothing else reads it**

```bash
grep -rn "startup snapshot" crates web/src
grep -rn "outcome\.note\|\.scan-report \.note" web/src
```

Expected: no output.

- [ ] **Step 4: Run and gate**

```bash
pnpm --dir web typecheck
pnpm --dir web lint
pnpm --dir web exec vitest run --project node --project jsdom --reporter=default
pnpm --dir web exec vitest run --project browser --reporter=default
cargo test -p taiyaku-serve -- --test-threads=8
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

Expected: all pass, and the `jsdom` count is one lower than on `main` (one case deleted). Write down the three web counts for Task 7, read off these runs.

- [ ] **Step 5: Commit**

```bash
git add crates/taiyaku-serve/src/config_api.rs web/src/config.ts web/src/ScanReport.tsx web/src/RootRow.tsx web/src/styles/admin.css web/src/ScanReport.test.tsx web/src/Config.browser.test.tsx web/src/RootRow.test.tsx
git commit -m "The scan report stops saying /api/works is a startup snapshot"
```

---

## Task 6: On a copy of the real library

**Files:**
- Create: `.superpowers/sdd/live-works-evidence/drive.log` (gitignored)

This proves the part's claim end to end on real data: a volume added by another process opens through the running server's reader route with no restart. Nothing in it is committed.

- [ ] **Step 1: Set up, and record the original's digest**

Set `SCRATCH` to your session's scratchpad directory. Then:

```bash
S="$SCRATCH/live-works-drive"
E=.superpowers/sdd/live-works-evidence
mkdir -p "$S" "$E"
sha256sum /home/davey/temp/manga-test/library.db >| "$E/original-sha-before.txt"
cat "$E/original-sha-before.txt"
cp /home/davey/temp/manga-test/library.db "$S/library.db"
touch "$S/marker"
ss -ltn 'sport = :18485'
cargo build --release -p taiyaku-cli
printf 'drive-pw\ndrive-pw\n' | target/release/taiyaku library user password local --db "$S/library.db"
```

Expected: the digest starts `d5ebc0c237865035`. `ss` prints only its header (the port is free). The release binary is built from this branch's head; `cargo test` does not rebuild it.

- [ ] **Step 2: Start the server on the copy**

With the Bash tool's `run_in_background`:

```bash
target/release/taiyaku serve --db "$S/library.db" --cache "$S/cache" --addr 127.0.0.1:18485 >| "$E/serve.log" 2>&1
```

Then wait until `grep -q 'listening' "$E/serve.log"` succeeds, and record the starting state:

```bash
curl -s -u local:drive-pw http://127.0.0.1:18485/api/works | jq length | tee -a "$E/drive.log"
sqlite3 "$S/library.db" "SELECT n FROM structure_version" | tee -a "$E/drive.log"
```

Expected: `1896` works.

- [ ] **Step 3: Add a volume from another process**

```bash
mkdir -p "$S/root/Drive Test/Drive Test - v01"
for p in 000 001; do printf 'not really a jpeg: drive %s\n' "$p" >| "$S/root/Drive Test/Drive Test - v01/$p.jpg"; done
R=$(realpath "$S/root")
target/release/taiyaku library scan --add "$R" --root "$R" --db "$S/library.db" 2>&1 | tee -a "$E/drive.log"
SLUG=$(sqlite3 "$S/library.db" "SELECT w.slug FROM work w JOIN edition e ON e.work_id = w.id JOIN member m ON m.edition_id = e.id WHERE m.path LIKE '%Drive Test - v01%'")
echo "slug: $SLUG" | tee -a "$E/drive.log"
curl -s -o "$E/new-work.json" -w '%{http_code} %{time_total}s\n' -u local:drive-pw "http://127.0.0.1:18485/api/works/$SLUG" | tee -a "$E/drive.log"
```

`--root "$R"` keeps the scan to the scratch root: without it, `library scan` walks every root, including `/home/davey/temp/manga-test/Volumes`, and its watch probe writes into that tree. Expected: the scan's report line names only `$R`; the request answers `200`; `new-work.json` holds an `"editions"` key.

- [ ] **Step 4: The listing catches up without a request naming the work**

```bash
sleep 3
curl -s -u local:drive-pw http://127.0.0.1:18485/api/works | jq length | tee -a "$E/drive.log"
sqlite3 "$S/library.db" "SELECT n FROM structure_version" | tee -a "$E/drive.log"
```

Expected: `1897`, and a counter higher than in Step 2.

- [ ] **Step 5: Stop the server by PID and prove isolation**

```bash
ss -lptn 'sport = :18485'
kill <the pid ss printed>
sha256sum /home/davey/temp/manga-test/library.db | tee -a "$E/drive.log"
find /home/davey/temp/manga-test -newer "$S/marker" | tee -a "$E/drive.log" | wc -l
cat "$E/serve.log" | tail -20
```

Kill by PID, never by pattern: this machine runs other projects' servers, and `pkill -f` matches its own wrapper shell. Expected: the digest is unchanged, `find` prints `0` lines, and `serve.log` holds no "could not refresh".

- [ ] **Step 6: Report**

Report the figures (1896 → 1897, the HTTP status and time of the first request for the new slug, the counter before and after, the digest before and after, the `find` count) for Task 7's notes entry. If any expectation failed, stop and report it; do not continue to Task 7.

---

## Task 7: Mutation sweep, `docs/status.md`, the gate, and the claim check

**Files:**
- Modify: `docs/status.md`, `docs/notes.md`, `README.md`, `docs/mutants-baseline.md`

- [ ] **Step 1: Mutation sweep on what this branch touched**

Commit everything first: `--since main` reads `git diff main...HEAD`, not the worktree.

```bash
git status --porcelain
NEXTEST_TEST_THREADS=8 RUST_TEST_THREADS=8 scripts/mutants.sh --since main >| .superpowers/sdd/live-works-evidence/mutants.log 2>&1; echo "exit $?"
tail -30 .superpowers/sdd/live-works-evidence/mutants.log
```

For every survivor in `registry.rs`, `structure.rs`, `works.rs`'s `reproject`/`from_library`, `lib.rs`'s `find`/`find_blocking`/`no_work`/`serve`, or the call sites Task 3 changed:

1. Hand-apply it in place.
2. Run all three crates' tests. A mutant only another package's tests catch reads MISSED here (O87).
3. Then either kill it with a test (watched failing first, committed) or record it in `docs/mutants-baseline.md` with its reason in that file's own terms.

`Works::report`'s body replaced by `()` is expected to survive: it only prints, and the decision it prints is `notice`'s, which is tested. Record it.

Read the unviable count too (O88): a function whose only generated mutant does not compile has no mutation coverage, and the run reports that as `unviable`, not as a gap. Migration 14's SQL is a string literal, which cargo-mutants never mutates. Task 1's hand-applied defects are that file's only mutation evidence; say so in the log row.

Add a row to the baseline's run log ("When it was last run") with the date, `live-works, --since main (N mutants, M m)`, and the result. Commit as `Mutation sweep of the live-works branch`.

- [ ] **Step 2: Sweep the counts on the way in**

```bash
grep -c '^| O[0-9]' docs/status.md                 # expected 182
grep -c '^| O[0-9]* | \[x\]' docs/status.md        # expected 119
grep -nE '[0-9]{4} tests|standing checks gated' docs/status.md
grep -n -A1 '^What remains:' docs/status.md
```

If a figure differs, stop and report: the document disagreed with itself before this branch touched it.

- [ ] **Step 3: Open and close O183**

Add after O182's row (six `|` per row; escape any `|` inside backticks as `\|`):

```
| O183 | [x] | **Closed <date>, the day it was opened, on the live-works branch (Part A of `docs/superpowers/specs/2026-10-04-day-to-day-deployment-design.md`).** **The reader served the works the server started with.** `ServeState.works` was a `Vec<Work>` built once at startup, while every `/api/library/*` route read the database. So a volume a scan had just derived was listed by the library and answered "no work" by the reader, OPDS and `/api/works` until the process restarted. A rule save, an accepted proposal, a root removal or a ComicInfo write-back left the reader on the old structure the same way, and `finished_scan_json` said so in a `note` on every scan report. Now migration 14 keeps a `structure_version` counter that triggers bump on every write to the six tables a `Work` is built from. A live registry (`registry::Works`) reads every work again when the counter has moved: every two seconds, and before answering a slug it does not hold. It keeps each surviving `Work` and swaps its document under that work's own lock. On a copy of the real library, a volume added by another process answered <status> on the first request for it, and `/api/works` went from <n> to <n+1> without a restart (`docs/notes.md`, "Live works (Part A, <date>)"). | `registry::Works`, `ServeState::find`, `ServeState::find_blocking`, `taiyaku_library::structure`, `schema/014_structure_version.sql` | the day-to-day deployment spec's §2, 2026-10-04 |
```

Fill `<date>`, `<status>`, `<n>` from Task 6's report. Do not invent a figure.

- [ ] **Step 4: The counts**

- The head: "One hundred and eighty-two, each found by…" → "One hundred and eighty-three, …".
- "**One hundred and nineteen are now closed — O1, … and O182 — leaving sixty-three.**" → "One hundred and twenty", with O183 added after O182 ("…, O182 and O183"). "Leaving sixty-three" stays: O183 opened and closed here.
- The parenthetical that ends "…O182 closed the same day on a branch of its own, and O138 reopened there by the user's decision, which left it at sixty-three.)*" gains, before `)*`: " O183 opened and closed on the live-works branch, as O105 and O106 did, so it moved the closed count and not the open one."
- "What remains: the sixty-three open\nitems": unchanged. Grep the opener and read the next line to confirm.
- The Rust test count, at both sites ("Six crates, N tests" and "…of the N are standing checks"): run the gate (Step 7) once; `status-test-count` fails naming the real figure. Write that figure at both sites, never a sum.
- The web counts in the same sentence: the three figures Task 5 read off its runs (`node`, `jsdom`, browser) and the node+jsdom sum.
- O122's row (open, unchanged otherwise): add one sentence at the end of its What column: "*Since O183, a refresh also holds each work's write lock while it reads that work back from the library, so a reader on the async worker can wait on it too. Unmeasured, like the correction's.*"

- [ ] **Step 5: `docs/notes.md`, and one line in the README**

Append a section `## Live works (Part A, <date>)` to `docs/notes.md` covering:
- what changed: the counter, the registry, `find`/`find_blocking`, the refresher;
- why the counter is read before the projection, and why each read is `BEGIN DEFERRED`;
- the race test's design;
- Task 6's figures, exactly as reported;
- the mutation sweep's result;
- what was **not** measured: the NAS; the refresh's cost during a long derive's churn, when every committed batch moves the counter; a library larger than the 1,896-work copy.

In `README.md`, after the comment block under `taiyaku serve --addr 127.0.0.1:8484` that ends "…until `taiyaku library export` writes one out.", add:

```
#
# a work a later scan derives is served without a restart: the server reads
# the library again within two seconds of its structure changing, and at once
# when asked for a work it does not hold yet
```

- [ ] **Step 6: Sweep again, and grep the bare number**

Re-run Step 2's commands. Expected: 183 rows, 120 closed, every count sentence agreeing. Then:

```bash
grep -rn -E '\bO183\b' docs web/src crates | grep -v '^docs/notes.md' | cut -c1-160
grep -nE '^\| O183 ' docs/status.md | tr -cd '|' | wc -c
```

Expected: the second prints `6`.

- [ ] **Step 7: The gate, and the corpus checks**

```bash
env -u TAIYAKU_CORPUS NEXTEST_TEST_THREADS=8 RUST_TEST_THREADS=8 scripts/check-all.sh >| .superpowers/sdd/live-works-evidence/gate.log 2>&1; echo "exit $?"
tail -5 .superpowers/sdd/live-works-evidence/gate.log
TAIYAKU_CORPUS="$PWD/corpus" cargo test --workspace --all-features corpus:: -- --nocapture --test-threads=1 >| .superpowers/sdd/live-works-evidence/corpus.log 2>&1; echo "exit $?"
```

Expected: `exit 0` for both, and the gate log's `all green` line, quoted in the report. Do not pipe the gate through `tail` before reading its exit status: a pipe masks it. The corpus job runs because `crates/` changed (`corpus.rs` builds a `ServeState`).

- [ ] **Step 8: Commit**

```bash
git add docs/status.md docs/notes.md README.md
git commit -m "Status: O183 opened and closed, the reader serves the library as it stands"
```

- [ ] **Step 9: The claim check, before anyone says "ready"**

Draft the PR body at `.superpowers/sdd/live-works-pr-body.md`, not hard-wrapped, and include it in the **first** pass. Read every factual sentence the branch added and check each against its source (the code, a test's output, Task 6's log, the mutation log):

- the PR body;
- O183's row and the O122 sentence;
- the notes section;
- the README line;
- every new or changed code comment: `git diff main -- crates web/src | grep '^+.*\(//\|\*\)'`.

**Prefer deleting a figure to correcting it.** A corrected *reason* is a new claim, so check it again. Fix what is wrong in a new commit, check that commit the same way, and repeat until a pass finds nothing. Report each pass's count.
