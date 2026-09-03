# C4 PR 1 — rules, roots and the scan trigger: implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship the server half of spec C4's rule editor, roots screen and scan
control — every route those three screens need, with no UI.

**Architecture:** A rule preview runs the *real* `derive` inside a transaction
and rolls it back, on a connection of its own, with a new documents policy that
suppresses `derive`'s writes to `<root>/.taiyaku/`. Root removal reuses that
same rollback machinery to state its counts before acting. A scan is triggered
by a route, runs on its own connection in a `spawn_blocking` task, and is
single-flighted per root by a registry in `ServeState`.

**Tech Stack:** Rust, `rusqlite` (system SQLite, WAL), `axum`, `tokio`,
`serde_json`; tests with `cargo nextest`, gate with `scripts/check-all.sh`.

**Design documents.** The build design is
[`2026-09-02-library-configuration-build-design.md`](../specs/2026-09-02-library-configuration-build-design.md);
the product design it builds is
[`2026-08-14-library-configuration-design.md`](../specs/2026-08-14-library-configuration-design.md).
Read both before Task 1. Section references below (`§2`, `C4 §1`) are to the
build design and the product design respectively.

## Global Constraints

- **The gate before pushing is `env -u TAIYAKU_CORPUS scripts/check-all.sh`.**
  Never run it with `TAIYAKU_CORPUS` set — the script exports it into an
  unfiltered `cargo test --workspace` and a run has been killed at 1h09m.
  Confirm success by quoting its `all green` line; piping it through `tail` or
  appending `; echo` masks the exit status.
- **The corpus job is separate:**
  `TAIYAKU_CORPUS="$PWD/corpus" cargo test --workspace --all-features corpus:: -- --nocapture --test-threads=1`
  (~85 s).
- **Every new route goes in two lists in `routes.rs`** — `guarded_routes()` and
  the grant-posture table beneath it. `every_route_declares_its_grant_posture`
  and `every_route_refuses_an_anonymous_request` both fail on a route in only
  one. A route added to `open_routes()` instead ships unauthenticated.
- **`docs/status.md` states the test count twice** and CI checks it against the
  JUnit report. The two exact strings, verified 2026-09-02: `Six crates, 1784
  tests` (line 121) and `Nineteen of the 1784 are standing checks` (line 179).
  The baseline entering this plan is **1784 Rust tests**. Any task that adds a
  test moves both figures; Task 13 sweeps them, but a mid-plan push needs them
  current.
- **No attribution lines in commit messages.** The remote is squash-only: the
  PR body becomes the single commit message, so local messages are working
  notes.
- **Test names are descriptive sentences in snake_case**, per the house style
  (`folding_a_work_away_orphans_its_hand_document_on_the_same_pass`).
- **Errors are `Result<_, String>` with a message naming the thing that
  failed**, matching every function in `taiyaku-library`.

---

### Task 1: A documents policy on `derive`

`derive` writes to the user's library tree in two places, and a `ROLLBACK`
undoes neither. Everything downstream of this task depends on being able to
derive without touching disk.

**Files:**
- Modify: `crates/taiyaku-library/src/derive.rs` (`derive` at :423, `write_work`
  at :1223, the `reconcile` call at :636, the `rename_for_rekey` call at :1399)
- Test: `crates/taiyaku-library/src/derive/tests.rs`

**Interfaces:**
- Consumes: nothing.
- Produces:
  ```rust
  #[derive(Debug, Clone, Copy, PartialEq, Eq)]
  pub enum Documents {
      /// Reconcile `<root>/.taiyaku/` as a real derivation does.
      Reconcile,
      /// Touch no file under any root. Reads stay on.
      Skip,
  }
  pub fn derive_with(conn: &Connection, documents: Documents) -> Result<DeriveReport, String>;
  pub fn derive(conn: &Connection) -> Result<DeriveReport, String>; // unchanged signature
  ```

- [ ] **Step 1: Write the failing test**

In `crates/taiyaku-library/src/derive/tests.rs`:

```rust
/// A preview derivation must not rename, write or delete anything under
/// `<root>/.taiyaku/`. `hand::rename_for_rekey` is gated on nothing — a rekey
/// stranding a document is the defect it exists to prevent — so without a
/// documents policy a rule preview would rename files in someone's library and
/// the database rollback would not undo it.
#[test]
fn skipping_documents_leaves_the_library_tree_byte_identical() {
    let fixture = crate::tests::library_with_a_kept_document();
    let conn = fixture.conn();
    let before = crate::tests::tree_snapshot(fixture.root_path());

    // Move the work's content_key: repack the archive so a rekey is what the
    // next derivation sees.
    crate::tests::add_a_page(&fixture, "Series - v01.cbz");
    crate::scan::scan_root(&conn, &crate::scan::roots(&conn).unwrap()[0]).unwrap();

    crate::derive::derive_with(&conn, crate::derive::Documents::Skip).unwrap();

    assert_eq!(
        crate::tests::tree_snapshot(fixture.root_path()),
        before,
        "a Skip derivation wrote, renamed or removed a file under the root"
    );
}

/// The default policy is unchanged behaviour: `derive` still reconciles.
#[test]
fn reconciling_documents_is_what_plain_derive_does() {
    let fixture = crate::tests::library_with_a_kept_document();
    let conn = fixture.conn();
    crate::tests::add_a_page(&fixture, "Series - v01.cbz");
    crate::scan::scan_root(&conn, &crate::scan::roots(&conn).unwrap()[0]).unwrap();

    let report = crate::derive::derive(&conn).unwrap();

    assert_eq!(report.rekeyed, 1, "the repack should have moved content_key");
    assert!(
        report.orphan_documents.is_empty(),
        "rename_for_rekey should have carried the document across: {:?}",
        report.orphan_documents
    );
}
```

If `library_with_a_kept_document`, `tree_snapshot` and `add_a_page` do not
exist in `crates/taiyaku-library/src/tests.rs`, write them there first:
`library_with_a_kept_document` builds a temp-dir root with one `.cbz`, scans,
derives, turns `keeps_documents` on, sets a hand title through
`hand::save`, and derives again; `tree_snapshot` returns a
`BTreeMap<PathBuf, Vec<u8>>` of every file under the root read with
`std::fs::read`; `add_a_page` rewrites the named archive with one extra entry.
Check first — `hand/tests.rs` and `derive/tests.rs` already build fixtures of
this shape and one of them may be reusable directly.

- [ ] **Step 2: Run the tests to verify the first fails**

Run: `cargo test -p taiyaku-library skipping_documents_leaves_the_library_tree_byte_identical`
Expected: FAIL to compile — `Documents` and `derive_with` do not exist.

- [ ] **Step 3: Add the policy**

In `derive.rs`, above `derive`:

```rust
/// Whether a derivation reconciles `<root>/.taiyaku/` or leaves the tree
/// alone.
///
/// **Two call sites, and both of them write.** `derive` calls
/// [`crate::hand::reconcile`] per root, which writes a missing document back
/// where the root keeps them, and `write_work` calls
/// [`crate::hand::rename_for_rekey`], which renames a document when a work's
/// `content_key` moves and is gated on nothing at all — a rekey stranding a
/// document is the defect it exists to prevent (#101).
///
/// **[`Documents::Skip`] suppresses writes, never reads.** `hand::load` stays
/// on: a hand-set title is part of the structure a preview claims to predict,
/// and loading it is what makes the preview's answer the same answer a save
/// would produce. Skipping the read instead would make the preview lie about
/// every work anyone has ever corrected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Documents {
    Reconcile,
    Skip,
}

/// [`derive_with`] under [`Documents::Reconcile`] — what a real derivation is.
///
/// Kept as its own function rather than a defaulted parameter because 133 call
/// sites read it, all but a handful of them tests, and none of them has an
/// opinion about documents.
///
/// # Errors
///
/// As [`derive_with`].
pub fn derive(conn: &Connection) -> Result<DeriveReport, String> {
    derive_with(conn, Documents::Reconcile)
}
```

Rename the existing body to
`pub fn derive_with(conn: &Connection, documents: Documents) -> Result<DeriveReport, String>`,
keeping its doc comment and adding a line about the parameter. Then:

- at the `write_work` call (`:509`), pass `documents` through;
- change `write_work`'s signature to take `documents: Documents` as its last
  parameter before `report`;
- guard `:1399`:

  ```rust
  if documents == Documents::Reconcile {
      crate::hand::rename_for_rekey(conn, work_id, &old_key)?;
  }
  ```

- guard the reconcile loop at `:636`:

  ```rust
  // **Skipped whole, not per-root.** `reconcile` writes (gap repair), renames
  // and reports; a preview wants none of the three, and the report fields it
  // populates would be artefacts of a derivation nobody asked to keep.
  if documents == Documents::Reconcile {
      for root in &roots {
          let found = crate::hand::reconcile(conn, root)?;
          report.repaired_documents += found.repaired;
          report.unwritable_documents.extend(found.unwritable);
          report.orphan_documents.extend(found.orphans);
          report.declined_documents.extend(found.declined);
          report.stale_documents.extend(found.stale);
      }
  }
  ```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p taiyaku-library derive::`
Expected: PASS, including the two new tests and every existing `derive` test
unchanged.

- [ ] **Step 5: Hand-apply the mutation and watch it fail**

Change the guard at `:1399` back to an unconditional call, re-run
`skipping_documents_leaves_the_library_tree_byte_identical`, and confirm it goes
red. Restore the guard. A fixture where right and wrong render the same is the
failure this step exists to prevent.

- [ ] **Step 6: Commit**

```bash
git add crates/taiyaku-library/src/derive.rs crates/taiyaku-library/src/derive/tests.rs crates/taiyaku-library/src/tests.rs
git commit -m "derive takes a documents policy, so a preview can leave the tree alone"
```

---

### Task 2: One connection or two, decided in one place

The server holds a single `Mutex<Connection>`; a preview or a scan on that
handle stalls every request. A second connection needs a path, and an in-memory
library — which every serve test fixture uses — has none.

**Files:**
- Modify: `crates/taiyaku-library/src/lib.rs` (`Library` at :248, `open` at
  :260, `open_in_memory` at :271, `conn` at :360)
- Test: `crates/taiyaku-library/src/tests.rs`

**Interfaces:**
- Consumes: nothing.
- Produces:
  ```rust
  pub enum Working<'a> {
      Owned(Connection),
      Shared(std::sync::MutexGuard<'a, Connection>),
  }
  impl std::ops::Deref for Working<'_> { type Target = Connection; }
  impl Library {
      /// A connection to work on for a long job.
      pub fn working(&self) -> Result<Working<'_>, String>;
      pub fn path(&self) -> Option<&Path>;
  }
  ```

- [ ] **Step 1: Write the failing test**

In `crates/taiyaku-library/src/tests.rs`:

```rust
/// A file-backed library hands out a *second* connection, so a long job does
/// not hold the mutex every request needs.
#[test]
fn a_file_backed_library_works_on_a_connection_of_its_own() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("library.db");
    let library = crate::Library::open(&path).expect("opens");

    let working = library.working().expect("a working connection");
    // The shared connection is free while the working one is held: taking it
    // here would deadlock if `working` were the same lock.
    let shared = library.conn();
    shared
        .execute("CREATE TABLE probe (id INTEGER)", [])
        .expect("the shared connection is usable");
    drop(shared);

    working
        .execute("INSERT INTO probe (id) VALUES (1)", [])
        .expect("the working connection writes");
}

/// An in-memory library has no file to open twice, so the working connection
/// *is* the shared one. Serve's fixtures are all in-memory, and this is what
/// lets them reach the preview and scan paths at all.
#[test]
fn an_in_memory_library_works_on_the_connection_it_already_has() {
    let library = crate::Library::open_in_memory().expect("opens");
    assert!(library.path().is_none());
    let working = library.working().expect("a working connection");
    working
        .execute("CREATE TABLE probe (id INTEGER)", [])
        .expect("the working connection writes");
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p taiyaku-library a_file_backed_library_works_on_a_connection_of_its_own`
Expected: FAIL to compile — no `working`, no `path`.

- [ ] **Step 3: Implement**

```rust
pub struct Library {
    conn: Mutex<Connection>,
    /// Where this library was opened from, or `None` for `:memory:`.
    ///
    /// Held so a long job can open a *second* connection to the same file
    /// rather than holding the mutex every request needs. An in-memory
    /// database has no such file — a second connection to `:memory:` is a
    /// different, empty database — which is why [`Working`] has two shapes
    /// rather than this being an `Option<Connection>`.
    path: Option<PathBuf>,
}

/// A connection to run a long job on: its own where there is a file to open,
/// the shared one where there is not.
///
/// **One accessor, two shapes, so no caller has two code paths.** The
/// alternative was returning `Option<Connection>` and letting each caller fall
/// back, which is two behaviours written out twice at every call site — the
/// arrangement this repo keeps finding drifts.
pub enum Working<'a> {
    Owned(Connection),
    Shared(MutexGuard<'a, Connection>),
}

impl std::ops::Deref for Working<'_> {
    type Target = Connection;
    fn deref(&self) -> &Connection {
        match self {
            Self::Owned(conn) => conn,
            Self::Shared(held) => held,
        }
    }
}

impl Library {
    /// Where this library lives, or `None` if it is in memory.
    #[must_use]
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// A connection for a job long enough that holding the shared one would
    /// stall the server.
    ///
    /// Under WAL a second connection's write transaction leaves readers on the
    /// first reading the pre-transaction snapshot, and `busy_timeout`
    /// (set in [`Self::from_connection`]) governs a collision with a real
    /// writer.
    ///
    /// # Errors
    ///
    /// If the file exists but a second connection cannot be opened against it
    /// — the message names the path.
    pub fn working(&self) -> Result<Working<'_>, String> {
        match &self.path {
            Some(path) => {
                let conn = Connection::open(path)
                    .map_err(|e| format!("opening a second connection to {}: {e}", path.display()))?;
                Self::configure(&conn)?;
                Ok(Working::Owned(conn))
            }
            None => Ok(Working::Shared(self.conn())),
        }
    }
}
```

`Self::configure` is whatever `from_connection` already does to a fresh
connection short of applying the schema — read `from_connection` and extract the
`busy_timeout` and foreign-keys pragmas into it, calling it from both. Do not
re-apply migrations on the second connection: the file is already migrated, and
`schema::apply` on a live database is not a no-op worth risking here.

Set `path: Some(path.to_path_buf())` in `open` and `path: None` in
`open_in_memory`.

- [ ] **Step 4: Run to verify both pass**

Run: `cargo test -p taiyaku-library library_works_on`
Expected: PASS, both tests.

- [ ] **Step 5: Run the crate's whole suite**

Run: `cargo nextest run -p taiyaku-library`
Expected: PASS — `Library`'s two constructors are used everywhere, so this is
where a missed field initialiser surfaces.

- [ ] **Step 6: Commit**

```bash
git add crates/taiyaku-library/src/lib.rs crates/taiyaku-library/src/tests.rs
git commit -m "a library hands out a working connection, its own where there is a file"
```

---

### Task 3: The preview — real derive, rolled back, diffed

> **Shipped 2026-09-02/03, and this section is kept as written rather than
> rewritten. Three things below are not what landed — read this before
> quoting any of it in a later task:**
>
> - **`Impact::proposals_resolved` does not exist.** A derivation cannot
>   resolve a proposal — it only raises and reopens — so the field could
>   never be anything but zero. It shipped as **`proposals_reopened`** and
>   **`proposals_raised`**, the owner's decision 2026-09-02. Task 8's
>   response body carries those two names.
> - **`rules::replace_all` is `pub(crate)`, not `pub`**, and opens its own
>   `Nested` scope rather than requiring the caller to hold a transaction.
> - **The mechanism gained a `Nested` transaction scope** (`lib.rs`), because
>   `import::scanned` and `fold::absorb` each open a transaction and SQLite
>   refuses that inside an outer one. Step 4's sketch below, which wraps
>   `derive` in a bare `unchecked_transaction`, does not work as written.
>   The build design's §2 carries the full correction. The snapshot also
>   moved *inside* the transaction, so `before` and `after` cannot describe
>   two states that never coexisted.
> - **`replace_all` does NOT ignore `Rule::id`, and Step 3's sketch below —
>   `DELETE FROM rule` then reinsert everything — is the shape that had to be
>   abandoned.** It minted fresh ids, so every `proposal.created_rule_id`
>   dangled to NULL and `reopen_orphaned_accepts` reopened *every* accepted
>   proposal on *every* save. Keeping the id through a delete-and-reinsert
>   does not fix it either: `ON DELETE SET NULL` fires the moment the row goes
>   and never re-links when a same-id row reappears. What shipped is a
>   **differential update** — delete only the ids the proposed list drops,
>   `UPDATE` kept rows in place by id so the foreign key is never disturbed,
>   insert only rules whose `id` is `0`, and refuse an id that names no
>   existing row or one that appears twice. **The save path this plan builds
>   later must send real ids for rules that already exist**, or it will
>   reopen every proposal the person has answered.
> - **Order:** position in the slice is inert. `rule.ordinal` decides, and
>   `read` orders by `(ordinal, id)`, so a tie on `ordinal` now sorts kept
>   rows (low ids) before newly inserted ones (high minted ids).

**Files:**
- Create: `crates/taiyaku-library/src/preview.rs`
- Create: `crates/taiyaku-library/src/preview/tests.rs`
- Modify: `crates/taiyaku-library/src/lib.rs` (add `pub mod preview;`)
- Modify: `crates/taiyaku-library/src/rules.rs` (extract `replace_all`)

**Interfaces:**
- Consumes: `derive::derive_with`, `derive::Documents` (Task 1).
- Produces:
  ```rust
  pub struct Placement { pub series: String, pub work: String, pub edition: String }
  pub type Grouping = std::collections::BTreeMap<String, Placement>; // keyed by member.path
  pub struct Impact {
      pub series_before: usize,
      pub series_after: usize,
      pub series_merged: usize,
      pub works_rehomed: usize,
      pub works_orphaned: usize,
      pub proposals_resolved: usize,
  }
  pub fn grouping(conn: &Connection) -> Result<Grouping, String>;
  pub fn preview(conn: &Connection, proposed: &[rules::Rule]) -> Result<Impact, String>;
  pub fn rules::replace_all(conn: &Connection, rules: &[rules::Rule]) -> Result<(), String>;
  ```

- [ ] **Step 1: Write the failing tests**

In `crates/taiyaku-library/src/preview/tests.rs`:

```rust
use crate::preview::{preview, grouping};

/// The promise the whole screen rests on: what the preview says will happen is
/// what happens. Same fixture, same proposed list, one previewed and one saved.
#[test]
fn a_preview_and_a_real_save_produce_the_same_structure() {
    let fixture = crate::tests::library_with_two_roots();
    let conn = fixture.conn();
    let mut proposed = crate::rules::read(&conn).unwrap().0;
    proposed.push(crate::tests::a_rule_setting_series("Alpha"));

    let predicted = preview(&conn, &proposed).unwrap();
    let after_preview = grouping(&conn).unwrap();

    crate::rules::replace_all(&conn, &proposed).unwrap();
    crate::derive::derive(&conn).unwrap();
    let saved = grouping(&conn).unwrap();

    assert_ne!(after_preview, saved, "the fixture must actually change, or this proves nothing");
    assert_eq!(
        predicted.works_rehomed,
        saved.values().filter(|p| p.series == "Alpha").count(),
        "the predicted count is not what the save produced"
    );
}

/// A preview writes nothing: the rule table and the grouping are as they were.
#[test]
fn a_preview_rolls_back_every_row_it_touched() {
    let fixture = crate::tests::library_with_two_roots();
    let conn = fixture.conn();
    let before_rules = crate::rules::read(&conn).unwrap().0;
    let before_grouping = grouping(&conn).unwrap();

    let mut proposed = before_rules.clone();
    proposed.push(crate::tests::a_rule_setting_series("Alpha"));
    preview(&conn, &proposed).unwrap();

    assert_eq!(crate::rules::read(&conn).unwrap().0, before_rules);
    assert_eq!(grouping(&conn).unwrap(), before_grouping);
}

/// A preview leaves the library tree alone — Task 1's policy, asserted through
/// the caller that needs it rather than only at its own level.
#[test]
fn a_preview_leaves_the_library_tree_byte_identical() {
    let fixture = crate::tests::library_with_a_kept_document();
    let conn = fixture.conn();
    let before = crate::tests::tree_snapshot(fixture.root_path());

    let mut proposed = crate::rules::read(&conn).unwrap().0;
    proposed.push(crate::tests::a_rule_setting_series("Alpha"));
    preview(&conn, &proposed).unwrap();

    assert_eq!(crate::tests::tree_snapshot(fixture.root_path()), before);
}
```

Add `a_rule_setting_series(series: &str) -> rules::Rule` and
`library_with_two_roots()` to `crates/taiyaku-library/src/tests.rs` if they do
not exist. The rule is `RuleKind::Regex`, `match_on: MatchOn::Path`,
`pattern: ".*"`, `set_series: Some(series.into())`, `ordinal` one past the last,
`enabled: true`, `id: 0`.

**Make the fixture's ids not collide.** `library_with_two_roots` must add its
roots in an order that gives the works and the roots *different* id sequences —
a fixture where root id 1 and work id 1 coincide has passed against a
deliberately broken build before (`docs/status.md`, spec C1's grant tests).

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p taiyaku-library preview::`
Expected: FAIL to compile — no `preview` module.

- [ ] **Step 3: Extract `rules::replace_all`**

In `rules.rs`, lift the body of `import_json`'s loop into:

```rust
/// Replace the whole rule table with `rules`, in the order given.
///
/// **No transaction of its own**: both callers already hold one —
/// `import_json`'s, and `preview`'s rollback — and `unchecked_transaction`
/// issues a plain `BEGIN`, which SQLite refuses inside an open transaction.
/// The same constraint `derive` documents at its own head.
///
/// `Rule::id` is ignored: this writes a list, and SQLite mints the ids.
///
/// # Errors
///
/// If SQLite refuses the delete or any insert.
pub fn replace_all(conn: &Connection, rules: &[Rule]) -> Result<(), String> {
    conn.execute("DELETE FROM rule", [])
        .map_err(|e| format!("clearing the existing rule set: {e}"))?;
    for rule in rules {
        conn.execute(
            "INSERT INTO rule (ordinal, enabled, root_id, kind, pattern, match_on, \
             set_edition_key, set_kind, set_language, set_series, note) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            rusqlite::params![
                rule.ordinal,
                rule.enabled,
                rule.root_id,
                rule.kind.to_string(),
                rule.pattern,
                rule.match_on.to_string(),
                rule.set_edition_key,
                rule.set_kind,
                rule.set_language,
                rule.set_series,
                rule.note,
            ],
        )
        .map_err(|e| format!("writing rule: {e}"))?;
    }
    Ok(())
}
```

`RuleKind` and `MatchOn` parse `FromStr` today and may not have a `Display` —
check, and add one rather than writing the string forms out a second time here.
The column order above is `import_json`'s own, unchanged: eleven columns,
eleven parameters. **A mutation that drops one of these bindings is caught
mechanically and proves nothing** — a SQL parameter-count mismatch fails every
test that touches the table — so do not count this loop as covered on the
strength of a surviving mutant.

and have `import_json` map its `RuleJson` rows (resolving `root_path` to a
`root_id` exactly as it does now) into `Vec<Rule>` and call it inside its
existing transaction.

- [ ] **Step 4: Write `preview.rs`**

```rust
//! What a proposed rule list would do, answered by doing it and rolling back.
//!
//! **The real `derive`, not a second evaluator.** Spec C4 §7's first test is
//! that a preview and a real save produce the same structure, and the whole
//! rule editor is a promise that the preview is true. Running the same
//! function makes that true by construction; a preview pass of its own makes
//! it true only for as long as a test keeps two implementations in step.
//!
//! **It writes nothing.** The transaction rolls back, and the derivation runs
//! under [`crate::derive::Documents::Skip`] so nothing under `<root>/.taiyaku/`
//! is written, renamed or removed — a rollback would not undo those.

/// Where one member sits: which series, work and edition hold it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Placement {
    pub series: String,
    pub work: String,
    pub edition: String,
}

/// Every member's placement, keyed by `member.path`.
///
/// **`member.path` is the identity that survives a derivation.** Row ids do
/// not: a derivation rewrites `work`, `edition` and `member` wholesale, so a
/// before-and-after diff keyed on ids would report every row as new.
pub type Grouping = std::collections::BTreeMap<String, Placement>;

/// # Errors
///
/// If SQLite refuses the query.
pub fn grouping(conn: &Connection) -> Result<Grouping, String> {
    let mut stmt = conn
        .prepare(
            // **`LEFT JOIN`, and the id as the fallback.** `series_title`
            // holds several names per series and `is_primary` picks the
            // display one — but nothing guarantees a series has a primary
            // title, and an inner join would silently drop every series that
            // does not, making a diff read as an orphaning that never
            // happened.
            "SELECT m.path, s.id, st.title, w.slug, e.key
               FROM member m
               JOIN edition e ON e.id = m.edition_id
               JOIN work    w ON w.id = e.work_id
               JOIN series  s ON s.id = w.series_id
               LEFT JOIN series_title st
                      ON st.series_id = s.id AND st.is_primary = 1
              ORDER BY m.path",
        )
        .map_err(|e| format!("preparing the grouping query: {e}"))?;
    let rows = stmt
        .query_map([], |r| {
            let series_id: i64 = r.get(1)?;
            Ok((
                r.get::<_, String>(0)?,
                Placement {
                    series: r
                        .get::<_, Option<String>>(2)?
                        .unwrap_or_else(|| format!("series {series_id}")),
                    work: r.get(3)?,
                    edition: r.get(4)?,
                },
            ))
        })
        .map_err(|e| format!("reading the grouping: {e}"))?;
    let mut out = Grouping::new();
    for row in rows {
        let (path, placement) = row.map_err(|e| format!("reading a placement: {e}"))?;
        out.insert(path, placement);
    }
    Ok(out)
}

/// What changes between the library as it stands and the library the proposed
/// rules would derive.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Impact {
    pub series_before: usize,
    pub series_after: usize,
    /// `series_before - series_after`, floored at zero. Merging is the only
    /// way rules reduce the count, so the difference names it directly.
    pub series_merged: usize,
    /// Members whose series changed.
    pub works_rehomed: usize,
    /// Members present before and absent after — a rule that swallowed them.
    pub works_orphaned: usize,
    pub proposals_resolved: usize,
}

pub fn preview(conn: &Connection, proposed: &[crate::rules::Rule]) -> Result<Impact, String> {
    let before = grouping(conn)?;
    let open_before = crate::proposals::count_open(conn)?;

    let tx = conn
        .unchecked_transaction()
        .map_err(|e| format!("starting the preview transaction: {e}"))?;
    crate::rules::replace_all(&tx, proposed)?;
    crate::derive::derive_with(&tx, crate::derive::Documents::Skip)?;
    let after = grouping(&tx)?;
    let open_after = crate::proposals::count_open(&tx)?;
    // Explicit, not `drop(tx)`: a rollback that happens because nobody
    // committed is a rollback nobody can find when reading this later.
    tx.rollback()
        .map_err(|e| format!("rolling the preview back: {e}"))?;

    let series_before: std::collections::BTreeSet<&str> =
        before.values().map(|p| p.series.as_str()).collect();
    let series_after: std::collections::BTreeSet<&str> =
        after.values().map(|p| p.series.as_str()).collect();

    Ok(Impact {
        series_before: series_before.len(),
        series_after: series_after.len(),
        series_merged: series_before.len().saturating_sub(series_after.len()),
        works_rehomed: after
            .iter()
            .filter(|(path, now)| before.get(*path).is_some_and(|was| was.series != now.series))
            .count(),
        works_orphaned: before.keys().filter(|path| !after.contains_key(*path)).count(),
        proposals_resolved: open_before.saturating_sub(open_after),
    })
}
```

Fill in `grouping`'s query and the diff. If `proposals::count_open` does not
exist, add it beside `proposals::list` — one `SELECT COUNT(*) FROM proposal
WHERE state = 'open'`.

Add `pub mod preview;` to `lib.rs` with a one-line doc pointing at the build
design's §2.

- [ ] **Step 5: Run to verify they pass**

Run: `cargo nextest run -p taiyaku-library preview`
Expected: PASS, all three.

- [ ] **Step 6: Hand-apply a mutation**

Change `Documents::Skip` to `Documents::Reconcile` in `preview` and re-run
`a_preview_leaves_the_library_tree_byte_identical`. It must go red. Restore.

- [ ] **Step 7: Commit**

```bash
git add crates/taiyaku-library/src/preview.rs crates/taiyaku-library/src/preview/tests.rs crates/taiyaku-library/src/lib.rs crates/taiyaku-library/src/rules.rs crates/taiyaku-library/src/tests.rs
git commit -m "a rule preview runs the real derivation and rolls it back"
```

---

### Task 4: Measure it, because C4 §11 says nobody has

C4 §11's "34 ms" is marked as an illustration. Everything downstream of
"debounced on every keystroke" waits on a real number.

**Files:**
- Modify: `docs/notes.md` (append a dated measurement section)
- Modify: `crates/taiyaku-cli/src/library.rs` (a hidden `rules --preview-cost`
  path, or a `#[ignore]`d test — pick whichever the crate already prefers for
  one-off measurements; check how L1 geometry's figures were taken)

- [ ] **Step 1: Take the measurement**

Against the real library (not a fixture), time ten consecutive
`preview::preview(&conn, &current_rules)` calls with the rule list unchanged,
and ten with one rule appended. Record: library size (roots, series, works,
members), median and worst wall time for each, and the machine.

Run it once with the library on this workstation's NFS mount and once with a
local copy if one is cheap to make — the derivation reads the database, not the
tree, so a difference here would be a surprise worth recording either way.

- [ ] **Step 2: Write it into `docs/notes.md`**

A dated section naming the command, the figures and the conclusion in one
sentence: whether a debounced per-keystroke preview is affordable, or whether
the screen previews on demand. `docs/status.md`'s preamble promises every claim
there traces to a measurement here, and C4 §11 is the claim this discharges.

- [ ] **Step 3: Commit**

```bash
git add docs/notes.md crates/taiyaku-cli/src/library.rs
git commit -m "measure what a rule preview costs on the real library"
```

---

### Task 5: Matched files, near-misses and provenance

> **Corrected 2026-09-03, before implementation, by Task 4's measurement.**
> The standalone `rows_for` below is **not** what to build. A `Row` carries an
> `after` placement, which exists only inside the rolled-back derivation, so a
> separate function would derive a second time — and a derivation costs
> **281 ms** on the real library (`docs/notes.md`, "What a rule preview costs
> on a real library"). The route sends the proposed list and the selected rule
> in one request, so two passes would cost 560 ms for data the first pass
> already holds. Build it as **one transaction producing both** the `Impact`
> and the rows, with `selected: None` doing no row-building work at all.

C4 §1's second and third preview tabs. The near-miss definition is a choice the
spec leaves open; this task makes it.

**Files:**
- Modify: `crates/taiyaku-library/src/preview.rs`
- Modify: `crates/taiyaku-library/src/preview/tests.rs`
- Modify: `crates/taiyaku-library/src/rules.rs` (`ROOT_FALLBACK` stays private;
  see step 3)

**Interfaces:**
- Consumes: `preview::preview` (Task 3), `rules::Engine`, `rules::Resolved`.
- Produces:
  ```rust
  pub struct Row {
      pub rel_path: String,
      pub root_id: i64,
      pub before: Option<Placement>,
      pub after: Option<Placement>,
      pub matched: bool,
      pub decided_by: std::collections::BTreeMap<rules::Field, Decided>,
  }
  pub enum Decided { Rule(i64), RootDefault }
  pub fn rows_for(conn: &Connection, proposed: &[rules::Rule], rule_id: i64, limit: usize)
      -> Result<Vec<Row>, String>;
  ```

- [ ] **Step 1: Write the failing test**

```rust
/// C4 §1: "a regex's *failure* is the thing you cannot see by reading it". A
/// near-miss here is an entry this rule did **not** match whose field nothing
/// else decided either — `Yu Yu Hakusho - Colored (Colored Council)` against a
/// colour-edition regex, with `edition` left on the root's default.
#[test]
fn matched_files_include_the_entries_the_rule_missed_and_nothing_else_caught() {
    let fixture = crate::tests::library_with_two_colour_editions();
    let conn = fixture.conn();
    let mut proposed = crate::rules::read(&conn).unwrap().0;
    let colour = crate::tests::a_colour_edition_rule(); // matches "- Full Color"
    proposed.push(colour.clone());

    let rows = crate::preview::rows_for(&conn, &proposed, colour.id, 200).unwrap();

    let missed: Vec<&str> = rows
        .iter()
        .filter(|r| !r.matched)
        .map(|r| r.rel_path.as_str())
        .collect();
    assert!(
        missed.iter().any(|p| p.contains("Colored (Colored Council)")),
        "the near-miss is the row worth showing, and it is not here: {missed:?}"
    );
}

/// Provenance says which rule set each field, and says so for the root's own
/// column too — the sentinel is not a rule id leaking onto the wire.
#[test]
fn provenance_names_the_root_default_rather_than_a_rule_id_of_zero() {
    let fixture = crate::tests::library_with_two_roots();
    let conn = fixture.conn();
    let proposed = crate::rules::read(&conn).unwrap().0;

    let rows = crate::preview::rows_for(&conn, &proposed, proposed[0].id, 200).unwrap();
    let decided = &rows[0].decided_by;

    // The fixture's root carries `edition_key` and no rule sets it, so this
    // field is decided by the root's own column and by nothing else — the one
    // case where `RootDefault` is the only correct answer. Asserting
    // "`RootDefault` **or** any rule" would pass whatever the mapping did.
    assert_eq!(
        decided.get(&crate::rules::Field::EditionKey),
        Some(&crate::preview::Decided::RootDefault),
        "the root's own column decided this and the wire format must say so"
    );
    assert!(
        !decided.values().any(|d| matches!(d, crate::preview::Decided::Rule(0))),
        "0 is the ROOT_FALLBACK sentinel and must never reach a caller as a rule id"
    );
}
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p taiyaku-library matched_files_include`
Expected: FAIL to compile — no `rows_for`.

- [ ] **Step 3: Implement**

`rows_for` compiles an `Engine` from `proposed`, walks `scan_entry` for the
rule's scope (`root_id IS NULL` means every root), and for each entry records
whether the named rule appears anywhere in `Resolved::provenance`. A row is
kept when either:

- **matched** — the named rule decided at least one field; or
- **a near-miss** — it did not, *and* every field the named rule sets is
  either undecided or decided by the root default.

Cap at `limit`, matches first, `rel_path` order within each. State the cap in
the response so a truncated table cannot read as a complete one — the same
discipline `inventory` holds.

Map the provenance sentinel at this boundary:

```rust
/// **`0` never crosses this boundary.** `rules::ROOT_FALLBACK` is a private
/// implementation detail of a `BTreeMap<Field, i64>`; a wire format that
/// inherited it would have every client special-casing a magic id.
fn decided(rule_id: i64) -> Decided {
    if rule_id == 0 { Decided::RootDefault } else { Decided::Rule(rule_id) }
}
```

Rather than making `ROOT_FALLBACK` public, add
`pub fn rules::is_root_fallback(id: i64) -> bool` beside it so the constant
stays private and this module asks a question instead of reading a number.

- [ ] **Step 4: Run to verify they pass**

Run: `cargo nextest run -p taiyaku-library preview`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/taiyaku-library/src/preview.rs crates/taiyaku-library/src/preview/tests.rs crates/taiyaku-library/src/rules.rs crates/taiyaku-library/src/tests.rs
git commit -m "the preview names the files a rule matched, the ones it missed, and what decided each field"
```

---

### Task 6: Conflict detection

**Files:**
- Modify: `crates/taiyaku-library/src/preview.rs`
- Modify: `crates/taiyaku-library/src/preview/tests.rs`

**Interfaces:**
- Consumes: `rules::Engine`.
- Produces:
  ```rust
  pub struct Conflict {
      pub field: rules::Field,
      pub rel_path: String,
      pub first: i64,
      pub wins: i64,
      pub examples: usize,
  }
  pub fn conflicts(conn: &Connection, proposed: &[rules::Rule]) -> Result<Vec<Conflict>, String>;
  ```

- [ ] **Step 1: Write the failing test**

```rust
/// C4 §1's worked example, and the reason the feature exists: rule 4 sets
/// `edition = raw` for everything under `Manga (RAW)/`, a later colour rule
/// sets `edition = en-color` for anything ending "Full Color", both match
/// `Manga (RAW)/Go-Toubun no Hanayome - Full Color`, the later wins, and a
/// Japanese colour edition is labelled English. Correct behaviour producing a
/// wrong answer.
#[test]
fn two_rules_setting_edition_on_one_file_are_reported_with_both_ids() {
    let fixture = crate::tests::library_with_the_seeded_defaults();
    let conn = fixture.conn();
    let proposed = crate::rules::read(&conn).unwrap().0;

    let found = crate::preview::conflicts(&conn, &proposed).unwrap();

    let one = found
        .iter()
        .find(|c| c.rel_path.contains("Go-Toubun no Hanayome - Full Color"))
        .expect("the seeded defaults' own conflict is not reported");
    assert_eq!(one.field, crate::rules::Field::EditionKey);
    assert_ne!(one.first, one.wins, "a conflict names both rules, not one twice");
}

/// A field one rule alone decides is not a conflict, however many files it
/// touches.
#[test]
fn one_rule_setting_a_field_across_the_library_is_not_a_conflict() {
    let fixture = crate::tests::library_with_two_roots();
    let conn = fixture.conn();
    let mut proposed = crate::rules::read(&conn).unwrap().0;
    proposed.push(crate::tests::a_rule_setting_series("Alpha"));

    let found = crate::preview::conflicts(&conn, &proposed).unwrap();
    assert!(found.iter().all(|c| c.field != crate::rules::Field::Series));
}
```

If `library_with_the_seeded_defaults` does not exist, build it from the seeded
rule set migration 3 installs plus a `scan_entry` row at
`Manga (RAW)/Go-Toubun no Hanayome - Full Color`.

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p taiyaku-library two_rules_setting_edition`
Expected: FAIL to compile — no `conflicts`.

- [ ] **Step 3: Implement**

Resolve every `scan_entry` through the proposed engine while recording, per
field, *every* rule that set it rather than only the last. Report one `Conflict`
per (field, first losing rule, winning rule) with a representative path and a
count of how many entries share it — not one row per file, which on a real
library would be thousands of rows saying one thing.

C4 §11 flags the cost as unmeasured: quadratic in rules, linear in files. Note
in the doc comment that the seeded set is seven rules and nobody has tried
seventy.

- [ ] **Step 4: Run to verify they pass**

Run: `cargo nextest run -p taiyaku-library preview`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/taiyaku-library/src/preview.rs crates/taiyaku-library/src/preview/tests.rs crates/taiyaku-library/src/tests.rs
git commit -m "two enabled rules setting one field on one file are named, with both ids"
```

---

### Task 7: Rules store CRUD

`rules.rs` writes the whole list at a time, which is what the CLI needed. The
editor needs rows.

**Files:**
- Modify: `crates/taiyaku-library/src/rules.rs`
- Modify: `crates/taiyaku-library/src/rules/tests.rs`

**Interfaces:**
- Produces:
  ```rust
  pub struct NewRule {
      pub ordinal: Option<i64>,        // None appends past the current maximum
      pub enabled: bool,
      pub root_id: Option<i64>,        // None means every root
      pub kind: rules::RuleKind,
      pub pattern: String,
      pub match_on: rules::MatchOn,
      pub set_edition_key: Option<String>,
      pub set_kind: Option<String>,
      pub set_language: Option<String>,
      pub set_series: Option<String>,
      pub note: Option<String>,
  }
  /// `Default`-derived, so a caller names only what it changes. The four
  /// `set_*` columns are `Option<Option<String>>`: the outer `None` is "leave
  /// it", the inner is "clear it" — a distinction a single `Option` cannot
  /// carry, and clearing one is how a rule stops setting a field.
  #[derive(Default)]
  pub struct RulePatch {
      pub ordinal: Option<i64>,
      pub enabled: Option<bool>,
      pub root_id: Option<Option<i64>>,
      pub kind: Option<rules::RuleKind>,
      pub pattern: Option<String>,
      pub match_on: Option<rules::MatchOn>,
      pub set_edition_key: Option<Option<String>>,
      pub set_kind: Option<Option<String>>,
      pub set_language: Option<Option<String>>,
      pub set_series: Option<Option<String>>,
      pub note: Option<Option<String>>,
  }
  pub fn create(conn: &Connection, new: &NewRule) -> Result<i64, String>;
  pub fn update(conn: &Connection, id: i64, patch: &RulePatch) -> Result<bool, String>;
  pub fn delete(conn: &Connection, id: i64) -> Result<Deleted, String>;
  pub struct Deleted { pub existed: bool, pub reopened_proposal: Option<i64> }
  ```

- [ ] **Step 1: Write the failing tests**

```rust
/// C4 §1: a rule written by an accepted proposal is marked as such, and
/// deleting it reopens the proposal it came from — the other end of C3 §3's
/// `proposal.created_rule_id`. Without this the two screens each hold half of
/// a reversible action and neither can complete it.
#[test]
fn deleting_a_proposal_written_rule_reopens_its_proposal() {
    let fixture = crate::tests::library_with_an_accepted_proposal();
    let conn = fixture.conn();
    let accepted = crate::proposals::list(&conn, &crate::proposals::Filter::default())
        .unwrap()
        .into_iter()
        .find(|p| p.state == "accepted")
        .expect("the fixture accepted one");
    let rule_id = accepted.created_rule_id.expect("acceptance wrote a rule");

    let deleted = crate::rules::delete(&conn, rule_id).unwrap();

    assert!(deleted.existed);
    assert_eq!(deleted.reopened_proposal, Some(accepted.id));
    let after = crate::proposals::lookup(&conn, accepted.id).unwrap().unwrap();
    assert_eq!(after.state, "open");
}

/// Reordering is a patch of `ordinal`, and it changes what derivation produces
/// — per-field layering asserted, not assumed.
#[test]
fn swapping_two_rules_ordinals_changes_the_derived_structure() {
    let fixture = crate::tests::library_with_two_competing_rules();
    let conn = fixture.conn();
    let rules = crate::rules::read(&conn).unwrap().0;
    let before = crate::preview::grouping(&conn).unwrap();

    crate::rules::update(&conn, rules[0].id, &crate::rules::RulePatch { ordinal: Some(rules[1].ordinal), ..Default::default() }).unwrap();
    crate::rules::update(&conn, rules[1].id, &crate::rules::RulePatch { ordinal: Some(rules[0].ordinal), ..Default::default() }).unwrap();
    crate::derive::derive(&conn).unwrap();

    assert_ne!(crate::preview::grouping(&conn).unwrap(), before);
}

/// A patch of one field leaves the others alone.
#[test]
fn a_patch_changes_only_the_fields_it_names() {
    let fixture = crate::tests::library_with_two_competing_rules();
    let conn = fixture.conn();
    let before = crate::rules::read(&conn).unwrap().0[0].clone();

    crate::rules::update(&conn, before.id, &crate::rules::RulePatch { enabled: Some(false), ..Default::default() }).unwrap();

    let after = crate::rules::read(&conn).unwrap().0.into_iter().find(|r| r.id == before.id).unwrap();
    assert!(!after.enabled);
    assert_eq!(after.pattern, before.pattern);
    assert_eq!(after.set_series, before.set_series);
}

/// Patching a rule that does not exist is `false`, not an error and not a
/// silently created row.
#[test]
fn patching_an_absent_rule_reports_it_rather_than_creating_one() {
    let fixture = crate::tests::library_with_two_competing_rules();
    let conn = fixture.conn();
    let before = crate::rules::read(&conn).unwrap().0.len();

    let changed = crate::rules::update(&conn, 9_999, &crate::rules::RulePatch { enabled: Some(false), ..Default::default() }).unwrap();

    assert!(!changed);
    assert_eq!(crate::rules::read(&conn).unwrap().0.len(), before);
}
```

Check `proposals.rs` first for how acceptance records `created_rule_id` and
whether an `unaccept` path already reopens a proposal — if `proposals::unaccept`
does exactly this, `delete` calls it rather than writing a second version of the
same state change.

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p taiyaku-library rules::tests::deleting_a_proposal_written_rule`
Expected: FAIL to compile.

- [ ] **Step 3: Implement**

`create` inserts and returns the new id, defaulting `ordinal` to one past the
current maximum when the caller gives none. `update` builds its `UPDATE` from
the `Some` fields only and returns whether a row changed. `delete` looks up any
`proposal` whose `created_rule_id` is this rule *before* deleting, deletes, then
reopens that proposal through the existing `proposals` function.

- [ ] **Step 4: Run to verify they pass**

Run: `cargo nextest run -p taiyaku-library rules`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/taiyaku-library/src/rules.rs crates/taiyaku-library/src/rules/tests.rs crates/taiyaku-library/src/tests.rs
git commit -m "rules can be created, patched and deleted one at a time"
```

---

### Task 8: The rules routes

**Files:**
- Create: `crates/taiyaku-serve/src/config_api.rs`
- Create: `crates/taiyaku-serve/src/config_api/tests.rs`
- Modify: `crates/taiyaku-serve/src/lib.rs` (`mod config_api;`)
- Modify: `crates/taiyaku-serve/src/routes.rs` (`guarded_routes()` **and** the
  grant-posture table)

**Interfaces:**
- Consumes: `rules::{read, create, update, delete, import_json}`,
  `preview::{preview, rows_for, conflicts}`, `Library::working` (Task 2).
- Produces: six routes; `config_api::admin_or_forbidden` reused by Tasks 11–12.

- [ ] **Step 1: Write the failing tests**

In `crates/taiyaku-serve/src/config_api/tests.rs`:

```rust
/// Every route here is an administrator's, per C4's opening line.
#[tokio::test]
async fn a_reader_cannot_read_the_rule_list() {
    let state = crate::test_support::state_with_a_reader();
    let response = crate::test_support::get_as_reader(&state, "/api/library/rules").await;
    assert_eq!(response.status(), axum::http::StatusCode::FORBIDDEN);
}

/// The preview route takes the whole proposed list, per C4 §6 — a preview of
/// one rule in isolation answers a question nobody asked, because layering
/// makes a rule's effect a property of its neighbours.
#[tokio::test]
async fn the_preview_route_answers_over_the_whole_proposed_list() {
    let state = crate::test_support::state_with_a_scanned_library();
    // `selected` names which rule's matched-files table to build. It is
    // separate from `rules` because the impact and the conflicts are
    // properties of the *list*, and only the rows are about one rule — the
    // editor previews the list on every keystroke and the table only for
    // whichever rule is open.
    let body = serde_json::json!({
        "rules": crate::test_support::proposed_rules_adding_one(),
        "selected": crate::test_support::the_appended_rule_id(),
    });

    let response = crate::test_support::post_as_admin(&state, "/api/library/rules/preview", &body).await;

    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let json = crate::test_support::json_body(response).await;
    assert!(json["impact"]["works_rehomed"].as_u64().unwrap() > 0);
    assert!(json["conflicts"].is_array());
    assert!(json["rows"].is_array());
    assert!(json["rows_truncated"].is_boolean(), "a capped table must say it is capped");
}

/// `selected` is optional: the impact and the conflicts stand without it, and
/// omitting it must not cost a row-building pass over every scan entry.
#[tokio::test]
async fn a_preview_with_no_rule_selected_answers_without_rows() {
    let state = crate::test_support::state_with_a_scanned_library();
    let body = serde_json::json!({ "rules": crate::test_support::proposed_rules_adding_one() });

    let response = crate::test_support::post_as_admin(&state, "/api/library/rules/preview", &body).await;

    let json = crate::test_support::json_body(response).await;
    assert!(json["impact"].is_object());
    assert!(json["rows"].as_array().is_none_or(std::vec::Vec::is_empty));
}

/// C4 §6's import takes the JSON `taiyaku library rules` exports, and replaces
/// the whole list — the same round trip the CLI already has, over HTTP.
#[tokio::test]
async fn the_import_route_takes_what_the_export_produced() {
    let state = crate::test_support::state_with_a_scanned_library();
    let exported = crate::test_support::exported_rules_json(&state);

    let response = crate::test_support::post_raw_as_admin(&state, "/api/library/rules/import", &exported).await;

    assert_eq!(response.status(), axum::http::StatusCode::OK);
    assert_eq!(crate::test_support::exported_rules_json(&state), exported);
}

/// It writes nothing, which is what makes it safe to call on every keystroke.
#[tokio::test]
async fn the_preview_route_leaves_the_rule_table_as_it_found_it() {
    let state = crate::test_support::state_with_a_scanned_library();
    let before = crate::test_support::rule_ids(&state);
    let body = serde_json::json!({ "rules": crate::test_support::proposed_rules_adding_one() });

    crate::test_support::post_as_admin(&state, "/api/library/rules/preview", &body).await;

    assert_eq!(crate::test_support::rule_ids(&state), before);
}
```

Add the `test_support` helpers beside the existing ones; `state_with_a_reader`
and `post_as_admin` may already exist under other names — `curation/tests.rs`
does exactly this for the proposals routes, so copy its shape rather than
inventing a second one.

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p taiyaku-serve config_api`
Expected: FAIL to compile — no module.

- [ ] **Step 3: Implement the handlers**

Follow `curation.rs` exactly: `State(state)`, `Extension(principal)`, an
`admin_or_forbidden` opening every handler, and the body in
`tokio::task::spawn_blocking`. The preview handler takes its connection from
`state.library.working()?` rather than `state.library.conn()` — a derivation on
the shared handle stalls every request behind it.

Six routes, in `guarded_routes()`:

```rust
("/api/library/rules", get(crate::config_api::list_rules).post(crate::config_api::create_rule)),
("/api/library/rules/{id}", axum::routing::patch(crate::config_api::patch_rule).delete(crate::config_api::delete_rule)),
("/api/library/rules/preview", axum::routing::post(crate::config_api::preview_rules)),
("/api/library/rules/import", axum::routing::post(crate::config_api::import_rules)),
```

- [ ] **Step 4: Declare each route's grant posture**

In the table beneath `guarded_routes()`, add an entry per path.
Each states its **own** reason — the rule this table holds callers to:

```rust
(
    "/api/library/rules",
    GrantIndependent {
        reason: "admin-only and about how the library is organised, not about \
                  what any one account may see: a rule is a property of the \
                  scan, and an administrator's roots are not filtered",
    },
),
```

…and likewise for the other three, each saying what *that* route does.

- [ ] **Step 5: Run to verify they pass**

Run: `cargo nextest run -p taiyaku-serve`
Expected: PASS — including `every_route_declares_its_grant_posture` and
`every_route_refuses_an_anonymous_request`, which fail on a route present in
only one list.

- [ ] **Step 6: Commit**

```bash
git add crates/taiyaku-serve/src/config_api.rs crates/taiyaku-serve/src/config_api/tests.rs crates/taiyaku-serve/src/lib.rs crates/taiyaku-serve/src/routes.rs crates/taiyaku-serve/src/test_support.rs
git commit -m "the rule list, its rows and its preview are served"
```

---

### Task 9: Roots — update, and removal that states its counts

**Files:**
- Modify: `crates/taiyaku-library/src/scan.rs`
- Modify: `crates/taiyaku-library/src/scan/tests.rs`

**Interfaces:**
- Consumes: `preview::grouping` (Task 3), `derive::derive`.
- Produces:
  ```rust
  /// `Default`-derived. The five nullable columns are `Option<Option<_>>` for
  /// `RulePatch`'s reason: clearing `edition_key` back to "no default" is a
  /// different request from not mentioning it.
  #[derive(Default)]
  pub struct RootPatch {
      pub edition_key: Option<Option<String>>,
      pub default_kind: Option<Option<String>>,
      pub default_language: Option<Option<String>>,
      pub exclude_globs: Option<Option<String>>,
      pub scan_interval_secs: Option<Option<i64>>,
      pub keeps_documents: Option<bool>,
  }
  pub fn update_root(conn: &Connection, id: i64, patch: &RootPatch) -> Result<bool, String>;
  pub struct Removal { pub works: usize, pub positions: usize, pub notes: usize }
  pub fn removal_cost(conn: &Connection, id: i64) -> Result<Removal, String>;
  pub fn remove_root(conn: &Connection, id: i64) -> Result<Removal, String>;
  ```

- [ ] **Step 1: Write the failing tests**

```rust
/// C4 §2 calls removal the most destructive action in the product. It states
/// counts before it acts, and the counts are what removal actually costs.
#[test]
fn removing_a_root_reports_the_works_positions_and_notes_it_will_take() {
    let fixture = crate::tests::library_with_two_roots_and_a_reading_position();
    let conn = fixture.conn();
    let root = crate::scan::roots(&conn).unwrap()[1].id;

    let predicted = crate::scan::removal_cost(&conn, root).unwrap();
    let actual = crate::scan::remove_root(&conn, root).unwrap();

    assert_eq!(predicted, actual, "the count stated is not the count taken");
    assert!(predicted.works > 0 && predicted.positions > 0);
}

/// A bare DELETE would leave the works standing with `member.root_id` NULL —
/// which O54 records as the value that excludes a member from *every* root
/// grant. Removal is delete-plus-derive, in one transaction.
#[test]
fn removing_a_root_leaves_no_work_behind_it() {
    let fixture = crate::tests::library_with_two_roots_and_a_reading_position();
    let conn = fixture.conn();
    let root = crate::scan::roots(&conn).unwrap()[1].id;
    let before = crate::preview::grouping(&conn).unwrap();

    crate::scan::remove_root(&conn, root).unwrap();

    let after = crate::preview::grouping(&conn).unwrap();
    assert!(after.len() < before.len());
    let stranded: i64 = conn
        .query_row("SELECT COUNT(*) FROM member WHERE root_id IS NULL", [], |r| r.get(0))
        .unwrap();
    assert_eq!(stranded, 0, "a member with no root is invisible to every grant");
}

/// The other root is untouched — removal is scoped, and the test that would
/// pass while it was not is the one worth writing.
#[test]
fn removing_one_root_leaves_the_other_roots_works_alone() {
    let fixture = crate::tests::library_with_two_roots_and_a_reading_position();
    let conn = fixture.conn();
    let roots = crate::scan::roots(&conn).unwrap();
    let kept: Vec<String> = crate::preview::grouping(&conn)
        .unwrap()
        .keys()
        .filter(|p| p.starts_with(roots[0].path.to_str().unwrap()))
        .cloned()
        .collect();

    crate::scan::remove_root(&conn, roots[1].id).unwrap();

    let after = crate::preview::grouping(&conn).unwrap();
    for path in kept {
        assert!(after.contains_key(&path), "{path} went with the other root");
    }
}
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p taiyaku-library removing_a_root_reports`
Expected: FAIL to compile.

- [ ] **Step 3: Implement**

`removal_cost` runs the same shape as `preview::preview`: open a transaction,
`DELETE FROM root WHERE id = ?1`, `derive_with(Documents::Skip)`, count what
went, roll back. `remove_root` does the same and commits, under
`Documents::Reconcile` — a real removal *should* reconcile documents, and the
works it removes are the ones whose documents become orphans, which is
`reconcile`'s job to report.

Count positions and notes by the works that disappear, joining `progress` and
`note` to the work ids present before and absent after — not by root, which
neither table carries.

- [ ] **Step 4: Run to verify they pass**

Run: `cargo nextest run -p taiyaku-library scan`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/taiyaku-library/src/scan.rs crates/taiyaku-library/src/scan/tests.rs crates/taiyaku-library/src/tests.rs
git commit -m "removing a root is delete-plus-derive, and it says what that costs first"
```

---

### Task 10: The three validations on adding a root

**Files:**
- Modify: `crates/taiyaku-library/src/scan.rs` (`add_root` at :108)
- Modify: `crates/taiyaku-library/src/scan/tests.rs`

**Interfaces:**
- Produces:
  ```rust
  pub struct Added { pub root: Root, pub probe: crate::watch::Probe, pub interval: std::time::Duration }
  pub fn add_root_checked(conn: &Connection, path: &Path) -> Result<Added, String>;
  pub fn add_root(conn: &Connection, path: &Path) -> Result<Root, String>; // unchanged
  ```

- [ ] **Step 1: Write the failing tests**

```rust
/// C4 §2: overlapping roots double-count every file underneath, and the
/// symptom — duplicate works — looks nothing like the cause. Both directions,
/// because a parent added second is the same mistake arriving the other way
/// round.
#[test]
fn a_root_inside_an_existing_root_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let library = crate::Library::open_in_memory().unwrap();
    let conn = library.conn();
    crate::scan::add_root_checked(&conn, dir.path()).unwrap();

    let nested = dir.path().join("inner");
    std::fs::create_dir(&nested).unwrap();
    let refused = crate::scan::add_root_checked(&conn, &nested).unwrap_err();

    assert!(refused.contains("inside"), "the refusal must name the overlap: {refused}");
}

#[test]
fn a_root_containing_an_existing_root_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let inner = dir.path().join("inner");
    std::fs::create_dir(&inner).unwrap();
    let library = crate::Library::open_in_memory().unwrap();
    let conn = library.conn();
    crate::scan::add_root_checked(&conn, &inner).unwrap();

    let refused = crate::scan::add_root_checked(&conn, dir.path()).unwrap_err();

    assert!(refused.contains("contains"), "the refusal must name the overlap: {refused}");
}

/// "Readable by the server process" is not the same question as "exists" — on
/// a containerised deployment it is exactly the question that differs.
#[test]
fn a_path_the_process_cannot_read_is_refused_saying_so() {
    let dir = tempfile::tempdir().unwrap();
    let closed = dir.path().join("closed");
    std::fs::create_dir(&closed).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&closed, std::fs::Permissions::from_mode(0o000)).unwrap();
    }
    let library = crate::Library::open_in_memory().unwrap();
    let conn = library.conn();

    let refused = crate::scan::add_root_checked(&conn, &closed).unwrap_err();

    assert!(refused.contains("read"), "the refusal must say what failed: {refused}");
}

/// The probe runs on adding, so the regime is known before the first scan
/// rather than discovered later — spec B §8's experiment, at C4 §2's moment.
#[test]
fn adding_a_root_probes_the_watcher_before_the_first_scan() {
    let dir = tempfile::tempdir().unwrap();
    let library = crate::Library::open_in_memory().unwrap();
    let conn = library.conn();

    let added = crate::scan::add_root_checked(&conn, dir.path()).unwrap();

    assert_eq!(
        added.interval,
        crate::watch::interval_for(&added.probe),
        "the interval must follow the probe, not a default"
    );
}
```

The permissions test must skip on a process running as root, where mode `0o000`
is readable anyway — guard it and say so, rather than letting it pass
vacuously.

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p taiyaku-library a_root_inside_an_existing_root_is_refused`
Expected: FAIL to compile.

- [ ] **Step 3: Implement**

`add_root_checked` canonicalises the path (which also answers "exists"), reads
the directory to prove it is readable *by this process*, compares against every
existing root's canonical path in both directions, then calls `add_root`, then
runs `watch::probe` with the CLI's `PROBE_TIMEOUT` and writes
`scan_interval_secs` from `watch::interval_for`.

Leave `add_root` as it is: `import` and the fixtures call it, and a validation
that runs inside every fixture is a validation nobody can test the absence of.

- [ ] **Step 4: Run to verify they pass**

Run: `cargo nextest run -p taiyaku-library scan`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/taiyaku-library/src/scan.rs crates/taiyaku-library/src/scan/tests.rs
git commit -m "adding a root validates the path, the nesting and the watcher regime"
```

---

### Task 11: The roots routes

**Files:**
- Modify: `crates/taiyaku-serve/src/config_api.rs`
- Modify: `crates/taiyaku-serve/src/config_api/tests.rs`
- Modify: `crates/taiyaku-serve/src/routes.rs` (both lists)

**Interfaces:**
- Consumes: `scan::{roots, add_root_checked, update_root, removal_cost, remove_root}`.
- Produces: four routes.

- [ ] **Step 1: Write the failing tests**

```rust
/// The counts C4 §2 promises reach the caller before anything is removed, so
/// the screen can state them.
#[tokio::test]
async fn deleting_a_root_answers_with_what_it_removed() {
    let state = crate::test_support::state_with_two_scanned_roots();
    let id = crate::test_support::second_root_id(&state);

    let response = crate::test_support::delete_as_admin(&state, &format!("/api/library/roots/{id}")).await;

    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let json = crate::test_support::json_body(response).await;
    assert!(json["removed"]["works"].as_u64().unwrap() > 0);
}

/// A nested root is refused with 400 and a message naming the overlap, not a
/// 500 carrying a SQL error.
#[tokio::test]
async fn adding_a_nested_root_is_refused_with_the_reason() {
    let state = crate::test_support::state_with_two_scanned_roots();
    let nested = crate::test_support::a_path_inside_the_first_root(&state);
    let body = serde_json::json!({ "path": nested });

    let response = crate::test_support::post_as_admin(&state, "/api/library/roots", &body).await;

    assert_eq!(response.status(), axum::http::StatusCode::BAD_REQUEST);
    let json = crate::test_support::json_body(response).await;
    assert!(json["error"].as_str().unwrap().contains("inside"));
}

/// The list carries the regime line's inputs, since C4 §3's screen reads them.
#[tokio::test]
async fn the_root_list_carries_the_interval_and_the_last_scan() {
    let state = crate::test_support::state_with_two_scanned_roots();
    let response = crate::test_support::get_as_admin(&state, "/api/library/roots").await;
    let json = crate::test_support::json_body(response).await;
    let first = &json["roots"][0];
    assert!(first["scan_interval_secs"].is_number() || first["scan_interval_secs"].is_null());
    assert!(first.get("last_scan_at").is_some());
    assert!(first.get("keeps_documents").is_some());
}
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p taiyaku-serve deleting_a_root_answers_with`
Expected: FAIL — route not found (404), or a compile error for missing helpers.

- [ ] **Step 3: Implement**

Four routes, in `guarded_routes()`:

```rust
("/api/library/roots", get(crate::config_api::list_roots).post(crate::config_api::add_root)),
("/api/library/roots/{id}", axum::routing::patch(crate::config_api::patch_root).delete(crate::config_api::remove_root)),
```

Refusals from `add_root_checked` and `remove_root` map to `400` with the store's
own message in an `error` field; a missing id is `404`. `remove_root` runs on
`state.library.working()` — it derives.

- [ ] **Step 4: Declare both paths' grant posture**

Two more entries in the table, each with its own reason.

- [ ] **Step 5: Run to verify they pass**

Run: `cargo nextest run -p taiyaku-serve`
Expected: PASS, both route-enumeration tests included.

- [ ] **Step 6: Commit**

```bash
git add crates/taiyaku-serve/src/config_api.rs crates/taiyaku-serve/src/config_api/tests.rs crates/taiyaku-serve/src/routes.rs crates/taiyaku-serve/src/test_support.rs
git commit -m "roots are listed, added, patched and removed over HTTP"
```

---

### Task 12: The scan trigger, its registry, and the two columns nothing has ever written

**Files:**
- Modify: `crates/taiyaku-library/src/scan.rs` (write `last_scan_at`,
  `last_scan_status`)
- Modify: `crates/taiyaku-serve/src/lib.rs` (`ServeState` gains the registry)
- Modify: `crates/taiyaku-serve/src/config_api.rs`
- Modify: `crates/taiyaku-serve/src/config_api/tests.rs`
- Modify: `crates/taiyaku-serve/src/routes.rs` (both lists)
- Modify: `crates/taiyaku-cli/src/library.rs` (the CLI writes them too)

**Interfaces:**
- Consumes: `scan::scan_root`, `derive::derive`, `Library::working` (Task 2).
- Produces:
  ```rust
  // in ServeState
  pub scans: Arc<std::sync::Mutex<std::collections::HashMap<i64, ScanProgress>>>,
  pub struct ScanProgress { pub started: std::time::Instant, pub done: bool, pub report: Option<serde_json::Value> }
  ```

- [ ] **Step 1: Write the failing tests**

```rust
/// C4 §3: a scan already running is not startable twice, and the refusal says
/// which one is running rather than queueing a second walk of the same tree.
#[tokio::test]
async fn a_second_scan_of_a_root_already_scanning_is_refused() {
    let state = crate::test_support::state_with_two_scanned_roots();
    let id = crate::test_support::first_root_id(&state);
    crate::test_support::mark_scan_in_flight(&state, id);

    let response = crate::test_support::post_as_admin(&state, &format!("/api/library/roots/{id}/scan"), &serde_json::json!({})).await;

    assert_eq!(response.status(), axum::http::StatusCode::CONFLICT);
}

/// The report names what the scan did not do, per C4 §3 and `inventory`'s
/// discipline — a scan that quietly ignored 40 EPUBs and said "done" is the
/// failure this project keeps writing rules against.
#[tokio::test]
async fn the_scan_report_names_what_it_could_not_read() {
    let state = crate::test_support::state_with_an_unreadable_member();
    let id = crate::test_support::first_root_id(&state);

    crate::test_support::post_as_admin(&state, &format!("/api/library/roots/{id}/scan"), &serde_json::json!({})).await;
    let response = crate::test_support::get_as_admin(&state, &format!("/api/library/roots/{id}/scan")).await;

    let json = crate::test_support::json_body(response).await;
    assert!(!json["report"]["unreadable_dirs"].as_array().unwrap().is_empty());
    assert!(json["report"].get("refused").is_some());
    assert!(json["report"].get("raised").is_some());
}
```

And in `crates/taiyaku-library/src/scan/tests.rs`:

```rust
/// `last_scan_at` and `last_scan_status` have existed since migration 1 and
/// nothing has ever written them. C4 §3's regime line quotes both.
#[test]
fn a_scan_records_when_it_ran_and_how_it_went() {
    let fixture = crate::tests::library_with_two_roots();
    let conn = fixture.conn();
    let root = crate::scan::roots(&conn).unwrap()[0].clone();

    crate::scan::scan_root(&conn, &root).unwrap();

    let (at, status): (Option<String>, Option<String>) = conn
        .query_row("SELECT last_scan_at, last_scan_status FROM root WHERE id = ?1", [root.id], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap();
    assert!(at.is_some());
    assert_eq!(status.as_deref(), Some("ok"));
}

/// Including the outcome nobody wants to write: B §8.1's refusal is a status,
/// not a silence.
#[test]
fn a_refused_empty_scan_records_the_refusal_rather_than_nothing() {
    let fixture = crate::tests::library_with_a_root_that_becomes_empty();
    let conn = fixture.conn();
    let root = crate::scan::roots(&conn).unwrap()[0].clone();

    let report = crate::scan::scan_root(&conn, &root).unwrap();

    assert!(report.refused.is_some());
    let status: Option<String> = conn
        .query_row("SELECT last_scan_status FROM root WHERE id = ?1", [root.id], |r| r.get(0))
        .unwrap();
    assert_eq!(status.as_deref(), Some("refused"));
}
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -p taiyaku-library a_scan_records_when_it_ran`
Expected: FAIL — both columns are NULL.

- [ ] **Step 3: Write the columns in `scan_root`**

On every path out, including the refusal: `last_scan_at = datetime('now')`,
`last_scan_status` one of `ok`, `refused`, `failed`. Document that the CLI and
the route share this because they share `scan_root` — the two cannot disagree.

- [ ] **Step 4: Implement the trigger and status routes**

```rust
("/api/library/roots/{id}/scan", get(crate::config_api::scan_status).post(crate::config_api::start_scan)),
```

`start_scan` inserts into the registry under the mutex, refusing with `409` if
the id is already there, then spawns the work on
`tokio::task::spawn_blocking`, taking its connection from
`state.library.working()`. It runs `scan_root` then `derive::derive` and writes
the rendered report back into the registry entry.

Answer `202 Accepted` on start, with the status URL.

`scan_status` renders `ScanReport` and `DeriveReport` **whole** — every field,
including the empty vectors, because C4 §3's discipline is that a scan names
what it did not do and an omitted empty list reads as a question nobody asked.

Add a comment where `ServeState.works` is read nearby, or in the status
response, stating that the reader's `/api/works` list is a startup snapshot and
is not refreshed by a scan.

- [ ] **Step 5: Declare the route's grant posture**

One entry, its own reason.

- [ ] **Step 6: Run to verify everything passes**

Run: `cargo nextest run -p taiyaku-library scan` then
`cargo nextest run -p taiyaku-serve`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add crates/taiyaku-library/src/scan.rs crates/taiyaku-library/src/scan/tests.rs crates/taiyaku-serve/src/lib.rs crates/taiyaku-serve/src/config_api.rs crates/taiyaku-serve/src/config_api/tests.rs crates/taiyaku-serve/src/routes.rs crates/taiyaku-cli/src/library.rs
git commit -m "a scan can be started from the server, once per root, and it records how it went"
```

---

### Task 13: The documents this moves

**Files:**
- Modify: `docs/status.md`
- Modify: `README.md`
- Modify: `docs/notes.md` (if Task 4's measurement needs a follow-up figure)

- [ ] **Step 1: Run the open-items sweep on the way in**

```sh
grep -c '^| O[0-9]' docs/status.md
grep -c '^| O[0-9]* | \[x\]' docs/status.md
grep -nE '[0-9]{3,4} tests|of the [0-9]+ are standing' docs/status.md
grep -n -A1 '^What remains:' docs/status.md
```

Record the four numbers before editing anything. The count word wraps across a
line break, so read the line *after* `What remains:`.

- [ ] **Step 2: Update the test count in both places**

Run `env -u TAIYAKU_CORPUS scripts/check-all.sh` and take the figure from its
`status-test-count` step. It appears twice — `Six crates, N tests` and `Nineteen of
the N are standing checks` — and CI compares both against the JUnit report.

- [ ] **Step 3: Write the branch's narrative section**

What this branch shipped, what each gate found that the ones before it could
not, and the two facts worth not re-deriving: that `derive` writes to the
library tree and a preview must not, and that `last_scan_at` /
`last_scan_status` were dead columns for the whole life of the project until
now.

- [ ] **Step 4: Update `README.md`**

Configuring a library stops being a CLI-only activity for rules, roots and
scanning. The "Seven commands" synopsis block is part of the contract — check
whether anything in it changed.

- [ ] **Step 5: Grep the bare identifiers**

```sh
grep -rn 'O54\|O58\|O60' docs/ web/src/
```

Read every hit. O54's entry says the two capability columns cannot be set
outside a fixture — still true after this PR, since users are PR 2 — so it must
*not* be edited here. O58 and O60 are unchanged. Prose outside the table
describes open items in wording no grep predicts; read the paragraphs, not just
the matches.

- [ ] **Step 6: Run the gate and quote it**

```sh
env -u TAIYAKU_CORPUS scripts/check-all.sh
```

Expected: the final line reads `all green`. Do not pipe it through `tail` and do
not append `; echo` — both mask the exit status.

Then the corpus job:

```sh
TAIYAKU_CORPUS="$PWD/corpus" cargo test --workspace --all-features corpus:: -- --nocapture --test-threads=1
```

- [ ] **Step 7: Sweep the mutation harness over what changed**

```sh
scripts/mutants.sh --since main
```

`--since main` reads `git diff main...HEAD`, so commit everything first. Check
the **unviable** count against what was changed: O88 reports untested functions
as `unviable`, so a number that moved by more than the new `Default` derives is
a finding rather than noise. A survivor in `preview.rs` or in `derive`'s
documents guard is the one worth chasing — those two carry this branch's claim.

- [ ] **Step 8: Commit and open the PR**

```bash
git add docs/status.md README.md docs/notes.md
git commit -m "the documents this branch moves"
```

Open the PR through the **gitea** MCP, not `gh` — `gh` is authenticated against
the wrong host. Never hard-wrap the PR body: it becomes the squash commit
message verbatim.

---

## Notes for the implementer

**On fixtures.** Several tasks name a fixture that may not exist
(`library_with_two_roots`, `library_with_a_kept_document`,
`library_with_the_seeded_defaults`). Look in
`crates/taiyaku-library/src/tests.rs`, `hand/tests.rs` and `derive/tests.rs`
before writing a new one — this crate has a lot of them already, and a second
fixture of the same shape is how two tests come to disagree about what the
library looks like.

**On fixtures that pass against a broken build.** A grant test in spec C1
passed against a deliberately broken build because the fixture's root ids and
work ids collided at 1, 2, 3. When a new fixture's assertion depends on an id,
make the two sequences differ, and hand-apply the mutation to watch the test
fail before trusting it.

**On `member.root_id`.** It is populated by `derive::write_roots`, which runs
*after* `import::scanned` returns — so anything addressing a document from
inside `import_with` reads `None`, always. Check `hand::path_for` empirically at
any new call site rather than reasoning about when it is populated.

**On reviews.** Per-task review passed on every task of spec C3's branch while
the whole-branch review found seven Criticals — and on #91 the fix for its
Critical was itself incomplete. Review the fixes too.
