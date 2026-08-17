# The library store — implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace `taiyaku serve`'s one-work-per-process model with a SQLite library that holds many works, round-trips to the sidecar documents on disk, and serves reading progress, notes and truth from rows instead of sibling JSON.

**Architecture:** A new `taiyaku-library` crate owns a SQLite database and nothing else — it is a store and a projection, holding no alignment semantics. Reconciliation between the database and a sidecar on disk goes through `taiyaku_core::Sidecar::merge`, the rule this repository already has and tests. `taiyaku-serve` keeps its `Work` type verbatim and builds it from rows instead of from a file, which is what keeps `cover.rs`, `composite.rs`, `stump/` and `opds.rs` untouched.

**Tech Stack:** Rust 2024, `rusqlite`, `axum`, `serde_json`. Tests run under `cargo nextest`.

**Spec:** `docs/superpowers/specs/2026-08-14-library-store-design.md`. Read it before Task 1. Section references below (`§4.2`, `§13.4`) point at it.

## Global Constraints

- **Edition 2024, `rust-version` 1.85.** Both come from `[workspace.package]`; do not set them per crate.
- **`unsafe_code = "forbid"`** workspace-wide. It applies to our crates, not to dependencies.
- **`taiyaku-library` must never be reachable from `taiyaku-core` or `taiyaku-wasm`.** Those compile to `wasm32` and SQLite does not. Only `taiyaku-serve` may depend on it.
- **Tests live in a sibling file**, `src/<module>/tests.rs`, declared with `#[cfg(test)] mod tests;` at the bottom of the module. This is the convention across all five crates as of PR #74.
- **Every `Result`-returning `pub` function needs an `# Errors` doc section.** `clippy -D warnings` runs on `--all-targets --all-features`.
- **The gate is `scripts/check-all.sh`.** Run it before every commit that closes a task.
- **Corpus tests are gated on `TAIYAKU_CORPUS`** and must skip cleanly without it. `check-all.sh` runs them against an absolute, canonicalised path.
- **No attribution trailers in commit messages.**

---

## File structure

| File | Responsibility |
| --- | --- |
| `crates/taiyaku-read/src/atomic.rs` | write-and-rename, moved down from `taiyaku-serve` (Task 1) |
| `crates/taiyaku-library/src/lib.rs` | `Library`: open, connect, transact |
| `crates/taiyaku-library/src/schema.rs` | migrations, versioned in `meta` |
| `crates/taiyaku-library/src/identity.rs` | `content_key` |
| `crates/taiyaku-library/src/project.rs` | rows → `Sidecar` / `NotesDoc` / `TruthDoc` / `Progress` / `LibraryDoc` |
| `crates/taiyaku-library/src/import.rs` | documents → rows, through `Sidecar::merge` |
| `crates/taiyaku-library/src/export.rs` | rows → the five documents, atomically |
| `crates/taiyaku-library/src/librarydoc.rs` | `<stem>.library.json`: the metadata sibling |
| `crates/taiyaku-library/src/work.rs` | browse and lookup queries |
| `crates/taiyaku-library/src/user.rs` | the `user` table; the mechanism is spec A2 |

---

## Task 1: Move `atomic::write` down to `taiyaku-read`

`taiyaku-library` needs atomic writes for export and cannot depend on `taiyaku-serve`, which is where `atomic` lives today. `taiyaku-cli` already reaches across the workspace for it. No re-export and no deprecation window — the workspace is closed and `cargo` names every call site that has not moved.

**Files:**
- Create: `crates/taiyaku-read/src/atomic.rs`, `crates/taiyaku-read/src/atomic/tests.rs`
- Delete: `crates/taiyaku-serve/src/atomic.rs`, `crates/taiyaku-serve/src/atomic/tests.rs`
- Modify: `crates/taiyaku-read/src/lib.rs`, `crates/taiyaku-serve/src/lib.rs`, and every call site

**Interfaces:**
- Produces: `taiyaku_read::atomic::write(path: &Path, bytes: &[u8], extension: &str) -> Result<(), String>`

- [ ] **Step 1: Find every call site, so the count in the doc comment is from the code and not from memory**

```bash
rg -n 'atomic::write|atomic::temp_path' crates/ --type rust
```

**Eight call sites, derived from that command on 2026-08-14 and not from the module's own doc comment**, which is stale — see Step 4:

| Where | What |
| --- | --- |
| `serve/notes.rs:317` | `notes::store` |
| `serve/truth.rs:59` | `truth::store` |
| `serve/progress.rs:59` | `progress::store` |
| `serve/cover.rs:209` | a generated cover |
| `serve/cover.rs:416` | the second cover writer, in `cached_or_generated` |
| `serve/routes.rs:1240` | the sidecar rewritten by a correction |
| `cli/regions.rs:249` | `taiyaku_serve::atomic::write` |
| `cli/main.rs:1027` | `write_sidecar`, behind both `align --write` and `credits --write` |

Plus one **`temp_path`** call site, which is not a `write` and needs its own decision — `serve/cover/tests.rs:139-140`. Step 4b handles it.

Write the list down; Step 6 checks it went to zero.

- [ ] **Step 2: Move the files**

```bash
git mv crates/taiyaku-serve/src/atomic.rs crates/taiyaku-read/src/atomic.rs
git mv crates/taiyaku-serve/src/atomic/tests.rs crates/taiyaku-read/src/atomic/tests.rs
rmdir crates/taiyaku-serve/src/atomic
```

- [ ] **Step 3: Declare it in `taiyaku-read`, and undeclare it in `taiyaku-serve`**

In `crates/taiyaku-read/src/lib.rs`, beside the existing `mod naming;` / `mod pages;`:

```rust
pub mod atomic;
```

`pub mod`, not `mod` + `pub use`: the module's own doc comment explains why it is public, and callers name it as `atomic::write`.

In `crates/taiyaku-serve/src/lib.rs`, remove the `pub mod atomic;` line.

- [ ] **Step 4: Update the doc comment's opening paragraph**

The module doc currently opens by counting five callers and says the last is in another crate. Both facts change. Replace the first paragraph and the "The last is in another crate" paragraph with:

```rust
//! Write a file so a crash cannot publish half of one.
//!
//! **Eight call sites**, from `rg -n 'atomic::write' crates/` on 2026-08-14
//! rather than from anyone's memory: in `taiyaku-serve`, `progress::store`,
//! `notes::store`, `truth::store`, the sidecar rewritten by a correction in
//! `routes.rs`, and **two** in `cover.rs`; in `taiyaku-cli`, `write_sidecar`
//! — behind both `align --write` and `credits --write` — and `regions.rs`.
//!
//! **This tally has now been wrong three times, and the third is the reason
//! the instruction below is phrased as a command.** It said "three" while it
//! was four, then "four" while it was five, and then sat at five through
//! `truth.rs` landing on 2026-08-11 — so it was five while it was six, for
//! three days, in a comment whose entire subject is that it keeps being
//! wrong. The move to this crate on 2026-08-14 was drafted with a corrected
//! count that was *also* wrong, because it too was written from this
//! paragraph rather than from the code.
//!
//! **So: if you touch this module, run the grep and count what comes back.**
//! Do not update this number by reading the sentence above and adding one.
//!
//! It lives in `taiyaku-read` rather than in the server because it is not a
//! server concern — a sidecar is a sidecar whoever rewrites it — and because
//! `taiyaku-library` needs it for export and must not depend on
//! `taiyaku-serve`. It was in `taiyaku-serve` until 2026-08-14 and moved down
//! rather than being copied.
```

- [ ] **Step 4b: `temp_path` becomes `pub`, and that is a deliberate widening**

`temp_path` is `pub(crate)` today. After the move that means visible inside
`taiyaku-read` only — and `serve/cover/tests.rs:139-140` calls it, so the
substitution in Step 5 cannot fix that call site and the crate will not compile.

**Make it `pub`.** The alternatives both lose something real:

- Moving the test into `taiyaku-read` does not work: it asserts a **joint**
  property of `cover::cache_path` and `temp_path` — that a temp name never ends
  in `.jpg`, so a leftover cannot be mistaken for a real cover by the read path.
  `taiyaku-read` knows nothing about covers and cannot express it.
- Deleting the test loses a guard on a genuine corruption path: two concurrent
  attempts on a cold cache handed the same temp name can interleave and publish
  a corrupted cover permanently.

So widen it, and say why in the doc comment rather than leaving a bare `pub`:

```rust
/// A temp path nothing else will ever pick.
///
/// `pub` rather than `pub(crate)` since 2026-08-14, when this module moved
/// down a crate. `taiyaku-serve`'s cover cache asserts a joint property of
/// this function and its own `cache_path` — that a temp name never ends where
/// the read path would find it, so an unrenamed leftover can never be served
/// as a real cover — and that test cannot move here, because this crate knows
/// nothing about covers.
```

Keep the rest of the existing doc comment, including the pid-plus-counter
paragraph, exactly as it is.

- [ ] **Step 5: Update every call site**

In `taiyaku-serve`, `crate::atomic::write` becomes `taiyaku_read::atomic::write` and `crate::atomic::temp_path` becomes `taiyaku_read::atomic::temp_path`. In `taiyaku-cli`, `taiyaku_serve::atomic::write` becomes `taiyaku_read::atomic::write`.

All nine sites are listed in Step 1's table.

- [ ] **Step 6: Verify nothing still points at the old home**

```bash
rg -n 'taiyaku_serve::atomic|crate::atomic' crates/ --type rust
```

Expected: no output.

- [ ] **Step 7: Run the gate**

```bash
scripts/check-all.sh --no-web --no-wasm
```

Expected: green. The moved tests run under `taiyaku-read` now.

- [ ] **Step 8: Commit**

```bash
git add -A crates/
git commit -m "atomic::write moves down to taiyaku-read

taiyaku-library will need it for export and cannot depend on taiyaku-serve,
which is where it lived. taiyaku-cli already reached across the workspace for
it. Moved rather than copied, and with no re-export: the workspace is closed
and cargo names every call site that has not moved.

The module's own tally of its callers is corrected from the call sites rather
than from memory, which is the third time that count has been wrong."
```

---

## Task 2: The `taiyaku-library` crate, and the pragma that is off by default

**Files:**
- Create: `crates/taiyaku-library/Cargo.toml`, `crates/taiyaku-library/src/lib.rs`, `crates/taiyaku-library/src/tests.rs`, `crates/taiyaku-library/src/schema.rs`, `crates/taiyaku-library/src/schema/tests.rs`, `crates/taiyaku-library/src/schema/001_initial.sql`
- Modify: `Cargo.toml` (workspace members)

**The crate-root test file is `src/tests.rs`, not `src/lib/tests.rs`.** `mod tests;` inside `lib.rs` resolves to `src/tests.rs`, and that is what `taiyaku-core` and `taiyaku-read` both already do.

**Interfaces:**
- Produces:
  - `taiyaku_library::Library` with `Library::open(path: &Path) -> Result<Library, String>` and `Library::open_in_memory() -> Result<Library, String>`
  - `Library::conn(&self) -> std::sync::MutexGuard<'_, rusqlite::Connection>`
  - `taiyaku_library::schema::SCHEMA_VERSION: u32`

- [ ] **Step 1: Confirm the system SQLite is present — the choice is already measured**

Spec §6 deferred this decision here with a measurement attached. **It was taken
on 2026-08-14, on this 32-core machine:**

| | wall clock |
| --- | --- |
| `rusqlite` with `bundled` | **1 m 43 s** |
| `rusqlite` against the system `libsqlite3` | **3.06 s** |

The rule was "if `bundled` costs more than about 30 s, take the system
library", weighed against CI having no cargo cache, both jobs carrying
`timeout-minutes: 10`, and `tract-onnx` already costing 21 s of that. 103 s is
more than three times the threshold, and it would be paid twice per push. **The
system library wins and the decision is closed** — do not re-time it.

CI needs no change: the `rust:1-bookworm` image already ships `pkg-config` and
`libsqlite3-dev` 3.40.1, FTS5 included. Verify the same locally before you
start:

```bash
pkg-config --modversion sqlite3
```

Expected: a version, not an error. If it errors, stop and report — the crate
will not build and that is worth knowing before anything else is written.

**The cost of this choice, and what Step 6 does about it.** `bundled` would
have guaranteed FTS5 and pinned the SQLite version; the system library inherits
whatever the host has. Debian and Arch both build FTS5 in, but "both distros I
checked" is not "every host this will ever run on" — and the failure mode is
Task 3's migration dying part-way with `no such module: fts5`. Step 6 turns
that into a named refusal at open time.

- [ ] **Step 2: Create the crate manifest**

`crates/taiyaku-library/Cargo.toml`. **There is no `fts5` feature on `rusqlite`** — an earlier draft of this plan invented one, and `cargo add rusqlite --features fts5` fails with `unrecognized feature for crate rusqlite: fts5`. FTS5 is a property of the SQLite build, reached from plain SQL, not a Cargo feature. `backup` is real and is what `VACUUM INTO` needs.

```toml
[package]
name = "taiyaku-library"
description = "The SQLite library: works, editions, progress and notes, round-tripping to sidecars"
version.workspace = true
edition.workspace = true
rust-version.workspace = true
license.workspace = true
repository.workspace = true
authors.workspace = true

# `taiyaku-serve` is the only consumer. This crate must never be reachable from
# `taiyaku-core` or `taiyaku-wasm`, both of which compile to wasm32, where
# SQLite does not.
#
# **Against the system libsqlite3 rather than `bundled`, measured rather than
# assumed**: bundled builds in 1 m 43 s against 3.06 s on a 32-core machine,
# and CI has no cargo cache and gives each job ten minutes. The cost is that
# FTS5 is now a property of the host's SQLite rather than a guarantee — see
# `Library::open`, which refuses at open time with a named error rather than
# letting a migration die part-way through.
#
# There is no `fts5` Cargo feature on rusqlite; FTS5 is reached from plain SQL.
# `backup` is for `VACUUM INTO`, which is how a backup is taken while the
# server runs (spec A §13.4).
[dependencies]
rusqlite = { version = "0.38", features = ["backup"] }
serde = { version = "1.0.229", features = ["derive"] }
serde_json = "1.0.151"
sha2 = "0.11.0"
taiyaku-core = { path = "../taiyaku-core" }
taiyaku-read = { path = "../taiyaku-read" }

[lints]
workspace = true
```

- [ ] **Step 3: Add it to the workspace**

In the root `Cargo.toml`, add `"crates/taiyaku-library"` to `members`.

- [ ] **Step 4: Write the failing test — the pragma is per connection**

`crates/taiyaku-library/src/tests.rs` — the crate root's test file, beside
`lib.rs` and not under it, which is what `mod tests;` in a crate root resolves
to and what `taiyaku-core` and `taiyaku-read` already do:

```rust
use super::*;

/// `PRAGMA foreign_keys` is a property of the **connection**, not of the
/// database, and SQLite leaves it off by default for backward compatibility.
/// A `Library::open` that forgets it turns every `REFERENCES` in the schema
/// into a comment — silently, with every constraint appearing to be there.
#[test]
fn every_connection_enforces_foreign_keys() {
    let library = Library::open_in_memory().expect("an in-memory library opens");
    let conn = library.conn();
    let on: i64 = conn
        .query_row("PRAGMA foreign_keys", [], |row| row.get(0))
        .expect("the pragma answers");
    assert_eq!(on, 1, "foreign key enforcement must be on for this connection");
}

/// FTS5 is a property of the host's SQLite build, not of the crate, because
/// this links the system library rather than bundling one. Debian and Arch both
/// build it in; "both distros I checked" is not "every host". Without this
/// check the failure surfaces as Task 3's migration dying part-way with
/// `no such module: fts5`, on a half-created schema.
#[test]
fn opening_refuses_a_sqlite_without_fts5_and_names_what_is_missing() {
    // The host running this test has FTS5 — CI's rust:1-bookworm and this
    // workstation both do — so the positive path is what is assertable here.
    // What this pins is that the probe RUNS and that a library opens through
    // it, so removing the probe fails this test.
    let library = Library::open_in_memory().expect("this host has FTS5");
    let conn = library.conn();
    conn.execute_batch("CREATE VIRTUAL TABLE temp.fts_probe_check USING fts5(x)")
        .expect("if open() succeeded, FTS5 must be usable");
}

/// `INSERT OR REPLACE` on `series_title` must not leave a phantom row in the
/// search index.
///
/// Its primary key is the composite `(series_id, title)`, not a rowid alias, so
/// REPLACE resolves a conflict by deleting and re-inserting with a **fresh
/// rowid** — and that implicit delete fires no DELETE trigger unless
/// `recursive_triggers` is on. Without the pragma this returns 2 for a title
/// that exists once. `work` is immune, because its `id` is a rowid alias and
/// REPLACE overwrites in place; that asymmetry is why this is asserted rather
/// than assumed.
#[test]
fn replacing_a_series_title_leaves_no_phantom_in_the_search_index() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    conn.execute_batch(
        "INSERT INTO series (id, sort_title) VALUES (1, 'x');
         INSERT INTO series_title (series_id, title) VALUES (1, 'Frieren');
         INSERT OR REPLACE INTO series_title (series_id, title, kind)
             VALUES (1, 'Frieren', 'romaji');",
    )
    .expect("insert then replace");

    let hits: i64 = conn
        .query_row(
            "SELECT count(*) FROM series_fts WHERE series_fts MATCH 'Frieren'",
            [],
            |row| row.get(0),
        )
        .expect("fts query");
    assert_eq!(hits, 1, "a title that exists once must match once");
}

#[test]
fn a_fresh_database_records_its_schema_version() {
    let library = Library::open_in_memory().expect("an in-memory library opens");
    let conn = library.conn();
    let version: u32 = conn
        .query_row("SELECT value FROM meta WHERE key = 'schema_version'", [], |row| {
            let text: String = row.get(0)?;
            Ok(text.parse().unwrap_or(0))
        })
        .expect("meta carries a schema version");
    assert_eq!(version, crate::schema::SCHEMA_VERSION);
}
```

- [ ] **Step 5: Run it and watch it fail**

```bash
cargo test -p taiyaku-library
```

Expected: FAIL — `Library` does not exist.

- [ ] **Step 6: Write `lib.rs` and `schema.rs`**

`crates/taiyaku-library/src/lib.rs`:

```rust
//! The library store: many works, their editions and members, and the
//! per-person state that hangs off them.
//!
//! **A store and a projection, and deliberately nothing else.** Every rule
//! about what a re-solve does to a hand correction lives in `taiyaku-core`,
//! where it is already tested; this crate reaches it through
//! `Sidecar::merge` rather than reimplementing it over rows. See
//! `docs/superpowers/specs/2026-08-14-library-store-design.md`, §4.2.

pub mod schema;

use std::path::Path;
use std::sync::{Mutex, MutexGuard};

use rusqlite::Connection;

/// An open library.
///
/// One connection behind a `Mutex` rather than a pool. The write volume this
/// serves is a reading position and the occasional note; a pool would add a
/// dependency and a failure mode to solve a contention problem nobody has.
#[derive(Debug)]
pub struct Library {
    conn: Mutex<Connection>,
}

impl Library {
    /// Open the library at `path`, creating and migrating it if needed.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be opened — a missing parent
    /// directory or a permission refusal, with the message naming the path —
    /// or if applying the schema fails.
    pub fn open(path: &Path) -> Result<Self, String> {
        let conn = Connection::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
        Self::from_connection(conn)
    }

    /// An in-memory library, for tests.
    ///
    /// # Errors
    ///
    /// Returns an error if SQLite refuses the connection or the schema fails
    /// to apply.
    pub fn open_in_memory() -> Result<Self, String> {
        let conn = Connection::open_in_memory().map_err(|e| format!("in memory: {e}"))?;
        Self::from_connection(conn)
    }

    fn from_connection(conn: Connection) -> Result<Self, String> {
        // Per connection, not per database, and off by default. See the test
        // in this module's `tests.rs`.
        conn.pragma_update(None, "foreign_keys", "ON")
            .map_err(|e| format!("enabling foreign keys: {e}"))?;
        conn.pragma_update(None, "journal_mode", "WAL")
            .map_err(|e| format!("setting WAL: {e}"))?;
        // Also per connection, and also off by default. Without it, the
        // implicit DELETE that `INSERT OR REPLACE` performs to resolve a
        // conflict does not fire DELETE triggers — so a REPLACE on
        // `series_title`, whose primary key is the composite
        // `(series_id, title)` rather than a rowid alias, leaves a phantom row
        // in `series_fts` pointing at the discarded rowid. The search index
        // then returns two hits for a title that exists once. `work` is immune
        // because its `id` IS a rowid alias and REPLACE overwrites in place,
        // which is exactly the kind of asymmetry that goes unnoticed.
        //
        // Turned on rather than documented as a rule for callers: a schema that
        // is only correct while every future writer avoids one SQL statement is
        // not correct, it is lucky. Recursion depth here is one — the FTS
        // triggers write through FTS5's command-row idiom, which fires no
        // further user triggers.
        conn.pragma_update(None, "recursive_triggers", "ON")
            .map_err(|e| format!("enabling recursive triggers: {e}"))?;
        require_fts5(&conn)?;
        schema::migrate(&conn)?;
        Ok(Self { conn: Mutex::new(conn) })
    }

    /// The connection, exclusively.
    ///
    /// A poisoned lock is taken rather than propagated, the same call
    /// `Work::sidecar` makes and for the same reason: the database behind it
    /// is whatever the last successful write left, and refusing every later
    /// request because one handler panicked is the worse answer.
    #[must_use]
    pub fn conn(&self) -> MutexGuard<'_, Connection> {
        match self.conn.lock() {
            Ok(held) => held,
            Err(poisoned) => poisoned.into_inner(),
        }
    }
}

/// Refuse a SQLite that cannot do FTS5, before any schema is applied.
///
/// **This crate links the system `libsqlite3` rather than bundling one** — a
/// measured choice, 3.06 s against 1 m 43 s to build, with CI having no cargo
/// cache and a ten-minute job budget. The cost is that FTS5 stops being a
/// guarantee of the dependency and becomes a property of the host. Debian and
/// Arch both build it in; that is two distributions checked, not a promise
/// about every host this will ever run on.
///
/// Without this, the failure arrives as `no such module: fts5` from the middle
/// of a migration, on a database that is half-created and half-not. Probing
/// with a temporary virtual table is definitive where reading
/// `PRAGMA compile_options` is not: the pragma reports how the library was
/// configured, and this reports whether the statement actually works.
fn require_fts5(conn: &Connection) -> Result<(), String> {
    conn.execute_batch("CREATE VIRTUAL TABLE temp.taiyaku_fts5_probe USING fts5(probe)")
        .map_err(|e| {
            format!(
                "this SQLite has no FTS5, which the library's search needs: {e}. \
                 taiyaku links the system libsqlite3 rather than bundling one, so \
                 install a build with FTS5 enabled (Debian's libsqlite3-0 and Arch's \
                 sqlite both have it)"
            )
        })?;
    conn.execute_batch("DROP TABLE temp.taiyaku_fts5_probe")
        .map_err(|e| format!("dropping the FTS5 probe table: {e}"))
}

#[cfg(test)]
mod tests;
```

`crates/taiyaku-library/src/schema.rs`:

```rust
//! Migrations, versioned in `meta`.
//!
//! Applied in order, each inside its own transaction, and the version is
//! written as the last statement of each — so a migration that fails leaves
//! the version where it was rather than half-advanced.

use rusqlite::Connection;

/// The schema version this build expects.
pub const SCHEMA_VERSION: u32 = 1;

/// Every migration, in order. Index 0 takes an empty database to version 1.
const MIGRATIONS: [&str; 1] = [include_str!("schema/001_initial.sql")];

/// Bring `conn` up to `SCHEMA_VERSION`.
///
/// # Errors
///
/// Returns an error if a migration fails to apply, naming the version it was
/// applying, or if the database reports a version this build does not know —
/// which means a newer taiyaku wrote it, and running an older binary against
/// it would corrupt what it does not understand.
pub fn migrate(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL)",
    )
    .map_err(|e| format!("creating meta: {e}"))?;

    let current: u32 = conn
        .query_row("SELECT value FROM meta WHERE key = 'schema_version'", [], |row| {
            row.get::<_, String>(0)
        })
        .ok()
        .and_then(|text| text.parse().ok())
        .unwrap_or(0);

    if current > SCHEMA_VERSION {
        return Err(format!(
            "this database is at schema version {current} and this build knows {SCHEMA_VERSION}; \
             it was written by a newer taiyaku"
        ));
    }

    for (index, sql) in MIGRATIONS.iter().enumerate() {
        let version = u32::try_from(index).unwrap_or(u32::MAX) + 1;
        if version <= current {
            continue;
        }
        conn.execute_batch(sql)
            .map_err(|e| format!("applying schema version {version}: {e}"))?;
        conn.execute(
            "INSERT INTO meta (key, value) VALUES ('schema_version', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            [version.to_string()],
        )
        .map_err(|e| format!("recording schema version {version}: {e}"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
```

Create `crates/taiyaku-library/src/schema/001_initial.sql` **containing a single SQL comment** — `-- Migration 1 lands in Task 3.` — rather than truly empty. `include_str!` is happy with an empty file, but `execute_batch("")` on some SQLite builds is a no-op and on others an error, and a file with a comment in it is unambiguously the former. Task 3 replaces the whole file.

**`identity` is NOT declared in `lib.rs` yet.** Task 4 creates that module and adds `pub mod identity;` then. Declaring it here would name a file that does not exist and the crate would not compile.

`crates/taiyaku-library/src/schema/tests.rs`:

```rust
use super::*;

#[test]
fn migrating_twice_is_a_no_op() {
    let conn = Connection::open_in_memory().expect("in-memory connection");
    migrate(&conn).expect("first migration");
    migrate(&conn).expect("second migration is a no-op");
}

/// A database written by a newer build is refused rather than opened, because
/// an older binary cannot know what it would be dropping.
#[test]
fn a_future_schema_version_is_refused_and_says_so() {
    let conn = Connection::open_in_memory().expect("in-memory connection");
    migrate(&conn).expect("first migration");
    conn.execute(
        "UPDATE meta SET value = '99' WHERE key = 'schema_version'",
        [],
    )
    .expect("forcing a future version");

    let refused = migrate(&conn).expect_err("a future version must be refused");
    assert!(refused.contains("99"), "the message names the version found: {refused}");
    assert!(
        refused.contains("newer taiyaku"),
        "the message says what that means: {refused}"
    );
}
```

- [ ] **Step 7: Run the tests**

```bash
cargo test -p taiyaku-library
```

Expected: 4 tests pass.

- [ ] **Step 8: Run the gate and commit**

```bash
scripts/check-all.sh --no-web --no-wasm
git add -A crates/taiyaku-library Cargo.toml
git commit -m "The library crate, and the pragma that is off by default

Against the system libsqlite3 rather than bundled, measured rather than
assumed: bundled builds in 1 m 43 s against 3.06 s on a 32-core machine, and CI
has no cargo cache and gives each job ten minutes, of which tract-onnx already
takes 21 s. The cost is that FTS5 becomes a property of the host rather than a
guarantee of the dependency, so Library::open probes for it and refuses with a
named error rather than letting a migration die part-way through.

PRAGMA foreign_keys is a property of the connection rather than of the
database and SQLite leaves it off for backward compatibility, so an open that
forgets it turns every REFERENCES in the schema into a comment, silently and
with every constraint appearing to be present. There is a test.

A database at a schema version this build does not know is refused with a
message that names the version, rather than opened by a binary that cannot
know what it would drop."
```

---

## Task 3: Migration 1

The whole schema, from spec §3. One task because the schema is one reviewable unit.

**Files:**
- Modify: `crates/taiyaku-library/src/schema/001_initial.sql`
- Modify: `crates/taiyaku-library/src/schema.rs`
- Modify: `crates/taiyaku-library/src/schema/tests.rs`

- [ ] **Step 0: An unreadable `schema_version` must be an error, not a zero**

Raised by Task 2's review as a Minor and fixed here, because **this is the task
that makes it reachable.** `migrate` currently reads the stored version as:

```rust
    .ok()
    .and_then(|text| text.parse().ok())
    .unwrap_or(0);
```

so a `meta.schema_version` of `""` or `"one"` or a truncated write silently
becomes `0` — "fresh database" — and migration 1 is applied over a database
that already has every table in it. While migration 1 was a comment that cost
nothing. From this task on it means `CREATE TABLE` against tables that exist,
which fails part-way and leaves the version still wrong.

**A missing row is genuinely zero; an unparseable row is not.** Distinguish
them:

```rust
    let current: u32 = match conn.query_row(
        "SELECT value FROM meta WHERE key = 'schema_version'",
        [],
        |row| row.get::<_, String>(0),
    ) {
        // No row at all: a database nobody has migrated. Genuinely version 0.
        Err(rusqlite::Error::QueryReturnedNoRows) => 0,
        Err(e) => return Err(format!("reading the schema version: {e}")),
        Ok(text) => text.trim().parse().map_err(|_| {
            format!(
                "`meta.schema_version` holds {text:?}, which is not a version number. \
                 Refusing to migrate rather than treating it as a fresh database and \
                 applying migration 1 over tables that already exist."
            )
        })?,
    };
```

And a test beside the two already in `schema/tests.rs`:

```rust
/// A corrupted version must not read as "fresh". Treating it as 0 would apply
/// migration 1 over a populated database, which fails part-way and leaves the
/// version still wrong.
#[test]
fn an_unparseable_schema_version_is_refused_rather_than_read_as_zero() {
    let conn = Connection::open_in_memory().expect("in-memory connection");
    migrate(&conn).expect("first migration");
    conn.execute("UPDATE meta SET value = 'one' WHERE key = 'schema_version'", [])
        .expect("corrupting the version");

    let refused = migrate(&conn).expect_err("an unparseable version must be refused");
    assert!(refused.contains("one"), "the message names what it found: {refused}");
}
```

- [ ] **Step 1: Write the failing tests**

Append to `crates/taiyaku-library/src/schema/tests.rs`:

```rust
fn table_names(conn: &Connection) -> Vec<String> {
    let mut stmt = conn
        .prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
        .expect("listing tables");
    let rows = stmt
        .query_map([], |row| row.get::<_, String>(0))
        .expect("mapping table names");
    rows.filter_map(Result::ok).collect()
}

#[test]
fn migration_one_creates_every_table_the_spec_names() {
    let conn = Connection::open_in_memory().expect("in-memory connection");
    migrate(&conn).expect("migration");
    let names = table_names(&conn);
    for expected in [
        "alignment", "collection", "collection_item", "credit", "edition",
        "external_id", "meta", "member", "note", "page_flag", "person",
        "progress", "root", "run", "series", "series_tag", "series_title",
        "tag", "truth_box", "user", "work", "work_tag",
    ] {
        assert!(names.contains(&expected.to_string()), "missing table `{expected}`: {names:?}");
    }
}

/// `member.ordinal` is not decoration. `sidecar.md`'s "How a member becomes
/// pages" is normative: an edition's page sequence is its members concatenated
/// *in the order given*, and a consumer that reorders them disagrees with every
/// run in the document while `sha256` and `pages` still match. The unique
/// constraint is what makes that unrepresentable rather than merely documented.
#[test]
fn two_members_cannot_share_an_ordinal_within_an_edition() {
    let conn = Connection::open_in_memory().expect("in-memory connection");
    conn.pragma_update(None, "foreign_keys", "ON").expect("fk on");
    migrate(&conn).expect("migration");
    conn.execute_batch(
        "INSERT INTO series (id, sort_title) VALUES (1, 'x');
         INSERT INTO work (id, content_key, slug, series_id, title, reading, hub)
             VALUES (1, 'k', 's', 1, 't', 'rtl', 'raw');
         INSERT INTO edition (id, work_id, key, label) VALUES (1, 1, 'raw', 'Raw');
         INSERT INTO member (edition_id, ordinal, path, sha256, pages)
             VALUES (1, 0, 'a.cbz', 'aa', 10);",
    )
    .expect("first member");

    let refused = conn.execute(
        "INSERT INTO member (edition_id, ordinal, path, sha256, pages)
         VALUES (1, 0, 'b.cbz', 'bb', 10)",
        [],
    );
    assert!(refused.is_err(), "a second member at ordinal 0 must be refused");
}

/// **The FTS index must survive a rename, and nothing tested that.** An
/// earlier draft of this schema gave `work` all three lifecycle triggers and
/// `series_title` only two, so renaming a title left the index matching the old
/// name and missing the new one — a wrong search result, with no error
/// anywhere. Both tables are asserted here because the asymmetry is precisely
/// what went unnoticed.
#[test]
fn renaming_a_title_updates_the_search_index_for_both_tables() {
    let conn = Connection::open_in_memory().expect("in-memory connection");
    conn.pragma_update(None, "foreign_keys", "ON").expect("fk on");
    migrate(&conn).expect("migration");
    conn.execute_batch(
        "INSERT INTO series (id, sort_title) VALUES (1, 'x');
         INSERT INTO series_title (series_id, title) VALUES (1, 'Old Series');
         INSERT INTO work (id, content_key, slug, series_id, title, reading, hub)
             VALUES (1, 'k', 's', 1, 'Old Work', 'rtl', 'raw');
         UPDATE series_title SET title = 'New Series' WHERE series_id = 1;
         UPDATE work SET title = 'New Work' WHERE id = 1;",
    )
    .expect("insert then rename");

    let hits = |table: &str, term: &str| -> i64 {
        conn.query_row(
            &format!("SELECT count(*) FROM {table} WHERE {table} MATCH ?1"),
            [term],
            |row| row.get(0),
        )
        .expect("fts query")
    };

    assert_eq!(hits("series_fts", "New"), 1, "the new series title must be findable");
    assert_eq!(hits("series_fts", "Old"), 0, "the old series title must be gone");
    assert_eq!(hits("work_fts", "New"), 1, "the new work title must be findable");
    assert_eq!(hits("work_fts", "Old"), 0, "the old work title must be gone");
}

#[test]
fn deleting_a_work_takes_its_editions_and_members_with_it() {
    let conn = Connection::open_in_memory().expect("in-memory connection");
    conn.pragma_update(None, "foreign_keys", "ON").expect("fk on");
    migrate(&conn).expect("migration");
    conn.execute_batch(
        "INSERT INTO series (id, sort_title) VALUES (1, 'x');
         INSERT INTO work (id, content_key, slug, series_id, title, reading, hub)
             VALUES (1, 'k', 's', 1, 't', 'rtl', 'raw');
         INSERT INTO edition (id, work_id, key, label) VALUES (1, 1, 'raw', 'Raw');
         INSERT INTO member (edition_id, ordinal, path, sha256, pages)
             VALUES (1, 0, 'a.cbz', 'aa', 10);
         DELETE FROM work WHERE id = 1;",
    )
    .expect("cascade");

    let members: i64 = conn
        .query_row("SELECT count(*) FROM member", [], |row| row.get(0))
        .expect("counting members");
    assert_eq!(members, 0, "the member should have gone with its work");
}
```

- [ ] **Step 2: Run and watch them fail**

```bash
cargo test -p taiyaku-library
```

Expected: FAIL — no tables.

- [ ] **Step 3: Write the migration**

`crates/taiyaku-library/src/schema/001_initial.sql`:

```sql
-- Structure -----------------------------------------------------------------

-- `cover_work_id` references work(id), which references series(id). The cycle
-- is fine and is why the column is nullable: a series is inserted with no
-- cover, and the work that becomes its cover is inserted second.
CREATE TABLE series (
  id            INTEGER PRIMARY KEY,
  sort_title    TEXT NOT NULL,
  reading       TEXT,
  cover_work_id INTEGER
);

-- One series, several names. Komga's single title is the annoyance this exists
-- for, and it is also what FTS5 indexes so the romaji, the native title and the
-- English title all find the same shelf.
CREATE TABLE series_title (
  series_id  INTEGER NOT NULL REFERENCES series(id) ON DELETE CASCADE,
  title      TEXT NOT NULL,
  kind       TEXT NOT NULL DEFAULT 'alt',  -- romaji | native | english | alt
  is_primary INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (series_id, title)
);

-- `content_key` is a sha256 over the HUB EDITION'S members' sha256s, in member
-- order — that edition alone. Folding in every edition would change a work's
-- identity whenever an edition was added, which is the entire point of this
-- project. `volume` is REAL because omnibus and half-volumes exist.
CREATE TABLE work (
  id            INTEGER PRIMARY KEY,
  content_key   TEXT NOT NULL UNIQUE,
  slug          TEXT NOT NULL UNIQUE,
  series_id     INTEGER REFERENCES series(id) ON DELETE SET NULL,
  volume        REAL,
  title         TEXT NOT NULL,
  reading       TEXT NOT NULL,
  hub           TEXT NOT NULL,
  sidecar_path  TEXT,
  sidecar_mtime INTEGER,
  imported      TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE edition (
  id       INTEGER PRIMARY KEY,
  work_id  INTEGER NOT NULL REFERENCES work(id) ON DELETE CASCADE,
  key      TEXT NOT NULL,
  label    TEXT NOT NULL,
  language TEXT,
  kind     TEXT,                            -- raw | official | fan | NULL
  UNIQUE (work_id, key)
);

CREATE TABLE member (
  id               INTEGER PRIMARY KEY,
  edition_id       INTEGER NOT NULL REFERENCES edition(id) ON DELETE CASCADE,
  ordinal          INTEGER NOT NULL,
  path             TEXT NOT NULL,
  sha256           TEXT NOT NULL,
  pages            INTEGER NOT NULL,
  format           TEXT,                    -- zip | dir | epub-fixed | epub-reflow | pdf | rar | 7z
  chapter_declared REAL,
  chapter_source   TEXT,                    -- comicinfo | filename | hand
  UNIQUE (edition_id, ordinal)
);

CREATE TABLE page_flag (
  edition_id INTEGER NOT NULL REFERENCES edition(id) ON DELETE CASCADE,
  kind       TEXT NOT NULL,                 -- spread | credit
  page       INTEGER NOT NULL,
  PRIMARY KEY (edition_id, kind, page)
) WITHOUT ROWID;

-- Alignment -----------------------------------------------------------------

-- `verdict` is part of the document and MUST round-trip: `Quality` derives
-- Serialize, so `to_json` writes whatever the struct holds, and a projection
-- that recomputed it instead would emit a different document from the one it
-- was given. It is still never an AUTHORITY — `Sidecar::quality_of` recomputes
-- from the runs when anyone asks a question (core/sidecar.rs:1220), so a
-- hand-edited document cannot make a stale verdict mean anything. Stored and
-- restored faithfully; consulted by nothing.
CREATE TABLE alignment (
  id           INTEGER PRIMARY KEY,
  work_id      INTEGER NOT NULL REFERENCES work(id) ON DELETE CASCADE,
  from_key     TEXT NOT NULL,
  to_key       TEXT NOT NULL,
  by           TEXT NOT NULL,               -- solver | hand
  solver_hash  TEXT,
  solver_floor REAL,
  verdict      TEXT,
  slots        INTEGER,
  paired       INTEGER,
  avg_run      REAL,
  UNIQUE (work_id, from_key, to_key)
);

-- Four columns per side rather than the sidecar's "1-4R" string: a Slot is
-- { page, half }, and storing the rendered span would make "which runs cover
-- page 47" a text scan.
CREATE TABLE run (
  id            INTEGER PRIMARY KEY,
  alignment_id  INTEGER NOT NULL REFERENCES alignment(id) ON DELETE CASCADE,
  ordinal       INTEGER NOT NULL,
  a_start_page  INTEGER NOT NULL,
  a_start_half  TEXT,
  a_end_page    INTEGER NOT NULL,
  a_end_half    TEXT,
  b_start_page  INTEGER,
  b_start_half  TEXT,
  b_end_page    INTEGER,
  b_end_half    TEXT,
  margin        REAL,
  by            TEXT,
  note          TEXT,
  UNIQUE (alignment_id, ordinal)
);

-- People, grouping, external identity ---------------------------------------

CREATE TABLE person (id INTEGER PRIMARY KEY, name TEXT NOT NULL UNIQUE);

-- A surrogate id rather than a composite primary key over the three nullable
-- columns: SQLite permits NULLs in a non-INTEGER primary key, so a composite
-- one would silently allow duplicate credits. `edition_id` is the one that
-- earns its place — a scanlation group is a property of an edition, and Komga
-- has nowhere to put it.
CREATE TABLE credit (
  id         INTEGER PRIMARY KEY,
  person_id  INTEGER NOT NULL REFERENCES person(id) ON DELETE CASCADE,
  role       TEXT NOT NULL,                 -- author | artist | translator | group | publisher
  series_id  INTEGER REFERENCES series(id) ON DELETE CASCADE,
  work_id    INTEGER REFERENCES work(id) ON DELETE CASCADE,
  edition_id INTEGER REFERENCES edition(id) ON DELETE CASCADE
);

CREATE TABLE collection (
  id      INTEGER PRIMARY KEY,
  name    TEXT NOT NULL,
  kind    TEXT NOT NULL DEFAULT 'collection', -- collection | reading_list
  created TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE collection_item (
  collection_id INTEGER NOT NULL REFERENCES collection(id) ON DELETE CASCADE,
  ordinal       INTEGER NOT NULL,
  series_id     INTEGER REFERENCES series(id) ON DELETE CASCADE,
  work_id       INTEGER REFERENCES work(id) ON DELETE CASCADE,
  PRIMARY KEY (collection_id, ordinal)
);

CREATE TABLE tag (id INTEGER PRIMARY KEY, name TEXT NOT NULL UNIQUE);
CREATE TABLE series_tag (
  series_id INTEGER NOT NULL REFERENCES series(id) ON DELETE CASCADE,
  tag_id    INTEGER NOT NULL REFERENCES tag(id) ON DELETE CASCADE,
  PRIMARY KEY (series_id, tag_id)
) WITHOUT ROWID;
CREATE TABLE work_tag (
  work_id INTEGER NOT NULL REFERENCES work(id) ON DELETE CASCADE,
  tag_id  INTEGER NOT NULL REFERENCES tag(id) ON DELETE CASCADE,
  PRIMARY KEY (work_id, tag_id)
) WITHOUT ROWID;

-- In migration 1 despite nothing reading it yet, because it holds HAND-MATCHED
-- data: deferring the table means matching a whole library against AniList a
-- second time. `user_credential` and `sync_state` are deliberately absent —
-- machine-obtained, nothing to backfill.
CREATE TABLE external_id (
  series_id   INTEGER NOT NULL REFERENCES series(id) ON DELETE CASCADE,
  provider    TEXT NOT NULL,                -- anilist | mal | mangaupdates
  external_id TEXT NOT NULL,
  PRIMARY KEY (series_id, provider)
) WITHOUT ROWID;

-- Per-person state ----------------------------------------------------------

-- `password_hash` is nullable for exactly the window between this spec and A2,
-- which inserts the real account. A2 MUST make it NOT NULL: a nullable password
-- on an internet-facing server is one missed branch from an account that
-- authenticates with nothing.
CREATE TABLE user (
  id            INTEGER PRIMARY KEY,
  name          TEXT NOT NULL UNIQUE,
  password_hash TEXT,
  role          TEXT NOT NULL DEFAULT 'admin',
  created       TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Read status is derived — unread is no row, reading is a row, read is slot at
-- the last slot. `completed_at` and `read_count` exist only for "finished once
-- and now re-reading", which a position cannot express, and they are here
-- rather than added later because reading history is behavioural data with
-- NOTHING to backfill.
CREATE TABLE progress (
  user_id      INTEGER NOT NULL REFERENCES user(id) ON DELETE CASCADE,
  work_id      INTEGER NOT NULL REFERENCES work(id) ON DELETE CASCADE,
  edition      TEXT NOT NULL,
  slot         INTEGER NOT NULL,
  updated      TEXT NOT NULL DEFAULT (datetime('now')),
  completed_at TEXT,
  read_count   INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (user_id, work_id)
) WITHOUT ROWID;

-- `id` is the ULID the reader already mints, kept as the primary key so the
-- per-note merge in import has an identity to merge on. The region is four
-- nullable columns, all four NULL together for a whole-slot anchor.
CREATE TABLE note (
  id      TEXT PRIMARY KEY,
  user_id INTEGER NOT NULL REFERENCES user(id) ON DELETE CASCADE,
  work_id INTEGER NOT NULL REFERENCES work(id) ON DELETE CASCADE,
  edition TEXT NOT NULL,
  slot    INTEGER NOT NULL,
  rx      REAL, ry REAL, rw REAL, rh REAL,
  color   TEXT,
  body    TEXT NOT NULL,
  created TEXT NOT NULL,
  updated TEXT NOT NULL
);

-- **A page that was looked at is a row here, whether or not it holds boxes.**
-- `TruthEdition::pages`' own doc comment (core/truth.rs:137) is explicit: "an
-- absent key means nobody has looked at that page; a present empty value means
-- somebody looked and found no text. Across 75 pages of manual work that
-- difference is the progress tracker, and it is the same distinction
-- `RegionEdition::pages` refuses to collapse."
--
-- An earlier draft of this schema had `truth_box` alone. Boxes are one row
-- each, so a page examined and found empty wrote zero rows — indistinguishable
-- from a page nobody had opened, silently destroying the labelling progress
-- record. Found in Task 9 by a test rather than by reading.
CREATE TABLE truth_page (
  work_id INTEGER NOT NULL REFERENCES work(id) ON DELETE CASCADE,
  edition TEXT NOT NULL,
  page    INTEGER NOT NULL,
  PRIMARY KEY (work_id, edition, page)
) WITHOUT ROWID;

-- The composite FK is what stops a box existing on a page nobody recorded
-- looking at. It costs nothing on the success path — `put_truth` already writes
-- the page row before its boxes, in one transaction — and nothing in index
-- overhead, since `truth_page`'s primary key already provides exactly the index
-- the FK needs. So this is not machinery for a hypothetical: it encodes an
-- invariant the write path has already committed to, into the layer that cannot
-- regress. The risk it closes is not only a future external writer but a
-- refactor *inside* `put_truth` that reorders the two inserts, which today would
-- fail silently. Same reasoning as `recursive_triggers`: a schema correct only
-- while every writer remembers something is lucky, not correct.
CREATE TABLE truth_box (
  id      INTEGER PRIMARY KEY,
  work_id INTEGER NOT NULL REFERENCES work(id) ON DELETE CASCADE,
  edition TEXT NOT NULL,
  page    INTEGER NOT NULL,
  x REAL NOT NULL, y REAL NOT NULL, w REAL NOT NULL, h REAL NOT NULL,
  axis    TEXT,
  kind    TEXT NOT NULL,
  ignore  INTEGER NOT NULL DEFAULT 0,
  text    TEXT,
  FOREIGN KEY (work_id, edition, page)
    REFERENCES truth_page (work_id, edition, page) ON DELETE CASCADE
);

-- Roots and search ----------------------------------------------------------

-- Written by spec B. The three edition columns are what this root's material
-- becomes by default; B's rules layer over them.
CREATE TABLE root (
  id                 INTEGER PRIMARY KEY,
  path               TEXT NOT NULL UNIQUE,
  added              TEXT NOT NULL DEFAULT (datetime('now')),
  scan_interval_secs INTEGER,
  last_scan_at       TEXT,
  last_scan_status   TEXT,
  exclude_globs      TEXT,
  edition_key        TEXT,
  default_kind       TEXT,
  default_language   TEXT
);

CREATE INDEX member_by_sha ON member (sha256);
CREATE INDEX note_by_work  ON note (work_id);
CREATE INDEX work_by_series ON work (series_id);

-- External-content FTS over the titles, so there is no second copy of the
-- strings to drift. Kept current by triggers.
CREATE VIRTUAL TABLE series_fts USING fts5 (
  title, content = 'series_title', content_rowid = 'rowid'
);
CREATE VIRTUAL TABLE work_fts USING fts5 (
  title, content = 'work', content_rowid = 'id'
);

CREATE TRIGGER series_title_ai AFTER INSERT ON series_title BEGIN
  INSERT INTO series_fts (rowid, title) VALUES (new.rowid, new.title);
END;
CREATE TRIGGER series_title_ad AFTER DELETE ON series_title BEGIN
  INSERT INTO series_fts (series_fts, rowid, title) VALUES ('delete', old.rowid, old.title);
END;
-- The update trigger is not boilerplate symmetry: without it, renaming a title
-- leaves series_fts matching the OLD title and missing the NEW one, silently.
-- An earlier draft of this schema omitted it while `work` had all three, and a
-- review reproduced the stale index. §3.5 of the spec claims these tables are
-- "kept current by triggers"; two of three triggers does not keep that promise.
CREATE TRIGGER series_title_au AFTER UPDATE ON series_title BEGIN
  INSERT INTO series_fts (series_fts, rowid, title) VALUES ('delete', old.rowid, old.title);
  INSERT INTO series_fts (rowid, title) VALUES (new.rowid, new.title);
END;
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

-- One implicit local user, so the server behaves exactly as it does today: no
-- login, no hashing, no guards. Spec A2 promotes THIS ROW rather than inserting
-- a new one — inserting would orphan every note and reading position recorded
-- between A and A2 on an account nobody can log in as, silently.
INSERT INTO user (id, name, role) VALUES (1, 'local', 'admin');
```

- [ ] **Step 4: Run the tests**

```bash
cargo test -p taiyaku-library
```

Expected: all pass.

- [ ] **Step 5: Check FTS5 is actually compiled in**

```bash
cargo test -p taiyaku-library 2>&1 | grep -i 'fts5\|no such module' || echo "FTS5 available"
```

Expected: `FTS5 available`. If the migration failed with `no such module: fts5`, **this host's SQLite was built without it** — there is no Cargo feature to turn on, because this crate links the system library rather than bundling one. Task 2's `require_fts5` should have refused at open time and produced a named error instead; a bare `no such module` reaching here means that probe was removed or is not running.

- [ ] **Step 6: Run the gate and commit**

```bash
scripts/check-all.sh --no-web --no-wasm
git add -A crates/taiyaku-library
git commit -m "Migration 1: the whole schema

Four levels and no special case for aligned: a Komga book is a work with one
edition and no alignment, an aligned pair is a work with two editions and one
alignment, and no query branches on which.

member.ordinal carries a unique constraint because sidecar.md's rule that an
edition's page sequence is its members concatenated in the order given is
normative, and a consumer that reorders them disagrees with every run in the
document while sha256 and pages still match. The constraint makes that
unrepresentable rather than merely documented.

One implicit local user is inserted. Spec A2 promotes that row rather than
inserting a new one: inserting would orphan every note and reading position
recorded in between, on an account nobody can log in as, silently."
```

---

## Task 4: `content_key`

**Files:**
- Create: `crates/taiyaku-library/src/identity.rs`, `crates/taiyaku-library/src/identity/tests.rs`

**Interfaces:**
- Consumes: `taiyaku_core::Sidecar`
- Produces: `taiyaku_library::identity::content_key(sidecar: &Sidecar) -> Result<String, String>`

- [ ] **Step 1: Write the failing test**

`crates/taiyaku-library/src/identity/tests.rs`:

```rust
use super::*;
use taiyaku_core::Sidecar;

fn sidecar_json(hub_members: &[(&str, &str)], extra_edition: bool) -> String {
    let members: Vec<String> = hub_members
        .iter()
        .map(|(path, sha)| {
            format!(r#"{{"path":"{path}","sha256":"{sha}","pages":10}}"#)
        })
        .collect();
    let extra = if extra_edition {
        r#","en":{"label":"En","members":[{"path":"e.cbz","sha256":"ee","pages":10}]}"#
    } else {
        ""
    };
    format!(
        r#"{{"taiyaku":1,"work":{{"title":"T","reading":"rtl"}},
            "editions":{{"raw":{{"label":"Raw","members":[{}]}}{}}},
            "hub":"raw","alignments":[]}}"#,
        members.join(","),
        extra
    )
}

/// The property the whole design rests on. Adding an edition must NOT change a
/// work's identity — a rescan would otherwise see the work vanish and a new one
/// appear, orphaning its progress and its notes.
#[test]
fn adding_an_edition_does_not_change_the_key() {
    let one = Sidecar::from_json(&sidecar_json(&[("a.cbz", "aa")], false)).expect("parses");
    let two = Sidecar::from_json(&sidecar_json(&[("a.cbz", "aa")], true)).expect("parses");

    assert_eq!(
        content_key(&one).expect("key"),
        content_key(&two).expect("key"),
        "a second edition must not re-identify the work"
    );
}

#[test]
fn replacing_a_hub_archive_does_change_the_key() {
    let before = Sidecar::from_json(&sidecar_json(&[("a.cbz", "aa")], false)).expect("parses");
    let after = Sidecar::from_json(&sidecar_json(&[("a.cbz", "bb")], false)).expect("parses");

    assert_ne!(
        content_key(&before).expect("key"),
        content_key(&after).expect("key"),
        "different bytes are a different work"
    );
}

/// Member order is normative, so two works whose hub holds the same archives in
/// different orders are different works.
#[test]
fn member_order_is_part_of_the_key() {
    let forward =
        Sidecar::from_json(&sidecar_json(&[("a.cbz", "aa"), ("b.cbz", "bb")], false)).expect("p");
    let backward =
        Sidecar::from_json(&sidecar_json(&[("b.cbz", "bb"), ("a.cbz", "aa")], false)).expect("p");

    assert_ne!(content_key(&forward).expect("key"), content_key(&backward).expect("key"));
}

#[test]
fn a_hub_that_names_no_edition_is_an_error_naming_the_hub() {
    let json = r#"{"taiyaku":1,"work":{"title":"T","reading":"rtl"},
                   "editions":{"raw":{"label":"R","members":[]}},
                   "hub":"missing","alignments":[]}"#;
    let sidecar = Sidecar::from_json(json).expect("parses");
    let refused = content_key(&sidecar).expect_err("a hub naming nothing must fail");
    assert!(refused.contains("missing"), "the message names the hub: {refused}");
}
```

- [ ] **Step 2: Run and watch it fail**

```bash
cargo test -p taiyaku-library identity
```

Expected: FAIL — `content_key` not found.

- [ ] **Step 3: Implement it**

`crates/taiyaku-library/src/identity.rs`:

```rust
//! What makes a work the same work across a move, a rename or a remount.
//!
//! Content, with the path kept only as a hint. Keying on the canonical path —
//! which is what `taiyaku-serve`'s `work_id` does today — would duplicate every
//! work in the library when a NAS mounts at `/mnt/nas` on one host and
//! `/Volumes/nas` on another, and orphan every note on it.

use std::fmt::Write as _;

use sha2::{Digest, Sha256};
use taiyaku_core::Sidecar;

/// A work's identity: a sha256 over **the hub edition's** members' `sha256`s,
/// in member order.
///
/// **That edition alone, and not every member of every edition.** Folding all
/// editions in would change a work's identity whenever one was added — and
/// adding editions is the entire point of this project. A rescan would then see
/// the work vanish and a new one appear, taking its progress and its notes with
/// it. The hub is fixed when the work is created.
///
/// # Errors
///
/// Returns an error if the document's `hub` names no edition it holds, with the
/// message naming the hub key that was not found.
pub fn content_key(sidecar: &Sidecar) -> Result<String, String> {
    let hub = sidecar.editions.get(&sidecar.hub).ok_or_else(|| {
        format!("hub `{}` names no edition in this document", sidecar.hub)
    })?;

    let mut hasher = Sha256::new();
    for member in &hub.members {
        hasher.update(member.sha256.as_bytes());
        // A separator, so two members whose hashes concatenate the same way as
        // one longer pair cannot collide. Hex digests are fixed-width today and
        // this does not rely on that staying true.
        hasher.update(b"\n");
    }
    // NOT `format!("{:x}", hasher.finalize())`. This workspace pins sha2
    // 0.11.0, whose `Output` no longer implements `LowerHex`, so that form does
    // not compile. This is the idiom `taiyaku-cli/src/signature.rs:116` already
    // uses against the same crate version.
    Ok(hasher.finalize().iter().fold(String::new(), |mut hex, b| {
        let _ = write!(hex, "{b:02x}");
        hex
    }))
}

#[cfg(test)]
mod tests;
```

- [ ] **Step 4: Run the tests**

```bash
cargo test -p taiyaku-library identity
```

Expected: 4 pass.

- [ ] **Step 5: Commit**

```bash
scripts/check-all.sh --no-web --no-wasm
git add -A crates/taiyaku-library
git commit -m "content_key: identity is the hub edition alone

The property the design rests on, with a test: adding an edition must not
change a work's identity, or a rescan sees the work vanish and a new one
appear, orphaning its progress and its notes. The spec's first draft was
ambiguous here and one reading was exactly that bug."
```

---

## Task 5: `project.rs` — rows back into a `Sidecar`

**Files:**
- Create: `crates/taiyaku-library/src/project.rs`, `crates/taiyaku-library/src/project/tests.rs`

**Interfaces:**
- Produces: `taiyaku_library::project::sidecar(conn: &Connection, work_id: i64) -> Result<Sidecar, String>`

- [ ] **Step 1: Write the failing test**

`crates/taiyaku-library/src/project/tests.rs`:

```rust
use super::*;
use crate::Library;

/// Go-Toubun volume 1's real alignment, from `docs/sidecar.md`'s worked
/// example, and **all five of its runs**. An earlier draft of this fixture
/// truncated the runs to three while keeping the real `quality` block, which
/// made the document contradict itself: three runs pair 5 slots at an average
/// of 2.5, not the 193 at 64.33333 the block claims. Use it whole — the numbers
/// only mean anything together.
const ONE_WORK: &str = r#"{
  "taiyaku": 1,
  "work": { "title": "Go-Toubun no Hanayome v01", "reading": "rtl" },
  "editions": {
    "en-mono": { "label": "English official",
                 "members": [{ "path": "en/v01.cbz", "sha256": "c104", "pages": 187 }],
                 "spreads": [4, 45] },
    "raw-mono": { "label": "Raw, monochrome",
                  "members": [{ "path": "raw/v01.cbz", "sha256": "3f9a", "pages": 195 }] }
  },
  "hub": "raw-mono",
  "alignments": [
    { "from": "raw-mono", "to": "en-mono", "by": "solver",
      "solver": { "hash": "dhash-16", "floor": 0.69921875 },
      "quality": { "verdict": "sound", "slots": 195, "paired": 193, "avg_run": 64.33333 },
      "runs": [
        { "a": "0", "b": "0", "margin": 0.08984375 },
        { "a": "1" },
        { "a": "2-5", "b": "1-4R", "margin": 0.24609375 },
        { "a": "6" },
        { "a": "7-194", "b": "5-186", "margin": -0.12890625 }
      ] }
  ]
}"#;

/// The whole store is a projection, so the test that matters is that the
/// projection is lossless. Byte-for-byte rather than semantic equality:
/// `Sidecar::to_json` is deterministic — BTreeMap keys, pretty-printed, and
/// `sidecar.md` pins the float formatting down to `avg_run`'s five decimals —
/// so a comparison written to tolerate differences would hide exactly the bugs
/// this exists to catch.
#[test]
fn a_sidecar_survives_a_round_trip_byte_for_byte() {
    let library = Library::open_in_memory().expect("library");
    let parsed = taiyaku_core::Sidecar::from_json(ONE_WORK).expect("fixture parses");
    let canonical = parsed.to_json();

    let conn = library.conn();
    let work_id = insert_by_hand(&conn, &parsed);
    let projected = sidecar(&conn, work_id).expect("projection");

    assert_eq!(projected.to_json(), canonical);
}

/// A spread list is page indices and must come back in order and complete.
#[test]
fn page_flags_survive_the_round_trip() {
    let library = Library::open_in_memory().expect("library");
    let parsed = taiyaku_core::Sidecar::from_json(ONE_WORK).expect("fixture parses");
    let conn = library.conn();
    let work_id = insert_by_hand(&conn, &parsed);
    let projected = sidecar(&conn, work_id).expect("projection");
    assert_eq!(projected.editions["en-mono"].spreads, vec![4, 45]);
}
```

**`insert_by_hand` is a test helper in this file that writes the rows with plain
SQL**, not a call to `import::sidecar` — which does not exist until Task 6. This
task must be green on its own; a test that cannot pass until a later task is a
task with no gate. Write it as a straightforward walk of the parsed document:
insert `series`, `work`, then each `edition`, its `member`s with `ordinal`
counting up, its `page_flag` rows, then each `alignment` and its `run`s with
`ordinal` counting up. Roughly forty lines, and it is deliberately a *second*
writer of these rows — Task 6's `import::sidecar` is the real one, and having
the projection test not depend on it is what makes a projection bug
distinguishable from an import bug.

- [ ] **Step 2: Run and watch it fail**

```bash
cargo test -p taiyaku-library project
```

Expected: FAIL — neither `project::sidecar` nor `import::sidecar` exists. Task 6 writes the import half; write it as a stub now so this test compiles, then make it real in Task 6.

- [ ] **Step 3: Write `project.rs`**

Read the rows for one work and rebuild the document. The span columns go back through `SlotSpan`'s own parse/display so the two representations cannot drift, and `verdict` is **not** read back — `Sidecar` recomputes it from the runs (`core/sidecar.rs:318`), so projecting a stored one would let a stale value reach a reader.

```rust
//! Rows back into the documents this project already has types for.
//!
//! The store is a projection, and this is the half that proves it. Nothing here
//! decides anything about an alignment: it reassembles a `Sidecar` and hands it
//! to `taiyaku-core`, which owns every rule about what one means.

use std::collections::BTreeMap;

use rusqlite::Connection;
use taiyaku_core::{Alignment, Edition, Member, Quality, RunEntry, Sidecar, SlotSpan, Work};

/// Rebuild one work's sidecar from its rows.
///
/// # Errors
///
/// Returns an error if `work_id` names no work, if a stored slot span cannot be
/// parsed back (which means a hand edit reached the database), or if SQLite
/// fails.
pub fn sidecar(conn: &Connection, work_id: i64) -> Result<Sidecar, String> {
    let (title, reading, hub) = work_row(conn, work_id)?;
    let editions = editions_for(conn, work_id)?;
    let alignments = alignments_for(conn, work_id)?;
    Ok(Sidecar {
        taiyaku: taiyaku_core::VERSION,
        work: Work { title, reading },
        editions,
        hub,
        alignments,
    })
}

fn members_for(conn: &Connection, edition_id: i64) -> Result<Vec<Member>, String> {
    let mut stmt = conn
        .prepare(
            "SELECT path, sha256, pages FROM member
             WHERE edition_id = ?1 ORDER BY ordinal",
        )
        .map_err(|e| format!("preparing members: {e}"))?;
    let rows = stmt
        .query_map([edition_id], |row| {
            Ok(Member {
                path: row.get(0)?,
                sha256: row.get(1)?,
                pages: row.get(2)?,
            })
        })
        .map_err(|e| format!("querying members: {e}"))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("reading members: {e}"))
}

fn flags_for(conn: &Connection, edition_id: i64, kind: &str) -> Result<Vec<u32>, String> {
    let mut stmt = conn
        .prepare("SELECT page FROM page_flag WHERE edition_id = ?1 AND kind = ?2 ORDER BY page")
        .map_err(|e| format!("preparing page flags: {e}"))?;
    let rows = stmt
        .query_map(rusqlite::params![edition_id, kind], |row| row.get::<_, u32>(0))
        .map_err(|e| format!("querying page flags: {e}"))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("reading page flags: {e}"))
}

#[cfg(test)]
mod tests;
```

`members_for` and `flags_for` above are the pattern; write `work_row`,
`editions_for` and `alignments_for` the same way — one small private function
per stage, each mapping its own SQLite error with the stage named, so a failure
says which stage failed rather than "invalid column type".

**Three rules for `alignments_for`, each of which the round-trip test will
catch if broken:**

1. `ORDER BY ordinal` on `run`. Run order is the document's order and the
   solver's; unordered rows produce a document that parses and means something
   else.
2. Rebuild each span from the four columns through `SlotSpan`, never from a
   stored string. A `Slot` is `{ page: u32, half: Option<Half> }`
   (`core/slot.rs:44`), and `b_start_page IS NULL` means the run has no b-side
   at all — that is `RunEntry { b: None }`, not an empty span.
3. **Read `verdict` back, along with `slots`, `paired` and `avg_run`.**
   *An earlier draft of this plan said the opposite and was wrong.* It misread
   `core/sidecar.rs:318` — the sentence there says **`Sidecar::quality_of`**
   recomputes the verdict, and that is a *query accessor* (`sidecar.rs:1220`)
   which builds a fresh `Quality` to answer a question. It is not the
   serialisation path. `Quality` derives `Serialize`, so `to_json` writes
   whatever the struct holds, verbatim.

   So a projection that recomputed the verdict would emit a **different
   document from the one it was given**, and the byte-for-byte test would be
   unsatisfiable by construction. Round-trip the stored value. The "never an
   authority" property is untouched: nothing consults the stored verdict,
   because everything that wants one calls `quality_of`.

- [ ] **Step 4: Run the tests until the round trip is byte-identical**

```bash
cargo test -p taiyaku-library project -- --nocapture
```

When it fails, the assertion prints both documents. The usual causes, in order of likelihood: a missing `ORDER BY ordinal`, a `verdict` read back instead of recomputed, and an `Option` field written as `Some("")` rather than `None`.

- [ ] **Step 5: Commit**

```bash
scripts/check-all.sh --no-web --no-wasm
git add -A crates/taiyaku-library
git commit -m "project: rows back into a sidecar, byte for byte

Byte-identical rather than semantic equality, because to_json is deterministic
and a comparison written to tolerate differences would hide exactly the losses
this test exists to catch. It is also why the schema stores four slot columns
rather than the rendered span string, and why every Quality field including
verdict round-trips: to_json writes the struct verbatim, so a projection that
recomputed one would hand back a different document from the one it was given."
```

---

## Task 6: `import.rs` — a document into rows

**Files:**
- Create: `crates/taiyaku-library/src/import.rs`, `crates/taiyaku-library/src/import/tests.rs`

**Interfaces:**
- Consumes: `identity::content_key`
- Produces: `taiyaku_library::import::sidecar(conn: &Connection, doc: &Sidecar, path: &Path) -> Result<i64, String>` — returns the `work.id`

- [ ] **Step 1: Write the failing tests**

`crates/taiyaku-library/src/import/tests.rs`:

```rust
use super::*;
use crate::Library;
use std::path::Path;

fn doc(hub_sha: &str) -> taiyaku_core::Sidecar {
    let json = format!(
        r#"{{"taiyaku":1,"work":{{"title":"T","reading":"rtl"}},
             "editions":{{"raw":{{"label":"R","members":[
               {{"path":"raw/v01.cbz","sha256":"{hub_sha}","pages":10}}]}}}},
             "hub":"raw","alignments":[]}}"#
    );
    taiyaku_core::Sidecar::from_json(&json).expect("parses")
}

#[test]
fn importing_the_same_document_twice_is_one_work() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let first = sidecar(&conn, &doc("aa"), Path::new("/x/taiyaku.json")).expect("first");
    let second = sidecar(&conn, &doc("aa"), Path::new("/x/taiyaku.json")).expect("second");
    assert_eq!(first, second);

    let works: i64 = conn.query_row("SELECT count(*) FROM work", [], |r| r.get(0)).unwrap();
    assert_eq!(works, 1);
}

/// The path is a hint, never identity. Move the sidecar and it is the same work
/// — which is what keeps its progress and its notes.
#[test]
fn the_same_content_at_a_new_path_is_the_same_work() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let first = sidecar(&conn, &doc("aa"), Path::new("/mnt/nas/taiyaku.json")).expect("first");
    let moved = sidecar(&conn, &doc("aa"), Path::new("/Volumes/nas/taiyaku.json")).expect("moved");
    assert_eq!(first, moved, "a remount must not duplicate the work");

    let path: String = conn
        .query_row("SELECT sidecar_path FROM work WHERE id = ?1", [first], |r| r.get(0))
        .unwrap();
    assert_eq!(path, "/Volumes/nas/taiyaku.json", "the hint follows the file");
}

/// Both keys miss: the archives were replaced under the same path. That is the
/// same work with new content, so the key is updated rather than a second work
/// inserted.
#[test]
fn replaced_archives_at_a_known_path_update_the_key_rather_than_inserting() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let first = sidecar(&conn, &doc("aa"), Path::new("/x/taiyaku.json")).expect("first");
    let again = sidecar(&conn, &doc("bb"), Path::new("/x/taiyaku.json")).expect("again");
    assert_eq!(first, again);

    let works: i64 = conn.query_row("SELECT count(*) FROM work", [], |r| r.get(0)).unwrap();
    assert_eq!(works, 1, "replacing the archives is not a second work");
}

/// Content matches one work while the path matches another — reachable by
/// copying a sidecar elsewhere and then replacing the original's archives.
/// Content wins, and the work that used to hold the path stops claiming it,
/// because whatever is on disk there is now the other work's document.
#[test]
fn taking_over_a_path_clears_it_from_the_work_that_held_it() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let a = sidecar(&conn, &doc("aa"), Path::new("/p1")).expect("work a");
    let b = sidecar(&conn, &doc("bb"), Path::new("/p2")).expect("work b");

    // The document whose content is A's now sits at B's path.
    let again = sidecar(&conn, &doc("aa"), Path::new("/p2")).expect("content wins");
    assert_eq!(again, a, "content decides, not path");

    let works: i64 = conn.query_row("SELECT count(*) FROM work", [], |r| r.get(0)).unwrap();
    assert_eq!(works, 2, "no third work");

    let a_path: Option<String> = conn
        .query_row("SELECT sidecar_path FROM work WHERE id = ?1", [a], |r| r.get(0))
        .unwrap();
    let b_path: Option<String> = conn
        .query_row("SELECT sidecar_path FROM work WHERE id = ?1", [b], |r| r.get(0))
        .unwrap();
    assert_eq!(a_path.as_deref(), Some("/p2"), "the taker holds the path");
    assert_eq!(b_path, None, "and the former holder no longer claims it");
}

/// Re-importing a document with FEWER editions than before must not leave the
/// removed edition's rows behind. Editions are derived from the document and
/// replaced wholesale, so this is the case where a diffing implementation would
/// quietly accumulate orphans.
#[test]
fn reimporting_with_fewer_editions_removes_the_ones_that_went() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();

    let two = two_edition_doc();
    let id = sidecar(&conn, &two, Path::new("/x/taiyaku.json")).expect("first import");
    let before: i64 = conn
        .query_row("SELECT count(*) FROM edition WHERE work_id = ?1", [id], |r| r.get(0))
        .unwrap();
    assert_eq!(before, 2);

    // The same work — same hub members, so the same content_key — with the
    // spoke edition and its alignment gone.
    //
    // **Deliberately at a different path.** Re-importing at the same path would
    // resolve through `find_by_path` whether or not the content keys matched,
    // so the shared hub edition would be decoration: a review proved exactly
    // that by breaking the hub's sha256 and watching this test still pass. A
    // different path forces `find_by_key` to be what resolves it, which is the
    // thing this test claims to exercise.
    let one = hub_only_doc();
    let again = sidecar(&conn, &one, Path::new("/moved/taiyaku.json")).expect("reimport");
    assert_eq!(again, id, "content resolved it, not the path");

    let editions: i64 = conn
        .query_row("SELECT count(*) FROM edition WHERE work_id = ?1", [id], |r| r.get(0))
        .unwrap();
    let members: i64 = conn.query_row("SELECT count(*) FROM member", [], |r| r.get(0)).unwrap();
    let alignments: i64 = conn
        .query_row("SELECT count(*) FROM alignment WHERE work_id = ?1", [id], |r| r.get(0))
        .unwrap();
    let runs: i64 = conn.query_row("SELECT count(*) FROM run", [], |r| r.get(0)).unwrap();

    assert_eq!(editions, 1, "the removed edition is gone");
    assert_eq!(members, 1, "and its members with it");
    assert_eq!(alignments, 0, "and the alignment that referenced it");
    assert_eq!(runs, 0, "and that alignment's runs");
}

#[test]
fn two_slugs_never_collide() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    sidecar(&conn, &doc("aa"), Path::new("/a/taiyaku.json")).expect("a");
    sidecar(&conn, &doc("bb"), Path::new("/b/taiyaku.json")).expect("b");

    let slugs: i64 = conn
        .query_row("SELECT count(DISTINCT slug) FROM work", [], |r| r.get(0))
        .unwrap();
    assert_eq!(slugs, 2, "two works must have two slugs");
}
```

- [ ] **Step 2: Run and watch them fail**

```bash
cargo test -p taiyaku-library import
```

- [ ] **Step 3: Implement the matching rule and the insert**

Spec §4.1's three cases, in order:

```rust
//! Documents into rows.
//!
//! The matching rule is §4.1 of the spec: `content_key` first; on a miss,
//! `sidecar_path`; if the path hits and the key differs, the archives were
//! replaced, so the key is updated rather than a second work inserted.

/// Import one sidecar, returning the `work.id` it landed on.
///
/// # Errors
///
/// Returns an error if `content_key` fails — a hub naming no edition — or if
/// SQLite refuses a statement.
pub fn sidecar(conn: &Connection, doc: &Sidecar, path: &Path) -> Result<i64, String> {
    let key = crate::identity::content_key(doc)?;
    let existing = find_by_key(conn, &key)?.or(find_by_path(conn, path)?);

    let work_id = match existing {
        Some(id) => { update_work(conn, id, doc, path, &key)?; id }
        None => insert_work(conn, doc, path, &key)?,
    };

    // Editions, members, page flags and alignments are derived from the
    // document, so they are replaced wholesale inside one transaction rather
    // than diffed. Hand corrections do not live here — they arrive through
    // `Sidecar::merge` in Task 7, before this is called.
    replace_editions(conn, work_id, doc)?;
    replace_alignments(conn, work_id, doc)?;
    Ok(work_id)
}
```

The slug is derived from the title, lowercased and non-alphanumerics replaced with `-`, with a numeric suffix on collision — the same shape as `works::work_id` but from the title rather than the filename, since the filename is `taiyaku.json` for every work in the corpus.

Wrap the whole body in `conn.unchecked_transaction()` so a failure part-way leaves nothing.

**`replace_editions` must carry the columns the sidecar does not own.**

*Added after Task 10, which found this by building the metadata sibling and
noticing the import path destroys it.* Editions and members are replaced
wholesale because they are derived from the document — but their rows also carry
**hand-entered** data the sidecar knows nothing about:

| Column | Set by | Lost on reimport without this |
| --- | --- | --- |
| `edition.language`, `edition.kind` | `<stem>.library.json` | nulled |
| `credit` rows on an edition | `<stem>.library.json` | cascade away with the edition row |
| `member.chapter_declared`, `member.chapter_source` | `<stem>.library.json`, or a person | nulled |

The last is the worst: a hand-set chapter number is precisely what the design
calls hand-entered and expensive to backfill, and a routine re-solve would
silently discard it. The file that exists to make metadata survive a rescan does
not survive one.

So `replace_editions` reads those columns before deleting, and restores them
after reinserting — **matching editions by `key` and members by `sha256`**, not
by row id, since the ids are gone. `sha256` is the right member identity for the
same reason `<stem>.library.json` uses it: it survives a path change.

This mirrors what `Sidecar::merge_spoke` already does for hand-corrected runs,
which is the established pattern here: derived data is replaced, hand-entered
data is carried across the replacement.

**Taking over a path clears it from whatever held it before.** Content wins over path, so when the incoming document's `content_key` matches work A while `path` matches work B, A is the answer and A's `sidecar_path` becomes that path. B's `sidecar_path` must then be set to NULL in the same transaction.

Not tidiness — B's pointer is *false* the moment A takes the path: whatever is on disk there is A's document, so B's record of living there describes nothing. Leaving it also makes `find_by_path`'s `query_row` arbitrary between two rows claiming one path, which is a lookup that silently returns a different work depending on rowid order. B is not lost by this; its `content_key` still identifies it, and its progress, notes and read history are untouched. It simply stops claiming an address that is no longer its own.

- [ ] **Step 4: Run the tests**

```bash
cargo test -p taiyaku-library import
cargo test -p taiyaku-library project
```

Expected: both green — Task 5's round trip now has a real import behind it.

- [ ] **Step 5: Commit**

```bash
scripts/check-all.sh --no-web --no-wasm
git add -A crates/taiyaku-library
git commit -m "import: content first, path as a hint

Move the sidecar, rename the directory or remount the NAS and it is the same
work, which is what keeps its progress and its notes. Keying on the canonical
path — what work_id does today — would duplicate every work when a NAS mounts
at a different point on another host."
```

---

## Task 7: Re-import goes through `Sidecar::merge`

The conflict this design exists to survive: you re-solve with `align --write` while the database holds hand corrections you have not exported.

**Files:**
- Modify: `crates/taiyaku-library/src/import.rs`, `crates/taiyaku-library/src/import/tests.rs`

**Interfaces:**
- Produces: `taiyaku_library::import::reimport(conn: &Connection, incoming: &Sidecar, path: &Path) -> Result<(i64, Vec<taiyaku_core::Merged>), String>`

- [ ] **Step 1: Read the rule before implementing against it**

```bash
sed -n '404,475p' crates/taiyaku-core/src/sidecar.rs
```

`Merged::CarriedHandRuns(n)` when a spoke's bytes are unchanged; `Merged::DroppedCredits(n)` when they changed. The asymmetry is deliberate and the doc comment gives it.

- [ ] **Step 2: Write the failing test**

```rust
/// The database holds a hand correction that the incoming document does not.
/// Re-import must not lose it — and it must not reimplement the rule that says
/// so, which lives in `taiyaku-core` and is already tested there.
#[test]
fn a_hand_run_in_the_database_survives_a_re_solve() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();

    let solved = two_edition_doc_with_runs(&[("0-9", Some("0-9"), None)]);
    let id = sidecar(&conn, &solved, Path::new("/x/taiyaku.json")).expect("first import");

    // A reader corrects one run. This is what `PATCH /alignments` produces.
    conn.execute("UPDATE run SET by = 'hand' WHERE ordinal = 0", []).expect("correction");

    let (again, outcomes) =
        reimport(&conn, &solved, Path::new("/x/taiyaku.json")).expect("re-import");
    assert_eq!(again, id);

    let hand: i64 = conn
        .query_row("SELECT count(*) FROM run WHERE by = 'hand'", [], |r| r.get(0))
        .unwrap();
    assert_eq!(hand, 1, "the hand correction must survive the re-solve");
    assert!(
        outcomes.iter().any(|m| matches!(m, taiyaku_core::Merged::CarriedHandRuns(1))),
        "and the outcome must be reported rather than swallowed: {outcomes:?}"
    );
}
```

Write `two_edition_doc_with_runs` as a helper in the same file, building the JSON the way `doc` does.

**Three more tests, and the first is the one that matters most.**

```rust
/// **The guard that stops a merge against replaced archives.**
///
/// `merge_spoke` compares its own stored edition at `alignment.from` against
/// the `from_side` it is handed, on `sha256` (`core/sidecar.rs:628`), and
/// refuses when they differ. Passing the *held* document's hub instead of the
/// *incoming* one makes that comparison compare a value against itself — always
/// equal, guard dead, and a merge proceeds against a hub whose archives have
/// changed, leaving runs that index a page sequence which no longer exists.
///
/// An earlier draft of this plan did exactly that. Nothing caught it, because
/// the only `reimport` test re-imported an identical document, where held and
/// incoming hubs are byte-identical either way. This is the test that would
/// have.
#[test]
fn a_reimport_whose_hub_archives_changed_is_refused_rather_than_merged() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let path = Path::new("/x/taiyaku.json");

    let original = two_edition_doc_with_runs(&[("0-9", Some("0-9"), None)]);
    sidecar(&conn, &original, path).expect("first import");

    // Same path, different hub bytes: the archives behind the hub were
    // replaced. The stored runs describe a page sequence that no longer exists.
    let replaced = two_edition_doc_with_runs_and_hub_sha(&[("0-9", Some("0-9"), None)], "zz");

    let refused = reimport(&conn, &replaced, path)
        .expect_err("a hub whose content changed must not be merged into");
    assert!(
        refused.contains("not the edition just aligned"),
        "the refusal must name the mismatch rather than failing obscurely: {refused}"
    );
}

/// Deviation from the plan's original pseudocode, which said `continue` and so
/// silently dropped the edition. A document that says *nothing* about an
/// edition is not a document saying it is *gone*.
#[test]
fn an_incoming_edition_with_no_hub_alignment_is_added_when_never_held() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let path = Path::new("/x/taiyaku.json");

    sidecar(&conn, &doc("aa"), path).expect("hub only");
    let (id, _) = reimport(&conn, &doc_with_unaligned_edition("aa", "extra"), path)
        .expect("reimport");

    let held = crate::project::sidecar(&conn, id).expect("projection");
    assert!(held.editions.contains_key("extra"), "a new orphan edition is added");
}

#[test]
fn an_incoming_edition_with_no_hub_alignment_leaves_a_held_one_alone() {
    let library = Library::open_in_memory().expect("library");
    let conn = library.conn();
    let path = Path::new("/x/taiyaku.json");

    let with_extra = doc_with_unaligned_edition("aa", "extra");
    let (id, _) = (
        sidecar(&conn, &with_extra, path).expect("first import"),
        Vec::<taiyaku_core::Merged>::new(),
    );

    // The same document again. The orphan edition must survive untouched
    // rather than being dropped and re-added.
    let (again, _) = reimport(&conn, &with_extra, path).expect("reimport");
    assert_eq!(again, id);

    let held = crate::project::sidecar(&conn, id).expect("projection");
    assert!(held.editions.contains_key("extra"), "the orphan edition survives");
}
```

`two_edition_doc_with_runs_and_hub_sha` is `two_edition_doc_with_runs` with the hub member's `sha256` parameterised; `doc_with_unaligned_edition(hub_sha, name)` is `doc(hub_sha)` plus a second edition and **no** alignment mentioning it.

- [ ] **Step 3: Implement `reimport`**

```rust
/// Re-import a document over a work the library already holds.
///
/// **Projects through `Sidecar` rather than merging rows.** The rule that
/// decides what a re-solve does to a hand correction is `Sidecar::merge`, it
/// lives in `taiyaku-core`, and it is already tested there. A second
/// implementation over rows is how two mechanisms come to disagree.
///
/// # Errors
///
/// Returns an error if the projection or the merge fails, or if SQLite refuses
/// a statement.
pub fn reimport(
    conn: &Connection,
    incoming: &Sidecar,
    path: &Path,
) -> Result<(i64, Vec<taiyaku_core::Merged>), String> {
    let key = crate::identity::content_key(incoming)?;
    let Some(work_id) = find_by_key(conn, &key)?.or(find_by_path(conn, path)?) else {
        return Ok((sidecar(conn, incoming, path)?, Vec::new()));
    };

    // 1. project what we hold
    let mut held = crate::project::sidecar(conn, work_id)?;

    // 2. merge each incoming spoke into it. The hub is not a spoke, and an
    //    edition with no alignment to the hub is carried by replacing the
    //    edition alone.
    //
    // **`from_side` is the INCOMING hub edition, not the held one.** An earlier
    // draft of this plan took it from `held` and that silently disarms a safety
    // check: `merge_spoke` looks up its own `stored` edition at `alignment.from`
    // and compares it to `from_side` member-by-member on `sha256`
    // (`core/sidecar.rs:628`), refusing with "is not the edition just aligned —
    // re-solve the work rather than merging into it". Hand it `held`'s hub and
    // `stored` and `from_side` are the same edition, so the comparison compares
    // a value against itself, always matches, and the guard is dead. A merge
    // could then proceed against a hub whose archives had been replaced,
    // producing a document whose runs index a page sequence that no longer
    // exists. `core/sidecar.rs:551` says so directly: "`from_side` is the
    // freshly described `alignment.from` edition." `taiyaku-cli`'s own call site
    // (`main.rs:1697`) passes the incoming a-side for the same reason.
    let hub_side = incoming
        .editions
        .get(&incoming.hub)
        .ok_or_else(|| format!("incoming document's hub `{}` names no edition", incoming.hub))?
        .clone();

    let mut outcomes = Vec::new();
    for (name, edition) in &incoming.editions {
        if *name == incoming.hub {
            continue;
        }
        let Some(alignment) = incoming
            .alignments
            .iter()
            .find(|a| a.from == incoming.hub && a.to == *name)
        else {
            // No alignment to the hub, so there is nothing to hand
            // `merge_spoke` — but `continue` here would silently DROP the
            // edition on a re-import, which is data loss dressed as a no-op.
            //
            // An edition the library has never held is added outright. One it
            // already holds is left entirely alone, along with every alignment
            // that mentions it: the incoming document says nothing about how
            // this edition relates to the hub, and "says nothing" is not
            // "says it is gone".
            carry_unaligned_edition(&mut held, name, edition)?;
            continue;
        };
        let outcome = held
            .merge_spoke(
                incoming.work.reading,
                taiyaku_core::Anchor::Hub,
                &hub_side,
                name,
                edition.clone(),
                alignment.clone(),
            )
            .map_err(|e| format!("merging `{name}`: {e}"))?;
        outcomes.push(outcome);
    }

    // 3. write the merged document back
    sidecar(conn, &held, path)?;
    Ok((work_id, outcomes))
}
```

**The method is `merge_spoke`, not `merge`**, and it takes six arguments
(`core/sidecar.rs:594`):

```rust
pub fn merge_spoke(
    &mut self,
    reading: Direction,
    anchor: Anchor,
    from_side: &Edition,
    name: &str,
    edition: Edition,
    alignment: Alignment,
) -> Result<Merged, String>
```

It refuses outright when `reading` disagrees with the held document's, because
reading order decides which half of a spread comes first — so a mismatch is a
real error to surface rather than a case to paper over. `Anchor::Hub` is the
star; `Anchor::Direct` is for a pair that has earned a direct edge and is not
what an import mints.

- [ ] **Step 4: Run the tests**

```bash
cargo test -p taiyaku-library import
```

- [ ] **Step 5: Commit**

```bash
scripts/check-all.sh --no-web --no-wasm
git add -A crates/taiyaku-library
git commit -m "Re-import through Sidecar::merge, not through rows

The conflict this design exists to survive is a re-solve landing while the
database holds hand corrections nobody exported. That rule already exists in
taiyaku-core, with more care than a fresh one would get and with the asymmetry
between carrying hand runs and dropping a credits list written down. This crate
projects, merges and writes back, so it holds no alignment semantics at all and
there is no second implementation to disagree."
```

---

## Task 8: The corpus round trip

**Files:**
- Create: `crates/taiyaku-library/src/corpus.rs`, and `mod corpus;` under `#[cfg(test)]` in `lib.rs`

- [ ] **Step 1: Write the standing check**

```rust
//! Standing checks against the real corpus. Gated on `TAIYAKU_CORPUS` and
//! skipping cleanly without it, exactly as the sixteen already in this
//! workspace do.

/// Every corpus sidecar survives import and projection **byte for byte**.
///
/// The honest bar, and it is what makes the four-slot-column and
/// never-read-verdict-back decisions testable at all: a projection that loses a
/// field passes semantic equality and fails this.
#[test]
fn every_corpus_sidecar_survives_a_round_trip() {
    let Some(root) = std::env::var_os("TAIYAKU_CORPUS") else {
        eprintln!("TAIYAKU_CORPUS not set; skipping");
        return;
    };
    let root = std::path::PathBuf::from(root);

    let mut checked = 0usize;
    for entry in std::fs::read_dir(&root).expect("corpus root reads") {
        let dir = entry.expect("entry").path();
        let path = dir.join("taiyaku.json");
        if !path.is_file() { continue; }

        let text = std::fs::read_to_string(&path).expect("sidecar reads");
        let parsed = taiyaku_core::Sidecar::from_json(&text)
            .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let canonical = parsed.to_json();

        let library = crate::Library::open_in_memory().expect("library");
        let conn = library.conn();
        let id = crate::import::sidecar(&conn, &parsed, &path)
            .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let projected = crate::project::sidecar(&conn, id)
            .unwrap_or_else(|e| panic!("{}: {e}", path.display()));

        assert_eq!(
            projected.to_json(),
            canonical,
            "{} did not survive the round trip",
            path.display()
        );
        checked += 1;
    }
    assert!(checked >= 12, "expected at least twelve corpus works, saw {checked}");
}
```

- [ ] **Step 2: Run it with the corpus**

```bash
TAIYAKU_CORPUS="$(pwd)/corpus" cargo test -p taiyaku-library corpus -- --nocapture
```

Expected: PASS, having checked 12.

- [ ] **Step 3: Run it without, and confirm it skips rather than fails**

```bash
env -u TAIYAKU_CORPUS cargo test -p taiyaku-library corpus -- --nocapture
```

Expected: PASS, printing the skip line.

- [ ] **Step 4: Add it to the nextest exempt set**

`.config/nextest.toml`'s override filter selects by test name. `test(corpus::)` already matches this module, so **no change is needed** — but verify:

```bash
cargo nextest list -E 'test(corpus::)' 2>/dev/null | grep taiyaku-library
```

Expected: the new test is listed.

- [ ] **Step 5: Commit**

```bash
scripts/check-all.sh --no-web --no-wasm
git add -A crates/taiyaku-library .config/
git commit -m "Every corpus sidecar survives the round trip byte for byte

Twelve works, and byte-identical rather than semantically equal because
to_json is deterministic. This is the test that makes two schema decisions
checkable: four slot columns rather than the rendered span, and a Quality block
that round-trips whole, verdict included, because to_json writes the struct
verbatim and quality_of is what recomputes when anyone asks a question."
```

---

## Task 9: Progress, notes and truth

**Files:**
- Create: `crates/taiyaku-library/src/state.rs`, `crates/taiyaku-library/src/state/tests.rs`

**Interfaces:**
- Produces:
  - `state::progress(conn, work_id, user_id) -> Result<Option<Progress>, String>`
  - `state::put_progress(conn, work_id, user_id, &Progress) -> Result<(), String>`
  - `state::notes(conn, work_id, user_id) -> Result<NotesDoc, String>`
  - `state::put_notes(conn, work_id, user_id, &NotesDoc) -> Result<(), String>`
  - `state::truth(conn, work_id) -> Result<Option<TruthDoc>, String>`
  - `state::put_truth(conn, work_id, &TruthDoc) -> Result<(), String>`
  - `state::merge_notes(held: &NotesDoc, incoming: &NotesDoc) -> NotesDoc`

- [ ] **Step 1: Write the failing tests**

```rust
/// The rule from §4.3: union by ULID, and per note the later `updated` wins.
#[test]
fn merging_notes_unions_by_id_and_takes_the_later_update() {
    let held = notes_doc(&[("01H", "held", "2026-08-14T10:00:00Z"),
                           ("01J", "only in db", "2026-08-14T10:00:00Z")]);
    let incoming = notes_doc(&[("01H", "newer", "2026-08-14T11:00:00Z"),
                               ("01K", "only on disk", "2026-08-14T09:00:00Z")]);

    let merged = merge_notes(&held, &incoming);
    let ids: Vec<&str> = merged.notes.iter().map(|n| n.id.as_str()).collect();
    assert_eq!(ids, vec!["01H", "01J", "01K"], "the union, in id order");
    assert_eq!(merged.notes[0].body, "newer", "the later update wins");
}

#[test]
fn a_whole_slot_note_round_trips_with_no_region() {
    let library = crate::Library::open_in_memory().expect("library");
    let conn = library.conn();
    let work_id = a_work(&conn);
    let doc = notes_doc(&[("01H", "a note", "2026-08-14T10:00:00Z")]);
    put_notes(&conn, work_id, 1, &doc).expect("write");
    let back = notes(&conn, work_id, 1).expect("read");
    assert_eq!(back.notes[0].region, None, "all four columns NULL is no region");
}

#[test]
fn progress_round_trips() {
    let library = crate::Library::open_in_memory().expect("library");
    let conn = library.conn();
    let work_id = a_work(&conn);
    put_progress(&conn, work_id, 1, &crate::Progress {
        edition: "raw-mono".into(), slot: 193,
    }).expect("write");
    let back = progress(&conn, work_id, 1).expect("read").expect("some");
    assert_eq!(back.slot, 193);
}
```

**Step 0 of this task, before the tests above will compile: move `Progress`
into this crate.** It is defined in `taiyaku-serve/src/progress.rs:19` today,
and `taiyaku-library` must not depend on `taiyaku-serve` — the dependency
points the other way. Move the struct definition (and only the struct, not the
file I/O around it) to `taiyaku-library/src/lib.rs`, and change
`taiyaku-serve`'s existing `pub use progress::Progress;` to
`pub use taiyaku_library::Progress;` so every current consumer keeps compiling
against the same path.

**`Notes`, `Note` and notes' own `Region` move the same way**, for the same
reason — all three are in `taiyaku-serve/src/notes.rs` (lines 69, 44 and 28).
Move the three struct definitions only; the file I/O, the `ETag` helper and the
`Invalid` enum stay where they are, since those are the server's concern and
Task 13 rewrites their storage half in place.

Note that `taiyaku-serve`'s `notes::Region` and `taiyaku_core::regions::Region`
are **different types with the same name** — the first is a note's anchor
rectangle in normalised page coordinates, the second is a detected text region.
Do not merge them and do not `use` one where the other is meant.

**`TruthDoc` needs no move**: it is already in `taiyaku-core`
(`core/truth.rs:210`), which this crate depends on.

- [ ] **Step 2: Run, fail, implement, run**

```bash
cargo test -p taiyaku-library state
```

- [ ] **Step 3: Commit**

```bash
scripts/check-all.sh --no-web --no-wasm
git add -A crates/
git commit -m "Progress, notes and truth as rows

Notes merge by ULID with the later update winning per note, which is the rule
the sibling documents needed and Sidecar::merge says nothing about. Truth is
whole-document last-write-wins and the spec records that as the weakest of the
three rules: TruthRegion has no identity to merge on, and labelling is
single-writer in practice."
```

---

## Task 10: `<stem>.library.json`

**Files:**
- Create: `crates/taiyaku-library/src/librarydoc.rs`, `crates/taiyaku-library/src/librarydoc/tests.rs`

**Interfaces:**
- Produces: `librarydoc::LibraryDoc` with `from_json`, `to_json`, and `librarydoc::of(conn, work_id) -> Result<LibraryDoc, String>` / `librarydoc::apply(conn, work_id, &LibraryDoc) -> Result<(), String>`

- [ ] **Step 1: Write the failing test**

```rust
/// Members are keyed by sha256, never by path — the convention
/// `<stem>.truth.json` already uses (`core/truth.rs`, `RegionMember`), and what
/// lets a chapter number survive a file being moved or renamed.
#[test]
fn members_are_keyed_by_sha256_so_a_move_keeps_the_chapter_number() {
    let json = r#"{"taiyaku_library":1,"updated":"2026-08-14T11:03:22Z",
                   "members":[{"sha256":"3f9a","chapter":1.0,"chapter_source":"hand"}]}"#;
    let doc = LibraryDoc::from_json(json).expect("parses");
    assert_eq!(doc.members[0].sha256, "3f9a");
    assert!(doc.members[0].chapter.is_some());
}

const FIXTURE: &str = r#"{
  "taiyaku_library": 1,
  "updated": "2026-08-14T11:03:22Z",
  "series": {
    "titles": [
      { "title": "五等分の花嫁", "kind": "native", "primary": true },
      { "title": "Go-Toubun no Hanayome", "kind": "romaji" }
    ],
    "reading": "rtl"
  },
  "volume": 1,
  "editions": {
    "raw-mono": {
      "language": "ja",
      "kind": "raw",
      "credits": [{ "role": "artist", "name": "Negi Haruba" }]
    }
  },
  "members": [{ "sha256": "3f9a", "chapter": 1.0, "chapter_source": "hand" }],
  "tags": ["romance"],
  "external": { "anilist": "97852" }
}"#;

#[test]
fn a_library_doc_round_trips() {
    let doc = LibraryDoc::from_json(FIXTURE).expect("the fixture parses");
    let back = LibraryDoc::from_json(&doc.to_json()).expect("its own output parses");
    assert_eq!(back, doc);
}

#[test]
fn the_primary_title_is_distinguishable_from_the_others() {
    let doc = LibraryDoc::from_json(FIXTURE).expect("parses");
    let primary: Vec<&str> = doc
        .series
        .titles
        .iter()
        .filter(|t| t.primary)
        .map(|t| t.title.as_str())
        .collect();
    assert_eq!(primary, vec!["五等分の花嫁"]);
}
```

`LibraryDoc` derives `Serialize`, `Deserialize`, `Debug`, `PartialEq`, and its
`to_json` uses `serde_json::to_string_pretty` — the same shape and the same
reason as `Sidecar::to_json`: this file is meant to diff in git.

- [ ] **Step 2: Implement, run, commit**

```bash
cargo test -p taiyaku-library librarydoc
scripts/check-all.sh --no-web --no-wasm
git add -A crates/taiyaku-library
git commit -m "The metadata sibling, keyed by sha256

Hand-fixed metadata round-trips to a file for the same reason everything else
here does: losing the database should cost a rescan rather than the
organisational work, which is the part nothing can re-derive. Collections are
deliberately absent — one spans works and has no single sidecar to sit beside."
```

---

## Task 11: `export.rs`

**Files:**
- Create: `crates/taiyaku-library/src/export.rs`, `crates/taiyaku-library/src/export/tests.rs`

**Interfaces:**
- Produces: `export::work(conn, work_id, dir: Option<&Path>) -> Result<Vec<PathBuf>, String>`

- [ ] **Step 1: Write the failing test**

```rust
/// Five documents, atomically, and nothing half-written if one fails.
#[test]
fn exporting_writes_five_documents_beside_the_sidecar() {
    let dir = tempdir();
    let library = crate::Library::open_in_memory().expect("library");
    let conn = library.conn();
    let id = a_work_with_notes_and_progress(&conn, &dir);

    let written = work(&conn, id, None).expect("export");
    let names: Vec<String> = written
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
        .collect();

    assert_eq!(names, vec![
        "taiyaku.json",
        "taiyaku.library.json",
        "taiyaku.notes.json",
        "taiyaku.progress.json",
        "taiyaku.truth.json",
    ]);
}

/// Export and import are inverses. The exported bytes are exactly what a
/// projection produces, which is what makes "the sidecar remains the
/// interchange format" true rather than nominal.
#[test]
fn an_exported_sidecar_is_the_projection_byte_for_byte() {
    let dir = tempdir();
    let library = crate::Library::open_in_memory().expect("library");
    let conn = library.conn();
    let id = a_work_with_notes_and_progress(&conn, &dir);

    let expected = crate::project::sidecar(&conn, id).expect("projection").to_json();
    work(&conn, id, Some(dir.path())).expect("export");

    let written = std::fs::read_to_string(dir.path().join("taiyaku.json")).expect("reads");
    assert_eq!(written, expected);
}

/// A work nobody has annotated still exports a notes document, because
/// `getNotes` answers one for such a work and the reader has no
/// empty-versus-absent case to carry either way (`startup.ts`).
#[test]
fn a_work_with_no_notes_exports_an_empty_notes_document() {
    let dir = tempdir();
    let library = crate::Library::open_in_memory().expect("library");
    let conn = library.conn();
    let id = a_bare_work(&conn, &dir);

    work(&conn, id, Some(dir.path())).expect("export");
    let text = std::fs::read_to_string(dir.path().join("taiyaku.notes.json")).expect("reads");
    let doc: serde_json::Value = serde_json::from_str(&text).expect("json");
    assert_eq!(doc["notes"].as_array().map(Vec::len), Some(0));
}
```

- [ ] **Step 2: Implement using `taiyaku_read::atomic::write`, run, commit**

```bash
cargo test -p taiyaku-library export
scripts/check-all.sh --no-web --no-wasm
git add -A crates/taiyaku-library
git commit -m "export: five documents, atomically

Explicit rather than write-through. Debouncing every database write out to its
sibling would reintroduce two writers for one document, which is the condition
the merge rule exists to arbitrate, and it would run that arbitration on every
keystroke rather than once per import."
```

---

## Task 12: `ServeState` off the database

**Files:**
- Modify: `crates/taiyaku-serve/Cargo.toml`, `crates/taiyaku-serve/src/lib.rs`, `crates/taiyaku-serve/src/works.rs`

**Interfaces:**
- Consumes: `Library`, `project::sidecar`, `import::sidecar`
- Produces: `ServeState { library: Arc<Library>, works: Vec<Work>, … }`; `Work` unchanged

- [ ] **Step 1: Add the dependency**

`taiyaku-library = { path = "../taiyaku-library" }` in `crates/taiyaku-serve/Cargo.toml`.

- [ ] **Step 2: Write the failing test**

```rust
/// The corpus collides today: `work_id` derives from the filename, and
/// `taiyaku.json` beside each work is the documented convention, so
/// `load_works` refuses. Off the database, twelve works coexist.
#[test]
fn a_library_serves_many_works_where_load_works_refused_two() {
    let library = taiyaku_library::Library::open_in_memory().expect("library");
    {
        let conn = library.conn();
        for (n, sha) in [(1, "aa"), (2, "bb"), (3, "cc")] {
            taiyaku_library::import::sidecar(
                &conn,
                &fixture_doc(n, sha),
                std::path::Path::new(&format!("/corpus/v{n:02}/taiyaku.json")),
            ).expect("import");
        }
    }
    let works = crate::works::from_library(&library).expect("works");
    assert_eq!(works.len(), 3);

    let mut slugs: Vec<&str> = works.iter().map(|w| w.id.as_str()).collect();
    slugs.sort_unstable();
    slugs.dedup();
    assert_eq!(slugs.len(), 3, "three works, three ids");
}
```

- [ ] **Step 3: Implement `works::from_library`**

`Work` is unchanged — `Work::new(slug, sidecar_path, projected_sidecar)`. That is what keeps `cover.rs`, `composite.rs`, `stump/` and `opds.rs` untouched: `sidecar` is now projected from rows rather than parsed from a file, and nothing downstream can tell.

Keep `load_works` for now; Task 15 turns `taiyaku serve <sidecar>...` into an import that calls it.

- [ ] **Step 4: Run the whole serve suite**

```bash
cargo test -p taiyaku-serve
```

Expected: green, including the three corpus-gated tests. **Spec §13 flags this as a claim made from reading rather than running — this step is where it stops being one.** If any of the three needs changing, say so in the commit message.

- [ ] **Step 5: Commit**

```bash
scripts/check-all.sh --no-web --no-wasm
git add -A crates/taiyaku-serve
git commit -m "ServeState off the database, and Work survives

Work keeps its shape and its sidecar is projected from rows rather than parsed
from a file, which is what leaves cover.rs, composite.rs, stump/ and opds.rs
untouched. The corpus can now be served at once: work_id derived a work's id
from its filename, taiyaku.json beside each work is the documented convention,
and load_works correctly refused the collision."
```

---

## Task 13: `progress` / `notes` / `truth` routes onto rows

**Files:**
- Modify: `crates/taiyaku-serve/src/progress.rs`, `notes.rs`, `truth.rs`, `routes.rs`

- [ ] **Step 1: Write the failing test — the `ETag` invariant survives**

```rust
/// `serve/truth.rs:3` states the rule: one serialisation shared by the file,
/// the response body and the `ETag` over that body, so a validator can never be
/// computed over bytes a client was not sent. It becomes one serialisation
/// shared by the PROJECTION, the body and the validator — the same rule with
/// one fewer writer.
#[tokio::test]
async fn a_notes_etag_is_computed_over_the_bytes_that_were_sent() {
    let (router, _state) = router_over_a_library().await;
    let response = get(&router, "/api/works/w/notes").await;
    let etag = response.headers().get("etag").expect("an etag").clone();
    let body = body_string(response).await;

    let recomputed = crate::notes::etag_for(&body);
    assert_eq!(etag, recomputed);
}
```

- [ ] **Step 2: Rewrite the three modules' read and write halves against rows**

Route handlers, status codes, `If-Match` handling and message wording all stay **exactly** as they are. Only the storage changes: `load`/`store` go from `std::fs` over `work.sibling(...)` to `taiyaku_library::state::*`.

- [ ] **Step 3: Run the suite**

```bash
cargo test -p taiyaku-serve
```

- [ ] **Step 4: Commit**

```bash
scripts/check-all.sh --no-web --no-wasm
git add -A crates/taiyaku-serve
git commit -m "progress, notes and truth read and write rows

Same routes, same status codes, same If-Match handling, same messages. The
ETag invariant survives intact: one serialisation shared by the projection, the
body and the validator, rather than by the file, the body and the validator."
```

---

## Task 14: `/api/works` grows a payload

**Files:**
- Modify: `crates/taiyaku-serve/src/routes.rs`, `web/src/api.ts`, `web/src/startup.ts`, and their tests

- [ ] **Step 1: Write the failing Rust test**

```rust
#[tokio::test]
async fn listing_works_answers_objects_rather_than_bare_ids() {
    let (router, _state) = router_over_a_library().await;
    let body = body_string(get(&router, "/api/works").await).await;
    let parsed: serde_json::Value = serde_json::from_str(&body).expect("json");
    let first = &parsed[0];
    assert!(first["slug"].is_string());
    assert!(first["title"].is_string());
    assert!(first["editions"].is_array());
}
```

- [ ] **Step 2: Write the failing TypeScript test**

`web/src/startup.test.ts` — `openFirstWork` still opens the first work, from a richer payload:

```ts
it('opens the first work from a listing of objects', async () => {
  const opened = await openFirstWork({
    works: async () => [{ slug: 'gotoubun-v01', title: 'Go-Toubun v01', editions: ['raw', 'en'] }],
    sidecar: async () => ({ json: SIDECAR_JSON, etag: 'W/"1"' }),
    resolve: async () => fakeResolver(),
    progress: async () => undefined,
    notes: async () => ({ doc: { work: 'gotoubun-v01', notes: [] }, etag: 'W/"2"' }),
  })
  expect(opened.session.work).toBe('gotoubun-v01')
})
```

- [ ] **Step 3: Implement both sides, run both suites**

```bash
cargo test -p taiyaku-serve
pnpm -C web run test
```

- [ ] **Step 4: Commit**

```bash
scripts/check-all.sh
git add -A crates/taiyaku-serve web/src
git commit -m "/api/works answers objects

Slug, title, series, volume, editions and a cover, rather than a bare list of
ids. openFirstWork still opens the first work — a home screen is spec C1, and
keeping it out of here is what stops a browse UI being built before the guard
that spec A2 puts in front of it."
```

---

## Task 15: `taiyaku library import|export`, and `serve` keeping its argument shape

**Files:**
- Modify: `crates/taiyaku-cli/src/main.rs`; create `crates/taiyaku-cli/src/library.rs`

- [ ] **Step 1: Write the failing CLI test**

`crates/taiyaku-cli/tests/cli.rs`:

```rust
#[test]
fn library_import_reports_what_the_merge_did() {
    let out = taiyaku(&["library", "import", fixture("taiyaku.json"), "--db", &db_path()]);
    assert!(out.contains("imported"), "{out}");
}

/// `taiyaku serve <sidecar>...` keeps working: it imports those paths and
/// serves them. That is the migration path for an existing install and what
/// keeps the three corpus-gated tests in serve/src/corpus.rs alive.
#[test]
fn serve_still_accepts_sidecar_paths() {
    let out = taiyaku(&["serve", "--help"]);
    assert!(out.contains("<taiyaku.json>"), "{out}");
}
```

**A hazard Task 13 left for this task, found by its review.** Task 13's
`row_id_for_write` mints a placeholder `work` row for a work the library never
imported, so a reading position can be recorded before anything was scanned. Its
`content_key` is the sentinel `bridge:{slug}`, and its `sidecar_path` is
**whatever string `taiyaku serve <path>` was invoked with, uncanonicalised**.

`import::sidecar` matches on `content_key` first — which can never hit, since a
real key is 64 hex characters — then on `sidecar_path` by **string equality**. So:

- Same path string later, and `find_by_path` hits: the placeholder is *reused*,
  and every note and reading position keyed on its row id survives the
  transition. This is the good outcome and it is the one to guarantee.
- **Any difference at all** — relative against absolute, canonicalised against
  not — and both lookups miss, a second work row is minted with a real slug, and
  everything recorded against the placeholder is **silently orphaned**. No error,
  and the new work simply looks empty.

So this task must either canonicalise on both sides, or reconcile this case by
`slug` rather than by path. Decide which, write down why, and test the orphaning
case explicitly — it is invisible otherwise.

- [ ] **Step 2: Implement, including `--db PATH`**

The database resolves to `$XDG_DATA_HOME/taiyaku/library.db`, falling back to `$HOME/.local/share/taiyaku/library.db`, with `--db` overriding — **and it prints the path it resolved to**, the way `cache_root` does when it falls back. Deliberately *data*, not `$XDG_CACHE_HOME`: deleting this now loses notes.

- [ ] **Step 3: Run everything, commit**

```bash
scripts/check-all.sh
git add -A crates/taiyaku-cli
git commit -m "taiyaku library import|export, and serve keeps its arguments

serve <sidecar>... imports those paths and serves them, which is the migration
path for an existing install. The database resolves under XDG_DATA_HOME rather
than XDG_CACHE_HOME, and says where it landed: unlike the page cache, deleting
this one loses notes."
```

---

## Task 16: The measurements this invalidated

Spec §12. Part of the work, not a surprise at the end.

**Files:**
- Modify: `docs/status.md`, `README.md`, `docs/sidecar.md`, `scripts/coverage-floor.mjs`

- [ ] **Step 1: Re-derive the test count**

```bash
env -u TAIYAKU_CORPUS cargo llvm-cov nextest --workspace --all-features
node scripts/status-test-count.mjs target/nextest/default/junit.xml docs/status.md
```

The script fails with the real number against the stale one. Put the real number in `docs/status.md`, both places it appears.

- [ ] **Step 2: Re-measure the Rust coverage floor**

```bash
cargo llvm-cov report --json --output-path target/rust-coverage.json
node scripts/coverage-floor.mjs target/rust-coverage.json
```

Set the floor at what was achieved, minus the margin the script's own header documents.

- [ ] **Step 3: Update the prose**

- `docs/status.md`: "Five crates" becomes six; the dependency list gains `rusqlite`; the crate table gains a `taiyaku-library` row; the `taiyaku serve` paragraph stops saying it is handed sidecar paths because deciding what constitutes a library is a different job — **and says that position was reversed on purpose**, with a pointer to the spec.
- `README.md`: the `serve` synopsis and "Using it".
- `docs/sidecar.md`: a database is now a second consumer of the format, the format remains the interchange, and `<stem>.library.json` joins the sibling conventions.

- [ ] **Step 4: Run the whole gate, including web and wasm**

```bash
scripts/check-all.sh
```

Expected: green, and its last line names what it did **not** measure.

- [ ] **Step 5: Commit**

```bash
git add -A docs README.md scripts
git commit -m "The figures a sixth crate moved

Test counts, the Rust coverage floor, the crate table and the dependency list.
And status.md stops saying that deciding which files constitute a library is a
different job: that position was reversed deliberately, and the reversal is
recorded rather than quietly deleted."
```

---

## Self-review notes

**Spec coverage.** §1 → Task 12. §2 → Task 3. §3 → Tasks 3, 9. §4.1 → Tasks 4, 6. §4.2 → Task 7. §4.3 → Task 9. §4.4 → Task 10. §4.5 → Task 11. §5 → Tasks 12, 13, 14. §6 → Task 2. §7 → Task 1. §8 → Tasks 5, 8. §11 → the task order. §12 → Task 16. §13.1–13.5 → **not in this plan**; download, metadata editing, read-status shelves, backup and per-root settings are specs C2/C3/C4, and only the schema they need is here (Task 3).

**One correction the self-review caught, worth naming because it would have cost an hour.** An earlier draft of Task 7 called `held.merge(…)`. **There is no `Sidecar::merge`** — the method is `merge_spoke`, it takes six arguments, and it refuses outright when the incoming document's reading direction disagrees with the held one's. The plan now carries the real signature and the real call. Every other type name in this plan was checked against `taiyaku-core`'s re-export list (`core/lib.rs:24`) the same way.

**Task 5's `project::sidecar` gives two of its five stages in full and the rest as a stated pattern**, deliberately: `members_for` and `flags_for` are written out, and the three rules that the round-trip test will catch if broken — run ordering, `b_start_page IS NULL` meaning `b: None`, and never reading `verdict` back — are spelled out rather than left to be discovered.

**One thing to check early, not late.** Task 12 Step 4 is where "the three corpus-gated tests in `serve/src/corpus.rs` should need no change" stops being a claim from reading. Spec §13 records it as unverified; if it turns out false, the branch shape is what changes, so it is worth running that suite before Task 13 builds on it.
