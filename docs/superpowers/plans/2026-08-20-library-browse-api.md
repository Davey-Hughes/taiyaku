# Library Browse API Implementation Plan (spec C1, part 1 of 2)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the server half of spec C1 — a guarded `/api/library/*` browse API over the SQLite store, a per-root permission filter that finally reads `Principal.roots`, and a `serve --db` mode without which the reader cannot open anything the scanner produced.

**Architecture:** Three layers, each with one writer. Migration 6 adds `member.root_id`, and `derive` writes it as a targeted `UPDATE` after `import::scanned` returns — the same shape `write_chapters` already uses for `member.chapter_declared`, so `work`/`edition`/`member` keep exactly one row writer. A new `taiyaku-library/src/browse.rs` holds every read query the UI needs, each taking the caller's granted roots and applying the filter in SQL rather than after it. `taiyaku-serve` adds six handlers that call into `browse.rs` and nothing else, registered in `guarded_routes()` so the existing enumeration test covers them the moment they exist.

**Tech Stack:** Rust 2024, `rusqlite` (bundled SQLite with FTS5), `axum`, `serde_json`. No new crate dependencies in this plan — `react-router`, the one new dependency C1 adds, belongs to part 2.

## Global Constraints

Every task's requirements implicitly include this section.

- **The workspace forbids `unsafe_code`.** No `termios`, no raw FFI; shell out or do without.
- **Clippy runs at pedantic and CI denies warnings.** `#[expect(...)]` with a `reason` where a lint is genuinely wrong, never `#[allow]` without one.
- **Migrations are append-only.** A migration that can have run somewhere is never edited. Migration 6 is new, so it may be edited freely until it merges — after that, never.
- **`work`, `edition` and `member` rows are written by `crate::import` alone.** `derive` writes only the columns the sidecar format has no field for, as targeted `UPDATE`s after `import::scanned` returns. `member.root_id` is one of those; do not add a field to `Sidecar`.
- **Every new `/api` route goes in `routes::guarded_routes()`**, never in `open_routes()`. `route_layer` wraps only what is registered before it, so a route in the wrong list ships unauthenticated with a green suite.
- **`/api` must not answer `WWW-Authenticate`.** That header makes the browser raise its own credential dialog; the reader routes a 401 to the login screen instead.
- **Empty `Principal.roots` means unrestricted**, never "sees nothing". Spec C1 §5.1: every account in every existing library holds an empty grant, so any other reading empties every library on upgrade.
- **Verify with `scripts/check-all.sh`**, which mirrors all four CI jobs. `--no-web` and `--no-wasm` skip jobs while iterating on Rust; the full run is required before the branch is called done.
- **Commit messages carry no attribution.** No `Co-Authored-By`, no `Generated with`.
- **Branch:** `library-browse`, already created, with the amended spec committed at `da7df3e`.

## Reference

- Spec: `docs/superpowers/specs/2026-08-14-library-browse-design.md` (amended 2026-08-20). §5.1 is the root filter, §5.2 the cover fallback, §5.3 `serve --db`.
- Part 2 of this plan — the router, shell, series page, browse module, search UI and empty states — is `docs/superpowers/plans/2026-08-20-library-browse-ui.md`, written after this one lands.

## File Structure

**Created:**
- `crates/taiyaku-library/src/schema/006_member_root.sql` — one `ALTER TABLE`, with the comment explaining why the column is on `member` and not on `work`.
- `crates/taiyaku-library/src/browse.rs` — every read query the browse UI needs: the home lists, the series list, one series, one work, and search. One responsibility: reading rows for a browse screen, with the root filter applied in SQL.
- `crates/taiyaku-library/src/browse/tests.rs` — its tests.
- `crates/taiyaku-serve/src/library_api.rs` — the six handlers. Thin: parse the request, call `browse.rs`, serialise. Kept out of `routes.rs`, which is already 1,579 lines.
- `crates/taiyaku-serve/src/library_api/tests.rs` — route-level tests, including the three root-filter cases.

**Modified:**
- `crates/taiyaku-library/src/schema.rs:29,36` — `SCHEMA_VERSION` 5 → 6 and the `MIGRATIONS` array. The `const _: () = assert!` makes changing one without the other a compile error.
- `crates/taiyaku-library/src/derive.rs` — `Candidate` gains `root_id`; a new `write_roots` beside `write_chapters`; `write_work` calls it.
- `crates/taiyaku-library/src/account.rs` — `grant_root`, `revoke_root`, `granted_roots`.
- `crates/taiyaku-library/src/lib.rs` — `pub mod browse;`.
- `crates/taiyaku-cli/src/user.rs` — `grant-root` and `revoke-root` subcommands.
- `crates/taiyaku-cli/src/main.rs` — `serve` accepting no paths, resolving works from the library.
- `crates/taiyaku-serve/src/works.rs` — `from_library`, restored with an answer for a NULL `sidecar_path`.
- `crates/taiyaku-serve/src/routes.rs:1338` — the alignment write, skipped for a work with no file; six routes added to `guarded_routes()`.
- `crates/taiyaku-serve/src/auth.rs:277` — the `#[expect(dead_code)]` on `Principal.roots` comes off.
- `crates/taiyaku-serve/src/lib.rs` — `pub mod library_api;` and the `from_library` comment rewritten.
- `docs/status.md` — the crate table, the CLI list, O54.

---

### Task 1: Migration 6 — `member.root_id`

**Files:**
- Create: `crates/taiyaku-library/src/schema/006_member_root.sql`
- Modify: `crates/taiyaku-library/src/schema.rs:29` and `:36`
- Test: `crates/taiyaku-library/src/schema/tests.rs`

**Interfaces:**
- Consumes: nothing.
- Produces: `member.root_id INTEGER REFERENCES root(id) ON DELETE SET NULL`, NULL on every existing row. `schema::SCHEMA_VERSION == 6`.

- [ ] **Step 1: Write the failing test**

Append to `crates/taiyaku-library/src/schema/tests.rs`:

```rust
/// Migration 6 adds `member.root_id` and leaves every existing member NULL.
///
/// **NULL is load-bearing rather than incidental.** An imported work sits
/// under no root at all, and spec C1 §5.1's filter reads a NULL member as
/// belonging to no root — which is what lets `member.root_id IN (…)` exclude
/// it with no special case in the SQL. A `NOT NULL DEFAULT 0` would have made
/// every pre-existing member claim root 0, which no row references.
#[test]
fn migration_6_adds_member_root_id_leaving_existing_members_null() {
    let conn = Connection::open_in_memory().expect("in-memory");
    crate::schema::migrate(&conn).expect("migrates");

    let json = r#"{"taiyaku":1,"work":{"title":"T","reading":"rtl"},
        "editions":{"raw":{"label":"R","members":[
          {"path":"raw/v01.cbz","sha256":"deadbeef","pages":300}]}},
        "hub":"raw","alignments":[]}"#;
    let doc = taiyaku_core::Sidecar::from_json(json).expect("parses");
    crate::import::sidecar(&conn, &doc, std::path::Path::new("/x/taiyaku.json"))
        .expect("imports");

    let root_id: Option<i64> = conn
        .query_row("SELECT root_id FROM member LIMIT 1", [], |row| row.get(0))
        .expect("reading the new column");
    assert_eq!(root_id, None, "an imported member belongs to no root");
}

/// The version and the migration count are one fact, and this is the refusal
/// that depends on it staying true.
#[test]
fn the_schema_version_is_six() {
    assert_eq!(crate::schema::SCHEMA_VERSION, 6);
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p taiyaku-library schema::tests::migration_6 -- --nocapture`

Expected: FAIL — `no such column: root_id`.

- [ ] **Step 3: Write the migration**

Create `crates/taiyaku-library/src/schema/006_member_root.sql`:

```sql
-- Spec C1 §5.1. The column a browse API needs in order to act on
-- `Principal.roots`, which spec A2 resolved and left nothing reading.
--
-- **On `member`, not on `work`, and that is forced rather than preferred.**
-- `derive.rs`'s `Group` is "every entry across every root that resolved to the
-- same (series, volume), split by edition key" — one volume held in `Manga`
-- and in `Manga (RAW)` is deliberately ONE work with TWO editions, which is
-- the entire point of this project. A scalar `work.root_id` has no value to
-- hold for such a work, and picking one would be a lie a permission filter
-- then acts on. A member, by contrast, resolves from exactly one `scan_entry`
-- under exactly one root, so the value is never ambiguous.
--
-- It is also the only granularity that expresses what per-root sharing is
-- for: grant `Manga` and not `Manga (RAW)` and the reader sees the work with
-- one edition, rather than with two or with none.
--
-- **NULL means "under no root"**, which is exactly true of an imported work —
-- it has a `sidecar_path` and no root — and of every member written before
-- this migration. `member.root_id IN (?, ?)` excludes NULL by SQL's own
-- rules, so the filter needs no special case and an empty grant (which every
-- account holds, since nothing could write `user_root` until spec C1) applies
-- no filter at all. `NOT NULL DEFAULT 0` was the alternative and would have
-- made every existing member claim a root id no row references.
--
-- **`ON DELETE SET NULL`, not CASCADE.** Removing a root must not delete the
-- members of works derived from it — `import::scanned` owns those rows, and a
-- root removed from configuration is a visibility change, not a deletion.
-- CASCADE here would take the work's pages with it and strand the reader's
-- progress on a work with no members.
ALTER TABLE member ADD COLUMN root_id INTEGER REFERENCES root(id) ON DELETE SET NULL;

CREATE INDEX member_by_root ON member (root_id);
```

- [ ] **Step 4: Register the migration**

In `crates/taiyaku-library/src/schema.rs`, change line 29:

```rust
pub const SCHEMA_VERSION: u32 = 6;
```

and the array at line 36:

```rust
const MIGRATIONS: [&str; 6] = [
    include_str!("schema/001_initial.sql"),
    include_str!("schema/002_absorbed.sql"),
    include_str!("schema/003_scanner.sql"),
    include_str!("schema/004_auth.sql"),
    include_str!("schema/005_note_id_per_user.sql"),
    include_str!("schema/006_member_root.sql"),
];
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p taiyaku-library schema:: -- --nocapture`

Expected: PASS, including the pre-existing `a_migration_that_fails_part_way_leaves_no_trace_and_can_be_retried` and the `pragma_foreign_key_check` that runs inside each migration's transaction.

- [ ] **Step 6: Run the whole library crate**

Run: `cargo test -p taiyaku-library`

Expected: PASS. Nothing else reads `member.root_id` yet, so a failure here means the `ALTER TABLE` disturbed something — most likely `project.rs`, if it selects columns by position rather than by name.

- [ ] **Step 7: Commit**

```bash
git add crates/taiyaku-library/src/schema/006_member_root.sql crates/taiyaku-library/src/schema.rs crates/taiyaku-library/src/schema/tests.rs
git commit -m "Migration 6: member.root_id, the column a root filter can act on"
```

---

### Task 2: `derive` writes `member.root_id`

**Files:**
- Modify: `crates/taiyaku-library/src/derive.rs` — `Candidate` (line 342), its construction (line 710), `write_work` (line 988), and a new `write_roots`
- Test: `crates/taiyaku-library/src/derive/tests.rs`

**Interfaces:**
- Consumes: `member.root_id` from Task 1.
- Produces: after any `derive::derive(&conn)`, every member row written by a scan carries the `root.id` of the root its entry was walked from. `fn write_roots(conn: &Connection, work_id: i64, group: &Group) -> Result<(), String>`.

- [ ] **Step 1: Write the failing test**

Append to `crates/taiyaku-library/src/derive/tests.rs`:

```rust
/// Two roots holding the same volume produce **one work with two editions**,
/// and each edition's members carry the root they actually came from.
///
/// This is the shape `work.root_id` could not have represented, and the
/// reason spec C1 §5.1 put the column on `member` instead. If this test ever
/// reads one root id for both editions, the grouping has changed and the
/// filter is filtering something else.
#[test]
fn a_work_spanning_two_roots_gives_each_member_its_own_root() {
    let english = crate::tests::tempdir("derive-root-en");
    write_pages(&english.path.join("Series").join("Series - v01"), 2);
    let raw = crate::tests::tempdir("derive-root-raw");
    write_pages(&raw.path.join("Series").join("Series - v01"), 2);

    let library = Library::open_in_memory().expect("in-memory library");
    let conn = library.conn();
    let en_root = scan::add_root(&conn, &english.path).expect("adds the English root");
    let raw_root = scan::add_root(&conn, &raw.path).expect("adds the raw root");
    scan::scan_root(&conn, &en_root).expect("scans English");
    scan::scan_root(&conn, &raw_root).expect("scans raw");
    derive::derive(&conn).expect("derives");

    let mut stmt = conn
        .prepare(
            "SELECT DISTINCT m.root_id FROM member m
               JOIN edition e ON e.id = m.edition_id
              ORDER BY m.root_id",
        )
        .expect("preparing the root read");
    let roots: Vec<Option<i64>> = stmt
        .query_map([], |row| row.get(0))
        .expect("reading roots")
        .collect::<rusqlite::Result<_>>()
        .expect("reading roots");

    assert_eq!(
        roots,
        vec![Some(en_root.id), Some(raw_root.id)],
        "each member carries the root its entry was walked from"
    );
}

/// A re-derive that no longer resolves a member must not leave the previous
/// derive's root id standing, for the same reason `write_chapters` clears
/// before it writes.
#[test]
fn a_rederive_rewrites_the_root_rather_than_leaving_the_old_one() {
    let dir = crate::tests::tempdir("derive-root-rewrite");
    write_pages(&dir.path.join("Series").join("Series - v01"), 2);

    let library = Library::open_in_memory().expect("in-memory library");
    let conn = library.conn();
    let root = scan::add_root(&conn, &dir.path).expect("adds the root");
    scan::scan_root(&conn, &root).expect("scans");
    derive::derive(&conn).expect("derives");

    conn.execute("UPDATE member SET root_id = 999", [])
        .expect("scribbles a wrong root over every member");
    derive::derive(&conn).expect("re-derives");

    let wrong: i64 = conn
        .query_row("SELECT COUNT(*) FROM member WHERE root_id <> ?1", [root.id], |row| {
            row.get(0)
        })
        .expect("counting");
    assert_eq!(wrong, 0, "a re-derive rewrites every member's root");
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p taiyaku-library derive::tests::a_work_spanning_two_roots -- --nocapture`

Expected: FAIL — `roots` reads `[None]`, because nothing writes the column yet.

- [ ] **Step 3: Give `Candidate` a root**

In `crates/taiyaku-library/src/derive.rs`, add the field to `Candidate` (after `rel_path`, around line 355):

```rust
    /// The root this entry was walked from.
    ///
    /// **A member has one; a work does not.** The `Group` above collects
    /// entries across every root, so this is the only level at which the
    /// question has a single answer — see `member.root_id`'s own comment in
    /// migration 6, and spec C1 §5.1.
    root_id: i64,
```

and set it at the construction site (around line 710), where `root` is already in scope:

```rust
    edition.push(Candidate {
        path: root
            .path
            .join(&entry.rel_path)
            .to_string_lossy()
            .into_owned(),
        rel_path: entry.rel_path.clone(),
        root_id: root.id,
        sha256,
        pages,
        chapter: resolved.chapter,
        title: resolved.title,
    });
```

- [ ] **Step 4: Write `write_roots`**

Add immediately after `write_chapters` in `crates/taiyaku-library/src/derive.rs`:

```rust
/// Write each member's root onto the rows `import::scanned` just wrote.
///
/// **The same shape as [`write_chapters`], and for the same reason**: the
/// sidecar format has no field for a root and must not gain one, because a
/// scanned work has no document at all. `derive` writes what the document
/// cannot carry, as a targeted update after the import — see this module's
/// own header for the rule.
///
/// **Unconditional, where `write_chapters` is scoped by `chapter_source`.**
/// A chapter number can have been entered by hand and must survive a
/// re-derive; a root is a fact about where a file was walked from, and
/// nothing but a scan can know it. So every member of every edition this
/// derive wrote is rewritten, which is what makes a root removed from a rule
/// set stop being claimed.
///
/// Matched on `(edition key, ordinal)`, which is where the members this call
/// just described actually landed: `replace_editions` inserts them in
/// document order counting from 0, and `project.rs` reads them back
/// `ORDER BY ordinal`.
fn write_roots(conn: &Connection, work_id: i64, group: &Group) -> Result<(), String> {
    for (key, candidates) in &group.editions {
        let mut ordered: Vec<&Candidate> = candidates.iter().collect();
        ordered.sort_by(|a, b| taiyaku_read::natural_cmp(&a.rel_path, &b.rel_path));
        for (ordinal, candidate) in (0_i64..).zip(ordered) {
            conn.execute(
                "UPDATE member SET root_id = ?1
                  WHERE ordinal = ?2
                    AND edition_id = (SELECT id FROM edition
                                       WHERE work_id = ?3 AND key = ?4)",
                rusqlite::params![candidate.root_id, ordinal, work_id, key],
            )
            .map_err(|e| {
                format!("recording the root of `{key}` member {ordinal} on work {work_id}: {e}")
            })?;
        }
    }
    Ok(())
}
```

- [ ] **Step 5: Call it from `write_work`**

In `write_work`, immediately after the existing `write_chapters` call (around line 988):

```rust
    write_chapters(conn, work_id, group)?;
    write_roots(conn, work_id, group)?;
    report.works += 1;
    Ok(())
```

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cargo test -p taiyaku-library derive:: -- --nocapture`

Expected: PASS, both new tests and every existing `derive` test.

- [ ] **Step 7: Check the derive still opens no file it did not have to**

Run: `cargo test -p taiyaku-library derive::tests -- --nocapture 2>&1 | tail -20`

Expected: PASS. `derive/tests.rs` deletes the scan tree to prove a re-derive touches no filesystem; `write_roots` reads only `Candidate`, so that property is unchanged. If a test that deletes the tree now fails, `write_roots` has reached for something it should not.

- [ ] **Step 8: Commit**

```bash
git add crates/taiyaku-library/src/derive.rs crates/taiyaku-library/src/derive/tests.rs
git commit -m "derive writes member.root_id, beside the chapter it already writes"
```

---

### Task 3: `account::grant_root`, `revoke_root`, `granted_roots`

**Files:**
- Modify: `crates/taiyaku-library/src/account.rs` (after `roots_for`, line 140)
- Test: `crates/taiyaku-library/src/account/tests.rs`

**Interfaces:**
- Consumes: `user_root` from migration 4, `root` from migration 1.
- Produces:
  - `pub fn grant_root(conn: &Connection, user_id: i64, root_id: i64) -> Result<bool, String>` — `true` if the grant was new.
  - `pub fn revoke_root(conn: &Connection, user_id: i64, root_id: i64) -> Result<bool, String>` — `true` if a grant was removed.
  - `pub fn granted_roots(conn: &Connection, user_id: i64) -> Result<Vec<(i64, String)>, String>` — `(root_id, path)` for display, ordered by path.

- [ ] **Step 1: Write the failing tests**

Append to `crates/taiyaku-library/src/account/tests.rs`:

```rust
fn a_root(conn: &rusqlite::Connection, id: i64, path: &str) {
    conn.execute(
        "INSERT INTO root (id, path) VALUES (?1, ?2)",
        rusqlite::params![id, path],
    )
    .expect("adding a root");
}

#[test]
fn granting_a_root_is_idempotent_and_says_which_it_was() {
    let library = library();
    let conn = library.conn();
    a_root(&conn, 7, "/library");

    assert!(grant_root(&conn, 1, 7).expect("grants"), "the first grant is new");
    assert!(
        !grant_root(&conn, 1, 7).expect("grants again"),
        "granting twice is not an error and is not a second grant"
    );
    assert_eq!(roots_for(&conn, 1).expect("reads"), vec![7]);
}

#[test]
fn revoking_a_root_says_whether_there_was_one() {
    let library = library();
    let conn = library.conn();
    a_root(&conn, 7, "/library");
    grant_root(&conn, 1, 7).expect("grants");

    assert!(revoke_root(&conn, 1, 7).expect("revokes"), "the grant was there");
    assert!(
        !revoke_root(&conn, 1, 7).expect("revokes again"),
        "revoking what is not granted is not an error"
    );
    assert!(roots_for(&conn, 1).expect("reads").is_empty());
}

/// **The grant is the restriction**, so an account with none is unrestricted
/// and this is the state every existing library is in — nothing could write
/// `user_root` before spec C1. Pinned here rather than only at the route,
/// because it is the property the migration depends on.
#[test]
fn an_account_with_no_grant_has_an_empty_list_rather_than_an_error() {
    let library = library();
    let conn = library.conn();
    assert!(roots_for(&conn, 1).expect("reads").is_empty());
}

#[test]
fn granted_roots_names_the_paths_for_display() {
    let library = library();
    let conn = library.conn();
    a_root(&conn, 7, "/b-library");
    a_root(&conn, 8, "/a-library");
    grant_root(&conn, 1, 7).expect("grants");
    grant_root(&conn, 1, 8).expect("grants");

    assert_eq!(
        granted_roots(&conn, 1).expect("reads"),
        vec![(8, "/a-library".to_string()), (7, "/b-library".to_string())],
        "ordered by path, which is what a person reads"
    );
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p taiyaku-library account::tests::granting -- --nocapture`

Expected: FAIL — `cannot find function grant_root in this scope`.

- [ ] **Step 3: Write the three functions**

Add to `crates/taiyaku-library/src/account.rs` after `roots_for`:

```rust
/// Grant `user_id` sight of `root_id`.
///
/// **Granting is restricting**, which is the counter-intuitive half of spec
/// C1 §5.1 and is worth stating where the writer lives: an account with *no*
/// grants sees every root, because that is the state every account in every
/// existing library is in and any other reading would empty every library the
/// day migration 6 applied. The first grant an account receives is therefore
/// the moment it stops seeing everything.
///
/// `INSERT OR IGNORE` rather than a probe and an insert: the table is
/// `WITHOUT ROWID` with `(user_id, root_id)` as its key, so the conflict is
/// exactly the case that means "already granted". The return says which
/// happened, so a CLI can print the truth rather than a cheerful lie.
///
/// # Errors
///
/// Returns an error if the insert fails — which includes a `root_id` no root
/// row has, since the column REFERENCES `root(id)`.
pub fn grant_root(conn: &Connection, user_id: i64, root_id: i64) -> Result<bool, String> {
    let changed = conn
        .execute(
            "INSERT OR IGNORE INTO user_root (user_id, root_id) VALUES (?1, ?2)",
            params![user_id, root_id],
        )
        .map_err(|e| format!("granting root {root_id} to user {user_id}: {e}"))?;
    Ok(changed == 1)
}

/// Take back a grant, reporting whether there was one.
///
/// **Revoking the last grant makes the account unrestricted again**, not
/// blind — see [`grant_root`]. That is a sharp edge and it is the same one in
/// both directions: an operator narrowing an account to one root and then
/// removing that root has widened it to everything.
///
/// # Errors
///
/// Returns an error if the delete fails.
pub fn revoke_root(conn: &Connection, user_id: i64, root_id: i64) -> Result<bool, String> {
    let changed = conn
        .execute(
            "DELETE FROM user_root WHERE user_id = ?1 AND root_id = ?2",
            params![user_id, root_id],
        )
        .map_err(|e| format!("revoking root {root_id} from user {user_id}: {e}"))?;
    Ok(changed == 1)
}

/// The roots granted to `user_id`, as `(id, path)`, ordered by path.
///
/// [`roots_for`] answers the same question in the shape the guard wants — ids
/// alone, resolved on every request. This one is for a person reading a
/// terminal, and joins `root` for the path.
///
/// # Errors
///
/// Returns an error if the query fails.
pub fn granted_roots(conn: &Connection, user_id: i64) -> Result<Vec<(i64, String)>, String> {
    let mut statement = conn
        .prepare(
            "SELECT r.id, r.path FROM user_root ur
               JOIN root r ON r.id = ur.root_id
              WHERE ur.user_id = ?1 ORDER BY r.path",
        )
        .map_err(|e| format!("preparing the granted-roots listing: {e}"))?;
    let rows = statement
        .query_map(params![user_id], |row| Ok((row.get(0)?, row.get(1)?)))
        .map_err(|e| format!("listing granted roots: {e}"))?;
    rows.collect::<rusqlite::Result<Vec<(i64, String)>>>()
        .map_err(|e| format!("listing granted roots: {e}"))
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p taiyaku-library account:: -- --nocapture`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/taiyaku-library/src/account.rs crates/taiyaku-library/src/account/tests.rs
git commit -m "account: grant_root, revoke_root and the listing a person reads"
```

---

### Task 4: `taiyaku library user grant-root` / `revoke-root`

**Files:**
- Modify: `crates/taiyaku-cli/src/user.rs` — `command` dispatch (line 30), a `ROOT_USAGE` constant, two functions
- Test: `crates/taiyaku-cli/src/user/tests.rs`

**Interfaces:**
- Consumes: `account::grant_root`, `account::revoke_root`, `account::granted_roots`, `account::id_of` from Task 3; `scan::roots` for resolving a path to a root id.
- Produces: `taiyaku library user grant-root <user> <root-path> [--db PATH]` and `revoke-root` with the same shape. Both print what changed.

**Why this is here rather than in C4:** spec C1 §5.1. Without a writer, the root filter is a permission check that can only be exercised against fixtures. Two subcommands on a command spec A2 already shipped is the smallest crossing that makes it real.

- [ ] **Step 1: Write the failing test**

Append to `crates/taiyaku-cli/src/user/tests.rs`:

```rust
/// The root is named by **path**, not by id: a person adding a root typed a
/// path (`library scan --add PATH`) and has no reason to know its rowid.
#[test]
fn granting_a_root_by_path_resolves_it_to_the_row() {
    let dir = crate::tests::tempdir("cli-grant-root");
    let db = dir.path.join("library.db");
    let root_dir = dir.path.join("shelf");
    std::fs::create_dir_all(&root_dir).expect("makes the root");

    let library = taiyaku_library::Library::open(&db).expect("opens");
    let root = {
        let conn = library.conn();
        taiyaku_library::account::add(&conn, "alice", "hunter2hunter2", "user").expect("adds");
        taiyaku_library::scan::add_root(&conn, &root_dir).expect("adds the root")
    };
    drop(library);

    grant_root(&[
        "alice".to_string(),
        root_dir.to_string_lossy().into_owned(),
        "--db".to_string(),
        db.to_string_lossy().into_owned(),
    ])
    .expect("grants");

    let library = taiyaku_library::Library::open(&db).expect("reopens");
    let conn = library.conn();
    let alice = taiyaku_library::account::id_of(&conn, "alice")
        .expect("looks up")
        .expect("alice exists");
    assert_eq!(
        taiyaku_library::account::roots_for(&conn, alice).expect("reads"),
        vec![root.id]
    );
}

#[test]
fn granting_a_root_this_library_does_not_hold_names_the_path() {
    let dir = crate::tests::tempdir("cli-grant-unknown-root");
    let db = dir.path.join("library.db");
    let library = taiyaku_library::Library::open(&db).expect("opens");
    {
        let conn = library.conn();
        taiyaku_library::account::add(&conn, "alice", "hunter2hunter2", "user").expect("adds");
    }
    drop(library);

    let why = grant_root(&[
        "alice".to_string(),
        "/not/a/root".to_string(),
        "--db".to_string(),
        db.to_string_lossy().into_owned(),
    ])
    .expect_err("refuses");
    assert!(why.contains("/not/a/root"), "the message names the path: {why}");
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p taiyaku-cli user::tests::granting_a_root -- --nocapture`

Expected: FAIL — `cannot find function grant_root in this scope`.

- [ ] **Step 3: Add the usage constant and the two functions**

In `crates/taiyaku-cli/src/user.rs`, add beside `KEY_USAGE`:

```rust
const ROOT_USAGE: &str = "usage: taiyaku library user grant-root <user> <root-path> [--db PATH]\n \
                          taiyaku library user revoke-root <user> <root-path> [--db PATH]";
```

and, before the `#[cfg(test)] mod tests;` at the end:

```rust
/// The root row whose canonicalised path is `given`, or an error naming what
/// was asked for.
///
/// **Canonicalised on the way in**, because `scan::add_root` canonicalises on
/// the way in too — a root added as `./shelf` is stored absolute, and a grant
/// typed the same way would otherwise miss. A path that does not exist cannot
/// be a root of this library, so the canonicalisation failing is the same
/// answer as no row matching, phrased with the reason.
fn root_by_path(conn: &rusqlite::Connection, given: &str) -> Result<i64, String> {
    let wanted = std::path::Path::new(given)
        .canonicalize()
        .map_err(|e| format!("`{given}` is not a root of this library ({e})"))?;
    taiyaku_library::scan::roots(conn)?
        .into_iter()
        .find(|root| root.path == wanted)
        .map(|root| root.id)
        .ok_or_else(|| {
            format!(
                "`{given}` is not a root of this library; \
                 `taiyaku library scan --add {given}` would make it one"
            )
        })
}

fn grant_root(args: &[String]) -> Result<(), String> {
    reject_unknown_flags("library user grant-root", args, &["--db"])?;
    let given = positional(args);
    let [user, path] = given.as_slice() else {
        return Err(ROOT_USAGE.to_string());
    };
    let library = open(args)?;
    let conn = library.conn();
    let user_id =
        account::id_of(&conn, user)?.ok_or_else(|| format!("no account named `{user}`"))?;
    let root_id = root_by_path(&conn, path)?;
    let was_new = account::grant_root(&conn, user_id, root_id)?;
    let held = account::granted_roots(&conn, user_id)?;
    if was_new {
        println!("granted `{user}` sight of {path}");
    } else {
        println!("`{user}` could already see {path}; nothing to do");
    }
    // **Said every time, because the rule is counter-intuitive**: an account
    // with no grants sees everything, so the FIRST grant is the one that
    // narrows it. An operator who grants one root expecting to widen access
    // has just done the opposite, and the only moment to say so is here.
    if held.len() == 1 {
        println!(
            "  `{user}` is now restricted to that one root. \
             An account with no grants at all sees every root."
        );
    }
    Ok(())
}

fn revoke_root(args: &[String]) -> Result<(), String> {
    reject_unknown_flags("library user revoke-root", args, &["--db"])?;
    let given = positional(args);
    let [user, path] = given.as_slice() else {
        return Err(ROOT_USAGE.to_string());
    };
    let library = open(args)?;
    let conn = library.conn();
    let user_id =
        account::id_of(&conn, user)?.ok_or_else(|| format!("no account named `{user}`"))?;
    let root_id = root_by_path(&conn, path)?;
    if account::revoke_root(&conn, user_id, root_id)? {
        println!("revoked `{user}`'s sight of {path}");
    } else {
        println!("`{user}` had no grant for {path}; nothing to revoke");
    }
    if account::granted_roots(&conn, user_id)?.is_empty() {
        println!("  `{user}` now holds no grants, and so sees every root again.");
    }
    Ok(())
}
```

- [ ] **Step 4: Wire them into the dispatch**

Replace `command`'s `match` in `crates/taiyaku-cli/src/user.rs`:

```rust
pub(crate) fn command(args: &[String]) -> Result<(), String> {
    match args.first().map(String::as_str) {
        Some("promote") => promote(&args[1..]),
        Some("add") => add(&args[1..]),
        Some("password") => password(&args[1..]),
        Some("list") => list(&args[1..]),
        Some("grant-root") => grant_root(&args[1..]),
        Some("revoke-root") => revoke_root(&args[1..]),
        Some(other) => Err(format!(
            "library user has no subcommand `{other}`; it has promote, add, password, list, \
             grant-root and revoke-root"
        )),
        None => Err("library user needs a subcommand: promote, add, password, list, grant-root \
                     or revoke-root"
            .to_string()),
    }
}
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p taiyaku-cli user:: -- --nocapture`

Expected: PASS. A pre-existing test asserting the "no subcommand" message will need its expected string widened — that is the intended failure, not a regression.

- [ ] **Step 6: Exercise it by hand, against a real root**

```bash
cargo run -q -p taiyaku-cli -- library user grant-root --help 2>&1 | head -3 || true
```

Expected: the `ROOT_USAGE` text. This is the smoke test that the dispatch is reachable at all.

- [ ] **Step 7: Commit**

```bash
git add crates/taiyaku-cli/src/user.rs crates/taiyaku-cli/src/user/tests.rs
git commit -m "library user grant-root and revoke-root, so the filter has a writer"
```

---

### Task 5: `serve --db` — the reader reaching a scanned work

**Files:**
- Modify: `crates/taiyaku-serve/src/works.rs` — restore `from_library`
- Modify: `crates/taiyaku-serve/src/lib.rs:40-51` — replace the "no `from_library` here" comment with what was built
- Modify: `crates/taiyaku-cli/src/main.rs:1410` — `serve_works` accepting an empty path list
- Test: `crates/taiyaku-serve/src/works/tests.rs`

**Interfaces:**
- Consumes: `taiyaku_library::project::sidecar`, `member.root_id` (not yet read here).
- Produces: `pub fn from_library(library: &taiyaku_library::Library) -> Result<Vec<Work>, String>` — every `work` row as a servable `Work`, with a NULL `sidecar_path` becoming an empty `PathBuf`.

**This is spec C1 §5.3.** Without it the browse UI is a grid of covers that open nothing.

- [ ] **Step 1: Write the failing test**

Append to `crates/taiyaku-serve/src/works/tests.rs`:

```rust
/// A scanned work has no `sidecar_path` — `import::scanned` leaves it NULL by
/// design — and must still become a servable `Work`.
///
/// Its member paths are absolute, which is what makes this work at all:
/// `derive` writes them from the root down, and `root.join(&member.path)`
/// returns an absolute path unchanged.
#[test]
fn a_work_with_no_sidecar_path_still_becomes_a_servable_work() {
    let library = taiyaku_library::Library::open_in_memory().expect("in-memory");
    {
        let conn = library.conn();
        let json = r#"{"taiyaku":1,"work":{"title":"Scanned","reading":"rtl"},
            "editions":{"raw":{"label":"raw","members":[
              {"path":"/abs/shelf/Series - v01","sha256":"deadbeef","pages":2}]}},
            "hub":"raw","alignments":[]}"#;
        let doc = taiyaku_core::Sidecar::from_json(json).expect("parses");
        taiyaku_library::import::scanned(&conn, &doc, None).expect("imports as a scan");
    }

    let works = crate::works::from_library(&library).expect("builds works");
    assert_eq!(works.len(), 1);
    assert_eq!(works[0].sidecar().work.title, "Scanned");
    assert_eq!(
        works[0].path,
        std::path::PathBuf::new(),
        "no sidecar path becomes an empty path, which `has_document` reads as none"
    );
    assert!(!works[0].has_document(), "a scanned work has no document");
}

#[test]
fn a_work_with_a_sidecar_path_keeps_it_and_has_a_document() {
    let library = taiyaku_library::Library::open_in_memory().expect("in-memory");
    {
        let conn = library.conn();
        let json = r#"{"taiyaku":1,"work":{"title":"Imported","reading":"rtl"},
            "editions":{"raw":{"label":"raw","members":[
              {"path":"raw/v01.cbz","sha256":"deadbeef","pages":2}]}},
            "hub":"raw","alignments":[]}"#;
        let doc = taiyaku_core::Sidecar::from_json(json).expect("parses");
        taiyaku_library::import::sidecar(&conn, &doc, std::path::Path::new("/x/taiyaku.json"))
            .expect("imports");
    }

    let works = crate::works::from_library(&library).expect("builds works");
    assert_eq!(works.len(), 1);
    assert_eq!(works[0].path, std::path::PathBuf::from("/x/taiyaku.json"));
    assert!(works[0].has_document());
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p taiyaku-serve works::tests::a_work_with_no_sidecar_path -- --nocapture`

Expected: FAIL — `cannot find function from_library`.

- [ ] **Step 3: Add `has_document` to `Work`**

In `crates/taiyaku-serve/src/works.rs`, inside `impl Work`:

```rust
    /// Whether this work has a `taiyaku.json` on disk.
    ///
    /// **The one question spec C1 §5.3's decision turns on.** A scanned work
    /// has no document — `import::scanned` leaves `sidecar_path` NULL by
    /// design, since one `taiyaku.json` per directory cannot describe fourteen
    /// volumes sitting beside each other — so the alignment route writes its
    /// rows and skips the file rather than writing to an empty path.
    ///
    /// An empty `PathBuf` is the in-memory spelling of that NULL. It is not a
    /// sentinel anybody could reach by accident: every real path a work is
    /// built from went through `canonical_sidecar_path`, which requires the
    /// file to exist.
    #[must_use]
    pub fn has_document(&self) -> bool {
        !self.path.as_os_str().is_empty()
    }
```

- [ ] **Step 4: Write `from_library`**

Add to `crates/taiyaku-serve/src/works.rs`:

```rust
/// Every `work` row in `library`, as a servable [`Work`].
///
/// **Restored deliberately, and this comment replaces the one that explained
/// why it was deleted.** It was removed in whole-branch review 2 as a
/// projection nothing ran: `taiyaku serve <sidecar>...` builds its works one
/// path at a time in `taiyaku-cli`'s `serve_work`, because it serves the works
/// it was *handed*. Spec C1 is the caller that lib.rs predicted — `serve --db`
/// with no paths at all — and a browse UI over a scanned library needs it,
/// since a scanned work has no path to be handed.
///
/// **A NULL `sidecar_path` becomes an empty `PathBuf`**, which
/// [`Work::has_document`] reads as "no document". See spec C1 §5.3 for what
/// that costs at the one route that writes a file.
///
/// # Errors
///
/// Returns an error if the rows cannot be read, or if a work's document
/// cannot be projected back out of them — which means the rows for that work
/// are inconsistent, and is worth refusing rather than serving.
pub fn from_library(library: &taiyaku_library::Library) -> Result<Vec<Work>, String> {
    let conn = library.conn();
    let mut stmt = conn
        .prepare("SELECT id, slug, sidecar_path FROM work ORDER BY id")
        .map_err(|e| format!("listing the library's works: {e}"))?;
    let rows: Vec<(i64, String, Option<String>)> = stmt
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
        .map_err(|e| format!("listing the library's works: {e}"))?
        .collect::<rusqlite::Result<_>>()
        .map_err(|e| format!("reading the library's works: {e}"))?;

    rows.into_iter()
        .map(|(id, slug, sidecar_path)| {
            let sidecar = taiyaku_library::project::sidecar(&conn, id)
                .map_err(|e| format!("projecting work {id} (`{slug}`): {e}"))?;
            let path = sidecar_path.map_or_else(std::path::PathBuf::new, std::path::PathBuf::from);
            Ok(Work::new(slug, path, sidecar))
        })
        .collect()
}
```

- [ ] **Step 5: Rewrite the stale comment in `lib.rs`**

Replace the comment block at `crates/taiyaku-serve/src/lib.rs:40-50` with:

```rust
// `from_library` is spec C1 §5.3's answer to the question this comment used to
// leave open. It was deleted in whole-branch review 2 as a projection nothing
// ran — `taiyaku serve <sidecar>...` builds its works one path at a time in
// `taiyaku-cli`'s `serve_work` — and the note left behind predicted the caller
// that would want it back: "a `serve --db` with no paths". That is this.
//
// What it decides about a work whose `sidecar_path` is NULL: the path becomes
// empty, `Work::has_document` reads that as "no document", and the alignment
// route writes rows without a file. Nothing else in `serve` reads `Work.path`
// except two error messages and the bridge row, which a work read from its own
// row never needs.
pub use works::{Work, from_library, load_works, work_id};
```

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cargo test -p taiyaku-serve works:: -- --nocapture`

Expected: PASS.

- [ ] **Step 7: Let `serve` be invoked with no paths**

In `crates/taiyaku-cli/src/main.rs`, in `serve_works`, replace the body's opening so an empty `paths` reads the library instead:

```rust
fn serve_works(
    paths: &[PathBuf],
    db_flag: Option<&str>,
) -> Result<(taiyaku_library::Library, Vec<taiyaku_serve::Work>), String> {
    // **No paths means the whole library** — spec C1 §5.3. The scanner
    // produces works with no `sidecar_path` at all, so there is nothing to put
    // on a command line; `serve --db` is how a browse UI's `/read/{work}` link
    // reaches one. Handed paths still behave exactly as before: they are
    // canonicalised, imported, and serve only what was named.
    if paths.is_empty() {
        let db = library::resolve_db_path(db_flag)?;
        let db_library =
            taiyaku_library::Library::open(&db).map_err(|e| format!("opening the library: {e}"))?;
        let works = taiyaku_serve::from_library(&db_library)?;
        if works.is_empty() {
            return Err(format!(
                "{} holds no works. `taiyaku library scan --add PATH` reads a shelf into it, \
                 or name a sidecar on the command line",
                db.display()
            ));
        }
        return Ok((db_library, works));
    }

    let canonical_paths: Vec<PathBuf> = paths
        .iter()
        .map(|p| library::canonical_sidecar_path(p))
        .collect::<Result<_, _>>()?;
    // …the existing body continues unchanged from here…
```

- [ ] **Step 8: Confirm `serve` no longer refuses an empty argument list**

Find the caller in `serve` (`crates/taiyaku-cli/src/main.rs:1229`) that refuses when no paths were given, and remove that refusal — `serve_works` now answers for both cases, including the "library holds no works" message above. Run:

```bash
cargo test -p taiyaku-cli serve -- --nocapture
```

Expected: PASS. A pre-existing test asserting "serve needs at least one path" is the intended failure; update it to assert the new behaviour — an empty invocation reads the library.

- [ ] **Step 9: Commit**

```bash
git add crates/taiyaku-serve/src/works.rs crates/taiyaku-serve/src/works/tests.rs crates/taiyaku-serve/src/lib.rs crates/taiyaku-cli/src/main.rs
git commit -m "serve --db: the reader reaching a work the scanner produced"
```

---

### Task 6: A hand correction on a work with no file

**Files:**
- Modify: `crates/taiyaku-serve/src/routes.rs:1337-1342`
- Test: `crates/taiyaku-serve/src/routes/tests.rs`

**Interfaces:**
- Consumes: `Work::has_document` from Task 5.
- Produces: `patch_alignments` writing `alignment`/`run` rows with no file for a work that has none, and no `taiyaku.json` appearing beside it.

**This is spec C1 §5.3's decision.** The route writes the file *before* the rows, deliberately — the file is the copy a person can read, diff and back up. With no file, the rows are written alone.

- [ ] **Step 1: Write the failing test**

Append to `crates/taiyaku-serve/src/routes/tests.rs`:

```rust
/// A correction on a scanned work writes rows and **no file**.
///
/// The assertion that matters is the second one: the server must not mint a
/// `taiyaku.json` into a tree spec B took care never to open for writing.
#[tokio::test]
async fn correcting_a_scanned_work_writes_rows_and_no_file() {
    let dir = crate::test_support::tempdir("patch-scanned");
    let shelf = dir.path.join("shelf");
    std::fs::create_dir_all(&shelf).expect("makes the shelf");

    let state = crate::test_support::scanned_state(&shelf);
    let before: Vec<_> = std::fs::read_dir(&shelf)
        .expect("reads the shelf")
        .map(|e| e.expect("entry").file_name())
        .collect();

    let response = crate::test_support::patch_alignment(&state, "scanned-v01").await;
    assert_eq!(response.status(), axum::http::StatusCode::OK);

    let after: Vec<_> = std::fs::read_dir(&shelf)
        .expect("reads the shelf")
        .map(|e| e.expect("entry").file_name())
        .collect();
    assert_eq!(before, after, "no taiyaku.json appeared beside the work");

    let conn = state.library.conn();
    let runs: i64 = conn
        .query_row("SELECT COUNT(*) FROM run", [], |row| row.get(0))
        .expect("counting runs");
    assert!(runs > 0, "the correction reached the rows");
}
```

Add the two fixtures it needs to `crates/taiyaku-serve/src/test_support.rs`: `scanned_state(shelf: &Path) -> Arc<ServeState>`, which imports one work through `import::scanned` (so `sidecar_path` is NULL) with two editions whose member paths are absolute under `shelf`, and `patch_alignment(state, slug)`, which drives the existing `/api/works/{work}/alignments` route with a one-pair correction body through `oneshot`. Model both on the fixtures already in that file — `works_in` for the import shape, and whichever helper the existing alignment tests use to build a request.

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p taiyaku-serve correcting_a_scanned_work -- --nocapture`

Expected: FAIL — the route calls `atomic::write` on an empty path and answers 500.

- [ ] **Step 3: Skip the file write when there is no file**

In `crates/taiyaku-serve/src/routes.rs`, replace the write block at line 1337:

```rust
            let json = next.to_json();
            if what != taiyaku_core::Corrected::AlreadyTrue {
                // **The file, then the rows — unless there is no file.**
                //
                // The ordering below is argued deliberately and is unchanged
                // for a work that has a document: the file is the copy a
                // person can read, diff and back up, and if the row write
                // fails after it the correction is recoverable, because the
                // next start imports that file.
                //
                // A scanned work has no document at all — `import::scanned`
                // leaves `sidecar_path` NULL, since one `taiyaku.json` per
                // directory cannot describe fourteen volumes sitting beside
                // each other — so there is no file to be first. The rows are
                // written alone, and spec C1 §5.3 states the cost plainly:
                // for such a work the correction's only copy is in the
                // database until somebody runs `library export`. That is the
                // outcome this same comment declines to accept for a
                // document-backed work, and it remains the right refusal
                // there, because there the file exists.
                //
                // Minting a `taiyaku.json` here instead was rejected: it would
                // make the server a writer into a tree spec B never opens for
                // writing, and hand `import`'s `sidecar_path` reconciliation a
                // second writer to race.
                if found.has_document() {
                    taiyaku_read::atomic::write(&found.path, json.as_bytes(), "json")
                        .map_err(|why| (StatusCode::INTERNAL_SERVER_ERROR, why).into_response())?;
                }
                crate::alignments::store(&conn, found, &next)
                    .map_err(|why| (StatusCode::INTERNAL_SERVER_ERROR, why).into_response())?;
            }
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p taiyaku-serve routes:: -- --nocapture`

Expected: PASS, including every existing alignment test — a document-backed work still writes its file first.

- [ ] **Step 5: Commit**

```bash
git add crates/taiyaku-serve/src/routes.rs crates/taiyaku-serve/src/routes/tests.rs crates/taiyaku-serve/src/test_support.rs
git commit -m "A correction on a work with no document writes rows and no file"
```

---

### Task 7: `browse.rs` — the store queries, with the root filter in the SQL

**Files:**
- Create: `crates/taiyaku-library/src/browse.rs`, `crates/taiyaku-library/src/browse/tests.rs`
- Modify: `crates/taiyaku-library/src/lib.rs` — `pub mod browse;`

**Interfaces:**
- Consumes: `member.root_id` (Task 2), `series`, `series_title`, `work`, `edition`, `member`, `alignment`, `run`, `progress`, `series_fts`, `work_fts`.
- Produces:

```rust
pub struct SeriesCard { pub id: i64, pub title: String, pub works: i64,
                        pub editions: i64, pub aligned: i64, pub cover_work: Option<i64> }
pub struct WorkCard { pub id: i64, pub slug: String, pub title: String,
                      pub volume: Option<f64>, pub editions: Vec<EditionCard>,
                      pub aligned: bool, pub pages: i64 }
pub struct EditionCard { pub key: String, pub label: String, pub members: i64, pub pages: i64 }
pub struct Home { pub continuing: Vec<WorkCard>, pub added: Vec<WorkCard>, pub read: Vec<WorkCard> }

pub fn series(conn: &Connection, roots: &[i64]) -> Result<Vec<SeriesCard>, String>;
pub fn unfiled(conn: &Connection, roots: &[i64]) -> Result<Vec<WorkCard>, String>;
pub fn one_series(conn: &Connection, roots: &[i64], id: i64) -> Result<Option<(SeriesCard, Vec<WorkCard>)>, String>;
pub fn work(conn: &Connection, roots: &[i64], slug: &str) -> Result<Option<WorkCard>, String>;
pub fn search(conn: &Connection, roots: &[i64], q: &str) -> Result<SearchResults, String>;
pub fn home(conn: &Connection, roots: &[i64], user_id: i64) -> Result<Home, String>;
pub fn cover_work(conn: &Connection, roots: &[i64], series_id: i64) -> Result<Option<i64>, String>;
```

Every function takes `roots` and applies it in SQL. **`roots` empty means no filter.**

- [ ] **Step 1: Write the failing tests**

Create `crates/taiyaku-library/src/browse/tests.rs`:

```rust
use super::*;
use crate::{Library, derive, scan};
use crate::scan::tests::write_pages;

/// Two roots, one series, one volume in each — the shape spec C1 §5.1 is
/// designed around, and the only one where the filter can be seen working.
fn two_root_library() -> (Library, i64, i64, crate::tests::TempDir, crate::tests::TempDir) {
    let en = crate::tests::tempdir("browse-en");
    write_pages(&en.path.join("Series").join("Series - v01"), 2);
    let raw = crate::tests::tempdir("browse-raw");
    write_pages(&raw.path.join("Series").join("Series - v01"), 2);

    let library = Library::open_in_memory().expect("in-memory");
    let (en_id, raw_id) = {
        let conn = library.conn();
        let en_root = scan::add_root(&conn, &en.path).expect("adds en");
        let raw_root = scan::add_root(&conn, &raw.path).expect("adds raw");
        scan::scan_root(&conn, &en_root).expect("scans en");
        scan::scan_root(&conn, &raw_root).expect("scans raw");
        derive::derive(&conn).expect("derives");
        (en_root.id, raw_root.id)
    };
    (library, en_id, raw_id, en, raw)
}

#[test]
fn an_empty_grant_sees_every_root() {
    let (library, _, _, _en, _raw) = two_root_library();
    let conn = library.conn();
    let cards = series(&conn, &[]).expect("lists");
    assert_eq!(cards.len(), 1, "one series");
    assert_eq!(cards[0].editions, 2, "both editions are visible");
}

#[test]
fn a_grant_hides_the_editions_it_does_not_name_and_keeps_the_work() {
    let (library, en_id, _raw_id, _en, _raw) = two_root_library();
    let conn = library.conn();
    let cards = series(&conn, &[en_id]).expect("lists");
    assert_eq!(cards.len(), 1, "the work is still listed");
    assert_eq!(cards[0].editions, 1, "only the granted root's edition");
}

/// An imported work is **`unfiled`, not a series** — and this test exists
/// because an earlier draft of this plan asserted it was a second series and
/// was wrong. `import.rs` never writes `series_id`, so no series row is
/// created and `series()` returns one either way. Spec C1 §5.4.
///
/// **NULL excludes itself**, which is what lets it need no special case: an
/// imported member has `root_id` NULL, so a restricted account does not see
/// it, and an unrestricted one does.
#[test]
fn an_imported_work_is_unfiled_rather_than_a_series_of_its_own() {
    let (library, en_id, _raw_id, _en, _raw) = two_root_library();
    {
        let conn = library.conn();
        let json = r#"{"taiyaku":1,"work":{"title":"Imported","reading":"rtl"},
            "editions":{"raw":{"label":"raw","members":[
              {"path":"raw/v01.cbz","sha256":"cafebabe","pages":2}]}},
            "hub":"raw","alignments":[]}"#;
        let doc = taiyaku_core::Sidecar::from_json(json).expect("parses");
        crate::import::sidecar(&conn, &doc, std::path::Path::new("/x/taiyaku.json"))
            .expect("imports");
    }
    let conn = library.conn();
    assert_eq!(
        series(&conn, &[]).expect("lists").len(),
        1,
        "an import creates no series, so the grid still holds one"
    );
    assert_eq!(
        unfiled(&conn, &[]).expect("lists").len(),
        1,
        "unrestricted sees the imported work in the unfiled bucket"
    );
    assert!(
        unfiled(&conn, &[en_id]).expect("lists").is_empty(),
        "a restricted account sees no work that sits under no root"
    );
}

/// Search reads **both** FTS indexes, so a work is findable by its own title
/// even when it belongs to no series.
///
/// `work_fts` has been populated by triggers since migration 1 with nothing
/// reading it; searching `series_fts` alone was this plan's first draft and
/// left every imported work unfindable.
#[test]
fn search_finds_an_unfiled_work_by_its_own_title() {
    let (library, _, _, _en, _raw) = two_root_library();
    {
        let conn = library.conn();
        let json = r#"{"taiyaku":1,"work":{"title":"Hanayome","reading":"rtl"},
            "editions":{"raw":{"label":"raw","members":[
              {"path":"raw/v01.cbz","sha256":"cafebabe","pages":2}]}},
            "hub":"raw","alignments":[]}"#;
        let doc = taiyaku_core::Sidecar::from_json(json).expect("parses");
        crate::import::sidecar(&conn, &doc, std::path::Path::new("/x/taiyaku.json"))
            .expect("imports");
    }
    let conn = library.conn();
    let hits = search(&conn, &[], "Hanayome").expect("searches");
    assert!(hits.series.is_empty(), "no series is named Hanayome");
    assert_eq!(hits.unfiled.len(), 1, "the work itself is the hit");
    assert_eq!(hits.unfiled[0].title, "Hanayome");
}

#[test]
fn a_work_is_found_by_slug_within_the_grant() {
    let (library, en_id, raw_id, _en, _raw) = two_root_library();
    let conn = library.conn();
    let slug: String = conn
        .query_row("SELECT slug FROM work LIMIT 1", [], |row| row.get(0))
        .expect("reads a slug");
    assert!(work(&conn, &[], &slug).expect("reads").is_some());
    assert!(work(&conn, &[en_id], &slug).expect("reads").is_some());
    assert!(work(&conn, &[raw_id], &slug).expect("reads").is_some());
}

#[test]
fn search_finds_a_series_by_its_title() {
    let (library, _, _, _en, _raw) = two_root_library();
    let conn = library.conn();
    let hits = search(&conn, &[], "Series").expect("searches");
    assert_eq!(hits.series.len(), 1);
    assert!(hits.unfiled.is_empty());
}

/// The cover falls back to the lowest-volume work while `cover_work_id` is
/// NULL, which it is for every series in every library today — spec C1 §5.2.
#[test]
fn the_cover_falls_back_to_the_lowest_volume_when_none_is_set() {
    let (library, _, _, _en, _raw) = two_root_library();
    let conn = library.conn();
    let series_id: i64 = conn
        .query_row("SELECT id FROM series LIMIT 1", [], |row| row.get(0))
        .expect("reads the series");
    assert!(cover_work(&conn, &[], series_id).expect("reads").is_some());
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p taiyaku-library browse:: -- --nocapture`

Expected: FAIL — `unresolved import crate::browse` / module does not exist.

- [ ] **Step 3: Write the module**

Create `crates/taiyaku-library/src/browse.rs`. Start with the module header and the filter helper, which every query below composes:

```rust
//! The read queries a browse screen needs, with the per-root permission
//! filter applied in SQL rather than after it.
//!
//! Spec C1 §5. One responsibility: reading rows for a screen. Nothing here
//! writes, and nothing here formats — `taiyaku-serve`'s `library_api` turns
//! these into JSON.
//!
//! # The filter, which is the reason this module exists rather than a handful
//! # of queries at the call sites
//!
//! Every function takes `roots: &[i64]`, the caller's granted roots, and
//! **an empty slice applies no filter at all**. That is spec C1 §5.1's rule
//! and it is counter-intuitive on purpose: `user_root` had no writer before
//! spec C1, so every account in every existing library holds an empty grant,
//! and any other reading of empty would have emptied every library the day
//! migration 6 applied.
//!
//! The filter is applied to **editions**: an edition survives if its members
//! do, and a work is listed if it keeps at least one edition. A member under
//! no root — every member of every imported work — has `root_id` NULL, and
//! `IN (…)` excludes NULL by SQL's own rules, so that case needs no branch.
//!
//! Filtering after the query was the alternative and is wrong twice over: it
//! reads rows the caller may not see into memory before deciding, and it makes
//! every count (`works`, `editions`, `aligned`) a number computed over the
//! unfiltered set.

use rusqlite::{Connection, params_from_iter};

/// The `AND` clause that restricts an edition to the caller's roots, and the
/// parameters it needs — empty and empty when the grant is empty.
///
/// Built as a string because the number of parameters varies with the grant,
/// which `rusqlite` cannot express in a prepared statement otherwise. The
/// values are `i64` read from `user_root`, never caller text, so there is
/// nothing here to inject; the placeholders are still generated rather than
/// the values interpolated, so that stays true if the source ever changes.
fn root_clause(roots: &[i64]) -> String {
    if roots.is_empty() {
        return String::new();
    }
    let holes = std::iter::repeat_n("?", roots.len()).collect::<Vec<_>>().join(",");
    // `rm`, not `m`: two callers below already have a `member m` in scope, and
    // an inner alias that shadows an outer one is legal SQL, correct here, and
    // one edit away from silently meaning the other thing.
    format!(
        " AND EXISTS (SELECT 1 FROM member rm WHERE rm.edition_id = e.id \
         AND rm.root_id IN ({holes}))"
    )
}
```

**Every placeholder in this module is anonymous `?`, never `?1`.** SQLite
numbers an anonymous parameter *one higher than the largest index assigned so
far*, so a single `?3` earlier in the statement pushes the following `?`s to 4
and 5 — and a query built by concatenating a caller's fragment onto a generated
`IN (?,?)` cannot know what indices those are. Mixing the two forms here was
measured on 2026-08-20 against the real schema: `WHERE e.work_id = ?3 AND …
IN (?,?)` counts **five** parameters, not three, and `rusqlite` refuses the
call. Anonymous-only means the binding order is the text order, which is the
one rule every function below follows: **roots first, then the caller's own
parameters.**

```rust
```

Then the card types, which every query below returns:

```rust
/// One series as a grid tile. `works`, `editions` and `aligned` are counts
/// **after** the filter, which is why they are computed in SQL: a count over
/// the unfiltered set is the exact bug filtering-in-Rust would introduce.
#[derive(Debug, Clone, PartialEq)]
pub struct SeriesCard {
    pub id: i64,
    pub title: String,
    pub works: i64,
    pub editions: i64,
    /// Works with at least one `run` row — spec C1 §3's `⇄ 3` mark. A series
    /// page that lists volumes without saying which are aligned is a Komga
    /// screen with extra badges.
    pub aligned: i64,
    pub cover_work: Option<i64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EditionCard {
    pub key: String,
    pub label: String,
    pub members: i64,
    pub pages: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WorkCard {
    pub id: i64,
    pub slug: String,
    pub title: String,
    pub volume: Option<f64>,
    /// **Only the editions this caller may see.** Empty is impossible: a work
    /// with no surviving edition is not listed at all.
    pub editions: Vec<EditionCard>,
    pub aligned: bool,
    pub pages: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Home {
    pub continuing: Vec<WorkCard>,
    pub added: Vec<WorkCard>,
    pub read: Vec<WorkCard>,
}

/// A grid, or a search over one: series, and the works that belong to none.
///
/// **Spec C1 §5.4.** `import` never writes `series_id`, so every hand-imported
/// work has no series and no series-joined query can see it. Both the grid and
/// search carry the same shape so one component renders either.
#[derive(Debug, Clone, PartialEq)]
pub struct SearchResults {
    pub series: Vec<SeriesCard>,
    pub unfiled: Vec<WorkCard>,
}

/// What the home lists and search return at most.
///
/// Not a parameter, because no caller has a reason to differ and a cap the
/// caller chooses is a cap the caller forgets. Spec C1 §7 declines
/// virtualisation on measured numbers — 127 series, 111 volumes in the largest
/// — so this bounds the three *recency* lists rather than the grid.
const LIST_CAP: i64 = 20;
```

Then the six queries. Each interpolates `root_clause(roots)` once and binds `roots` through `params_from_iter`. Write them in this order, running the matching test after each.

**`series` — the grid.**

```rust
/// Every series this caller may see, with its counts, ordered by title.
///
/// `is_primary` picks the display title; `series_title` holds several names
/// per series so the romaji, the native title and the English one all find the
/// same shelf. A series with no primary title falls back to `sort_title`,
/// which is NOT NULL — without that fallback a series whose titles were all
/// written by a scan with `is_primary = 0` would be invisible.
///
/// # Errors
///
/// Returns an error if the query fails.
pub fn series(conn: &Connection, roots: &[i64]) -> Result<Vec<SeriesCard>, String> {
    let sql = format!(
        "SELECT s.id,
                COALESCE(MAX(CASE WHEN st.is_primary = 1 THEN st.title END), s.sort_title),
                COUNT(DISTINCT w.id),
                COUNT(DISTINCT e.id),
                COUNT(DISTINCT CASE WHEN a.id IS NOT NULL THEN w.id END),
                s.cover_work_id
           FROM series s
           JOIN work w ON w.series_id = s.id
           JOIN edition e ON e.work_id = w.id{clause}
           LEFT JOIN series_title st ON st.series_id = s.id
           LEFT JOIN alignment a ON a.work_id = w.id
          GROUP BY s.id
          ORDER BY 2 COLLATE NOCASE",
        clause = root_clause(roots)
    );
    let mut stmt = conn
        .prepare(&sql)
        .map_err(|e| format!("preparing the series listing: {e}"))?;
    let rows = stmt
        .query_map(params_from_iter(roots), |row| {
            Ok(SeriesCard {
                id: row.get(0)?,
                title: row.get(1)?,
                works: row.get(2)?,
                editions: row.get(3)?,
                aligned: row.get(4)?,
                cover_work: row.get(5)?,
            })
        })
        .map_err(|e| format!("listing series: {e}"))?;
    rows.collect::<rusqlite::Result<Vec<SeriesCard>>>()
        .map_err(|e| format!("reading series: {e}"))
}
```

**`works_of` — the shared work reader**, which `one_series`, `work` and `home` all call rather than repeating a join three times:

```rust
/// The works matching `predicate`, with only the editions this caller may
/// see, in volume order.
///
/// **Two queries, not one join.** A work joined to its editions returns one
/// row per edition, and rebuilding the nesting from a flat result is where
/// counts go wrong. The work list is small — spec C1 §7 measures 111 volumes
/// in the largest series — so a second query per call is cheaper than the
/// bug.
///
/// `predicate` is a SQL fragment over `w`, with its own parameters bound
/// after the roots. It is never caller text: the three call sites below pass
/// literals.
fn works_of(
    conn: &Connection,
    roots: &[i64],
    predicate: &str,
    extra: &[&dyn rusqlite::ToSql],
    order: &str,
) -> Result<Vec<WorkCard>, String> {
    let clause = root_clause(roots);
    let sql = format!(
        "SELECT DISTINCT w.id, w.slug, w.title, w.volume,
                EXISTS (SELECT 1 FROM alignment a WHERE a.work_id = w.id)
           FROM work w
           JOIN edition e ON e.work_id = w.id{clause}
          WHERE {predicate}
          ORDER BY {order}"
    );
    let mut params: Vec<&dyn rusqlite::ToSql> =
        roots.iter().map(|r| r as &dyn rusqlite::ToSql).collect();
    params.extend_from_slice(extra);

    let mut stmt = conn
        .prepare(&sql)
        .map_err(|e| format!("preparing the work listing: {e}"))?;
    let rows: Vec<(i64, String, String, Option<f64>, bool)> = stmt
        .query_map(params.as_slice(), |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?))
        })
        .map_err(|e| format!("listing works: {e}"))?
        .collect::<rusqlite::Result<_>>()
        .map_err(|e| format!("reading works: {e}"))?;

    rows.into_iter()
        .map(|(id, slug, title, volume, aligned)| {
            let editions = editions_of(conn, roots, id)?;
            let pages = editions.iter().map(|e| e.pages).max().unwrap_or(0);
            Ok(WorkCard { id, slug, title, volume, editions, aligned, pages })
        })
        .collect()
}

/// One work's surviving editions, in key order.
fn editions_of(conn: &Connection, roots: &[i64], work_id: i64) -> Result<Vec<EditionCard>, String> {
    let clause = root_clause(roots);
    // `WHERE 1 = 1{clause} AND …`, so the roots bind before `work_id` in text
    // order and every placeholder stays anonymous. Putting `e.work_id = ?`
    // first and numbering it `?{n}` was the first version of this and was
    // wrong — see the note above `root_clause`.
    let sql = format!(
        "SELECT e.key, e.label, COUNT(m.id), COALESCE(SUM(m.pages), 0)
           FROM edition e
           LEFT JOIN member m ON m.edition_id = e.id
          WHERE 1 = 1{clause} AND e.work_id = ?
          GROUP BY e.id
          ORDER BY e.key"
    );
    let mut params: Vec<&dyn rusqlite::ToSql> =
        roots.iter().map(|r| r as &dyn rusqlite::ToSql).collect();
    params.push(&work_id);

    let mut stmt = conn
        .prepare(&sql)
        .map_err(|e| format!("preparing the edition listing: {e}"))?;
    let rows = stmt
        .query_map(params.as_slice(), |row| {
            Ok(EditionCard {
                key: row.get(0)?,
                label: row.get(1)?,
                members: row.get(2)?,
                pages: row.get(3)?,
            })
        })
        .map_err(|e| format!("listing editions of work {work_id}: {e}"))?;
    rows.collect::<rusqlite::Result<Vec<EditionCard>>>()
        .map_err(|e| format!("reading editions of work {work_id}: {e}"))
}
```

**`one_series`, `work` and `search`**, each three lines over the two functions above:

```rust
/// One series and its works, or `None` if this caller cannot see it.
///
/// # Errors
///
/// Returns an error if the query fails.
pub fn one_series(
    conn: &Connection,
    roots: &[i64],
    id: i64,
) -> Result<Option<(SeriesCard, Vec<WorkCard>)>, String> {
    let Some(card) = series(conn, roots)?.into_iter().find(|c| c.id == id) else {
        return Ok(None);
    };
    let works = works_of(conn, roots, "w.series_id = ?", &[&id], "w.volume, w.id")?;
    Ok(Some((card, works)))
}

/// One work by slug, or `None` if this caller cannot see it.
///
/// **`None` rather than a 403 for a work the grant hides**, which is the same
/// answer as one that does not exist — deliberately. Distinguishing them tells
/// an unauthorised caller which slugs are real.
///
/// # Errors
///
/// Returns an error if the query fails.
pub fn work(conn: &Connection, roots: &[i64], slug: &str) -> Result<Option<WorkCard>, String> {
    Ok(works_of(conn, roots, "w.slug = ?", &[&slug], "w.id")?.into_iter().next())
}

/// The works belonging to no series, which no series-joined query can reach.
///
/// **Spec C1 §5.4**, and measured rather than assumed: `import.rs` never
/// writes `series_id` — `taiyaku.json` has no field for it — so this is every
/// hand-imported work in the library, and before this it was invisible on the
/// one screen a reader opens on.
///
/// **A restricted account sees none of them**, and that needs no branch: an
/// imported member's `root_id` is NULL, and `IN (…)` excludes NULL. That is
/// §5.1's rule applying unchanged, not a second rule — verified 2026-08-20
/// against the real schema.
///
/// # Errors
///
/// Returns an error if the query fails.
pub fn unfiled(conn: &Connection, roots: &[i64]) -> Result<Vec<WorkCard>, String> {
    works_of(
        conn,
        roots,
        "w.series_id IS NULL",
        &[],
        "w.title COLLATE NOCASE, w.id",
    )
}

/// Series whose titles match `q`, through FTS5 — **over both indexes**.
///
/// **The query is quoted as an FTS5 string literal**, doubling any `"`, so a
/// reader typing `AND`, `*` or `-` gets a search for those characters rather
/// than a syntax error or an operator they did not ask for. Spec C1 §13 notes
/// that FTS5's default tokeniser does not segment CJK, so 「五等分の花嫁」 may
/// match as one token or not at all; §8's "name what was searched" empty state
/// exists partly for that, and this is where a `trigram` tokeniser would be
/// measured against real titles before the search box is called done.
///
/// # Errors
///
/// Returns an error if the query fails.
pub fn search(conn: &Connection, roots: &[i64], q: &str) -> Result<SearchResults, String> {
    let trimmed = q.trim();
    if trimmed.is_empty() {
        return Ok(SearchResults { series: Vec::new(), unfiled: Vec::new() });
    }
    let quoted = format!("\"{}\"", trimmed.replace('"', "\"\""));

    // **Both indexes**, which is what §5 always said and what an earlier draft
    // of this plan did not do: `work_fts` has been populated by triggers since
    // migration 1 with nothing reading it, and searching series alone leaves an
    // imported work unfindable by its own title.
    let mut by_series = conn
        .prepare(
            "SELECT DISTINCT st.series_id FROM series_fts f
               JOIN series_title st ON st.rowid = f.rowid
              WHERE series_fts MATCH ?1 LIMIT ?2",
        )
        .map_err(|e| format!("preparing the series search: {e}"))?;
    let mut matched: Vec<i64> = by_series
        .query_map(rusqlite::params![quoted, LIST_CAP], |row| row.get(0))
        .map_err(|e| format!("searching series for `{trimmed}`: {e}"))?
        .collect::<rusqlite::Result<_>>()
        .map_err(|e| format!("reading series results for `{trimmed}`: {e}"))?;

    // A work whose title matches drags its series in with it — searching
    // "Hanayome" should find the shelf, not only the volume that spells it.
    let mut by_work = conn
        .prepare(
            "SELECT DISTINCT w.series_id FROM work_fts f
               JOIN work w ON w.id = f.rowid
              WHERE work_fts MATCH ?1 AND w.series_id IS NOT NULL LIMIT ?2",
        )
        .map_err(|e| format!("preparing the work search: {e}"))?;
    let from_works: Vec<i64> = by_work
        .query_map(rusqlite::params![quoted, LIST_CAP], |row| row.get(0))
        .map_err(|e| format!("searching works for `{trimmed}`: {e}"))?
        .collect::<rusqlite::Result<_>>()
        .map_err(|e| format!("reading work results for `{trimmed}`: {e}"))?;
    matched.extend(from_works);

    // Filtered through `series` and `unfiled` rather than joined into the FTS
    // queries, so a hit this caller may not see is dropped by exactly the same
    // code that decides every other screen — one filter, one place to read.
    let series_hits = series(conn, roots)?
        .into_iter()
        .filter(|c| matched.contains(&c.id))
        .collect();

    let mut by_unfiled = conn
        .prepare(
            "SELECT f.rowid FROM work_fts f
               JOIN work w ON w.id = f.rowid
              WHERE work_fts MATCH ?1 AND w.series_id IS NULL LIMIT ?2",
        )
        .map_err(|e| format!("preparing the unfiled search: {e}"))?;
    let unfiled_ids: Vec<i64> = by_unfiled
        .query_map(rusqlite::params![quoted, LIST_CAP], |row| row.get(0))
        .map_err(|e| format!("searching unfiled works for `{trimmed}`: {e}"))?
        .collect::<rusqlite::Result<_>>()
        .map_err(|e| format!("reading unfiled results for `{trimmed}`: {e}"))?;
    let unfiled_hits = unfiled(conn, roots)?
        .into_iter()
        .filter(|w| unfiled_ids.contains(&w.id))
        .collect();

    Ok(SearchResults { series: series_hits, unfiled: unfiled_hits })
}
```

**`home` — three recency lists:**

```rust
/// The three lists the library screen opens on, for one reader.
///
/// `continuing` is progress that is not finished, `read` is progress that is,
/// and `added` is the newest works — each capped at [`LIST_CAP`], each
/// filtered by the same grant.
///
/// # Errors
///
/// Returns an error if a query fails.
pub fn home(conn: &Connection, roots: &[i64], user_id: i64) -> Result<Home, String> {
    let continuing = works_of(
        conn,
        roots,
        "w.id IN (SELECT work_id FROM progress
                   WHERE user_id = ? AND completed_at IS NULL)",
        // **Twice.** The predicate takes one and the ORDER BY subquery below
        // takes another, and `works_of` binds `extra` in text order after the
        // roots. One `user_id` here is a hard refusal from `rusqlite`, not a
        // wrong answer — measured 2026-08-20.
        &[&user_id, &user_id],
        "(SELECT updated FROM progress p WHERE p.work_id = w.id AND p.user_id = ?) DESC",
    )?;
    let read = works_of(
        conn,
        roots,
        "w.id IN (SELECT work_id FROM progress
                   WHERE user_id = ? AND completed_at IS NOT NULL)",
        &[&user_id, &user_id],
        "(SELECT completed_at FROM progress p WHERE p.work_id = w.id AND p.user_id = ?) DESC",
    )?;
    let added = works_of(conn, roots, "1 = 1", &[], "w.id DESC")?;

    let cap = usize::try_from(LIST_CAP).unwrap_or(usize::MAX);
    Ok(Home {
        continuing: continuing.into_iter().take(cap).collect(),
        read: read.into_iter().take(cap).collect(),
        added: added.into_iter().take(cap).collect(),
    })
}
```

**The `ORDER BY` subqueries are why `user_id` is bound twice**, which the code above does and an earlier draft of this plan did not. It is a loud failure rather than a quiet one — `rusqlite` refuses a statement whose parameter count it cannot satisfy — but the test below still asserts *order* rather than membership, because the neighbouring mistake (binding the right number in the wrong order) is quiet.

**`cover_work` — spec C1 §5.2's fallback:**

```rust
/// The work whose hub cover stands for this series.
///
/// **A fallback, not a lookup.** `series.cover_work_id` exists in migration 1
/// and nothing in this repository writes it — verified 2026-08-20 — so it is
/// NULL for every series a scan has produced, which is all of them. C1 writes
/// it at no point; C3's metadata editing is its writer, which is where
/// choosing a cover belongs.
///
/// The fallback is the lowest-`volume` work this caller may see, `w.id` as the
/// tiebreak so a series of volume-less works still answers the same row every
/// time rather than whichever the b-tree returned.
///
/// # Errors
///
/// Returns an error if the query fails.
pub fn cover_work(conn: &Connection, roots: &[i64], series_id: i64) -> Result<Option<i64>, String> {
    let set: Option<i64> = conn
        .query_row(
            "SELECT cover_work_id FROM series WHERE id = ?1",
            rusqlite::params![series_id],
            |row| row.get(0),
        )
        .map_err(|e| format!("reading the cover of series {series_id}: {e}"))?;
    // A hand-set cover still has to survive the filter: a reader granted one
    // root must not be handed a cover generated from a root they cannot see.
    let visible = works_of(conn, roots, "w.series_id = ?", &[&series_id], "w.volume, w.id")?;
    if let Some(chosen) = set {
        if visible.iter().any(|w| w.id == chosen) {
            return Ok(Some(chosen));
        }
    }
    Ok(visible.first().map(|w| w.id))
}
```


- [ ] **Step 4: Register the module**

In `crates/taiyaku-library/src/lib.rs`, beside the other `pub mod` declarations:

```rust
pub mod browse;
```

and at the foot of `browse.rs`:

```rust
#[cfg(test)]
mod tests;
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p taiyaku-library browse:: -- --nocapture`

Expected: PASS, all six.

- [ ] **Step 6: Check the filter against a real shelf**

Serve a scratch copy rather than the corpus itself — never point a writer at `corpus/`:

```bash
SCRATCH="/tmp/claude-1000/-home-davey-projects-taiyaku/7754e742-03d8-4778-b746-cd4d74becf2f/scratchpad/browse-check"
mkdir -p "${SCRATCH:?}"
cargo run -q -p taiyaku-cli -- library scan --add corpus --db "${SCRATCH:?}/library.db"
cargo run -q -p taiyaku-cli -- library user add reader --role user --db "${SCRATCH:?}/library.db"
```

Then, in a Rust test or a scratch binary, call `browse::series` with an empty grant and with one root, and compare the counts. Record both numbers — spec C1 §7 asserts 127 series and §13 says the filter's cost has never been timed, so this is where that stops being unknown.

- [ ] **Step 7: Commit**

```bash
git add crates/taiyaku-library/src/browse.rs crates/taiyaku-library/src/browse/tests.rs crates/taiyaku-library/src/lib.rs
git commit -m "browse.rs: the store queries, with the root filter in the SQL"
```

---

### Task 8: The six `/api/library/*` routes

**Files:**
- Create: `crates/taiyaku-serve/src/library_api.rs`, `crates/taiyaku-serve/src/library_api/tests.rs`
- Modify: `crates/taiyaku-serve/src/lib.rs` — `pub mod library_api;`
- Modify: `crates/taiyaku-serve/src/routes.rs:1386` — six entries in `guarded_routes()`
- Modify: `crates/taiyaku-serve/src/auth.rs:277` — the `#[expect(dead_code)]` comes off

**Interfaces:**
- Consumes: every function from Task 7; `Principal.roots`.
- Produces:

```
GET /api/library/home
GET /api/library/series
GET /api/library/series/{id}
GET /api/library/series/{id}/cover
GET /api/library/works/{slug}
GET /api/library/search?q=
```

- [ ] **Step 1: Write the failing tests**

Create `crates/taiyaku-serve/src/library_api/tests.rs` with, at minimum:

```rust
/// Every new route refuses an anonymous request. The workspace already has
/// `every_route_refuses_an_anonymous_request`, which walks `guarded_routes`
/// and derives its own count — so this test is about the *shape* of the
/// refusal, which spec A2 §4 fixes: no `WWW-Authenticate`, because that header
/// makes the browser raise its own dialog instead of the login screen.
#[tokio::test]
async fn the_browse_routes_refuse_anonymously_without_a_challenge_header() {
    let state = crate::test_support::scanned_state_with_accounts();
    for path in [
        "/api/library/home",
        "/api/library/series",
        "/api/library/search?q=x",
    ] {
        let response = crate::test_support::get_anonymous(&state, path).await;
        assert_eq!(response.status(), axum::http::StatusCode::UNAUTHORIZED, "{path}");
        assert!(
            !response.headers().contains_key(axum::http::header::WWW_AUTHENTICATE),
            "{path} must not challenge the browser"
        );
    }
}

/// The three root-filter cases, at the route rather than at the query — this
/// is where `Principal.roots` is actually read.
///
/// The fixture is Task 7's shape driven through HTTP: one series, one volume,
/// two editions in two roots, and two accounts — `open`, holding no grant, and
/// `narrow`, granted the English root alone.
#[tokio::test]
async fn an_account_with_no_grant_sees_every_root_through_the_api() {
    let fixture = crate::test_support::two_root_fixture().await;
    let body = crate::test_support::get_json(&fixture.state, "/api/library/series", "open").await;
    let series = body.as_array().expect("an array of series");
    assert_eq!(series.len(), 1, "one series");
    assert_eq!(
        series[0]["editions"], 2,
        "an empty grant is unrestricted, not blind — spec C1 §5.1"
    );
}

#[tokio::test]
async fn a_granted_account_sees_only_its_root_through_the_api() {
    let fixture = crate::test_support::two_root_fixture().await;
    let body = crate::test_support::get_json(&fixture.state, "/api/library/series", "narrow").await;
    let series = body.as_array().expect("an array of series");
    assert_eq!(series.len(), 1, "the work is still listed");
    assert_eq!(series[0]["editions"], 1, "only the granted root's edition");
    assert_eq!(series[0]["works"], 1, "the count is computed after the filter");
}

/// Spec C1 §5.1: a grant that hides the hub still opens the work, at a
/// surviving edition.
///
/// **The case the rule does not answer for free.** `work.hub` names the
/// edition `content_key` was computed over and the one the reader opens.
/// Hiding the whole work instead was rejected — it withholds material the
/// account *was* granted — so the work is listed and the reader falls back to
/// the first surviving edition.
#[tokio::test]
async fn a_grant_that_hides_the_hub_still_lists_the_work() {
    let fixture = crate::test_support::two_root_fixture().await;
    // Force the hub to the edition `narrow` cannot see, so the fallback is
    // what is under test rather than an accident of which key sorted first.
    {
        let conn = fixture.state.library.conn();
        conn.execute("UPDATE work SET hub = ?1", [&fixture.hidden_edition_key])
            .expect("points the hub at the hidden edition");
    }

    let body = crate::test_support::get_json(&fixture.state, "/api/library/series", "narrow").await;
    let series = body.as_array().expect("an array of series");
    assert_eq!(series.len(), 1, "the work is listed even though its hub is hidden");

    let slug = &fixture.slug;
    let work = crate::test_support::get_json(
        &fixture.state,
        &format!("/api/library/works/{slug}"),
        "narrow",
    )
    .await;
    let editions = work["editions"].as_array().expect("editions");
    assert_eq!(editions.len(), 1, "one surviving edition");
    assert_ne!(
        editions[0]["key"], fixture.hidden_edition_key,
        "the surviving edition is not the hidden hub"
    );
}

/// A work this caller cannot see answers **404, not 403** — the same answer as
/// one that does not exist.
///
/// Distinguishing them would tell an unauthorised caller which slugs are real,
/// which is the enumeration this project already refuses elsewhere.
#[tokio::test]
async fn a_work_outside_the_grant_is_indistinguishable_from_one_that_does_not_exist() {
    let fixture = crate::test_support::two_root_fixture().await;
    let hidden = crate::test_support::get(
        &fixture.state,
        &format!("/api/library/works/{}", fixture.other_root_only_slug),
        "narrow",
    )
    .await;
    let absent =
        crate::test_support::get(&fixture.state, "/api/library/works/no-such-work", "narrow").await;
    assert_eq!(hidden.status(), absent.status());
    assert_eq!(hidden.status(), axum::http::StatusCode::NOT_FOUND);
}

/// `home`'s lists are ordered by recency, and the ordering subquery binds
/// `user_id` a second time.
///
/// **Asserting order rather than membership is the point.** Get the extra
/// binding wrong and every membership assertion still passes while the list
/// comes back in the wrong order — see the note under `home` in Task 7.
#[tokio::test]
async fn continuing_is_ordered_by_when_each_work_was_last_read() {
    let fixture = crate::test_support::two_root_fixture().await;
    fixture.record_progress("open", 0, "2026-08-01T00:00:00Z").await;
    fixture.record_progress("open", 1, "2026-08-19T00:00:00Z").await;

    let body = crate::test_support::get_json(&fixture.state, "/api/library/home", "open").await;
    let continuing = body["continuing"].as_array().expect("continuing");
    assert_eq!(continuing.len(), 2);
    assert_eq!(
        continuing[0]["slug"], fixture.slugs[1],
        "most recently read first"
    );
}
```

The fixture these share goes in `crates/taiyaku-serve/src/test_support.rs`, beside the ones Task 6 added:

```rust
/// Task 8's fixture: one series, two roots, two accounts.
///
/// `open` holds no grant and so sees everything; `narrow` is granted the first
/// root alone. Both are real accounts with real credentials, because the point
/// is to drive the guard rather than to bypass it.
pub(crate) struct TwoRootFixture {
    pub state: Arc<crate::ServeState>,
    /// The slug of the work both roots hold.
    pub slug: String,
    /// Every work's slug, in the order they were created.
    pub slugs: Vec<String>,
    /// A work only the root `narrow` cannot see holds.
    pub other_root_only_slug: String,
    /// The edition key belonging to the root `narrow` was not granted.
    pub hidden_edition_key: String,
}

impl TwoRootFixture {
    /// Record a reading position for `user` on `slugs[index]`, stamped
    /// `updated`, by writing the row directly.
    ///
    /// Written as a row rather than driven through `PUT /progress`, because
    /// that route stamps `updated` itself and this test is about ordering by
    /// a controlled clock — the same reason spec B's own tests stamp a
    /// directory forward rather than racing a one-second mtime.
    pub async fn record_progress(&self, user: &str, index: usize, updated: &str) { /* … */ }
}

pub(crate) async fn two_root_fixture() -> TwoRootFixture { /* … */ }

/// `GET path` as `user`, through the real guard with a real session.
pub(crate) async fn get(state: &Arc<crate::ServeState>, path: &str, user: &str) -> axum::response::Response { /* … */ }

/// [`get`], with the body parsed. Panics on a non-200, naming the status —
/// a fixture that silently returns `null` from a 401 is a test that asserts
/// nothing.
pub(crate) async fn get_json(state: &Arc<crate::ServeState>, path: &str, user: &str) -> serde_json::Value { /* … */ }
```

Implement the four fixture bodies against the helpers already in `test_support.rs` — the file has an established way to build a `ServeState` over a real library and to drive `build_router` with `oneshot`, and these are that loop with an account and a grant added. **The `/* … */` markers above are the fixture's scaffolding, not the tests'**: every assertion in this task is written out above, and a committed `test_support.rs` containing one of these markers is a task failure.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p taiyaku-serve library_api:: -- --nocapture`

Expected: FAIL — the module does not exist.

- [ ] **Step 3: Write the handlers**

Create `crates/taiyaku-serve/src/library_api.rs`. Each handler follows the shape `whoami` establishes — pull the `Principal` out of the request extensions, refuse with 401 if it is absent rather than unwrapping — then calls one `browse` function and serialises:

```rust
//! The `/api/library/*` handlers. Spec C1 §5.
//!
//! Thin by design: parse, call `taiyaku_library::browse`, serialise. Every
//! decision about *what* a browse screen may see lives in that module, in SQL,
//! so there is one place the root filter is applied and one place to read to
//! know whether it is.
//!
//! Kept out of `routes.rs`, which is already 1,579 lines and holds the reader's
//! own routes — the two have no shared state and no reason to be read together.

use crate::{ServeState, auth::Principal};
use axum::{extract::{Path, Query, State}, http::StatusCode, response::{IntoResponse, Response}};
use std::sync::Arc;

/// The `Principal` the guard inserted, or a 401.
///
/// Unreachable behind the guard, which inserts one or refuses. Answered rather
/// than unwrapped, so a future routing mistake is a 401 and not a panic — the
/// same call `session::whoami` makes, for the same reason.
fn principal(request: &axum::http::Extensions) -> Result<&Principal, Response> {
    request
        .get::<Principal>()
        .ok_or_else(|| (StatusCode::UNAUTHORIZED, "unauthorized").into_response())
}
```

Then one handler per route. `get_cover` resolves through `browse::cover_work` and then hands off to the existing `cover.rs` generation path, so a series cover and an edition cover are the same content-addressed file.

- [ ] **Step 4: Register the routes**

In `crates/taiyaku-serve/src/routes.rs`, extend `guarded_routes()` before `routes.extend(crate::opds::routes());`:

```rust
    routes.extend([
        ("/api/library/home", get(crate::library_api::home)),
        // Returns `{ series, unfiled }` — spec C1 §5.4. The grid needs both,
        // and `search` returns the same shape so one component renders either.
        ("/api/library/series", get(crate::library_api::series)),
        ("/api/library/series/{id}", get(crate::library_api::one_series)),
        ("/api/library/series/{id}/cover", get(crate::library_api::cover)),
        ("/api/library/works/{slug}", get(crate::library_api::work)),
        ("/api/library/search", get(crate::library_api::search)),
    ]);
```

**In `guarded_routes`, never `open_routes`.** `route_layer` wraps only what is registered before it.

- [ ] **Step 5: Take the `#[expect]` off `Principal.roots`**

In `crates/taiyaku-serve/src/auth.rs`, remove the attribute at line 277 and rewrite the field's doc comment, which currently calls it "the one field genuinely nothing reads":

```rust
    /// The roots this account was granted, and **the restriction**: an empty
    /// grant sees every root, because `user_root` had no writer before spec
    /// C1 and every account in every existing library holds one. Read by
    /// `library_api`, which passes it to every `taiyaku_library::browse`
    /// query so the filter is applied in SQL. Spec C1 §5.1; O54 closes for
    /// this field and stays open for `can_download` and `can_stream`.
    pub roots: Vec<i64>,
```

The `#[expect(dead_code)]` **must** be removed in the same commit that first reads the field — `#[expect]` fails the build when the lint does not fire, which is the forcing function spec A2 installed for exactly this moment.

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cargo test -p taiyaku-serve -- --nocapture`

Expected: PASS, including `every_route_refuses_an_anonymous_request`, which derives its count from both route lists and will fail if any of the six landed in `open_routes`.

- [ ] **Step 7: Run the whole gate**

Run: `scripts/check-all.sh --no-web --no-wasm`

Expected: green. Clippy at pedantic is the likely complainer — `root_clause`'s string building and the handlers' `Result<_, Response>` returns are the two places to look.

- [ ] **Step 8: Commit**

```bash
git add crates/taiyaku-serve/src/library_api.rs crates/taiyaku-serve/src/library_api/tests.rs crates/taiyaku-serve/src/lib.rs crates/taiyaku-serve/src/routes.rs crates/taiyaku-serve/src/auth.rs
git commit -m "The six /api/library routes, and Principal.roots finally read"
```

---

### Task 9: The documentation this invalidates

**Files:**
- Modify: `docs/status.md` — the `taiyaku-library` crate row, the CLI command list, O54
- Modify: `README.md` — `taiyaku serve` gaining an invocation that takes no paths

**Interfaces:**
- Consumes: everything above.
- Produces: a `docs/status.md` that describes what shipped, per spec C1 §12.

- [ ] **Step 1: Close O54 for `roots` only**

In `docs/status.md`, edit O54's row. It currently says three fields are computed and read by nothing. Two still are. Record what closed, what it cost, and what the address-probe-style trade was — that `work.root_id` was declined twice, and why the third answer was neither.

- [ ] **Step 2: Update the `taiyaku-library` crate row**

Add `browse.rs` to the module list, `member.root_id` to the schema description, and state the empty-grant rule — it is the kind of counter-intuitive invariant this file exists to hold.

- [ ] **Step 3: Update the CLI list**

`taiyaku library user` gains `grant-root` and `revoke-root`; `taiyaku serve` gains its no-paths form.

- [ ] **Step 4: Re-run the measurement scripts**

```bash
node scripts/status-test-count.mjs
```

Expected: a new test count. Update `docs/status.md` with what it prints rather than with an estimate — per this repository's rule that an asserted figure was measured on this machine.

- [ ] **Step 5: Run the full gate**

Run: `scripts/check-all.sh`

Expected: green, all four jobs. This is the run that must pass before the branch is called done — the docs job reads `docs/status.md`.

- [ ] **Step 6: Commit**

```bash
git add docs/status.md README.md
git commit -m "status.md: what the browse API shipped, and what O54 has left"
```

---

## Self-Review

**Spec coverage.** §5's six routes → Task 8. §5.1's filter → Tasks 1, 2, 3, 4, 7, 8. §5.2's cover → Tasks 7 (`cover_work`) and 8 (`cover`). §5.3's `serve --db` → Tasks 5 and 6. §5.4's unfiled bucket and dual-index search → Task 7 (`unfiled`, `search`) and Task 8's grid shape. §9's tests: the three filter cases (Task 8), the cover fallback both ways (Task 7), `serve --db` over a derived library and a correction writing no file (Tasks 5, 6). §12's invalidated measurements → Task 9.

**Deliberately not in this plan**, and each belongs to part 2: `react-router` and the bundle measurement, `openWork(slug)` and `<App key={work}>`, the shell, the series page, the `browse` module and its registry, the search box, the empty states, and the reader's way back out. §7's virtualisation decision and §13's FTS5-on-Japanese measurement are both UI-facing and go there too.

**Two things this plan leaves measured-but-unasserted**, per §13, and both have a step that produces the number rather than a promise: the root filter's cost on a real shelf (Task 7 Step 6) and the test count (Task 9 Step 4).

**Type consistency.** `roots: &[i64]` is the parameter name and type in every `browse` function and in `Principal.roots`. `root_id` is the column, the `Candidate` field and the `grant_root`/`revoke_root` parameter throughout. `has_document()` is defined in Task 5 Step 3 and used in Task 6 Step 3. `from_library` is defined in Task 5 Step 4 and exported in Step 5.

**Placeholder scan, run against the finished document.** No `TBD`, no `TODO`, no "add appropriate error handling", no "write tests for the above". Every code step carries the code. Four `/* … */` markers survive on purpose and all four are in **fixture scaffolding** in Task 8 Step 1 — `TwoRootFixture::record_progress`, `two_root_fixture`, `get`, `get_json` — each of which is a loop over helpers already in `test_support.rs` that the step names. Every *assertion* in every test is written out. A committed file containing one of those markers is a task failure, and the step says so.

**Three of this plan's own defects were found by running its SQL rather than reading it** (2026-08-20, against the real schema plus migration 6 in `sqlite3`), and each is now fixed with the measurement recorded beside it: `editions_of` mixed `?3` with anonymous `?` and counted five parameters where it bound three; `home` bound `user_id` once against two placeholders; and `series()` could not see a hand-imported work at all, because `import` never writes `series_id` — which became §5.4 and the `unfiled` bucket. **The lesson is in the plan rather than only in this note**: Task 7 Step 6 runs the queries against a real shelf before the routes are built on them.

**Two traps are called out where an implementer would otherwise hit them silently**, both of the same kind — a mistake that leaves the tests green:

- `works_of`'s `extra` parameters bind after the roots, so `home`'s `continuing` and `read` must pass `user_id` **twice**, once for the predicate and once for the ordering subquery. Miss it and the list returns in the wrong order with every membership assertion passing. `continuing_is_ordered_by_when_each_work_was_last_read` asserts order for exactly this reason.
- A route registered in `open_routes()` rather than `guarded_routes()` ships unauthenticated. The existing `every_route_refuses_an_anonymous_request` derives its count from both lists and catches it — noted in Task 8 Step 4 and again in the Global Constraints, because it is the one mistake here whose cost is a public library.
