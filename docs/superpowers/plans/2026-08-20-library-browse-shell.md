# Library browse — Plan A: the shell Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the library reachable end to end — the browse API gains the four
things §5.5 named, the app gains a router, and a reader can open it, see their
library, filter it, land on what they were reading and get back out.

**Architecture:** Four additive Rust changes to `taiyaku_library::browse` and
`taiyaku-serve`'s `library_api`, then `react-router` over `web/src` with a new
`library.ts` client. `App.tsx` stops opening the first work and starts opening
the one in the URL, remounted by `key` rather than made re-entrant.

**Tech Stack:** Rust (rusqlite, axum), TypeScript, React 19, `react-router`,
Vitest (`node` and `jsdom` projects), Biome.

## Global Constraints

- **Spec:** `docs/superpowers/specs/2026-08-14-library-browse-design.md`. This
  plan is its §11 "Plan A": §5.5's four API changes, then steps 2, 3, 6 and 10.
- **Empty grant means unrestricted.** `roots: &[i64]` empty applies *no* filter
  (§5.1). Never invert this.
- **Every placeholder in `browse.rs` is anonymous `?`, never `?1`**, and
  parameters bind in **text order: roots first, then everything else.** A single
  `?3` renumbers the anonymous holes after it and `rusqlite` refuses the call.
  See that module's header.
- **New guarded routes go in `guarded_routes()`**, never `open_routes()`, and
  must be added to the typed list in
  `crates/taiyaku-serve/src/auth/tests.rs::every_route_refuses_an_anonymous_request`.
- **Nothing outside `App.tsx`'s own test file may value-import `resolver.ts`**
  (it pulls in `taiyaku-wasm`, generated into a gitignored directory). Browse
  components must not touch it.
- **`web/src/prefs.ts` is the only module that touches `localStorage`.**
- **Coverage gates:** Rust `cargo llvm-cov --fail-under-lines 93`; web
  aggregate `statements 96 / branches 92 / functions 97 / lines 97`, plus the
  per-file floor and file count in `scripts/coverage-floor.mjs`.
- **The gate is `scripts/check-all.sh`.** Read its exit status directly — do not
  pipe it through `tail` or append `; echo`, both of which mask a failure.
- **Preferences are Plan B's.** The shell reads `continueStrip` and `showRoots`
  as hard-coded defaults here (strip on; roots on for an admin, off otherwise).
  The `browse` module that makes them settings is §11 step 8, in Plan B. Do not
  add a `loadPrefs` call for them in this plan.

---

### Task 1: `SeriesCard` gains `added` and `unread`

**Files:**
- Modify: `crates/taiyaku-library/src/browse.rs` — `SeriesCard` (~line 48), `series()` (~line 169), `one_series()` (~line 310)
- Modify: `crates/taiyaku-serve/src/library_api.rs` — `series_json`, `series`, `one_series`
- Test: `crates/taiyaku-library/src/browse/tests.rs`

**Interfaces:**
- Consumes: nothing.
- Produces: `browse::series(conn: &Connection, roots: &[i64], user_id: i64) -> Result<Vec<SeriesCard>, String>`; `SeriesCard { id: i64, title: String, works: i64, editions: i64, aligned: i64, cover_work: Option<i64>, added: i64, unread: i64 }`. Task 2 changes this signature again; Task 5 consumes the JSON.

- [ ] **Step 1: Write the fixture that makes the counts distinguishable**

`two_root_library()` has one work, so `unread == works` and `unread == works - 1`
are 1 and 0 — a test over it cannot tell "all unread" from "one read". Add to
`crates/taiyaku-library/src/browse/tests.rs`, after `import_a_work`:

```rust
/// One root, one series, **two** volumes — the smallest fixture where
/// `unread == works` and `unread == works - 1` are different numbers.
///
/// `two_root_library` has a single volume, so every count it can produce is 1
/// or 0, and a test over it passes whether or not the progress join works.
fn two_volume_library() -> (Library, i64, crate::tests::TempDir) {
    let dir = crate::tests::tempdir("browse-two-volumes");
    write_pages(&dir.path.join("Series").join("Series - v01"), 2);
    write_pages(&dir.path.join("Series").join("Series - v02"), 2);

    let library = Library::open_in_memory().expect("in-memory");
    let root_id = {
        let conn = library.conn();
        let root = scan::add_root(&conn, &dir.path).expect("adds the root");
        scan::scan_root(&conn, &root).expect("scans");
        derive::derive(&conn).expect("derives");
        root.id
    };
    (library, root_id, dir)
}

/// The ids of this library's works, lowest volume first.
fn work_ids(library: &Library) -> Vec<i64> {
    let conn = library.conn();
    let mut stmt = conn
        .prepare("SELECT id FROM work ORDER BY volume, id")
        .expect("prepares");
    let ids = stmt
        .query_map([], |row| row.get(0))
        .expect("reads")
        .collect::<rusqlite::Result<Vec<i64>>>()
        .expect("reads");
    ids
}
```

- [ ] **Step 2: Write the failing tests**

Append to `crates/taiyaku-library/src/browse/tests.rs`:

```rust
#[test]
fn a_series_nobody_has_opened_is_entirely_unread() {
    let (library, _root, _dir) = two_volume_library();
    let conn = library.conn();
    let cards = series(&conn, &[], 1).expect("lists series");
    let card = cards.first().expect("one series");
    assert_eq!(card.works, 2, "the fixture has two volumes");
    assert_eq!(card.unread, 2, "neither has been opened");
}

#[test]
fn one_position_leaves_the_rest_of_the_series_unread() {
    let (library, _root, _dir) = two_volume_library();
    let conn = library.conn();
    let ids = work_ids(&library);
    conn.execute(
        "INSERT INTO progress (user_id, work_id, edition, slot, updated)
         VALUES (1, ?1, 'raw', 0, '2026-08-19T00:00:00Z')",
        [ids[0]],
    )
    .expect("a position on volume 1");

    let cards = series(&conn, &[], 1).expect("lists series");
    let card = cards.first().expect("one series");
    assert_eq!(card.unread, 1, "volume 2 is still unread");
    assert_ne!(card.unread, card.works, "the series is no longer untouched");
}

/// **The test that pins the `user_id` binding.** With the parameter bound in
/// the wrong order — or not bound at all — this reads as if reader 1 had
/// opened the volume, and every other test in this file still passes.
#[test]
fn another_readers_position_does_not_mark_this_one_read() {
    let (library, _root, _dir) = two_volume_library();
    let conn = library.conn();
    let ids = work_ids(&library);
    conn.execute(
        "INSERT INTO progress (user_id, work_id, edition, slot, updated)
         VALUES (2, ?1, 'raw', 0, '2026-08-19T00:00:00Z')",
        [ids[0]],
    )
    .expect("reader 2's position");

    let cards = series(&conn, &[], 1).expect("lists series");
    assert_eq!(
        cards.first().expect("one series").unread,
        2,
        "reader 1 has opened nothing"
    );
}

#[test]
fn added_is_the_highest_work_id_in_the_series() {
    let (library, _root, _dir) = two_volume_library();
    let conn = library.conn();
    let ids = work_ids(&library);
    let cards = series(&conn, &[], 1).expect("lists series");
    assert_eq!(
        cards.first().expect("one series").added,
        *ids.iter().max().expect("two works"),
    );
}
```

- [ ] **Step 3: Run them and watch them fail**

Run: `cargo test -p taiyaku-library browse::tests`
Expected: FAIL — `series` takes 2 arguments, not 3; no field `unread` on `SeriesCard`.

- [ ] **Step 4: Add the two fields**

In `crates/taiyaku-library/src/browse.rs`, extend `SeriesCard`:

```rust
    pub aligned: i64,
    pub cover_work: Option<i64>,
    /// The highest `work.id` in the series — spec C1 §5.5's recency proxy.
    ///
    /// The same one `home`'s `added` list already sorts on (`w.id DESC`). A
    /// real `work.created_at` is what would replace it, and a migration for a
    /// sort order is not worth one while every row any scan has produced sorts
    /// identically under both.
    pub added: i64,
    /// Works in this series with no `progress` row for the calling reader.
    ///
    /// The `Unread` chip means `unread == works`: nothing here started.
    /// `unread > 0` was the alternative and leaves a 14-volume series in the
    /// chip forever after volume 1.
    pub unread: i64,
```

- [ ] **Step 5: Rewrite the query**

Replace the body of `series` in `crates/taiyaku-library/src/browse.rs`:

```rust
pub fn series(conn: &Connection, roots: &[i64], user_id: i64) -> Result<Vec<SeriesCard>, String> {
    let clause = root_clause(roots);
    // Placeholders bind in text order, all anonymous: the edition join's
    // roots, then `user_id` in the progress join below it. See the module
    // header. The progress join sits *after* the edition join for exactly
    // that reason — as a `SELECT`-list subquery its parameter would bind
    // first, ahead of the roots, and break the rule this module has one of.
    let sql = format!(
        "SELECT s.id,
                COALESCE(MAX(CASE WHEN st.is_primary = 1 THEN st.title END), s.sort_title),
                COUNT(DISTINCT w.id),
                COUNT(DISTINCT e.id),
                COUNT(DISTINCT CASE WHEN a.id IS NOT NULL THEN w.id END),
                s.cover_work_id,
                MAX(w.id),
                COUNT(DISTINCT CASE WHEN p.work_id IS NULL THEN w.id END)
           FROM series s
           JOIN work w ON w.series_id = s.id
           JOIN edition e ON e.work_id = w.id{clause}
           LEFT JOIN progress p ON p.work_id = w.id AND p.user_id = ?
           LEFT JOIN series_title st ON st.series_id = s.id
           LEFT JOIN alignment a ON a.work_id = w.id
          GROUP BY s.id
          ORDER BY 2 COLLATE NOCASE"
    );
    let mut params: Vec<i64> = roots.to_vec();
    params.push(user_id);

    let mut stmt = conn
        .prepare(&sql)
        .map_err(|e| format!("preparing the series listing: {e}"))?;
    let rows = stmt
        .query_map(params_from_iter(params), |row| {
            Ok(SeriesCard {
                id: row.get(0)?,
                title: row.get(1)?,
                works: row.get(2)?,
                editions: row.get(3)?,
                aligned: row.get(4)?,
                cover_work: row.get(5)?,
                added: row.get(6)?,
                unread: row.get(7)?,
            })
        })
        .map_err(|e| format!("listing series: {e}"))?;
    rows.collect::<rusqlite::Result<Vec<SeriesCard>>>()
        .map_err(|e| format!("reading series: {e}"))
}
```

- [ ] **Step 6: Update the two callers**

In `browse.rs`, `one_series` gains the parameter and passes it through:

```rust
pub fn one_series(
    conn: &Connection,
    roots: &[i64],
    user_id: i64,
    id: i64,
) -> Result<Option<(SeriesCard, Vec<WorkCard>)>, String> {
    let Some(card) = series(conn, roots, user_id)?.into_iter().find(|c| c.id == id) else {
        return Ok(None);
    };
    let works = works_of(conn, roots, "w.series_id = ?", &[&id], "w.volume, w.id")?;
    Ok(Some((card, works)))
}
```

In `crates/taiyaku-serve/src/library_api.rs`, `series_json` gains the fields and
both handlers pass `principal.user_id`:

```rust
fn series_json(s: &browse::SeriesCard) -> serde_json::Value {
    serde_json::json!({
        "id": s.id,
        "title": s.title,
        "works": s.works,
        "editions": s.editions,
        "aligned": s.aligned,
        "added": s.added,
        "unread": s.unread,
        "cover": format!("/api/library/series/{}/cover", s.id),
    })
}
```

and in the `series` handler `browse::series(&conn, &principal.roots, principal.user_id)`,
in `one_series` `browse::one_series(&conn, &principal.roots, principal.user_id, id)`.

`browse::search` calls `series(conn, roots)` directly — it filters the FTS hits
through it *"so a hit this caller may not see is dropped by exactly the same
code that decides every other screen"*. So it gains the parameter too:

```rust
pub fn search(
    conn: &Connection,
    roots: &[i64],
    user_id: i64,
    q: &str,
) -> Result<SearchResults, String> {
```

with `series(conn, roots, user_id)?` inside it, and the `search` handler in
`library_api.rs` passing `principal.user_id`. Task 2 adds a fourth argument to
`series`; `search` passes `None` for it.

- [ ] **Step 7: Run the tests**

Run: `cargo test -p taiyaku-library browse::tests`
Expected: PASS, including the four new cases.

Run: `cargo test -p taiyaku-serve library_api`
Expected: PASS.

- [ ] **Step 8: Add the route-level assertion**

In `crates/taiyaku-serve/src/library_api/tests.rs`:

```rust
#[tokio::test]
async fn the_grid_carries_the_two_fields_the_chips_sort_and_filter_on() {
    let (router, _state) = test_support::router_over_a_library().await;
    let body = get_json(&router, "/api/library/series").await;
    let first = body["series"]
        .as_array()
        .and_then(|s| s.first())
        .expect("at least one series");
    assert!(first["added"].is_i64(), "added is a number: {first}");
    assert!(first["unread"].is_i64(), "unread is a number: {first}");
}
```

Run: `cargo test -p taiyaku-serve library_api`
Expected: PASS.

- [ ] **Step 9: Commit**

```bash
git add crates/taiyaku-library/src/browse.rs crates/taiyaku-library/src/browse/tests.rs crates/taiyaku-serve/src/library_api.rs crates/taiyaku-serve/src/library_api/tests.rs
git commit -m "SeriesCard gains added and unread, and series() gains the reader it counts for"
```

---

### Task 2: `?root=` on the grid

**Files:**
- Modify: `crates/taiyaku-library/src/browse.rs` — `series()`, `one_series()`
- Modify: `crates/taiyaku-serve/src/library_api.rs` — `series` handler
- Test: `crates/taiyaku-library/src/browse/tests.rs`, `crates/taiyaku-serve/src/library_api/tests.rs`

**Interfaces:**
- Consumes: Task 1's `series(conn, roots, user_id)`.
- Produces: `browse::series(conn: &Connection, roots: &[i64], user_id: i64, only: Option<i64>) -> Result<Vec<SeriesCard>, String>`. `one_series` passes `None`. Task 5 consumes `GET /api/library/series?root=N`.

- [ ] **Step 1: Write the failing tests**

Append to `crates/taiyaku-library/src/browse/tests.rs`:

```rust
#[test]
fn a_root_filter_keeps_only_that_roots_editions() {
    let (library, en_id, _raw_id, _en, _raw) = two_root_library();
    let conn = library.conn();
    let all = series(&conn, &[], 1, None).expect("lists series");
    assert_eq!(all.first().expect("one series").editions, 2);

    let english = series(&conn, &[], 1, Some(en_id)).expect("lists series");
    assert_eq!(
        english.first().expect("one series").editions,
        1,
        "only the English root's edition survives"
    );
}

/// **The filter narrows the grant; it never widens it.** A reader granted the
/// English root alone, asking for the raw one, is answered nothing — not the
/// raw edition, and not an error that would confirm the root exists.
#[test]
fn a_root_filter_outside_the_grant_answers_nothing() {
    let (library, en_id, raw_id, _en, _raw) = two_root_library();
    let conn = library.conn();
    let cards = series(&conn, &[en_id], 1, Some(raw_id)).expect("lists series");
    assert!(cards.is_empty(), "the grant and the filter do not intersect");
}
```

- [ ] **Step 2: Run them and watch them fail**

Run: `cargo test -p taiyaku-library browse::tests::a_root_filter`
Expected: FAIL — `series` takes 3 arguments, not 4.

- [ ] **Step 3: Add the parameter**

In `browse.rs`, add the helper beside `root_clause`:

```rust
/// The `AND` restricting an edition to **one** root, on top of the grant.
///
/// Spec C1 §5.5's `?root=`. Intersected with [`root_clause`] rather than
/// replacing it: a root the caller does not hold yields no rows, which is the
/// same answer as a root that does not exist. Confirming which ids are real is
/// the enumeration `one_series` already answers 404 rather than 403 to avoid.
fn only_clause(only: Option<i64>) -> String {
    if only.is_none() {
        return String::new();
    }
    " AND EXISTS (SELECT 1 FROM member om WHERE om.edition_id = e.id \
     AND om.root_id = ?)"
        .to_string()
}
```

Change the signature and the SQL — the new clause goes immediately after
`{clause}` so the binding order stays roots, then `only`, then `user_id`:

```rust
pub fn series(
    conn: &Connection,
    roots: &[i64],
    user_id: i64,
    only: Option<i64>,
) -> Result<Vec<SeriesCard>, String> {
    let clause = root_clause(roots);
    let narrowed = only_clause(only);
    // Text order: the edition join's roots, then `only`, then `user_id`.
```

with `JOIN edition e ON e.work_id = w.id{clause}{narrowed}` and:

```rust
    let mut params: Vec<i64> = roots.to_vec();
    params.extend(only);
    params.push(user_id);
```

`one_series` and `search` both call `series(conn, roots, user_id, None)` — neither narrows to a root, and a search that silently answered only one root's hits would be a filter nobody asked for.

- [ ] **Step 4: Run the store tests**

Run: `cargo test -p taiyaku-library browse::tests`
Expected: PASS.

- [ ] **Step 5: Accept the query parameter**

In `crates/taiyaku-serve/src/library_api.rs`, above the `series` handler:

```rust
/// The optional `?root=` a grid carries — spec C1 §5.5.
#[derive(serde::Deserialize)]
pub(crate) struct GridQuery {
    root: Option<i64>,
}
```

and in the handler, `Query(query): Query<GridQuery>` with
`browse::series(&conn, &principal.roots, principal.user_id, query.root)`.
`browse::unfiled` is unchanged: an unfiled work's members have `root_id` NULL,
so no root filter can ever match one, and asking for a root means asking for
what is under it.

- [ ] **Step 6: Add the route test**

In `crates/taiyaku-serve/src/library_api/tests.rs`:

```rust
#[tokio::test]
async fn a_root_that_does_not_exist_answers_an_empty_grid_rather_than_an_error() {
    let (router, _state) = test_support::router_over_a_library().await;
    let body = get_json(&router, "/api/library/series?root=999999").await;
    assert_eq!(
        body["series"].as_array().map(Vec::len),
        Some(0),
        "no series, and a 200: {body}"
    );
}
```

Run: `cargo test -p taiyaku-serve library_api`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add crates/taiyaku-library/src/browse.rs crates/taiyaku-library/src/browse/tests.rs crates/taiyaku-serve/src/library_api.rs crates/taiyaku-serve/src/library_api/tests.rs
git commit -m "The grid takes ?root=, intersected with the grant rather than replacing it"
```

---

### Task 3: alignments on the single-work route

**Files:**
- Modify: `crates/taiyaku-library/src/browse.rs` — new `AlignmentCard`, `WorkDetail`, `alignments_of`, `work()`
- Modify: `crates/taiyaku-serve/src/library_api.rs` — `work` handler
- Test: `crates/taiyaku-library/src/browse/tests.rs`, `crates/taiyaku-serve/src/library_api/tests.rs`

**Interfaces:**
- Consumes: nothing from Tasks 1–2.
- Produces: `browse::work(conn: &Connection, roots: &[i64], slug: &str) -> Result<Option<WorkDetail>, String>` where `WorkDetail { card: WorkCard, alignments: Vec<AlignmentCard> }` and `AlignmentCard { from: String, to: String, verdict: Option<String>, slots: Option<i64>, paired: Option<i64> }`. Task 5 consumes the JSON; Plan B's panel renders it.

- [ ] **Step 1: Write the failing tests**

Append to `crates/taiyaku-library/src/browse/tests.rs`:

```rust
/// An alignment row for a work, written directly: derivation only mints these
/// from a sidecar, and every fixture here is scanned.
fn align(library: &Library, work_id: i64, from: &str, to: &str) {
    let conn = library.conn();
    conn.execute(
        "INSERT INTO alignment (work_id, from_key, to_key, by, verdict, slots, paired)
         VALUES (?1, ?2, ?3, 'solver', 'good', 20, 18)",
        rusqlite::params![work_id, from, to],
    )
    .expect("an alignment");
}

#[test]
fn a_work_carries_the_verdict_and_paired_count_of_each_pair() {
    let (library, _en, _raw, _en_dir, _raw_dir) = two_root_library();
    let ids = work_ids(&library);
    align(&library, ids[0], "en", "raw");

    let conn = library.conn();
    let detail = work(&conn, &[], "series-v01")
        .expect("reads the work")
        .expect("a work with that slug");
    assert_eq!(detail.alignments.len(), 1);
    let pair = &detail.alignments[0];
    assert_eq!(pair.from, "en");
    assert_eq!(pair.to, "raw");
    assert_eq!(pair.verdict.as_deref(), Some("good"));
    assert_eq!(pair.paired, Some(18));
    assert_eq!(pair.slots, Some(20));
}

/// **A grant that hides one side hides the pair.** Listing an alignment against
/// an edition the reader cannot open would name material the grant withheld.
#[test]
fn an_alignment_naming_a_hidden_edition_is_not_listed() {
    let (library, en_id, _raw_id, _en_dir, _raw_dir) = two_root_library();
    let ids = work_ids(&library);
    align(&library, ids[0], "en", "raw");

    let conn = library.conn();
    let detail = work(&conn, &[en_id], "series-v01")
        .expect("reads the work")
        .expect("the work survives on its English edition");
    assert_eq!(detail.card.editions.len(), 1, "only the English edition");
    assert!(
        detail.alignments.is_empty(),
        "the pair names an edition this grant hides"
    );
}
```

The slug: confirm it by running
`cargo test -p taiyaku-library browse::tests::a_work_is_found_by_slug_within_the_grant -- --nocapture`
and reading the slug that test already uses; substitute it for `series-v01`
above if it differs.

- [ ] **Step 2: Run them and watch them fail**

Run: `cargo test -p taiyaku-library browse::tests::a_work_carries`
Expected: FAIL — no field `alignments` on `WorkCard`.

- [ ] **Step 3: Add the types and the query**

In `crates/taiyaku-library/src/browse.rs`, after `EditionCard`:

```rust
/// One edition pair's alignment, as §3's panel shows it.
///
/// **One row per pair, never a scalar on the work.** A work with three
/// editions has no single verdict, and picking one pair's would repeat §5.1's
/// `work.root_id` mistake somewhere new: a field with nothing true to hold that
/// a screen then presents as fact.
#[derive(Debug, Clone, PartialEq)]
pub struct AlignmentCard {
    pub from: String,
    pub to: String,
    pub verdict: Option<String>,
    pub slots: Option<i64>,
    pub paired: Option<i64>,
}

/// One work with its alignment detail — what `/api/library/works/{slug}`
/// answers and **what no list route carries**.
///
/// `works_of` already fires an `editions_of` query per row; an `alignments_of`
/// per row would put back the N+1 the whole-branch review closed in `home`, at
/// 111 queries on the largest series. §3's panel opens one work at a time, so
/// this is the route that fills it.
#[derive(Debug, Clone, PartialEq)]
pub struct WorkDetail {
    pub card: WorkCard,
    pub alignments: Vec<AlignmentCard>,
}
```

and, beside `editions_of`:

```rust
/// One work's alignments, with both sides restricted to the caller's roots.
///
/// The two `EXISTS` clauses come **before** `a.work_id = ?` so the roots bind
/// first, which is this module's one parameter rule. Written the other way
/// round the work id takes hole 1 and every root shifts.
fn alignments_of(
    conn: &Connection,
    roots: &[i64],
    work_id: i64,
) -> Result<Vec<AlignmentCard>, String> {
    let clause = root_clause(roots);
    let sql = format!(
        "SELECT a.from_key, a.to_key, a.verdict, a.slots, a.paired
           FROM alignment a
          WHERE EXISTS (SELECT 1 FROM edition e
                         WHERE e.work_id = a.work_id AND e.key = a.from_key{clause})
            AND EXISTS (SELECT 1 FROM edition e
                         WHERE e.work_id = a.work_id AND e.key = a.to_key{clause})
            AND a.work_id = ?
          ORDER BY a.from_key, a.to_key"
    );
    let mut params: Vec<i64> = roots.to_vec();
    params.extend(roots.iter().copied());
    params.push(work_id);

    let mut stmt = conn
        .prepare(&sql)
        .map_err(|e| format!("preparing the alignment listing: {e}"))?;
    let rows = stmt
        .query_map(params_from_iter(params), |row| {
            Ok(AlignmentCard {
                from: row.get(0)?,
                to: row.get(1)?,
                verdict: row.get(2)?,
                slots: row.get(3)?,
                paired: row.get(4)?,
            })
        })
        .map_err(|e| format!("listing alignments of work {work_id}: {e}"))?;
    rows.collect::<rusqlite::Result<Vec<AlignmentCard>>>()
        .map_err(|e| format!("reading alignments of work {work_id}: {e}"))
}
```

- [ ] **Step 4: Change `work` to return the detail**

```rust
pub fn work(conn: &Connection, roots: &[i64], slug: &str) -> Result<Option<WorkDetail>, String> {
    let Some(card) = works_of(conn, roots, "w.slug = ?", &[&slug], "w.id")?
        .into_iter()
        .next()
    else {
        return Ok(None);
    };
    let alignments = alignments_of(conn, roots, card.id)?;
    Ok(Some(WorkDetail { card, alignments }))
}
```

- [ ] **Step 5: Run the store tests**

Run: `cargo test -p taiyaku-library browse::tests`
Expected: PASS.

- [ ] **Step 6: Serialise it**

In `crates/taiyaku-serve/src/library_api.rs`, the `work` handler:

```rust
pub(crate) async fn work(
    State(state): State<Arc<ServeState>>,
    Extension(principal): Extension<Principal>,
    AxPath(slug): AxPath<String>,
) -> Response {
    let conn = state.library.conn();
    match browse::work(&conn, &principal.roots, &slug) {
        Ok(Some(detail)) => {
            let mut body = work_json(&detail.card);
            body["alignments"] = serde_json::json!(
                detail.alignments.iter().map(|a| serde_json::json!({
                    "from": a.from,
                    "to": a.to,
                    "verdict": a.verdict,
                    "slots": a.slots,
                    "paired": a.paired,
                })).collect::<Vec<_>>()
            );
            Json(body).into_response()
        }
        Ok(None) => (StatusCode::NOT_FOUND, format!("no work `{slug}`")).into_response(),
        Err(why) => failed("the work", &why),
    }
}
```

- [ ] **Step 7: Add the route test and run**

In `crates/taiyaku-serve/src/library_api/tests.rs`:

```rust
#[tokio::test]
async fn the_work_route_carries_an_alignments_array_and_the_list_routes_do_not() {
    let (router, _state) = test_support::router_over_a_library().await;
    let grid = get_json(&router, "/api/library/series").await;
    let listed = grid["series"].as_array().and_then(|s| s.first());
    if let Some(listed) = listed {
        assert!(
            listed.get("alignments").is_none(),
            "a list route must not carry alignment detail: {listed}"
        );
    }
    let home = get_json(&router, "/api/library/home").await;
    let slug = home["added"][0]["slug"].as_str().expect("a work").to_string();
    let detail = get_json(&router, &format!("/api/library/works/{slug}")).await;
    assert!(
        detail["alignments"].is_array(),
        "the single-work route carries them: {detail}"
    );
}
```

Run: `cargo test -p taiyaku-serve library_api`
Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add crates/taiyaku-library/src/browse.rs crates/taiyaku-library/src/browse/tests.rs crates/taiyaku-serve/src/library_api.rs crates/taiyaku-serve/src/library_api/tests.rs
git commit -m "The single-work route carries its alignments, and no list route does"
```

---

### Task 4: `GET /api/library/status`

**Files:**
- Modify: `crates/taiyaku-library/src/browse.rs` — `RootCard`, `Status`, `status()`, `named()`, `last_component()`
- Modify: `crates/taiyaku-serve/src/library_api.rs` — `status` handler
- Modify: `crates/taiyaku-serve/src/routes.rs` — `guarded_routes()`
- Modify: `crates/taiyaku-serve/src/auth/tests.rs` — the typed expected list
- Test: `crates/taiyaku-library/src/browse/tests.rs`, `crates/taiyaku-serve/src/library_api/tests.rs`

**Interfaces:**
- Consumes: nothing.
- Produces: `browse::status(conn: &Connection, roots: &[i64]) -> Result<Status, String>`, `Status { roots: Vec<RootCard>, scanned: bool }`, `RootCard { id: i64, name: String }`; the route `GET /api/library/status`. Task 9 consumes it.

- [ ] **Step 1: Write the failing tests**

Append to `crates/taiyaku-library/src/browse/tests.rs`:

```rust
#[test]
fn status_names_a_root_by_its_last_path_component() {
    let (library, _root, _dir) = two_volume_library();
    let conn = library.conn();
    let status = status(&conn, &[]).expect("reads status");
    assert_eq!(status.roots.len(), 1);
    assert!(
        !status.roots[0].name.contains('/'),
        "a nav item, not a filesystem path: {}",
        status.roots[0].name
    );
    assert!(status.scanned, "the fixture has been scanned");
}

#[test]
fn status_lists_only_the_roots_the_grant_names() {
    let (library, en_id, _raw_id, _en, _raw) = two_root_library();
    let conn = library.conn();
    assert_eq!(status(&conn, &[]).expect("reads").roots.len(), 2);
    let restricted = status(&conn, &[en_id]).expect("reads");
    assert_eq!(restricted.roots.len(), 1);
    assert_eq!(restricted.roots[0].id, en_id);
}

#[test]
fn two_roots_sharing_a_last_component_fall_back_to_their_paths() {
    let outer = crate::tests::tempdir("browse-collide");
    let library = Library::open_in_memory().expect("in-memory");
    {
        let conn = library.conn();
        for parent in ["a", "b"] {
            let path = outer.path.join(parent).join("Manga");
            write_pages(&path.join("Series - v01"), 1);
            scan::add_root(&conn, &path).expect("adds a root");
        }
    }
    let conn = library.conn();
    let status = status(&conn, &[]).expect("reads status");
    assert_eq!(status.roots.len(), 2);
    for root in &status.roots {
        assert!(
            root.name.contains('/'),
            "a colliding name falls back to the whole path: {}",
            root.name
        );
    }
    assert!(!status.scanned, "nothing has been scanned");
}
```

- [ ] **Step 2: Run them and watch them fail**

Run: `cargo test -p taiyaku-library browse::tests::status`
Expected: FAIL — cannot find function `status`.

- [ ] **Step 3: Implement it**

Append to `crates/taiyaku-library/src/browse.rs`:

```rust
/// One root, as §2's provisional `ROOTS` section lists it.
#[derive(Debug, Clone, PartialEq)]
pub struct RootCard {
    pub id: i64,
    pub name: String,
}

/// What a first-run screen needs to know, and the only thing that can tell it.
///
/// **Spec C1 §5.5.** `/api/library/series` answers `{ series: [], unfiled: [] }`
/// for "no roots configured" and for "roots configured, scan not yet run"
/// alike, and §8 wants those to be different screens: the difference between
/// "you have no library" and "your library has not been read yet".
///
/// Read-only, with no writer and no screen of its own. C4 still builds roots
/// administration.
#[derive(Debug, Clone, PartialEq)]
pub struct Status {
    pub roots: Vec<RootCard>,
    pub scanned: bool,
}

/// The last component of a path, or the whole thing if it has none.
fn last_component(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .map_or_else(|| path.to_string(), |n| n.to_string_lossy().into_owned())
}

/// Label each root, falling back to the whole path where two would collide.
///
/// A nav item reading `Manga` twice is worse than one reading
/// `/media/a/Manga`, and a non-admin reader has no business being handed the
/// server's filesystem layout to render a sidebar that does not collide.
fn named(rows: Vec<(i64, String)>) -> Vec<RootCard> {
    let mut counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for (_, path) in &rows {
        *counts.entry(last_component(path)).or_default() += 1;
    }
    rows.into_iter()
        .map(|(id, path)| {
            let short = last_component(&path);
            let name = if counts.get(&short).copied().unwrap_or(0) > 1 {
                path
            } else {
                short
            };
            RootCard { id, name }
        })
        .collect()
}

/// The roots this caller may see, and whether anything has been scanned.
///
/// # Errors
///
/// Returns an error if either query fails.
pub fn status(conn: &Connection, roots: &[i64]) -> Result<Status, String> {
    let clause = if roots.is_empty() {
        String::new()
    } else {
        let holes = std::iter::repeat_n("?", roots.len())
            .collect::<Vec<_>>()
            .join(",");
        format!(" WHERE id IN ({holes})")
    };
    let sql = format!("SELECT id, path FROM root{clause} ORDER BY path");
    let mut stmt = conn
        .prepare(&sql)
        .map_err(|e| format!("preparing the root listing: {e}"))?;
    let rows = stmt
        .query_map(params_from_iter(roots), |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|e| format!("listing roots: {e}"))?
        .collect::<rusqlite::Result<Vec<(i64, String)>>>()
        .map_err(|e| format!("reading roots: {e}"))?;

    let scanned: bool = conn
        .query_row("SELECT EXISTS (SELECT 1 FROM dir_state)", [], |row| row.get(0))
        .map_err(|e| format!("reading whether a scan has run: {e}"))?;

    Ok(Status {
        roots: named(rows),
        scanned,
    })
}
```

- [ ] **Step 4: Run the store tests**

Run: `cargo test -p taiyaku-library browse::tests`
Expected: PASS.

- [ ] **Step 5: Add the handler**

In `crates/taiyaku-serve/src/library_api.rs`:

```rust
/// `GET /api/library/status` — spec C1 §5.5.
///
/// Read-only. Two of spec C1 §8's first-run states are indistinguishable to
/// every other route, and a first-run state nobody can render is one that
/// ships broken to the only people who will ever see it.
pub(crate) async fn status(
    State(state): State<Arc<ServeState>>,
    Extension(principal): Extension<Principal>,
) -> Response {
    let conn = state.library.conn();
    match browse::status(&conn, &principal.roots) {
        Ok(status) => Json(serde_json::json!({
            "roots": status.roots.iter().map(|r| serde_json::json!({
                "id": r.id,
                "name": r.name,
            })).collect::<Vec<_>>(),
            "scanned": status.scanned,
        }))
        .into_response(),
        Err(why) => failed("the library status", &why),
    }
}
```

- [ ] **Step 6: Register it, in the guarded list**

In `crates/taiyaku-serve/src/routes.rs`, inside the `routes.extend([...])` block
that already holds the browse routes:

```rust
        ("/api/library/status", get(crate::library_api::status)),
```

In `crates/taiyaku-serve/src/auth/tests.rs`, add to the typed `expected` array
beside the other `/api/library/*` entries:

```rust
        "/api/library/status",
```

In `crates/taiyaku-serve/src/library_api/tests.rs`, add `"/api/library/status"`
to the path array in `every_browse_route_refuses_an_anonymous_request` (the
`for path in [...]` loop).

- [ ] **Step 7: Add the route test and run everything**

In `crates/taiyaku-serve/src/library_api/tests.rs`:

```rust
#[tokio::test]
async fn status_answers_the_two_facts_a_first_run_screen_needs() {
    let (router, _state) = test_support::router_over_a_library().await;
    let body = get_json(&router, "/api/library/status").await;
    assert!(body["roots"].is_array(), "roots is a list: {body}");
    assert!(body["scanned"].is_boolean(), "scanned is a bool: {body}");
}
```

Run: `cargo test -p taiyaku-serve`
Expected: PASS, including `every_route_refuses_an_anonymous_request`.

- [ ] **Step 8: Commit**

```bash
git add crates/taiyaku-library/src/browse.rs crates/taiyaku-library/src/browse/tests.rs crates/taiyaku-serve/src/library_api.rs crates/taiyaku-serve/src/library_api/tests.rs crates/taiyaku-serve/src/routes.rs crates/taiyaku-serve/src/auth/tests.rs
git commit -m "GET /api/library/status: the two facts a first-run screen needs and nothing else could tell it"
```

---

### Task 5: `web/src/library.ts`, the browse client

**Files:**
- Create: `web/src/library.ts`
- Create: `web/src/library.test.ts`
- Modify: `web/src/api.ts` — export a guarded `getJson`

**Interfaces:**
- Consumes: Tasks 1–4's JSON.
- Produces: `home()`, `grid(root?)`, `oneSeries(id)`, `work(slug)`, `search(q)`, `status()`, and the pure helpers `matchesChip(series, chip)` and `sorted(series, sort)`. Types `SeriesCard`, `WorkCard`, `EditionCard`, `AlignmentCard`, `WorkDetail`, `Grid`, `HomeLists`, `SearchHits`, `RootCard`, `LibraryStatus`, `Chip`, `Sort`. Tasks 8 and 9 consume all of it.

- [ ] **Step 1: Write the failing test**

Create `web/src/library.test.ts`:

```ts
import { describe, expect, it, vi } from 'vitest'
import { Unauthenticated } from './api'
import {
  grid,
  matchesChip,
  type SeriesCard,
  sorted,
  status,
  work,
} from './library'

const card = (over: Partial<SeriesCard> = {}): SeriesCard => ({
  id: 1,
  title: 'A Series',
  works: 3,
  editions: 3,
  aligned: 0,
  added: 10,
  unread: 3,
  cover: '/api/library/series/1/cover',
  ...over,
})

const json = (body: unknown) =>
  new Response(JSON.stringify(body), {
    status: 200,
    headers: { 'content-type': 'application/json' },
  })

describe('the browse client', () => {
  it('asks for the grid without a root, and with one', async () => {
    const fetchMock = vi.fn().mockResolvedValue(json({ series: [], unfiled: [] }))
    vi.stubGlobal('fetch', fetchMock)

    await grid()
    expect(fetchMock.mock.calls[0]?.[0]).toBe('/api/library/series')
    await grid(7)
    expect(fetchMock.mock.calls[1]?.[0]).toBe('/api/library/series?root=7')
  })

  it('escapes a slug rather than pasting it into the path', async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValue(json({ id: 1, slug: 'a b', alignments: [] }))
    vi.stubGlobal('fetch', fetchMock)
    await work('a b')
    expect(fetchMock.mock.calls[0]?.[0]).toBe('/api/library/works/a%20b')
  })

  it('routes a 401 through the one place that notices it', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response(null, { status: 401 })))
    await expect(status()).rejects.toBeInstanceOf(Unauthenticated)
  })

  it('reports a failing status by its code rather than parsing the body', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response('boom', { status: 500 })))
    await expect(status()).rejects.toThrow(/500/)
  })

  it('multi-edition means some work here has a second edition', () => {
    expect(matchesChip(card({ works: 3, editions: 3 }), 'multi')).toBe(false)
    expect(matchesChip(card({ works: 3, editions: 4 }), 'multi')).toBe(true)
  })

  it('unread means nothing here has been started', () => {
    expect(matchesChip(card({ works: 3, unread: 3 }), 'unread')).toBe(true)
    expect(matchesChip(card({ works: 3, unread: 2 }), 'unread')).toBe(false)
  })

  it('all keeps everything', () => {
    expect(matchesChip(card({ works: 3, unread: 0 }), 'all')).toBe(true)
  })

  it('sorts by added, highest first, without mutating its input', () => {
    const input = [card({ id: 1, added: 5 }), card({ id: 2, added: 9 })]
    expect(sorted(input, 'added').map((s) => s.id)).toEqual([2, 1])
    expect(input.map((s) => s.id)).toEqual([1, 2])
  })

  it('sorts by title with a locale comparison rather than code points', () => {
    const input = [card({ id: 1, title: 'beta' }), card({ id: 2, title: 'Alpha' })]
    expect(sorted(input, 'title').map((s) => s.id)).toEqual([2, 1])
  })
})
```

- [ ] **Step 2: Run it and watch it fail**

Run: `cd web && pnpm vitest run --project node src/library.test.ts`
Expected: FAIL — cannot resolve `./library`.

- [ ] **Step 3: Export the guarded JSON read from `api.ts`**

In `web/src/api.ts`, immediately after the `request` const:

```ts
/**
 * A guarded `GET` returning JSON.
 *
 * Exported for `library.ts`, which is a second module of calls against the
 * same server and the same session. A second `fetch` wrapper there would be a
 * second place for the 401 rule to be forgotten, which is the exact failure
 * `whenSignedOut` above exists to have fixed once.
 */
export async function getJson<T>(path: string): Promise<T> {
  const res = await request(path)
  if (!res.ok) throw new Error(`${path}: ${res.status}`)
  return (await res.json()) as T
}
```

- [ ] **Step 4: Write `library.ts`**

Create `web/src/library.ts`:

```ts
/**
 * The `/api/library/*` browse API. Knows HTTP; knows nothing about screens.
 *
 * Spec C1 §5 and §5.5. Separate from `api.ts` — which is the *reader's* API,
 * sidecars and pages and positions — because the two share a session and
 * nothing else: no browse screen touches a resolver, and no reader screen
 * reads a series. They share `getJson`, so the 401 rule is one rule.
 *
 * The chip and sort helpers are here rather than in a component because
 * §7 declines a `?sort=` round trip on measured numbers (127 series), which
 * makes "which series does this chip keep" a pure function over data the
 * browser already holds — and a pure function is the part worth testing.
 */
import { getJson } from './api'

export interface EditionCard {
  key: string
  label: string
  members: number
  pages: number
}

/** One edition pair's alignment — only `work()` carries these. */
export interface AlignmentCard {
  from: string
  to: string
  verdict: string | null
  slots: number | null
  paired: number | null
}

export interface WorkCard {
  id: number
  slug: string
  title: string
  volume: number | null
  aligned: boolean
  pages: number
  editions: EditionCard[]
}

export interface WorkDetail extends WorkCard {
  alignments: AlignmentCard[]
}

export interface SeriesCard {
  id: number
  title: string
  works: number
  editions: number
  /** Works with at least one alignment — §3's `⇄ 3` mark. */
  aligned: number
  /** The highest `work.id` in the series; §5.5's recency proxy. */
  added: number
  /** Works with no position for this reader. */
  unread: number
  cover: string
}

/** A grid, or a search over one: series, and the works that belong to none. */
export interface Grid {
  series: SeriesCard[]
  unfiled: WorkCard[]
}

export interface SearchHits extends Grid {
  /** Echoed back, so §8's empty state can name what was searched. */
  query: string
}

export interface HomeLists {
  continuing: WorkCard[]
  added: WorkCard[]
  read: WorkCard[]
}

export interface RootCard {
  id: number
  name: string
}

export interface LibraryStatus {
  roots: RootCard[]
  scanned: boolean
}

export const home = () => getJson<HomeLists>('/api/library/home')

export const grid = (root?: number) =>
  getJson<Grid>(
    root === undefined
      ? '/api/library/series'
      : `/api/library/series?root=${root}`,
  )

export const oneSeries = (id: number) =>
  getJson<{ series: SeriesCard; works: WorkCard[] }>(
    `/api/library/series/${id}`,
  )

export const work = (slug: string) =>
  getJson<WorkDetail>(`/api/library/works/${encodeURIComponent(slug)}`)

export const search = (q: string) =>
  getJson<SearchHits>(`/api/library/search?q=${encodeURIComponent(q)}`)

export const status = () => getJson<LibraryStatus>('/api/library/status')

/** The grid's filter chips — §2. */
export type Chip = 'all' | 'multi' | 'unread'

/**
 * Whether a chip keeps this series.
 *
 * `multi` is `editions > works` **exactly**, not approximately: a work with no
 * edition is not counted at all, so the two counts differ if and only if some
 * work here has a second edition.
 *
 * `unread` is `unread === works` — nothing here started. `unread > 0` was the
 * alternative and leaves a 14-volume series in the chip forever after volume 1.
 */
export function matchesChip(series: SeriesCard, chip: Chip): boolean {
  if (chip === 'multi') return series.editions > series.works
  if (chip === 'unread') return series.unread === series.works
  return true
}

export type Sort = 'title' | 'added'

/** A sorted copy — never the caller's array reordered under it. */
export function sorted(series: SeriesCard[], sort: Sort): SeriesCard[] {
  return [...series].sort((a, b) =>
    sort === 'added' ? b.added - a.added : a.title.localeCompare(b.title),
  )
}
```

- [ ] **Step 5: Run the tests**

Run: `cd web && pnpm vitest run --project node src/library.test.ts`
Expected: PASS, all nine.

- [ ] **Step 6: Commit**

```bash
git add web/src/library.ts web/src/library.test.ts web/src/api.ts
git commit -m "library.ts: the browse client, and the chip rules as pure functions"
```

---

### Task 6: `openWork` replaces `openFirstWork`

**Files:**
- Modify: `web/src/startup.ts` — `StartupPorts`, `openFirstWork` → `openWork`
- Modify: `web/src/startup.test.ts`
- Modify: `web/src/App.tsx:172` (import), `:256` (`works:` port), `:765` (call site)
- Modify: `web/src/App.test.tsx` — the `listWorks` port and its mock

**Interfaces:**
- Consumes: nothing.
- Produces: `openWork(ports: StartupPorts, slug: string): Promise<Opened>`; `StartupPorts` **loses** its `works` field. Task 7 calls it through `App`.

- [ ] **Step 1: Rewrite `startup.test.ts`'s expectations**

In `web/src/startup.test.ts`, replace every `openFirstWork(ports)` call with
`openWork(ports, 'the-work')`, drop `works` from every ports object, and delete
the test asserting `'no works available'` — with a slug in hand there is no such
state; a slug that does not exist is a failing sidecar read. Add:

```ts
it('reads the sidecar for the slug it was given, not the first work listed', async () => {
  const ports = portsFor()
  await openWork(ports, 'gotoubun-v03')
  expect(ports.sidecar).toHaveBeenCalledWith('gotoubun-v03')
  expect(ports.progress).toHaveBeenCalledWith('gotoubun-v03')
  expect(ports.notes).toHaveBeenCalledWith('gotoubun-v03')
})

it('rejects when the sidecar read fails, rather than opening something else', async () => {
  const ports = portsFor()
  ports.sidecar.mockRejectedValue(new Error('loading x: 404 no work `x`'))
  await expect(openWork(ports, 'x')).rejects.toThrow(/404/)
})
```

(`portsFor` is that file's existing helper — reuse it, minus `works`.)

- [ ] **Step 2: Run and watch it fail**

Run: `cd web && pnpm vitest run --project node src/startup.test.ts`
Expected: FAIL — `openWork` is not exported.

- [ ] **Step 3: Change `startup.ts`**

Delete the `works(): Promise<WorkListing[]>` field from `StartupPorts`, delete
the now-unused `WorkListing` type and its `WorkSummary` import if nothing else
uses them, and replace the function:

```ts
/**
 * Open one work by slug.
 *
 * **Was `openFirstWork`, which read `/api/works` and took `[0]`.** That was
 * the only way to reach a work when the reader was handed sidecar paths on
 * argv; spec C1's browse UI knows which work it wants, and `serve --db` serves
 * every row, so listing them to discard all but one was a request whose answer
 * was thrown away.
 *
 * Throws for a work that defines no editions — a state with no reader to show,
 * so there is nothing to degrade to — and lets a failing sidecar read reject,
 * which is what a slug that is not in the library looks like. The notes read is
 * still the one failure that does not stop the reader.
 *
 * **The order of the four reads is load-bearing** and is not an accident of how
 * this was written. The resolver is built before progress is read, so a failure
 * parsing the sidecar surfaces before a second request is made — and because
 * `restore` needs the resolver itself, to check that a stored slot still exists
 * rather than trusting stale JSON outright. Notes are read after progress
 * because neither depends on the other and the one that can fail should not be
 * in front of the one that cannot.
 */
export async function openWork(
  ports: StartupPorts,
  slug: string,
): Promise<Opened> {
  const fetched = await ports.sidecar(slug)
  const sidecar: SidecarShape = JSON.parse(fetched.json)
  const editions = orderedEditions(sidecar.hub, Object.keys(sidecar.editions))
  const [hub] = editions
  if (!hub) throw new Error(`${slug} defines no editions`)
  const resolver = await ports.resolve(fetched.json)
  const progress = await ports.progress(slug)
  let notesError: unknown
  const notes = await ports.notes(slug).catch((err: unknown) => {
    notesError = err
    return undefined
  })
  const state = restore(progress, hub, resolver)
  return {
    session: {
      work: slug,
      resolver,
      reading: sidecar.work.reading,
      editions,
      alignments: sidecar.alignments,
      state,
      previous: initialPrevious(editions, state.authority),
    },
    etag: fetched.etag,
    notes: notes?.doc ?? { work: slug, notes: [] },
    notesEtag: notes?.etag,
    notesError,
  }
}
```

- [ ] **Step 4: Update `App.tsx`**

Three edits, and no others inside that file:

- line ~7: drop `listWorks` from the `./api` import if nothing else in the file uses it
- line ~172: `openFirstWork` → `openWork`
- line ~256: delete `works: listWorks,` from `STARTUP_PORTS`
- line ~765: `openFirstWork(STARTUP_PORTS)` → `openWork(STARTUP_PORTS, work)`

`work` is the prop Task 7 adds. Until then, TypeScript will fail here — that is
expected, and Task 7 is the next task. To keep this task's commit compiling,
add the prop now as part of this step:

```tsx
export interface AppProps {
  /** The slug in the URL. `<App key={work}>` is what makes a change remount. */
  work: string
}

export default function App({ work }: AppProps) {
```

and in `web/src/main.tsx`, pass a slug so the entry point still type-checks —
Task 7 replaces this with the router:

```tsx
createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <App work={new URLSearchParams(location.search).get('work') ?? ''} />
  </StrictMode>,
)
```

- [ ] **Step 5: Update `App.test.tsx`**

Delete `listWorks: vi.fn(),` from the hoisted ports object, delete
`listWorks: ports.listWorks,` from the `vi.mock('./api', …)` factory, delete the
`ports.listWorks.mockResolvedValue([...])` setup (~line 243) and rewrite the
test at ~line 504 that drove a failure through `listWorks`:

```ts
it('routes a failing sidecar read to the console rather than a blank reader', async () => {
  ports.getSidecarWithEtag.mockRejectedValue(new Error('no server'))
  // …the rest of that test unchanged.
})
```

Every `mount(<App />)` in the file becomes `mount(<App work={WORK} />)`, using
that file's existing `WORK` constant.

- [ ] **Step 6: Run the web suite**

Run: `cd web && pnpm vitest run --project node --project jsdom`
Expected: PASS.

Run: `cd web && pnpm typecheck`
Expected: no errors.

- [ ] **Step 7: Commit**

```bash
git add web/src/startup.ts web/src/startup.test.ts web/src/App.tsx web/src/App.test.tsx web/src/main.tsx
git commit -m "openWork(slug) replaces openFirstWork, and App takes the work it opens"
```

---

### Task 7: `react-router`, the four routes, and the bundle measurement

**Files:**
- Modify: `web/package.json`, `web/pnpm-lock.yaml`
- Modify: `web/src/main.tsx`
- Create: `web/src/Routes.tsx`
- Create: `web/src/Routes.test.tsx`

**Interfaces:**
- Consumes: Task 6's `App({ work })`.
- Produces: `<AppRoutes />` mounting `/`, `/series/:id`, `/search`, `/read/:work`; `App` gains `onLeave: () => void`. Tasks 8–10 fill the screens.

- [ ] **Step 1: Measure the bundle before**

Run: `cd web && pnpm build`
Record the reported `dist/assets/*.js` size **and** its gzip figure. Write both
into the commit message in Step 8 and into `docs/status.md` in Task 11. This is
§13's first unknown and the measurement is the point of the step.

- [ ] **Step 2: Install `react-router`**

Run: `cd web && pnpm add react-router`
Expected: `react-router` appears under `dependencies` in `web/package.json`.

- [ ] **Step 3: Write the failing test**

Create `web/src/Routes.test.tsx`:

```tsx
import { MemoryRouter } from 'react-router'
import { describe, expect, it, vi } from 'vitest'
import { AppRoutes } from './Routes'
import { mount } from './testing/mount'

vi.mock('./App', () => ({
  default: ({ work }: { work: string }) => <div data-testid="reader">{work}</div>,
}))
// Task 10 of this plan extends this mock with an `onLeave` button.
vi.mock('./library', () => ({
  home: vi.fn().mockResolvedValue({ continuing: [], added: [], read: [] }),
  grid: vi.fn().mockResolvedValue({ series: [], unfiled: [] }),
  status: vi.fn().mockResolvedValue({ roots: [], scanned: true }),
  matchesChip: () => true,
  sorted: (s: unknown[]) => s,
}))

const at = (path: string) =>
  mount(
    <MemoryRouter initialEntries={[path]}>
      <AppRoutes />
    </MemoryRouter>,
  )

describe('the four routes', () => {
  it('opens the reader on the work in the URL', () => {
    const host = at('/read/gotoubun-v01')
    expect(host.querySelector('[data-testid="reader"]')?.textContent).toBe(
      'gotoubun-v01',
    )
  })

  it('does not mount the reader anywhere else', () => {
    expect(at('/').querySelector('[data-testid="reader"]')).toBeNull()
  })

  it('answers an unknown path with the library rather than the reader', () => {
    const host = at('/nonsense')
    // Asserted structurally rather than by text: the `Library` this task
    // mounts is Task 8's placeholder and has no text content yet.
    expect(host.querySelector('main')).not.toBeNull()
    expect(host.querySelector('[data-testid="reader"]')).toBeNull()
  })
})
```

- [ ] **Step 4: Run and watch it fail**

Run: `cd web && pnpm vitest run --project jsdom src/Routes.test.tsx`
Expected: FAIL — cannot resolve `./Routes`.

- [ ] **Step 5: Write `Routes.tsx`**

Create `web/src/Routes.tsx`:

```tsx
/**
 * The four routes, and the one place `App` is mounted.
 *
 * Spec C1 §1. `react-router` is the first runtime dependency beyond React
 * since the project began, taken over a hand-rolled fifty-line router because
 * scroll restoration, nested routes and navigation blocking are all things
 * this UI wants and all things that are tedious to get right twice.
 *
 * **`key={work}` is the whole reason `App.tsx` did not have to become
 * re-entrant.** A changing key unmounts and remounts, so that file's "loads
 * exactly once" assumption stays true rather than becoming a lie maintained by
 * hand. The wasm module is a module-level ES import, so it initialises once per
 * *page* and a remount costs a sidecar fetch and a resolver construction.
 */
import { Navigate, Route, Routes, useNavigate, useParams } from 'react-router'
import App from './App'
import { Library } from './Library'

/** The reader, keyed by the slug in the URL. */
function Reader() {
  const { work } = useParams()
  const navigate = useNavigate()
  if (!work) return <Navigate to="/" replace />
  return <App key={work} work={work} onLeave={() => void navigate('/')} />
}

/** Plan B builds these two. Named rather than blank, so the seam is visible. */
function NotBuiltYet({ what }: { what: string }) {
  return (
    <main>
      <h1>{what}</h1>
      <p>Not built yet.</p>
    </main>
  )
}

export function AppRoutes() {
  return (
    <Routes>
      <Route path="/" element={<Library />} />
      <Route path="/series/:id" element={<NotBuiltYet what="Series" />} />
      <Route path="/search" element={<NotBuiltYet what="Search" />} />
      <Route path="/read/:work" element={<Reader />} />
      <Route path="*" element={<Navigate to="/" replace />} />
    </Routes>
  )
}
```

Create a minimal `web/src/Library.tsx` for now — Task 8 replaces its body:

```tsx
/** The library grid. Task 8 of this plan builds it; §11 step 6. */
export function Library() {
  return <main aria-busy="true" />
}
```

**`App.tsx` is not touched in this task.** `Reader` above passes only `work`,
and its `onLeave` line is written in Task 10, which adds the prop, the plan
case and the control together — a prop nothing reads would be dead weight in
this commit and a lint failure in the next.

So in this task `Reader`'s last line is:

```tsx
  return <App key={work} work={work} />
```

and `useNavigate` is not imported yet. Task 10 restores both.

- [ ] **Step 6: Point the entry point at the router**

Replace `web/src/main.tsx`:

```tsx
import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { BrowserRouter } from 'react-router'
import './index.css'
import { AppRoutes } from './Routes.tsx'

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <BrowserRouter>
      <AppRoutes />
    </BrowserRouter>
  </StrictMode>,
)
```

- [ ] **Step 7: Run the tests and measure the bundle after**

Run: `cd web && pnpm vitest run --project jsdom src/Routes.test.tsx`
Expected: PASS.

Run: `cd web && pnpm build`
Record the new JS and gzip sizes. The delta is what `react-router` costs.
**If it is large enough to matter, stop and report it** — §13 says the
fifty-line hand-rolled router is still on the table and only the routing section
of the spec changes.

- [ ] **Step 8: Commit**

```bash
git add web/package.json web/pnpm-lock.yaml web/src/main.tsx web/src/Routes.tsx web/src/Routes.test.tsx web/src/Library.tsx web/src/App.tsx
# Substitute the four figures measured in Steps 1 and 7. This repository states
# measured numbers, and an unfilled placeholder here is a figure nobody took.
git commit -m "react-router and the four routes: the bundle went from <before> to <after> (<gz-before> to <gz-after> gzipped)"
```

---

### Task 8: the shell — sidebar, chips, continue strip

**Files:**
- Modify: `web/src/Library.tsx`
- Create: `web/src/Library.test.tsx`
- Create: `web/src/Sidebar.tsx`
- Create: `web/src/Sidebar.test.tsx`
- Modify: `web/src/index.css`

**Interfaces:**
- Consumes: Task 5's `home`, `grid`, `status`, `matchesChip`, `sorted`; Task 7's routes.
- Produces: `<Library />` and `<Sidebar roots={RootCard[]} showRoots={boolean} />`. Task 9 adds the empty states to `Library`.

- [ ] **Step 1: Write the failing tests**

Create `web/src/Library.test.tsx`:

```tsx
import { act } from 'react'
import { MemoryRouter } from 'react-router'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import * as library from './library'
import { Library } from './Library'
import { mount } from './testing/mount'

vi.mock('./library', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./library')>()),
  home: vi.fn(),
  grid: vi.fn(),
  status: vi.fn(),
}))

const series = (over: Partial<library.SeriesCard> = {}): library.SeriesCard => ({
  id: 1,
  title: 'A Series',
  works: 3,
  editions: 3,
  aligned: 1,
  added: 10,
  unread: 3,
  cover: '/api/library/series/1/cover',
  ...over,
})

const work = (over: Partial<library.WorkCard> = {}): library.WorkCard => ({
  id: 1,
  slug: 'a-series-v01',
  title: 'A Series v01',
  volume: 1,
  aligned: true,
  pages: 180,
  editions: [{ key: 'raw', label: 'Raw', members: 1, pages: 180 }],
  ...over,
})

/**
 * Mount and let the three loads settle.
 *
 * Two flushes, not one: the effect fires a `Promise.all` over three mocked
 * promises and *then* sets state, so a single microtask turn lands between the
 * resolution and the re-render and the assertions run against a screen that has
 * not drawn yet.
 */
const show = async () => {
  let host!: HTMLElement
  await act(async () => {
    host = mount(
      <MemoryRouter>
        <Library />
      </MemoryRouter>,
    )
  })
  await act(async () => {
    await Promise.resolve()
    await Promise.resolve()
  })
  return host
}

describe('the library screen', () => {
  beforeEach(() => {
    vi.mocked(library.status).mockResolvedValue({ roots: [], scanned: true })
    vi.mocked(library.home).mockResolvedValue({
      continuing: [],
      added: [],
      read: [],
    })
    vi.mocked(library.grid).mockResolvedValue({ series: [], unfiled: [] })
  })

  it('lists every series it was given, with a link to each', async () => {
    vi.mocked(library.grid).mockResolvedValue({
      series: [series({ id: 1, title: 'One' }), series({ id: 2, title: 'Two' })],
      unfiled: [],
    })
    const host = await show()
    const links = [...host.querySelectorAll('a[href^="/series/"]')]
    expect(links.map((a) => a.getAttribute('href'))).toEqual([
      '/series/1',
      '/series/2',
    ])
  })

  it('marks how many volumes of a series are aligned', async () => {
    vi.mocked(library.grid).mockResolvedValue({
      series: [series({ aligned: 3 })],
      unfiled: [],
    })
    const host = await show()
    expect(host.textContent).toContain('3')
  })

  it('links an unfiled work straight into the reader, since it has no series page', async () => {
    vi.mocked(library.grid).mockResolvedValue({
      series: [],
      unfiled: [work({ slug: 'imported-01' })],
    })
    const host = await show()
    expect(host.querySelector('a[href="/read/imported-01"]')).not.toBeNull()
  })

  it('the multi-edition chip keeps only the series with a second edition', async () => {
    vi.mocked(library.grid).mockResolvedValue({
      series: [
        series({ id: 1, title: 'Single', works: 3, editions: 3 }),
        series({ id: 2, title: 'Double', works: 3, editions: 5 }),
      ],
      unfiled: [],
    })
    const host = await show()
    const chip = [...host.querySelectorAll('button')].find((b) =>
      /multi/i.test(b.textContent ?? ''),
    )
    expect(chip, 'a multi-edition chip').toBeDefined()
    await act(async () => {
      chip?.click()
      await Promise.resolve()
    })
    expect(host.textContent).toContain('Double')
    expect(host.textContent).not.toContain('Single')
  })

  it('the continue strip links to what was being read', async () => {
    vi.mocked(library.home).mockResolvedValue({
      continuing: [work({ slug: 'half-read-v02' })],
      added: [],
      read: [],
    })
    const host = await show()
    expect(host.querySelector('a[href="/read/half-read-v02"]')).not.toBeNull()
  })

  it('draws no continue strip when nothing is half-read', async () => {
    const host = await show()
    expect(host.querySelector('[data-testid="continue"]')).toBeNull()
  })
})
```

Create `web/src/Sidebar.test.tsx`:

```tsx
import { MemoryRouter } from 'react-router'
import { describe, expect, it } from 'vitest'
import { Sidebar } from './Sidebar'
import { mount } from './testing/mount'

const at = (ui: React.ReactNode) => mount(<MemoryRouter>{ui}</MemoryRouter>)

describe('the sidebar', () => {
  it('always offers a way to the library and to search', () => {
    const host = at(<Sidebar roots={[]} showRoots={false} />)
    expect(host.querySelector('a[href="/"]')).not.toBeNull()
    expect(host.querySelector('a[href="/search"]')).not.toBeNull()
  })

  it('hides the roots section when it is switched off', () => {
    const host = at(
      <Sidebar roots={[{ id: 1, name: 'Manga' }]} showRoots={false} />,
    )
    expect(host.textContent).not.toContain('Manga')
  })

  it('filters the grid by root, and never by a path only the server knows', () => {
    const host = at(
      <Sidebar roots={[{ id: 4, name: 'Manga' }]} showRoots={true} />,
    )
    const link = host.querySelector('a[href="/?root=4"]')
    expect(link?.textContent).toBe('Manga')
  })
})
```

- [ ] **Step 2: Run them and watch them fail**

Run: `cd web && pnpm vitest run --project jsdom src/Library.test.tsx src/Sidebar.test.tsx`
Expected: FAIL — `Sidebar` does not exist; `Library` renders an empty `<main>`.

- [ ] **Step 3: Write `Sidebar.tsx`**

```tsx
/**
 * The persistent navigation — spec C1 §2.
 *
 * **`ROOTS` is provisional and is marked as such here rather than discovered
 * later.** A root is a *storage* fact, and spec B's whole argument is that
 * where a file sits should stop being how you find it. So the rule that keeps
 * it harmless: **no navigation path may require it.** Everything reachable
 * through a root link is reachable from the grid without one. If it turns out
 * to be an administrative tool it moves to C4 and nothing breaks.
 *
 * `showRoots` is a prop rather than a preference read here: the `browse` module
 * that makes it a setting is §11 step 8, in Plan B. Its caller passes the
 * default — on for an admin, off for everyone else.
 */
import { Link } from 'react-router'
import type { RootCard } from './library'

export interface SidebarProps {
  roots: RootCard[]
  showRoots: boolean
}

export function Sidebar({ roots, showRoots }: SidebarProps) {
  return (
    <nav className="sidebar" aria-label="Library">
      <ul className="sidebar-primary">
        <li>
          <Link to="/">Library</Link>
        </li>
        <li>
          <Link to="/search">Search</Link>
        </li>
      </ul>
      {showRoots && roots.length > 0 && (
        <>
          <h2 className="sidebar-heading">Roots</h2>
          <ul className="sidebar-roots">
            {roots.map((root) => (
              <li key={root.id}>
                <Link to={`/?root=${root.id}`}>{root.name}</Link>
              </li>
            ))}
          </ul>
        </>
      )}
    </nav>
  )
}
```

- [ ] **Step 4: Write `Library.tsx`**

```tsx
/**
 * The library screen: the sidebar, the chips, the continue strip and the grid.
 *
 * Spec C1 §2 and §11 step 6. **No virtualisation** — §7's measured numbers are
 * 127 series and 111 volumes in the largest series, and covers carry
 * `loading="lazy"`, which is free. The figure that would change that is a
 * chapter-organised edition, and it should be produced rather than imagined.
 *
 * The chips filter and sort **client-side**, over data already held: §7's
 * numbers make a `?sort=` round trip a request for something the browser does
 * instantly. `?root=` is a query parameter for the opposite reason — it
 * changes which rows exist, and which rows exist is the grant's business.
 */
import { useEffect, useState } from 'react'
import { Link, useSearchParams } from 'react-router'
import {
  type Chip,
  type Grid,
  grid as fetchGrid,
  home as fetchHome,
  type HomeLists,
  type LibraryStatus,
  matchesChip,
  type Sort,
  sorted,
  status as fetchStatus,
} from './library'
import { Sidebar } from './Sidebar'

const CHIPS: { id: Chip; label: string }[] = [
  { id: 'all', label: 'All' },
  { id: 'multi', label: 'Multi-edition' },
  { id: 'unread', label: 'Unread' },
]

export function Library() {
  const [params] = useSearchParams()
  const rootParam = params.get('root')
  const root = rootParam === null ? undefined : Number(rootParam)

  const [gridData, setGrid] = useState<Grid | undefined>()
  const [home, setHome] = useState<HomeLists | undefined>()
  const [status, setStatus] = useState<LibraryStatus | undefined>()
  const [failure, setFailure] = useState<string | undefined>()
  const [chip, setChip] = useState<Chip>('all')
  const [sort, setSort] = useState<Sort>('title')

  useEffect(() => {
    let cancelled = false
    Promise.all([fetchGrid(root), fetchHome(), fetchStatus()])
      .then(([grid, home, status]) => {
        if (cancelled) return
        setGrid(grid)
        setHome(home)
        setStatus(status)
      })
      .catch((err: unknown) => {
        if (cancelled) return
        setFailure(err instanceof Error ? err.message : String(err))
      })
    return () => {
      cancelled = true
    }
  }, [root])

  const visible = gridData
    ? sorted(
        gridData.series.filter((s) => matchesChip(s, chip)),
        sort,
      )
    : []

  return (
    <div className="library">
      <Sidebar roots={status?.roots ?? []} showRoots={false} />
      <main>
        {failure !== undefined && <p role="alert">{failure}</p>}
        {home && home.continuing.length > 0 && (
          <section data-testid="continue" className="continue">
            <h2>Continue</h2>
            <ul>
              {home.continuing.map((w) => (
                <li key={w.slug}>
                  <Link to={`/read/${w.slug}`}>{w.title}</Link>
                </li>
              ))}
            </ul>
          </section>
        )}
        <div className="chips">
          {CHIPS.map((c) => (
            <button
              key={c.id}
              type="button"
              aria-pressed={chip === c.id}
              onClick={() => setChip(c.id)}
            >
              {c.label}
            </button>
          ))}
          <button
            type="button"
            aria-pressed={sort === 'added'}
            onClick={() => setSort(sort === 'added' ? 'title' : 'added')}
          >
            Added
          </button>
        </div>
        <ul className="grid">
          {visible.map((s) => (
            <li key={s.id}>
              <Link to={`/series/${s.id}`}>
                <img src={s.cover} alt="" loading="lazy" />
                <span className="title">{s.title}</span>
                {s.aligned > 0 && <span className="aligned">⇄ {s.aligned}</span>}
              </Link>
            </li>
          ))}
          {gridData?.unfiled.map((w) => (
            <li key={w.slug}>
              {/* An unfiled work has no series page — §5.4 — so it links
                  straight into the reader. */}
              <Link to={`/read/${w.slug}`}>
                <span className="title">{w.title}</span>
              </Link>
            </li>
          ))}
        </ul>
      </main>
    </div>
  )
}
```

- [ ] **Step 5: Style it**

Append to `web/src/index.css` a `.library` grid (sidebar column plus main), a
`.grid` using `display: grid` with `grid-template-columns:
repeat(auto-fill, minmax(9rem, 1fr))`, `.chips` as a flex row, and
`.continue ul` as a single scrolling row. Keep it to the variables already in
that file.

- [ ] **Step 6: Run the tests**

Run: `cd web && pnpm vitest run --project jsdom src/Library.test.tsx src/Sidebar.test.tsx`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add web/src/Library.tsx web/src/Library.test.tsx web/src/Sidebar.tsx web/src/Sidebar.test.tsx web/src/index.css
git commit -m "The shell: a sidebar, three chips, a continue strip and a grid that filters client-side"
```

---

### Task 9: the empty states

**Files:**
- Modify: `web/src/Library.tsx`
- Create: `web/src/EmptyState.tsx`
- Create: `web/src/EmptyState.test.tsx`
- Modify: `web/src/Library.test.tsx`

**Interfaces:**
- Consumes: Task 4's `/api/library/status`; Task 8's `Library`.
- Produces: `<EmptyState kind={'no-roots' | 'not-scanned' | 'no-matches'} query?: string />`.

- [ ] **Step 1: Write the failing tests**

Create `web/src/EmptyState.test.tsx`:

```tsx
import { describe, expect, it } from 'vitest'
import { EmptyState } from './EmptyState'
import { mount } from './testing/mount'

describe('the empty states', () => {
  it('tells a new deployment how to add a root and what a scan will do', () => {
    const host = mount(<EmptyState kind="no-roots" />)
    expect(host.textContent).toContain('library scan --add')
  })

  it('says a configured library has not been read yet, which is not the same thing', () => {
    const host = mount(<EmptyState kind="not-scanned" />)
    expect(host.textContent).toContain('library scan')
    expect(host.textContent).not.toContain('--add')
  })

  it('names what was searched, since FTS5 on CJK can match nothing surprisingly', () => {
    const host = mount(<EmptyState kind="no-matches" query="五等分の花嫁" />)
    expect(host.textContent).toContain('五等分の花嫁')
  })

  it('renders something for every kind — none of them is a null return', () => {
    for (const kind of ['no-roots', 'not-scanned', 'no-matches'] as const) {
      expect(mount(<EmptyState kind={kind} />).textContent).not.toBe('')
    }
  })
})
```

Append to `web/src/Library.test.tsx`:

```tsx
it('a library with no roots is told so, rather than shown an empty grid', async () => {
  vi.mocked(library.status).mockResolvedValue({ roots: [], scanned: false })
  const host = await show()
  expect(host.textContent).toContain('library scan --add')
})

it('a configured but unscanned library is told the other thing', async () => {
  vi.mocked(library.status).mockResolvedValue({
    roots: [{ id: 1, name: 'Manga' }],
    scanned: false,
  })
  const host = await show()
  expect(host.textContent).not.toContain('--add')
  expect(host.textContent).toContain('library scan')
})

it('a scanned library with no series says nothing matched, not that it is unconfigured', async () => {
  vi.mocked(library.status).mockResolvedValue({
    roots: [{ id: 1, name: 'Manga' }],
    scanned: true,
  })
  const host = await show()
  expect(host.textContent).not.toContain('library scan')
})
```

- [ ] **Step 2: Run and watch them fail**

Run: `cd web && pnpm vitest run --project jsdom src/EmptyState.test.tsx src/Library.test.tsx`
Expected: FAIL — cannot resolve `./EmptyState`.

- [ ] **Step 3: Write `EmptyState.tsx`**

```tsx
/**
 * The screens someone sees on their first run — spec C1 §8.
 *
 * **None of them is a `null` return.** "No roots configured" and "roots
 * configured, scan not yet run" are told apart by `/api/library/status` and by
 * nothing else: `/api/library/series` answers `{ series: [], unfiled: [] }` in
 * both states, which is correct of it and useless here. The difference is
 * between "you have no library" and "your library has not been read yet", and
 * the person seeing it can act on exactly one of those.
 */
export type EmptyKind = 'no-roots' | 'not-scanned' | 'no-matches'

export interface EmptyStateProps {
  kind: EmptyKind
  /** What was searched, for `no-matches`. */
  query?: string
}

export function EmptyState({ kind, query }: EmptyStateProps) {
  if (kind === 'no-roots') {
    return (
      <section className="empty">
        <h2>No library yet</h2>
        <p>
          Nothing has been added to this server. Point it at a directory of
          archives:
        </p>
        <pre>taiyaku library scan --add /path/to/manga</pre>
        <p>
          A scan reads what is on disk — series, volumes, editions and page
          counts — without writing anything back into the tree.
        </p>
      </section>
    )
  }
  if (kind === 'not-scanned') {
    return (
      <section className="empty">
        <h2>Not read yet</h2>
        <p>This library has roots, and nothing has scanned them.</p>
        <pre>taiyaku library scan</pre>
      </section>
    )
  }
  return (
    <section className="empty">
      <h2>Nothing here</h2>
      <p>
        {query === undefined || query === ''
          ? 'No series match this filter.'
          : `Nothing matched ${query}.`}
      </p>
    </section>
  )
}
```

- [ ] **Step 4: Wire it into `Library.tsx`**

Replace the `<ul className="grid">` block with a branch that chooses first:

```tsx
        {gridData && status && visible.length === 0 && gridData.unfiled.length === 0 ? (
          <EmptyState
            kind={
              status.roots.length === 0
                ? 'no-roots'
                : status.scanned
                  ? 'no-matches'
                  : 'not-scanned'
            }
          />
        ) : (
          <ul className="grid">
            {/* …the list from Task 8, unchanged… */}
          </ul>
        )}
```

and import `EmptyState` at the top.

- [ ] **Step 5: Run the tests**

Run: `cd web && pnpm vitest run --project jsdom`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add web/src/EmptyState.tsx web/src/EmptyState.test.tsx web/src/Library.tsx web/src/Library.test.tsx
git commit -m "The three empty states, and the route that is the only thing able to tell two of them apart"
```

---

### Task 10: the reader's way back out

**Files:**
- Modify: `web/src/keys.ts` — `Intent`, `READER_KEYS`
- Modify: `web/src/keys.test.ts`
- Modify: `web/src/keyplan.ts` — `KeyAction`
- Modify: `web/src/keyplan.test.ts`
- Modify: `web/src/Chrome.tsx`, `web/src/Chrome.test.tsx`
- Modify: `web/src/App.tsx`, `web/src/App.test.tsx`
- Modify: `web/src/Routes.tsx`, `web/src/Routes.test.tsx`

**Interfaces:**
- Consumes: Task 7's `Reader`.
- Produces: `App({ work, onLeave })`; `Intent` gains `{ kind: 'library' }`; `KeyAction` gains `{ kind: 'leave-reader' }`; `ChromeProps` gains `onLeave: () => void`.

- [ ] **Step 1: Write the failing tests**

In `web/src/keys.test.ts`:

```ts
it('L asks to go back to the library', () => {
  expect(
    intentOf(new KeyboardEvent('keydown', { key: 'L' }), ['raw'], []),
  ).toEqual({ kind: 'library' })
})

it('the library key is in the one table the help screen is generated from', () => {
  const row = READER_KEYS.find((b) => b.keys.includes('L'))
  expect(row?.label, 'a label, or Help cannot describe it').toBeTruthy()
})
```

In `web/src/keyplan.test.ts`, mirroring that file's existing `open-account`
cases:

```ts
it('L leaves the reader', () => {
  expect(plan('L', { kind: 'library' })).toEqual({
    action: { kind: 'leave-reader' },
    preventDefault: true,
  })
})

it('L does nothing while the account screen is open', () => {
  expect(plan('L', { kind: 'library' }, { accountOpen: true })).toBeUndefined()
})
```

(`plan` is that file's existing helper — match its signature.)

In `web/src/Routes.test.tsx` — add `act` to its `react` import first, which
Task 7 did not need:

```tsx
it('leaving the reader navigates back to the library', async () => {
  const host = at('/read/gotoubun-v01')
  expect(host.querySelector('[data-testid="reader"]')).not.toBeNull()
  // The mocked App exposes its `onLeave` as a button.
  await act(async () => {
    host.querySelector<HTMLButtonElement>('[data-testid="leave"]')?.click()
    await Promise.resolve()
  })
  expect(host.querySelector('[data-testid="reader"]')).toBeNull()
})
```

and extend that file's `vi.mock('./App', …)`:

```tsx
vi.mock('./App', () => ({
  default: ({ work, onLeave }: { work: string; onLeave: () => void }) => (
    <div data-testid="reader">
      {work}
      <button type="button" data-testid="leave" onClick={onLeave}>
        leave
      </button>
    </div>
  ),
}))
```

- [ ] **Step 2: Run and watch them fail**

Run: `cd web && pnpm vitest run --project node --project jsdom src/keys.test.ts src/keyplan.test.ts src/Routes.test.tsx`
Expected: FAIL — no `library` intent.

- [ ] **Step 3: Add the intent and the binding**

In `web/src/keys.ts`, add to `Intent`:

```ts
  /** Leave the reader for the library grid — spec C1 §6. */
  | { kind: 'library' }
```

and a row to `READER_KEYS`, after the account row:

```ts
  {
    keys: ['L'],
    label: 'back to the library',
    resolve: () => ({ kind: 'library' }),
  },
```

**Capital `L`, and it goes through this table rather than anywhere else.**
`Help` is generated from this table and from the registry, so a binding added
elsewhere would be a key the help screen does not know about — which is the
drift `help` exists to prevent.

- [ ] **Step 4: Plan it**

In `web/src/keyplan.ts`, add to `KeyAction`:

```ts
  | { kind: 'leave-reader' }
```

and, in the same place the `open-account` case is decided and behind the same
`guards.accountOpen` early return:

```ts
  if (intent?.kind === 'library') {
    return { action: { kind: 'leave-reader' }, preventDefault: true }
  }
```

- [ ] **Step 5: Handle it, and give it a control**

In `web/src/App.tsx`, add the prop:

```tsx
export interface AppProps {
  work: string
  /** Back to the library — spec C1 §6. */
  onLeave: () => void
}

export default function App({ work, onLeave }: AppProps) {
```

add a case to the `switch (plan.action.kind)` block:

```tsx
        case 'leave-reader':
          return onLeave()
```

and pass it to `Chrome`: `onLeave={onLeave}`.

In `web/src/Chrome.tsx`, add `onLeave: () => void` to `ChromeProps` and render a
control beside the existing ones:

```tsx
      <button type="button" className="chrome-leave" onClick={onLeave}>
        Library
      </button>
```

In `web/src/Routes.tsx`, `Reader` passes it:

```tsx
  return <App key={work} work={work} onLeave={() => void navigate('/')} />
```

- [ ] **Step 6: Assert the remount, which is §1's whole argument**

In `web/src/App.test.tsx`:

```tsx
it('a changing work remounts rather than reloading in place', async () => {
  const host = mount(<App key="one" work="one" onLeave={noop} />)
  await settle()
  expect(ports.getSidecarWithEtag).toHaveBeenCalledWith('one')
  rerender(<App key="two" work="two" onLeave={noop} />)
  await settle()
  expect(ports.getSidecarWithEtag).toHaveBeenCalledWith('two')
  expect(host).toBeDefined()
})
```

using that file's existing settle helper and `rerender` from `./testing/mount`.
Update every other `mount(<App work={WORK} />)` in the file to pass
`onLeave={() => {}}`.

- [ ] **Step 7: Run the whole web suite**

Run: `cd web && pnpm vitest run --project node --project jsdom`
Expected: PASS.

Run: `cd web && pnpm lint && pnpm typecheck`
Expected: clean.

- [ ] **Step 8: Commit**

```bash
git add web/src/keys.ts web/src/keys.test.ts web/src/keyplan.ts web/src/keyplan.test.ts web/src/Chrome.tsx web/src/Chrome.test.tsx web/src/App.tsx web/src/App.test.tsx web/src/Routes.tsx web/src/Routes.test.tsx
git commit -m "L leaves the reader, through the one table the help screen is generated from"
```

---

### Task 11: the measurements, the docs, and the bookkeeping

**Files:**
- Modify: `docs/status.md`
- Modify: `README.md`, `web/README.md`
- Modify: `docs/superpowers/specs/2026-08-14-library-store-design.md` — §10's roadmap table
- Modify: `scripts/coverage-floor.mjs` if its per-file floors need moving

**Interfaces:**
- Consumes: everything above.
- Produces: a branch whose stated figures match what it measures.

- [ ] **Step 1: Read the Rust coverage margin**

Run: `cargo llvm-cov --workspace --fail-under-lines 93`
Record the percentage and the derived slack the report prints. C1 left it at
**93.28% on the runner, 39 covered lines of slack**, and the forge measures a
few lines worse than this machine — it failed at 92.99% where local said 93.04%.
If this branch's figure is under about 93.15% locally, add store tests before
pushing rather than after CI says so.

- [ ] **Step 2: Run the web coverage gate**

Run: `cd web && pnpm test:coverage`
Expected: the four aggregate bars pass. Then run `node scripts/coverage-floor.mjs`
from the repository root and fix any file that fell below its floor. The file
count moves — this branch adds six source files and six test files.

- [ ] **Step 3: Update `docs/status.md`**

Four edits:

- the `web/` row of the crate table — `react-router`, and the new files
- the dependency list — `react-router` is the first runtime dependency beyond
  React since the project began, with the measured bundle delta from Task 7
- the route list — `/api/library/status`, and `?root=` on the grid
- the test counts, from `scripts/status-test-count.mjs`

- [ ] **Step 4: Open the item this plan deliberately left**

Add an open item to `docs/status.md`'s table for §8's missing-from-disk state.
Closing or opening one is a five-part edit — the count in the section's opening
sentence ("Fifty-nine…"), the closed list and its "leaving twenty-six" tail, the
table row itself, and any cross-reference. The highest number in the table today
is **O61**, so this is **O62** — confirm with
`grep -o 'O6[0-9]' docs/status.md | sort -u | tail -1` before writing it, since
another branch may have taken it first:

> **A work whose members are gone from disk is offered and then fails at its
> first page.** Spec C1 §8 asks for it to be shown as unreadable. The fact is
> derivable — the scanner deletes `scan_entry` rows for files it no longer
> meets while `member` rows survive — but not cheaply: `member.path` is
> absolute and `scan_entry.rel_path` is relative to its root, so the join is
> over a computed string rather than an index. Spec C1 §5.5 defers it to C3
> with the rest of provenance.

- [ ] **Step 5: Annotate C1's roadmap row**

In `docs/superpowers/specs/2026-08-14-library-store-design.md` §10, C1's row is
still unannotated while the rest carry `*(shipped 2026-08-18, #76)*`. Leave it
unannotated until Plan B merges — this branch is half of C1 — but add the
server half's reference so the table stops being silent about it:

```
| **C1** *(server half shipped 2026-08-20, #81)* | browse API and UI: …
```

- [ ] **Step 6: Update the two READMEs**

`README.md`'s "To read in the browser" now describes a library rather than a
work. `web/README.md` gains the four routes. Both already needed the `serve
--db` invocation — check it is there and add it if the server half did not.

- [ ] **Step 7: Run the whole gate**

Run: `scripts/check-all.sh`
Expected: the closing line reads exactly `all green`. Quote it in the commit
message. Do not pipe this through `tail` and do not append `; echo` — both mask
a failure.

- [ ] **Step 8: Commit**

```bash
git add docs/status.md README.md web/README.md docs/superpowers/specs/2026-08-14-library-store-design.md
git commit -m "The figures this branch moved, the open item it left, and the roadmap row that was silent about C1"
```

---

## What this plan does not do

Plan B — §11 steps 7, 8 and 9 — builds the series page (grid and table over one
fixture, the edition selector, the panel that reads Task 3's alignments), the
`browse` module with its second registry and generated settings, and search with
§13's FTS5 tokenisation measured against real titles.

**At the end of this plan `/series/:id` renders "Not built yet".** The grid's
tiles link to it, the continue strip and the unfiled list open works, and search
is a stub. That is a checkpoint rather than a release: both plans land on this
one branch and the branch merges once.
