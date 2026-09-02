# Hand Document Reconciliation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make a stranded, missing or stale `<root>/.taiyaku/<content_key>.library.json` impossible to lose silently — repairing what is unambiguous and reporting what is not.

**Architecture:** One new function, `hand::reconcile`, runs at the end of `derive()` once per root: it lists `<root>/.taiyaku/`, joins it against the works that owe a document (`migrated_document` where `document = 'library' AND absorbed = 1`), writes any document that is missing, and reports orphans and stale files without touching them. A second, smaller change renames a work's document at the one moment both its old and new `content_key` are known — `import_with`'s address-probe hit — because after that transaction commits the link is unrecoverable.

**Tech Stack:** Rust, `rusqlite`, `serde_json`. All changes are inside `crates/taiyaku-library`. No new dependency, no new route, no new CLI subcommand.

**Spec:** [`../specs/2026-08-31-hand-document-reconciliation-design.md`](../specs/2026-08-31-hand-document-reconciliation-design.md)
**Measurement it rests on:** [`../../notes.md`](../../notes.md), *"O81 and O82, measured against real archives — 2026-08-31"*

## Global Constraints

- **Never use `set_readonly` to make a directory unwritable in a test.** CI runs as root and ignores the write bit; a test written that way passes for every unprivileged developer and fails on the forge. Plant a plain file where the directory must go — `std::fs::write(root.join(".taiyaku"), b"not a directory")` — which makes `create_dir_all` fail with `AlreadyExists` for any uid. This is recorded at `hand/tests.rs`'s `a_root_that_cannot_take_the_document_still_arms_the_guard`, which was fixed for exactly this on 2026-08-29.
- **`LibraryDoc::updated` is stamped fresh on every projection** and is not stored anywhere (`librarydoc::of`'s own doc comment). Any comparison between a projected document and one read off disk must normalise `updated` first, or every reachable document reads as stale.
- **A failed write is counted and named, never raised.** `hand::save` already answers `persisted: false` rather than an error for a root that cannot take the document; `reconcile` keeps that contract. A scan must not die because one root is read-only.
- **Name sites by symbol, never by line number**, in every doc comment and in `docs/status.md`.
- **Every new `DeriveReport` field needs a doc comment saying why it is a field**, under the rule the existing ones state: *any derivation outcome that changes what a person sees and cannot be inferred from the other counts gets a field.*
- **Run the gate as `env -u TAIYAKU_CORPUS scripts/check-all.sh`** and confirm the literal line `all green`. Never pipe it through `tail` or append `; echo` — that masks the exit status.
- **The test count in `docs/status.md` appears twice** ("Five crates, N tests" and "Nine of the N are standing checks") and CI checks it against the JUnit report. Adding tests moves it at both sites.

## File Structure

| File | Responsibility | Change |
| ---- | -------------- | ------ |
| `crates/taiyaku-library/src/hand.rs` | the document's address, and now its reconciliation | add `Reconciled`, `Orphan`, `reconcile`, `rename_for_rekey` |
| `crates/taiyaku-library/src/hand/tests.rs` | unit tests for the above | add tests for all four states |
| `crates/taiyaku-library/src/derive.rs` | the walk, and where outcomes are reported | 4 new `DeriveReport` fields; call `reconcile` before `Ok(report)` |
| `crates/taiyaku-library/src/derive/tests.rs` | derive-level tests | the end-to-end durability test |
| `crates/taiyaku-library/src/import.rs` | identity reconciliation | return the old key from the rekey; call `rename_for_rekey` after commit |
| `crates/taiyaku-cli/src/library.rs` | the scan summary a person reads | new clauses in `derive_summary` |
| `docs/status.md` | the open-items table | O81/O82 annotations, test counts |

`hand.rs` is 300 lines and gains roughly 150; it stays one file because the address arithmetic and the reconciliation over that arithmetic are one responsibility, and splitting them would put `path_for` and its only non-trivial consumer in different files.

---

# PR 1 — the safety net

Branch: `hand-reconcile-report`. Lands first: it makes every already-stranded document in an existing library visible, and it does not touch the import path.

---

### Task 1: `Reconciled` and `Orphan`, and `reconcile` finding orphans and repairing gaps

Detection and repair of a gap are one task, deliberately. Splitting them would
mean an intermediate state in which a gap nothing has tried to write is
reported as `unwritable`, which is not true of it.

**Files:**
- Modify: `crates/taiyaku-library/src/hand.rs`
- Test: `crates/taiyaku-library/src/hand/tests.rs`

**Interfaces:**
- Consumes: `hand::path_for(conn, work_id) -> Result<Option<PathBuf>, String>`; `hand::save(conn, work_id) -> Result<Saved, String>` with `Saved { path, persisted, why }`; `scan::Root { id, path: PathBuf, .. }`; `migrate::LIBRARY`
- Produces:
  - `pub struct Orphan { pub path: PathBuf, pub title: Option<String> }`
  - `pub struct Reconciled { pub repaired: usize, pub unwritable: Vec<String>, pub orphans: Vec<Orphan>, pub stale: Vec<String> }` — `stale` stays empty until Task 2
  - `pub fn reconcile(conn: &Connection, root: &Root) -> Result<Reconciled, String>`

- [ ] **Step 1: Write the failing tests**

Add to `crates/taiyaku-library/src/hand/tests.rs`:

```rust
/// A document whose work has been rekeyed under it is an **orphan**, and the
/// report has to carry the title out of it. The filename is a hash of content
/// that no longer exists, so a report naming only the path gives a person
/// nothing to act on — and since O80 closed, a title-only document carries no
/// series, no volume and no member digests to identify it by any other route.
#[test]
fn a_document_no_work_addresses_is_an_orphan_named_with_its_title() {
    let dir = crate::tests::tempdir("hand-reconcile-orphan");
    let root = dir.path.to_string_lossy().to_string();
    let conn = Connection::open_in_memory().expect("in-memory database");
    let work_id = a_scanned_work_rooted_at(&conn, &root);

    let mut doc = crate::librarydoc::of(&conn, work_id).expect("projecting");
    doc.title = Some("Hand Set".to_string());
    crate::librarydoc::apply(&conn, work_id, &doc).expect("applying");
    super::save(&conn, work_id).expect("saving");

    // The volume's bytes changed: a repack, or a page added. `import` follows
    // the row to the new key, and the document does not follow it.
    conn.execute(
        "UPDATE work SET content_key = 'moved' WHERE id = ?1",
        [work_id],
    )
    .expect("rekeying the work");

    let root_row = crate::scan::roots(&conn).expect("reading roots").remove(0);
    let found = super::reconcile(&conn, &root_row).expect("reconciling");

    assert_eq!(found.orphans.len(), 1, "the stranded document is an orphan");
    assert_eq!(
        found.orphans[0].title.as_deref(),
        Some("Hand Set"),
        "the report must carry the title, because the filename is a hash of \
         content that no longer exists and nothing else identifies the work"
    );
    assert!(
        found.orphans[0]
            .path
            .to_string_lossy()
            .contains("abc123.library.json"),
        "named by the address it is actually at: {:?}",
        found.orphans[0].path
    );
}

/// **O82's claim, driven.** A correction made while the root could not take
/// the document is written on the next pass once it can. Before this, nothing
/// re-attempted the write and no command existed to sweep — measured on
/// 2026-08-31 by restoring write permission and running a full scan and
/// derive, after which the document still held the superseded title.
#[test]
fn a_gap_is_written_on_the_next_pass() {
    let dir = crate::tests::tempdir("hand-reconcile-retry");
    let root = dir.path.join("root");
    std::fs::create_dir(&root).expect("creating the root");
    let conn = Connection::open_in_memory().expect("in-memory database");
    let work_id = a_scanned_work_rooted_at(&conn, &root.to_string_lossy());

    // **A file where the directory must go, not `set_readonly`.** CI runs as
    // root and ignores the write bit; this fails for any uid. See
    // `a_root_that_cannot_take_the_document_still_arms_the_guard`.
    std::fs::write(root.join(".taiyaku"), b"not a directory")
        .expect("planting a file where the directory must go");

    let mut doc = crate::librarydoc::of(&conn, work_id).expect("projecting");
    doc.title = Some("Hand Set".to_string());
    crate::librarydoc::apply(&conn, work_id, &doc).expect("applying");
    let saved = super::save(&conn, work_id).expect("saving must not fail");
    assert!(!saved.persisted, "the document could not reach the disk");

    // The obstruction is removed — the media became writable.
    std::fs::remove_file(root.join(".taiyaku")).expect("clearing the obstruction");

    let root_row = crate::scan::roots(&conn).expect("reading roots").remove(0);
    let found = super::reconcile(&conn, &root_row).expect("reconciling");

    assert_eq!(found.repaired, 1, "the gap was closed: {found:?}");
    assert_eq!(found.unwritable.len(), 0, "and nothing was left unwritable");
    assert_eq!(found.orphans.len(), 0, "and nothing was orphaned");

    let path = super::path_for(&conn, work_id)
        .expect("addressing")
        .expect("a rooted work has an address");
    let text = std::fs::read_to_string(&path).expect("the document is on disk now");
    assert!(
        text.contains("Hand Set"),
        "and it carries the correction: {text}"
    );
}

/// A root that stays unwritable reports the same failure every pass, naming
/// the path. Reported, never raised: a scan must not die because one root of
/// several cannot take a document.
#[test]
fn a_gap_that_cannot_be_written_is_named_and_does_not_fail_the_pass() {
    let dir = crate::tests::tempdir("hand-reconcile-still-unwritable");
    let root = dir.path.join("root");
    std::fs::create_dir(&root).expect("creating the root");
    let conn = Connection::open_in_memory().expect("in-memory database");
    let work_id = a_scanned_work_rooted_at(&conn, &root.to_string_lossy());

    std::fs::write(root.join(".taiyaku"), b"not a directory")
        .expect("planting a file where the directory must go");

    let mut doc = crate::librarydoc::of(&conn, work_id).expect("projecting");
    doc.title = Some("Hand Set".to_string());
    crate::librarydoc::apply(&conn, work_id, &doc).expect("applying");
    super::save(&conn, work_id).expect("saving must not fail");

    let root_row = crate::scan::roots(&conn).expect("reading roots").remove(0);
    let found = super::reconcile(&conn, &root_row).expect("reconciling must not fail");

    assert_eq!(found.repaired, 0, "nothing could be written");
    assert_eq!(found.unwritable.len(), 1, "and the failure is named");
    assert!(
        found.unwritable[0].contains(&root.to_string_lossy().to_string()),
        "naming the path that could not be written: {:?}",
        found.unwritable[0]
    );
}

/// **The control.** A library in which nothing has gone wrong must report
/// nothing at all — without this, a `reconcile` that returned every work as a
/// gap, or rewrote every document on every pass, would still pass the three
/// tests above.
#[test]
fn a_library_with_nothing_wrong_reports_nothing() {
    let dir = crate::tests::tempdir("hand-reconcile-clean");
    let root = dir.path.to_string_lossy().to_string();
    let conn = Connection::open_in_memory().expect("in-memory database");
    let work_id = a_scanned_work_rooted_at(&conn, &root);

    let mut doc = crate::librarydoc::of(&conn, work_id).expect("projecting");
    doc.title = Some("Hand Set".to_string());
    crate::librarydoc::apply(&conn, work_id, &doc).expect("applying");
    super::save(&conn, work_id).expect("saving");

    let root_row = crate::scan::roots(&conn).expect("reading roots").remove(0);
    let found = super::reconcile(&conn, &root_row).expect("reconciling");

    assert_eq!(found.orphans.len(), 0, "nothing orphaned");
    assert_eq!(found.stale.len(), 0, "nothing stale");
    assert_eq!(found.repaired, 0, "nothing needed repair");
    assert_eq!(found.unwritable.len(), 0, "nothing failed to write");
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p taiyaku-library hand::tests::a_document_no_work_addresses -- --nocapture`
Expected: FAIL to compile — `` cannot find function `reconcile` in module `super` ``

- [ ] **Step 3: Write the implementation**

Add to `crates/taiyaku-library/src/hand.rs`, after `load`:

```rust
/// A document under `<root>/.taiyaku` that no work in the library addresses.
///
/// **The title travels with the path because the path is a hash.** The
/// filename is the `content_key` the document was written under, which names
/// content that no longer exists; and since O80 closed (#97, *the document
/// projects only what a person set*) a title-only document — the common case,
/// and the one the metadata editor's single control produces — carries no
/// series, no volume and no member digests either. So there is no route back
/// to a work from the file's contents, and a report naming only the path
/// leaves a person nothing to act on. The title is the one actionable thing
/// in it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Orphan {
    pub path: PathBuf,
    /// `None` for a document that carries no title — one correcting only a
    /// series or an edition label. Reported anyway: the file is still
    /// somebody's work and still unreachable.
    pub title: Option<String>,
}

/// What one root's `.taiyaku` directory and the works that owe it documents
/// had to say about each other.
///
/// Three states, three answers, and only one of them is a repair — see the
/// design's §2 for why the stale case resolves to nothing. `repaired` and
/// `unwritable` partition the gaps; `orphans` and `stale` are reported and
/// touched by nothing.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Reconciled {
    /// Gaps closed on this pass: a marked work whose document was missing and
    /// has now been written.
    pub repaired: usize,
    /// Gaps that could not be closed, each carrying the `io::Error`'s own
    /// message naming the path — [`Saved::why`]'s shape, for the same reason.
    ///
    /// **A permanently read-only root reports the same list every scan, and
    /// that is correct rather than noisy**: the condition really is still
    /// true, and the alternative — reporting it once and going quiet — would
    /// leave a library whose corrections are not durable looking like one
    /// whose corrections are.
    pub unwritable: Vec<String>,
    /// Documents no work addresses. See [`Orphan`] for why each carries a
    /// title.
    pub orphans: Vec<Orphan>,
    /// Documents that exist at a work's address and disagree with the
    /// database. **Reported and not resolved**: the database is not
    /// automatically right, since O44 records this as the one document this
    /// project designs for hand-editing. Populated in Task 2.
    pub stale: Vec<String>,
}

/// Reconcile one root's `.taiyaku` directory against the works that owe it a
/// document.
///
/// **One `read_dir` per root, and it runs inside `derive`** — which already
/// walks every work and already calls [`load`] per work, so this adds one
/// directory listing to a pass that is hashing archives.
///
/// **`absorbed = 1` is the register of works that owe a document**, not the
/// presence of a file: `save` records the marker whether or not the write
/// landed, which is exactly what makes a gap detectable at all.
///
/// # Errors
///
/// Returns an error if the root's directory cannot be listed for a reason
/// other than its absence, or if SQLite fails. **Not** if a document cannot be
/// written — that is [`Reconciled::unwritable`].
pub fn reconcile(conn: &Connection, root: &crate::scan::Root) -> Result<Reconciled, String> {
    let mut found = Reconciled::default();
    let dir = root.path.join(DIR);

    // Every work that owes a document, with the address it owes it at.
    // Addresses outside this root belong to another root's pass — a work
    // spans roots by design, and `path_for` answers with exactly one of them.
    let mut expected: std::collections::HashMap<PathBuf, i64> = std::collections::HashMap::new();
    let mut owed: Vec<i64> = Vec::new();
    {
        let mut stmt = conn
            .prepare(
                "SELECT work_id FROM migrated_document
                  WHERE document = ?1 AND absorbed = 1
                  ORDER BY work_id",
            )
            .map_err(|e| format!("preparing the marked-work query: {e}"))?;
        let rows = stmt
            .query_map([crate::migrate::LIBRARY], |row| row.get::<_, i64>(0))
            .map_err(|e| format!("reading marked works: {e}"))?;
        for row in rows {
            owed.push(row.map_err(|e| format!("reading a marked work: {e}"))?);
        }
    }
    for work_id in owed {
        let Some(path) = path_for(conn, work_id)? else {
            // No address at all — a sidecar-imported work, whose document
            // lives beside its sidecar and is `export::work`'s to write. Out
            // of scope by design; see the design's §7.
            continue;
        };
        if path.parent() == Some(dir.as_path()) {
            expected.insert(path, work_id);
        }
    }

    // A root that has never taken a hand edit has no `.taiyaku` directory,
    // which is the ordinary case rather than a failure — `save` creates it on
    // first write for exactly that reason.
    let entries = match std::fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new().into_iter().collect(),
        Err(e) => return Err(format!("{}: {e}", dir.display())),
    };

    let mut present: std::collections::HashSet<PathBuf> = std::collections::HashSet::new();
    for entry in entries {
        let entry = entry.map_err(|e| format!("{}: {e}", dir.display()))?;
        let path = entry.path();
        if !path.to_string_lossy().ends_with(".library.json") {
            continue;
        }
        present.insert(path.clone());
        if expected.contains_key(&path) {
            continue;
        }
        // Unreachable. Read the title out of it if we can; a document we
        // cannot parse is still an orphan, just one with less to say.
        let title = std::fs::read_to_string(&path)
            .ok()
            .and_then(|text| crate::librarydoc::LibraryDoc::from_json(&text).ok())
            .and_then(|doc| doc.title);
        found.orphans.push(Orphan { path, title });
    }

    for (path, work_id) in &expected {
        if present.contains(path) {
            continue;
        }
        // **A gap, and this is the retry O82 says does not exist.** Written
        // through `save` rather than a second writer, so the marker, the
        // empty-document removal and the failure shape are all the ones the
        // PATCH routes already produce.
        let saved = save(conn, *work_id)?;
        if saved.persisted {
            found.repaired += 1;
        } else {
            found.unwritable.push(
                saved
                    .why
                    .unwrap_or_else(|| format!("{}: could not be written", path.display())),
            );
        }
    }

    Ok(found)
}
```

**Note on the `read_dir` early return.** The `Vec::new().into_iter().collect()`
above is written to keep both match arms one type; if that fights the borrow
checker or reads badly, return `Ok(found)` from the `NotFound` arm instead and
restructure the function so the directory walk sits behind that early return.
Either shape is fine — what must not change is that a missing `.taiyaku`
directory is the ordinary case and produces an empty `Reconciled`, never an
error and never a report of every work as a gap.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p taiyaku-library hand::tests -- --nocapture`
Expected: PASS, including the four new tests and every existing `hand` test.

- [ ] **Step 5: Hand-apply two mutations and watch a test fail each time**

The two-minute defence this file's own fixtures document.

1. Change `found.orphans.push(Orphan { path, title });` to `found.orphans.push(Orphan { path, title: None });`
   Expected: `a_document_no_work_addresses_is_an_orphan_named_with_its_title` FAILS on the title assertion.
2. Change `if saved.persisted {` to `if true {`
   Expected: `a_gap_that_cannot_be_written_is_named_and_does_not_fail_the_pass` FAILS on `found.repaired`.

Revert both.

- [ ] **Step 6: Commit**

```bash
git add crates/taiyaku-library/src/hand.rs crates/taiyaku-library/src/hand/tests.rs
git commit -m "hand::reconcile finds orphans and writes the documents a failed save left missing"
```

---

### Task 2: staleness, compared with `updated` normalised

**Files:**
- Modify: `crates/taiyaku-library/src/hand.rs`
- Test: `crates/taiyaku-library/src/hand/tests.rs`

**Interfaces:**
- Consumes: `librarydoc::of(conn, work_id) -> Result<LibraryDoc, String>`; `LibraryDoc::from_json(&str)`; `LibraryDoc { updated: String, .. }`
- Produces: `Reconciled::stale` is populated by `reconcile`; no signature change.

- [ ] **Step 1: Write the failing test**

Add to `crates/taiyaku-library/src/hand/tests.rs`:

```rust
/// **The third state, and the one neither O81 nor O82 named.** A document that
/// exists at the right address and disagrees with the database is neither a
/// gap nor an orphan, so a sweep counting only presence walks straight past
/// it — and a rebuild then reads the older title back over the newer one.
/// Measured on 2026-08-31; see `docs/notes.md`.
#[test]
fn a_document_that_disagrees_with_the_database_is_stale_and_is_not_rewritten() {
    let dir = crate::tests::tempdir("hand-reconcile-stale");
    let root = dir.path.to_string_lossy().to_string();
    let conn = Connection::open_in_memory().expect("in-memory database");
    let work_id = a_scanned_work_rooted_at(&conn, &root);

    let mut doc = crate::librarydoc::of(&conn, work_id).expect("projecting");
    doc.title = Some("First Correction".to_string());
    crate::librarydoc::apply(&conn, work_id, &doc).expect("applying");
    super::save(&conn, work_id).expect("saving");

    // A second edit that never reached the disk — the read-only-root case,
    // reproduced by editing the row directly rather than by permissions.
    let mut doc = crate::librarydoc::of(&conn, work_id).expect("projecting");
    doc.title = Some("Second Correction".to_string());
    crate::librarydoc::apply(&conn, work_id, &doc).expect("applying");

    let path = super::path_for(&conn, work_id)
        .expect("addressing")
        .expect("a rooted work has an address");
    let before = std::fs::read_to_string(&path).expect("reading the document");

    let root_row = crate::scan::roots(&conn).expect("reading roots").remove(0);
    let found = super::reconcile(&conn, &root_row).expect("reconciling");

    assert_eq!(found.stale.len(), 1, "the disagreement is seen: {found:?}");
    assert_eq!(found.repaired, 0, "a stale document is not a gap");
    assert_eq!(
        std::fs::read_to_string(&path).expect("re-reading"),
        before,
        "a stale document is reported and touched by nothing — the database \
         is not automatically right, since O44 records this as the one \
         document this project designs for hand-editing"
    );
}

/// **The control that pins the `updated` normalisation.** `librarydoc::of`
/// stamps `updated` with the time the projection was taken and nothing in the
/// schema stores it, so a comparison that did not normalise it would call
/// every reachable document stale — including one written a moment ago.
#[test]
fn a_document_that_agrees_is_not_stale_despite_a_newer_updated_stamp() {
    let dir = crate::tests::tempdir("hand-reconcile-agrees");
    let root = dir.path.to_string_lossy().to_string();
    let conn = Connection::open_in_memory().expect("in-memory database");
    let work_id = a_scanned_work_rooted_at(&conn, &root);

    let mut doc = crate::librarydoc::of(&conn, work_id).expect("projecting");
    doc.title = Some("Hand Set".to_string());
    crate::librarydoc::apply(&conn, work_id, &doc).expect("applying");
    super::save(&conn, work_id).expect("saving");

    // Rewrite the file with a deliberately older stamp and the same content.
    let path = super::path_for(&conn, work_id)
        .expect("addressing")
        .expect("a rooted work has an address");
    let mut on_disk = crate::librarydoc::LibraryDoc::from_json(
        &std::fs::read_to_string(&path).expect("reading"),
    )
    .expect("parsing");
    on_disk.updated = "2001-01-01T00:00:00Z".to_string();
    std::fs::write(&path, on_disk.to_json()).expect("rewriting with an older stamp");

    let root_row = crate::scan::roots(&conn).expect("reading roots").remove(0);
    let found = super::reconcile(&conn, &root_row).expect("reconciling");

    assert_eq!(
        found.stale.len(),
        0,
        "only the content decides staleness, never the stamp: {found:?}"
    );
}
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p taiyaku-library hand::tests::a_document_that -- --nocapture`
Expected: `a_document_that_disagrees...` FAILS with `assertion left == right` on `found.stale.len()` — 0 against 1.

- [ ] **Step 3: Implement**

In `hand.rs`, inside `reconcile`'s directory loop, replace the `if expected.contains_key(&path) { continue; }` arm with:

```rust
        if let Some(&work_id) = expected.get(&path) {
            // Present and addressed. The remaining question is whether it
            // agrees with the database.
            //
            // **`updated` is normalised before the comparison, and must be.**
            // `librarydoc::of`'s own doc comment records that it is stamped
            // with the time the projection was taken and that nothing in the
            // schema holds it — so comparing it would call every reachable
            // document stale, including one written a moment ago.
            //
            // A file that cannot be read or parsed is **not** stale: it is
            // already `DeriveReport::unreadable_hand`'s, which the read-back
            // populates and this does not duplicate.
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            let Ok(mut on_disk) = crate::librarydoc::LibraryDoc::from_json(&text) else {
                continue;
            };
            let projected = crate::librarydoc::of(conn, work_id)?;
            on_disk.updated.clone_from(&projected.updated);
            if on_disk != projected {
                found.stale.push(path.display().to_string());
            }
            continue;
        }
```

`LibraryDoc` must derive `PartialEq` for this. If it does not already, add it to the existing derive list on `pub struct LibraryDoc`.

- [ ] **Step 4: Run to verify they pass**

Run: `cargo test -p taiyaku-library hand::tests -- --nocapture`
Expected: PASS, all tests including Task 1's.

- [ ] **Step 5: Hand-apply one mutation and watch a test fail**

Temporarily delete the `on_disk.updated.clone_from(&projected.updated);` line and re-run.
Expected: `a_document_that_agrees_is_not_stale_despite_a_newer_updated_stamp` FAILS, reporting 1 stale. Revert.

- [ ] **Step 6: Commit**

```bash
git add crates/taiyaku-library/src/hand.rs crates/taiyaku-library/src/hand/tests.rs
git commit -m "hand::reconcile reports a document that disagrees with the database"
```

---

### Task 3: wire `reconcile` into `derive()`

**Files:**
- Modify: `crates/taiyaku-library/src/derive.rs`
- Test: `crates/taiyaku-library/src/derive/tests.rs`

**Interfaces:**
- Consumes: `hand::reconcile(conn, &Root) -> Result<Reconciled, String>`; `hand::Orphan`
- Produces: `DeriveReport` gains `pub repaired_documents: usize`, `pub unwritable_documents: Vec<String>`, `pub orphan_documents: Vec<crate::hand::Orphan>`, `pub stale_documents: Vec<String>`

- [ ] **Step 1: Write the failing test**

Add to `crates/taiyaku-library/src/derive/tests.rs`:

```rust
/// **Where the sweep runs, and the ordering that makes it work.** `derive`
/// already walks every work and already calls `hand::load` per work; the
/// reconciliation runs at the end of the same pass, after `write_roots`.
/// Before that point `member.root_id` is still NULL on a freshly minted work,
/// `path_for`'s `JOIN root` matches nothing, and every work in a new library
/// would report as having no address at all.
#[test]
fn a_derive_reports_a_stranded_document() {
    let dir = archived_volumes();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, &dir.path).expect("add");
    scan::scan_root(&conn, &root).expect("scan");
    derive::derive(&conn).expect("first derive");

    let work_id: i64 = conn
        .query_row("SELECT id FROM work ORDER BY id LIMIT 1", [], |row| row.get(0))
        .expect("a work was derived");

    let mut doc = crate::librarydoc::of(&conn, work_id).expect("projecting");
    doc.title = Some("Hand Set".to_string());
    crate::librarydoc::apply(&conn, work_id, &doc).expect("applying");
    crate::hand::save(&conn, work_id).expect("saving");

    // The volume's bytes change under it: a repack.
    conn.execute(
        "UPDATE work SET content_key = 'moved' WHERE id = ?1",
        [work_id],
    )
    .expect("rekeying");

    let report = derive::derive(&conn).expect("second derive");

    assert_eq!(
        report.orphan_documents.len(),
        1,
        "the stranded document is reported by the pass that stranded it: {report:?}"
    );
    assert_eq!(
        report.orphan_documents[0].title.as_deref(),
        Some("Hand Set"),
        "with the title, because the filename is a hash"
    );
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p taiyaku-library derive::tests::a_derive_reports_a_stranded_document -- --nocapture`
Expected: FAIL to compile — `` no field `orphan_documents` on type `DeriveReport` ``

- [ ] **Step 3: Add the four fields**

In `derive.rs`, on `pub struct DeriveReport`, after `unreadable_hand`:

```rust
    /// Documents written on this pass that a failed `hand::save` had left
    /// missing — O82's retry.
    ///
    /// **A field rather than a silence, under the same rule as
    /// [`Self::suppressed`] and [`Self::rekeyed`]:** any derivation outcome
    /// that changes what a person sees and cannot be inferred from the other
    /// counts gets one. A correction becoming durable is exactly that.
    pub repaired_documents: usize,
    /// Documents that still could not be written, each naming the path and the
    /// `io::Error` — `hand::Saved::why`'s own shape.
    ///
    /// A `Vec` rather than a counter, for [`Self::unreadable_hand`]'s reason:
    /// the person has to be told *which* root cannot take its documents. A
    /// permanently read-only root reports the same list every scan, which is
    /// honest rather than noisy — the condition really is still true.
    pub unwritable_documents: Vec<String>,
    /// Documents under a root's `.taiyaku` that no work addresses — O81.
    ///
    /// Each carries the title out of the file, because the filename is the
    /// `content_key` the document was written under and names content that no
    /// longer exists. See [`crate::hand::Orphan`].
    pub orphan_documents: Vec<crate::hand::Orphan>,
    /// Documents at a work's own address that disagree with the database.
    ///
    /// **Reported and resolved by nothing**, deliberately: O44 records this as
    /// the one document this project designs for hand-editing, so the database
    /// is not automatically right. The design's §2 records the alternatives
    /// and what would earn a resolution rule.
    pub stale_documents: Vec<String>,
```

- [ ] **Step 4: Call it, at the end of `derive()`**

In `derive.rs`, immediately before `Ok(report)`:

```rust
    // **After everything, and after `write_roots` above in particular.**
    // `hand::path_for` resolves an address through `member.root_id`, which is
    // still NULL on a work this pass has just minted until `write_roots` has
    // run for it — the same ordering the read-back's own comment records, and
    // getting it wrong here would report every work in a new library as having
    // no address rather than as reconciled.
    for root in &roots {
        let found = crate::hand::reconcile(conn, root)?;
        report.repaired_documents += found.repaired;
        report.unwritable_documents.extend(found.unwritable);
        report.orphan_documents.extend(found.orphans);
        report.stale_documents.extend(found.stale);
    }

    Ok(report)
```

- [ ] **Step 5: Run the whole library suite**

Run: `cargo test -p taiyaku-library`
Expected: PASS. If a test that asserts a whole `DeriveReport` fails on the new fields, extend its expected value rather than loosening the assertion.

- [ ] **Step 6: Commit**

```bash
git add crates/taiyaku-library/src/derive.rs crates/taiyaku-library/src/derive/tests.rs
git commit -m "derive reports orphaned, repaired and stale hand documents"
```

---

### Task 4: the scan summary a person actually reads

**Files:**
- Modify: `crates/taiyaku-cli/src/library.rs` (`derive_summary`)
- Test: `crates/taiyaku-cli/src/library/tests.rs`, in the block headed
  `-- scan_summary / derive_summary: pure, so every branch is testable --`

**Interfaces:**
- Consumes: the four `DeriveReport` fields from Task 3
- Produces: no new API — clauses appended to the existing summary line

- [ ] **Step 1: Write the failing test**

Add to `crates/taiyaku-cli/src/library/tests.rs`, using that file's own
`derive_report()` helper. **Every count in the fixture differs from every
other**, which is the convention
`derive_summary_names_the_four_headline_counts_and_the_proposal_count`
documents: a fixture that reused a number let a mutation printing the wrong
field produce an identical line and survive.

```rust
/// The filename is a `content_key` naming content that no longer exists, so a
/// line naming only the path gives a person nothing to act on. The title is
/// the actionable half, and this is what pins it into the output.
#[test]
fn derive_summary_names_a_stranded_document_and_its_title() {
    let mut report = derive_report();
    report.repaired_documents = 2;
    report.orphan_documents.push(taiyaku_library::hand::Orphan {
        path: PathBuf::from("/library/.taiyaku/abc123.library.json"),
        title: Some("Go-Toubun no Hanayome".to_string()),
    });

    let line = derive_summary(&report, 0);

    assert!(line.contains("2 document(s) written"), "{line}");
    assert!(line.contains("abc123.library.json"), "{line}");
    assert!(
        line.contains("Go-Toubun no Hanayome"),
        "the title is the actionable half — the filename is a hash: {line}"
    );
}

/// An orphan with no title — a document correcting only a series or an
/// edition label — is still reported, and must not print an empty pair of
/// backticks where a title would go.
#[test]
fn derive_summary_names_a_titleless_orphan_without_empty_backticks() {
    let mut report = derive_report();
    report.orphan_documents.push(taiyaku_library::hand::Orphan {
        path: PathBuf::from("/library/.taiyaku/def456.library.json"),
        title: None,
    });

    let line = derive_summary(&report, 0);

    assert!(line.contains("def456.library.json"), "{line}");
    assert!(!line.contains("(``)"), "no empty title parens: {line}");
}

/// The three silent-outcome lists stay out of the line entirely when empty —
/// the same shape `scan_summary_omits_unreadable_directories_when_none` pins.
#[test]
fn derive_summary_omits_the_document_clauses_when_there_is_nothing_to_say() {
    let line = derive_summary(&derive_report(), 0);
    assert!(!line.contains("document(s)"), "{line}");
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p taiyaku-cli derive_summary_names_a_stranded_document -- --nocapture`
Expected: FAIL on the first assertion — the summary has no such clause.

- [ ] **Step 3: Implement**

In `derive_summary`, after the `report.rekeyed` clause and following the same `if … > 0` shape as its neighbours:

```rust
    if report.repaired_documents > 0 {
        let _ = write!(
            line,
            ", {} document(s) written that a failed save had left missing",
            report.repaired_documents
        );
    }
    if !report.unwritable_documents.is_empty() {
        let _ = write!(
            line,
            ", {} document(s) still not durable: {}",
            report.unwritable_documents.len(),
            report.unwritable_documents.join("; ")
        );
    }
    if !report.orphan_documents.is_empty() {
        // Path **and** title: the filename is a `content_key` naming content
        // that no longer exists, so the path alone gives a person nothing to
        // act on.
        let named: Vec<String> = report
            .orphan_documents
            .iter()
            .map(|orphan| match &orphan.title {
                Some(title) => format!("{} (`{title}`)", orphan.path.display()),
                None => orphan.path.display().to_string(),
            })
            .collect();
        let _ = write!(
            line,
            ", {} document(s) no work addresses: {}",
            named.len(),
            named.join("; ")
        );
    }
    if !report.stale_documents.is_empty() {
        let _ = write!(
            line,
            ", {} document(s) disagree with the database: {}",
            report.stale_documents.len(),
            report.stale_documents.join("; ")
        );
    }
```

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test -p taiyaku-cli`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/taiyaku-cli/src/library.rs crates/taiyaku-cli/src/tests.rs
git commit -m "the scan summary names stranded and repaired documents"
```

---

### Task 5: the durability test — rebuild the database and assert the correction survives

This is the check that would have caught all of this originally, and the existing suite could not, because it never rebuilt.

**Files:**
- Test: `crates/taiyaku-library/src/derive/tests.rs`

**Interfaces:**
- Consumes: everything from Tasks 1-4. No production change in this task; if it fails, the bug is in an earlier task.

- [ ] **Step 1: Write the test**

```rust
/// **Store spec §4.4 stated as a test:** *losing the database costs a rescan,
/// not the organisational work.* Measured failing on 2026-08-31 — three of
/// five works lost or reverted a hand-set title — and this is the assertion
/// that keeps it fixed.
///
/// A second, empty database over the **same root** is what a lost database
/// actually means. Nothing here shares a connection with the first.
#[test]
fn a_correction_survives_losing_the_database() {
    let dir = archived_volumes();
    correct_the_first_work(&dir);

    // The database is gone. Only the root remains — which is what losing it
    // actually means.
    assert_eq!(
        title_after_rebuilding(&dir),
        "Hand Set",
        "one rescan restores the correction — the whole point of writing the \
         document into the library rather than only into the database"
    );
}

/// Scan `dir` into a fresh database, hand-set the first work's title to
/// `Hand Set`, save the document beside the archives, and drop the database.
/// What is left on disk is a library and its documents, and nothing else.
fn correct_the_first_work(dir: &crate::tests::TempDir) {
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, &dir.path).expect("add");
    scan::scan_root(&conn, &root).expect("scan");
    derive::derive(&conn).expect("deriving");

    let work_id: i64 = conn
        .query_row("SELECT id FROM work ORDER BY id LIMIT 1", [], |row| row.get(0))
        .expect("a work was derived");
    let mut doc = crate::librarydoc::of(&conn, work_id).expect("projecting");
    doc.title = Some("Hand Set".to_string());
    crate::librarydoc::apply(&conn, work_id, &doc).expect("applying");
    let saved = crate::hand::save(&conn, work_id).expect("saving");
    assert!(saved.persisted, "the document reached the disk: {saved:?}");
}

/// Build a second, empty database over the same root and answer what the
/// first work's title is once the scan and derive have run.
fn title_after_rebuilding(dir: &crate::tests::TempDir) -> String {
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, &dir.path).expect("add");
    scan::scan_root(&conn, &root).expect("rescanning");
    derive::derive(&conn).expect("rederiving");
    conn.query_row("SELECT title FROM work ORDER BY id LIMIT 1", [], |row| {
        row.get(0)
    })
    .expect("the work came back")
}
```

- [ ] **Step 2: Run it**

Run: `cargo test -p taiyaku-library derive::tests::a_correction_survives_losing_the_database -- --nocapture`
Expected: PASS. Nothing moved the `content_key` here, so this passes on today's code too. That is the point: it is the **baseline** the Task 7 variant is measured against, and the two helpers it factors out are what that variant reuses.

- [ ] **Step 3: Commit**

```bash
git add crates/taiyaku-library/src/derive/tests.rs
git commit -m "a correction survives losing the database"
```

---

### Task 6: PR 1's documentation and gate

**Files:**
- Modify: `docs/status.md`

- [ ] **Step 1: Annotate O82, do not close it**

O82's row keeps `[ ]`. Append to its prose cell, inside the same table row and using no `|` character:

> *Partly answered on 2026-08-31: `hand::reconcile`, run per root at the end of `derive`, writes a document a failed save left missing and reports the two states it must not resolve. What stays open is the stale case — a document that exists and disagrees — which is reported and nothing else.*

Update the site column to `hand::save`, `hand::path_for`, `hand::reconcile`.

- [ ] **Step 2: Annotate O81, do not close it**

Append, same rules:

> *Partly answered on 2026-08-31: an orphan is now reported with the title it holds, so the loss is no longer silent. It is not prevented — that is PR 2's `rename_for_rekey`.*

- [ ] **Step 3: Update the test count at both sites**

Run: `cargo test -p taiyaku-library -p taiyaku-cli 2>&1 | tail -5` to get the new total, then update **both** occurrences in `docs/status.md`: "Five crates, N tests" and "Nine of the N are standing checks" two lines later.

- [ ] **Step 4: Run the count sweep**

```bash
grep -c '^| O[0-9]' docs/status.md
grep -c '^| O[0-9]* | \[x\]' docs/status.md
grep -n -A1 '^What remains:' docs/status.md
```
Expected: 88 rows, 45 closed, "the forty-three open items" — **unchanged**, because nothing opened or closed.

- [ ] **Step 5: Run the gate**

Run: `env -u TAIYAKU_CORPUS scripts/check-all.sh`
Expected: the literal line `all green`. Do not pipe through `tail`.

- [ ] **Step 6: Commit and open the PR**

```bash
git add docs/status.md
git commit -m "status: O81 and O82 partly answered"
git push -u origin hand-reconcile-report
```

Open the PR through the gitea MCP (`pull_request_write`, owner `davey`, repo `taiyaku`), not `gh`. **Do not hard-wrap the body** — the squash merge makes it the commit message verbatim.

---

# PR 2 — prevention

Branch: `hand-rename-on-rekey`, cut from `main` after PR 1 merges. If it is stacked instead, rebase with `git rebase --onto` after the squash deletes its base.

---

### Task 7: `rename_for_rekey`, after the commit

**Files:**
- Modify: `crates/taiyaku-library/src/hand.rs`, `crates/taiyaku-library/src/import.rs`
- Test: `crates/taiyaku-library/src/hand/tests.rs`, `crates/taiyaku-library/src/derive/tests.rs`

**Interfaces:**
- Consumes: `hand::path_for`; `import_with`'s `existing` resolution
- Produces: `pub fn rename_for_rekey(conn: &Connection, work_id: i64, old_key: &str) -> Result<bool, String>` — `true` when a document was moved, `false` when there was none to move. Never overwrites.

- [ ] **Step 1: Write the failing test**

Add to `crates/taiyaku-library/src/derive/tests.rs`:

```rust
/// **O81, prevented.** Repacking an archive moves the work's `content_key`;
/// the document must follow, because after this pass commits the old key is
/// gone from the database and the file is unattributable except by a person
/// opening it. Measured on 2026-08-31: without this, the correction is lost on
/// the next database rebuild.
#[test]
fn a_repacked_volume_takes_its_document_with_it() {
    let dir = archived_volumes();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, &dir.path).expect("add");
    scan::scan_root(&conn, &root).expect("scan");
    derive::derive(&conn).expect("deriving");

    let work_id: i64 = conn
        .query_row("SELECT id FROM work ORDER BY id LIMIT 1", [], |row| row.get(0))
        .expect("a work was derived");
    let mut doc = crate::librarydoc::of(&conn, work_id).expect("projecting");
    doc.title = Some("Hand Set".to_string());
    crate::librarydoc::apply(&conn, work_id, &doc).expect("applying");
    crate::hand::save(&conn, work_id).expect("saving");

    // **One page added — one of the two events the measurement used, and it
    // needs no new helper.** `write_archive` is deterministic and each page's
    // bytes name the archive holding them, so rewriting v01 with four pages
    // instead of three changes the member digest, and with it the work's
    // `content_key`. v02 already holds four pages and does not collide,
    // because the label differs — `write_archive`'s own comment says why it
    // is built that way.
    write_archive(&dir.path.join("Series").join("v01").join("Series v01.cbz"), 4);

    scan::scan_root(&conn, &root).expect("rescanning");
    let report = derive::derive(&conn).expect("rederiving");

    assert_eq!(report.rekeyed, 1, "the work followed its files: {report:?}");
    assert_eq!(
        report.orphan_documents.len(),
        0,
        "and its document followed the work: {report:?}"
    );

    let path = crate::hand::path_for(&conn, work_id)
        .expect("addressing")
        .expect("a rooted work has an address");
    assert!(
        path.is_file(),
        "the document is at the new address: {}",
        path.display()
    );
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p taiyaku-library derive::tests::a_repacked_volume_takes_its_document -- --nocapture`
Expected: FAIL — `report.orphan_documents.len()` is 1, which is PR 1's reporting correctly seeing the strand that PR 2 exists to prevent.

- [ ] **Step 3: Implement `rename_for_rekey`**

In `hand.rs`:

```rust
/// Move this work's document from the address it had under `old_key` to the
/// one it has now.
///
/// **Called at the one moment the link is knowable.** `import_with`'s probe
/// resolves a work by content first and by its `(series_id, volume)` address
/// on a miss; a hit on the second is a rekey, and it is the only point at
/// which the old and new `content_key` are both in hand. Once that transaction
/// commits the old key is gone from the database, and since O80 closed a
/// title-only document carries no series, no volume and no member digests — so
/// nothing can match the file to a work afterwards. See [`Orphan`].
///
/// **Never overwrites.** Two works cannot legitimately share an address, so a
/// file already at the target means something this was not designed for;
/// `false` comes back, the correction under the old key survives, and
/// [`reconcile`] reports it as an orphan for a person to read.
///
/// # Errors
///
/// Returns an error if SQLite fails resolving the new address. **Not** if the
/// rename fails: a root that cannot take the move leaves the document where it
/// is, which [`reconcile`] then reports.
pub fn rename_for_rekey(
    conn: &Connection,
    work_id: i64,
    old_key: &str,
) -> Result<bool, String> {
    let Some(new_path) = path_for(conn, work_id)? else {
        return Ok(false);
    };
    let Some(dir) = new_path.parent() else {
        return Ok(false);
    };
    let old_path = dir.join(format!("{old_key}.library.json"));
    if !old_path.is_file() || new_path.exists() {
        return Ok(false);
    }
    Ok(std::fs::rename(&old_path, &new_path).is_ok())
}
```

- [ ] **Step 4: Call it from `import_with`, after the commit**

In `import.rs`, capture the old key when the address probe is what resolved the work, and apply the rename **after** `tx.commit()`:

```rust
    // The old key, kept only when the *address* resolved this work — a
    // content-key hit means nothing moved. Read before `update_work` writes
    // the new key over it.
    let rekeyed_from = match existing {
        Some(id) if by_address => conn
            .query_row("SELECT content_key FROM work WHERE id = ?1", [id], |row| {
                row.get::<_, String>(0)
            })
            .optional()
            .map_err(|e| format!("reading the previous content key of work {id}: {e}"))?
            .filter(|previous| previous != &key),
        _ => None,
    };
```

where `by_address` is a `bool` set in the `existing` match: `true` in the arm that falls through to `find_by_path`/`known`, `false` in the `find_by_key` arm.

Then, after `tx.commit()` and before `Ok(work_id)`:

```rust
    // **After the commit, never inside it.** A rollback that left the document
    // renamed would strand it under a key the database never adopted — this
    // module's own defect, inverted, and harder to see because the
    // reconciliation would then report everything as fine. A rename that fails
    // leaves the file where it is and `hand::reconcile` reports it.
    if let Some(old_key) = rekeyed_from {
        crate::hand::rename_for_rekey(conn, work_id, &old_key)?;
    }
```

- [ ] **Step 5: Run to verify it passes**

Run: `cargo test -p taiyaku-library`
Expected: PASS, including `a_repacked_volume_takes_its_document_with_it`.

- [ ] **Step 6: Add the collision test**

```rust
/// A file already at the target address is never overwritten: two works cannot
/// legitimately share one, so a collision means something unpredicted, and the
/// correction under the old key is kept for a person to read.
#[test]
fn a_rekey_never_overwrites_a_document_already_at_the_new_address() {
    let dir = crate::tests::tempdir("hand-rekey-collision");
    let root = dir.path.to_string_lossy().to_string();
    let conn = Connection::open_in_memory().expect("in-memory database");
    let work_id = a_scanned_work_rooted_at(&conn, &root);

    let mut doc = crate::librarydoc::of(&conn, work_id).expect("projecting");
    doc.title = Some("Old".to_string());
    crate::librarydoc::apply(&conn, work_id, &doc).expect("applying");
    super::save(&conn, work_id).expect("saving");

    conn.execute("UPDATE work SET content_key = 'new' WHERE id = ?1", [work_id])
        .expect("rekeying");
    let occupied = super::path_for(&conn, work_id)
        .expect("addressing")
        .expect("an address");
    std::fs::create_dir_all(occupied.parent().expect("a directory")).expect("creating");
    std::fs::write(&occupied, br#"{"taiyaku_library":1,"updated":"x","title":"Squatter"}"#)
        .expect("planting a document at the new address");

    let moved = super::rename_for_rekey(&conn, work_id, "abc123").expect("renaming");

    assert!(!moved, "the rename refused");
    assert!(
        std::fs::read_to_string(&occupied)
            .expect("reading")
            .contains("Squatter"),
        "and left what was already there untouched"
    );
    assert!(
        dir.path.join(".taiyaku/abc123.library.json").is_file(),
        "and kept the old document for a person to read"
    );
}
```

Run: `cargo test -p taiyaku-library hand::tests::a_rekey_never_overwrites -- --nocapture`
Expected: PASS.

- [ ] **Step 7: The durability test, with the key moving**

Add to `derive/tests.rs`, reusing Task 5's `title_after_rebuilding`:

```rust
/// **The measured failure, as an assertion.** Task 5's baseline never moves
/// the `content_key`, so it passed before any of this existed. Here the volume
/// gains a page **while a database is watching**, which is what took two of
/// five works' corrections on 2026-08-31, and it is the transition
/// `rename_for_rekey` exists to carry the document across.
#[test]
fn a_correction_survives_a_page_being_added_and_then_losing_the_database() {
    let dir = archived_volumes();
    let v01 = dir.path.join("Series").join("v01").join("Series v01.cbz");

    {
        let library = Library::open_in_memory().expect("open");
        let conn = library.conn();
        let root = scan::add_root(&conn, &dir.path).expect("add");
        scan::scan_root(&conn, &root).expect("scan");
        derive::derive(&conn).expect("deriving");

        let work_id: i64 = conn
            .query_row("SELECT id FROM work ORDER BY id LIMIT 1", [], |row| row.get(0))
            .expect("a work was derived");
        let mut doc = crate::librarydoc::of(&conn, work_id).expect("projecting");
        doc.title = Some("Hand Set".to_string());
        crate::librarydoc::apply(&conn, work_id, &doc).expect("applying");
        crate::hand::save(&conn, work_id).expect("saving");

        // **One page added, and a scan that sees it.** This is the only moment
        // at which both keys exist: `import_with`'s probe holds the old one
        // and has just computed the new one. The document moves between them
        // here or it never moves at all.
        write_archive(&v01, 4);
        scan::scan_root(&conn, &root).expect("rescanning");
        let report = derive::derive(&conn).expect("rederiving");
        assert_eq!(report.rekeyed, 1, "the work followed its files: {report:?}");
        assert_eq!(
            report.orphan_documents.len(),
            0,
            "and its document followed the work: {report:?}"
        );
    }

    // Only now is the database gone, and only the root remains.
    assert_eq!(
        title_after_rebuilding(&dir),
        "Hand Set",
        "the document was carried to the new address while a scan could still \
         see both, so one rescan restores the correction — store spec §4.4"
    );
}
```

**The scan between the edit and the loss is load-bearing, and the test is
written in that order deliberately.** Adding the page *after* the database is
dropped strands the document unrecoverably: the rebuilt database computes the
new `content_key` and looks for `<new>.library.json`, while the file on disk is
still named for the old bytes, and no scan ever held both keys. That case is
not a bug in this design — it is outside what any rename can reach — and it is
recorded as a limitation in the spec's §10 rather than tested here. Writing
this test the other way round would assert something the feature cannot do and
fail for the right reason at the wrong time.

- [ ] **Step 8: Hand-apply one mutation and watch a test fail**

Temporarily change `rename_for_rekey`'s `if !old_path.is_file() || new_path.exists()` to `if !old_path.is_file()` and re-run.
Expected: `a_rekey_never_overwrites_a_document_already_at_the_new_address` FAILS. Revert.

- [ ] **Step 9: Commit**

```bash
git add crates/taiyaku-library/src/hand.rs crates/taiyaku-library/src/import.rs \
        crates/taiyaku-library/src/hand/tests.rs crates/taiyaku-library/src/derive/tests.rs
git commit -m "a rekeyed work takes its hand document with it"
```

---

### Task 8: PR 2's real-material check, documentation and gate

**Files:**
- Modify: `docs/status.md`, `docs/notes.md`

- [ ] **Step 1: Drive it against real archives**

The second mode from `docs/notes.md`. Copy — do not hardlink, since this rewrites an archive — a few `.cbz` files out of `corpus/` into a root under `target/`, scan them, hand-edit through `PATCH /api/library/works/{slug}` with a session cookie, repack one archive, rescan, then rebuild the database from the same root and confirm the correction is still there. Stamp `date -Iseconds` first and confirm `find corpus -newermt "$START"` returns nothing at the end.

This is not optional: on #91 this mode found a merge blocker that ten per-task reviews, a whole-branch review, the full suite and a green gate had all passed.

- [ ] **Step 2: Record the result in `docs/notes.md`**

Append to the 2026-08-31 section, as a new `###` subsection: the events run, the before/after of the rebuild, and the corpus-untouched proof. A figure asserted there was measured, not recalled.

- [ ] **Step 3: Close O81, and decide O82**

O81 closes: the stranding is prevented at its cause and reported where it cannot be. Follow the five-part edit from the ritual — the checkbox, the annotation, the prose, the test count at **both** sites, and the "N are now closed — leaving M" sentence, plus the "What remains" line whose count word wraps across a newline.

O82 does **not** close: the stale case remains. Leave its row `[ ]` with the annotation from Task 6 standing.

- [ ] **Step 4: Run the count sweep and the gate**

```bash
grep -c '^| O[0-9]' docs/status.md          # expect 88
grep -c '^| O[0-9]* | \[x\]' docs/status.md # expect 46
grep -n -A1 '^What remains:' docs/status.md # expect forty-two
env -u TAIYAKU_CORPUS scripts/check-all.sh  # expect the literal line: all green
```

- [ ] **Step 5: Commit and open the PR**

```bash
git add docs/status.md docs/notes.md
git commit -m "status: O81 closes"
git push -u origin hand-rename-on-rekey
```

Open through the gitea MCP, body unwrapped.

---

## What this plan deliberately does not do

- **No automatic resolution of a stale document.** The design's §2 records the three rejected alternatives and what evidence would earn a timestamp column. PR 1's reporting is that evidence-gathering.
- **No reconciliation of `<stem>.library.json` beside a sidecar.** `path_for` answers `None` for a sidecar-imported work by design; its document is `export::work`'s. Design §7.
- **No deletion of an orphan, on any schedule.**
- **Neither PR merges O81 and O82 into one row.** That moves the counts in `docs/status.md` and is a decision about the list, not the code.
