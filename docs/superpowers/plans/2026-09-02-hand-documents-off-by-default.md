# Hand Documents Off By Default Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Stop `<root>/.taiyaku/` appearing in a person's media tree unless that root asks for it, without deleting the mechanism or anything already on disk.

**Architecture:** One column, `root.keeps_documents`, defaulting to off. `hand::save` writes nothing when the work's root declines; `hand::reconcile` never *repairs* in such a root but still reports orphans left there; the PATCH response gains a third state so `persisted: false` keeps meaning "a write failed" rather than becoming permanent furniture.

**Tech Stack:** Rust, `rusqlite`, `serde_json`, React/TypeScript for one editor branch. Changes span `taiyaku-library`, `taiyaku-serve` and `web/`.

**Spec:** [`../specs/2026-09-02-hand-documents-off-by-default-design.md`](../specs/2026-09-02-hand-documents-off-by-default-design.md)

## Global Constraints

- **The default is off.** Migration 012 adds `keeps_documents INTEGER NOT NULL DEFAULT 0`, and every existing root takes that default. A root that wants documents says so.
- **Nothing is ever deleted.** Turning the setting off leaves every document already on disk exactly where it is. No task in this plan removes a file.
- **`reconcile` must not repair in an off root.** A marked work with no file is a *gap*, and repairing a gap writes the file — so without an explicit branch, turning documents off would have the next scan recreate every one of them. **This was called "the only way this change can fail silently" here, and that stopped being true once Task 2 gave `hand::save` its own guard: `save` already refuses the write for a declined root, so deleting this branch alone hands every gap to `save`, which still writes nothing — a hand-applied mutation during implementation confirmed it, reporting `repaired: 1` with no `.taiyaku` directory created.** What this branch actually prevents is `reconcile` claiming a repair that never happened, not a resurrected file — the two guards are independent refusals, and the real hazard is relaxing one while believing the other is the backstop. Task 3 exists for it.
- **`keeps_documents` has three states, not two.** `Some(false)` is *this root declined*; `Some(true)` is *it keeps them*; `None` is *there was no root to ask* (an imported work whose document belongs beside its sidecar). A plain `bool` conflates the first and third.
- **The JSON field is `keeps_documents`, emitted only when the root declined** — absent otherwise, matching how `why` is absent unless there is one.
- **Never use `set_readonly` to make a directory unwritable in a test.** CI runs as root and ignores the write bit. Plant a plain file where the directory must go — `std::fs::write(root.join(".taiyaku"), b"not a directory")` — which fails for any uid.
- **A failed write is counted and named, never raised.** `hand::save` answers `persisted: false` rather than an error; `reconcile` must not start returning `Err`.
- **Name sites by symbol, never by line number.** Doc comments explain *why*, naming the alternative the decision beat.
- **Verify with all four**, and each must be clean: `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all -- --check`, and `env -u TAIYAKU_CORPUS scripts/check-all.sh` showing the literal line `all green` — never piped through `tail`, never with `; echo` appended.
- **The test count appears twice in `docs/status.md`** — "Six crates, N tests" and "Nineteen of the N are standing checks" about two lines later. Adding tests moves both, or the gate's `status-test-count` check fails. The baseline entering this plan is **1762**.

## File Structure

| File | Responsibility | Change |
| ---- | -------------- | ------ |
| `crates/taiyaku-library/src/schema/012_root_keeps_documents.sql` | the column and why it defaults off | create |
| `crates/taiyaku-library/src/schema.rs` | migration registry | one `include_str!` line |
| `crates/taiyaku-library/src/scan.rs` | `Root` and the query that builds it | add the field |
| `crates/taiyaku-library/src/hand.rs` | whether a document is written at all | `Saved.keeps_documents`, a root lookup, branches in `save` and `reconcile` |
| `crates/taiyaku-library/src/hand/tests.rs` | unit coverage | tests for every branch |
| `crates/taiyaku-serve/src/metadata.rs` | the PATCH response | carry the third state out |
| `web/src/metadata.ts` | the client type | `keeps_documents?: boolean` |
| `web/src/MetadataEditor.tsx` | the "not durable" note | it already keys off `persisted`; confirm and pin |
| `docs/superpowers/specs/2026-08-14-library-store-design.md` | §4.4's claim | qualify it |
| `docs/status.md` | test counts | both sites |

---

### Task 1: the column, and `Root` carrying it

**Files:**
- Create: `crates/taiyaku-library/src/schema/012_root_keeps_documents.sql`
- Modify: `crates/taiyaku-library/src/schema.rs`, `crates/taiyaku-library/src/scan.rs`
- Test: `crates/taiyaku-library/src/scan/tests.rs`

**Interfaces:**
- Consumes: `scan::Root { id, path, edition_key, default_kind, default_language, scan_interval_secs }`; `scan::roots(conn) -> Result<Vec<Root>, String>`
- Produces: `scan::Root` gains `pub keeps_documents: bool`, read from the new column. `roots()` selects it. Later tasks read `root.keeps_documents` directly rather than querying.

- [ ] **Step 1: Write the failing test**

Add to `crates/taiyaku-library/src/scan/tests.rs`:

```rust
/// **The default is off, and that is the whole point of migration 012.**
/// `<root>/.taiyaku/` appeared in a person's media tree without their asking;
/// a root now says so or gets nothing. A root added by `add_root` takes the
/// column's default, so this pins the default rather than any call site.
#[test]
fn a_new_root_keeps_no_documents_by_default() {
    let dir = crate::tests::tempdir("scan-keeps-documents-default");
    let conn = Connection::open_in_memory().expect("in-memory database");
    crate::schema::migrate(&conn).expect("migrating");

    let root = super::add_root(&conn, &dir.path).expect("adding the root");
    assert!(
        !root.keeps_documents,
        "a root added without being asked keeps no documents: {root:?}"
    );

    let listed = super::roots(&conn).expect("listing roots");
    assert_eq!(listed.len(), 1);
    assert!(
        !listed[0].keeps_documents,
        "and `roots` reads the same answer back off the column, rather than \
         defaulting in Rust where the schema would not agree: {:?}",
        listed[0]
    );
}

/// A root told to keep documents keeps them. Without this the setting could
/// be hard-coded off and every other test in this plan would still pass.
#[test]
fn a_root_can_be_told_to_keep_documents() {
    let dir = crate::tests::tempdir("scan-keeps-documents-on");
    let conn = Connection::open_in_memory().expect("in-memory database");
    crate::schema::migrate(&conn).expect("migrating");
    let root = super::add_root(&conn, &dir.path).expect("adding the root");

    conn.execute(
        "UPDATE root SET keeps_documents = 1 WHERE id = ?1",
        [root.id],
    )
    .expect("turning documents on");

    let listed = super::roots(&conn).expect("listing roots");
    assert!(listed[0].keeps_documents, "{:?}", listed[0]);
}
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p taiyaku-library scan::tests::a_new_root_keeps_no_documents -- --nocapture`
Expected: FAIL to compile — ``no field `keeps_documents` on type `Root` ``

- [ ] **Step 3: Write the migration**

Create `crates/taiyaku-library/src/schema/012_root_keeps_documents.sql`:

```sql
-- **Whether this root keeps `<root>/.taiyaku/<content_key>.library.json` at
-- all, and it does not unless asked.**
--
-- The document is a projection, not a source: `hand::save` writes what
-- `librarydoc::of` reads out of the `_hand` columns the database already
-- holds, and nothing reads it back while the `library` marker is armed. Its
-- whole purpose is store spec §4.4 — losing the database costs a rescan
-- rather than costing the organisational work — and it earns that purpose in
-- exactly one circumstance: the database is lost or rebuilt.
--
-- So it is insurance, and a deployment whose database sits in a backed-up
-- application-data directory has already bought it. Off is the default
-- because the cost of the file is paid by everyone, in their own media tree,
-- while the benefit is paid out only to someone who loses a database they did
-- not back up.
--
-- **On `root` rather than in a settings table, and that is the cheaper
-- choice, not the more flexible one.** `root` already carries `edition_key`,
-- `default_kind`, `default_language`, `exclude_globs` and
-- `scan_interval_secs`; `hand::path_for` already reads that row to resolve an
-- address, so the value arrives where it is needed with no new query. A
-- `setting` table would have been new schema, a new accessor and a new cache,
-- to hold strictly less. That it also lets a portable disk keep documents
-- while a backed-up NAS does not is a side effect rather than the reason.
--
-- Existing rows take the default, which changes behaviour for any library
-- already writing documents: it stops writing new ones. Nothing on disk is
-- touched, and `hand::reconcile` reports what is already there as orphans so
-- it is not discovered years later.
ALTER TABLE root ADD COLUMN keeps_documents INTEGER NOT NULL DEFAULT 0;
```

- [ ] **Step 4: Register it and extend `Root`**

In `crates/taiyaku-library/src/schema.rs`, add after the `011_hand_provenance.sql` line:

```rust
    include_str!("schema/012_root_keeps_documents.sql"),
```

In `crates/taiyaku-library/src/scan.rs`, add to `pub struct Root`:

```rust
    /// Whether this root keeps `<root>/.taiyaku/<content_key>.library.json`.
    ///
    /// **Off unless asked** — migration 012's own comment carries the
    /// reasoning. Read here rather than queried per work because `reconcile`
    /// already holds a `Root` and `hand::save` is the only caller that does
    /// not.
    pub keeps_documents: bool,
```

and extend `roots`' query and its row mapping:

```rust
            "SELECT id, path, edition_key, default_kind, default_language, scan_interval_secs, \
             keeps_documents \
             FROM root ORDER BY id",
```

```rust
                keeps_documents: r.get(6)?,
```

Then find every other constructor of `Root` in the crate — `add_root` at minimum — and give it the same field, reading the column rather than hard-coding `false`, so the schema stays the single answer.

- [ ] **Step 5: Run to verify they pass**

Run: `cargo test -p taiyaku-library`
Expected: PASS. Baseline is 1762; you have added 2.

- [ ] **Step 6: Hand-apply a mutation and watch a test fail**

Change `keeps_documents: r.get(6)?` to `keeps_documents: false` and re-run.
Expected: `a_root_can_be_told_to_keep_documents` FAILS. Revert.

- [ ] **Step 7: Commit**

```bash
git add crates/taiyaku-library/src/schema/012_root_keeps_documents.sql \
        crates/taiyaku-library/src/schema.rs crates/taiyaku-library/src/scan.rs \
        crates/taiyaku-library/src/scan/tests.rs
git commit -m "a root says whether it keeps hand documents, and does not by default"
```

---

### Task 2: `hand::save` writes nothing when the root declines

**Files:**
- Modify: `crates/taiyaku-library/src/hand.rs`
- Test: `crates/taiyaku-library/src/hand/tests.rs`

**Interfaces:**
- Consumes: `hand::path_for(conn, work_id) -> Result<Option<PathBuf>, String>`; `hand::Saved { path, persisted, why }`; `migrate::record_absorbed`
- Produces:
  - `hand::Saved` gains `pub keeps_documents: Option<bool>`
  - `pub fn root_keeps_documents(conn: &Connection, work_id: i64) -> Result<Option<bool>, String>` — `None` when the work has no rooted member on its hub edition

- [ ] **Step 1: Write the failing tests**

Add to `crates/taiyaku-library/src/hand/tests.rs`:

```rust
/// **The change's headline: an off root gets no file.** Not an error, not a
/// failed write — nothing was owed, so nothing is missing and `persisted`
/// says so.
#[test]
fn a_root_that_keeps_no_documents_gets_no_file_and_reports_success() {
    let dir = crate::tests::tempdir("hand-save-documents-off");
    let root = dir.path.to_string_lossy().to_string();
    let conn = Connection::open_in_memory().expect("in-memory database");
    let work_id = a_scanned_work_rooted_at(&conn, &root);
    // `a_scanned_work_rooted_at` seeds the root directly, so the column takes
    // migration 012's default of 0 — which is the case under test.

    let mut doc = crate::librarydoc::of(&conn, work_id).expect("projecting");
    doc.title = Some("Hand Set".to_string());
    crate::librarydoc::apply(&conn, work_id, &doc).expect("applying");

    let saved = super::save(&conn, work_id).expect("saving must not fail");

    assert!(
        saved.persisted,
        "nothing was owed, so nothing is missing: {saved:?}"
    );
    assert_eq!(saved.why, None, "and there is no failure to name");
    assert_eq!(
        saved.keeps_documents,
        Some(false),
        "the root declined, which is not the same as having no root: {saved:?}"
    );
    assert!(
        !dir.path.join(".taiyaku").exists(),
        "and the directory a person objected to was never created"
    );
    assert!(
        crate::migrate::absorbed(&conn, work_id, crate::migrate::LIBRARY)
            .expect("reading the marker"),
        "the marker is still armed: it means the document has been absorbed, \
         which stays true when there was never one to absorb, and \
         `derive::write_work`'s read-back guard reads it before deciding \
         whether to call `load`"
    );
}

/// The setting is a setting, not a removal. Without this the whole feature
/// could be hard-coded off and every other test here would still pass.
#[test]
fn a_root_that_keeps_documents_still_writes_one() {
    let dir = crate::tests::tempdir("hand-save-documents-on");
    let root = dir.path.to_string_lossy().to_string();
    let conn = Connection::open_in_memory().expect("in-memory database");
    let work_id = a_scanned_work_rooted_at(&conn, &root);
    conn.execute("UPDATE root SET keeps_documents = 1", [])
        .expect("turning documents on");

    let mut doc = crate::librarydoc::of(&conn, work_id).expect("projecting");
    doc.title = Some("Hand Set".to_string());
    crate::librarydoc::apply(&conn, work_id, &doc).expect("applying");

    let saved = super::save(&conn, work_id).expect("saving");

    assert!(saved.persisted, "{saved:?}");
    assert_eq!(saved.keeps_documents, Some(true), "{saved:?}");
    let path = saved.path.expect("an on root has an address");
    assert!(path.is_file(), "and the document is at it: {}", path.display());
}

/// **`None` is not `Some(false)`.** A work with no rooted member has no root
/// to ask, which is a different thing from a root that declined — the
/// distinction the field exists to carry, and the one a plain `bool` would
/// have lost.
#[test]
fn a_work_with_no_rooted_member_has_no_root_to_ask() {
    let conn = Connection::open_in_memory().expect("in-memory database");
    let work_id = a_work_with_no_rooted_member(&conn);

    let mut doc = crate::librarydoc::of(&conn, work_id).expect("projecting");
    doc.title = Some("Hand Set".to_string());
    crate::librarydoc::apply(&conn, work_id, &doc).expect("applying");

    let saved = super::save(&conn, work_id).expect("saving");

    assert_eq!(
        saved.keeps_documents, None,
        "there was no root to ask, which is not the same as one that said no: \
         {saved:?}"
    );
    assert_eq!(saved.path, None, "and no address either");
}
```

`a_work_with_no_rooted_member` already exists in this file — it is the fixture behind `a_work_with_no_rooted_member_has_no_address`. Reuse it; if its name differs, use whatever that test calls and do not add a second fixture of the same shape.

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p taiyaku-library hand::tests::a_root_that_keeps -- --nocapture`
Expected: FAIL to compile — ``no field `keeps_documents` on type `Saved` ``

- [ ] **Step 3: Add the field and the lookup**

In `crates/taiyaku-library/src/hand.rs`, add to `pub struct Saved`:

```rust
    /// Whether the root this work sits under keeps documents at all —
    /// **three states, and the third is why this is not a `bool`.**
    ///
    /// `Some(true)` is a root that keeps them; `Some(false)` is one that
    /// declined, so nothing was owed and [`Self::persisted`] is `true` with
    /// no [`Self::why`]; `None` is a work with no rooted member, an imported
    /// work whose document belongs beside its sidecar and whose [`Self::path`]
    /// is `None` for that entirely different reason.
    ///
    /// A plain `bool` would have made the third case read as the first, which
    /// is the conflation `path`'s own `None` already carries two of.
    pub keeps_documents: Option<bool>,
```

and add, beside `path_for`:

```rust
/// Whether the root this work's hub edition sits under keeps documents.
///
/// `None` when no member of the hub edition names a root — the same condition
/// [`path_for`] answers `None` for, and for the same reason: there is no root
/// to ask. Resolved by the same "first member by ordinal that still names a
/// root" rule, so the two functions cannot disagree about which root a work
/// belongs to.
///
/// **A second query rather than widening [`path_for`]'s return type**, which
/// would have moved every one of its call sites to answer a question only
/// `save` asks: `reconcile` already holds a `Root` and reads
/// `Root::keeps_documents` off it directly.
///
/// # Errors
///
/// Returns an error if SQLite fails.
pub fn root_keeps_documents(conn: &Connection, work_id: i64) -> Result<Option<bool>, String> {
    conn.query_row(
        "SELECT root.keeps_documents
           FROM member
           JOIN edition ON edition.id = member.edition_id
           JOIN work    ON work.id    = edition.work_id
           JOIN root    ON root.id    = member.root_id
          WHERE work.id = ?1 AND edition.key = work.hub
          ORDER BY member.ordinal
          LIMIT 1",
        [work_id],
        |row| row.get::<_, bool>(0),
    )
    .optional()
    .map_err(|e| format!("reading whether work {work_id}'s root keeps documents: {e}"))
}
```

- [ ] **Step 4: Branch in `save`**

In `save`, resolve the setting before the write and take an early branch when the root declined. The marker call stays exactly where it is — arming it does not depend on a file existing:

```rust
    let keeps_documents = root_keeps_documents(conn, work_id)?;
    let doc = crate::librarydoc::of(conn, work_id)?;

    // **A root that keeps no documents is not a failed write.** Nothing was
    // owed, so nothing is missing: `persisted` is `true` and `why` is `None`,
    // and the caller tells this apart from a write that landed by
    // `keeps_documents` rather than by guessing from `path`. Reporting
    // `persisted: false` here was the alternative — literally true, since
    // there is no file — and it was rejected because it makes every save on a
    // default install carry the "not durable" note, turning a real signal
    // into permanent furniture.
    //
    // The marker below is still recorded. It means "this work's document has
    // been absorbed", which stays true when there was never one to absorb,
    // and `derive::write_work`'s read-back guard reads it before deciding
    // whether to call `load` at all.
    if keeps_documents == Some(false) {
        crate::migrate::record_absorbed(conn, work_id, crate::migrate::LIBRARY, !doc.is_empty())?;
        return Ok(Saved {
            path: None,
            persisted: true,
            why: None,
            keeps_documents,
        });
    }
```

Then add `keeps_documents` to the `Saved` built on the existing path at the end of the function.

- [ ] **Step 5: Opt the `taiyaku-serve` fixtures in, or thirteen tests break**

**This is the step that turns a wall of red into two lines.** `crates/taiyaku-serve/src/metadata/tests.rs` has **13 assertions that read the hand document off disk** — `patching_a_title_writes_the_row_and_the_document` asserts the file contains the title it just set, and it is not alone. Every one of those fixtures is built by `router_over_a_scanned_tree` in `crates/taiyaku-serve/src/test_support.rs`, which calls `taiyaku_library::scan::add_root` — so after Task 1 they all get `keeps_documents = 0` and every one of those assertions fails.

Those tests are *about* the document mechanism and should keep exercising it. So the shared builder opts in, immediately after its `add_root` call:

```rust
        let root = taiyaku_library::scan::add_root(&conn, fixture.path()).expect("adds the root");
        // **These fixtures are about the document, so they keep documents.**
        // Migration 012 made `keeps_documents` default off, which is right for
        // a person's library and wrong for a fixture whose whole subject is
        // the file — `patching_a_title_writes_the_row_and_the_document` and
        // twelve assertions like it read it off disk. A fixture that wants the
        // off case turns it off explicitly, which is the rarer thing to say.
        conn.execute(
            "UPDATE root SET keeps_documents = 1 WHERE id = ?1",
            [root.id],
        )
        .expect("the fixture keeps documents");
```

Apply it to **every** `add_root` call in that file that backs a fixture used by `metadata/tests.rs`. Run `grep -n "add_root" crates/taiyaku-serve/src/test_support.rs` and treat each hit; a fixture no metadata test uses needs no change, and adding it anyway is harmless.

- [ ] **Step 6: Run to verify they pass**

Run: `cargo test --workspace`
Expected: PASS, across both crates. Existing `hand` tests that assert on a whole `Saved` need the new field added to their expected value — extend them, never loosen an assertion.

- [ ] **Step 7: Hand-apply two mutations**

1. Change `if keeps_documents == Some(false)` to `if keeps_documents == Some(true)`.
   Expected: `a_root_that_keeps_documents_still_writes_one` FAILS.
2. Change `root_keeps_documents`'s `.optional()` result so `None` becomes `Some(false)` (wrap it in `.map(|v| v.or(Some(false)))`).
   Expected: `a_work_with_no_rooted_member_has_no_root_to_ask` FAILS.

Revert both.

- [ ] **Step 8: Commit**

```bash
git add crates/taiyaku-library/src/hand.rs crates/taiyaku-library/src/hand/tests.rs \
        crates/taiyaku-serve/src/test_support.rs
git commit -m "hand::save writes nothing for a root that keeps no documents"
```

---

### Task 3: `reconcile` never repairs in an off root

This is the task the whole change turns on — though not, it turns out, because this guard alone stands between an off root and a recreated document. **`hand::save` already refuses the write for a declined root (Task 2), so without this guard `reconcile`'s gap loop still hands every gap to `save`, which still writes nothing — a hand-applied mutation during implementation confirmed it, reporting `repaired: 1` with no `.taiyaku` directory created.** What actually breaks without this guard is the report: `save` answers `persisted: true` for a write it never attempted, and the loop below counts that as a repair, so `reconcile` claims documents were written that never touched disk. The two guards are independent refusals — this one keeps the report honest, `save`'s keeps the disk honest — and the failure this task guards against is a false `repaired` count, not a resurrected file.

**Files:**
- Modify: `crates/taiyaku-library/src/hand.rs`
- Test: `crates/taiyaku-library/src/hand/tests.rs`

**Interfaces:**
- Consumes: `hand::reconcile(conn, root: &scan::Root) -> Result<Reconciled, String>`; `scan::Root::keeps_documents` from Task 1
- Produces: no signature change. `Reconciled::repaired` stays `0` and `unwritable` stays empty for a root that keeps no documents; `orphans` and `stale` are still populated.

- [ ] **Step 1: Write the failing tests**

Add to `crates/taiyaku-library/src/hand/tests.rs`:

```rust
/// **The one way this change can fail silently.** A marked work whose address
/// holds no file is a gap, and `reconcile` repairs a gap by writing the file.
/// In a root that keeps no documents every marked work looks exactly like
/// that — so without the branch under test, the next scan would recreate
/// every document the setting exists to prevent, and the setting would appear
/// to work and then quietly undo itself.
#[test]
fn an_off_root_is_never_repaired_into_growing_documents() {
    let dir = crate::tests::tempdir("hand-reconcile-off-no-repair");
    let root = dir.path.to_string_lossy().to_string();
    let conn = Connection::open_in_memory().expect("in-memory database");
    let work_id = a_scanned_work_rooted_at(&conn, &root);

    // A marked work with no file: exactly the shape of a gap.
    let mut doc = crate::librarydoc::of(&conn, work_id).expect("projecting");
    doc.title = Some("Hand Set".to_string());
    crate::librarydoc::apply(&conn, work_id, &doc).expect("applying");
    super::save(&conn, work_id).expect("saving");
    assert!(
        crate::migrate::absorbed(&conn, work_id, crate::migrate::LIBRARY)
            .expect("reading the marker"),
        "the fixture only means anything if the marker is armed"
    );

    let root_row = crate::scan::roots(&conn).expect("reading roots").remove(0);
    assert!(!root_row.keeps_documents, "the fixture's root is off");
    let found = super::reconcile(&conn, &root_row).expect("reconciling");

    assert_eq!(found.repaired, 0, "nothing was repaired: {found:?}");
    assert_eq!(found.unwritable.len(), 0, "and nothing was attempted: {found:?}");
    assert!(
        !dir.path.join(".taiyaku").exists(),
        "and no document was written behind the setting's back"
    );
}

/// A document left on disk from before the setting changed is inert but real,
/// and a person should be told once rather than finding it years later.
/// **Nothing is deleted** — the rule this module was built on does not bend
/// for a setting.
#[test]
fn an_off_root_still_reports_a_document_left_behind() {
    let dir = crate::tests::tempdir("hand-reconcile-off-orphan");
    let root = dir.path.to_string_lossy().to_string();
    let conn = Connection::open_in_memory().expect("in-memory database");
    let work_id = a_scanned_work_rooted_at(&conn, &root);

    // Written while the root kept documents, then the setting is turned off.
    conn.execute("UPDATE root SET keeps_documents = 1", [])
        .expect("on");
    let mut doc = crate::librarydoc::of(&conn, work_id).expect("projecting");
    doc.title = Some("Hand Set".to_string());
    crate::librarydoc::apply(&conn, work_id, &doc).expect("applying");
    let saved = super::save(&conn, work_id).expect("saving");
    let path = saved.path.expect("an on root has an address");
    assert!(path.is_file(), "the document exists before the setting changes");
    conn.execute("UPDATE root SET keeps_documents = 0", [])
        .expect("off");

    let root_row = crate::scan::roots(&conn).expect("reading roots").remove(0);
    let found = super::reconcile(&conn, &root_row).expect("reconciling");

    assert_eq!(
        found.orphans.len(),
        1,
        "the document left behind is reported: {found:?}"
    );
    assert_eq!(
        found.orphans[0].title.as_deref(),
        Some("Hand Set"),
        "with the title, because the filename is a hash"
    );
    assert!(path.is_file(), "and it is still on disk — nothing is deleted");
}

/// **The spec claims `rename_for_rekey` needs no change in an off root; this
/// is what makes that a checked claim rather than a reading.** It looks for a
/// file at the old address and finds none, so it answers `false` and touches
/// nothing — no new branch, no new failure mode, and in particular it does
/// not create the address it was asked to move a document to.
#[test]
fn a_rekey_in_an_off_root_moves_nothing_and_creates_nothing() {
    let dir = crate::tests::tempdir("hand-rekey-documents-off");
    let root = dir.path.to_string_lossy().to_string();
    let conn = Connection::open_in_memory().expect("in-memory database");
    let work_id = a_scanned_work_rooted_at(&conn, &root);

    let mut doc = crate::librarydoc::of(&conn, work_id).expect("projecting");
    doc.title = Some("Hand Set".to_string());
    crate::librarydoc::apply(&conn, work_id, &doc).expect("applying");
    super::save(&conn, work_id).expect("saving writes nothing here");

    conn.execute("UPDATE work SET content_key = 'moved' WHERE id = ?1", [work_id])
        .expect("rekeying");

    let moved = super::rename_for_rekey(&conn, work_id, "abc123").expect("renaming");

    assert!(!moved, "there was no document to move: {moved}");
    assert!(
        !dir.path.join(".taiyaku").exists(),
        "and asking to move one did not create the directory"
    );
}
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p taiyaku-library hand::tests::an_off_root -- --nocapture`
Expected: `an_off_root_is_never_repaired_into_growing_documents` FAILS — `found.repaired` is 1 and `.taiyaku` exists, which is precisely the defect.

- [ ] **Step 3: Branch the gap loop**

In `reconcile`, guard the gap loop — not the whole function, because the orphan walk above it must still run:

```rust
    // **A root that keeps no documents owes none, so it has no gaps.**
    // Without this the loop below reads every marked work as a gap and
    // repairs it by writing the file, which would have the next scan recreate
    // every document the setting exists to prevent — the setting appearing to
    // work and then undoing itself. The orphan walk above still runs: a
    // document left from before the setting changed is inert but real, and is
    // reported rather than deleted.
    if !root.keeps_documents {
        return Ok(found);
    }
```

placed after the directory walk that fills `found.orphans` and `found.stale`, and before the gap loop.

- [ ] **Step 4: Run to verify they pass**

Run: `cargo test -p taiyaku-library`
Expected: PASS, including every test from Tasks 1 and 2.

- [ ] **Step 5: Hand-apply the mutation that matters**

Delete the `if !root.keeps_documents { return Ok(found); }` guard entirely and re-run.
Expected: `an_off_root_is_never_repaired_into_growing_documents` FAILS. **If it passes, the test is not doing its job — its pass state and its bug state differ only by a file nobody looked for, so check the assertion actually reads the filesystem.** Revert.

- [ ] **Step 6: Commit**

```bash
git add crates/taiyaku-library/src/hand.rs crates/taiyaku-library/src/hand/tests.rs
git commit -m "reconcile never repairs a root that keeps no documents"
```

---

### Task 4: the response's third state, and the editor

**Files:**
- Modify: `crates/taiyaku-serve/src/metadata.rs`, `web/src/metadata.ts`, `web/src/MetadataEditor.tsx`
- Test: `crates/taiyaku-serve/src/metadata/tests.rs`, `web/src/MetadataEditor.test.tsx`

**Interfaces:**
- Consumes: `hand::Saved { path, persisted, why, keeps_documents }` from Task 2; `metadata::merge_worst_case(body, persisted, why)`
- Produces: the PATCH body gains `keeps_documents: false`, **emitted only when the root declined**; `PatchResult` in `web/src/metadata.ts` gains `keeps_documents?: boolean`

- [ ] **Step 1: Write the failing server test**

Add to `crates/taiyaku-serve/src/metadata/tests.rs`. The idiom is that file's own — `router_over_a_single_scanned_work()`, `send_json`, `test_support::body_string` — and Task 2 made that fixture keep documents, so the off case turns it off explicitly:

```rust
/// A save against a root that keeps no documents reports success and says so,
/// so a client has something to key off other than the absence of a failure.
/// `persisted: false` keeps meaning "a write was attempted and failed" rather
/// than becoming permanent furniture on every default install.
#[tokio::test]
async fn patching_a_work_in_a_root_that_keeps_no_documents_says_so() {
    let (router, state, _fixture) = test_support::router_over_a_single_scanned_work();
    let slug = state.works[0].id.clone();
    // The fixture keeps documents so the rest of this file can assert on the
    // file; this test is about the other case, so it says so.
    state
        .library
        .conn()
        .execute("UPDATE root SET keeps_documents = 0", [])
        .expect("the root keeps no documents");

    let response = send_json(
        &router,
        Method::PATCH,
        &format!("/api/library/works/{slug}"),
        &serde_json::json!({ "title": "Hand Set" }),
    )
    .await;
    let status = response.status();
    let text = test_support::body_string(response).await;
    assert_eq!(status, StatusCode::OK, "{text}");
    let body: serde_json::Value = serde_json::from_str(&text).expect("a JSON body");

    assert_eq!(body["title"], "Hand Set", "{body}");
    assert_eq!(body["persisted"], true, "nothing was owed: {body}");
    assert_eq!(body["keeps_documents"], false, "and the answer says why: {body}");
    assert!(body.get("why").is_none(), "no failure to name: {body}");
    assert!(
        !the_hand_document_path(&state).exists(),
        "and no document was written"
    );
}

/// And a root that keeps them does not carry the field at all — absent means
/// the ordinary case, the same rule `why` follows and the same one
/// `a_clean_edit_omits_why_entirely_rather_than_sending_an_empty_array` pins
/// for its own field.
#[tokio::test]
async fn patching_a_work_in_a_root_that_keeps_documents_omits_the_field() {
    let (router, state, _fixture) = test_support::router_over_a_single_scanned_work();
    let slug = state.works[0].id.clone();

    let response = send_json(
        &router,
        Method::PATCH,
        &format!("/api/library/works/{slug}"),
        &serde_json::json!({ "title": "Hand Set" }),
    )
    .await;
    let text = test_support::body_string(response).await;
    let body: serde_json::Value = serde_json::from_str(&text).expect("a JSON body");

    assert_eq!(body["persisted"], true, "{body}");
    assert!(
        body.get("keeps_documents").is_none(),
        "absent unless the root declined: {body}"
    );
}
```

If `state.library.conn()` is not how this file reaches the database, use whatever `the_hand_document_path(&state)` and its neighbours use — do not introduce a second way in.

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p taiyaku-serve keeps_no_documents -- --nocapture`
Expected: FAIL — `body["keeps_documents"]` is `Null`.

- [ ] **Step 3: Carry it out of the handler**

`merge_worst_case` folds several `Saved` values into one answer, so the field it emits must be the worst case in the same sense the others are. Extend it:

```rust
fn merge_worst_case(
    body: &mut serde_json::Value,
    persisted: bool,
    why: Vec<String>,
    keeps_documents: Option<bool>,
) {
    body["persisted"] = serde_json::Value::Bool(persisted);
    if !why.is_empty() {
        body["why"] =
            serde_json::Value::Array(why.into_iter().map(serde_json::Value::String).collect());
    }
    // **Emitted only when a root declined**, absent otherwise — the rule
    // `why` above already follows. `Some(true)` is the ordinary case and says
    // nothing; `None` is a work with no rooted member, which `path`'s own
    // `None` has always covered and which this field does not re-report.
    if keeps_documents == Some(false) {
        body["keeps_documents"] = serde_json::Value::Bool(false);
    }
}
```

Update `worst_case`, which folds a `Vec<Saved>` for the fan-out routes, to fold `keeps_documents` the same way: `Some(false)` if **every** save reported it, otherwise `None` — a work whose siblings span an on root and an off root is not wholly document-less, and saying so would be a claim the answer cannot support. Update all four call sites of `merge_worst_case`.

- [ ] **Step 4: Run to verify they pass**

Run: `cargo test -p taiyaku-serve`
Expected: PASS.

- [ ] **Step 5: The client type and the editor test**

In `web/src/metadata.ts`, extend `PatchResult`:

```ts
export interface PatchResult extends LibraryDoc {
  persisted: boolean
  why?: string[]
  /**
   * Present, and `false`, only when the work's root keeps no documents at
   * all — absent means the ordinary case, the same rule `why` follows.
   *
   * The editor needs no branch for it: a root that keeps no documents answers
   * `persisted: true`, and the "not durable" note already keys off
   * `persisted`. It is here so a future screen can say *this library keeps its
   * corrections in the database only* without re-deriving that from a silence.
   */
  keeps_documents?: boolean
}
```

Add to `web/src/MetadataEditor.test.tsx`:

```tsx
it('says nothing about durability when the root keeps no documents', async () => {
  const patch = vi.fn().mockResolvedValue({
    title: 'Hand Set',
    persisted: true,
    keeps_documents: false,
  })
  render(<MetadataEditor slug="w" title="Derived" onPatch={patch} />)
  // …drive the save the way the neighbouring tests in this file do…

  expect(screen.queryByText(/not durable/i)).toBeNull()
})
```

Match the file's existing helpers for rendering and submitting rather than introducing a new pattern; copy the assertion text from whatever the "unwritten" note actually renders.

- [ ] **Step 6: Run the web tests**

Run: `cd web && npx vitest run src/MetadataEditor.test.tsx`
Expected: PASS. **`MetadataEditor.tsx` should need no change** — it already renders the note only when `persisted` is false, and this case is true. If the test passes without touching the component, that is the correct outcome and the test is what pins it.

- [ ] **Step 7: Commit**

```bash
git add crates/taiyaku-serve/src/metadata.rs crates/taiyaku-serve/src/metadata/tests.rs \
        web/src/metadata.ts web/src/MetadataEditor.test.tsx
git commit -m "the patch response names a root that keeps no documents"
```

---

### Task 5: the claim §4.4 can no longer make unconditionally

**Files:**
- Modify: `docs/superpowers/specs/2026-08-14-library-store-design.md`, `docs/status.md`

- [ ] **Step 1: Qualify §4.4**

§4.4 says hand-fixed metadata round-trips to a file *"so that losing the database costs a rescan rather than costing the organizational work, which is the part nothing can re-derive."* That is now true only of a root whose `keeps_documents` is set.

Add a marked paragraph to §4.4 — leave the existing text as written, this project's practice for a claim that narrows — recording that the `<root>/.taiyaku/` document is off by default as of 2026-09-02, that `<stem>.library.json` beside a sidecar is unaffected because a person asks for it by running `export`, and that for a root which keeps no documents, losing the database without a backup loses the corrections. Name `root.keeps_documents` and the design doc.

- [ ] **Step 2: Update the test count at both sites**

Run: `cargo test --workspace 2>&1 | grep "^test result:" | awk '{p+=$4} END {print p}'`

Update **both** occurrences in `docs/status.md`: "Six crates, N tests" and "Nineteen of the N are standing checks" about two lines later.

- [ ] **Step 3: Run the count sweep**

```bash
grep -c '^| O[0-9]' docs/status.md              # expect 90
grep -c '^| O[0-9]* | \[x\]' docs/status.md     # expect 45
grep -n -A1 '^What remains:' docs/status.md     # expect "forty-five"
```

Nothing opens and nothing closes here, so all three must be unchanged. The "What remains" count word **wraps across a line break**, so grep the sentence opener and read the next line — grepping the phrase alone finds nothing and reads as already-correct when it is not.

- [ ] **Step 4: Run the gate**

Run: `env -u TAIYAKU_CORPUS scripts/check-all.sh`
Expected: the literal line `all green`. Do not pipe through `tail`; do not append `; echo`.

- [ ] **Step 5: Commit**

```bash
git add docs/superpowers/specs/2026-08-14-library-store-design.md docs/status.md
git commit -m "store spec: the durability claim now holds per root"
```

---

## What this plan deliberately does not do

- **No deletion or migration of documents already on disk.** Turning the setting off leaves them where they are; `reconcile` reports them as orphans so they are not discovered years later.
- **No change to `<stem>.library.json` beside a sidecar.** It is part of the sidecar format, written by `export::work` when asked. It is not littering — it was requested.
- **No UI control for the setting.** Spec C4 owns configuration as a screen and gives this column a control when it gives `root`'s others theirs. Until then it is set with `UPDATE root SET keeps_documents = 1`.
- **No answer to whether the address should be content-derived at all.** §8 of the spec records why that is the larger question this leaves open.
