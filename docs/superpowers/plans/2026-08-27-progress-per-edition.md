# A reading position per edition, and O73's fold — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Widen `progress`'s key so a reader holds a place in every edition of a volume, then make a scan move an emptied work's reader rows onto the work its material went to and delete it — closing O73.

**Architecture:** A table rebuild in migration 009, a new `Position` read type beside the unchanged `Progress` write type, a versioned `taiyaku.progress.json` that still reads the legacy bare object, and a supersession pass at the end of `derive` that matches editions by member digest, moves `progress`/`note`/`truth_*`, and then issues the first `DELETE FROM work` in production code.

**Tech Stack:** Rust (`taiyaku-library`, `taiyaku-serve`), `rusqlite`/SQLite, TypeScript + React (`web/`), `cargo nextest`, `vitest`.

**Design:** [`../specs/2026-08-27-progress-per-edition-design.md`](../specs/2026-08-27-progress-per-edition-design.md). **Evidence:** `docs/notes.md`, *"O73's cascade, measured — 2026-08-27"*.

## Global Constraints

- **Migrations are append-only.** Never edit `001`–`008`. `SCHEMA_VERSION` and `MIGRATIONS.len()` are one fact written twice and a `const _: () = assert!` binds them — both change together or it is a compile error.
- **Sites are named by symbol, never by line number**, in every comment and doc you write.
- **`docs/status.md` owns the open-items state.** Closing O73 is the five-part edit in Task 9, not a tick.
- **No attribution lines in commit messages.** No `Co-Authored-By`, no `Generated with`.
- **The gate is `env -u TAIYAKU_CORPUS scripts/check-all.sh`**, and it must print `all green`. Do not run it with `TAIYAKU_CORPUS` set: that exports into an unfiltered `cargo test --workspace` and runs `geometry_experiment` in debug (40+ minutes).
- **Corpus checks run separately:** `TAIYAKU_CORPUS="$PWD/corpus" cargo test --workspace --all-features corpus:: -- --nocapture --test-threads=1`.
- **A shell variable feeding a recursive delete is written `"${VAR:?}"`**, never `"$VAR"`.
- Test-count figures in `docs/status.md` appear **twice** and are checked by `scripts/status-test-count.mjs` against the JUnit report. Task 9 updates them from the run, not from arithmetic.

---

### Task 1: Migration 009 widens `progress`'s key

**Files:**
- Create: `crates/taiyaku-library/src/schema/009_progress_per_edition.sql`
- Modify: `crates/taiyaku-library/src/schema.rs` (`SCHEMA_VERSION`, `MIGRATIONS`)
- Test: `crates/taiyaku-library/src/schema/tests.rs`

**Interfaces:**
- Consumes: nothing.
- Produces: a `progress` table keyed `(user_id, work_id, edition)`; `SCHEMA_VERSION == 9`.

- [ ] **Step 1: Write the failing test**

In `crates/taiyaku-library/src/schema/tests.rs`, beside the existing migration tests:

```rust
/// Two editions of one work, both with a position, both surviving 009.
///
/// The narrow key made this shape unreachable before the migration, so the
/// rows are inserted after it has run — what is under test is that the key
/// admits them at all, and that a database carrying a pre-009 row still holds
/// it afterwards.
#[test]
fn migration_9_lets_one_work_hold_a_position_per_edition() {
    let conn = Connection::open_in_memory().expect("in-memory database");
    crate::schema::migrate(&conn).expect("migrating");

    conn.execute("INSERT INTO user (id, name) VALUES (1, 'reader')", [])
        .expect("user");
    conn.execute(
        "INSERT INTO work (id, content_key, slug, title, reading, hub)
         VALUES (1, 'k', 's', 't', 'rtl', 'raw')",
        [],
    )
    .expect("work");

    for edition in ["raw", "full-color"] {
        conn.execute(
            "INSERT INTO progress (user_id, work_id, edition, slot) VALUES (1, 1, ?1, 7)",
            [edition],
        )
        .expect("a position per edition");
    }

    let held: i64 = conn
        .query_row("SELECT count(*) FROM progress WHERE work_id = 1", [], |r| {
            r.get(0)
        })
        .expect("counting");
    assert_eq!(held, 2, "both editions' positions must stand");
}
```

- [ ] **Step 2: Run it and watch it fail**

Run: `cargo nextest run -p taiyaku-library migration_9_lets_one_work_hold_a_position_per_edition`
Expected: FAIL — the second insert reports `UNIQUE constraint failed: progress.user_id, progress.work_id`.

- [ ] **Step 3: Write the migration**

Create `crates/taiyaku-library/src/schema/009_progress_per_edition.sql`:

```sql
-- **A reading position per edition, not per work.** O73's fold — accepting an
-- `edition-of` proposal and rescanning — moves a volume's material onto
-- another work, and the reader rows hanging off the work it left have to move
-- with it. `UPDATE progress SET work_id = <target>` is refused by the old key
-- whenever the reader held a position on both works, which is the ordinary
-- case rather than the corner: measured 2026-08-27, `docs/notes.md`.
--
-- **What the widening also fixes, which is older than O73.** `put_progress`
-- upserts on `(user_id, work_id)` naming `edition = excluded.edition`, so a
-- reader at slot 50 of one edition who opened a sibling edition and turned one
-- page lost the first position, silently. After this they do not. A test that
-- pinned the old behaviour is pinning a defect.
--
-- **Nothing is dropped or merged by the copy.** Every row already satisfies
-- the wider key, the narrow one being a prefix of it.
--
-- `completed_at` and `read_count` come across and are now per edition, which
-- is what 001 always described them as: "finished once and now re-reading" is
-- a fact about a rendition, not about a volume.
--
-- **Nothing REFERENCES `progress`** — `grep -rn 'REFERENCES progress' schema/`
-- is empty — so unlike migration 4's rebuild of `user` there is no child to
-- lose. `without_foreign_keys` still wraps this and `pragma_foreign_key_check`
-- still runs before the version row, because the rebuild drops `progress`'s
-- *own* foreign keys onto `user` and `work` and re-declares them.
--
-- **There is no index to restore.** 005's note about `DROP TABLE` taking a
-- table's indexes with it is why that is said out loud rather than left to be
-- noticed: `progress` has none, its primary key being the only access path.
CREATE TABLE progress_new (
  user_id      INTEGER NOT NULL REFERENCES user(id) ON DELETE CASCADE,
  work_id      INTEGER NOT NULL REFERENCES work(id) ON DELETE CASCADE,
  edition      TEXT NOT NULL,
  slot         INTEGER NOT NULL,
  updated      TEXT NOT NULL DEFAULT (datetime('now')),
  completed_at TEXT,
  read_count   INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (user_id, work_id, edition)
) WITHOUT ROWID;

INSERT INTO progress_new (user_id, work_id, edition, slot, updated, completed_at, read_count)
SELECT user_id, work_id, edition, slot, updated, completed_at, read_count FROM progress;

DROP TABLE progress;
ALTER TABLE progress_new RENAME TO progress;
```

- [ ] **Step 4: Register it**

In `crates/taiyaku-library/src/schema.rs`, change the version and the array. Both, or the `const _: () = assert!(MIGRATIONS.len() == SCHEMA_VERSION as usize)` below them fails to compile — which is the point of it.

```rust
pub const SCHEMA_VERSION: u32 = 9;
```

```rust
const MIGRATIONS: [&str; 9] = [
    include_str!("schema/001_initial.sql"),
    include_str!("schema/002_absorbed.sql"),
    include_str!("schema/003_scanner.sql"),
    include_str!("schema/004_auth.sql"),
    include_str!("schema/005_note_id_per_user.sql"),
    include_str!("schema/006_member_root.sql"),
    include_str!("schema/007_backfill_member_root.sql"),
    include_str!("schema/008_trigram_fts.sql"),
    include_str!("schema/009_progress_per_edition.sql"),
];
```

- [ ] **Step 5: Verify the claim the migration's comment makes**

Run: `grep -rn 'REFERENCES progress' crates/taiyaku-library/src/schema/`
Expected: no output. If there is output, the comment is wrong — fix the comment and reconsider the rebuild before continuing.

- [ ] **Step 6: Run the test and the rest of the schema suite**

Run: `cargo nextest run -p taiyaku-library schema::`
Expected: PASS, including `migration_9_lets_one_work_hold_a_position_per_edition` and every existing migration test.

- [ ] **Step 7: Commit**

```bash
git add crates/taiyaku-library/src/schema/009_progress_per_edition.sql crates/taiyaku-library/src/schema.rs crates/taiyaku-library/src/schema/tests.rs
git commit -m "Migration 009: a reading position per edition"
```

---

### Task 2: `Position`, and `state::progress` returns a set

**Files:**
- Modify: `crates/taiyaku-library/src/lib.rs` (add `Position` beside `Progress`)
- Modify: `crates/taiyaku-library/src/state.rs` (`progress`, `put_progress`)
- Test: `crates/taiyaku-library/src/state/tests.rs`

**Interfaces:**
- Consumes: Task 1's schema.
- Produces:
  - `pub struct Position { pub edition: String, pub slot: u32, pub updated: String }`
  - `state::progress(conn: &Connection, work_id: i64, user_id: i64) -> Result<Vec<Position>, String>` — newest `updated` first.
  - `state::put_progress(conn, work_id, user_id, &Progress) -> Result<(), String>` — unchanged signature, wider conflict target.

- [ ] **Step 1: Write the failing tests**

In `crates/taiyaku-library/src/state/tests.rs`:

```rust
/// A write to one edition leaves a sibling edition's position standing.
///
/// **Red before migration 009 and green after**, which is the behaviour change
/// stated in the design: before it, `put_progress` upserted on
/// `(user_id, work_id)` and rewrote `edition`, so opening a sibling edition
/// destroyed the place held in the first.
#[test]
fn a_write_to_one_edition_leaves_another_editions_position_standing() {
    let (conn, work_id, user_id) = one_work_and_reader();

    put_progress(
        &conn,
        work_id,
        user_id,
        &Progress { edition: "raw".into(), slot: 50 },
    )
    .expect("first edition");
    put_progress(
        &conn,
        work_id,
        user_id,
        &Progress { edition: "full-color".into(), slot: 3 },
    )
    .expect("second edition");

    let held = progress(&conn, work_id, user_id).expect("reading back");
    let mut pairs: Vec<(String, u32)> =
        held.into_iter().map(|p| (p.edition, p.slot)).collect();
    pairs.sort();
    assert_eq!(
        pairs,
        vec![("full-color".to_string(), 3), ("raw".to_string(), 50)],
        "both positions must stand"
    );
}

/// Newest first, so a caller that wants "where was I" takes the head.
#[test]
fn positions_come_back_newest_first() {
    let (conn, work_id, user_id) = one_work_and_reader();

    put_progress(&conn, work_id, user_id, &Progress { edition: "raw".into(), slot: 1 })
        .expect("older");
    // `updated` defaults to `datetime('now')`, whose resolution is one second,
    // so the two rows are separated by hand rather than by waiting.
    conn.execute(
        "UPDATE progress SET updated = '2020-01-01 00:00:00' WHERE edition = 'raw'",
        [],
    )
    .expect("ageing the first row");
    put_progress(
        &conn,
        work_id,
        user_id,
        &Progress { edition: "full-color".into(), slot: 2 },
    )
    .expect("newer");

    let held = progress(&conn, work_id, user_id).expect("reading back");
    assert_eq!(
        held.iter().map(|p| p.edition.as_str()).collect::<Vec<_>>(),
        vec!["full-color", "raw"],
        "the most recently updated position comes first"
    );
}
```

Add the helper beside them if `state/tests.rs` has no equivalent already — check first and reuse rather than duplicate:

```rust
fn one_work_and_reader() -> (Connection, i64, i64) {
    let conn = Connection::open_in_memory().expect("in-memory database");
    crate::schema::migrate(&conn).expect("migrating");
    conn.execute("INSERT INTO user (id, name) VALUES (1, 'reader')", [])
        .expect("user");
    conn.execute(
        "INSERT INTO work (id, content_key, slug, title, reading, hub)
         VALUES (1, 'k', 's', 't', 'rtl', 'raw')",
        [],
    )
    .expect("work");
    (conn, 1, 1)
}
```

- [ ] **Step 2: Run them and watch them fail**

Run: `cargo nextest run -p taiyaku-library state::tests::a_write_to_one_edition state::tests::positions_come_back_newest_first`
Expected: FAIL to compile — `progress` returns `Option<Progress>`, so `.into_iter().map(|p| (p.edition, p.slot))` does not typecheck, and `Position` does not exist.

- [ ] **Step 3: Add `Position`**

In `crates/taiyaku-library/src/lib.rs`, immediately after `Progress`:

```rust
/// One recorded position, as a reader of the store sees it.
///
/// **Separate from [`Progress`], which is the write shape.** `Progress` is the
/// body of `PUT /api/works/{work}/progress` and what `migrate::apply_progress`
/// deserialises out of a sibling document; `updated` is the store's to set, so
/// putting it on `Progress` would ask a client to invent a timestamp the
/// server then ignores. The two are one field apart and that field is the
/// whole distinction.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Position {
    pub edition: String,
    pub slot: u32,
    pub updated: String,
}
```

- [ ] **Step 4: Widen the store**

In `crates/taiyaku-library/src/state.rs`, replace `progress` and the conflict target of `put_progress`. Keep `put_progress`'s existing doc comment about `ON CONFLICT … DO UPDATE` naming its columns rather than becoming a `REPLACE` — that reasoning is unchanged and its test still guards it.

```rust
/// Every position one reader holds on one work, newest first.
///
/// **A vector rather than an `Option` since migration 009**: a reader holds a
/// place in each edition of a volume. The head is the most recently updated,
/// which is what a caller wanting "where was I" should take — see
/// `serve`'s `get_progress` and `web/src/api.ts`'s `getProgress`.
///
/// # Errors
///
/// Returns an error if SQLite fails.
pub fn progress(
    conn: &Connection,
    work_id: i64,
    user_id: i64,
) -> Result<Vec<Position>, String> {
    let mut statement = conn
        .prepare(
            "SELECT edition, slot, updated FROM progress
              WHERE work_id = ?1 AND user_id = ?2
              ORDER BY updated DESC, edition ASC",
        )
        .map_err(|e| format!("preparing the progress read: {e}"))?;
    let rows = statement
        .query_map(params![work_id, user_id], |row| {
            Ok(Position {
                edition: row.get(0)?,
                slot: row.get(1)?,
                updated: row.get(2)?,
            })
        })
        .map_err(|e| format!("reading progress for work {work_id}, user {user_id}: {e}"))?;
    rows.collect::<rusqlite::Result<Vec<Position>>>()
        .map_err(|e| format!("reading progress for work {work_id}, user {user_id}: {e}"))
}
```

`ORDER BY updated DESC, **edition ASC**`: `updated` has one-second resolution, so two positions written in the same second would otherwise come back in an order SQLite is free to vary — a test that passed on insertion order and failed on a rebuild. The tiebreak makes the order total.

In `put_progress`, the statement becomes:

```rust
        "INSERT INTO progress (user_id, work_id, edition, slot, updated)
         VALUES (?1, ?2, ?3, ?4, datetime('now'))
         ON CONFLICT(user_id, work_id, edition) DO UPDATE SET
           slot = excluded.slot,
           updated = excluded.updated",
```

`edition` leaves the `DO UPDATE` list: it is part of the conflict target now, so assigning it would be assigning a column to itself.

- [ ] **Step 5: Run the state suite**

Run: `cargo nextest run -p taiyaku-library state::`
Expected: PASS, including `put_progress_leaves_completed_at_and_read_count_alone`. If that one fails, the `DO UPDATE` list has grown — it must still name only `slot` and `updated`.

- [ ] **Step 6: Confirm the guard still guards**

Temporarily change the statement to `INSERT OR REPLACE INTO progress (user_id, work_id, edition, slot, updated) VALUES (?1, ?2, ?3, ?4, datetime('now'))` and run:

Run: `cargo nextest run -p taiyaku-library put_progress_leaves_completed_at_and_read_count_alone`
Expected: FAIL. Then revert the change and re-run: PASS.

This is the same check the design asks for and it costs one edit: a test whose subject is something *not* happening passes both when the property holds and when the test cannot see it.

- [ ] **Step 7: Commit**

```bash
git add crates/taiyaku-library/src/lib.rs crates/taiyaku-library/src/state.rs crates/taiyaku-library/src/state/tests.rs
git commit -m "A position per edition in the store, and Position beside Progress"
```

---

### Task 3: The three callers of `state::progress`

**Files:**
- Modify: `crates/taiyaku-serve/src/progress.rs` (`load`)
- Modify: `crates/taiyaku-library/src/migrate.rs` (`migrate_progress`)
- Modify: `crates/taiyaku-library/src/export.rs` (`export_one`)
- Test: `crates/taiyaku-serve/src/progress/tests.rs`, `crates/taiyaku-library/src/migrate/tests.rs`

**Interfaces:**
- Consumes: `state::progress -> Vec<Position>` from Task 2.
- Produces: `serve::progress::load(&Library, &Work, i64) -> Result<Vec<Position>, String>` — empty vector where it used to answer `None`.

This task is the compile-error sweep. It carries no new behaviour and exists so the next two tasks start from a green tree.

- [ ] **Step 1: See the whole blast radius**

Run: `cargo check --workspace 2>&1 | grep -E '^error' | head -20`
Expected: errors in exactly `taiyaku-serve/src/progress.rs`, `taiyaku-library/src/migrate.rs` and `taiyaku-library/src/export.rs`, plus their tests. If a fourth production file appears, stop and re-read it before changing anything — the design named three and a fourth means the design is wrong.

- [ ] **Step 2: `serve::progress::load`**

```rust
/// Every position this reader holds on this work, newest first.
///
/// An empty vector where nothing was ever recorded. The distinction the old
/// `Option` carried is intact: "never opened" is an empty set and "on the
/// first page" is a set holding slot 0, and conflating them would reopen a
/// work at its cover forever.
///
/// A work with no row in `library`'s `work` table answers empty without
/// touching `progress` at all: `works::find_row` never creates a row.
///
/// # Errors
///
/// Returns an error if SQLite fails.
pub fn load(library: &Library, work: &Work, user_id: i64) -> Result<Vec<Position>, String> {
    let conn = library.conn();
    let Some(work_id) = works::find_row(&conn, work)? else {
        return Ok(Vec::new());
    };
    taiyaku_library::state::progress(&conn, work_id, user_id)
}
```

and change the import at the head of the file to `use taiyaku_library::{Library, Position, Progress};`.

- [ ] **Step 3: `migrate_progress`**

One line, inside the `step(…)` closure:

```rust
        Ok(!state::progress(conn, work_id, user_id)?.is_empty())
```

The question it answers is unchanged — "does this work already hold a position for this reader" — and `is_empty` is that question against a set.

- [ ] **Step 4: `export_one`**

Leave the document shape alone in this task; Task 4 owns it. Here, only keep it compiling by taking the head:

```rust
    let progress = crate::state::progress(conn, work_id, user_id)?;
```

and, at the serialisation below it:

```rust
    let progress_json = progress
        .first()
        .map(|p| {
            serde_json::to_string_pretty(p)
                .map_err(|e| format!("serialising progress for work `{slug}`: {e}"))
        })
        .transpose()?;
```

**This is deliberately a stopgap and Task 4 replaces it.** Left as-is it would write only the newest position into a document meant to carry all of them, which is exactly the lossy round trip the design rejected. Do not stop here.

- [ ] **Step 5: Run the workspace suite**

Run: `env -u TAIYAKU_CORPUS cargo nextest run --workspace`
Expected: PASS. Existing tests that asserted `Some(Progress { … })` from `load` need their expectations changed to a one-element vector; that is a mechanical edit and part of this task.

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "Carry the widened progress read through its three callers"
```

---

### Task 4: `taiyaku.progress.json` becomes versioned, and still reads the old form

**Files:**
- Create: `crates/taiyaku-library/src/progressdoc.rs`
- Modify: `crates/taiyaku-library/src/lib.rs` (declare `pub mod progressdoc;`)
- Modify: `crates/taiyaku-library/src/export.rs` (`export_one`)
- Modify: `crates/taiyaku-library/src/migrate.rs` (`apply_progress`)
- Test: `crates/taiyaku-library/src/progressdoc/tests.rs`

**Interfaces:**
- Consumes: `Position`, `Progress`.
- Produces:
  - `progressdoc::ProgressDoc { pub taiyaku_progress: u32, pub positions: Vec<Position> }`
  - `progressdoc::from_json(&str) -> Result<Vec<Progress>, String>` — accepts the versioned form **and** a bare `{"edition":…,"slot":…}`.
  - `progressdoc::to_json(&[Position]) -> Result<String, String>`

A module of its own rather than more surface on `export.rs`: the two read paths and their reasoning are one responsibility, and `export.rs` is already long.

- [ ] **Step 1: Write the failing tests**

Create `crates/taiyaku-library/src/progressdoc/tests.rs`:

```rust
use super::{from_json, to_json};
use crate::Position;

/// The legacy form, byte for byte as it exists on disk today.
///
/// **Over the real corpus document rather than a fixture**, because a fixture
/// for this would be written by the same hand as the reader of it and would
/// render right and wrong identically. `corpus/gotoubun-v01/taiyaku.progress.json`
/// is `{"edition": "raw-mono", "slot": 193}`, and `taiyaku serve <sidecar>…`
/// runs `migrate::apply_progress` over documents in exactly that shape at
/// every start.
#[test]
fn a_bare_edition_and_slot_reads_as_one_position() {
    let held = from_json("{\n  \"edition\": \"raw-mono\",\n  \"slot\": 193\n}")
        .expect("the legacy form must still read");
    assert_eq!(held.len(), 1);
    assert_eq!(held[0].edition, "raw-mono");
    assert_eq!(held[0].slot, 193);
}

#[test]
fn the_versioned_form_reads_every_position() {
    let held = from_json(
        r#"{"taiyaku_progress":1,"positions":[
             {"edition":"raw","slot":11,"updated":"2026-08-27 09:00:00"},
             {"edition":"full-color","slot":33,"updated":"2026-08-27 09:24:41"}]}"#,
    )
    .expect("the versioned form");
    assert_eq!(
        held.iter().map(|p| p.edition.as_str()).collect::<Vec<_>>(),
        vec!["raw", "full-color"]
    );
    assert_eq!(held[1].slot, 33);
}

/// A version this build does not know is refused rather than guessed at.
#[test]
fn an_unknown_version_is_refused_by_name() {
    let why = from_json(r#"{"taiyaku_progress":2,"positions":[]}"#)
        .expect_err("a newer document must not be read as though it were this one");
    assert!(
        why.contains('2'),
        "the refusal must name the version it found, got: {why}"
    );
}

#[test]
fn export_writes_the_versioned_form_only() {
    let json = to_json(&[Position {
        edition: "raw".into(),
        slot: 11,
        updated: "2026-08-27 09:00:00".into(),
    }])
    .expect("serialising");
    assert!(json.contains("\"taiyaku_progress\": 1"), "got: {json}");
    assert!(json.contains("\"positions\""), "got: {json}");
}
```

- [ ] **Step 2: Run them and watch them fail**

Run: `cargo nextest run -p taiyaku-library progressdoc::`
Expected: FAIL to compile — the module does not exist.

- [ ] **Step 3: Write the module**

Create `crates/taiyaku-library/src/progressdoc.rs`:

```rust
//! `taiyaku.progress.json`: every position one reader holds on one work.
//!
//! **Versioned since a position became per-edition.** The document carried a
//! bare `{"edition": …, "slot": …}` while a reader held one position per work,
//! and that shape cannot say what it now has to. `taiyaku_progress: 1` is the
//! same device `TruthDoc`'s `taiyaku_truth: 1` uses, and for the same reason:
//! this document now has a shape that can change again.
//!
//! **The legacy form is still read, and that is not politeness.** Real
//! documents are on disk — `corpus/gotoubun-v01/taiyaku.progress.json` is one
//! — and `taiyaku serve <sidecar>…` runs `migrate::apply_progress` over them
//! at *every* start. A refusal there would drop a reader's position silently,
//! and `export --in-place` would then have nothing to write back.
//!
//! Reading answers `Vec<Progress>` rather than `Vec<Position>`: `updated` is
//! the store's to set, and a document's idea of when a position was recorded
//! must not overwrite the store's. Writing takes `Position`s, which is what
//! the store holds.

use serde::{Deserialize, Serialize};

use crate::{Position, Progress};

/// The version this build writes and is the only one it reads.
const VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressDoc {
    pub taiyaku_progress: u32,
    pub positions: Vec<Position>,
}

/// Read either form.
///
/// **Tried versioned-first, legacy-second, and the order matters.** A bare
/// `{"edition", "slot"}` cannot deserialise as a `ProgressDoc` — the required
/// `taiyaku_progress` is absent — while the reverse is not true of every
/// serde configuration, so the unambiguous shape is tested first and the
/// permissive one is the fallback.
///
/// # Errors
///
/// Returns an error if the text is neither form, or is a version this build
/// does not know.
pub fn from_json(text: &str) -> Result<Vec<Progress>, String> {
    if let Ok(doc) = serde_json::from_str::<ProgressDoc>(text) {
        if doc.taiyaku_progress != VERSION {
            return Err(format!(
                "this is a taiyaku_progress {} document and this build reads {VERSION}; \
                 a newer taiyaku wrote it",
                doc.taiyaku_progress
            ));
        }
        return Ok(doc
            .positions
            .into_iter()
            .map(|p| Progress {
                edition: p.edition,
                slot: p.slot,
            })
            .collect());
    }
    let legacy: Progress = serde_json::from_str(text)
        .map_err(|e| format!("not a readable progress document: {e}"))?;
    Ok(vec![legacy])
}

/// Write the versioned form.
///
/// # Errors
///
/// Returns an error if serialisation fails.
pub fn to_json(positions: &[Position]) -> Result<String, String> {
    serde_json::to_string_pretty(&ProgressDoc {
        taiyaku_progress: VERSION,
        positions: positions.to_vec(),
    })
    .map_err(|e| format!("serialising a progress document: {e}"))
}

#[cfg(test)]
mod tests;
```

In `crates/taiyaku-library/src/lib.rs`, declare it beside the other modules: `pub mod progressdoc;`

- [ ] **Step 4: Run the module's tests**

Run: `cargo nextest run -p taiyaku-library progressdoc::`
Expected: PASS, all four.

- [ ] **Step 5: Point export and migrate at it**

In `export.rs`, replace the Task 3 stopgap:

```rust
    let progress_json = if progress.is_empty() {
        None
    } else {
        Some(crate::progressdoc::to_json(&progress)?)
    };
```

In `migrate.rs`, `apply_progress` becomes:

```rust
fn apply_progress(
    conn: &Connection,
    work_id: i64,
    user_id: i64,
    path: &Path,
) -> Result<(), String> {
    let text = read_sibling(path)?;
    let positions = crate::progressdoc::from_json(&text)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    for progress in &positions {
        state::put_progress(conn, work_id, user_id, progress)?;
    }
    Ok(())
}
```

Every position in the document is applied, not the first: applying one would make an import lossy in exactly the way the new format exists to prevent.

- [ ] **Step 6: Run the library suite**

Run: `cargo nextest run -p taiyaku-library`
Expected: PASS. `exporting_writes_five_documents_beside_the_sidecar` and any test asserting the progress document's bytes need their expected JSON updated to the versioned form — that is this task's work, not a surprise.

- [ ] **Step 7: Read the real corpus document through it**

Run: `cargo nextest run -p taiyaku-library a_bare_edition_and_slot_reads_as_one_position -- --nocapture`
Then confirm the file it describes still has that shape: `cat corpus/gotoubun-v01/taiyaku.progress.json`
Expected: `{"edition": "raw-mono", "slot": 193}`. If the file has changed, update the test's literal to match what is actually on disk — the test is only worth something while it quotes a real document.

- [ ] **Step 8: Commit**

```bash
git add crates/taiyaku-library/src/progressdoc.rs crates/taiyaku-library/src/progressdoc/tests.rs crates/taiyaku-library/src/lib.rs crates/taiyaku-library/src/export.rs crates/taiyaku-library/src/migrate.rs
git commit -m "taiyaku.progress.json carries every position, and still reads the old one"
```

---

### Task 5: `GET /progress` answers the set

**Files:**
- Modify: `crates/taiyaku-serve/src/routes.rs` (`get_progress`)
- Test: `crates/taiyaku-serve/src/routes/tests.rs`

**Interfaces:**
- Consumes: `serve::progress::load -> Vec<Position>`.
- Produces: `GET /api/works/{work}/progress` → `204` when empty, else `200` with `{"positions": [{edition, slot, updated}, …]}`.

- [ ] **Step 1: Write the failing tests**

In `crates/taiyaku-serve/src/routes/tests.rs`, beside the existing progress route tests:

```rust
/// Every edition the reader has a position in comes back, newest first.
///
/// Built on `progress_round_trips_through_the_api`'s harness directly above:
/// `test_support::state` mints an isolated library, and the fixture's `raw`
/// edition is 4 pages with no spreads, so 3 is its last slot.
#[tokio::test]
async fn every_edition_the_reader_has_a_position_in_comes_back() {
    let state = Arc::new(crate::test_support::state("taiyaku-test-progress-set"));
    let app = build_router(state);

    for (edition, slot) in [("raw", 3), ("en", 1)] {
        let put = app
            .clone()
            .oneshot(
                Request::put("/api/works/t/progress")
                    .header("content-type", "application/json")
                    .body(Body::from(format!(
                        r#"{{"edition":"{edition}","slot":{slot}}}"#
                    )))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(put.status(), StatusCode::NO_CONTENT, "writing {edition}");
    }

    let got = app
        .clone()
        .oneshot(
            Request::get("/api/works/t/progress")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(got.status(), StatusCode::OK);
    let body = axum::body::to_bytes(got.into_body(), 1 << 16)
        .await
        .expect("reading the body");
    let doc: serde_json::Value = serde_json::from_slice(&body).expect("json");
    let editions: Vec<&str> = doc["positions"]
        .as_array()
        .expect("positions is an array")
        .iter()
        .map(|p| p["edition"].as_str().expect("an edition"))
        .collect();
    assert_eq!(editions.len(), 2, "both positions, not the newest only");
    assert!(editions.contains(&"raw") && editions.contains(&"en"), "got {editions:?}");

    crate::test_support::cleanup("taiyaku-test-progress-set");
}
```

**Check the fixture has a second edition before writing this.** `test_support::state`'s work `t` is the one `progress_round_trips_through_the_api` uses; run
`grep -n "editions" crates/taiyaku-serve/src/test_support.rs` and use whichever
second edition key it actually declares in place of `en`. A test that writes to
an edition the fixture does not have gets a 404 from `put_progress`'s own
edition check and proves nothing.

`a_work_with_no_position_still_answers_204` already exists inside
`progress_round_trips_through_the_api` as its first assertion — that path is
covered, and duplicating it is not this task's work.

- [ ] **Step 2: Run them and watch them fail**

Run: `cargo nextest run -p taiyaku-serve every_edition_the_reader_has_a_position_in_comes_back_newest_first`
Expected: FAIL — the body is a bare object with `edition`/`slot`, so `body["positions"]` is null.

- [ ] **Step 3: Change the handler**

In `routes.rs`, the match arms of `get_progress`:

```rust
    match loaded {
        // Nothing recorded is not an error: a work never opened has no
        // position, and 404 would say the work does not exist. An empty set
        // is that same fact, and it stays a status code rather than becoming
        // a document a client has to unpack.
        Ok(Ok(positions)) if positions.is_empty() => StatusCode::NO_CONTENT.into_response(),
        Ok(Ok(positions)) => axum::Json(serde_json::json!({ "positions": positions }))
            .into_response(),
        Ok(Err(refusal)) => refusal.into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("progress read panicked: {e}"),
        )
            .into_response(),
    }
```

- [ ] **Step 4: Run the serve suite**

Run: `cargo nextest run -p taiyaku-serve`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/taiyaku-serve/src/routes.rs crates/taiyaku-serve/src/routes/tests.rs
git commit -m "GET /progress answers every edition's position"
```

---

### Task 6: The web client and the reader

**Files:**
- Modify: `web/src/api.ts` (`StoredProgress`, `getProgress`)
- Modify: `web/src/App.tsx` (the resume path)
- Test: `web/src/api.test.ts`

**Interfaces:**
- Consumes: Task 5's route.
- Produces: `getProgress(work) -> Promise<StoredProgress | undefined>` — unchanged signature, now the newest of the set.

- [ ] **Step 1: Write the failing test**

In `web/src/api.test.ts`, beside the existing progress test:

```ts
it('takes the newest position out of the set the server answers', async () => {
  fetchMock.mockResolvedValueOnce(
    new Response(
      JSON.stringify({
        positions: [
          { edition: 'full-color', slot: 33, updated: '2026-08-27 09:24:41' },
          { edition: 'raw', slot: 11, updated: '2026-08-27 09:00:00' },
        ],
      }),
      { status: 200, headers: { 'content-type': 'application/json' } },
    ),
  )
  await expect(getProgress('taiyaku')).resolves.toEqual({
    edition: 'full-color',
    slot: 33,
  })
})

it('is undefined when the server answers 204', async () => {
  fetchMock.mockResolvedValueOnce(new Response(null, { status: 204 }))
  await expect(getProgress('taiyaku')).resolves.toBeUndefined()
})
```

Match the surrounding tests' mocking style — read them first; `api.test.ts:173` shows how a progress call is currently asserted.

- [ ] **Step 2: Run it and watch it fail**

Run: `cd web && npx vitest run src/api.test.ts`
Expected: FAIL — `getProgress` returns the whole `{positions: […]}` object.

- [ ] **Step 3: Change the client**

In `web/src/api.ts`:

```ts
export interface StoredProgress {
  edition: string
  slot: number
}

/** One position per edition, as the server stores them. */
interface ServerPosition extends StoredProgress {
  updated: string
}

/**
 * `undefined` where nothing is recorded — the server answers 204, not 404.
 *
 * The server answers **every** edition's position, newest first, since a
 * position became per-edition. The reader resumes into the head: it opens one
 * edition at a time, and the most recently updated is the one the reader was
 * last in. The rest are not discarded server-side — switching edition and
 * coming back finds the place that edition held.
 */
export async function getProgress(
  work: string,
): Promise<StoredProgress | undefined> {
  const res = await request(`/api/works/${seg(work)}/progress`)
  if (res.status === 204) return undefined
  if (!res.ok)
    throw new Error(`reading progress: ${res.status} ${await res.text()}`)
  const body = (await res.json()) as { positions?: ServerPosition[] }
  const newest = body.positions?.[0]
  return newest ? { edition: newest.edition, slot: newest.slot } : undefined
}
```

`putProgress` is unchanged: its body is still `{edition, slot}`.

- [ ] **Step 4: Run the web tests**

Run: `cd web && npx vitest run`
Expected: PASS.

- [ ] **Step 5: Check `App.tsx` needs nothing**

Run: `grep -n "getProgress" web/src/App.tsx`
Expected: the call site consumes `StoredProgress | undefined`, whose shape has not changed — so no edit. If it destructures anything else, change it here and say so in the commit message.

- [ ] **Step 6: Commit**

```bash
git add web/src/api.ts web/src/api.test.ts
git commit -m "The reader resumes into the newest of the positions the server answers"
```

---

### Task 7: `browse::home`, where a set breaks a scalar subquery

**Files:**
- Modify: `crates/taiyaku-library/src/browse.rs` (`home`)
- Test: `crates/taiyaku-library/src/browse/tests.rs`

**Interfaces:**
- Consumes: Task 1's schema.
- Produces: no signature change; `home`'s two lists stay correct and stay disjoint.

**`series` needs no change and must not be given one.** Its `LEFT JOIN progress`
now multiplies rows, but every aggregate over it is `COUNT(DISTINCT …)` —
`COUNT(DISTINCT w.id)`, `COUNT(DISTINCT e.id)` and
`COUNT(DISTINCT CASE WHEN p.work_id IS NULL THEN w.id END)` — so the counts are
unaffected and `unread` keeps its meaning exactly: a work with no position in
any edition. Checked rather than assumed; it is written down here so the next
reader does not "fix" a query that is already right.

`home` is the one that breaks, in two ways.

- [ ] **Step 1: Write the failing tests**

In `crates/taiyaku-library/src/browse/tests.rs`:

```rust
/// A work sorts by the newest of its positions, not by an arbitrary one.
///
/// `(SELECT updated FROM progress p WHERE p.work_id = w.id AND p.user_id = ?)`
/// is a scalar subquery over what is now a set: SQLite returns whichever row
/// it reaches first and reports no error, so the ordering was silently
/// arbitrary among a work's editions.
#[test]
fn continuing_sorts_on_the_newest_position_a_work_holds() {
    let (conn, user_id) = library_with_two_works();

    // Work 1 was touched long ago in `raw` and just now in `en`; work 2 in
    // between. Newest-first must therefore put work 1 ahead of work 2.
    put_progress(&conn, 1, user_id, &Progress { edition: "raw".into(), slot: 1 }).unwrap();
    conn.execute("UPDATE progress SET updated = '2020-01-01 00:00:00' WHERE work_id = 1", []).unwrap();
    put_progress(&conn, 2, user_id, &Progress { edition: "raw".into(), slot: 1 }).unwrap();
    conn.execute("UPDATE progress SET updated = '2024-01-01 00:00:00' WHERE work_id = 2", []).unwrap();
    put_progress(&conn, 1, user_id, &Progress { edition: "en".into(), slot: 1 }).unwrap();

    let home = home(&conn, &[], user_id).expect("home");
    let order: Vec<i64> = home.continuing.iter().map(|w| w.id).collect();
    assert_eq!(order, vec![1, 2], "work 1's newest position is the newest of all");
}

/// Finishing one edition while reading another does not put a work in both
/// lists.
#[test]
fn a_work_finished_in_one_edition_and_open_in_another_is_still_continuing() {
    let (conn, user_id) = library_with_two_works();
    put_progress(&conn, 1, user_id, &Progress { edition: "raw".into(), slot: 3 }).unwrap();
    conn.execute(
        "UPDATE progress SET completed_at = '2026-01-01 00:00:00'
          WHERE work_id = 1 AND edition = 'raw'",
        [],
    )
    .unwrap();
    put_progress(&conn, 1, user_id, &Progress { edition: "en".into(), slot: 1 }).unwrap();

    let home = home(&conn, &[], user_id).expect("home");
    assert!(
        home.continuing.iter().any(|w| w.id == 1),
        "an unfinished edition keeps the work continuing"
    );
    assert!(
        !home.read.iter().any(|w| w.id == 1),
        "and it must not also be sitting in Read"
    );
}
```

`library_with_two_works` seeds two works each with a `raw` and an `en` edition
and one member apiece, plus a user — reuse whatever the neighbouring tests in
that file already build rather than adding a second seeder.

- [ ] **Step 2: Run them and watch them fail**

Run: `cargo nextest run -p taiyaku-library browse::tests::continuing_sorts_on_the_newest_position_a_work_holds browse::tests::a_work_finished_in_one_edition_and_open_in_another_is_still_continuing`
Expected: FAIL. The first on ordering; the second because `read`'s predicate
matches any completed position, so work 1 appears in both lists.

- [ ] **Step 3: Aggregate the sorts, and make `read` mean finished**

In `home`, the two orderings become `MAX(…)`, and `read`'s predicate gains the
clause that keeps the lists disjoint:

```rust
    let continuing = works_of(
        conn,
        roots,
        "w.id IN (SELECT work_id FROM progress
                   WHERE user_id = ? AND completed_at IS NULL)",
        &[&user_id, &user_id],
        "(SELECT MAX(updated) FROM progress p WHERE p.work_id = w.id AND p.user_id = ?) DESC",
    )?;
    let read = works_of(
        conn,
        roots,
        "w.id IN (SELECT work_id FROM progress
                   WHERE user_id = ? AND completed_at IS NOT NULL)
         AND w.id NOT IN (SELECT work_id FROM progress
                           WHERE user_id = ? AND completed_at IS NULL)",
        &[&user_id, &user_id, &user_id],
        "(SELECT MAX(completed_at) FROM progress p WHERE p.work_id = w.id AND p.user_id = ?) DESC",
    )?;
```

**`read` binds `user_id` three times now, not twice.** `works_of` binds `extra`
in text order, and this module's own doc comment says a wrong count is "a hard
refusal from `rusqlite` rather than a wrong answer" — so the `&[…]` slice
grows with the predicate or the query refuses. Update that doc comment: it
currently states "**`user_id` binds twice** in the first two", which stops
being true here.

**Why `read` narrows rather than both lists overlapping.** A position per
edition makes "finished" and "still reading" simultaneously true of one volume,
which the old key could not express. Showing the same volume in both lists on
one screen states a contradiction to a reader; treating a work as read only
once no edition is still open keeps the two lists meaning what their headings
say. The alternative — a volume in both — was rejected for that reason alone,
and nothing measured either.

- [ ] **Step 4: Run the browse suite**

Run: `cargo nextest run -p taiyaku-library browse::`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/taiyaku-library/src/browse.rs crates/taiyaku-library/src/browse/tests.rs
git commit -m "home: aggregate the position sorts, and keep Continuing and Read disjoint"
```

---

### Task 8: Detect a superseded work

**Files:**
- Modify: `crates/taiyaku-library/src/derive.rs` (`write_work` returns its `work_id`; a new `superseded` function; `DeriveReport`)
- Test: `crates/taiyaku-library/src/derive/tests.rs`

**Interfaces:**
- Consumes: nothing from Tasks 1–7.
- Produces:
  - `write_work(…) -> Result<i64, String>` — the work row it wrote.
  - `fn superseded(conn: &Connection, written: &BTreeSet<i64>) -> Result<Vec<(i64, i64)>, String>` — `(superseded_work_id, target_work_id)` pairs.
  - `DeriveReport::emptied_works: usize`.

Detection only. Task 9 moves and deletes.

- [ ] **Step 1: Write the failing test**

In `crates/taiyaku-library/src/derive/tests.rs`:

```rust
/// A work whose every member is now held by a work this derive wrote.
///
/// **Not "a work with zero editions", which never occurs.** Under
/// `Scope::Scan`, `import::replace_editions` deletes only the editions the
/// incoming document names, and the superseded work is not in `groups` at all
/// — so nothing empties it. Measured 2026-08-27: after the fold, the two
/// superseded works still held their editions over the same files.
#[test]
fn a_work_whose_material_all_moved_is_superseded() {
    let conn = Connection::open_in_memory().expect("in-memory database");
    crate::schema::migrate(&conn).expect("migrating");
    conn.execute("INSERT INTO root (id, path) VALUES (1, '/r')", [])
        .expect("root");

    // Work 1 keeps its own `raw` and gains `full-color` over the same archive
    // work 2 holds; work 2 is the one the fold supersedes.
    seed_work(&conn, 1, "target", &[("raw", "aaa"), ("full-color", "bbb")]);
    seed_work(&conn, 2, "superseded", &[("raw", "bbb")]);

    let written = BTreeSet::from([1_i64]);
    let found = superseded(&conn, &written).expect("detecting");
    assert_eq!(found, vec![(2, 1)], "work 2's material is all under work 1");
}

/// Half a work's material moving is not supersession.
#[test]
fn a_work_that_kept_some_of_its_material_is_not_superseded() {
    let conn = Connection::open_in_memory().expect("in-memory database");
    crate::schema::migrate(&conn).expect("migrating");
    conn.execute("INSERT INTO root (id, path) VALUES (1, '/r')", [])
        .expect("root");
    seed_work(&conn, 1, "target", &[("full-color", "bbb")]);
    seed_work(&conn, 2, "kept", &[("raw", "bbb"), ("raw2", "ccc")]);

    let found = superseded(&conn, &BTreeSet::from([1_i64])).expect("detecting");
    assert!(found.is_empty(), "ccc is held by nothing else, got {found:?}");
}

/// An imported work is never superseded by a scan.
#[test]
fn a_work_with_no_root_is_left_alone() {
    let conn = Connection::open_in_memory().expect("in-memory database");
    crate::schema::migrate(&conn).expect("migrating");
    conn.execute("INSERT INTO root (id, path) VALUES (1, '/r')", [])
        .expect("root");
    seed_work(&conn, 1, "target", &[("full-color", "bbb")]);
    seed_work_without_root(&conn, 2, "imported", &[("raw", "bbb")]);

    let found = superseded(&conn, &BTreeSet::from([1_i64])).expect("detecting");
    assert!(found.is_empty(), "an imported work has no root, got {found:?}");
}
```

Write `seed_work` and `seed_work_without_root` as helpers in the same file — one `work` row, one `edition` per pair, one `member` per pair carrying that `sha256`, with `root_id` 1 and NULL respectively.

- [ ] **Step 2: Run them and watch them fail**

Run: `cargo nextest run -p taiyaku-library derive::tests::a_work_whose_material_all_moved_is_superseded`
Expected: FAIL to compile — `superseded` does not exist.

- [ ] **Step 3: Implement it**

In `derive.rs`:

```rust
/// Works whose every member is now held by a work this derive wrote, paired
/// with the work that holds it.
///
/// **The three conditions, and why each is needed.** A work qualifies when
/// this derive did not write it; when it holds at least one member and *every*
/// member's `sha256` is also held by a written work; and when every one of its
/// members carries a non-NULL `root_id`.
///
/// The third keeps an imported work safe. `006_member_root.sql` states that
/// NULL means "under no root", "which is exactly true of an imported work" —
/// so a hand-imported `taiyaku.json` naming the same archive is never
/// superseded by a scan. A scanned work whose root was removed also reads NULL
/// throughout, `root_id` being `ON DELETE SET NULL`, and is likewise left
/// alone: the safe direction of an ambiguity this function must not resolve.
///
/// "Every member", never "any": a work half of whose material moved is not
/// emptied, and deleting it would take material nothing else holds.
///
/// # Errors
///
/// Returns an error if SQLite fails.
fn superseded(
    conn: &Connection,
    written: &BTreeSet<i64>,
) -> Result<Vec<(i64, i64)>, String> {
    if written.is_empty() {
        return Ok(Vec::new());
    }
    let held = digests_by_work(conn)?;
    let mut found = Vec::new();
    for (work_id, members) in &held {
        if written.contains(work_id) || members.is_empty() {
            continue;
        }
        if members.iter().any(|m| m.root_id.is_none()) {
            continue;
        }
        let mut target: Option<i64> = None;
        let covered = members.iter().all(|m| {
            let holder = written
                .iter()
                .find(|w| held.get(w).is_some_and(|ms| ms.iter().any(|h| h.sha256 == m.sha256)));
            if let Some(w) = holder {
                target.get_or_insert(*w);
                true
            } else {
                false
            }
        });
        if covered {
            if let Some(t) = target {
                found.push((*work_id, t));
            }
        }
    }
    found.sort_unstable();
    Ok(found)
}
```

with a small row type and one query beside it:

```rust
/// One member as [`superseded`] reads it.
struct HeldDigest {
    sha256: String,
    root_id: Option<i64>,
}

/// Every work's members, by work id. One query rather than one per work: a
/// library of thousands is one pass here and thousands of round trips the
/// other way.
fn digests_by_work(
    conn: &Connection,
) -> Result<BTreeMap<i64, Vec<HeldDigest>>, String> {
    let mut statement = conn
        .prepare(
            "SELECT edition.work_id, member.sha256, member.root_id
               FROM member JOIN edition ON edition.id = member.edition_id",
        )
        .map_err(|e| format!("preparing the digest read: {e}"))?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                HeldDigest {
                    sha256: row.get(1)?,
                    root_id: row.get(2)?,
                },
            ))
        })
        .map_err(|e| format!("reading member digests: {e}"))?;
    let mut by_work: BTreeMap<i64, Vec<HeldDigest>> = BTreeMap::new();
    for row in rows {
        let (work_id, digest) = row.map_err(|e| format!("reading a member digest: {e}"))?;
        by_work.entry(work_id).or_default().push(digest);
    }
    Ok(by_work)
}
```

- [ ] **Step 4: Have `write_work` report its work id**

Change its signature to `-> Result<i64, String>` and return the resolved id at the end. In `derive`, collect them:

```rust
    let mut written: BTreeSet<i64> = BTreeSet::new();
    for group in groups.into_values() {
        let series_id = *series_ids
            .get(&group.series)
            .ok_or_else(|| format!("no series row was reconciled for `{}`", group.series))?;
        written.insert(write_work(conn, series_id, &group, &mut report)?);
    }

    let folded = superseded(conn, &written)?;
    report.emptied_works = folded.len();
```

- [ ] **Step 5: Add the report field**

In `DeriveReport`, beside `kept_editions`:

```rust
    /// Works whose every member is now held by a work this derive wrote, and
    /// which this derive therefore folded away.
    ///
    /// **Counted for `kept_editions`' reason**: a store that silently deleted
    /// a work would be worse than the duplication it fixes, and a number
    /// nobody prints is a change nobody can see. This one is the first thing
    /// in the project to delete a `work` row, so it is the last one that
    /// should do it quietly.
    pub emptied_works: usize,
```

- [ ] **Step 6: Run the derive suite**

Run: `cargo nextest run -p taiyaku-library derive::`
Expected: PASS, all three new tests included.

- [ ] **Step 7: Commit**

```bash
git add crates/taiyaku-library/src/derive.rs crates/taiyaku-library/src/derive/tests.rs
git commit -m "Detect a work whose material has all moved to another"
```

---

### Task 9: Move the reader rows, then delete the work

**Files:**
- Create: `crates/taiyaku-library/src/fold.rs`, `crates/taiyaku-library/src/fold/tests.rs`
- Modify: `crates/taiyaku-library/src/lib.rs` (declare `mod fold;`)
- Modify: `crates/taiyaku-library/src/derive.rs` (call it)

**Interfaces:**
- Consumes: `superseded` from Task 8.
- Produces: `fold::absorb(conn: &Connection, from_work: i64, into_work: i64) -> Result<Moved, String>`, where `Moved { positions: usize, notes: usize, truth_pages: usize, collisions: usize }`.

Its own module: this is the only code in the project that deletes a work, and the ordering argument that makes it safe should be readable in one file.

- [ ] **Step 1: Write the failing tests**

Create `crates/taiyaku-library/src/fold/tests.rs`:

```rust
/// The whole point: a reader's place follows the material.
#[test]
fn a_position_moves_onto_the_edition_holding_the_same_archive() {
    let (conn, from, into) = folded_pair();
    put_progress(&conn, from, 1, &Progress { edition: "raw".into(), slot: 33 })
        .expect("the reader's place on the superseded work");
    put_progress(&conn, into, 1, &Progress { edition: "raw".into(), slot: 11 })
        .expect("and a place on the target's own edition");

    absorb(&conn, from, into).expect("folding");

    let mut held: Vec<(String, u32)> = progress(&conn, into, 1)
        .expect("reading back")
        .into_iter()
        .map(|p| (p.edition, p.slot))
        .collect();
    held.sort();
    assert_eq!(
        held,
        vec![("full-color".to_string(), 33), ("raw".to_string(), 11)],
        "both places stand, the moved one under the edition that holds its archive"
    );
}

/// The edition string is rewritten, and this is the test that proves it.
///
/// Left as `raw`, a note about a colour page lands on the monochrome archive:
/// same slot number, different book, and — measured 2026-08-27 — accepted by
/// every constraint in the schema. Nothing but this assertion catches it.
#[test]
fn a_moved_note_names_the_edition_that_holds_its_archive() {
    let (conn, from, into) = folded_pair();
    seed_note(&conn, from, "raw", 33);

    absorb(&conn, from, into).expect("folding");

    let edition: String = conn
        .query_row("SELECT edition FROM note WHERE work_id = ?1", [into], |r| {
            r.get(0)
        })
        .expect("the moved note");
    assert_eq!(edition, "full-color", "a note kept on `raw` is on the wrong book");
}

/// `truth_page` cannot be moved by UPDATE — `truth_box`'s composite foreign
/// key has no ON UPDATE — so this is delete-and-reinsert, page rows first.
#[test]
fn truth_moves_with_its_boxes_and_its_digests() {
    let (conn, from, into) = folded_pair();
    seed_truth(&conn, from, "raw", 33, "bbb");

    absorb(&conn, from, into).expect("folding");

    let (edition, page): (String, u32) = conn
        .query_row(
            "SELECT edition, page FROM truth_box WHERE work_id = ?1",
            [into],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .expect("the moved box");
    assert_eq!((edition.as_str(), page), ("full-color", 33));
    let digest: String = conn
        .query_row(
            "SELECT sha256 FROM truth_member WHERE work_id = ?1",
            [into],
            |r| r.get(0),
        )
        .expect("the moved digest");
    assert_eq!(digest, "bbb", "the labelling still names the archive it was drawn on");
}

/// The delete is what closes O73, and it runs last.
#[test]
fn the_superseded_work_is_gone_and_took_nothing_with_it() {
    let (conn, from, into) = folded_pair();
    put_progress(&conn, from, 1, &Progress { edition: "raw".into(), slot: 33 })
        .expect("a place");
    seed_note(&conn, from, "raw", 33);

    absorb(&conn, from, into).expect("folding");

    let works: i64 = conn
        .query_row("SELECT count(*) FROM work WHERE id = ?1", [from], |r| r.get(0))
        .expect("counting");
    assert_eq!(works, 0, "the superseded work is deleted");
    let positions: i64 = conn
        .query_row("SELECT count(*) FROM progress", [], |r| r.get(0))
        .expect("counting");
    let notes: i64 = conn
        .query_row("SELECT count(*) FROM note", [], |r| r.get(0))
        .expect("counting");
    assert_eq!(
        (positions, notes),
        (1, 1),
        "the cascade took nothing: the rows moved before the delete ran"
    );
}
```

`folded_pair()` builds the shape Task 8's `seed_work` builds — work `into` holding `raw`/`aaa` and `full-color`/`bbb`, work `from` holding `raw`/`bbb` — plus a user row, and returns `(conn, from_id, into_id)`.

- [ ] **Step 2: Run them and watch them fail**

Run: `cargo nextest run -p taiyaku-library fold::`
Expected: FAIL to compile — the module does not exist.

- [ ] **Step 3: Write the module**

Create `crates/taiyaku-library/src/fold.rs`. The mapping and the order are the whole content:

```rust
//! Absorbing a superseded work into the work its material moved to.
//!
//! **The only code in this project that deletes a `work` row**, which is why
//! it is a module rather than a function inside `derive`. The safety argument
//! is an ordering argument and it should be readable in one place: the reader
//! rows move first, in one transaction, and the delete is the last statement.
//! Reversed, `work`'s `ON DELETE CASCADE` takes a reader's position, their
//! notes and the library's ground truth with it — measured, not feared:
//! `docs/notes.md`, "O73's cascade, measured — 2026-08-27".

use std::collections::HashMap;

use rusqlite::Connection;

/// What one absorption moved.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Moved {
    pub positions: usize,
    pub notes: usize,
    pub truth_pages: usize,
    /// Rows the target already held, kept as the target's.
    pub collisions: usize,
}

/// Which edition of `into_work` holds the same archives as each edition of
/// `from_work`.
///
/// **By member digest, not by the rule that caused the fold.** The two
/// editions are the same bytes — measured at `920d44d8…`, 195 pages, on both
/// sides — so slots, page indices and box coordinates transfer one for one.
/// Reading `set_edition_key` off the rule instead would tie this to why the
/// material moved and would guess where no rule exists.
///
/// An edition with no counterpart is absent from the map, and its rows are
/// left where they are: [`absorb`] refuses the whole fold rather than move
/// half of it.
fn edition_map(
    conn: &Connection,
    from_work: i64,
    into_work: i64,
) -> Result<HashMap<String, String>, String> {
    let by_work = |work_id: i64| -> Result<Vec<(String, Vec<String>)>, String> {
        let mut statement = conn
            .prepare(
                "SELECT edition.key, member.sha256
                   FROM edition JOIN member ON member.edition_id = edition.id
                  WHERE edition.work_id = ?1
                  ORDER BY edition.key, member.ordinal",
            )
            .map_err(|e| format!("preparing the edition read: {e}"))?;
        let rows = statement
            .query_map([work_id], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|e| format!("reading editions of work {work_id}: {e}"))?;
        let mut grouped: Vec<(String, Vec<String>)> = Vec::new();
        for row in rows {
            let (key, sha) = row.map_err(|e| format!("reading an edition row: {e}"))?;
            match grouped.last_mut() {
                Some((held, digests)) if *held == key => digests.push(sha),
                _ => grouped.push((key, vec![sha])),
            }
        }
        Ok(grouped)
    };

    let source = by_work(from_work)?;
    let target = by_work(into_work)?;
    let mut map = HashMap::new();
    for (key, digests) in source {
        if let Some((their_key, _)) = target.iter().find(|(_, theirs)| *theirs == digests) {
            map.insert(key, their_key.clone());
        }
    }
    Ok(map)
}

/// Move every reader row off `from_work` and delete it.
///
/// # Errors
///
/// Returns an error if SQLite fails, or if any edition of `from_work` has no
/// digest-identical counterpart on `into_work` — in which case nothing is
/// moved and nothing is deleted.
pub fn absorb(
    conn: &Connection,
    from_work: i64,
    into_work: i64,
) -> Result<Moved, String> {
    let map = edition_map(conn, from_work, into_work)?;
    let editions: Vec<String> = conn
        .prepare("SELECT key FROM edition WHERE work_id = ?1")
        .and_then(|mut s| {
            s.query_map([from_work], |r| r.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<String>>>()
        })
        .map_err(|e| format!("reading editions of work {from_work}: {e}"))?;
    for key in &editions {
        if !map.contains_key(key) {
            return Err(format!(
                "work {from_work}'s edition `{key}` has no digest-identical edition on \
                 work {into_work}; nothing was moved"
            ));
        }
    }

    let mut moved = Moved::default();
    for (from_key, into_key) in &map {
        moved.positions += move_positions(conn, from_work, into_work, from_key, into_key)?;
        moved.notes += move_notes(conn, from_work, into_work, from_key, into_key)?;
        let (pages, collisions) =
            move_truth(conn, from_work, into_work, from_key, into_key)?;
        moved.truth_pages += pages;
        moved.collisions += collisions;
    }

    // **Last, and only after every move above.** `progress`, `note`,
    // `truth_page`, `truth_box` and `truth_member` all reference `work(id)
    // ON DELETE CASCADE`; a delete before the moves takes every one of them.
    conn.execute("DELETE FROM work WHERE id = ?1", [from_work])
        .map_err(|e| format!("deleting the superseded work {from_work}: {e}"))?;
    Ok(moved)
}
```

Write the three movers beneath it:

```rust
/// A reader's place, carried across with its history.
///
/// `completed_at` and `read_count` come too: they are facts about reading this
/// rendition, and the rendition is what moved.
///
/// The conflict is impossible by construction — the target's edition was
/// derived by the scan that caused this fold, so no reader can hold a position
/// in it yet — and is guarded anyway, the newer position winning. A guard for
/// an unreachable case is cheap; discovering it was reachable by losing a
/// position is not.
fn move_positions(
    conn: &Connection,
    from_work: i64,
    into_work: i64,
    from_key: &str,
    into_key: &str,
) -> Result<usize, String> {
    conn.execute(
        "INSERT INTO progress (user_id, work_id, edition, slot, updated, completed_at, read_count)
         SELECT user_id, ?2, ?4, slot, updated, completed_at, read_count
           FROM progress WHERE work_id = ?1 AND edition = ?3
         ON CONFLICT(user_id, work_id, edition) DO UPDATE SET
           slot = excluded.slot,
           updated = excluded.updated
         WHERE excluded.updated > progress.updated",
        rusqlite::params![from_work, into_work, from_key, into_key],
    )
    .map_err(|e| format!("moving positions off work {from_work}: {e}"))
}

/// Notes, with the edition string rewritten.
///
/// **The rewrite is the whole of this function's risk.** `note.id` is a ULID
/// so nothing collides, and an `UPDATE` that moved `work_id` alone would be
/// accepted by every constraint in the schema — landing a note about a colour
/// page on the monochrome archive at the same slot number, with nothing saying
/// so. Measured 2026-08-27.
fn move_notes(
    conn: &Connection,
    from_work: i64,
    into_work: i64,
    from_key: &str,
    into_key: &str,
) -> Result<usize, String> {
    conn.execute(
        "UPDATE note SET work_id = ?2, edition = ?4
          WHERE work_id = ?1 AND edition = ?3",
        rusqlite::params![from_work, into_work, from_key, into_key],
    )
    .map_err(|e| format!("moving notes off work {from_work}: {e}"))
}

/// Ground truth: pages, then boxes, then the digests they were drawn against.
///
/// **Delete-and-reinsert, not `UPDATE`, and page rows first.** `truth_box`'s
/// foreign key names `(work_id, edition, page)` with no `ON UPDATE`, so
/// `UPDATE truth_page SET work_id = …` is refused outright — measured, both
/// with and without the edition rewrite. The insertion order is `put_truth`'s
/// own, which that composite key exists to enforce.
///
/// `INSERT OR IGNORE` on the two keyed tables: a page the target already holds
/// stays the target's, and is counted rather than silently overwritten. Truth
/// is library-wide with no `user_id`, so "the target already holds it" means
/// somebody has labelled that page, and their work is not this fold's to
/// discard.
///
/// Returns the pages moved and the collisions kept.
fn move_truth(
    conn: &Connection,
    from_work: i64,
    into_work: i64,
    from_key: &str,
    into_key: &str,
) -> Result<(usize, usize), String> {
    let params = rusqlite::params![from_work, into_work, from_key, into_key];
    let offered: usize = conn
        .query_row(
            "SELECT count(*) FROM truth_page WHERE work_id = ?1 AND edition = ?2",
            rusqlite::params![from_work, from_key],
            |r| r.get::<_, i64>(0),
        )
        .map(|n| usize::try_from(n).unwrap_or(usize::MAX))
        .map_err(|e| format!("counting truth pages on work {from_work}: {e}"))?;

    let pages = conn
        .execute(
            "INSERT OR IGNORE INTO truth_page (work_id, edition, page)
             SELECT ?2, ?4, page FROM truth_page WHERE work_id = ?1 AND edition = ?3",
            params,
        )
        .map_err(|e| format!("moving truth pages off work {from_work}: {e}"))?;

    conn.execute(
        "INSERT INTO truth_box (work_id, edition, page, x, y, w, h, axis, kind, ignore, text)
         SELECT ?2, ?4, b.page, b.x, b.y, b.w, b.h, b.axis, b.kind, b.ignore, b.text
           FROM truth_box b
          WHERE b.work_id = ?1 AND b.edition = ?3
            AND EXISTS (SELECT 1 FROM truth_page t
                         WHERE t.work_id = ?2 AND t.edition = ?4 AND t.page = b.page)",
        params,
    )
    .map_err(|e| format!("moving truth boxes off work {from_work}: {e}"))?;

    conn.execute(
        "INSERT OR IGNORE INTO truth_member (work_id, edition, ordinal, sha256)
         SELECT ?2, ?4, ordinal, sha256
           FROM truth_member WHERE work_id = ?1 AND edition = ?3",
        params,
    )
    .map_err(|e| format!("moving truth digests off work {from_work}: {e}"))?;

    // The source rows go with the work in `absorb`'s final DELETE; nothing is
    // deleted here, so a failure below leaves both copies rather than neither.
    Ok((pages, offered.saturating_sub(pages)))
}
```

The `EXISTS` clause on the box insert is not decoration: where the target
already held that page, `INSERT OR IGNORE` left the target's page row standing
and the source's boxes must not be added underneath it — that would merge two
labellers' boxes on one page, which is a different decision from "the target's
labelling wins" and not one this fold gets to make.

Wrap the whole of `absorb`'s body in a transaction — `let tx = conn.unchecked_transaction().map_err(…)?;` … `tx.commit().map_err(…)?;` — so a failure part way leaves the work and every one of its rows exactly as they were.

- [ ] **Step 4: Run the module's tests**

Run: `cargo nextest run -p taiyaku-library fold::`
Expected: PASS, all four.

- [ ] **Step 5: Call it from derive**

Replace Task 8's `report.emptied_works = folded.len();` with:

```rust
    let folded = superseded(conn, &written)?;
    for (from_work, into_work) in folded {
        let moved = fold::absorb(conn, from_work, into_work)?;
        report.emptied_works += 1;
        report.moved_positions += moved.positions;
        report.moved_notes += moved.notes;
        report.moved_truth_pages += moved.truth_pages;
    }
```

and add `moved_positions`, `moved_notes`, `moved_truth_pages` and `moved_truth_collisions` to `DeriveReport` beside `emptied_works`, each with a one-line doc. The collision count is `Moved::collisions` — pages the target already held, kept as the target's; it is reported rather than dropped for `kept_editions`' reason, since a labelling silently not moved is exactly the kind of loss these counters exist to make visible.

- [ ] **Step 6: Count the series a fold leaves empty**

The design's §6 says the emptied `series` row is left standing and "gets a count in the report". After the fold loop:

```rust
    report.emptied_series = conn
        .query_row(
            "SELECT count(*) FROM series s
              WHERE NOT EXISTS (SELECT 1 FROM work w WHERE w.series_id = s.id)",
            [],
            |r| r.get::<_, i64>(0),
        )
        .map(|n| usize::try_from(n).unwrap_or(usize::MAX))
        .map_err(|e| format!("counting series left with no works: {e}"))?;
```

with the field documented as what it is:

```rust
    /// Series rows holding no work at all.
    ///
    /// **Left standing deliberately.** `series` has no dependency on `work` —
    /// `work.series_id` is `ON DELETE SET NULL` in the other direction — so a
    /// fold that deletes a series' last work leaves the row behind. `browse`
    /// filters a series with no works out of its listing, so it is invisible
    /// rather than wrong, and deleting it would cascade `series_title`,
    /// `series_tag`, `credit`, `external_id` and `collection_item` — a second
    /// cascade question, and not the one the fold was built to answer. Counted
    /// so that "invisible" never quietly becomes "unknown".
    pub emptied_series: usize,
```

Print it from `derive_summary` in the non-zero group alongside `emptied_works`.

- [ ] **Step 7: Print it**

`crates/taiyaku-cli/src/library.rs`'s **`derive_summary`** builds that line, and it already has the pattern: `series`/`works`/`editions`/`members` print unconditionally and every other counter prints only when non-zero, for the reason its own doc comment gives. Add `emptied_works` to the non-zero group:

```rust
    if report.emptied_works > 0 {
        let _ = write!(
            line,
            ", {} work(s) folded away ({} position(s), {} note(s) moved)",
            report.emptied_works, report.moved_positions, report.moved_notes
        );
    }
```

Extend that doc comment's list of non-zero counters to name `emptied_works` too — it enumerates them, and a list that stops naming what the function prints is the drift the comment exists to prevent.

- [ ] **Step 8: Run the whole suite**

Run: `env -u TAIYAKU_CORPUS cargo nextest run --workspace`
Expected: PASS.

- [ ] **Step 9: Commit**

```bash
git add crates/taiyaku-library/src/fold.rs crates/taiyaku-library/src/fold/tests.rs crates/taiyaku-library/src/lib.rs crates/taiyaku-library/src/derive.rs crates/taiyaku-cli/src/library.rs
git commit -m "Absorb a superseded work into the work its material moved to"
```

---

### Task 10: Prove it against real archives, and close O73

**Files:**
- Modify: `docs/notes.md` (the run)
- Modify: `docs/status.md` (the five-part close)
- Test: the four-archive reproduction, by hand

**Interfaces:**
- Consumes: everything above.
- Produces: O73 closed on evidence.

- [ ] **Step 1: Rebuild the reproduction**

```bash
cd /home/davey/projects/taiyaku
mkdir -p target/o73/tree/"Go-Toubun no Hanayome" target/o73/tree/"Go-Toubun no Hanayome - Full Color"
ln corpus/gotoubun-v01/raw-mono/*.cbz corpus/gotoubun-v03/raw-mono/*.cbz  target/o73/tree/"Go-Toubun no Hanayome"/
ln corpus/gotoubun-v01/raw-color/*.cbz corpus/gotoubun-v03/raw-color/*.cbz target/o73/tree/"Go-Toubun no Hanayome - Full Color"/
cargo build --bin taiyaku
./target/debug/taiyaku library scan --add "$PWD/target/o73/tree" --db target/o73/lib.db
```

Expected: `derived 2 series, 4 works, 4 editions, 4 members; 1 proposal(s) raised`.

- [ ] **Step 2: Plant a reader, through the API**

```bash
printf 'readerpw\nreaderpw\n' | ./target/debug/taiyaku library user promote reader --db target/o73/lib.db
./target/debug/taiyaku serve --db target/o73/lib.db --addr 127.0.0.1:8099 &
```

Then `PUT` a position, a note and a truth document onto each of the four works — `docs/notes.md`'s "O73's cascade, measured" section gives the exact calls, including the `If-Match` each write needs. Slots 11, 22, 33, 44, one per work.

**Not by `INSERT`.** A row the product's own write path did not produce proves nothing about the product's own write path.

- [ ] **Step 3: Fold it**

```bash
./target/debug/taiyaku library proposals --accept 1 --db target/o73/lib.db
./target/debug/taiyaku library scan --db target/o73/lib.db
```

Expected: the derived line now reports works folded away.

- [ ] **Step 4: Assert the end state**

```bash
sqlite3 target/o73/lib.db "SELECT (SELECT count(*) FROM work), (SELECT count(*) FROM edition), (SELECT count(*) FROM member), (SELECT count(DISTINCT path) FROM member);"
sqlite3 target/o73/lib.db "SELECT work_id, edition, slot FROM progress ORDER BY work_id, edition;"
```

Expected: `2|4|4|4` — two works, four editions, four members over four files, the duplication gone. And the reader's colour position on work 1 under edition `full-color` at slot 33, beside their mono position on `raw` at slot 11.

- [ ] **Step 5: Confirm the reader can still reach it**

Restart `serve` and fetch work 1's progress. Expected: both positions in the set, newest first. Fetch `full-color/pages/33`. Expected: `200` and a colour page.

- [ ] **Step 6: Clean up and confirm the corpus is untouched**

```bash
W=target/o73
rm -rf "${W:?}"
find corpus -newermt '<the time the run started>' -printf '%p\n'
```

Expected: no output. `${W:?}` rather than `"$W"`: the colon form aborts when the variable is set but empty, which `set -u` does not catch.

- [ ] **Step 7: Record the run**

Append a section to `docs/notes.md` — what was run, the counts before and after, and the reader rows that survived. Figures from the run, never recalled.

- [ ] **Step 8: Close O73, all five parts**

In `docs/status.md`:
1. the row's checkbox `[ ]` → `[x]`;
2. the row's annotation, saying what closed it;
3. the prose bullet, rewritten to say what was done;
4. the test count, which appears **twice** — "Six crates, N tests" and "Nine of the N are standing checks";
5. the "N are now closed — leaving M" sentence, and the "What remains … M open items" line near the bottom.

Then the sweep:

```bash
grep -c '^| O[0-9]' docs/status.md
grep -c '^| O[0-9]* | \[x\]' docs/status.md
grep -n -A1 '^What remains:' docs/status.md
grep -rn 'O73' docs/ web/src/ crates/
```

Read **every** hit of that last one. `web/src/curation.ts`'s `LEFT_STANDING` and its two tests name O73 and assert on its text; a closed item makes that copy a false statement, so the sentence and its tests change with it.

- [ ] **Step 9: Run the gate**

Run: `env -u TAIYAKU_CORPUS scripts/check-all.sh`
Expected: a final line reading `all green`. Do not pipe it through `tail` or append `; echo` — either masks the exit status. Quote the line.

Then the corpus job on its own:

Run: `TAIYAKU_CORPUS="$PWD/corpus" cargo test --workspace --all-features corpus:: -- --nocapture --test-threads=1`
Expected: PASS.

- [ ] **Step 10: Commit**

```bash
git add docs/notes.md docs/status.md web/src/curation.ts web/src/curation.test.ts web/src/AcceptForm.test.tsx
git commit -m "O73 closes: the library stops holding a folded volume twice"
```

---

## Notes for the implementer

**The mutation sweep is worth running over `fold.rs` and `superseded`.** Both are places where a silently wrong answer looks exactly like a right one — an edition mapped to the wrong counterpart, a `every` that became an `any`. `scripts/mutants.sh` with `--since main` reads `git diff main...HEAD`, so commit before sweeping or it re-reports what you already fixed. A mutation that changes a SQL parameter count is caught mechanically and proves nothing; ignore those.

**Task 3 leaves `export.rs` knowingly lossy for one commit.** That is called out in its own step and Task 4 repairs it. If the plan is executed out of order, do not stop after Task 3.

**Two claims in this plan are load-bearing and both were measured rather than reasoned about:** that `UPDATE truth_page SET work_id = …` is refused by `truth_box`'s foreign key, and that a `note` moved without rewriting its `edition` is accepted by every constraint in the schema. If either turns out false when you reach it, stop and re-read `docs/notes.md`'s measurement section before designing around it.
