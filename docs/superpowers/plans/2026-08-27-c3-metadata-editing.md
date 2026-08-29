# C3 PR 2 — metadata editing, its on-disk home, and O62: implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The library stops asserting things that are not true, and stops
forgetting the corrections — a person can fix a work's title, volume, series,
edition labels and chapter numbers; the correction survives the next scan; it
survives losing the database; and a work whose files are gone is shown as
unreadable rather than offered and then failing at its first page.

**Architecture:** Four layers. **Migration 010** adds `work.title_hand` and
`edition.label_hand`, the provenance the document needs so that projecting a
title does not make every work in the library look hand-edited. **`librarydoc`**
gains those two fields. **A new `hand.rs`** owns the address
`<root>/.taiyaku/<content_key>.library.json`, its write, and the read-back arm
in `derive::write_work` that makes a hand edit survive a rescan with no
database. **A new `metadata.rs` in `taiyaku-serve`** is four thin PATCH handlers
over `librarydoc::apply` + `hand::save`, following `curation.rs`. O62 is one
query in `browse`, one response field, and one piece of web treatment.

**Tech Stack:** Rust (`rusqlite`, `axum`, `serde`, `serde_json`, `sha2`),
TypeScript/React 19 (`react-router`, `vitest` + `jsdom`,
`@testing-library/react`).

## Global Constraints

- **Specs:** `docs/superpowers/specs/2026-08-26-library-curation-build-design.md`
  is authoritative — **read its §1 "Widened", §7's two correction blocks and
  §12 first**, all added 2026-08-27; three of that document's original premises
  were false and the corrections are what this plan is built on. C3's product
  design is `docs/superpowers/specs/2026-08-14-library-curation-design.md`; the
  store is `docs/superpowers/specs/2026-08-14-library-store-design.md`.
- **The gate is `env -u TAIYAKU_CORPUS scripts/check-all.sh`.** Never run it
  with `TAIYAKU_CORPUS` set: the script exports it into an unfiltered
  `cargo test --workspace`, which runs `geometry_experiment` in debug (40+ min).
  Quote the `all green` line when reporting; piping through `tail` masks the
  exit status.
- **Every new route is admin-gated** (spec A2) and **must** get a row in
  `routes.rs`'s `grant_postures()` table alongside its registration in
  `guarded_routes()` — `auth::tests::every_route_declares_its_grant_posture`
  asserts the two route sets against each other, so a route added to one
  without the other fails the build on the commit that adds it. Also add it to
  the typed list `auth::tests::every_route_refuses_an_anonymous_request` drives.
- **`pnpm -C web test` is NOT the web suite.** It is `vitest run --project node`
  and never executes a single `.test.tsx`. The gate runs
  `pnpm -C web run test:coverage` (`--project node --project jsdom --coverage`),
  and that is the command to verify against. `pnpm -C web test:jsdom` runs the
  component half alone.
- **Web tests land in whichever vitest project matches what they test** —
  `jsdom` for components (`.test.tsx`), **`node` for DOM-free modules**
  (`.test.ts`).
- **`@testing-library/react`'s `screen` and `userEvent` are not available
  here** — `web/src/testing/mount.tsx` explicitly rejects them. Component
  tests use this repo's own `mount()` + `act()` + `querySelector` convention.
  Nothing here may import `App.tsx` or `resolver.ts`, which value-import
  `taiyaku-wasm` from a gitignored generated directory and break a fresh clone.
- **No `Date.now()`-style ambient values in fixtures**, and **no attribution
  lines in commit messages**.
- **Derivation still has exactly one input.** Nothing in this PR makes a hand
  edit a second input to a rule. A hand edit *withdraws* a work's four derived
  columns from derivation, via the marker; it never feeds them.
- **A hand edit never moves a slug.** `slug` is written by exactly one
  statement in the crate — `import::insert_work`'s INSERT (`import.rs:579`),
  fed by `unique_slug` from the title at creation — and appears in **zero**
  `UPDATE`s. Task 2 pins that with a test rather than building it. *(Re-checked
  2026-08-28 with a multi-line-aware scan after the claim below turned out to
  have been taken from a truncated grep. The slug half holds.)*

- **The effective column is always `COALESCE(hand, derived)`, and four
  statements must keep it so.** *This constraint replaces a false one.* The
  plan originally asserted that `work.title` is written only by
  `import::insert_work`'s INSERT and `edition.label` only by `import.rs:793`.
  That came from a `grep … | head -20` whose output was cut off before
  `import.rs:555`, and Task 2's re-review found the rest. The real writers are:

  | statement | writes |
  | --- | --- |
  | `import::insert_work` (`import.rs:579`) | mints `title`, `title_derived` |
  | `import::update_work` (`import.rs:555`) | overwrites `title` on **`Scope::Document`** — a sidecar reimport, which is routine. Takes an early return on `Scope::Scan` and leaves the work block alone. |
  | `import::replace_editions` (`import.rs:792`) | deletes and reinserts **every** edition row on every scan and every reimport, restoring only what `held_edition_meta` carried |
  | `librarydoc::apply` / `apply_editions` | the hand edit itself |

  `held_edition_meta` **used to select** `id, key, label, language, kind` —
  neither provenance column — so a rescan wiped `label_hand` to NULL *and*
  rebound `label_derived` to the current, possibly hand-set, `label`: the
  provenance vanished and the safety net was overwritten with the very value it
  exists to fall back from. Step 6c is what closed that; this paragraph
  describes the defect, not the code as it stands.

---

## File Structure

**Created**

- `crates/taiyaku-library/src/schema/010_hand_metadata.sql` — the two
  provenance columns.
- `crates/taiyaku-library/src/hand.rs` + `crates/taiyaku-library/src/hand/tests.rs`
  — where a work's hand-fixed metadata lives on disk, how it is written, and
  how it is found again. Build design §12.
- `crates/taiyaku-serve/src/metadata.rs` + `crates/taiyaku-serve/src/metadata/tests.rs`
  — the four PATCH handlers. Thin, like `curation.rs`.
- `web/src/metadata.ts` + `web/src/metadata.test.ts` — the HTTP client and its
  pure helpers.
- `web/src/MetadataEditor.tsx` + `web/src/MetadataEditor.test.tsx` — the editor,
  reachable from the series page.

**Modified**

- `crates/taiyaku-library/src/schema.rs:36-51` — the `MIGRATIONS` array and
  `SCHEMA_VERSION`.
- `crates/taiyaku-library/src/librarydoc.rs` — `LibraryDoc.title`,
  `EditionMeta.label`, and their arms in `of`, `apply` and `is_empty`.
- `crates/taiyaku-library/src/derive.rs:1197` — the read-back arm, immediately
  before the I7 guard.
- `crates/taiyaku-library/src/lib.rs` — `pub mod hand;`.
- `crates/taiyaku-library/src/browse.rs` — O62's `missing` count.
- `crates/taiyaku-serve/src/lib.rs` — `mod metadata;`.
- `crates/taiyaku-serve/src/routes.rs` — four registrations in
  `guarded_routes()` and four rows in `grant_postures()`.
- `web/src/Series.tsx` — the editor's entry point and O62's treatment.

**Why `hand.rs` rather than `export::work`:** build design §12, "Not
`export::work`". That function writes five documents, removes stale ones, takes
a `user_id`, and refuses a work with a NULL `sidecar_path`. Routing a title
correction through it would rewrite `taiyaku.json` and **delete**
`taiyaku.progress.json` and `taiyaku.truth.json` whenever the database happened
to hold no position and no truth for that work.

---

### Task 1: Migration 010 — provenance for title and label

**Files:**
- Create: `crates/taiyaku-library/src/schema/010_hand_metadata.sql`
- Modify: `crates/taiyaku-library/src/schema.rs:36-51`
- Test: `crates/taiyaku-library/src/schema/tests.rs`

**Interfaces:**
- Produces: two nullable columns, `work.title_hand TEXT` and
  `edition.label_hand TEXT`, and `SCHEMA_VERSION` 10. Task 2 reads and writes
  both.

**Why this exists at all.** `work.title` and `edition.label` are both
`NOT NULL` and both always populated, and neither is ever updated after
creation — `work.title` is written only by `import::insert_work`'s INSERT
(`import.rs:579`), `edition.label` only by `import.rs:793`. So a document that
projected them directly would be non-empty for **every work in the library**,
and `librarydoc::of(...).is_empty()` is load-bearing in two places that would
then both be wrong: `migrate::migrate_library`'s "does this work already hold
hand-fixed metadata" test, and `export::work`'s write-or-remove decision, which
would start writing a `taiyaku.library.json` beside every work in the corpus.
A nullable sibling column says "a person set this" without disturbing either.

- [ ] **Step 1: Write the failing test**

In `crates/taiyaku-library/src/schema/tests.rs`:

```rust
#[test]
fn migration_ten_adds_hand_provenance_columns_defaulting_to_null() {
    let conn = Connection::open_in_memory().expect("in-memory database");
    super::migrate(&conn).expect("migrating");

    conn.execute_batch(
        "INSERT INTO work (id, content_key, slug, title, reading, hub)
         VALUES (1, 'k', 'w', 'Derived Title', 'rtl', 'en');
         INSERT INTO edition (id, work_id, key, label)
         VALUES (1, 1, 'en', 'Derived Label');",
    )
    .expect("seeding");

    let title_hand: Option<String> = conn
        .query_row("SELECT title_hand FROM work WHERE id = 1", [], |r| r.get(0))
        .expect("reading work.title_hand");
    let label_hand: Option<String> = conn
        .query_row("SELECT label_hand FROM edition WHERE id = 1", [], |r| {
            r.get(0)
        })
        .expect("reading edition.label_hand");

    assert_eq!(title_hand, None, "a derived title is not a hand-set one");
    assert_eq!(label_hand, None, "a derived label is not a hand-set one");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p taiyaku-library schema::tests::migration_ten_adds_hand_provenance_columns_defaulting_to_null`
Expected: FAIL — `no such column: title_hand`.

- [ ] **Step 3: Write the migration**

Create `crates/taiyaku-library/src/schema/010_hand_metadata.sql`:

```sql
-- **Provenance for the two fields that are always present.** C3 §5 makes
-- `work.title` and `edition.label` editable, and O67 is the defect that asks
-- for it: a work spanning five editions is named after whichever file minted
-- its row, `.cbz` and `(2021)` included.
--
-- Both columns are `NOT NULL` and both are always populated, so a
-- `<stem>.library.json` that projected them directly would be non-empty for
-- every work in the library. `librarydoc::of(...).is_empty()` decides two
-- things that would then both be wrong: whether `migrate` thinks this work
-- already holds hand-fixed metadata, and whether `export::work` writes a
-- library document or removes a stale one. The second would put a
-- `taiyaku.library.json` beside every work in the corpus.
--
-- So the hand value gets a column of its own and the effective value keeps
-- its. `librarydoc::apply` writes both; everything that reads a title or a
-- label goes on reading `work.title` / `edition.label` unchanged, which is
-- why this is two `ALTER TABLE`s and no rewrite.
--
-- **Four columns, not two, and the reason is what `NOT NULL` costs.**
-- `work.title` and `edition.label` are `NOT NULL`, so the effective column
-- cannot be emptied — and the value derivation minted is destroyed by the
-- first hand edit that overwrites it, since nothing recomputes a title after
-- `import::insert_work`'s INSERT. Without somewhere to keep it, *clearing* a
-- hand-set title cannot restore anything: the row would keep displaying the
-- withdrawn hand value while `title_hand` read NULL, so `librarydoc::of`
-- would report no hand edit, `export::work` would delete the document
-- recording it, and a rescan of the same library would mint a title the
-- database disagrees with. Found by Task 2's review, 2026-08-28.
--
-- So the derived value gets a column of its own, and `apply` writes
-- `title = COALESCE(?, title_derived)`. Clearing then restores exactly what
-- derivation minted, and every reader goes on reading `work.title` unchanged.
--
-- **Additive, per the store spec §10's rule.** The backfill is exact rather
-- than approximate: O44 records that there has never been a route to apply a
-- hand edit after a work's first import, so every `title` in every existing
-- database *is* the derived one, and copying it across states a truth rather
-- than guessing one.
ALTER TABLE work    ADD COLUMN title_hand    TEXT;
ALTER TABLE work    ADD COLUMN title_derived TEXT;
ALTER TABLE edition ADD COLUMN label_hand    TEXT;
ALTER TABLE edition ADD COLUMN label_derived TEXT;

UPDATE work    SET title_derived = title;
UPDATE edition SET label_derived = label;
```

- [ ] **Step 4: Register it**

In `crates/taiyaku-library/src/schema.rs`, change the array length and add the
line:

```rust
const MIGRATIONS: [&str; 10] = [
    include_str!("schema/001_initial.sql"),
    include_str!("schema/002_absorbed.sql"),
    include_str!("schema/003_scanner.sql"),
    include_str!("schema/004_auth.sql"),
    include_str!("schema/005_note_id_per_user.sql"),
    include_str!("schema/006_member_root.sql"),
    include_str!("schema/007_backfill_member_root.sql"),
    include_str!("schema/008_trigram_fts.sql"),
    include_str!("schema/009_progress_per_edition.sql"),
    include_str!("schema/010_hand_metadata.sql"),
];
```

Then find `SCHEMA_VERSION` in the same file and raise it to `10`. The
`const _: () = assert!(MIGRATIONS.len() == SCHEMA_VERSION as usize);` on line
51 fails the build if you change one and not the other — that is the check, do
not add a second.

- [ ] **Step 5: Run the test to verify it passes**

Run: `cargo test -p taiyaku-library schema::`
Expected: PASS, including the existing version-row and foreign-key tests.

- [ ] **Step 6: Commit**

```bash
git add crates/taiyaku-library/src/schema/010_hand_metadata.sql crates/taiyaku-library/src/schema.rs crates/taiyaku-library/src/schema/tests.rs
git commit -m "Migration 010: provenance for a hand-set title and edition label"
```

---

### Task 2: `LibraryDoc` gains a work title and an edition label

**Files:**
- Modify: `crates/taiyaku-library/src/librarydoc.rs` — the `LibraryDoc` and
  `EditionMeta` structs, `is_empty`, `of`, `editions_meta_of`, `apply`,
  `apply_editions`
- Test: `crates/taiyaku-library/src/librarydoc/tests.rs`

**Interfaces:**
- Consumes: Task 1's `work.title_hand`, `edition.label_hand`.
- Produces: `LibraryDoc { title: Option<String>, .. }` and
  `EditionMeta { label: Option<String>, .. }`. Tasks 3–6 read and write the
  document through these.

**The three fields C3 §5 lists at work level are all reachable after this.**
`volume` and `series` already round-trip; `title` is what this adds. See the
build design §7's second correction for why series membership is no longer
deferred: `derive.rs:1197` has guarded the derived columns behind
`!migrate::absorbed(conn, work_id, LIBRARY)` since #77, and the marker is set
by the document write Task 4 performs.

- [ ] **Step 1: Write the failing tests**

In `crates/taiyaku-library/src/librarydoc/tests.rs`:

```rust
#[test]
fn a_hand_set_title_and_label_round_trip_through_the_document() {
    let (conn, work_id) = a_work_with_one_edition();

    let mut doc = super::of(&conn, work_id).expect("projecting");
    doc.title = Some("Go-Toubun no Hanayome, volume 1".to_string());
    doc.editions.insert(
        "en-mono".to_string(),
        super::EditionMeta {
            label: Some("English, monochrome".to_string()),
            ..Default::default()
        },
    );
    super::apply(&conn, work_id, &doc).expect("applying");

    let effective: String = conn
        .query_row("SELECT title FROM work WHERE id = ?1", [work_id], |r| {
            r.get(0)
        })
        .expect("reading work.title");
    assert_eq!(
        effective, "Go-Toubun no Hanayome, volume 1",
        "apply writes the effective column, not only the provenance one"
    );

    let label: String = conn
        .query_row(
            "SELECT label FROM edition WHERE work_id = ?1 AND key = 'en-mono'",
            [work_id],
            |r| r.get(0),
        )
        .expect("reading edition.label");
    assert_eq!(label, "English, monochrome");

    let again = super::of(&conn, work_id).expect("re-projecting");
    assert_eq!(again.title.as_deref(), Some("Go-Toubun no Hanayome, volume 1"));
    assert_eq!(
        again.editions["en-mono"].label.as_deref(),
        Some("English, monochrome")
    );
}

#[test]
fn a_derived_title_and_label_leave_the_document_empty() {
    let (conn, work_id) = a_work_with_one_edition();

    let doc = super::of(&conn, work_id).expect("projecting");

    assert!(
        doc.is_empty(),
        "a work nobody has hand-edited must project to the empty document, or \
         `migrate` and `export::work` both start believing every work in the \
         library holds hand-fixed metadata: {doc:?}"
    );
}

#[test]
fn clearing_a_hand_set_title_restores_the_one_derivation_minted() {
    let (conn, work_id) = a_work_with_one_edition();
    let derived: String = conn
        .query_row("SELECT title FROM work WHERE id = ?1", [work_id], |r| {
            r.get(0)
        })
        .expect("reading the derived title");

    let mut doc = super::of(&conn, work_id).expect("projecting");
    doc.title = Some("Hand Set".to_string());
    super::apply(&conn, work_id, &doc).expect("applying");

    let mut doc = super::of(&conn, work_id).expect("re-projecting");
    doc.title = None;
    super::apply(&conn, work_id, &doc).expect("clearing");

    let after: String = conn
        .query_row("SELECT title FROM work WHERE id = ?1", [work_id], |r| {
            r.get(0)
        })
        .expect("reading the title back");
    assert_eq!(
        after, derived,
        "withdrawing a hand title must restore what derivation minted. \
         `COALESCE(?, title)` would leave `Hand Set` here forever, while \
         `title_hand` read NULL — so `of` would report no hand edit, \
         `export::work` would delete the document recording it, and a rescan \
         would mint a title this database disagrees with"
    );
    assert!(
        super::of(&conn, work_id).expect("projecting").is_empty(),
        "and the document is empty again, which is what removes it from disk"
    );
}

#[test]
fn editing_a_title_does_not_move_the_slug() {
    let (conn, work_id) = a_work_with_one_edition();
    let before: String = conn
        .query_row("SELECT slug FROM work WHERE id = ?1", [work_id], |r| {
            r.get(0)
        })
        .expect("reading slug");

    let mut doc = super::of(&conn, work_id).expect("projecting");
    doc.title = Some("A Completely Different Name".to_string());
    super::apply(&conn, work_id, &doc).expect("applying");

    let after: String = conn
        .query_row("SELECT slug FROM work WHERE id = ?1", [work_id], |r| {
            r.get(0)
        })
        .expect("reading slug");
    assert_eq!(
        before, after,
        "a slug is an address and a title is a display name; progress, notes \
         and truth all hang off the slug (O67)"
    );
}
```

If `a_work_with_one_edition()` does not already exist in this test module, add
it beside the other helpers, following whatever fixture convention the file
already uses — it must produce a migrated connection holding one `work` row and
one `edition` row keyed `en-mono`, with `title_hand` and `label_hand` NULL.

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p taiyaku-library librarydoc::tests::a_hand_set_title_and_label_round_trip_through_the_document`
Expected: FAIL — `LibraryDoc` has no field named `title`.

- [ ] **Step 3: Add the fields**

In `crates/taiyaku-library/src/librarydoc.rs`, add to `LibraryDoc` immediately
after `volume`:

```rust
    /// A hand-set work title, overriding the one derivation minted.
    ///
    /// `Option`, and projected from `work.title_hand` rather than from
    /// `work.title` — see migration 010 for why the effective column cannot
    /// be the one that appears here. **O67** is the defect: a work spanning
    /// five editions is named after whichever file minted its row, so a
    /// reader is shown "…Full Color Edition… — en-mono" for a monochrome
    /// edition. `work.slug` is unaffected and deliberately so; it is an
    /// address, and progress, notes and truth all hang off it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
```

And to `EditionMeta`, before `language`:

```rust
    /// A hand-set edition label, overriding the one the import minted.
    /// Projected from `edition.label_hand`, for migration 010's reason.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
```

- [ ] **Step 4: Extend `is_empty`**

```rust
    pub fn is_empty(&self) -> bool {
        self.title.is_none()
            && self.series.is_empty()
            && self.volume.is_none()
            && self.editions.is_empty()
            && self.members.is_empty()
            && self.tags.is_empty()
            && self.external.is_empty()
    }
```

- [ ] **Step 5: Project both fields in `of`**

Change `of`'s opening query to read the provenance column too:

```rust
    let (volume, series_id, title): (Option<f64>, Option<i64>, Option<String>) = conn
        .query_row(
            "SELECT volume, series_id, title_hand FROM work WHERE id = ?1",
            [work_id],
```

adjusting the row closure to `Ok((row.get(0)?, row.get(1)?, row.get(2)?))`, and
set `title` on the returned `LibraryDoc`.

In `editions_meta_of`, read `label_hand` and keep the omission rule honest — an
edition still appears only if a person has actually set something on it:

```rust
    let mut stmt = conn
        .prepare("SELECT id, key, label_hand, language, kind FROM edition WHERE work_id = ?1")
        .map_err(|e| format!("preparing editions: {e}"))?;
```

with the tuple widened to carry `label: Option<String>`, and the skip condition
widened to match:

```rust
        if label.is_none() && language.is_none() && kind.is_none() && credits.is_empty() {
            continue;
        }
```

- [ ] **Step 6: Write both fields in `apply`**

In `apply`, replace the opening `UPDATE` — it is the row-count check as well as
the volume write, so keep both jobs:

```rust
    // `title` is written to two columns: `title_hand` is the provenance this
    // document projects from, and `title` is what every reader of a work
    // actually displays. **`COALESCE(?2, title_derived)`, never
    // `COALESCE(?2, title)`** — falling back to the column being written
    // leaves a withdrawn hand title in place forever, because the value
    // derivation minted was destroyed by the first edit that overwrote it and
    // nothing recomputes one. `title_derived` is migration 010's copy of it,
    // so clearing a hand title restores derivation's own answer, which is the
    // only sense in which "no hand title" can be written to a NOT NULL column.
    let updated = tx
        .execute(
            "UPDATE work
                SET volume     = ?1,
                    title_hand = ?2,
                    title      = COALESCE(?2, title_derived, title)
              WHERE id = ?3",
            params![doc.volume, doc.title, work_id],
        )
        .map_err(|e| format!("writing volume and title for work {work_id}: {e}"))?;
```

In `apply_editions`, clear the hand label alongside the other owned columns:

```rust
    conn.execute(
        "UPDATE edition SET label_hand = NULL, language = NULL, kind = NULL WHERE work_id = ?1",
        [work_id],
    )
    .map_err(|e| format!("clearing edition metadata for work {work_id}: {e}"))?;
```

and write it in the per-edition update, with the same `COALESCE` reasoning:

```rust
        conn.execute(
            "UPDATE edition
                SET label_hand = ?1,
                    label      = COALESCE(?1, label_derived, label),
                    language   = ?2,
                    kind       = ?3
              WHERE id = ?4",
            params![
                meta.label,
                meta.language,
                meta.kind.map(edition_kind_text),
                edition_id
            ],
        )
        .map_err(|e| format!("writing metadata for edition `{key}`: {e}"))?;
```

- [ ] **Step 6b: Every new row records what derivation minted**

`COALESCE(?, title_derived, title)` needs `title_derived` populated, and
migration 010 backfills only the rows that existed when it ran. The two
statements that mint rows must fill it too, or a work imported after the
migration falls through to the third arm and goes sticky again.

In `crates/taiyaku-library/src/import.rs:579`, `insert_work`:

```rust
    conn.execute(
        "INSERT INTO work (content_key, slug, title, title_derived, reading, hub, sidecar_path)
         VALUES (?1, ?2, ?3, ?3, ?4, ?5, ?6)",
```

and in the edition INSERT at `import.rs:793`:

```rust
            "INSERT INTO edition (work_id, key, label, label_derived) VALUES (?1, ?2, ?3, ?3)",
```

Both reuse the bound parameter rather than binding it twice: at INSERT the
derived value and the effective value are by definition the same string, and
writing it once is what keeps them from drifting apart in a later edit to this
statement.

- [ ] **Step 6c: Hold the invariant in the three paths that also write these columns**

`apply` is not the only writer, and provenance that any routine rescan erases
is worse than no provenance — it makes `of` assert a hand edit is gone while
the hand text is still on display. The invariant to hold everywhere is:

> `X_derived` is what derivation or the document said. `X_hand` is what a
> person said, or NULL. The effective `X` — which is `NOT NULL` and is what
> every reader displays — is always `COALESCE(X_hand, X_derived)`.

**`apply_editions`'s blanket clear** (`librarydoc.rs:785`) resets `label_hand`
for every edition but leaves `label`, so an edition *dropped from the document
entirely* — the key absent, rather than `label: None` — keeps its withdrawn
hand label forever. Restore it in the same statement:

```rust
    conn.execute(
        "UPDATE edition
            SET label_hand = NULL,
                label      = COALESCE(label_derived, label),
                language   = NULL,
                kind       = NULL
          WHERE work_id = ?1",
        [work_id],
    )
```

**`held_edition_meta`** must carry both provenance columns, so add
`label_hand` and `label_derived` to its `SELECT` and to `HeldEdition`.
**`replace_editions`** then reinserts against the invariant rather than
against the current display value:

- `Scope::Document` — the document is the derived truth:
  `label_derived = edition.label`.
- `Scope::Scan` — a walk has no display name, so the held derived value
  stands: `label_derived = held.label_derived`.
- Either way `label_hand = held.label_hand` and
  `label = COALESCE(held.label_hand, label_derived)`.

The existing comment at `import.rs:776` explains why a scan must not take the
incoming label at face value — *"an import that took the incoming label at
face value renamed a person's `Japanese raw` to `raw` on the next routine
rescan"*, measured in C2's whole-branch review. That argument is the same one
this step extends to the provenance columns; keep it and extend it rather than
writing a second one.

**`update_work`'s `Scope::Document` branch** (`import.rs:555`) overwrites
`title` from the sidecar on every reimport. The sidecar's title is a derived
value, so write it as one:

```rust
        "UPDATE work SET content_key = ?1, title_derived = ?2,
                title = COALESCE(title_hand, ?2), reading = ?3, hub = ?4,
                sidecar_path = COALESCE(?5, sidecar_path)
         WHERE id = ?6",
```

The `Scope::Scan` branch above it is already correct and must stay untouched:
it writes `content_key` alone and returns, which is why a scanned work — the
kind O67 is about — was never at risk from this path.

**Three tests, each watched failing first.** They are the coverage whose
absence let this ship: no test exercised a second apply that *omits* an
edition key, a rescan over a hand-set label, or a document reimport over a
hand-set title.

```rust
#[test]
fn dropping_an_edition_from_the_document_restores_its_derived_label() {
    let (conn, work_id) = a_work_with_one_edition();

    let mut doc = super::of(&conn, work_id).expect("projecting");
    doc.editions.insert(
        "en-mono".to_string(),
        super::EditionMeta {
            label: Some("English, monochrome".to_string()),
            ..Default::default()
        },
    );
    super::apply(&conn, work_id, &doc).expect("applying");

    // The key is absent entirely, which is a different statement from
    // `label: None` and takes a different branch: the blanket clear, not the
    // per-key update.
    let mut doc = super::of(&conn, work_id).expect("re-projecting");
    doc.editions.clear();
    super::apply(&conn, work_id, &doc).expect("clearing");

    let label: String = conn
        .query_row(
            "SELECT label FROM edition WHERE work_id = ?1 AND key = 'en-mono'",
            [work_id],
            |r| r.get(0),
        )
        .expect("reading the label");
    assert_eq!(
        label, "en-mono",
        "an edition dropped from the document must fall back to its derived \
         label, not keep the withdrawn hand one while `label_hand` reads NULL"
    );
}
```

The rescan and reimport tests belong beside `import.rs`'s own suite, where the
fixtures for driving `import_with` under each `Scope` already exist. Each must
assert both halves: that the hand value survives, **and** that
`label_derived` / `title_derived` still holds derivation's answer rather than
the hand text.

- [ ] **Step 7: Run the tests to verify they pass**

Run: `cargo test -p taiyaku-library librarydoc:: import::`
Expected: PASS, including the existing
`of_is_empty_for_a_work_nothing_has_been_applied_to`.

- [ ] **Step 8: Commit**

```bash
git add crates/taiyaku-library/src/librarydoc.rs crates/taiyaku-library/src/librarydoc/tests.rs
git commit -m "LibraryDoc carries a hand-set work title and edition label"
```

---

### Task 3: `hand.rs` — the address

**Files:**
- Create: `crates/taiyaku-library/src/hand.rs`, `crates/taiyaku-library/src/hand/tests.rs`
- Modify: `crates/taiyaku-library/src/lib.rs`

**Interfaces:**
- Produces: `hand::path_for(conn: &Connection, work_id: i64) -> Result<Option<PathBuf>, String>`,
  answering `<root>/.taiyaku/<content_key>.library.json` — or `None` for a work
  whose hub edition has no member with a root, which is every work imported
  from a sidecar. Tasks 4 and 5 both call it.

Build design §12 is the argument for every part of this address. In short: a
dotted directory is skipped by `scan.rs:679` and so is invisible to the fact
walk and to `watch.rs` for free; inside a root because the durability claim is
that the organisational work survives losing the database, which a file living
with the machine does not keep; and keyed by `content_key` because
`identity::content_key` hashes the **hub edition's** members alone and the hub
is fixed at creation, so the address survives renames, remounts and added
editions.

- [ ] **Step 1: Write the failing test**

Create `crates/taiyaku-library/src/hand/tests.rs`:

```rust
use rusqlite::Connection;

/// A migrated database holding one scanned work: one root at `/library`, a hub
/// edition `en`, and one member under that root.
fn a_scanned_work(conn: &Connection) -> i64 {
    crate::schema::migrate(conn).expect("migrating");
    conn.execute_batch(
        "INSERT INTO root (id, path) VALUES (1, '/library');
         INSERT INTO work (id, content_key, slug, title, reading, hub)
         VALUES (1, 'abc123', 'w', 'Derived', 'rtl', 'en');
         INSERT INTO edition (id, work_id, key, label) VALUES (1, 1, 'en', 'en');
         INSERT INTO member (id, edition_id, ordinal, path, sha256, pages, root_id)
         VALUES (1, 1, 0, '/library/v01.cbz', 'deadbeef', 100, 1);",
    )
    .expect("seeding");
    1
}

#[test]
fn the_address_is_a_dotfile_directory_under_the_hub_editions_root() {
    let conn = Connection::open_in_memory().expect("in-memory database");
    let work_id = a_scanned_work(&conn);

    let path = super::path_for(&conn, work_id)
        .expect("resolving")
        .expect("a scanned work has a root");

    assert_eq!(
        path,
        std::path::Path::new("/library/.taiyaku/abc123.library.json"),
        "the address is <root>/.taiyaku/<content_key>.library.json — a dotted \
         directory so `scan.rs` skips it without an exclusion list, and keyed \
         by content so a rename or a remount does not move it"
    );
}

#[test]
fn a_work_with_no_rooted_member_has_no_address() {
    let conn = Connection::open_in_memory().expect("in-memory database");
    crate::schema::migrate(&conn).expect("migrating");
    conn.execute_batch(
        "INSERT INTO work (id, content_key, slug, title, reading, hub)
         VALUES (1, 'abc123', 'w', 'Derived', 'rtl', 'en');
         INSERT INTO edition (id, work_id, key, label) VALUES (1, 1, 'en', 'en');",
    )
    .expect("seeding");

    assert_eq!(
        super::path_for(&conn, 1).expect("resolving"),
        None,
        "an imported work keeps its sidecar's directory and does not want this \
         address; only a scanned work, whose `sidecar_path` is NULL by design, \
         has nowhere else to put the document"
    );
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p taiyaku-library hand::`
Expected: FAIL — unresolved module `hand`.

- [ ] **Step 3: Write the module**

Create `crates/taiyaku-library/src/hand.rs`:

```rust
//! Where a work's hand-fixed metadata lives on disk, and how it is found
//! again. Build design §12.
//!
//! `<stem>.library.json` beside a sidecar is the store spec §4.4's answer, and
//! it answers only for a work that has a sidecar. `import::scanned` leaves
//! `sidecar_path` NULL **by design** — one `taiyaku.json` per work directory
//! cannot describe fourteen volumes sitting beside each other
//! (`derive.rs:535`) — so a scanned work had nowhere at all to keep the part
//! nothing can re-derive. That is what this module gives it:
//!
//! ```text
//! <root>/.taiyaku/<content_key>.library.json
//! ```
//!
//! **Dotted, so the scanner never has to be taught to ignore it.**
//! `scan.rs:679` skips every entry whose name begins with `.`, and
//! `watch.rs`'s own probe file already relies on that. An exclusion list would
//! have been a second thing to keep in step with this one.
//!
//! **Inside a root**, because the claim being kept is the store spec §4.4's:
//! losing the database costs a rescan rather than costing the organisational
//! work. A file that lives with the machine rather than with the library does
//! not keep that claim.
//!
//! **Keyed by `content_key`**, per §4.1 — identity is content, path is a hint.
//! [`crate::identity::content_key`] hashes the hub edition's members alone,
//! and its own doc comment records that the hub is fixed when the work is
//! created *so that* adding an edition does not change a work's identity. So
//! this address survives every file being renamed, the library being remounted
//! somewhere else, and editions being added — which is the whole list of
//! things that actually happen to a library.

use std::path::PathBuf;

use rusqlite::{Connection, OptionalExtension};

/// The directory, inside a root, that holds documents taiyaku owns.
const DIR: &str = ".taiyaku";

/// Where this work's `<content_key>.library.json` belongs, or `None` when the
/// work has no member under any root.
///
/// **The hub edition's first member decides the root.** A `Group` collects
/// entries across every root, so "which root" has a single answer only at
/// member level — the same reasoning `member.root_id`'s own comment in
/// migration 6 gives for that column existing.
///
/// `None` is the ordinary answer for a work imported from a sidecar: it has a
/// `sidecar_path`, so `<stem>.library.json` beside it is already its home and
/// `export::work` already writes it.
///
/// # Errors
///
/// Returns an error if `work_id` names no work, or if SQLite fails.
pub fn path_for(conn: &Connection, work_id: i64) -> Result<Option<PathBuf>, String> {
    let content_key: String = conn
        .query_row(
            "SELECT content_key FROM work WHERE id = ?1",
            [work_id],
            |row| row.get(0),
        )
        .map_err(|e| format!("reading the content key of work {work_id}: {e}"))?;

    let root: Option<String> = conn
        .query_row(
            "SELECT root.path
               FROM member
               JOIN edition ON edition.id = member.edition_id
               JOIN work    ON work.id    = edition.work_id
               JOIN root    ON root.id    = member.root_id
              WHERE work.id = ?1 AND edition.key = work.hub
              ORDER BY member.ordinal
              LIMIT 1",
            [work_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| format!("reading the root of work {work_id}: {e}"))?;

    Ok(root.map(|root| {
        PathBuf::from(root)
            .join(DIR)
            .join(format!("{content_key}.library.json"))
    }))
}

#[cfg(test)]
mod tests;
```

- [ ] **Step 4: Register the module**

In `crates/taiyaku-library/src/lib.rs`, add `pub mod hand;` in the alphabetical
position the file already uses — between `fold` and `identity`.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p taiyaku-library hand::`
Expected: PASS, both tests.

- [ ] **Step 6: Commit**

```bash
git add crates/taiyaku-library/src/hand.rs crates/taiyaku-library/src/hand/tests.rs crates/taiyaku-library/src/lib.rs
git commit -m "hand.rs: where a scanned work's hand metadata lives on disk"
```

---

### Task 4: `hand::save` — write the document, arm the guard

**Files:**
- Modify: `crates/taiyaku-library/src/hand.rs`
- Test: `crates/taiyaku-library/src/hand/tests.rs`

**Interfaces:**
- Consumes: Task 3's `path_for`, Task 2's `LibraryDoc`.
- Produces: `hand::save(conn: &Connection, work_id: i64) -> Result<Saved, String>`
  and `pub struct Saved { pub path: Option<PathBuf>, pub persisted: bool }`.
  Task 6's PATCH handlers call it after every `librarydoc::apply`.

**The marker is the load-bearing half, not the file.** `derive.rs:1197` guards
`UPDATE work SET series_id = ?1, volume = ?2` — and `write_edition_metadata`'s
`edition.kind`/`edition.language` under the same `if` — behind
`!migrate::absorbed(conn, work_id, LIBRARY)`. Recording the marker is what
stops the next scan reverting the edit. So a root that cannot be written **must
not fail the edit**: the row is written, the marker is recorded, and
`persisted: false` reports that the document did not reach the disk. A library
on read-only media stays editable and is honestly labelled as not durable.

- [ ] **Step 1: Write the failing tests**

Append to `crates/taiyaku-library/src/hand/tests.rs`:

```rust
#[test]
fn saving_writes_the_document_and_records_the_marker() {
    let dir = tempfile::tempdir().expect("temp dir");
    let root = dir.path().to_string_lossy().to_string();
    let conn = Connection::open_in_memory().expect("in-memory database");
    let work_id = a_scanned_work_rooted_at(&conn, &root);

    let mut doc = crate::librarydoc::of(&conn, work_id).expect("projecting");
    doc.title = Some("Hand Set".to_string());
    crate::librarydoc::apply(&conn, work_id, &doc).expect("applying");

    let saved = super::save(&conn, work_id).expect("saving");

    assert!(saved.persisted, "a writable root takes the document");
    let path = saved.path.expect("a scanned work has an address");
    let text = std::fs::read_to_string(&path).expect("reading the document back");
    assert!(
        text.contains("Hand Set"),
        "asserted on the file rather than on the call, per C3 §8: {text}"
    );
    assert!(
        crate::migrate::absorbed(&conn, work_id, crate::migrate::LIBRARY)
            .expect("reading the marker"),
        "the marker is what arms `derive`'s guard; without it the next scan \
         reverts the edit"
    );
}

#[test]
fn a_root_that_cannot_be_written_still_arms_the_guard() {
    let dir = crate::tests::tempdir("hand-readonly");
    let root = dir.path.join("read-only");
    std::fs::create_dir(&root).expect("creating the root");
    let conn = Connection::open_in_memory().expect("in-memory database");
    let work_id = a_scanned_work_rooted_at(&conn, &root.to_string_lossy());

    // **Without this the test proves nothing**, and it passed without it while
    // the plan was wrong: an unedited work projects to the empty document, so
    // `save` takes the `doc.is_empty()` branch and tries to *remove* a file
    // that was never written. That exercises neither the write nor the
    // read-only refusal. Found by Task 4's implementer.
    let mut doc = crate::librarydoc::of(&conn, work_id).expect("projecting");
    doc.title = Some("Hand Set".to_string());
    crate::librarydoc::apply(&conn, work_id, &doc).expect("applying");

    let mut perms = std::fs::metadata(&root).expect("stat").permissions();
    perms.set_readonly(true);
    std::fs::set_permissions(&root, perms).expect("making the root read-only");

    let saved = super::save(&conn, work_id).expect("saving must not fail here");

    assert!(!saved.persisted, "the document did not reach the disk");
    assert!(
        crate::migrate::absorbed(&conn, work_id, crate::migrate::LIBRARY)
            .expect("reading the marker"),
        "an edit on read-only media is still an edit: the row is written and \
         the guard is armed, and only durability is lost"
    );
}
```

Add the fixture beside `a_scanned_work`, parameterised by root path:

```rust
fn a_scanned_work_rooted_at(conn: &Connection, root: &str) -> i64 {
    crate::schema::migrate(conn).expect("migrating");
    conn.execute(
        "INSERT INTO root (id, path) VALUES (1, ?1)",
        rusqlite::params![root],
    )
    .expect("seeding the root");
    conn.execute_batch(
        "INSERT INTO work (id, content_key, slug, title, reading, hub)
         VALUES (1, 'abc123', 'w', 'Derived', 'rtl', 'en');
         INSERT INTO edition (id, work_id, key, label) VALUES (1, 1, 'en', 'en');
         INSERT INTO member (id, edition_id, ordinal, path, sha256, pages, root_id)
         VALUES (1, 1, 0, 'v01.cbz', 'deadbeef', 100, 1);",
    )
    .expect("seeding");
    1
}
```

**Use `crate::tests::tempdir(label)`, not `tempfile`.** *This corrects a plan
error found by Task 4's implementer.* The plan originally said `tempfile` was
already a dev-dependency and to add it if not — it is neither, and adding it
would reverse a decision this crate has documented three times. `tests.rs:1-30`
hand-rolls a fifteen-line `TempDir` and says why: *"this workspace's dependency
list is short enough to quote in `docs/status.md`, and a `tempfile` dependency
would earn its place for about fifteen lines."* Its `Drop` uses
`let _ = remove_dir_all(...)`, so a read-only directory makes cleanup a silent
no-op rather than a panic — restore writable permissions at the end of the
read-only test anyway, or the directory leaks.

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p taiyaku-library hand::tests::saving_writes_the_document_and_records_the_marker`
Expected: FAIL — no function `save` in module `hand`.

- [ ] **Step 3: Write `save`**

Append to `crates/taiyaku-library/src/hand.rs`:

```rust
/// What [`save`] did.
///
/// `path` is `None` for a work with no address at all — an imported work,
/// whose document belongs beside its sidecar and is written by
/// [`crate::export::work`]. `persisted` is `false` when an address existed and
/// the write did not land, which is a read-only root and not an error: see
/// [`save`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Saved {
    pub path: Option<PathBuf>,
    pub persisted: bool,
    /// Why the document did not reach the disk, when it did not — the
    /// `io::Error`'s own message, naming the path.
    ///
    /// **A bool alone was the first shape and it was not honest enough.**
    /// `taiyaku_read::atomic::write` is built to distinguish a staging
    /// failure from a rename failure and to name the exact path (its O29
    /// comment); collapsing that to `false` threw away the only thing a
    /// caller could act on. Not failing the edit is what a read-only root
    /// requires — discarding the reason is not.
    pub why: Option<String>,
}

/// Project this work's hand-fixed metadata and write it to
/// [`path_for`]'s address, then record the `LIBRARY` marker.
///
/// **The marker is recorded whether or not the file lands, and that is
/// deliberate.** It is what `derive::write_work` consults before overwriting
/// `work.series_id`, `work.volume`, `edition.kind` and `edition.language`
/// (`derive.rs:1197`, I7's fix). An edit made against a read-only root is
/// still an edit the next scan must not revert; what is lost is durability,
/// and [`Saved::persisted`] is how a caller reports that rather than
/// discovering it later.
///
/// A document that projects empty removes the file instead of writing an empty
/// one, matching `export::work`'s own write-or-remove decision.
///
/// # Errors
///
/// Returns an error if `work_id` names no work, if projecting the document
/// fails, or if recording the marker fails. **Not** if the write fails: that
/// is `persisted: false`.
pub fn save(conn: &Connection, work_id: i64) -> Result<Saved, String> {
    let Some(path) = path_for(conn, work_id)? else {
        return Ok(Saved {
            path: None,
            persisted: false,
            why: None,
        });
    };

    let doc = crate::librarydoc::of(conn, work_id)?;
    let (persisted, why) = if doc.is_empty() {
        match std::fs::remove_file(&path) {
            Ok(()) => (true, None),
            // **Nothing there is the ordinary case, not a failure**, and it is
            // the common one: every work that has never been hand-edited takes
            // this branch. `export::work` draws the same line for the same
            // reason — "nothing there is the ordinary case […] and is not a
            // removal to report" — and reporting `persisted: false` for it
            // would tell a caller the library is not durable whenever there
            // was simply nothing to do.
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (true, None),
            Err(e) => (false, Some(format!("{}: {e}", path.display()))),
        }
    } else {
        // The parent is created here rather than at root registration: a root
        // that never holds a hand edit never grows a `.taiyaku` directory, and
        // a directory nothing writes into is a directory that reads as
        // unexplained the first time someone finds it in their library.
        let parent = path.parent().expect("an address always has a directory");
        match std::fs::create_dir_all(parent)
            .map_err(|e| format!("{}: {e}", parent.display()))
            .and_then(|()| {
                taiyaku_read::atomic::write(&path, doc.to_json().as_bytes(), "json")
            }) {
            Ok(()) => (true, None),
            Err(why) => (false, Some(why)),
        }
    };

    // **`!doc.is_empty()`, not `persisted`.** The marker answers "has this
    // library absorbed this document", and it has: the rows are written
    // whether or not the file reached the disk, and the guard must be armed
    // either way. Passing `persisted` here would leave a read-only root's edit
    // unprotected, which is the exact case two lines of doc comment above
    // promise it is not. The empty document is the one that records `false` —
    // there is nothing left to protect, so derivation should resume, which is
    // the same write-or-remove symmetry `export::work` keeps.
    crate::migrate::record_absorbed(
        conn,
        work_id,
        crate::migrate::LIBRARY,
        !doc.is_empty(),
    )?;

    Ok(Saved {
        path: Some(path),
        persisted,
        why,
    })
}
```

**`record_absorbed` is `pub(crate)` today** — this call is inside the same
crate, so nothing needs widening. `migrate::absorbed` is `pub(crate)` too and
the tests above call it from within the crate, which is likewise fine.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p taiyaku-library hand::`
Expected: PASS, all four.

- [ ] **Step 5: Verify the marker actually arms the guard**

Run: `cargo test -p taiyaku-library derive::`
Expected: PASS — no existing derive test regresses. This is the pass that would
catch `record_absorbed` being called with the wrong document constant.

- [ ] **Step 6: Commit**

```bash
git add crates/taiyaku-library/src/hand.rs crates/taiyaku-library/src/hand/tests.rs
git commit -m "hand::save writes the document and arms derive's guard, read-only roots included"
```

---

### Task 5: `hand::load`, and the read-back that makes a rescan recover the work

**Files:**
- Modify: `crates/taiyaku-library/src/hand.rs`, `crates/taiyaku-library/src/derive.rs:1197`
- Test: `crates/taiyaku-library/src/hand/tests.rs`, `crates/taiyaku-library/src/derive/tests.rs`

**Interfaces:**
- Consumes: Task 3's `path_for`, Task 2's `LibraryDoc::from_json`.
- Produces: `hand::load(conn: &Connection, work_id: i64) -> Result<Option<LibraryDoc>, String>`.

**This is the arm O53 says is missing.** That item records that
`migrate::work`'s `Step::Apply` is unreachable for any work a scan has touched,
because `librarydoc::of` reads non-empty the moment a work has a derived volume
or series. This path does not go through `Step::Apply` at all — and O53's own
entry predicted exactly this shape: *"The I7 fix works through the marker
`export::work` sets, a real but different path."*

- [ ] **Step 1: Write the failing test**

Append to `crates/taiyaku-library/src/hand/tests.rs`:

```rust
#[test]
fn a_rescan_with_no_database_recovers_the_hand_set_title() {
    let dir = tempfile::tempdir().expect("temp dir");
    let root = dir.path().to_string_lossy().to_string();

    // First database: a person corrects the title, and it is saved.
    let first = Connection::open_in_memory().expect("in-memory database");
    let work_id = a_scanned_work_rooted_at(&first, &root);
    let mut doc = crate::librarydoc::of(&first, work_id).expect("projecting");
    doc.title = Some("Go-Toubun no Hanayome, volume 1".to_string());
    crate::librarydoc::apply(&first, work_id, &doc).expect("applying");
    super::save(&first, work_id).expect("saving");
    drop(first);

    // The database is lost. A second one is built from the same root, and the
    // work is minted afresh with the title derivation gives it.
    let second = Connection::open_in_memory().expect("in-memory database");
    let work_id = a_scanned_work_rooted_at(&second, &root);

    let found = super::load(&second, work_id)
        .expect("loading")
        .expect("the document is still beside the library");

    assert_eq!(
        found.title.as_deref(),
        Some("Go-Toubun no Hanayome, volume 1"),
        "losing the database must cost a rescan, not the organisational work \
         (store spec §4.4)"
    );
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p taiyaku-library hand::tests::a_rescan_with_no_database_recovers_the_hand_set_title`
Expected: FAIL — no function `load` in module `hand`.

- [ ] **Step 3: Write `load`**

Append to `crates/taiyaku-library/src/hand.rs`:

```rust
/// Read the document at this work's address, if there is one there.
///
/// `Ok(None)` for a work with no address and for an address with no file —
/// both are the ordinary case and neither is a failure. A file that exists and
/// cannot be read or parsed **is** an error: it is somebody's organisational
/// work, and silently deriving over it is the outcome this whole module exists
/// to prevent.
///
/// # Errors
///
/// Returns an error if `work_id` names no work, if the file exists and cannot
/// be read, or if it is not a readable library document.
pub fn load(conn: &Connection, work_id: i64) -> Result<Option<crate::librarydoc::LibraryDoc>, String> {
    let Some(path) = path_for(conn, work_id)? else {
        return Ok(None);
    };
    if !path.is_file() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    crate::librarydoc::LibraryDoc::from_json(&text)
        .map(Some)
        .map_err(|e| format!("{}: {e}", path.display()))
}
```

- [ ] **Step 4: Write the failing derive test**

In `crates/taiyaku-library/src/derive/tests.rs`, following that file's existing
fixture convention for a root walked into a work:

```rust
#[test]
fn a_hand_document_beside_the_library_survives_the_next_derive() {
    // The scenario O53 and I7 are both about: the database is fresh, the work
    // is minted by derivation, and the person's correction is on disk beside
    // the archives rather than in any row.
    let (dir, conn) = a_root_with_one_volume();
    let work_id = derive(&conn).expect("first derive");

    let mut doc = crate::librarydoc::of(&conn, work_id).expect("projecting");
    doc.volume = Some(4.5);
    crate::librarydoc::apply(&conn, work_id, &doc).expect("applying");
    crate::hand::save(&conn, work_id).expect("saving");

    derive(&conn).expect("second derive");

    let volume: Option<f64> = conn
        .query_row("SELECT volume FROM work WHERE id = ?1", [work_id], |r| {
            r.get(0)
        })
        .expect("reading volume");
    assert_eq!(
        volume,
        Some(4.5),
        "`derive.rs`'s own comment: a hand-set `volume = 4.5` came back as \
         `1.0` after one derive before the I7 guard"
    );
    drop(dir);
}
```

Reuse whatever helper `derive/tests.rs` already has in place of
`a_root_with_one_volume()` and `derive()`; do not add a second fixture that
walks a root if one exists.

- [ ] **Step 5: Run it — it should already pass**

Run: `cargo test -p taiyaku-library derive::tests::a_hand_document_beside_the_library_survives_the_next_derive`
Expected: PASS. The guard has been in place since #77; this test pins it, and
is the regression net for Step 6, which changes the code around it.

- [ ] **Step 6: Wire the read-back into `derive::write_work`**

In `crates/taiyaku-library/src/derive.rs`, immediately **before** the existing
`if !crate::migrate::absorbed(...)` on line 1197:

```rust
    // **The document on disk, read back before the guard is consulted.** A
    // fresh database has no marker for a work it has just minted, so without
    // this the guard below is open and derivation writes over a correction
    // that is sitting right beside the archives. `migrate::work`'s
    // `Step::Apply` cannot do this job — O53: `librarydoc::of` reads non-empty
    // the moment a work has a derived volume or series, so that step is
    // unreachable for any work a scan has touched. This is the "real but
    // different path" O53's own entry names.
    if !crate::migrate::absorbed(conn, work_id, crate::migrate::LIBRARY)? {
        match crate::hand::load(conn, work_id) {
            Ok(Some((path, doc))) => {
                let empty = doc.is_empty();
                match crate::librarydoc::apply(conn, work_id, &doc)
                    .map_err(|e| format!("{}: {e}", path.display()))
                {
                    // `!empty`, matching `hand::save`'s own rule: a
                    // hand-emptied document has nothing left to protect and
                    // derivation should resume. Arming the marker for it would
                    // freeze the cleared state permanently, since nothing calls
                    // `save` on this work again.
                    Ok(()) => crate::migrate::record_absorbed(
                        conn,
                        work_id,
                        crate::migrate::LIBRARY,
                        !empty,
                    )?,
                    Err(why) => report.unreadable_hand.push(why),
                }
            }
            Ok(None) => {}
            // **Reported, not raised.** `write_work` is called with `?` inside
            // the group loop and `derive` holds no transaction of its own, so a
            // `?` here would leave the library half-derived — works before this
            // one written, works after it untouched, and `superseded`/
            // `fold::absorb` and the proposals pass never reached — because one
            // file in someone's library would not parse. This module states the
            // opposite principle three times: *"one unmounted share or one
            // truncated download must not leave a whole library underived"*,
            // *"a scan that died because one work of thousands could not be
            // folded would be worse than a scan that folds nothing"*, and
            // `scan.rs`'s *"one unreadable file must not leave a library
            // unscanned"*. On a fresh database every work is marker-less, so
            // every document is read on that one pass — exactly when a single
            // bad file is most likely to be met.
            Err(why) => report.unreadable_hand.push(why),
        }
    }
```

Add the field to `DeriveReport`, beside `unfoldable`, which is the same shape
for the same reason:

```rust
    /// Hand documents that could not be read, parsed, or applied, each with
    /// the path and the reason.
    ///
    /// A `Vec` rather than a counter — `unfoldable`'s choice — because the
    /// person has to be told *which* file to fix.
    ///
    /// **The path is prepended by the read-back, not inherited from the
    /// error.** `hand::load`'s own messages carry it, but
    /// `librarydoc::apply`'s name a `work_id` and a `series_id` and never a
    /// file — so an apply failure reported verbatim would tell a person a
    /// document is broken without saying which one. That is the exact gap
    /// this field exists to close, so `load` hands the path back and the
    /// read-back formats both arms the same way.
    pub unreadable_hand: Vec<String>,
```

**`load` returns the path alongside the document**, for that reason — the
signature is `Result<Option<(PathBuf, LibraryDoc)>, String>`. Prepending
`"{path}: "` at the report site would work too, but it would mean re-deriving
the address after `load` already knew it, and a message's honesty would then
depend on two call sites agreeing.

- [ ] **Step 6b: The test that actually reaches the read-back**

**Step 4's test does not exercise this code and never did.** It calls
`hand::save`, which arms the marker, so `!absorbed` is `false`, the `&&`
short-circuits, and `hand::load` is never called — deleting the whole read-back
block leaves that test green. It pins the #77 guard, which is worth pinning, but
it is not evidence for anything this task adds. Found by Task 5's review.

The distinguishing condition is **a document on disk with no marker beside it**,
which is what a database rebuilt from scratch looks like. Reuse the same fixture
Step 4 used:

```rust
#[test]
fn a_derive_applies_a_hand_document_this_database_has_never_seen() {
    // The O53 case: the correction is beside the archives, and this database
    // has no record of ever having read it. `migrate::work`'s `Step::Apply`
    // cannot reach it — `librarydoc::of` reads non-empty the moment a work has
    // a derived volume — so if the read-back does not apply it, nothing does.
    let (dir, conn) = a_root_with_one_volume();
    let work_id = derive(&conn).expect("first derive");

    let mut doc = crate::librarydoc::of(&conn, work_id).expect("projecting");
    doc.volume = Some(4.5);
    crate::librarydoc::apply(&conn, work_id, &doc).expect("applying");
    crate::hand::save(&conn, work_id).expect("saving");

    // What a fresh database has: the file, and no memory of it.
    conn.execute("DELETE FROM migrated_document", [])
        .expect("clearing the markers");
    conn.execute("UPDATE work SET volume = 1.0 WHERE id = ?1", [work_id])
        .expect("returning the row to its derived state");

    derive(&conn).expect("second derive");

    let volume: Option<f64> = conn
        .query_row("SELECT volume FROM work WHERE id = ?1", [work_id], |r| {
            r.get(0)
        })
        .expect("reading volume");
    assert_eq!(
        volume,
        Some(4.5),
        "a hand document this database has never seen must be applied by the \
         read-back; without it the guard is open and derivation writes over a \
         correction sitting beside the archives"
    );
    drop(dir);
}
```

**Prove it discriminates by deleting the read-back block**, not by removing
`hand::save` — removing `save` also removes the file, so `load` returns `None`
and the test would go red for the guard's reason rather than this one. Comment
out `derive.rs`'s read-back, watch this test fail, restore it. Record both.

- [ ] **Step 6b2: A malformed document must not stop the scan, proven**

The previous round's whole point — a user's file cannot abort a derive — is
reached by no test. `derive.rs`'s two `report.unreadable_hand.push(why)` arms
have zero coverage; the only test naming the field builds a `DeriveReport` by
hand and calls `derive_summary` directly, which exercises string formatting and
nothing else.

Write the test that drives it. **Two works under one root**, so the assertion
that the scan continued is a statement about a second work rather than about a
counter:

```rust
#[test]
fn a_hand_document_that_will_not_parse_is_reported_and_the_scan_goes_on() {
    let (dir, conn) = a_root_with_two_volumes();
    derive(&conn).expect("first derive");

    // A hand document at the first work's own address, holding text that is
    // not a library document at all.
    let path = crate::hand::path_for(&conn, first_work_id)
        .expect("resolving")
        .expect("a scanned work has an address");
    std::fs::create_dir_all(path.parent().expect("a directory")).expect("mkdir");
    std::fs::write(&path, b"{ not a document").expect("writing the bad file");

    conn.execute("DELETE FROM migrated_document", [])
        .expect("clearing the markers");

    let report = derive(&conn).expect(
        "one unparseable file must not fail the derive — `write_work` is          called with `?` inside the group loop, so raising here would leave          every later work underived",
    );

    assert_eq!(report.unreadable_hand.len(), 1);
    assert!(
        report.unreadable_hand[0].contains(&path.display().to_string()),
        "the message must name the file to fix: {:?}",
        report.unreadable_hand
    );
    assert_eq!(
        report.works, 2,
        "the second work must still have been derived"
    );
    drop(dir);
}
```

Adapt the fixture and the report field names to what `derive/tests.rs` actually
provides — if no two-volume fixture exists, build the second volume rather than
asserting on a counter alone, because a count that only ever sees one work
cannot distinguish "the scan continued" from "the scan stopped after the one
work it had".

**Watch it fail** against a `?` in place of the report arm, which is what the
previous round removed. That is the mutation it exists to catch.

If an apply-failure case is cheap to reach as well — a document naming an
edition key the work does not have, which `apply_editions` refuses — add it as
a second case, since that is the arm whose message needed the path prepended.

- [ ] **Step 6c: Three comments the read-back falsifies**

- **`derive.rs`'s argument block** still says the marker *"is `true` only once
  `LibraryOutcome::Applied` has actually landed, never merely because a
  `.library.json` file happens to sit on disk."* The read-back sets it for
  exactly that reason. That sentence was load-bearing — it is why the guard
  could be called narrow — so retract it in place rather than leaving it to be
  read as still true.
- The same block's *"`librarydoc::apply` — reached through `migrate::work`,
  §4.4"* now names one of two routes as the route.
- **`write_work`'s own summary** says it writes only *"the columns the document
  cannot carry — `work.series_id`, `work.volume` and, through `write_chapters`,
  `member.chapter_declared`."* It now also applies a whole `LibraryDoc`: title,
  series row and titles, external ids, edition metadata, member chapters, tags.
- **`hand.rs`** points at "`derive.rs:1197`, I7's fix"; the guard has moved.
  Name the function rather than a line number, which drifts.

- [ ] **Step 7: Run the whole crate's tests**

Run: `cargo test -p taiyaku-library`
Expected: PASS. Watch `derive::` and `migrate::` in particular — this adds a
writer to a function that already had a careful ordering argument in its
comments.

- [ ] **Step 8: Commit**

```bash
git add crates/taiyaku-library/src/hand.rs crates/taiyaku-library/src/hand/tests.rs crates/taiyaku-library/src/derive.rs crates/taiyaku-library/src/derive/tests.rs
git commit -m "derive reads back the hand document before its guard, closing O53's unreachable arm"
```

---

### Task 6: The four PATCH routes

**Files:**
- Create: `crates/taiyaku-serve/src/metadata.rs`, `crates/taiyaku-serve/src/metadata/tests.rs`
- Modify: `crates/taiyaku-serve/src/lib.rs`, `crates/taiyaku-serve/src/routes.rs`
- Test: `crates/taiyaku-serve/src/metadata/tests.rs`, `crates/taiyaku-serve/src/auth/tests.rs`

**Interfaces:**
- Consumes: `librarydoc::{of, apply}`, `hand::save`.
- Produces: `PATCH /api/library/works/{slug}`, `/api/library/series/{id}`,
  `/api/library/editions/{id}`, `/api/library/members/{id}`, each answering
  `200` with the work's re-projected `LibraryDoc` plus a `persisted` boolean.

**This is what closes O44.** `librarydoc::apply` has existed with no caller
outside `migrate::apply_library` since the store shipped; these routes are the
callers. Every one of them does the same three things in the same order —
apply, save, re-project — which is why they share `edit_work` below rather than
each growing their own copy.

**Series is the one that touches more than one work.** `series`, `series_title`
and `external_id` are shared by every work in the series, and every one of
those works' documents carries the series block, so a series edit must re-save
each of them. `librarydoc.rs`'s own module comment is the reason this is not
optional: those tables are upserted rather than replaced precisely because one
work's document must not erase a sibling's contribution.

- [ ] **Step 1: Write the failing tests**

Create `crates/taiyaku-serve/src/metadata/tests.rs`, following
`curation/tests.rs`'s harness exactly — same `test_support` server fixture, same
admin and reader principals:

```rust
#[tokio::test]
async fn every_metadata_route_refuses_a_reader() {
    let server = a_server_with_one_scanned_work().await;
    for (method, path) in [
        (Method::PATCH, "/api/library/works/w"),
        (Method::PATCH, "/api/library/series/1"),
        (Method::PATCH, "/api/library/editions/1"),
        (Method::PATCH, "/api/library/members/1"),
    ] {
        let response = server.as_reader(method, path, json!({})).await;
        assert_eq!(
            response.status(),
            StatusCode::FORBIDDEN,
            "{path}: the credential is valid and the caller is who they say, \
             which is exactly why a 401 would be the wrong answer"
        );
    }
}

#[tokio::test]
async fn patching_a_title_writes_the_row_and_the_document() {
    let server = a_server_with_one_scanned_work().await;

    let response = server
        .as_admin(
            Method::PATCH,
            "/api/library/works/w",
            json!({ "title": "Go-Toubun no Hanayome, volume 1" }),
        )
        .await;

    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value = server.json(response).await;
    assert_eq!(body["title"], "Go-Toubun no Hanayome, volume 1");
    assert_eq!(body["persisted"], true);

    let text = std::fs::read_to_string(server.hand_document_path())
        .expect("the document is beside the library");
    assert!(
        text.contains("Go-Toubun no Hanayome, volume 1"),
        "C3 §8 asks for this asserted on the file rather than on the call: {text}"
    );
}

#[tokio::test]
async fn a_hand_set_chapter_number_is_recorded_as_hand() {
    let server = a_server_with_one_scanned_work().await;

    let response = server
        .as_admin(Method::PATCH, "/api/library/members/1", json!({ "chapter": 12.5 }))
        .await;

    assert_eq!(response.status(), StatusCode::OK);
    let body: serde_json::Value = server.json(response).await;
    assert_eq!(body["members"][0]["chapter"], 12.5);
    assert_eq!(
        body["members"][0]["chapter_source"], "hand",
        "C3 §5: a hand-set chapter number is a different kind of claim from a \
         parsed one, and spec G's sync will need to know which it has. The \
         client cannot choose this value"
    );
}

#[tokio::test]
async fn editing_one_work_in_a_series_saves_every_works_document() {
    let server = a_server_with_two_works_in_one_series().await;

    let response = server
        .as_admin(
            Method::PATCH,
            "/api/library/series/1",
            json!({ "titles": [{ "title": "五等分の花嫁", "kind": "native", "primary": true }] }),
        )
        .await;

    assert_eq!(response.status(), StatusCode::OK);
    for path in server.hand_document_paths() {
        let text = std::fs::read_to_string(&path).expect("each work keeps its own document");
        assert!(
            text.contains("五等分の花嫁"),
            "a series block lives in every member work's document; saving one \
             would leave the others naming the old titles, and the first \
             rescan to read one back would undo the edit: {}",
            path.display()
        );
    }
}

#[tokio::test]
async fn patching_an_unknown_work_is_a_404() {
    let server = a_server_with_one_scanned_work().await;
    let response = server
        .as_admin(Method::PATCH, "/api/library/works/nope", json!({ "title": "x" }))
        .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p taiyaku-serve metadata::`
Expected: FAIL — unresolved module `metadata`.

- [ ] **Step 3: Write the module**

Create `crates/taiyaku-serve/src/metadata.rs`. It follows `curation.rs`'s shape
exactly: a `NoAnswer` enum, `admin_or_forbidden`, `spawn_blocking` around every
SQLite call, `panicked` for a join failure. The one piece that is this module's
own:

```rust
/// Apply an edit to one work's document and persist it, answering the
/// re-projected document.
///
/// **Three steps in this order, on every route here.** Apply writes the rows;
/// [`taiyaku_library::hand::save`] writes `<root>/.taiyaku/<key>.library.json`
/// *and records the `LIBRARY` marker*, which is what stops the next scan
/// reverting the edit (`derive.rs:1197`); re-projecting is what the client is
/// answered with, so it sees what the store actually holds rather than what it
/// sent.
///
/// `persisted` is reported rather than enforced: a read-only root cannot take
/// the document, and refusing the edit for that would make a library on
/// read-only media uneditable. The row is written and the guard is armed
/// either way — only durability is lost, and the client is told so.
fn edit_work(
    conn: &rusqlite::Connection,
    work_id: i64,
    edit: impl FnOnce(&mut LibraryDoc),
) -> Result<serde_json::Value, NoAnswer> {
    let mut doc = librarydoc::of(conn, work_id).map_err(NoAnswer::Refused)?;
    edit(&mut doc);
    librarydoc::apply(conn, work_id, &doc).map_err(NoAnswer::Refused)?;
    let saved = hand::save(conn, work_id).map_err(NoAnswer::Refused)?;
    let doc = librarydoc::of(conn, work_id).map_err(NoAnswer::Refused)?;

    let mut body = serde_json::to_value(&doc).map_err(|e| NoAnswer::Refused(e.to_string()))?;
    body["persisted"] = serde_json::Value::Bool(saved.persisted);
    // `why` too, and not only `persisted`. Task 4's review is the reason the
    // field exists: a caller told "not durable" with no reason cannot act on
    // it, and `atomic::write` goes to trouble to say whether it was staging or
    // renaming that failed, and where. Dropping it here would make that work
    // reach nobody.
    if let Some(why) = saved.why {
        body["why"] = serde_json::Value::String(why);
    }
    Ok(body)
}
```

The four handlers, each resolving its own id to the work or works whose
document carries the field, then delegating:

```rust
/// `PATCH /api/library/works/{slug}` — volume, title, series membership.
///
/// All three of C3 §5's work-level fields. Series membership is here rather
/// than deferred: see the build design §7's second correction — `derive` has
/// guarded these columns behind the `LIBRARY` marker since #77, and
/// [`edit_work`] records that marker.
#[derive(serde::Deserialize)]
struct WorkEdit {
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    volume: Option<f64>,
    #[serde(default)]
    series: Option<librarydoc::SeriesInfo>,
}

pub(crate) async fn patch_work(
    State(state): State<Arc<ServeState>>,
    Extension(principal): Extension<Principal>,
    AxPath(slug): AxPath<String>,
    Json(edit): Json<WorkEdit>,
) -> Response {
    if let Some(refusal) = admin_or_forbidden(&principal) {
        return refusal;
    }
    let edited = tokio::task::spawn_blocking(move || {
        let conn = state.library.conn();
        let work_id = work_by_slug(&conn, &slug)?;
        edit_work(&conn, work_id, |doc| {
            if let Some(title) = edit.title {
                doc.title = Some(title);
            }
            if let Some(volume) = edit.volume {
                doc.volume = Some(volume);
            }
            if let Some(series) = edit.series {
                doc.series = series;
            }
        })
    })
    .await;

    match edited {
        Ok(Ok(body)) => Json(body).into_response(),
        Ok(Err(no_answer)) => no_answer.into_response(),
        Err(why) => panicked("editing a work", &why),
    }
}

/// `PATCH /api/library/editions/{id}` — label, kind, language, credits.
#[derive(serde::Deserialize)]
struct EditionEdit {
    #[serde(default)]
    label: Option<String>,
    #[serde(default)]
    language: Option<String>,
    #[serde(default)]
    kind: Option<librarydoc::EditionKind>,
    #[serde(default)]
    credits: Option<Vec<librarydoc::Credit>>,
}

pub(crate) async fn patch_edition(
    State(state): State<Arc<ServeState>>,
    Extension(principal): Extension<Principal>,
    AxPath(id): AxPath<i64>,
    Json(edit): Json<EditionEdit>,
) -> Response {
    if let Some(refusal) = admin_or_forbidden(&principal) {
        return refusal;
    }
    let edited = tokio::task::spawn_blocking(move || {
        let conn = state.library.conn();
        let (work_id, key) = edition_owner(&conn, id)?;
        edit_work(&conn, work_id, |doc| {
            let meta = doc.editions.entry(key).or_default();
            if let Some(label) = edit.label {
                meta.label = Some(label);
            }
            if let Some(language) = edit.language {
                meta.language = Some(language);
            }
            if let Some(kind) = edit.kind {
                meta.kind = Some(kind);
            }
            if let Some(credits) = edit.credits {
                meta.credits = credits;
            }
        })
    })
    .await;

    match edited {
        Ok(Ok(body)) => Json(body).into_response(),
        Ok(Err(no_answer)) => no_answer.into_response(),
        Err(why) => panicked("editing an edition", &why),
    }
}

/// `PATCH /api/library/members/{id}` — a hand-set chapter number.
///
/// **`chapter_source` is forced to `Hand` and is not a field of the body.**
/// C3 §5: *"a hand-set chapter number is a different kind of claim from a
/// parsed one"* — `chapter_at` refuses to infer one from a filename because
/// two corpus series have declared names that are wrong
/// (`core/sidecar.rs:477`), and spec G's AniList sync will need to know which
/// it has. A client that could send `chapter_source: "comicinfo"` alongside a
/// number a person typed would be able to launder the one into the other.
#[derive(serde::Deserialize)]
struct MemberEdit {
    chapter: Option<f64>,
}

pub(crate) async fn patch_member(
    State(state): State<Arc<ServeState>>,
    Extension(principal): Extension<Principal>,
    AxPath(id): AxPath<i64>,
    Json(edit): Json<MemberEdit>,
) -> Response {
    if let Some(refusal) = admin_or_forbidden(&principal) {
        return refusal;
    }
    let edited = tokio::task::spawn_blocking(move || {
        let conn = state.library.conn();
        let (work_id, sha256) = member_owner(&conn, id)?;
        edit_work(&conn, work_id, |doc| {
            doc.members.retain(|m| m.sha256 != sha256);
            doc.members.push(librarydoc::MemberMeta {
                sha256,
                chapter: edit.chapter,
                chapter_source: edit.chapter.map(|_| librarydoc::ChapterSource::Hand),
            });
        })
    })
    .await;

    match edited {
        Ok(Ok(body)) => Json(body).into_response(),
        Ok(Err(no_answer)) => no_answer.into_response(),
        Err(why) => panicked("editing a member", &why),
    }
}
```

`patch_series` is the one that is not a single `edit_work`. `series`,
`series_title` and `external_id` are shared by every work in the series, and
every one of those works' documents carries the series block, so it resolves
the whole membership and edits each in turn:

```rust
/// `PATCH /api/library/series/{id}` — titles and their kind, which is
/// primary, reading direction, tags, external ids.
///
/// **Every work in the series, not one.** Those tables are shared, and
/// `librarydoc.rs`'s module comment is why that matters: they are upserted
/// rather than replaced precisely so one work's document cannot erase a
/// sibling's contribution. Saving only one work's document would leave every
/// other work in the series holding a document that still names the old
/// titles, and the first rescan to read one of them back would undo the edit.
pub(crate) async fn patch_series(
    State(state): State<Arc<ServeState>>,
    Extension(principal): Extension<Principal>,
    AxPath(id): AxPath<i64>,
    Json(edit): Json<SeriesEdit>,
) -> Response {
    if let Some(refusal) = admin_or_forbidden(&principal) {
        return refusal;
    }
    let edited = tokio::task::spawn_blocking(move || {
        let conn = state.library.conn();
        let works = works_in_series(&conn, id)?;
        if works.is_empty() {
            return Err(NoAnswer::Missing);
        }
        let mut last = serde_json::Value::Null;
        for work_id in works {
            last = edit_work(&conn, work_id, |doc| apply_series_edit(doc, &edit))?;
        }
        Ok(last)
    })
    .await;

    match edited {
        Ok(Ok(body)) => Json(body).into_response(),
        Ok(Err(no_answer)) => no_answer.into_response(),
        Err(why) => panicked("editing a series", &why),
    }
}
```

Write the four small resolvers — `work_by_slug`, `edition_owner`,
`member_owner`, `works_in_series` — as `query_row`/`query_map` calls returning
`NoAnswer::Missing` for an id that names nothing, matching `curation.rs`'s
`present`. `SeriesEdit` carries `titles`, `reading`, `tags` and `external`,
each `Option`, and `apply_series_edit` writes each present field onto the
document's `series`, `tags` and `external` in turn.

- [ ] **Step 4: Register the routes**

In `crates/taiyaku-serve/src/lib.rs` add `mod metadata;`.

In `crates/taiyaku-serve/src/routes.rs`, add to `guarded_routes()` beside the
curation block:

```rust
        (
            "/api/library/works/{slug}",
            axum::routing::patch(crate::metadata::patch_work),
        ),
        (
            "/api/library/series/{id}",
            axum::routing::patch(crate::metadata::patch_series),
        ),
        (
            "/api/library/editions/{id}",
            axum::routing::patch(crate::metadata::patch_edition),
        ),
        (
            "/api/library/members/{id}",
            axum::routing::patch(crate::metadata::patch_member),
        ),
```

and four matching rows in `grant_postures()`, in the same order. All four are
admin-only writes about the library's organisation rather than about what a
reader may see, which is the same posture the curation routes carry — copy the
reason wording from the curation rows rather than inventing a second phrasing.

Add the four paths to the typed list
`auth::tests::every_route_refuses_an_anonymous_request` drives.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p taiyaku-serve metadata:: auth::tests::every_route_declares_its_grant_posture auth::tests::every_route_refuses_an_anonymous_request`
Expected: PASS. The posture test is the one that fails loudly if a route was
registered without a row.

- [ ] **Step 6: Commit**

```bash
git add crates/taiyaku-serve/src/metadata.rs crates/taiyaku-serve/src/metadata/tests.rs crates/taiyaku-serve/src/lib.rs crates/taiyaku-serve/src/routes.rs crates/taiyaku-serve/src/auth/tests.rs
git commit -m "The four metadata PATCH routes, closing O44"
```

---

### Task 6b: What the routes promised and did not hold

*Added 2026-08-28 from Task 6's review. Five findings, four of them about the
same thing: a route that reports success while the edit does not survive.*

- [ ] **Step 1: `derive` must not overwrite a chapter it did not set**

`write_chapters`' **clear** is scoped to the scanner's own rows —
`WHERE chapter_source = 'filename'` — which is what `SCANNER_CHAPTER_SOURCE`'s
doc comment means when it says a hand value is out of reach. **Its write is
not.** The per-member `UPDATE` stamps `chapter_declared` and
`chapter_source = 'filename'` for every candidate whose name yields a number,
with no predicate at all — so a hand-set chapter on a member whose filename
parses is reverted on the next rescan, silently. `write_chapters` also runs
*outside* the `LIBRARY` guard (`derive.rs`), so the marker does not cover it.

That makes `PATCH /api/library/members/{id}` — whose whole purpose is
correcting a wrongly-parsed chapter — a route that reports 200 and loses the
edit at the next scan. **`builtin-chapter` is a seeded default rule**
(`schema/003_scanner.sql`), so the parse succeeds for exactly the members
people want to correct.

Scope the write the same way the clear already is:

```rust
                conn.execute(
                    "UPDATE member SET chapter_declared = ?1, chapter_source = ?2
                      WHERE ordinal = ?3
                        AND edition_id = (SELECT id FROM edition
                                           WHERE work_id = ?4 AND key = ?5)
                        AND (chapter_source IS NULL OR chapter_source = ?2)",
                    rusqlite::params![chapter, SCANNER_CHAPTER_SOURCE, ordinal, work_id, key],
                )
```

**This also fixes a `comicinfo` chapter**, which survives the clear today and
is then overwritten by the write — the same defect, pre-existing from the
scanner branch, and reachable without any of this branch's work.

Two tests, each watched failing: a hand-set chapter survives a re-derive, and
a `comicinfo` chapter does too.

- [ ] **Step 2: an absent `chapter` must not erase one**

`patch_member` drops the member's entry and pushes a fresh one unconditionally,
so `PATCH` with `{}` sets `chapter_declared` and `chapter_source` to NULL —
destroying a `comicinfo` or hand value with an empty body. The other three
handlers use `if let Some(...)` and leave an absent field alone, and
`SeriesEdit`'s own doc states that "send only what it changed" contract for the
module. Make `MemberEdit` honour it: leave the member's entry untouched when
`chapter` is absent, and say so in its doc comment.

Note that `every_metadata_route_refuses_a_reader` sends `{}` to this route —
harmless as a reader, destructive as an admin, which is what makes this worth a
test of its own.

- [ ] **Step 3: `patch_series` must answer for the series, not for its last work**

`persisted` and `why` are per work, and the loop keeps only the last
iteration's body. A series whose works span two roots — a shape this codebase
models in `auth/tests.rs` — can have the first work's document fail to land
while the answer says `persisted: true` with no `why`. Answer the *worst* case:
`persisted` false if any work's document did not land, and `why` carrying the
reasons that did occur. Task 4's review is the reason `why` exists; dropping
N−1 of them re-creates the finding it closed.

- [ ] **Step 4: `patch_work`'s `series` field reaches shared rows through a second door**

`apply_series` writes `UPDATE series SET sort_title = ?1, reading = ?2` and
upserts `series_title` on the **shared** rows, and `patch_work` then saves only
this work's document — leaving every sibling's document naming the old titles,
which is the exact condition `patch_series` exists to prevent. Give both
handlers one helper that saves this work's document and every sibling in the
series it now belongs to, and in the one it left. A test must cover the
`patch_work` route specifically, since that is the door with no coverage.

- [ ] **Step 5: `patch_edition` has no success-path test**

Editions appear only in the 403 loop. `doc.editions.entry(key).or_default()`
and the `EditionKind`/`Credit` body parsing are executed by nothing. One of the
four shipped routes is unproven — add the success test, asserting the label,
kind, language and a credit round-trip and that the document on disk carries
them.

- [ ] **Step 6: Two doc comments that are false**

- `metadata.rs` twice cites *"`chapter_at` refuses to infer one from a filename
  … (`core/sidecar.rs:477`)"*. That line is `NotCorrected::Unreadable`;
  `chapter_at` is elsewhere and says no such thing; and the scanner **does**
  infer chapters from filenames. The real justification is at `derive.rs`'s
  `SCANNER_CHAPTER_SOURCE` and is stronger. This wording came from the plan and
  is a shipped falsehood.
- `edit_work` attributes `persisted: false` to a read-only root. `hand::save`
  also answers `persisted: false, why: None` for a work with **no address at
  all** — every sidecar-imported work — so those callers get "not durable" with
  no reason, the outcome `why` exists to prevent. Say so, and consider whether
  that case wants a `why` of its own.

- [ ] **Step 7: Run the gate for both crates**

Run: `cargo test --workspace`, `cargo fmt --check`,
`cargo clippy --workspace --all-targets -- -D warnings`.

---

### Task 6c: What the fix round left

*Added 2026-08-28 from the re-review of Task 6b.*

- [ ] **Step 1: `why` is one shape on every route**

`patch_work` answers `why` as a **string** when `series` is absent and as an
**array** when it is present — the same route, two types, decided by the
request body. `patch_series` is always an array; `patch_edition` and
`patch_member` always a string. Tasks 8 and 9 build the client that consumes
this, and a client that does not special-case per-request hits a runtime type
error on one route only.

**Make `why` an array everywhere**, including the two single-document routes,
where it is an array of nought or one. A uniform shape costs the client one
`.length` check and costs the server nothing. State the shape in each handler's
own doc comment, not only in a private helper's.

- [ ] **Step 2: say what a half-finished fan-out leaves behind**

The brief allowed a savepoint *or* stated retry semantics, and neither landed —
the fix report claimed the semantics were "stated in the code comment on
`patch_work`" and they are not. A real `Err` from `hand::save` partway through
the fan-out — `record_absorbed` failing, say — aborts the request through
`NoAnswer::Refused` with the siblings already saved left saved, and nothing
says so.

A savepoint is the better fix if it is reachable: `librarydoc::apply` opens its
own `unchecked_transaction`, so an outer `BEGIN` fails, but `SAVEPOINT` nests
where `BEGIN` does not. **Try it first.** If it works, the fan-out becomes one
unit and the comment says that. If it does not, write the semantics down where
the loop is: which writes survive an abort, that a retry converges, and that
the response's `persisted` describes only the works reached before the failure.

- [ ] **Step 3: a correction that reversed the truth**

`metadata.rs` now says `SCANNER_CHAPTER_SOURCE` *"is the one value a re-derive
is scoped never to overwrite."* It is the exact opposite: `'filename'` is the
value the clear and the write both own and rewrite on every pass, and `'hand'`
and `'comicinfo'` are the protected ones. `derive.rs`'s own rewritten comment
states this correctly a few hundred lines away — *"a number a person entered by
hand … is not this derive's to recover"* — so the two now contradict each
other. Fix the paraphrase, or drop it and point at `derive.rs`.

This is the third correction on this branch to introduce a new inaccuracy while
fixing an old one. Read the code, not the sentence being replaced.

- [ ] **Step 4: the branch nobody exercises**

The fan-out saves siblings in the series a work **left** as well as the one it
joined, and no test covers the left branch —
`patching_a_works_series_saves_every_sibling_too` only moves a work within one
series. An unexercised branch is the failure mode this whole task exists to
close. Move a work out of a series that keeps another work, and assert the
remaining sibling's document no longer names the departed one.

- [ ] **Step 5: Run the gate for both crates**

`cargo test --workspace`, `cargo fmt --check`,
`cargo clippy --workspace --all-targets -- -D warnings`.

---

### Task 7: O62 — a work whose files are gone

**Files:**
- Modify: `crates/taiyaku-library/src/browse.rs`
- Test: `crates/taiyaku-library/src/browse/tests.rs`

**Interfaces:**
- Produces: a `missing: usize` count on whatever struct `browse` already
  answers an edition with, so C1's series page can show an edition as
  unreadable rather than offering it and failing at the first page.

**The query, measured 2026-08-26** (build design §8, and O62's own entry). The
join is over a computed string on the **probing** side; the side being probed is
`scan_entry`, whose `UNIQUE (root_id, rel_path)` is exactly the index it wants:

```sql
LEFT JOIN scan_entry se
       ON se.root_id  = m.root_id
      AND se.rel_path = ltrim(substr(m.path, length(r.path) + 1), '/')
```

`ltrim` rather than `length(r.path) + 2`, because a root stored with a trailing
slash makes the arithmetic off by one and `ltrim` is right either way.
`derive` builds `member.path` as `root.path.join(rel_path)`, so this is exact
rather than approximate. Shipped code already performs this same join for
another reason — `proposals::edition_formats` reads an edition's formats back
out of `scan_entry` on `(root_id, rel_path)` — so this is a second caller of a
known-good shape rather than a new idea.

- [ ] **Step 1: Write the failing test**

In `crates/taiyaku-library/src/browse/tests.rs`:

```rust
#[test]
fn an_edition_whose_file_has_left_the_disk_reports_it_missing() {
    let conn = Connection::open_in_memory().expect("in-memory database");
    crate::schema::migrate(&conn).expect("migrating");
    conn.execute_batch(
        "INSERT INTO root (id, path) VALUES (1, '/library');
         INSERT INTO work (id, content_key, slug, title, reading, hub)
         VALUES (1, 'k', 'w', 'W', 'rtl', 'en');
         INSERT INTO edition (id, work_id, key, label) VALUES (1, 1, 'en', 'en');
         INSERT INTO member (id, edition_id, ordinal, path, sha256, pages, root_id)
         VALUES (1, 1, 0, '/library/here.cbz', 'a', 10, 1),
                (2, 1, 1, '/library/gone.cbz', 'b', 10, 1);
         INSERT INTO scan_entry (root_id, rel_path) VALUES (1, 'here.cbz');",
    )
    .expect("seeding");

    let missing = super::missing_members(&conn, 1).expect("counting");

    assert_eq!(
        missing, 1,
        "the scanner deletes `scan_entry` rows for files it no longer meets \
         while `member` rows survive — that difference is the whole fact"
    );
}
```

Match the `scan_entry` insert to that table's actual NOT NULL columns; add
whatever else it requires rather than changing the schema.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p taiyaku-library browse::tests::an_edition_whose_file_has_left_the_disk_reports_it_missing`
Expected: FAIL — no function `missing_members`.

- [ ] **Step 3: Write the query**

In `crates/taiyaku-library/src/browse.rs`:

```rust
/// How many of this edition's members no longer exist on disk.
///
/// **O62.** The scanner deletes a `scan_entry` for a file it no longer meets
/// (`scan::forget_unseen`) while the `member` row survives, so the difference
/// between the two is the fact — no new column is needed, and C1 §5.5's
/// framing of this as "what the store should record when a file vanishes" is
/// answered *nothing further*.
///
/// # Errors
///
/// Returns an error if SQLite fails.
pub fn missing_members(conn: &Connection, edition_id: i64) -> Result<usize, String> {
    conn.query_row(
        "SELECT COUNT(*)
           FROM member m
           JOIN root r ON r.id = m.root_id
           LEFT JOIN scan_entry se
                  ON se.root_id  = m.root_id
                 AND se.rel_path = ltrim(substr(m.path, length(r.path) + 1), '/')
          WHERE m.edition_id = ?1 AND se.rel_path IS NULL",
        [edition_id],
        |row| row.get::<_, i64>(0),
    )
    .map(|n| usize::try_from(n).unwrap_or(0))
    .map_err(|e| format!("counting missing members of edition {edition_id}: {e}"))
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test -p taiyaku-library browse::`
Expected: PASS.

- [ ] **Step 5: Surface it on the edition the browse API answers**

Add `missing` to the edition struct `browse` already returns and populate it
from `missing_members`, so C1's existing series-page response carries it with
no new route. Extend the browse test that asserts an edition's shape rather
than adding a second one.

- [ ] **Step 6: Run the crate and the serve tests**

Run: `cargo test -p taiyaku-library -p taiyaku-serve`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add crates/taiyaku-library/src/browse.rs crates/taiyaku-library/src/browse/tests.rs
git commit -m "O62: an edition reports how many of its files have left the disk"
```

---

### Task 7b: `missing_members` must take the caller's grant

*Added 2026-08-28. Found by Task 7's implementer and reported rather than
worked around.*

- [ ] **Step 1: the signature the module's own contract requires**

`browse.rs`'s module doc opens with *"Every function takes `roots: &[i64]`, the
caller's granted roots"*. **`missing_members` is the only one that does not** —
the plan specified it that way, and that is a grant leak: a reader granted one
root would see a `missing` count reflecting a file gone under a root they
cannot see, which is a fact about material the grant hides.

This is not a corner case. `member_clause`'s own doc comment records that an
edition spanning two roots is *"reachable in the default configuration rather
than in a corner: two roots added with `library scan --add` carry no
`edition_key`, both fall back to `raw`, and derivation merges their members
into one edition."* It also records that this exact leak was found and fixed
once already, by a whole-branch review, for the same alias.

Take `roots` and use the clause that exists:

```rust
pub fn missing_members(
    conn: &Connection,
    roots: &[i64],
    edition_id: i64,
) -> Result<usize, String> {
    let members = member_clause(roots);
    let sql = format!(
        "SELECT COUNT(*)
           FROM member m
           JOIN root r ON r.id = m.root_id
           LEFT JOIN scan_entry se
                  ON se.root_id  = m.root_id
                 AND se.rel_path = ltrim(substr(m.path, length(r.path) + 1), '/')
          WHERE m.edition_id = ?1 AND se.rel_path IS NULL{members}"
    );
    ...
}
```

**Parameter order is text order** — this module's one binding rule, stated in
`pair_clause`'s doc. `?1` is the edition id and the roots bind after it, so
build the parameter list in that order or the roots shift.

**The test must be a grant test, not a count test.** Seed one edition whose
members span two roots, one missing under each, and assert that a caller
granted only the first root is told about one missing file rather than two.
An empty grant is unrestricted and must still see both — `member_clause`
returns an empty string for that case, so the query is unchanged there.

- [ ] **Step 2: the count has to reach the client**

`taiyaku-serve`'s `library_api.rs`'s work response does not forward `missing`,
so nothing a client can read carries it and Task 9 has nothing to render.
Forward it, passing the caller's granted roots — the same `Principal.roots`
that route already resolves for its other queries. Extend the existing response
test rather than adding a parallel one.

---

### Task 8: The web client

**Files:**
- Create: `web/src/metadata.ts`, `web/src/metadata.test.ts`
- Test: `web/src/metadata.test.ts` (**`node` project** — DOM-free module)

**Interfaces:**
- Consumes: Task 6's four routes.
- Produces: `patchWork`, `patchSeries`, `patchEdition`, `patchMember`, each
  `(id, fields) => Promise<LibraryDoc & { persisted: boolean }>`.

Follow `web/src/curation.ts` exactly — same fetch helper, same error shape,
same `credentials` handling. Do not invent a second HTTP convention.

- [ ] **Step 1: Write the failing test**

```ts
import { describe, expect, it, vi } from "vitest";
import { patchWork } from "./metadata";

describe("patchWork", () => {
  it("sends only the fields it was given", async () => {
    const fetchMock = vi.fn().mockResolvedValue(
      new Response(JSON.stringify({ title: "New", persisted: true }), {
        status: 200,
        headers: { "content-type": "application/json" },
      }),
    );
    vi.stubGlobal("fetch", fetchMock);

    const result = await patchWork("w", { title: "New" });

    expect(fetchMock).toHaveBeenCalledWith(
      "/api/library/works/w",
      expect.objectContaining({ method: "PATCH", body: JSON.stringify({ title: "New" }) }),
    );
    expect(result.persisted).toBe(true);
  });

  it("reports a document that could not be written to disk", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn().mockResolvedValue(
        new Response(JSON.stringify({ title: "New", persisted: false }), {
          status: 200,
          headers: { "content-type": "application/json" },
        }),
      ),
    );

    const result = await patchWork("w", { title: "New" });

    expect(result.persisted).toBe(false);
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `pnpm -C web exec vitest run --project node metadata`
Expected: FAIL — cannot resolve `./metadata`.

- [ ] **Step 3: Write the module**

Write `web/src/metadata.ts` mirroring `curation.ts`'s helper and exporting the
four functions.

- [ ] **Step 4: Run the test to verify it passes**

Run: `pnpm -C web exec vitest run --project node metadata`
Expected: PASS, both tests.

- [ ] **Step 5: Commit**

```bash
git add web/src/metadata.ts web/src/metadata.test.ts
git commit -m "The metadata editor's HTTP client"
```

---

### Task 9: The editor, and the unreadable edition

**Files:**
- Create: `web/src/MetadataEditor.tsx`, `web/src/MetadataEditor.test.tsx`
- Modify: `web/src/Series.tsx`
- Test: `web/src/MetadataEditor.test.tsx`, `web/src/Series.test.tsx` (**`jsdom`**)

**Interfaces:**
- Consumes: Task 8's `patchWork`/`patchSeries`/`patchEdition`/`patchMember`,
  Task 7's `missing` count on an edition.

**Contextual, from the thing being corrected** (C3 §5): no separate metadata
section to navigate to. The series page edits series-level fields; a volume's
row opens the work-level ones. Use this repo's own `mount()` + `act()` +
`querySelector` convention — `screen` and `userEvent` are not available and
`web/src/testing/mount.tsx` rejects them.

- [ ] **Step 1: Write the failing tests**

```tsx
import { describe, expect, it, vi } from "vitest";
import { act } from "react";
import { mount } from "./testing/mount";
import { MetadataEditor } from "./MetadataEditor";

describe("MetadataEditor", () => {
  it("saves a corrected title and shows the new one", async () => {
    const patch = vi.fn().mockResolvedValue({ title: "Volume 1", persisted: true });
    const { container } = mount(
      <MetadataEditor slug="w" title="Series - v01 (2021).cbz" onPatch={patch} />,
    );

    const field = container.querySelector<HTMLInputElement>("input[name='title']");
    expect(field?.value).toBe("Series - v01 (2021).cbz");

    await act(async () => {
      field!.value = "Volume 1";
      field!.dispatchEvent(new Event("input", { bubbles: true }));
      container.querySelector<HTMLFormElement>("form")!.requestSubmit();
    });

    expect(patch).toHaveBeenCalledWith("w", { title: "Volume 1" });
    expect(container.textContent).toContain("Volume 1");
  });

  it("says so when the correction could not be written beside the library", async () => {
    const patch = vi.fn().mockResolvedValue({ title: "Volume 1", persisted: false });
    const { container } = mount(
      <MetadataEditor slug="w" title="old" onPatch={patch} />,
    );

    await act(async () => {
      container.querySelector<HTMLFormElement>("form")!.requestSubmit();
    });

    expect(container.textContent).toMatch(/could not be saved beside your library/i);
  });
});
```

And in `web/src/Series.test.tsx`, for O62:

```tsx
it("shows an edition whose files are gone as unreadable rather than offering it", () => {
  const { container } = mount(
    <SeriesTable
      editions={[
        { key: "en", label: "English", missing: 0 },
        { key: "raw", label: "Raw", missing: 3 },
      ]}
    />,
  );

  const rows = container.querySelectorAll("tr[data-edition]");
  expect(rows[1].getAttribute("data-unreadable")).toBe("true");
  expect(rows[1].querySelector("a[href*='/read/']")).toBeNull();
});
```

Adjust the props to whatever `Series.tsx` actually renders — read it first and
match its existing shape rather than reshaping the component to suit the test.

- [ ] **Step 2: Run tests to verify they fail**

Run: `pnpm -C web test:jsdom MetadataEditor Series`
Expected: FAIL — cannot resolve `./MetadataEditor`.

- [ ] **Step 3: Write the component and wire it in**

Write `MetadataEditor.tsx`, and in `Series.tsx` add its entry point plus the
`missing > 0` treatment: mark the row unreadable and do not render a link into
the reader for it. The link is the point — C1 §8 asks for an edition to be
*shown as unreadable rather than offered and then failing at the first page*.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `pnpm -C web test:jsdom MetadataEditor Series`
Expected: PASS.

- [ ] **Step 5: Run the whole web suite the gate actually runs**

Run: `pnpm -C web run test:coverage`
Expected: PASS, and coverage not below its floor. `pnpm -C web test` is **not**
this command and never executes a `.test.tsx`.

- [ ] **Step 6: Commit**

```bash
git add web/src/MetadataEditor.tsx web/src/MetadataEditor.test.tsx web/src/Series.tsx web/src/Series.test.tsx
git commit -m "The metadata editor, and an edition whose files are gone shown as unreadable"
```

---

### Task 10: The open items, the docs, and the gate

**Files:**
- Modify: `docs/status.md`, `docs/notes.md`

**Interfaces:**
- Consumes: everything above.

Closing an O-number is a five-part edit; follow `docs/status.md`'s own ritual
rather than only ticking the box.

- [ ] **Step 1: Close what this PR actually closed**

- **O44** — `librarydoc::apply` now has callers: Task 6's four routes, and
  Task 5's read-back in `derive`. `forget_orphaned_series` is reachable with
  it.
- **O53** — the unreachable `Step::Apply` is routed around by
  `hand::load` + `derive`'s read-back, which is the "real but different path"
  that entry already named.
- **O62** — `browse::missing_members`, and the series page no longer offers an
  edition it cannot open.
- **O67** — a hand-set `work.title`, with the slug pinned in place by Task 2's
  test.

- [ ] **Step 2: Open what this PR opened**

- **The I7 freeze.** A hand edit to any one of `work.series_id`,
  `work.volume`, `edition.kind` or `edition.language` withdraws **all four**
  from derivation for that work, permanently and at whole-work granularity.
  `derive.rs`'s own comment already calls this open — *"a provenance column,
  scoping this to works with no `sidecar_path`, and so on"* — and this PR makes
  it reachable rather than answering it. Cite the correction in the build
  design §7.
- **A stranded hand document.** A work whose **hub edition's own files** change
  gets a new `content_key`, so `hand::load` no longer finds the document at the
  old address. It is not deleted and an exact key match still finds it if the
  content returns. Narrow — renames, remounts and added editions all leave the
  key alone — but real. Build design §12.
- **`persisted: false` is reported and nothing acts on it.** A correction made
  against a read-only root lives only in the database. Nothing retries it when
  the root becomes writable.

- [ ] **Step 3: Record the measurement in `docs/notes.md`**

Whatever was actually measured while building — at minimum the O62 query plan
re-taken against a database with migration 010 applied, since §8's plan was
measured against `001`–`008` and this PR adds a migration. If the plan changed,
the claim in O62's entry changes with it.

- [ ] **Step 4: Run the gate**

Run: `env -u TAIYAKU_CORPUS scripts/check-all.sh`
Expected: the line `all green`. Quote it verbatim when reporting; piping this
through `tail` masks the exit status.

- [ ] **Step 5: Commit**

```bash
git add docs/status.md docs/notes.md
git commit -m "Close O44, O53, O62 and O67; open the I7 freeze and the stranded document"
```

---

## What this plan does not do

- **The ComicInfo write-back** (C3 §6, its §10 step 7). Out per the build
  design §1, and unchanged by the widening: it depends on nothing else in C3
  and is the one feature that rewrites the user's own archives.
- **Answer I7.** The freeze is made reachable and written down, not resolved.
  A provenance column per field, or scoping the guard to works with no
  `sidecar_path`, are both still open.
- **Bulk metadata editing** across many series. C3 §9.
