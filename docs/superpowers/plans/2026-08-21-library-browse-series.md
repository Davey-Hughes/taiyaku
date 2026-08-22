# Library browse — Plan B: the series page, the module, and search Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give the grid's tiles somewhere to go. `/series/:id` renders §3's two
presentations over one fixture with its two optional attachments, a `browse`
module turns §3's three switches into generated settings, and `/search` answers
a Japanese title typed mid-string — which the shipped tokeniser could not.

**Architecture:** One Rust migration replaces both FTS5 tables' tokeniser with
`trigram` and `browse::search` gains a `LIKE` path for queries too short for
one; the rest is `web/src`. `Series.tsx` is a grid and a table over one
`oneSeries` fetch, with an edition selector and an alignment panel that fetches
`work(slug)` on demand. `browseModule.ts` is a second registry — `buildRegistry`
and `settingsPanel.ts` reused verbatim, the list separate — and `Search.tsx`
reuses the grid `Library.tsx` already draws.

**Tech Stack:** Rust (rusqlite, axum, SQLite FTS5), TypeScript, React 19,
`react-router`, Vitest (`node` and `jsdom` projects), Biome.

## Global Constraints

- **Spec:** `docs/superpowers/specs/2026-08-14-library-browse-design.md`. This
  plan is its §11 "Plan B": steps 7, 8 and 9.
- **Plan A shipped as `b9581ac` (#82) and is on `main`.** §11 said both plans
  land on one branch; that is no longer true, and this plan gets its own branch
  off `main`. Nothing else about the seam changes.
- **Empty grant means unrestricted.** `roots: &[i64]` empty applies *no* filter
  (§5.1). Never invert this.
- **Every placeholder in `browse.rs` is anonymous `?`, never `?1`**, and
  parameters bind in the **text order of the statement**. A single `?3`
  renumbers the anonymous holes after it and `rusqlite` refuses the call. See
  that module's header, which was corrected on 2026-08-21: the old "roots
  first, then everything else" phrasing is no longer the rule.
- **Migrations are append-only.** Never edit `001`–`007`; `008` is a new file
  and `SCHEMA_VERSION` moves with it or the `const _: () = assert!` fails the
  build.
- **New guarded routes go in `guarded_routes()`**, never `open_routes()`, and
  must be added to the typed list in
  `crates/taiyaku-serve/src/auth/tests.rs::every_route_refuses_an_anonymous_request`.
  *(This plan adds no route — every endpoint it needs shipped in #81.)*
- **Nothing outside `App.tsx`'s own test file may value-import `resolver.ts`**
  (it pulls in `taiyaku-wasm`, generated into a gitignored directory). Browse
  components must not touch it.
- **`web/src/prefs.ts` is the only module that touches `localStorage`.**
- **The browse screens are light-and-dark UI chrome, not artwork.** Use CSS
  system colours (`Canvas`, `CanvasText`, `ButtonFace`, `ButtonBorder`,
  `LinkText`, `Highlight`) as `index.css`'s `.library` block does. Never a
  hardcoded hex. `Login.tsx` shipped `#111` on this branch's front door and had
  to be fixed in `a6450f3`; `Settings.tsx`'s dark panel is correct only because
  it draws over artwork.
- **`cargo fmt --all -- --check` is a hard CI gate** (`.forgejo/workflows/ci.yml:71`),
  running just before clippy. Task 1 shipped a commit that failed it on one
  line: CJK characters count as double-width, so an `assert_eq!` carrying a
  Japanese string literal goes past `max_width` where the same line in ASCII
  would not. Every task in this plan writes Japanese string literals into
  tests. Run `cargo fmt --all` before committing.
- **Coverage gates:** Rust `cargo llvm-cov --fail-under-lines 93`; web
  aggregate `statements 96 / branches 92 / functions 97 / lines 97`, plus the
  per-file floor and file count in `scripts/coverage-floor.mjs`
  (`minFiles: 59` web, `40` Rust).
- **Run web commands as `pnpm -C web …`, never `cd web && …`.** This shell's
  `cd` is zoxide: `cd web && x` can land somewhere else entirely and then run
  `x` there. The web job is four commands — `pnpm -C web run lint` (biome),
  `run typecheck` (`tsc -b`), `run test` (the `node` project, `src/**/*.test.ts`)
  and `run test:jsdom` (`src/**/*.test.tsx`). **Never plain `vitest run`**: it
  also starts the `browser` project, which needs a Chromium CI deliberately
  does not download.
- **The gate is `scripts/check-all.sh`.** Read its exit status directly — do not
  pipe it through `tail` or append `; echo`, both of which mask a failure. Do
  not run it with `TAIYAKU_CORPUS` set; use `env -u TAIYAKU_CORPUS`.
- **Report rather than repair.** If a step here looks wrong, stop and say so
  with the evidence. Every task on Plan A that under-counted callers was found
  this way, and every one that was worked around silently was found later and
  cost more.

## What this plan deliberately does not do

- **`?root=` does not reach `/search`.** `search()` calls
  `unfiled(conn, roots, None)` and `series(conn, roots, user_id, None)`, and
  both stay `None` here. Plan A carried this forward as "the second place
  `?root=` must reach"; it is a *decision* — whether a root link scopes search
  as well as the grid — and the sidebar's root links point at `/?root=`, not at
  `/search`, so nothing is inconsistent until someone decides otherwise. Task 2
  pins the current answer with a test rather than leaving it to argument.
- **`works_of` still passes `None` for `only` by argument, not by
  construction.** Threading `only` through it is what a search-by-root would
  need, and this plan does not build one. The carried warning stands.
- **`/api/library/home` gains no `?root=`.** The continue strip still spans
  every root in the grant while the grid beside it is narrowed. Spec follow-up,
  named in Plan A's ledger, not a handler bug.
- **O60 and O61 stay open.** Neither is this plan's; both are decisions
  recorded in `docs/status.md`.

---

### Task 1: Migration 8 — both FTS5 tables get the `trigram` tokeniser

**Files:**
- Create: `crates/taiyaku-library/src/schema/008_trigram_fts.sql`
- Modify: `crates/taiyaku-library/src/schema.rs` — `SCHEMA_VERSION` (line 30), `MIGRATIONS` (lines 36–44)
- Test: `crates/taiyaku-library/src/schema/tests.rs` — `the_schema_version_is_seven` (line 762), plus two new tests

**Interfaces:**
- Consumes: nothing.
- Produces: `series_fts` and `work_fts` tokenised by `trigram`, so
  `series_fts MATCH '"の花嫁"'` matches `五等分の花嫁`. Task 2 consumes this.
  `SCHEMA_VERSION` becomes `8` and `MIGRATIONS` has eight entries.

**Why trigram, measured rather than argued.** §13 left FTS5 tokenisation on
Japanese unmeasured and said the plan should settle it against real titles.
Measured 2026-08-21 against SQLite 3.53.4 — the same library `rusqlite` links,
since no `bundled` feature is enabled — with `五等分の花嫁` and
`かぐや様は告らせたい～天才たちの恋愛頭脳戦～` in a table built from
`001_initial.sql` verbatim. `fts5vocab` shows the default `unicode61` tokeniser
emits **each whole CJK run as one token**: the index holds exactly
`五等分の花嫁` and `かぐや様は告らせたい`, so nothing mid-string can ever match.

| query | `unicode61` (shipped) | `unicode61` + `*` | `trigram` |
| --- | --- | --- | --- |
| `五等分の花嫁` whole | ✅ | ✅ | ✅ |
| `五` / `五等` (1–2 chars) | ❌ | ✅ | ❌ |
| `五等分` (prefix, 3) | ❌ | ✅ | ✅ |
| `花嫁` (mid-string, 2) | ❌ | ❌ | ❌ |
| `の花嫁` / `告らせたい` (mid-string, 3+) | ❌ | ❌ | ✅ |
| `Hanay` (Latin prefix) | ❌ | ✅ | ✅ |

Trigram wins everywhere except queries under three characters, which it answers
with silence — including the first two keystrokes of every search. Task 2's
`LIKE` path is that hole filled, and the two together are the only combination
in the table with no ❌ in it. **User decision, 2026-08-21: build both.**

- [ ] **Step 1: Write the migration**

Create `crates/taiyaku-library/src/schema/008_trigram_fts.sql`:

```sql
-- Spec C1 §13's one unmeasured question, measured and then acted on.
--
-- **The default tokeniser emits each whole CJK run as a single token.**
-- Measured 2026-08-21 against SQLite 3.53.4 with `fts5vocab`: a table built
-- from `001_initial.sql` and given the real titles `五等分の花嫁` and
-- `かぐや様は告らせたい～天才たちの恋愛頭脳戦～` holds the terms
-- `五等分の花嫁`, `かぐや様は告らせたい` and `天才たちの恋愛頭脳戦` — whole
-- runs, split only where the `～` interrupts them. So `花嫁` matched nothing,
-- `告らせたい` matched nothing, and the only Japanese query that could ever
-- succeed was the entire run typed exactly. That is not a tuning problem; it
-- is the index having no smaller unit to offer.
--
-- `trigram` indexes every three-character window instead, which makes any
-- substring of three characters or more findable in either script and costs
-- the query under three characters, which now matches nothing at all.
-- `browse::search` answers those with `LIKE` rather than leaving them silent;
-- see its `TRIGRAM_MIN`.
--
-- **DROP and CREATE rather than ALTER**, because `tokenize=` is fixed at
-- creation. The triggers are untouched by this: they live on `series_title`
-- and `work`, not on the virtual tables, so they survive the drop and keep
-- pointing at the new tables by name. Rehearsed on 2026-08-21 against a
-- database built by applying 001–007 in order — after this migration an
-- INSERT and an UPDATE through the triggers both landed in the trigram index.
--
-- **`'rebuild'` rather than a hand-written INSERT SELECT.** These are
-- external-content tables, so `rebuild` re-reads the content table through
-- the same `content_rowid` mapping the triggers use; an INSERT SELECT would
-- be a second, hand-maintained copy of that mapping.
DROP TABLE series_fts;
DROP TABLE work_fts;

CREATE VIRTUAL TABLE series_fts USING fts5 (
  title, content = 'series_title', content_rowid = 'rowid', tokenize = 'trigram'
);
CREATE VIRTUAL TABLE work_fts USING fts5 (
  title, content = 'work', content_rowid = 'id', tokenize = 'trigram'
);

INSERT INTO series_fts (series_fts) VALUES ('rebuild');
INSERT INTO work_fts (work_fts) VALUES ('rebuild');
```

- [ ] **Step 2: Register it**

In `crates/taiyaku-library/src/schema.rs`, change the version and the array:

```rust
/// The schema version this build expects.
pub const SCHEMA_VERSION: u32 = 8;
```

```rust
const MIGRATIONS: [&str; 8] = [
    include_str!("schema/001_initial.sql"),
    include_str!("schema/002_absorbed.sql"),
    include_str!("schema/003_scanner.sql"),
    include_str!("schema/004_auth.sql"),
    include_str!("schema/005_note_id_per_user.sql"),
    include_str!("schema/006_member_root.sql"),
    include_str!("schema/007_backfill_member_root.sql"),
    include_str!("schema/008_trigram_fts.sql"),
];
```

- [ ] **Step 3: Update the version test and write the failing ones**

In `crates/taiyaku-library/src/schema/tests.rs`, rename and re-point the
existing assertion at line 762:

```rust
/// The version and the migration count are one fact, and this is the refusal
/// that depends on it staying true.
#[test]
fn the_schema_version_is_eight() {
    assert_eq!(SCHEMA_VERSION, 8);
}
```

Append two new tests to the same file:

```rust
/// Migration 8 makes a Japanese title findable by a substring of it, which is
/// what the default tokeniser could not do at any query length.
///
/// **`の花嫁` rather than `花嫁`.** Trigram indexes three-character windows, so
/// a two-character query matches nothing however correct the index is;
/// `browse::search` answers those with `LIKE` instead, and this test is about
/// the index rather than about that fallback.
#[test]
fn migration_8_finds_a_japanese_title_by_its_middle() {
    let conn = Connection::open_in_memory().expect("in-memory connection");
    conn.pragma_update(None, "foreign_keys", "ON")
        .expect("fk on");
    migrate(&conn).expect("migration");
    conn.execute_batch(
        "INSERT INTO series (id, sort_title) VALUES (1, 'gotoubun');
         INSERT INTO series_title (series_id, title) VALUES (1, '五等分の花嫁');
         INSERT INTO work (id, content_key, slug, series_id, title, reading, hub)
             VALUES (1, 'k', 's', 1, '小林さんちのメイドラゴン 第1巻', 'rtl', 'raw');",
    )
    .expect("two real titles");

    let hits = |table: &str, term: &str| -> i64 {
        conn.query_row(
            &format!("SELECT count(*) FROM {table} WHERE {table} MATCH ?"),
            [format!("\"{term}\"")],
            |row| row.get(0),
        )
        .expect("fts query")
    };

    assert_eq!(
        hits("series_fts", "の花嫁"),
        1,
        "a substring from the middle of the run must match"
    );
    assert_eq!(
        hits("series_fts", "五等分"),
        1,
        "and so must one from the start"
    );
    assert_eq!(
        hits("work_fts", "メイドラゴン"),
        1,
        "the work index is tokenised the same way"
    );
    assert_eq!(
        hits("series_fts", "花婿"),
        0,
        "a substring that is not there still matches nothing"
    );
}

/// The rebuild is not the end of it: the triggers must still be writing into
/// the tables migration 8 replaced.
///
/// **This is the failure the migration would otherwise hide.** `'rebuild'`
/// populates the index once, so every test that inserts before migrating
/// passes whether or not a trigger ever fires again. A row written *after*
/// the migration is the only one that proves it.
#[test]
fn migration_8_leaves_the_triggers_writing_into_the_new_tables() {
    let conn = Connection::open_in_memory().expect("in-memory connection");
    conn.pragma_update(None, "foreign_keys", "ON")
        .expect("fk on");
    migrate(&conn).expect("migration");
    conn.execute_batch(
        "INSERT INTO series (id, sort_title) VALUES (1, 'kaguya');
         INSERT INTO series_title (series_id, title) VALUES (1, 'かぐや様');
         UPDATE series_title SET title = 'ぼっち・ざ・ろっく!' WHERE series_id = 1;",
    )
    .expect("insert then rename, both after the migration");

    let hits = |term: &str| -> i64 {
        conn.query_row(
            "SELECT count(*) FROM series_fts WHERE series_fts MATCH ?",
            [format!("\"{term}\"")],
            |row| row.get(0),
        )
        .expect("fts query")
    };

    assert_eq!(hits("ざ・ろっ"), 1, "the update trigger wrote the new title");
    assert_eq!(hits("かぐや様"), 0, "and deleted the old one");
}
```

- [ ] **Step 4: Run them and watch the right things fail**

Run: `cargo test -p taiyaku-library schema::tests -- --nocapture`

Expected before the migration file is in place: a compile failure on the
`const _: () = assert!(MIGRATIONS.len() == SCHEMA_VERSION as usize)` if only
one of the two was changed. With both changed and the file created, expected:
all green.

**If `renaming_a_title_updates_the_search_index_for_both_tables` (line 318)
fails, stop and report it.** It was checked against this migration on
2026-08-21 and passes: it searches bare `New` and `Old`, three characters each,
and `Old` is not a substring of `New Series` or `New Work`. A failure there
means the migration did something other than what was rehearsed.

- [ ] **Step 5: Measure what the tokeniser costs the file**

Trigram indexes every three-character window, so the index is larger than one
over whole tokens and the figure belongs in `docs/notes.md` rather than in a
guess. Run against a real library:

```bash
env -u TAIYAKU_CORPUS cargo run -q -p taiyaku-cli -- library import corpus/gotoubun-v01/taiyaku.json --db /tmp/plan-b-size.db
sqlite3 /tmp/plan-b-size.db "SELECT name, SUM(pgsize) FROM dbstat WHERE name LIKE '%fts%' GROUP BY name ORDER BY 2 DESC;"
```

`--db` follows the subcommand, not `library` — the usage line is
`taiyaku library import <taiyaku.json> [--db PATH]`; check `library --help`
rather than trusting this plan if a command is refused.

**`dbstat` says nothing at small scale**, measured in this task on
2026-08-21: over a single imported work, the `*fts*` shadow tables report an
identical 32768 bytes before and after, because SQLite's per-table minimum page
allocation swamps the difference. `SELECT SUM(length(block)) FROM
work_fts_data;` carries the real signal — 69 → 216 bytes, 3.1×, over one
25-character title. Report both, and say plainly that the `dbstat` figure is
allocation rather than content. Task 11 takes the figure worth publishing, from
a scan of three works rather than an import of one.

Record both figures — before this task on a database at version 7, and after —
in the scratch report. Task 10 writes them into `docs/notes.md`. If `dbstat` is
unavailable in this SQLite build, use `SELECT SUM(length(block)) FROM
series_fts_data;` and say which measure was used.

- [ ] **Step 6: Run the crate's whole suite**

Run: `env -u TAIYAKU_CORPUS cargo test -p taiyaku-library`

Expected: PASS. `browse::tests` holds 41 tests, five of them search-named,
which ran against `unicode61` until now; every term they search (`Series`,
`Hanayome`, `Quintessential`, `Nakano`, `Volume05`) is three characters or more
and is a genuine substring of what it looks for, so they should pass unchanged.
**A test that now finds *more* than it asserts is the interesting failure** —
trigram matches substrings, so a query that used to match a whole token only
can now match inside another title. Report the assertion and the new count
rather than loosening it.

- [ ] **Step 7: Commit**

```bash
git add crates/taiyaku-library/src/schema/008_trigram_fts.sql crates/taiyaku-library/src/schema.rs crates/taiyaku-library/src/schema/tests.rs
git commit -m "migration 8: trigram tokeniser on both FTS5 tables"
```

---

### Task 2: `search` answers a query too short for trigram

**Files:**
- Modify: `crates/taiyaku-library/src/browse.rs` — `search()` (line 683), `matching_series` (725), `series_of_matching_works` (737), `matching_unfiled_works` (752), `ids` (768)
- Test: `crates/taiyaku-library/src/browse/tests.rs`

**Interfaces:**
- Consumes: Task 1's trigram tables.
- Produces: `browse::search(conn: &Connection, roots: &[i64], user_id: i64, q: &str) -> Result<SearchResults, String>` — **signature unchanged**. Task 10's search screen consumes it through the route that already exists.

- [ ] **Step 1: Write the failing tests**

Append to `crates/taiyaku-library/src/browse/tests.rs`:

```rust
/// A two-character Japanese query is a real word, and trigram answers it with
/// silence — so `search` answers it with `LIKE`.
///
/// `花嫁` is the second half of `五等分の花嫁` and two characters long: below
/// trigram's window, above nothing. Measured 2026-08-21: `series_fts MATCH
/// '"花嫁"'` returns no rows against an index that contains that exact title.
#[test]
fn a_query_too_short_for_trigram_is_answered_by_like() {
    let (library, _, _, _en, _raw) = two_root_library();
    let conn = library.conn();
    conn.execute(
        "INSERT INTO series_title (series_id, title, kind, is_primary)
         SELECT id, '五等分の花嫁', 'native', 0 FROM series LIMIT 1",
        [],
    )
    .expect("gives the series its native title");

    let hits = search(&conn, &[], 1, "花嫁").expect("searches");
    assert_eq!(
        hits.series.len(),
        1,
        "two characters must still find the shelf"
    );

    let long = search(&conn, &[], 1, "分の花嫁").expect("searches");
    assert_eq!(
        long.series.len(),
        1,
        "and four characters go through the index instead"
    );
}

/// The `LIKE` path escapes the reader's own wildcards.
///
/// **Without `ESCAPE`, a search for `%` returns the whole library.** Measured
/// 2026-08-21: `title LIKE '%' || '%' || '%'` matched all six rows of a
/// fixture. That is the one input that turns a search box into a table dump,
/// and it is one character long, which is exactly the path this task adds.
#[test]
fn a_wildcard_typed_into_the_search_box_is_a_literal() {
    let (library, _, _, _en, _raw) = two_root_library();
    let conn = library.conn();

    for wildcard in ["%", "_"] {
        let hits = search(&conn, &[], 1, wildcard).expect("searches");
        assert!(
            hits.series.is_empty() && hits.unfiled.is_empty(),
            "`{wildcard}` is a character to look for, not an operator"
        );
    }
}

/// A short query matching a **work's** title must still drag its series in.
///
/// **The third `LIKE` branch, and the one a mutation sweep proved was pinned by
/// nothing.** `search` reaches `LIKE` through three helpers; the test above
/// covers `matching_series` and the one below covers `matching_unfiled_works`,
/// while `series_of_matching_works` could be made to match zero rows with the
/// whole workspace still green. That is the same gap
/// `each_search_index_is_load_bearing_on_its_own` exists to close on the FTS
/// path — the fixture's series and its work shared a word, so either index
/// alone produced the hit and each covered for the other's absence.
///
/// The work's short title must therefore be one the **series** does not carry.
#[test]
fn a_short_query_matching_a_works_title_drags_its_series_in() {
    let (library, _, _, _en, _raw) = two_root_library();
    let conn = library.conn();
    // A name only the WORK has, two characters — below `TRIGRAM_MIN`, so this
    // can only be answered by the `LIKE` path.
    conn.execute("UPDATE work SET title = '花嫁' WHERE series_id IS NOT NULL", [])
        .expect("gives the filed work a title its series does not carry");

    let hits = search(&conn, &[], 1, "花嫁").expect("searches");
    assert_eq!(
        hits.series.len(),
        1,
        "a work title found by the fallback must surface its series"
    );
}

/// An unfiled work is reachable by a short query too — the `LIKE` path has to
/// cover both indexes, not only the series one.
#[test]
fn a_short_query_finds_an_unfiled_work_by_its_own_title() {
    let (library, _, _, _en, _raw) = two_root_library();
    // `two_root_library` files its one work into a series; an unfiled work
    // needs an import, same as `search_finds_an_unfiled_work_by_its_own_title`
    // above. Without this, the `UPDATE` below renames zero rows — and
    // `.expect()` on `execute` checks for a SQL error, not for a row count, so
    // the test would pass while asserting nothing.
    import_a_work(&library, "Placeholder", "/x/taiyaku.json");
    let conn = library.conn();
    conn.execute("UPDATE work SET title = '花嫁' WHERE series_id IS NULL", [])
        .expect("renames the unfiled work");

    let hits = search(&conn, &[], 1, "花嫁").expect("searches");
    assert_eq!(
        hits.unfiled.len(),
        1,
        "the unfiled index needs the same fallback"
    );
}

/// The backslash needs a **match** to prove anything, where the wildcards need
/// a miss.
///
/// **Measured 2026-08-21, against a fixture holding `back\slash` and
/// `100% done`.** The query builds `title LIKE '%' || ? || '%' ESCAPE '\'`:
///
/// | needle | escaped | escaping removed |
/// | --- | --- | --- |
/// | `\` | matches the one row | **0 rows** |
/// | `%` | matches the one row | **every row** |
/// | `_` | matches the one row | **every row** |
///
/// So an "expect nothing" assertion catches an unescaped `%` or `_`, which
/// matches everything — and cannot catch an unescaped `\`, which swallows the
/// wrapper's trailing `%` and leaves a pattern meaning "ends with a literal
/// `%`" that nothing matches either way. A reviewer proved that by deleting
/// `'\\'` from [`escape_like`]'s `matches!` arm and watching the whole
/// workspace stay green with the backslash inside the wildcard loop above.
#[test]
fn a_typed_backslash_finds_a_title_that_contains_one() {
    let (library, _, _, _en, _raw) = two_root_library();
    let conn = library.conn();
    conn.execute(
        r"UPDATE work SET title = 'back\slash' WHERE series_id IS NOT NULL",
        [],
    )
    .expect("gives the work a title with a literal backslash");

    let hits = search(&conn, &[], 1, "\\").expect("searches");
    assert_eq!(
        hits.series.len(),
        1,
        "a backslash is a character to look for, and this title has one"
    );
}

/// The grant still decides, whichever path answered.
///
/// **This is the property the whole-branch review found broken once already**
/// — `ids` used to `LIMIT` before the grant filter ran, so a search that
/// matched only invisible rows reported nothing found. A second query path is
/// a second chance to make that mistake, so it is pinned on both.
#[test]
fn a_short_query_is_filtered_by_the_grant_like_a_long_one() {
    let (library, _en_root, raw_root, _en, _raw) = two_root_library();
    let conn = library.conn();
    conn.execute(
        "INSERT INTO series_title (series_id, title, kind, is_primary)
         SELECT id, '五等分の花嫁', 'native', 0 FROM series LIMIT 1",
        [],
    )
    .expect("native title");

    let unrestricted = search(&conn, &[], 1, "花嫁").expect("searches");
    assert_eq!(unrestricted.series.len(), 1, "an empty grant sees it");

    let granted = search(&conn, &[raw_root], 1, "花嫁").expect("searches");
    assert_eq!(
        granted.series.len(),
        1,
        "a grant covering an edition of it still sees it"
    );

    let elsewhere = search(&conn, &[9999], 1, "花嫁").expect("searches");
    assert!(
        elsewhere.series.is_empty(),
        "a grant covering nothing sees nothing, however the query was answered"
    );
}

/// `?root=` deliberately does not reach search, and this says so out loud.
///
/// `search` passes `None` for `only` to both `series` and `unfiled`. That is a
/// decision rather than an omission — the sidebar's root links point at
/// `/?root=`, not at `/search` — and it is pinned here so that changing it is
/// a change to a test rather than a silent widening. See this plan's "What
/// this plan deliberately does not do", and Plan A's ledger, which carried it.
#[test]
fn search_spans_the_whole_grant_rather_than_one_root() {
    let (library, en_root, raw_root, _en, _raw) = two_root_library();
    let conn = library.conn();

    let both = search(&conn, &[en_root, raw_root], 1, "Series").expect("searches");
    assert_eq!(
        both.series.len(),
        1,
        "the series is found through the grant covering both its roots"
    );
    let card = both.series.first().expect("one series");
    assert_eq!(
        card.editions, 2,
        "and it carries both editions — search narrows to no single root"
    );
}
```

- [ ] **Step 2: Run them to verify they fail**

Run: `env -u TAIYAKU_CORPUS cargo test -p taiyaku-library browse::tests -- --nocapture`

Expected: `a_query_too_short_for_trigram_is_answered_by_like` FAILS with
`assertion `left == right` failed: left: 0, right: 1`, and
`a_wildcard_typed_into_the_search_box_is_a_literal` FAILS — `%` currently goes
to FTS5 as `"%"`, which trigram answers with nothing, so **this one may already
pass**. If it does, say so and keep it: it is the regression guard for the
`LIKE` path this task is about to add, and a test that passes before the change
and after it still earns its place when the change is what could break it.

- [ ] **Step 3: Add the two query shapes**

In `crates/taiyaku-library/src/browse.rs`, above `search()`, add:

**The constant must be named `TRIGRAM_MIN` exactly.**
`008_trigram_fts.sql`'s comment already points at it by name ("see its
`TRIGRAM_MIN`"), and that migration shipped in Task 1. A different name here
leaves a permanent dangling reference in a file the append-only rule says not
to revisit.

```rust
/// The shortest query FTS5's `trigram` tokeniser can answer.
///
/// Measured 2026-08-21 against SQLite 3.53.4: a trigram index over
/// `五等分の花嫁` answers `五` and `五等` with nothing, and `五等分` with the
/// row — the `*` prefix operator does not change that, because trigram has no
/// prefix to operate on. Below this length the index is not slow, it is
/// silent, which is a different failure and needs a different answer.
const TRIGRAM_MIN: usize = 3;

/// How one query is going to be asked, chosen by its length alone.
///
/// **Two paths rather than one widened one.** `LIKE '%q%'` would answer every
/// query correctly at this library's size — §7's measured 127 series — and was
/// considered; the index is kept because it is the part that still holds at
/// ten times that, and the scan is kept because it is the only thing that
/// answers the first two keystrokes at all.
enum Match {
    /// An FTS5 string literal, quotes doubled, for `MATCH`.
    Fts(String),
    /// A `LIKE` needle with `%`, `_` and `\` escaped, for `LIKE … ESCAPE '\'`.
    Like(String),
}

/// Which path a query takes, by `chars().count()` rather than `len()`.
///
/// **Bytes would be wrong in exactly the case this exists for**: `花嫁` is two
/// characters and six UTF-8 bytes, so a byte length would send it to trigram,
/// which is the silence this function is avoiding. Every Japanese query short
/// enough to need the fallback is long enough in bytes to miss it.
fn match_for(trimmed: &str) -> Match {
    if trimmed.chars().count() < TRIGRAM_MIN {
        Match::Like(escape_like(trimmed))
    } else {
        Match::Fts(format!("\"{}\"", trimmed.replace('"', "\"\"")))
    }
}

/// A `LIKE` needle in which the reader's own `%`, `_` and `\` are literals.
///
/// Measured 2026-08-21: without this, `title LIKE '%' || '%' || '%'` matches
/// every row — a one-character search that dumps the library. `ESCAPE '\'` is
/// what makes the backslash meaningful, and so what makes a typed backslash
/// need one of its own.
fn escape_like(q: &str) -> String {
    let mut out = String::with_capacity(q.len());
    for ch in q.chars() {
        if matches!(ch, '%' | '_' | '\\') {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}
```

- [ ] **Step 4: Point the three helpers at whichever path they were given**

Replace `matching_series`, `series_of_matching_works`, `matching_unfiled_works`
and `ids` in `crates/taiyaku-library/src/browse.rs` with:

```rust
/// Series ids whose own titles match.
fn matching_series(conn: &Connection, m: &Match, shown: &str) -> Result<Vec<i64>, String> {
    let (sql, needle) = match m {
        Match::Fts(q) => (
            "SELECT DISTINCT st.series_id FROM series_fts f
               JOIN series_title st ON st.rowid = f.rowid
              WHERE series_fts MATCH ?",
            q,
        ),
        Match::Like(q) => (
            "SELECT DISTINCT st.series_id FROM series_title st
              WHERE st.title LIKE '%' || ? || '%' ESCAPE '\\'",
            q,
        ),
    };
    ids(conn, sql, needle, &format!("searching series for `{shown}`"))
}

/// Series ids dragged in by a *work* whose title matches — searching
/// "Hanayome" should find the shelf, not only the volume that spells it.
fn series_of_matching_works(
    conn: &Connection,
    m: &Match,
    shown: &str,
) -> Result<Vec<i64>, String> {
    let (sql, needle) = match m {
        Match::Fts(q) => (
            "SELECT DISTINCT w.series_id FROM work_fts f
               JOIN work w ON w.id = f.rowid
              WHERE work_fts MATCH ? AND w.series_id IS NOT NULL",
            q,
        ),
        Match::Like(q) => (
            "SELECT DISTINCT w.series_id FROM work w
              WHERE w.title LIKE '%' || ? || '%' ESCAPE '\\'
                AND w.series_id IS NOT NULL",
            q,
        ),
    };
    ids(conn, sql, needle, &format!("searching works for `{shown}`"))
}

/// Work ids matching, among those belonging to no series.
fn matching_unfiled_works(
    conn: &Connection,
    m: &Match,
    shown: &str,
) -> Result<Vec<i64>, String> {
    let (sql, needle) = match m {
        Match::Fts(q) => (
            "SELECT f.rowid FROM work_fts f
               JOIN work w ON w.id = f.rowid
              WHERE work_fts MATCH ? AND w.series_id IS NULL",
            q,
        ),
        Match::Like(q) => (
            "SELECT w.id FROM work w
              WHERE w.title LIKE '%' || ? || '%' ESCAPE '\\'
                AND w.series_id IS NULL",
            q,
        ),
    };
    ids(conn, sql, needle, &format!("searching unfiled works for `{shown}`"))
}

/// One column of ids from a title query, which the three above differ only in.
///
/// **Unbounded, deliberately, and the cap moved to the caller.** These used to
/// `LIMIT ?2` and were only then intersected with what the grant allows — so
/// if the twenty rows SQLite happened to return were all outside the caller's
/// roots, a search that genuinely matched visible series reported nothing
/// found. That is the "filter after the query" failure the module header
/// argues against, committed by the one function that argued it. Found by the
/// whole-branch review.
///
/// Unbounded is affordable because these return ids from a query over title
/// columns, and [`search`] caps the filtered result instead. It stays
/// affordable on the `LIKE` path for the same reason §7 declines
/// virtualisation: 127 series and their titles is a scan measured in
/// microseconds, and it is only ever reached by a query of one or two
/// characters.
fn ids(conn: &Connection, sql: &str, needle: &str, doing: &str) -> Result<Vec<i64>, String> {
    let mut stmt = conn
        .prepare(sql)
        .map_err(|e| format!("preparing: {doing}: {e}"))?;
    let rows = stmt
        .query_map(rusqlite::params![needle], |row| row.get(0))
        .map_err(|e| format!("{doing}: {e}"))?;
    rows.collect::<rusqlite::Result<Vec<i64>>>()
        .map_err(|e| format!("{doing}: {e}"))
}
```

- [ ] **Step 5: Choose the path once, in `search`**

In `crates/taiyaku-library/src/browse.rs`, replace the `quoted` line in
`search()` and the three call sites below it:

```rust
    let matcher = match_for(trimmed);

    let mut matched = matching_series(conn, &matcher, trimmed)?;
    matched.extend(series_of_matching_works(conn, &matcher, trimmed)?);
```

and further down:

```rust
    let unfiled_ids = matching_unfiled_works(conn, &matcher, trimmed)?;
```

Update `search`'s own doc comment — the paragraph beginning "**The query is
quoted as an FTS5 string literal**" — to:

```rust
/// Search titles through FTS5 — **over both indexes, and past it when the
/// query is too short for one.**
///
/// **A query of three characters or more is quoted as an FTS5 string
/// literal**, doubling any `"`, so a reader typing `AND`, `*` or `-` gets a
/// search for those characters rather than a syntax error or an operator they
/// did not ask for. **A shorter one is a `LIKE` scan**, because migration 8's
/// `trigram` tokeniser has no term shorter than three characters to match and
/// answers such a query with silence rather than slowness — and `花嫁`, two
/// characters, is an ordinary Japanese word and the second half of a title
/// this library holds. See [`TRIGRAM_MIN`] and [`escape_like`].
///
/// `work_fts` has been populated by triggers since migration 1 with nothing
/// reading it. Searching series alone leaves an imported work unfindable by its
/// own title, which is spec C1 §5.4.
```

- [ ] **Step 6: Run the tests — the whole workspace, not just this crate**

Run: `env -u TAIYAKU_CORPUS cargo test -p taiyaku-library browse::tests -- --nocapture`

Expected: PASS, all of them.

Then run: `env -u TAIYAKU_CORPUS cargo test --workspace`

Expected: PASS. **One test is red before this task and must go green with it:**
`taiyaku-serve`'s `library_api::tests::a_search_echoes_the_query_back_so_an_empty_result_can_name_it`
(`crates/taiyaku-serve/src/library_api/tests.rs:145`) searches `?q=W` against a
fixture whose work title is the single character `W`
(`crates/taiyaku-serve/src/test_support.rs`). `unicode61` matched that as a
whole token; `trigram` cannot match anything under three characters; the `LIKE`
path this task adds is what answers it. It is the one existing test in the
repository that proves the fallback works on a real route rather than in a unit
fixture — **if it is still red, this task is not done**, and if it went green
for some other reason, say which.

Task 1's brief scoped its own testing to `-p taiyaku-library`, which is why
this regression shipped unflagged for one commit. Do not repeat that: the
workspace run is part of this step, not an optional extra.

- [ ] **Step 7: Prove the new tests can fail**

A test over a fallback that was never taken is a test of nothing. Mutate and
watch, **keeping the parameter count intact** — a mutation that changes how
many placeholders a statement carries is caught by `rusqlite`'s arity check and
proves nothing about semantics:

```rust
// In `match_for`, temporarily:
if trimmed.chars().count() < 0 {
```

Run the suite. Expected: **three** tests FAIL —
`a_query_too_short_for_trigram_is_answered_by_like`,
`a_short_query_finds_an_unfiled_work_by_its_own_title` and
`a_short_query_is_filtered_by_the_grant_like_a_long_one`, the last because it
also queries the two-character `花嫁`. Then revert and re-run to green. Record
both counts in the report.

- [ ] **Step 8: Clippy, then commit**

Run: `cargo clippy -p taiyaku-library --tests -- -D warnings`

Expected: no output. CI runs clippy with `-D warnings`
(`.forgejo/workflows/ci.yml:73`), so a warning here is a failed build, not a
note. Plan A's Task 1 shipped a `let_and_return` this way.

```bash
git add crates/taiyaku-library/src/browse.rs crates/taiyaku-library/src/browse/tests.rs
git commit -m "search: a LIKE path for queries shorter than a trigram"
```

---
### Task 3: `Shell` — one sidebar, three screens

**Files:**
- Create: `web/src/shell.ts` — the status context and its hook
- Create: `web/src/Shell.tsx` — the component
- Create: `web/src/Shell.test.tsx`
- Modify: `web/src/Library.tsx` — drop its own `status()` fetch and its `<Sidebar>`, read the context instead
- Modify: `web/src/Library.test.tsx` — supply the context the way it already supplies `GateContext`
- Modify: `web/src/index.css` — nothing moves; `.library` stays the grid container and is now set by `Shell`

**Interfaces:**
- Consumes: `library.status()`, `Sidebar`, `useGate` — all shipped.
- Produces: `Shell({ children }: { children: React.ReactNode })`;
  `StatusContext: React.Context<LibraryStatus | undefined>`;
  `useLibraryStatus(): LibraryStatus | undefined`. Tasks 4 and 10 wrap their
  screens in `Shell`; Task 9 makes `showRoots` a preference in one place
  because of it.

**Why now and not later.** `Series` and `Search` both need the sidebar, and
`Sidebar` needs `status.roots` and the account's `admin` bit. Three copies of
that fetch is three chances for the roots list to differ between screens, and
Task 9 would then have to thread one preference into three components. This is
a pure refactor: `Library` must behave identically afterwards, and its existing
tests are the proof.

- [ ] **Step 1: Write the context**

Create `web/src/shell.ts`:

```ts
/**
 * What the library's chrome knows, and how a screen under it asks.
 *
 * Separate from `Shell.tsx`'s component for the reason `gate.ts` states: a
 * non-component export sharing a file with a component defeats React Fast
 * Refresh, and biome's `style/useComponentExportOnlyModules` refuses it.
 */
import { createContext, useContext } from 'react'
import type { LibraryStatus } from './library'

/**
 * `undefined` where no shell sits above the reader of this context, and
 * **also while the shell's own fetch is still in flight** — the two are
 * deliberately one value. A screen that has to tell them apart is a screen
 * asking the chrome a question the chrome cannot answer; `Library`'s empty
 * states wait for it either way.
 */
export const StatusContext = createContext<LibraryStatus | undefined>(undefined)

/** The library's roots and whether anything has scanned them, once known. */
export function useLibraryStatus(): LibraryStatus | undefined {
  return useContext(StatusContext)
}
```

- [ ] **Step 2: Write the failing test**

Create `web/src/Shell.test.tsx`:

```tsx
import { act } from 'react'
import { MemoryRouter } from 'react-router'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { Me } from './api'
import { GateContext } from './gate'
import * as library from './library'
import { Shell } from './Shell'
import { useLibraryStatus } from './shell'
import { mount } from './testing/mount'

vi.mock('./library', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./library')>()),
  status: vi.fn(),
}))

const who = (over: Partial<Me> = {}): Me => ({
  user_id: 1,
  admin: false,
  can_download: false,
  can_stream: false,
  ...over,
})

/** Mount and let the status load settle — two flushes, as `Library.test.tsx` explains. */
async function settle(ui: React.ReactNode, me?: Me): Promise<HTMLElement> {
  let host: HTMLElement | undefined
  await act(async () => {
    host = mount(
      <MemoryRouter>
        <GateContext.Provider
          value={me ? { who: me, signOut: () => {} } : undefined}
        >
          {ui}
        </GateContext.Provider>
      </MemoryRouter>,
    )
    await Promise.resolve()
    await Promise.resolve()
  })
  if (!host) throw new Error('nothing mounted')
  return host
}

/**
 * A child that reports what the context handed it.
 *
 * **Nested inside a function rather than declared at module scope.** A
 * component exported or declared beside non-component exports trips biome's
 * `style/useComponentExportOnlyModules` — the same rule that forces the
 * `gate.ts` / `Gate.tsx` split this file's subject copies. `Gate.test.tsx`'s
 * `probe()` is the established pattern; this follows it.
 */
function reporter() {
  function Reporter() {
    const status = useLibraryStatus()
    return <p data-testid="roots">{status ? status.roots.length : 'pending'}</p>
  }
  return <Reporter />
}

describe('Shell', () => {
  beforeEach(() => {
    vi.mocked(library.status).mockResolvedValue({
      roots: [{ id: 1, name: 'Manga' }],
      scanned: true,
    })
  })

  it('fetches the status once and offers it to what it renders', async () => {
    const host = await settle(
      <Shell>{reporter()}</Shell>,
    )
    expect(host.querySelector('[data-testid="roots"]')?.textContent).toBe('1')
    expect(vi.mocked(library.status)).toHaveBeenCalledTimes(1)
  })

  it('draws the roots list for an admin and not for a reader', async () => {
    const asAdmin = await settle(
      <Shell>
        <p>x</p>
      </Shell>,
      who({ admin: true }),
    )
    expect(asAdmin.querySelector('.sidebar-roots')).not.toBeNull()

    const asReader = await settle(
      <Shell>
        <p>x</p>
      </Shell>,
      who({ admin: false }),
    )
    expect(asReader.querySelector('.sidebar-roots')).toBeNull()
  })

  // **This one is not optional and is easy to lose.** `Shell` reads
  // `useGate()?.who.admin ?? false`, and `gate.ts` spends a paragraph on why
  // that default must read as "no account known" and never as "an admin".
  // Task 3 first shipped without it and a reviewer flipped `?? false` to
  // `?? true` with the whole jsdom suite still green. It must assert that the
  // roots **resolved** as well as that the list is absent, or it passes
  // because there was nothing to draw.
  it('draws no roots list where no gate sits above it, even though roots are there to draw', async () => {
    const host = await settle(<Shell>{reporter()}</Shell>)
    expect(host.querySelector('[data-testid="roots"]')?.textContent).toBe('1')
    expect(host.querySelector('.sidebar-roots')).toBeNull()
  })

  it('says so when the status cannot be read, rather than drawing an empty sidebar in silence', async () => {
    vi.mocked(library.status).mockRejectedValue(new Error('/api/library/status: 500'))
    const host = await settle(
      <Shell>
        <p>x</p>
      </Shell>,
    )
    expect(host.querySelector('[role="alert"]')?.textContent).toContain('500')
  })
})
```

- [ ] **Step 3: Run it to verify it fails**

Run: `pnpm -C web exec vitest run --project jsdom src/Shell.test.tsx`
Expected: FAIL — `Failed to resolve import "./Shell"`.

- [ ] **Step 4: Write the component**

Create `web/src/Shell.tsx`:

```tsx
/**
 * The library's chrome: the sidebar, and the `<main>` a screen draws into.
 *
 * **The status fetch lives here rather than in each screen**, because three
 * screens need the same roots list and three fetches is three chances for them
 * to disagree. `Library` also reads it for §8's empty states, through
 * `shell.ts`'s context rather than through a prop, so a screen that does not
 * care about it says nothing about it.
 *
 * System colours throughout — see `index.css`'s `.library` block, and
 * `Login.tsx`, which shipped a hardcoded `#111` on this app's front door.
 */
import { useEffect, useState } from 'react'
import { useGate } from './gate'
import { type LibraryStatus, status as fetchStatus } from './library'
import { StatusContext } from './shell'
import { Sidebar } from './Sidebar'

export function Shell({ children }: { children: React.ReactNode }) {
  // **The roots list is an admin's, and only the gate knows who is asking.**
  // `undefined` — no gate above, which means a test mounting one screen — reads
  // as "no account known" and so as not an admin: what an account may *see* is
  // the server's to enforce, and this chooses only what to draw.
  //
  // A default rather than the last word: §11 step 8's `browse` module makes it
  // a preference, and a preference has to start somewhere.
  const admin = useGate()?.who.admin ?? false
  const [status, setStatus] = useState<LibraryStatus | undefined>()
  const [failure, setFailure] = useState<string | undefined>()

  useEffect(() => {
    let cancelled = false
    fetchStatus()
      .then((next) => {
        if (cancelled) return
        setStatus(next)
        setFailure(undefined)
      })
      .catch((err: unknown) => {
        if (cancelled) return
        setFailure(err instanceof Error ? err.message : String(err))
      })
    return () => {
      cancelled = true
    }
  }, [])

  return (
    <StatusContext.Provider value={status}>
      <div className="library">
        <Sidebar roots={status?.roots ?? []} showRoots={admin} />
        <main>
          {failure !== undefined && <p role="alert">{failure}</p>}
          {children}
        </main>
      </div>
    </StatusContext.Provider>
  )
}
```

- [ ] **Step 5: Run the test**

Run: `pnpm -C web exec vitest run --project jsdom src/Shell.test.tsx`
Expected: PASS, three tests.

- [ ] **Step 6: Move `Library` inside it**

In `web/src/Library.tsx`: delete the `admin` line, the `status` state, the
`fetchStatus` import and its element of the `Promise.all`, and the `<div
className="library">`/`<Sidebar>`/`<main>` wrapper. Read the context instead.
The effect becomes:

```tsx
  useEffect(() => {
    let cancelled = false
    Promise.all([fetchGrid(root), fetchHome()])
      .then(([grid, home]) => {
        if (cancelled) return
        setGrid(grid)
        setHome(home)
        // A retry that lands after an earlier failure must clear it — this
        // effect re-fires on `root` alone, so a reader who failed once and
        // then switched (or re-triggered) a load that succeeded would
        // otherwise keep the banner on screen forever, beside the data that
        // just proved it stale.
        setFailure(undefined)
      })
      .catch((err: unknown) => {
        if (cancelled) return
        setFailure(err instanceof Error ? err.message : String(err))
      })
    return () => {
      cancelled = true
    }
  }, [root])
```

and the status read becomes:

```tsx
  const status = useLibraryStatus()
```

with the return value losing its outer two elements — `Library` now returns the
`<>…</>` that used to sit inside `<main>`, starting at the failure banner.

In `web/src/Routes.tsx`, wrap the route element:

```tsx
      <Route
        path="/"
        element={
          <Shell>
            <Library />
          </Shell>
        }
      />
```

- [ ] **Step 7: Fix `Library.test.tsx` and run the whole web suite**

`Library.test.tsx` mocks `status` and asserts on `.sidebar-roots`; those
assertions now belong to `Shell.test.tsx` and the mock no longer has a caller
inside `Library`. Wrap the mounts in a `StatusContext.Provider` carrying the
value the mock used to resolve, and **move the two roots-list assertions to
`Shell.test.tsx` rather than deleting them** — they are the regression guard
for the hardcoded `false` that shipped on this branch.

Run: `pnpm -C web run test && pnpm -C web run test:jsdom`
Expected: PASS. **The count must not fall by more than the two assertions
moved.** If a test disappears, say which and why.

- [ ] **Step 8: Commit**

```bash
git add web/src/shell.ts web/src/Shell.tsx web/src/Shell.test.tsx web/src/Library.tsx web/src/Library.test.tsx web/src/Routes.tsx
git commit -m "shell: one sidebar and one status fetch for every browse screen"
```

---

### Task 4: The series page, grid layout

**Files:**
- Create: `web/src/Cover.tsx` — `Library.tsx`'s cover-or-placeholder, moved so two screens share one
- Create: `web/src/Series.tsx`
- Create: `web/src/Series.test.tsx`
- Modify: `web/src/library.ts` — add `workCover`
- Modify: `web/src/Routes.tsx` — `/series/:id` renders `Series` inside `Shell`
- Modify: `web/src/Routes.test.tsx` — its hand-rolled `vi.mock('./library', …)` has no `oneSeries`, so mounting `/series/1` crashes, and its one series-route test still asserts the `NotBuiltYet` placeholder text. Add the fixture and rewrite that test to assert the shell mounts, mirroring the file's existing structural-marker pattern rather than duplicating `Series.tsx`'s own content tests
- Modify: `web/src/Library.tsx` — move the `⇄` mark onto the cover
- Modify: `web/src/index.css` — the volume grid and the mark's new position

**Interfaces:**
- Consumes: Task 3's `Shell`; `library.oneSeries(id)` → `{ series: SeriesCard; works: WorkCard[] }`; `WorkCard { id, slug, title, volume: number | null, aligned: boolean, pages: number, editions: EditionCard[] }`.
- Produces: `Series()` — no props, reads `:id` from the route. `workCover(slug: string, edition: string): string`. Tasks 5, 6 and 7 add the table, the selector and the panel to this same component.

**What this page must show that a Komga series page cannot** (§3): Go-Toubun is
14 volumes in `Manga`, 14 in `Manga (RAW)`, a fan release — and only volumes 1,
3 and 8 are aligned. A page that lists volumes and editions without saying
which are aligned is a Komga screen with extra badges. `WorkCard.aligned` is
the bool that says it, and it shipped in #81.

- [ ] **Step 1: Move `Cover` out of `Library.tsx`**

Create `web/src/Cover.tsx` holding `Library.tsx`'s `Cover` **verbatim** — its
whole doc comment included, since every paragraph of it is a measured reason —
`export`ed, with one line added at the end of that comment:

```tsx
 * **Two screens draw it**: the library's series tiles and a series page's
 * volume tiles. One copy, because the failure-by-`src` rule and the empty
 * `alt` are the kind of detail that survives in one file and drifts in two.
 */
export function Cover({ src, title }: { src: string; title: string }) {
```

`Library.tsx` imports it and loses its local copy. Nothing else about that file
changes in this step.

Run: `pnpm -C web exec vitest run --project jsdom src/Library.test.tsx`
Expected: PASS, unchanged. If it needs an edit, this stopped being a move —
stop and report what changed.

- [ ] **Step 2: Add the cover URL helper**

In `web/src/library.ts`, after `oneSeries`:

```ts
/**
 * A volume's cover: page 0 of one of its editions, through the thumbnail route
 * the reader already has.
 *
 * **Not a `/api/library/…` route, and there is deliberately no such thing.**
 * `/api/library/series/{id}/cover` is a 307 to exactly this URL — see
 * `library_api.rs::cover`, whose whole argument is that a series cover *is* an
 * edition cover and a second pipeline would be a second implementation of
 * caching, guarding and `Cache-Control`. A volume tile already knows which
 * edition it is showing, so it needs no redirect to find out.
 */
export const workCover = (slug: string, edition: string) =>
  `/api/works/${encodeURIComponent(slug)}/${encodeURIComponent(edition)}/pages/0/thumb`
```

- [ ] **Step 3: Write the failing tests**

Create `web/src/Series.test.tsx`:

```tsx
import { act } from 'react'
import { MemoryRouter, Route, Routes } from 'react-router'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import * as library from './library'
import { Series } from './Series'
import { mount } from './testing/mount'

vi.mock('./library', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./library')>()),
  oneSeries: vi.fn(),
}))

const edition = (
  over: Partial<library.EditionCard> = {},
): library.EditionCard => ({
  key: 'raw',
  label: 'Manga (RAW)',
  members: 1,
  pages: 180,
  ...over,
})

const work = (over: Partial<library.WorkCard> = {}): library.WorkCard => ({
  id: 1,
  slug: 'gotoubun-v01',
  title: 'Go-Toubun no Hanayome v01',
  volume: 1,
  aligned: false,
  pages: 180,
  editions: [edition()],
  ...over,
})

const card = (over: Partial<library.SeriesCard> = {}): library.SeriesCard => ({
  id: 7,
  title: 'Go-Toubun no Hanayome',
  works: 3,
  editions: 5,
  aligned: 1,
  added: 3,
  unread: 2,
  cover: '/api/library/series/7/cover',
  ...over,
})

/** Mount at `/series/7` and let the load settle — two flushes, as `Library.test.tsx` explains. */
async function settle(): Promise<HTMLElement> {
  let host: HTMLElement | undefined
  await act(async () => {
    host = mount(
      <MemoryRouter initialEntries={['/series/7']}>
        <Routes>
          <Route path="/series/:id" element={<Series />} />
        </Routes>
      </MemoryRouter>,
    )
    await Promise.resolve()
    await Promise.resolve()
  })
  if (!host) throw new Error('nothing mounted')
  return host
}

const titles = (host: HTMLElement) =>
  [...host.querySelectorAll('.volumes .title')].map((n) => n.textContent)

describe('Series, grid layout', () => {
  beforeEach(() => {
    // **Without this the last test in the file fails against a correct
    // implementation.** `toHaveBeenCalledWith`/`not.toHaveBeenCalled` read a
    // counter that accumulates across every test in the file, so by the time
    // "does not fetch when the URL carries something that is not a number"
    // runs, `oneSeries` has six calls from its neighbours. `App.test.tsx` does
    // the same thing for the same reason.
    vi.clearAllMocks()
    vi.mocked(library.oneSeries).mockResolvedValue({
      series: card(),
      works: [
        work(),
        work({ id: 2, slug: 'gotoubun-v03', title: 'Go-Toubun no Hanayome v03', volume: 3, aligned: true }),
        work({ id: 3, slug: 'gotoubun-v08', title: 'Go-Toubun no Hanayome v08', volume: 8 }),
      ],
    })
  })

  it('asks for the series named in the URL', async () => {
    await settle()
    expect(vi.mocked(library.oneSeries)).toHaveBeenCalledWith(7)
  })

  it('names the series and lists its volumes in order', async () => {
    const host = await settle()
    expect(host.querySelector('h1')?.textContent).toBe('Go-Toubun no Hanayome')
    expect(titles(host)).toEqual([
      'Go-Toubun no Hanayome v01',
      'Go-Toubun no Hanayome v03',
      'Go-Toubun no Hanayome v08',
    ])
  })

  it('marks the aligned volume and only the aligned volume', async () => {
    const host = await settle()
    const marks = [...host.querySelectorAll('.volumes li')].map((li) =>
      li.querySelector('.aligned') === null ? '-' : '⇄',
    )
    expect(marks).toEqual(['-', '⇄', '-'])
  })

  it('links each volume into the reader by its slug', async () => {
    const host = await settle()
    const hrefs = [...host.querySelectorAll('.volumes a')].map((a) =>
      a.getAttribute('href'),
    )
    expect(hrefs).toEqual([
      '/read/gotoubun-v01',
      '/read/gotoubun-v03',
      '/read/gotoubun-v08',
    ])
  })

  it('draws each volume cover from its own edition rather than the series redirect', async () => {
    const host = await settle()
    expect(host.querySelector('.volumes img')?.getAttribute('src')).toBe(
      '/api/works/gotoubun-v01/raw/pages/0/thumb',
    )
  })

  it('says so when the series cannot be read', async () => {
    vi.mocked(library.oneSeries).mockRejectedValue(
      new Error('/api/library/series/7: 404'),
    )
    const host = await settle()
    expect(host.querySelector('[role="alert"]')?.textContent).toContain('404')
    expect(host.querySelector('.volumes')).toBeNull()
  })

  it('does not fetch when the URL carries something that is not a number', async () => {
    let host: HTMLElement | undefined
    await act(async () => {
      host = mount(
        <MemoryRouter initialEntries={['/series/not-a-number']}>
          <Routes>
            <Route path="/series/:id" element={<Series />} />
          </Routes>
        </MemoryRouter>,
      )
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(vi.mocked(library.oneSeries)).not.toHaveBeenCalled()
    expect(host?.querySelector('[role="alert"]')?.textContent).toContain(
      'not-a-number',
    )
  })
})
```

- [ ] **Step 4: Run it to verify it fails**

Run: `pnpm -C web exec vitest run --project jsdom src/Series.test.tsx`
Expected: FAIL — `Failed to resolve import "./Series"`.

- [ ] **Step 5: Write the component**

Create `web/src/Series.tsx`:

```tsx
/**
 * One series and its volumes — spec C1 §3 and §11 step 7.
 *
 * **Not three layouts.** §3's three mockups are three positions of three
 * switches (`layout`, `editionSelector`, `panel`), so this file is a grid and
 * a table plus two optional attachments. The switches arrive as preferences in
 * §11 step 8; until then they are this component's own props with §3's preset
 * A as the defaults.
 *
 * **The alignment mark is the whole point of the page.** Go-Toubun is 14
 * volumes in `Manga`, 14 in `Manga (RAW)` and a fan release, and only volumes
 * 1, 3 and 8 are aligned — a page that lists volumes and editions without
 * saying which are paired is a Komga screen with extra badges.
 * `WorkCard.aligned` is one bool per volume and no list route carries more;
 * the verdict and the paired count come from `/api/library/works/{slug}` and
 * from nowhere else, which is what the panel is for.
 */
import { useEffect, useState } from 'react'
import { Link, useParams } from 'react-router'
import { Cover } from './Cover'
import {
  oneSeries as fetchSeries,
  type SeriesCard,
  type WorkCard,
  workCover,
} from './library'

interface Loaded {
  series: SeriesCard
  works: WorkCard[]
}

export function Series() {
  const { id } = useParams()
  // **Parsed before the effect, not inside it.** `Number('not-a-number')` is
  // `NaN`, and `/api/library/series/NaN` is a request the server answers with
  // a 400 that says nothing useful to the person who mistyped a URL.
  // **`isSafeInteger`, not just digits.** `/^\d+$/` accepts
  // `9007199254740993`, which `Number` silently rounds to
  // `…992` — a different series — and `99999999999999999999999999`, which
  // becomes `1e+26` and is sent to the server in that spelling. An id this
  // build cannot represent takes the same honest path as a non-numeric one.
  const parsed = id !== undefined && /^\d+$/.test(id) ? Number(id) : Number.NaN
  const seriesId = Number.isSafeInteger(parsed) ? parsed : undefined

  const [loaded, setLoaded] = useState<Loaded | undefined>()
  const [failure, setFailure] = useState<string | undefined>()

  useEffect(() => {
    if (seriesId === undefined) {
      setFailure(`${id ?? ''} is not a series`)
      return
    }
    let cancelled = false
    fetchSeries(seriesId)
      .then((next) => {
        if (cancelled) return
        setLoaded(next)
        setFailure(undefined)
      })
      .catch((err: unknown) => {
        if (cancelled) return
        setFailure(err instanceof Error ? err.message : String(err))
      })
    return () => {
      cancelled = true
    }
  }, [seriesId, id])

  return (
    <>
      {failure !== undefined && <p role="alert">{failure}</p>}
      {loaded && (
        <>
          <h1>{loaded.series.title}</h1>
          <ul className="volumes">
            {loaded.works.map((w) => (
              <li key={w.slug}>
                <Link to={`/read/${w.slug}`}>
                  <span className="cover-box">
                    <Cover
                      src={workCover(w.slug, w.editions[0]?.key ?? '')}
                      title={w.title}
                    />
                    {w.aligned && (
                      <span className="aligned" title="has an aligned edition">
                        ⇄
                      </span>
                    )}
                  </span>
                  <span className="title">{w.title}</span>
                </Link>
              </li>
            ))}
          </ul>
        </>
      )}
    </>
  )
}
```

- [ ] **Step 6: Route it**

In `web/src/Routes.tsx`, replace the `/series/:id` placeholder:

```tsx
      <Route
        path="/series/:id"
        element={
          <Shell>
            <Series />
          </Shell>
        }
      />
```

`NotBuiltYet` now has one caller (`/search`). Leave it and its comment; Task 10
removes both.

- [ ] **Step 7: Put the mark on the cover, on both screens**

Plan A shipped `Library.tsx`'s `⇄ n` below the title, and its ledger carried
that as Plan B's to inherit — §3 says the mark is on the cover. Both screens
now wrap the image in a positioned box. In `web/src/Library.tsx`, the tile
becomes:

```tsx
                <Link to={`/series/${s.id}`}>
                  <span className="cover-box">
                    <Cover src={s.cover} title={s.title} />
                    {s.aligned > 0 && (
                      <span className="aligned">⇄ {s.aligned}</span>
                    )}
                  </span>
                  <span className="title">{s.title}</span>
                </Link>
```

Append to `web/src/index.css`, after the `.grid .aligned` rule:

**Delete `.grid .aligned` while you are here.** It sets `font-size: 0.8rem;
opacity: 0.7;` from when the mark sat below the title, and the mark is now a
descendant of `.grid` *and* of `.cover-box` on that screen. The two selectors
have equal specificity (0,2,0) and only one sets `opacity`, so it composes
rather than being overridden: the library badge renders at 70% and the series
page's at 100%. That is worse than an inconsistency — a 70%-opaque `Canvas`
background lets the cover show through, defeating the reason
`.cover-box .aligned` gives for choosing system colours at all. Found by review
after Task 4 shipped it.

**The cover, tile and title rules are shared by widening the existing
selectors, not by copying them.** `Cover.tsx` is one component (Step 1), and
two identical CSS blocks under two class names is the same drift in the
stylesheet that the component move just closed. So in
`web/src/index.css`, add `.volumes` to the four existing selector lists —
`.grid li a`, `.grid img, .grid .cover-missing`, `.grid img`,
`.grid .cover-missing` and `.grid .title` — leaving their declarations
untouched:

```css
.grid li a,
.volumes li a {
  /* …unchanged… */
}

.grid img,
.grid .cover-missing,
.volumes img,
.volumes .cover-missing {
  /* …unchanged… */
}

.grid img,
.volumes img {
  object-fit: cover;
}

.grid .cover-missing,
.volumes .cover-missing {
  /* …unchanged… */
}

.grid .title,
.volumes .title {
  /* …unchanged… */
}
```

Then append only what is genuinely new — the list itself, at its own column
width:

```css
/* The volume list on a series page — spec C1 §3. Narrower tiles than the
 * library grid's `9rem`: a series page shows volumes of one work, so the
 * covers are near-identical and the title under each is what distinguishes
 * them. Everything else it shares with `.grid` by name, above. */
.volumes {
  list-style: none;
  margin: 0;
  padding: 0;
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(7.5rem, 1fr));
  gap: 1rem;
}

/* §3 puts the alignment mark **on the cover**, which needs a positioned box
 * around it — Plan A shipped it below the title and its ledger carried that
 * forward as this plan's to fix.
 *
 * `Canvas`/`CanvasText` rather than a translucent black: a mark over artwork
 * has to stay legible against a white gutter and a saturated cover alike,
 * which is the same requirement `--box-*` exists for above and the same one
 * `Login.tsx`'s hardcoded `#111` failed. */
.cover-box {
  position: relative;
  display: block;
}

.cover-box .aligned {
  position: absolute;
  right: 0.25rem;
  bottom: 0.25rem;
  padding: 0.1rem 0.35rem;
  border: 1px solid ButtonBorder;
  border-radius: 4px;
  background: Canvas;
  color: CanvasText;
  font-size: 0.8rem;
  line-height: 1.4;
}
```

- [ ] **Step 8: Run the web suite**

Run: `pnpm -C web run test && pnpm -C web run test:jsdom`

Expected: PASS. `Library.test.tsx` asserts on `.aligned`'s text content, which
this moves but does not change — if an assertion fails on the *structure*
(`li > .aligned` vs `li .cover-box .aligned`), fix the selector rather than the
markup, and say which test it was.

- [ ] **Step 9: Commit**

```bash
git add web/src/Cover.tsx web/src/Series.tsx web/src/Series.test.tsx web/src/library.ts web/src/Routes.tsx web/src/Library.tsx web/src/index.css
git commit -m "series page: the volume grid, and the alignment mark where §3 puts it"
```

---
### Task 5: The table layout — one row per volume, one column per edition

**Files:**
- Modify: `web/src/Series.tsx` — add the `layout` prop and the table
- Modify: `web/src/Series.test.tsx`
- Modify: `web/src/index.css` — the table

**Interfaces:**
- Consumes: Task 4's `Series`, `WorkCard.editions: EditionCard[]`.
- Produces: `Series({ layout }: { layout?: 'grid' | 'table' })`, defaulting to
  `'grid'` — §3's preset A. Task 9 passes the preference in.

**The honest cost, stated in §3 and repeated here.** Two presentations of one
dataset are genuinely two components to keep in step, and the tests double for
anything that differs between them. That is the price of answering "all three,
per-user" and it does not disappear because the switches are cheap.

**The dashed badge is what the table adds.** A grid tile shows the volumes a
series has; a table shows which editions each volume is *missing*, which is the
question a reader pairing two releases actually asks. §3 names it.

- [ ] **Step 1: Write the failing tests**

Append to `web/src/Series.test.tsx`, inside a new `describe`:

```tsx
describe('Series, table layout', () => {
  beforeEach(() => {
    vi.mocked(library.oneSeries).mockResolvedValue({
      series: card(),
      works: [
        work({
          editions: [
            edition({ key: 'raw', label: 'Manga (RAW)', pages: 180 }),
            edition({ key: 'en', label: 'Manga', pages: 178 }),
          ],
        }),
        work({
          id: 2,
          slug: 'gotoubun-v03',
          title: 'Go-Toubun no Hanayome v03',
          volume: 3,
          aligned: true,
          // **A different label for the same key, deliberately.** Real edition
          // labels are generated per volume from member filenames, so two
          // volumes sharing a key routinely disagree — which is what makes
          // `editionColumns` taking the *first* volume's label a decision
          // rather than an accident. With both volumes carrying
          // `'Manga (RAW)'` the column test passes under last-seen-wins too,
          // and this branch shipped exactly that fixture until review
          // mutated it and watched all fourteen tests stay green.
          editions: [
            edition({ key: 'raw', label: 'Manga (RAW) - v03.cbz', pages: 190 }),
          ],
        }),
      ],
    })
  })

  it('gives every edition in the series a column, in first-seen order', async () => {
    const host = await settleWith({ layout: 'table' })
    const heads = [...host.querySelectorAll('.editions thead th')].map(
      (th) => th.textContent,
    )
    expect(heads).toEqual(['Volume', 'Manga (RAW)', 'Manga'])
  })

  it('shows the page count where a volume has that edition', async () => {
    const host = await settleWith({ layout: 'table' })
    const firstRow = host.querySelectorAll('.editions tbody tr')[0]
    const cells = [...(firstRow?.querySelectorAll('td') ?? [])].map(
      (td) => td.textContent,
    )
    expect(cells).toEqual(['Go-Toubun no Hanayome v01', '180', '178'])
  })

  it('marks a missing edition rather than leaving the cell blank', async () => {
    const host = await settleWith({ layout: 'table' })
    const secondRow = host.querySelectorAll('.editions tbody tr')[1]
    const missing = secondRow?.querySelectorAll('td')[2]
    expect(missing?.classList.contains('missing')).toBe(true)
    expect(missing?.textContent).toBe('—')
    // A dash alone is not a label. §9's rule: what a sighted reader infers
    // from a dashed cell, a screen reader has to be told.
    expect(missing?.getAttribute('aria-label')).toBe('no Manga edition')
  })

  it('still links each volume into the reader', async () => {
    const host = await settleWith({ layout: 'table' })
    const hrefs = [...host.querySelectorAll('.editions a')].map((a) =>
      a.getAttribute('href'),
    )
    expect(hrefs).toEqual(['/read/gotoubun-v01', '/read/gotoubun-v03'])
  })

  it('marks the aligned volume here too — the mark is the page, not the layout', async () => {
    const host = await settleWith({ layout: 'table' })
    const marked = [...host.querySelectorAll('.editions tbody tr')].map(
      (tr) => tr.querySelector('.aligned') !== null,
    )
    expect(marked).toEqual([false, true])
  })

  it('draws no grid when it is drawing a table', async () => {
    const host = await settleWith({ layout: 'table' })
    expect(host.querySelector('.volumes')).toBeNull()
    expect(host.querySelector('.editions')).not.toBeNull()
  })
})
```

and replace the file's `settle` helper with one that takes props, keeping the
old name working for the tests already written:

```tsx
async function settleWith(
  props: React.ComponentProps<typeof Series> = {},
): Promise<HTMLElement> {
  let host: HTMLElement | undefined
  await act(async () => {
    host = mount(
      <MemoryRouter initialEntries={['/series/7']}>
        <Routes>
          <Route path="/series/:id" element={<Series {...props} />} />
        </Routes>
      </MemoryRouter>,
    )
    await Promise.resolve()
    await Promise.resolve()
  })
  if (!host) throw new Error('nothing mounted')
  return host
}

const settle = () => settleWith()
```

- [ ] **Step 2: Run them to verify they fail**

Run: `pnpm -C web exec vitest run --project jsdom src/Series.test.tsx`
Expected: FAIL — six failures, each `expected null not to be null` or a
TypeScript error on the unknown `layout` prop.

- [ ] **Step 3: Derive the columns**

In `web/src/Series.tsx`, above the component:

```tsx
/**
 * Every edition key in this series, in the order the volumes first mention it.
 *
 * **First-seen rather than sorted**, and that is deliberate: `derive.rs` gives
 * the hub edition to the first member it resolves, so first-seen puts the
 * edition most of this series is held in on the left, where a reader scanning
 * for gaps looks first. Sorting by key would put `en` before `raw` on a shelf
 * that is mostly raw.
 *
 * The label travels with the key because a `WorkCard` carries only the
 * editions **this caller may see** — a grant can hide an edition from volume 1
 * and not from volume 3, so the first volume to mention a key is not always
 * the first volume.
 */
function editionColumns(works: WorkCard[]): { key: string; label: string }[] {
  const seen = new Map<string, string>()
  for (const w of works) {
    for (const e of w.editions) {
      if (!seen.has(e.key)) seen.set(e.key, e.label)
    }
  }
  return [...seen].map(([key, label]) => ({ key, label }))
}
```

- [ ] **Step 4: Add the prop and the table**

Give `Series` its props:

```tsx
export interface SeriesProps {
  /** §3's `layout` switch. `'grid'` is preset A. */
  layout?: 'grid' | 'table'
}

export function Series({ layout = 'grid' }: SeriesProps) {
```

and render the table instead of the grid when it is asked for. Inside the
`{loaded && (…)}` block, after the `<h1>`:

```tsx
          {layout === 'table' ? (
            <Table works={loaded.works} />
          ) : (
            <ul className="volumes">{/* …Task 4's tiles, unchanged… */}</ul>
          )}
```

and above `Series`, the table itself:

```tsx
/**
 * One row per volume, one column per edition — §3's preset B.
 *
 * **The dashed cell is what this layout is for.** A grid says which volumes
 * exist; this says which editions each volume is *missing*, which is the
 * question someone pairing two releases actually asks. A blank cell would say
 * the same thing to a sighted reader and nothing at all to a screen reader,
 * hence the `aria-label` — §9's rule that this screen is testable in `jsdom`
 * cuts both ways: what is easy to assert is also what is easy to leave out.
 */
function Table({ works }: { works: WorkCard[] }) {
  const columns = editionColumns(works)
  return (
    <table className="editions">
      <thead>
        <tr>
          <th scope="col">Volume</th>
          {columns.map((c) => (
            <th scope="col" key={c.key}>
              {c.label}
            </th>
          ))}
        </tr>
      </thead>
      <tbody>
        {works.map((w) => (
          <tr key={w.slug}>
            <td>
              <Link to={`/read/${w.slug}`}>{w.title}</Link>
              {w.aligned && (
                <span className="aligned" title="has an aligned edition">
                  {' '}
                  ⇄
                </span>
              )}
            </td>
            {columns.map((c) => {
              const held = w.editions.find((e) => e.key === c.key)
              return held ? (
                <td key={c.key}>{held.pages}</td>
              ) : (
                <td
                  key={c.key}
                  className="missing"
                  aria-label={`no ${c.label} edition`}
                >
                  —
                </td>
              )
            })}
          </tr>
        ))}
      </tbody>
    </table>
  )
}
```

- [ ] **Step 5: Style it**

Append to `web/src/index.css`:

```css
/* §3's preset B. System colours, like every other rule in this block. */
.editions {
  border-collapse: collapse;
  width: 100%;
}

.editions th,
.editions td {
  border-bottom: 1px solid ButtonBorder;
  padding: 0.45rem 0.6rem;
  text-align: left;
  font-size: 0.9rem;
}

.editions th {
  font-weight: 600;
}

.editions a {
  color: LinkText;
  text-decoration: none;
}

.editions a:hover {
  text-decoration: underline;
}

/* A dashed border rather than a lighter colour: the cell means "this edition
 * is not here", and opacity alone reads as "loading" against a screen whose
 * covers arrive one at a time. */
.editions td.missing {
  border: 1px dashed ButtonBorder;
  color: CanvasText;
  opacity: 0.55;
  text-align: center;
}
```

- [ ] **Step 6: Run the tests**

Run: `pnpm -C web exec vitest run --project jsdom src/Series.test.tsx`
Expected: PASS, both describes.

- [ ] **Step 7: Commit**

```bash
git add web/src/Series.tsx web/src/Series.test.tsx web/src/index.css
git commit -m "series page: the table layout, and the dashed cell §3 asks for"
```

---

### Task 6: The edition selector

**Files:**
- Modify: `web/src/Series.tsx` — the `editionSelector` prop and the control
- Modify: `web/src/Series.test.tsx`
- Modify: `web/src/index.css` — the selector row

**Interfaces:**
- Consumes: Task 5's `Series`, `editionColumns`.
- Produces: `Series({ layout, editionSelector }: SeriesProps)` with
  `editionSelector?: boolean` defaulting to `true` — §3's preset A.

**What "edition as a mode" means here, concretely.** Choosing an edition
changes which edition's cover and page count each volume tile shows, and dims
the volumes that edition does not cover. It does **not** change what the reader
opens: `/read/:work` is addressed by work, and the reader picks its own edition
from the sidecar. A selector that pretended otherwise would be a control whose
effect disappears on the next screen.

**Its default value is `all`, not the first edition**, so a page with the
selector on renders exactly as one with it off until the reader touches it —
which is what makes Task 4's cover assertion still true here.

- [ ] **Step 1: Write the failing tests**

Append a third `describe` to `web/src/Series.test.tsx`:

```tsx
describe('Series, the edition selector', () => {
  beforeEach(() => {
    vi.mocked(library.oneSeries).mockResolvedValue({
      series: card(),
      works: [
        work({
          editions: [
            edition({ key: 'raw', label: 'Manga (RAW)', pages: 180 }),
            edition({ key: 'en', label: 'Manga', pages: 178 }),
          ],
        }),
        work({
          id: 2,
          slug: 'gotoubun-v03',
          title: 'Go-Toubun no Hanayome v03',
          volume: 3,
          editions: [edition({ key: 'raw', label: 'Manga (RAW)', pages: 190 })],
        }),
      ],
    })
  })

  it('offers every edition in the series, plus all of them', async () => {
    const host = await settleWith({ editionSelector: true })
    const options = [
      ...host.querySelectorAll('.edition-selector option'),
    ].map((o) => o.textContent)
    expect(options).toEqual(['All editions', 'Manga (RAW)', 'Manga'])
  })

  it('is absent when it is switched off', async () => {
    const host = await settleWith({ editionSelector: false })
    expect(host.querySelector('.edition-selector')).toBeNull()
  })

  it('draws the chosen edition’s cover', async () => {
    const host = await settleWith({ editionSelector: true })
    const select = host.querySelector('select') as HTMLSelectElement
    act(() => {
      select.value = 'en'
      select.dispatchEvent(new Event('change', { bubbles: true }))
    })
    expect(host.querySelector('.volumes img')?.getAttribute('src')).toBe(
      '/api/works/gotoubun-v01/en/pages/0/thumb',
    )
  })

  it('dims a volume the chosen edition does not cover, rather than hiding it', async () => {
    const host = await settleWith({ editionSelector: true })
    const select = host.querySelector('select') as HTMLSelectElement
    act(() => {
      select.value = 'en'
      select.dispatchEvent(new Event('change', { bubbles: true }))
    })
    const items = [...host.querySelectorAll('.volumes li')]
    expect(items).toHaveLength(2)
    expect(items[0]?.classList.contains('without')).toBe(false)
    expect(items[1]?.classList.contains('without')).toBe(true)
  })

  it('goes back to every edition when All is chosen again', async () => {
    const host = await settleWith({ editionSelector: true })
    const select = host.querySelector('select') as HTMLSelectElement
    act(() => {
      select.value = 'en'
      select.dispatchEvent(new Event('change', { bubbles: true }))
    })
    act(() => {
      select.value = 'all'
      select.dispatchEvent(new Event('change', { bubbles: true }))
    })
    expect(host.querySelector('.volumes img')?.getAttribute('src')).toBe(
      '/api/works/gotoubun-v01/raw/pages/0/thumb',
    )
    expect(
      [...host.querySelectorAll('.volumes li')].some((li) =>
        li.classList.contains('without'),
      ),
    ).toBe(false)
  })
})
```

- [ ] **Step 2: Run them to verify they fail**

Run: `pnpm -C web exec vitest run --project jsdom src/Series.test.tsx`
Expected: FAIL — **four** failures out of five tests. `is absent when it is
switched off` passes before the implementation exists, because
`.edition-selector` is legitimately absent either way; it becomes meaningful
only once there is a selector to switch off. Keep it, and say in the report
that it passed for that reason rather than implying you watched it go red.

- [ ] **Step 3: Add the prop, the state and the control**

In `web/src/Series.tsx`, extend the props:

```tsx
export interface SeriesProps {
  /** §3's `layout` switch. `'grid'` is preset A. */
  layout?: 'grid' | 'table'
  /** §3's `editionSelector` switch. On is preset A. */
  editionSelector?: boolean
}

export function Series({ layout = 'grid', editionSelector = true }: SeriesProps) {
```

Add the state beside the others:

```tsx
  // **`'all'` rather than the first edition**, so the page with the selector
  // on renders identically to the page with it off until the reader chooses.
  // A default of "the first edition" would silently dim every volume the hub
  // does not cover, on a screen nobody had touched.
  const [chosen, setChosen] = useState('all')
```

Render it above the layout, inside `{loaded && (…)}`:

```tsx
          {editionSelector && (
            <label className="edition-selector">
              <span>Edition</span>
              <select
                value={chosen}
                onChange={(e) => {
                  setChosen(e.target.value)
                }}
              >
                <option value="all">All editions</option>
                {editionColumns(loaded.works).map((c) => (
                  <option key={c.key} value={c.key}>
                    {c.label}
                  </option>
                ))}
              </select>
            </label>
          )}
```

- [ ] **Step 4: Let the choice reach the tiles**

Replace the grid's `<li>` body so the cover and the class follow `chosen`:

```tsx
            {loaded.works.map((w) => {
              // `undefined` under `'all'` means "whatever this volume leads
              // with", which is `editions[0]` — the hub, by `derive.rs`'s
              // ordering. Under a chosen key it means the volume does not have
              // that edition, which is a fact to show rather than to hide: a
              // series page that drops volumes when you pick an edition is
              // answering "which volumes exist" differently depending on a
              // control that was supposed to be about covers.
              const shown =
                chosen === 'all'
                  ? w.editions[0]
                  : w.editions.find((e) => e.key === chosen)
              return (
                <li key={w.slug} className={shown ? undefined : 'without'}>
                  <Link to={`/read/${w.slug}`}>
                    <span className="cover-box">
                      <Cover
                        src={workCover(w.slug, (shown ?? w.editions[0])?.key ?? '')}
                        title={w.title}
                      />
                      {w.aligned && (
                        <span className="aligned" title="has an aligned edition">
                          ⇄
                        </span>
                      )}
                    </span>
                    <span className="title">{w.title}</span>
                  </Link>
                </li>
              )
            })}
```

- [ ] **Step 5: Style it**

Append to `web/src/index.css`:

```css
.edition-selector {
  display: inline-flex;
  align-items: center;
  gap: 0.5rem;
  margin: 0 0 1rem;
  font-size: 0.9rem;
}

/* Dimmed, not hidden — see the component's own note.
 *
 * **Opacity *and* greyscale, because opacity alone is ambiguous here.** A tile
 * at reduced opacity on a screen whose covers arrive one at a time reads as
 * "still loading"; draining the colour out of the artwork as well reads as a
 * deliberate state, because nothing about loading makes an image monochrome.
 * The two together say "this volume exists and the edition you picked does not
 * cover it", which is the fact §3 wants shown rather than hidden. */
.volumes li.without {
  opacity: 0.5;
}

.volumes li.without img {
  filter: grayscale(1);
}
```

- [ ] **Step 6: Run the tests, then the suite**

Run: `pnpm -C web exec vitest run --project jsdom src/Series.test.tsx`
Expected: PASS.

Run: `pnpm -C web run test && pnpm -C web run test:jsdom`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add web/src/Series.tsx web/src/Series.test.tsx web/src/index.css
git commit -m "series page: the edition selector, defaulting to all of them"
```

---

### Task 7: The alignment panel

**Files:**
- Modify: `web/src/Series.tsx` — the `panel` prop, the button, the panel
- Modify: `web/src/Series.test.tsx`
- Modify: `web/src/index.css` — the panel

**Interfaces:**
- Consumes: Task 6's `Series`; `library.work(slug)` → `WorkDetail`, whose
  `alignments: AlignmentCard[]` carry `{ from, to, verdict: string | null,
  slots: number | null, paired: number | null }`.
- Produces: `Series({ layout, editionSelector, panel }: SeriesProps)` with
  `panel?: 'off' | 'button' | 'on-select'` defaulting to `'button'` — §3's
  preset A.

**Why `button` is the default, and why that is cheap rather than merely tidy.**
About 90% of series are single-edition, so a panel that opens on every
selection is mostly empty. **No list endpoint carries alignment detail** — the
verdict and the paired count come from `/api/library/works/{slug}` and from
nowhere else, deliberately, because an `alignments_of` per row would put back
the N+1 the whole-branch review closed in `home`, at 111 queries on the largest
series. So a series of 111 volumes costs one query for the list and one more
for whichever volume is asked about.

**A per-slug cache, so `on-select` is affordable.** In `on-select` mode the
panel follows the pointer and the focus ring, which without a cache is one
request per volume per hover. The cache is a `Map` in state, keyed by slug,
lasting as long as the page.

- [ ] **Step 1: Write the failing tests**

Append a fourth `describe` to `web/src/Series.test.tsx`, and add `work` to the
module mock at the top of the file:

```tsx
vi.mock('./library', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./library')>()),
  oneSeries: vi.fn(),
  work: vi.fn(),
}))
```

```tsx
describe('Series, the alignment panel', () => {
  beforeEach(() => {
    // The call-count assertions below need this, for the reason the
    // grid-layout block's own `beforeEach` documents: the counter accumulates
    // across every test in the file.
    vi.clearAllMocks()
    vi.mocked(library.oneSeries).mockResolvedValue({
      series: card(),
      works: [work({ aligned: true }), work({ id: 2, slug: 'gotoubun-v03', volume: 3 })],
    })
    vi.mocked(library.work).mockResolvedValue({
      ...work({ aligned: true }),
      alignments: [
        { from: 'raw', to: 'en', verdict: 'confident', slots: 180, paired: 178 },
        { from: 'raw', to: 'en-fan', verdict: null, slots: null, paired: null },
      ],
    })
  })

  const open = async (host: HTMLElement, index = 0) => {
    const buttons = host.querySelectorAll('.volumes .panel-open')
    await act(async () => {
      ;(buttons[index] as HTMLButtonElement).click()
      await Promise.resolve()
      await Promise.resolve()
    })
  }

  it('offers a button per volume and fetches nothing until one is pressed', async () => {
    const host = await settleWith({ panel: 'button' })
    expect(host.querySelectorAll('.volumes .panel-open')).toHaveLength(2)
    expect(vi.mocked(library.work)).not.toHaveBeenCalled()
  })

  it('shows the verdict and the paired count, which no list route carries', async () => {
    const host = await settleWith({ panel: 'button' })
    await open(host)
    const rows = [...host.querySelectorAll('.alignment-panel li')].map(
      (li) => li.textContent,
    )
    expect(rows[0]).toContain('raw → en')
    expect(rows[0]).toContain('confident')
    expect(rows[0]).toContain('178 of 180')
  })

  it('says an unaligned pair is unaligned rather than printing null', async () => {
    const host = await settleWith({ panel: 'button' })
    await open(host)
    const rows = [...host.querySelectorAll('.alignment-panel li')].map(
      (li) => li.textContent,
    )
    expect(rows[1]).toContain('raw → en-fan')
    expect(rows[1]).toContain('not aligned')
    expect(rows[1]).not.toContain('null')
  })

  it('fetches one volume once, however many times it is opened', async () => {
    const host = await settleWith({ panel: 'button' })
    await open(host)
    await open(host)
    expect(vi.mocked(library.work)).toHaveBeenCalledTimes(1)
    expect(vi.mocked(library.work)).toHaveBeenCalledWith('gotoubun-v01')
  })

  it('offers no button at all when the panel is off', async () => {
    const host = await settleWith({ panel: 'off' })
    expect(host.querySelector('.panel-open')).toBeNull()
    expect(host.querySelector('.alignment-panel')).toBeNull()
  })

  it('opens on focus when it is set to follow the selection', async () => {
    const host = await settleWith({ panel: 'on-select' })
    expect(host.querySelector('.panel-open')).toBeNull()
    const link = host.querySelector('.volumes a') as HTMLAnchorElement
    await act(async () => {
      link.dispatchEvent(new FocusEvent('focusin', { bubbles: true }))
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(host.querySelector('.alignment-panel')).not.toBeNull()
  })

  it('says so when the detail cannot be read', async () => {
    vi.mocked(library.work).mockRejectedValue(
      new Error('/api/library/works/gotoubun-v01: 500'),
    )
    const host = await settleWith({ panel: 'button' })
    await open(host)
    expect(host.querySelector('.alignment-panel')?.textContent).toContain('500')
  })
})
```

- [ ] **Step 2: Run them to verify they fail**

Run: `pnpm -C web exec vitest run --project jsdom src/Series.test.tsx`
Expected: FAIL — **six** of the seven. Vitest does not type-check at runtime,
so the unknown `panel` prop is a `tsc` error rather than a test failure, and
one test ("offers no button at all when the panel is off") passes vacuously
before there is a button to withhold.

**Two of these seven tests are too weak as drafted, and the implementer is
expected to strengthen them.** Both were proved to pass against real bugs by
mutation: "fetches one volume once" survives a *wrong cache key*, and "opens on
focus" survives an implementation where focusing any volume reveals the first.
Make the first assert which slug was fetched and that a second volume still
costs a request; make the second assert the panel opened *for that volume*.
Add a third: that a click still navigates rather than being swallowed by the
`on-select` handlers.

- [ ] **Step 3: Add the prop, the cache and the panel**

In `web/src/Series.tsx`, extend the props and add the state:

```tsx
export interface SeriesProps {
  /** §3's `layout` switch. `'grid'` is preset A. */
  layout?: 'grid' | 'table'
  /** §3's `editionSelector` switch. On is preset A. */
  editionSelector?: boolean
  /** §3's `panel` switch. `'button'` is preset A, and is the owner's correction to mockup C. */
  panel?: 'off' | 'button' | 'on-select'
}

export function Series({
  layout = 'grid',
  editionSelector = true,
  panel = 'button',
}: SeriesProps) {
```

```tsx
  const [open, setOpen] = useState<string | undefined>()
  // Keyed by slug and kept for the life of the page. **The cache is what makes
  // `on-select` affordable**: the panel then follows the pointer and the focus
  // ring, and without this that is one request per volume per hover. A failure
  // is cached too, as its own message — retrying on every re-render would turn
  // a 500 into a request loop.
  const [detail, setDetail] = useState<
    Record<string, { alignments: AlignmentCard[] } | { error: string }>
  >({})

  // **In flight is tracked separately from settled, and it has to be.**
  // `detail[slug]` is only set once a request *resolves*, so gating on it
  // alone blocks a second fetch after the first finishes and not during it —
  // and in `on-select` mode the same tile carries both `onMouseEnter` and
  // `onFocusCapture`, while clicking a link moves focus to it. So "hover, then
  // click", the ordinary way a mouse user opens a volume, fired two identical
  // requests. A ref rather than state: this must not re-render, and it must
  // not seed `detail` with a sentinel, because `detail[slug] === undefined` is
  // what renders the panel's "Reading…" state.
  const pending = useRef(new Set<string>())

  const reveal = (slug: string) => {
    setOpen(slug)
    if (detail[slug] !== undefined || pending.current.has(slug)) return
    pending.current.add(slug)
    fetchWork(slug)
      .then((w) => {
        setDetail((prev) => ({ ...prev, [slug]: { alignments: w.alignments } }))
      })
      .catch((err: unknown) => {
        setDetail((prev) => ({
          ...prev,
          [slug]: { error: err instanceof Error ? err.message : String(err) },
        }))
      })
  }
```

with the imports extended:

```tsx
import {
  type AlignmentCard,
  oneSeries as fetchSeries,
  type SeriesCard,
  work as fetchWork,
  type WorkCard,
  workCover,
} from './library'
```

Add the panel component above `Series`:

```tsx
/**
 * One volume's alignment detail — §3's panel.
 *
 * **`verdict` and `paired` come from `/api/library/works/{slug}` and from
 * nowhere else.** `WorkCard.aligned` is a bool, deliberately: an
 * `alignments_of` per row would put back the N+1 the whole-branch review
 * closed in `home`, at 111 queries on the largest series. That is what makes
 * the `button` default cheap rather than merely tidy.
 *
 * A pair with no verdict is **stated as unaligned**, not left blank. The two
 * are different facts — "these editions have no alignment" against "the panel
 * has not loaded" — and a blank cell says the second while meaning the first.
 */
function AlignmentPanel({
  state,
}: {
  state: { alignments: AlignmentCard[] } | { error: string } | undefined
}) {
  if (state === undefined)
    return (
      <div className="alignment-panel">
        <p>Reading…</p>
      </div>
    )
  if ('error' in state)
    return (
      <div className="alignment-panel">
        <p role="alert">{state.error}</p>
      </div>
    )
  if (state.alignments.length === 0)
    return (
      <div className="alignment-panel">
        <p>This volume has one edition, so there is nothing to pair it with.</p>
      </div>
    )
  return (
    <div className="alignment-panel">
      <ul>
        {state.alignments.map((a) => (
          <li key={`${a.from}→${a.to}`}>
            <span className="pair">
              {a.from} → {a.to}
            </span>{' '}
            {a.verdict === null ? (
              <span className="verdict">not aligned</span>
            ) : (
              <>
                <span className="verdict">{a.verdict}</span>
                {a.paired !== null && a.slots !== null && (
                  <span className="paired">
                    {' '}
                    · {a.paired} of {a.slots}
                  </span>
                )}
              </>
            )}
          </li>
        ))}
      </ul>
    </div>
  )
}
```

- [ ] **Step 4: Attach it to the tiles**

Inside the grid's `<li>`, after the `<Link>`:

```tsx
                  {panel === 'button' && (
                    <button
                      type="button"
                      className="panel-open"
                      aria-expanded={open === w.slug}
                      aria-label={`alignment detail for ${w.title}`}
                      onClick={() => {
                        reveal(w.slug)
                      }}
                    >
                      i
                    </button>
                  )}
                  {open === w.slug && <AlignmentPanel state={detail[w.slug]} />}
```

and give the `<li>` the focus handler `on-select` needs:

```tsx
                <li
                  key={w.slug}
                  className={shown ? undefined : 'without'}
                  onFocusCapture={
                    panel === 'on-select'
                      ? () => {
                          reveal(w.slug)
                        }
                      : undefined
                  }
                  onMouseEnter={
                    panel === 'on-select'
                      ? () => {
                          reveal(w.slug)
                        }
                      : undefined
                  }
                >
```

**Hover and focus both**, because "select" with a mouse and "select" with a
keyboard are the same intent and only one of them is a click. A click still
navigates: nothing here calls `preventDefault`.

**On capture versus bubble, say only what is true.** An earlier draft of this
plan claimed `onFocusCapture` was needed because "capture fires for a child
link's focus without the `<li>` itself being focusable" — that is wrong, and
was disproved by swapping one for the other and re-running the file. React maps
both to the bubbling native `focusin`, and this `<li>` is on that event's path
either way. Keep capture if you like, for the phase a later bubble-phase
`stopPropagation` could not defeat — but write it as a guard against a bug that
does not exist yet, which is what it is.

- [ ] **Step 5: Style it**

Append to `web/src/index.css`:

```css
.volumes .panel-open {
  margin-top: 0.15rem;
  padding: 0 0.4rem;
  border: 1px solid ButtonBorder;
  border-radius: 999px;
  background: ButtonFace;
  color: CanvasText;
  font-size: 0.75rem;
  line-height: 1.5;
  cursor: pointer;
}

.alignment-panel {
  margin-top: 0.35rem;
  padding: 0.5rem 0.6rem;
  border: 1px solid ButtonBorder;
  border-radius: 6px;
  background: ButtonFace;
  color: CanvasText;
  font-size: 0.8rem;
}

.alignment-panel ul {
  list-style: none;
  margin: 0;
  padding: 0;
  display: grid;
  gap: 0.25rem;
}

.alignment-panel .pair {
  font-weight: 600;
}

.alignment-panel .paired {
  opacity: 0.75;
}
```

- [ ] **Step 6: Run the tests, then the suite**

Run: `pnpm -C web exec vitest run --project jsdom src/Series.test.tsx`
Expected: PASS, four describes.

Run: `pnpm -C web run test && pnpm -C web run test:jsdom`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add web/src/Series.tsx web/src/Series.test.tsx web/src/index.css
git commit -m "series page: the alignment panel, and the one route that carries a verdict"
```

---
### Task 8: `FieldControl` moves out of the reader's panel

**Files:**
- Create: `web/src/settingsFields.tsx` — `FieldControl`, unchanged but for its styling prop
- Create: `web/src/settingsFields.test.tsx`
- Modify: `web/src/Settings.tsx` — import it, delete the local copy, pass `rowStyle`

**Interfaces:**
- Consumes: `settingsPanel.ts`'s `Field`, `fieldKind`, `humanize`, `nestedCount`, `nestedSummary` — all shipped.
- Produces: `FieldControl({ field, value, onChange, rowStyle }: { field: Field; value: unknown; onChange: (raw: unknown) => void; rowStyle: CSSProperties })`. Task 9's browse settings render with it.

**A pure refactor with one prop added, and it must stay pure.** `Settings.tsx`
is the reader's panel: a centred modal on `rgba(24,24,24,0.96)`, correct
because it draws over artwork. The browse screens are light-and-dark chrome.
The *inference* is what §4 promises is reused — "a control derived from a
preference's runtime type" — and copying sixty lines of it into a second file
would make that promise false the first time one of them changed.
`rowStyle` is the only thing the two disagree about.

**Why not reuse `Settings.tsx` whole.** It hardcodes a dark palette, and this
branch has already shipped that mistake once: `Login.tsx` was `#111` on a light
library and had to be fixed in `a6450f3` after somebody opened it in a browser.

- [ ] **Step 1: Write the failing test**

Create `web/src/settingsFields.test.tsx`:

```tsx
import { act } from 'react'
import { describe, expect, it, vi } from 'vitest'
import { FieldControl } from './settingsFields'
import { mount } from './testing/mount'

const noStyle = {}

describe('FieldControl', () => {
  it('renders a checkbox for a boolean and reports the new value', () => {
    const onChange = vi.fn()
    const host = mount(
      <FieldControl
        field={{ key: 'continueStrip', kind: 'boolean' }}
        value={true}
        onChange={onChange}
        rowStyle={noStyle}
      />,
    )
    const box = host.querySelector('input[type="checkbox"]') as HTMLInputElement
    expect(box.checked).toBe(true)
    act(() => {
      box.click()
    })
    expect(onChange).toHaveBeenCalledWith(false)
  })

  // **One option must be camelCase or this test proves nothing.** `humanize`
  // splits camelCase and lowercases; it does not touch a hyphen, so it is a
  // no-op on both `'off'` and `'on-select'` — and a version of this test using
  // only those two passed with `humanize()` deleted from the options render.
  // Whether `FieldControl` humanizes its *options*, as distinct from its field
  // key, is a behaviour nothing else covers: `settingsPanel.test.ts` tests
  // `humanize` itself, not who calls it.
  it('renders a select for a string with options, humanised', () => {
    const host = mount(
      <FieldControl
        field={{
          key: 'panel',
          kind: 'select',
          options: ['off', 'on-select', 'onSelect'],
        }}
        value={'off'}
        onChange={() => {}}
        rowStyle={noStyle}
      />,
    )
    expect([...host.querySelectorAll('option')].map((o) => o.textContent)).toEqual(
      ['off', 'on-select', 'on select'],
    )
  })

  it('summarises an object rather than inventing a control for it', () => {
    const host = mount(
      <FieldControl
        field={{ key: 'breaks', kind: 'object' }}
        value={{ a: 1, b: 2 }}
        onChange={() => {}}
        rowStyle={noStyle}
      />,
    )
    expect(host.textContent).toContain('2')
    expect(host.querySelector('input')).toBeNull()
  })

  // **The value must be unique and must carry no bare zero.** `'6px 0'` — the
  // first draft — was wrong twice over. It is `Settings.tsx`'s own `rowStyle`
  // value, so the test would pass against a bug that ignored the prop and kept
  // using a leftover module constant; and jsdom's `CSSStyleDeclaration`
  // normalises the shorthand's bare `0` to `0px` on read-back, so `.toBe` could
  // never match even against a correct implementation. A test that is both
  // vacuous and impossible is the whole failure mode this plan keeps hitting,
  // in one line.
  it('takes its row styling from its caller', () => {
    const host = mount(
      <FieldControl
        field={{ key: 'x', kind: 'boolean' }}
        value={false}
        onChange={() => {}}
        rowStyle={{ padding: '3px 9px' }}
      />,
    )
    expect((host.firstElementChild as HTMLElement).style.padding).toBe(
      '3px 9px',
    )
  })
})
```

- [ ] **Step 2: Run it to verify it fails**

Run: `pnpm -C web exec vitest run --project jsdom src/settingsFields.test.tsx`
Expected: FAIL — `Failed to resolve import "./settingsFields"`.

- [ ] **Step 3: Move the component**

Create `web/src/settingsFields.tsx` holding `Settings.tsx`'s `FieldControl`
**verbatim**, with three changes and no others:

1. the `rowStyle` module constant becomes a required prop of the same name and
   type `CSSProperties`;
2. it is `export`ed;
3. its imports come from `./settingsPanel` rather than from the file's own
   scope.

Its doc comment travels with it, plus one line about why it is here:

```tsx
/**
 * One field's control — inferred purely from `field.kind`, which
 * `settingsPanel.ts`'s `fieldKind` already derived from the preference's own
 * runtime type. Nothing here branches on which module or which key this is.
 *
 * **Its own file because two panels render it and they disagree about exactly
 * one thing.** The reader's `Settings.tsx` is a centred modal over artwork and
 * is dark on purpose; the browse screens are UI chrome and follow
 * `color-scheme`. `rowStyle` is that disagreement, passed in. Everything else —
 * the inference §4 calls "generated rather than hoped" — is shared, which is
 * the only way that claim stays true when one of the two changes.
 */
```

- [ ] **Step 4: Point `Settings.tsx` at it**

Delete the local `FieldControl` from `web/src/Settings.tsx`, import the new one,
and pass the module's existing `rowStyle` constant at its single call site.
`rowStyle` itself stays in `Settings.tsx` — it is that panel's styling, not a
shared value.

- [ ] **Step 5: Run the tests**

Run: `pnpm -C web exec vitest run --project jsdom src/settingsFields.test.tsx src/Settings.test.tsx`
Expected: PASS both. **`Settings.test.tsx` must pass unchanged.** If it needs
an edit, this stopped being a pure refactor — stop and report what changed.

- [ ] **Step 6: Commit**

```bash
git add web/src/settingsFields.tsx web/src/settingsFields.test.tsx web/src/Settings.tsx
git commit -m "settings: the generated control moves out of the reader's modal"
```

---

### Task 9: The `browse` module, its registry, and the generated settings

**Files:**
- Create: `web/src/browseModule.ts` — the module, the second registry, the preference context
- Create: `web/src/browseModule.test.ts`
- Create: `web/src/BrowseSettings.tsx`
- Create: `web/src/BrowseSettings.test.tsx`
- Modify: `web/src/Shell.tsx` — own the preference state, render the settings, pass `showRoots`
- Modify: `web/src/Library.tsx` — read `continueStrip` and `density`
- Modify: `web/src/Series.tsx` — take its three switches from the preferences
- Modify: `web/src/Routes.tsx` — `<Series />` needs no props; `Shell` supplies them
- Modify: `web/src/index.css` — the settings block in the sidebar, and `density`

**Interfaces:**
- Consumes: `module.ts`'s `ReaderModule`, `buildRegistry`, `initialPrefs`, `persistPrefs`, `fieldChanged`, `sliceReset`, `sliceFor` — every one of them generic over whichever registry it is handed; `prefs.ts`'s `loadPrefs`, `savePrefs`, `resetPrefs`; `settingsPanel.ts`'s `fieldsOf(defaults, options)`; Task 8's `FieldControl`.
- Produces: `BrowsePrefs`; `browseModule: ReaderModule<BrowsePrefs>`; `browseRegistry: Registry`; `BrowsePrefsContext`; `useBrowsePrefs(): BrowsePrefs`. Task 10's search screen reads `density` from the same context.

**A second registry, not an entry in the existing one** (§4).
`modules/registry.ts` is *"every reading behaviour this build ships"*, built at
module scope so `buildRegistry`'s collision check fires on import. A browse
module is not a reading behaviour, and its keys live in a different collision
domain — `/` for search on a browse screen has nothing to argue with a reader
key about, and folding them together would forbid that for no benefit.
`buildRegistry` and the settings generation are reused verbatim; only the list
is separate.

- [ ] **Step 1: Write the failing tests for the module**

Create `web/src/browseModule.test.ts`:

```ts
import { describe, expect, it } from 'vitest'
import { browseModule, browseRegistry } from './browseModule'
import { fieldsOf } from './settingsPanel'

describe('the browse module', () => {
  it('declares §3’s preset A as its defaults', () => {
    expect(browseModule.defaults).toEqual({
      layout: 'grid',
      editionSelector: true,
      panel: 'button',
      density: 'comfortable',
      continueStrip: true,
      showRoots: false,
    })
  })

  it('generates a control for every preference, and a choice where there are options', () => {
    const fields = fieldsOf(browseModule.defaults, browseModule.options)
    expect(fields.map((f) => [f.key, f.kind])).toEqual([
      ['layout', 'select'],
      ['editionSelector', 'boolean'],
      ['panel', 'select'],
      ['density', 'select'],
      ['continueStrip', 'boolean'],
      ['showRoots', 'boolean'],
    ])
  })

  it('offers exactly the values §3’s table names', () => {
    const fields = fieldsOf(browseModule.defaults, browseModule.options)
    const options = (key: string) =>
      fields.find((f) => f.key === key)?.options
    expect(options('layout')).toEqual(['grid', 'table'])
    expect(options('panel')).toEqual(['off', 'button', 'on-select'])
    expect(options('density')).toEqual(['compact', 'comfortable'])
  })

  it('is a registry of its own, holding exactly one module', () => {
    expect(browseRegistry.ids).toEqual(['browse'])
    expect(browseRegistry.get('browse')).toBe(browseModule)
  })

  it('claims no key, so it can never collide with a reader’s', () => {
    expect(browseModule.keys ?? []).toEqual([])
  })
})
```

`fieldsOf(defaults, options)` takes the two values, not the module —
`Settings.tsx:215` calls it exactly that way. It maps `Object.entries(defaults)`,
so the field order above is the order `browseModule.defaults` is written in;
keep the two in step.

- [ ] **Step 2: Run it to verify it fails**

Run: `pnpm -C web exec vitest run --project node src/browseModule.test.ts`
Expected: FAIL — `Failed to resolve import "./browseModule"`.

- [ ] **Step 3: Write the module**

Create `web/src/browseModule.ts`:

```ts
/**
 * The browse screens' preferences — spec C1 §4 and §11 step 8.
 *
 * **A second registry, not an entry in `modules/registry.ts`.** That file is
 * "every reading behaviour this build ships"; a browse screen is not one, and
 * its keys live in a different collision domain — `/` for search here has
 * nothing to argue with a reader key about, and one registry would forbid that
 * for no benefit. `buildRegistry` is reused unchanged, so a second module
 * added here gets the same import-time duplicate-id and key-collision check
 * the reader's list has.
 *
 * **Nothing here is a settings screen.** `settingsPanel.ts`'s `fieldKind`
 * derives a control from a preference's runtime type — boolean to checkbox,
 * a string whose module declares `options` to a select — so the six values
 * below are the whole declaration and `BrowseSettings.tsx` renders whatever
 * they turn out to be.
 *
 * **The honest cost**, recorded in §4 and not softened here: the reader's
 * module system works because each module is a small *behaviour*, and this is
 * the first one that owns whole screens. Two presentations of one dataset are
 * genuinely two components to keep in step.
 */
import { createContext, useContext } from 'react'
import { buildRegistry, type ReaderModule } from './module'

export interface BrowsePrefs {
  /** §3's two presentations. */
  layout: 'grid' | 'table'
  /** §3's edition-as-a-mode switch. */
  editionSelector: boolean
  /**
   * §3's alignment panel. `'button'` rather than `'on-select'` is the owner's
   * correction to mockup C, and it is right for this library: with about 90%
   * of series single-edition, a panel that opens on every selection is mostly
   * empty, while a small `i` on the cover costs nothing until it is wanted.
   */
  panel: 'off' | 'button' | 'on-select'
  /** How tightly the grids pack. */
  density: 'compact' | 'comfortable'
  /** Whether the library screen opens on what you were reading. */
  continueStrip: boolean
  /**
   * Whether the sidebar lists roots.
   *
   * **Defaults to `false` here and to the account's `admin` bit in `Shell`.**
   * §4 names `false`; an admin who has never opened settings should still see
   * the list Plan A gave them, so the *stored* value wins over a default
   * computed at load. A root is a storage fact and no navigation path may
   * require one — see `Sidebar.tsx`.
   */
  showRoots: boolean
}

export const browseModule: ReaderModule<BrowsePrefs> = {
  id: 'browse',
  label: 'Library',
  defaults: {
    layout: 'grid',
    editionSelector: true,
    panel: 'button',
    density: 'comfortable',
    continueStrip: true,
    showRoots: false,
  },
  options: {
    layout: ['grid', 'table'],
    panel: ['off', 'button', 'on-select'],
    density: ['compact', 'comfortable'],
  },
}

/**
 * Built at module scope, exactly as `modules/registry.ts` is and for the same
 * reason: `buildRegistry`'s checks run on import, before any component mounts,
 * rather than on a first render an error boundary could swallow.
 */
export const browseRegistry = buildRegistry([browseModule])

/**
 * `browseModule.defaults` where no shell sits above the reader of this
 * context — a test mounting one screen on its own. Never `undefined`: a screen
 * that cannot get preferences should draw preset A, not nothing.
 */
export const BrowsePrefsContext = createContext<BrowsePrefs>(
  browseModule.defaults,
)

export function useBrowsePrefs(): BrowsePrefs {
  return useContext(BrowsePrefsContext)
}
```

- [ ] **Step 4: Run it**

Run: `pnpm -C web exec vitest run --project node src/browseModule.test.ts`
Expected: PASS, five tests.

- [ ] **Step 5: Write the failing test for the panel**

Create `web/src/BrowseSettings.test.tsx`:

```tsx
import { act } from 'react'
import { describe, expect, it, vi } from 'vitest'
import { browseModule } from './browseModule'
import { BrowseSettings } from './BrowseSettings'
import { mount } from './testing/mount'

describe('BrowseSettings', () => {
  it('renders one control per preference, generated rather than named', () => {
    const host = mount(
      <BrowseSettings
        prefs={browseModule.defaults}
        onChange={() => {}}
        onReset={() => {}}
      />,
    )
    expect(host.querySelectorAll('label')).toHaveLength(6)
    expect([...host.querySelectorAll('select')]).toHaveLength(3)
  })

  it('reports a changed choice as the value, not the event', () => {
    const onChange = vi.fn()
    const host = mount(
      <BrowseSettings
        prefs={browseModule.defaults}
        onChange={onChange}
        onReset={() => {}}
      />,
    )
    const select = host.querySelector('select') as HTMLSelectElement
    act(() => {
      select.value = 'table'
      select.dispatchEvent(new Event('change', { bubbles: true }))
    })
    expect(onChange).toHaveBeenCalledWith('layout', 'table')
  })

  it('offers a reset', () => {
    const onReset = vi.fn()
    const host = mount(
      <BrowseSettings
        prefs={browseModule.defaults}
        onChange={() => {}}
        onReset={onReset}
      />,
    )
    act(() => {
      ;(host.querySelector('button') as HTMLButtonElement).click()
    })
    expect(onReset).toHaveBeenCalledTimes(1)
  })
})
```

- [ ] **Step 6: Write the panel**

Create `web/src/BrowseSettings.tsx`:

```tsx
/**
 * The browse screens' settings, in the sidebar — spec C1 §4.
 *
 * **Generated, not written.** One control per key of `browseModule.defaults`,
 * inferred by `settingsPanel.ts` from each value's runtime type. A seventh
 * preference needs no edit here.
 *
 * **Not `Settings.tsx`.** That panel is the reader's: a centred modal on a
 * near-black slab, correct because it draws over artwork. This is sidebar
 * chrome and follows `color-scheme`, so it shares the control (Task 8's
 * `settingsFields.tsx`) and not the chrome. `Login.tsx` is why that
 * distinction is written down rather than assumed.
 */
import type { CSSProperties } from 'react'
import { type BrowsePrefs, browseModule } from './browseModule'
import { FieldControl } from './settingsFields'
import { fieldsOf } from './settingsPanel'

const rowStyle: CSSProperties = {
  display: 'flex',
  alignItems: 'center',
  justifyContent: 'space-between',
  gap: 8,
  padding: '4px 0',
  fontSize: '0.85rem',
}

export interface BrowseSettingsProps {
  prefs: BrowsePrefs
  /**
   * One preference changed, **uncoerced** — `raw` is whatever the control's
   * own change event handed back, which for a number input is a string.
   * `module.ts`'s `fieldChanged` is what coerces it, through
   * `settingsPanel.ts`'s `withFieldValue`. This is `Settings.tsx`'s contract
   * verbatim; see its own `onChange` doc and the test named "reports a number
   * field change as the raw string, uncoerced".
   */
  onChange: (key: string, raw: unknown) => void
  onReset: () => void
}

export function BrowseSettings({
  prefs,
  onChange,
  onReset,
}: BrowseSettingsProps) {
  return (
    <section className="browse-settings">
      <h2 className="sidebar-heading">{browseModule.label}</h2>
      {fieldsOf(browseModule.defaults, browseModule.options).map((field) => (
        <FieldControl
          key={field.key}
          field={field}
          value={prefs[field.key as keyof BrowsePrefs]}
          rowStyle={rowStyle}
          onChange={(raw) => {
            onChange(field.key, raw)
          }}
        />
      ))}
      <button type="button" onClick={onReset}>
        Reset
      </button>
    </section>
  )
}
```

- [ ] **Step 7: Own the state in `Shell`**

In `web/src/Shell.tsx`, add the imports:

```tsx
import {
  BrowsePrefsContext,
  browseModule,
  browseRegistry,
} from './browseModule'
import { BrowseSettings } from './BrowseSettings'
import {
  fieldChanged,
  initialPrefs,
  persistPrefs,
  sliceFor,
  sliceReset,
} from './module'
import { loadPrefs, resetPrefs, savePrefs } from './prefs'
```

and the state:

```tsx
  // **The reader's own preference machinery, unchanged.** `initialPrefs`,
  // `fieldChanged`, `persistPrefs` and `sliceReset` are `module.ts`'s and are
  // generic over whatever registry they are handed — which is what §4 means
  // by "a second registry, not a second implementation". `App.tsx` holds the
  // reader's map exactly this way.
  const [map, setMap] = useState<Record<string, object>>(() =>
    initialPrefs(browseRegistry.all(), (id, defaults) =>
      // **`admin` folded into the defaults, not into the stored value.**
      // `loadPrefs` merges what is stored over what it is given, so an account
      // that has never opened settings gets the roots list Plan A gave it, and
      // one that switched the list off keeps it off. The gate resolves before
      // any route mounts, so `admin` is known on this first render.
      loadPrefs(id, { ...defaults, showRoots: admin }),
    ),
  )
  const prefs = sliceFor(map, browseModule)

  const change = (key: string, raw: unknown) => {
    setMap((prev) => {
      const next = fieldChanged(prev, browseRegistry, browseModule.id, key, raw)
      persistPrefs(browseRegistry.all(), next, savePrefs)
      return next
    })
  }

  const reset = () => {
    resetPrefs(browseModule.id)
    setMap((prev) => sliceReset(prev, browseRegistry, browseModule.id))
  }
```

**Both halves of the reset are needed and neither is enough** — `module.ts`'s
`sliceReset` says so: `resetPrefs` alone satisfies "reload, confirm it
persisted" and silently fails "change a setting, confirm it takes effect".

**`sliceReset` restores `browseModule.defaults`, so `showRoots` resets to
`false` even for an admin.** That is the module's declared default and the
right answer for a control the reader just asked to reset; the admin fold is a
*first-run* default, not a floor.

and use them:

```tsx
    <BrowsePrefsContext.Provider value={prefs}>
      <StatusContext.Provider value={status}>
        <div className="library">
          <div className="sidebar-column">
            <Sidebar roots={status?.roots ?? []} showRoots={prefs.showRoots} />
            <BrowseSettings prefs={prefs} onChange={change} onReset={reset} />
          </div>
          <main>
            {failure !== undefined && <p role="alert">{failure}</p>}
            {children}
          </main>
        </div>
      </StatusContext.Provider>
    </BrowsePrefsContext.Provider>
```

with `.sidebar-column` added to `index.css`:

**Edit the existing `.sidebar` rule; do not override it further down.** It
currently carries `border-right: 1px solid ButtonBorder;` and
`overflow-y: auto;` — both move to the new column that holds the sidebar and
the settings, and `.sidebar` keeps only its padding. A second `.sidebar` block
later in the file would leave two rules disagreeing about the same property,
which is how a stylesheet stops being readable.

```css
/* was `.sidebar`; the border and the scroll belong to the column holding both
 * the navigation and the settings, so the two scroll together. */
.sidebar-column {
  border-right: 1px solid ButtonBorder;
  overflow-y: auto;
}

.sidebar {
  padding: 1rem;
}

.browse-settings {
  padding: 0 1rem 1rem;
}

.browse-settings button {
  margin-top: 0.5rem;
  padding: 0.25rem 0.6rem;
  border: 1px solid ButtonBorder;
  border-radius: 4px;
  background: ButtonFace;
  color: CanvasText;
  font-size: 0.8rem;
  cursor: pointer;
}

/* §4's `density`. Compact is the same grid with less room per tile — not a
 * different layout, which is the whole argument of §3. */
.library.compact .grid,
.library.compact .volumes {
  grid-template-columns: repeat(auto-fill, minmax(6.5rem, 1fr));
  gap: 0.6rem;
}
```

and the container carrying it: `<div className={`library ${prefs.density === 'compact' ? 'compact' : ''}`.trim()}>`.

- [ ] **Step 8: Let the screens read it**

In `web/src/Library.tsx`, replace the unconditional continue strip with

```tsx
        {prefs.continueStrip && home && home.continuing.length > 0 && (
```

reading `const prefs = useBrowsePrefs()` at the top.

In `web/src/Series.tsx`, delete the three props and read them instead:

```tsx
export function Series() {
  const { layout, editionSelector, panel } = useBrowsePrefs()
```

`Series.test.tsx`'s `settleWith(props)` then wraps in a
`BrowsePrefsContext.Provider` carrying `{ ...browseModule.defaults, ...props }`
rather than spreading props onto the component. Change the helper, not the
tests below it.

In `web/src/Routes.tsx`, `<Series />` takes no props.

- [ ] **Step 9: Run everything**

Run: `pnpm -C web run test && pnpm -C web run test:jsdom`
Expected: PASS.

Run: `pnpm -C web run lint && pnpm -C web run typecheck`
Expected: clean. `browseModule.ts` exports no component and
`BrowseSettings.tsx` exports only one, which is what
`style/useComponentExportOnlyModules` asks for.

- [ ] **Step 10: Commit**

```bash
git add web/src/browseModule.ts web/src/browseModule.test.ts web/src/BrowseSettings.tsx web/src/BrowseSettings.test.tsx web/src/Shell.tsx web/src/Library.tsx web/src/Series.tsx web/src/Series.test.tsx web/src/Routes.tsx web/src/index.css
git commit -m "browse module: §3's three switches as generated settings, in a registry of their own"
```

---
### Task 10: Search

**Files:**
- Create: `web/src/SeriesGrid.tsx` — the tiles, extracted so two screens draw them identically
- Create: `web/src/Search.tsx`
- Create: `web/src/Search.test.tsx`
- Modify: `web/src/Library.tsx` — render `SeriesGrid` instead of its own `<ul>`
- Modify: `web/src/Routes.tsx` — `/search` renders `Search`; `NotBuiltYet` goes
- Modify: `web/src/Routes.test.tsx` — the same trap Task 4 hit: its `vi.mock('./library', …)` will need a `search` fixture, and its `/search` test still asserts the `NotBuiltYet` placeholder
- Modify: `web/src/Library.test.tsx` — the grid's markup moves, its assertions follow

**Interfaces:**
- Consumes: Tasks 1 and 2's Rust; `library.search(q)` → `SearchHits { series, unfiled, query }`; `debounce.ts`; `EmptyState` with its `query` prop, which shipped in Plan A and has had no caller.
- Produces: `SeriesGrid({ series, unfiled }: { series: SeriesCard[]; unfiled: WorkCard[] })`; `Search()`.

**`?q=` lives in the URL.** A search someone wants to send to themselves, or to
come back to, is a URL — and `useSearchParams` is already how `?root=` works on
the library screen. The input is the URL's value, debounced on the way in.

**Out-of-order responses are the defect this screen invites.** Typing `五等分`
fires three requests, and nothing guarantees they land in that order. A
sequence number, compared on arrival, is what makes the last *typed* query the
one on screen rather than the last *answered* one.

- [ ] **Step 1: Extract the grid**

Create `web/src/SeriesGrid.tsx` holding `Library.tsx`'s `<ul className="grid">`
**verbatim** — `Cover` is already its own file, from Task 4 — as:

```tsx
/**
 * The library's tiles: series, then the works that belong to none.
 *
 * **One component because two screens draw the same thing.** `/` and `/search`
 * both answer with §5.4's `{ series, unfiled }` shape — deliberately, so one
 * component renders either — and two copies of a cover fallback is two places
 * for the `⇄` mark to end up in different positions, which is exactly what
 * Plan A's ledger carried forward.
 */
export function SeriesGrid({
  series,
  unfiled,
}: {
  series: SeriesCard[]
  unfiled: WorkCard[]
}) {
```

`Library.tsx` then renders
`<SeriesGrid series={visible} unfiled={gridData?.unfiled ?? []} />` and keeps
only its chips, its continue strip and its empty states.

**Keep the `?.`.** `gridData` can still be `undefined` in that branch —
TypeScript does not narrow it from the surrounding conjunction — and
`shell.ts`'s doc comment reasons explicitly about `Library` rendering the grid
branch before its data has arrived. Written as `gridData.unfiled`, the first
render crashes and that comment becomes false in the same stroke.

- [ ] **Step 2: Write the failing tests**

Create `web/src/Search.test.tsx`:

```tsx
import { act } from 'react'
import { MemoryRouter } from 'react-router'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import * as library from './library'
import { Search } from './Search'
import { mount } from './testing/mount'

vi.mock('./library', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./library')>()),
  search: vi.fn(),
}))

const card = (over: Partial<library.SeriesCard> = {}): library.SeriesCard => ({
  id: 7,
  title: '五等分の花嫁',
  works: 3,
  editions: 5,
  aligned: 1,
  added: 3,
  unread: 2,
  cover: '/api/library/series/7/cover',
  ...over,
})

async function settle(entry = '/search'): Promise<HTMLElement> {
  let host: HTMLElement | undefined
  await act(async () => {
    host = mount(
      <MemoryRouter initialEntries={[entry]}>
        <Search />
      </MemoryRouter>,
    )
    await Promise.resolve()
    await Promise.resolve()
  })
  if (!host) throw new Error('nothing mounted')
  return host
}

/** Type into the box and let the debounce and the fetch settle. */
async function type(host: HTMLElement, q: string) {
  const box = host.querySelector('input[type="search"]') as HTMLInputElement
  await act(async () => {
    // **The native setter, not `box.value = q`.** React installs a
    // per-instance value tracker over `value` on a controlled input, so a
    // plain assignment updates the tracker along with the DOM property, and
    // the `input` event that follows finds nothing changed and never calls
    // `onChange`. Seven test files here already do it this way —
    // `Login.test.tsx` and `Account.test.tsx` among them. Written as a plain
    // assignment, four of this file's six tests fail against a *correct*
    // `Search.tsx`.
    const setter = Object.getOwnPropertyDescriptor(
      HTMLInputElement.prototype,
      'value',
    )?.set
    setter?.call(box, q)
    box.dispatchEvent(new Event('input', { bubbles: true }))
  })
  await act(async () => {
    vi.advanceTimersByTime(300)
    await Promise.resolve()
    await Promise.resolve()
  })
}

describe('Search', () => {
  beforeEach(() => {
    vi.useFakeTimers()
    vi.mocked(library.search).mockResolvedValue({
      series: [card()],
      unfiled: [],
      query: '花嫁',
    })
  })

  // **The timer advance is what makes this test its name.** Without it,
  // deleting the `q === ''` guard entirely — so a debounced search for the
  // empty string fires on mount — leaves this passing, because `settle()`
  // never runs the debounce out.
  it('asks for nothing until something is typed', async () => {
    await settle()
    await act(async () => {
      vi.advanceTimersByTime(300)
      await Promise.resolve()
    })
    expect(vi.mocked(library.search)).not.toHaveBeenCalled()
  })

  it('searches what was typed, once the typing stops', async () => {
    const host = await settle()
    await type(host, '花嫁')
    expect(vi.mocked(library.search)).toHaveBeenCalledTimes(1)
    expect(vi.mocked(library.search)).toHaveBeenCalledWith('花嫁')
    expect(host.querySelector('.grid .title')?.textContent).toBe('五等分の花嫁')
  })

  it('runs the query in the URL on arrival, so a search can be a link', async () => {
    const host = await settle('/search?q=%E8%8A%B1%E5%AB%81')
    await act(async () => {
      vi.advanceTimersByTime(300)
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(vi.mocked(library.search)).toHaveBeenCalledWith('花嫁')
    expect(
      (host.querySelector('input[type="search"]') as HTMLInputElement).value,
    ).toBe('花嫁')
  })

  it('names what was searched when nothing matched', async () => {
    vi.mocked(library.search).mockResolvedValue({
      series: [],
      unfiled: [],
      query: '花婿',
    })
    const host = await settle()
    await type(host, '花婿')
    expect(host.querySelector('.empty')?.textContent).toContain('花婿')
  })

  it('keeps the answer to the last query typed, not the last one to arrive', async () => {
    // The slow one is typed first and resolves last — the shape that puts a
    // stale result on screen when nothing counts the requests.
    let releaseSlow: (v: library.SearchHits) => void = () => {}
    vi.mocked(library.search)
      .mockImplementationOnce(
        () => new Promise((resolve) => { releaseSlow = resolve }),
      )
      .mockResolvedValueOnce({ series: [card({ title: 'second' })], unfiled: [], query: 'b' })

    const host = await settle()
    await type(host, 'aaa')
    await type(host, 'bbb')
    await act(async () => {
      releaseSlow({ series: [card({ title: 'first' })], unfiled: [], query: 'a' })
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(host.querySelector('.grid .title')?.textContent).toBe('second')
  })

  it('says so when the search fails', async () => {
    vi.mocked(library.search).mockRejectedValue(
      new Error('/api/library/search: 500'),
    )
    const host = await settle()
    await type(host, '花嫁')
    expect(host.querySelector('[role="alert"]')?.textContent).toContain('500')
  })
})
```

- [ ] **Step 3: Run them to verify they fail**

Run: `pnpm -C web exec vitest run --project jsdom src/Search.test.tsx`
Expected: FAIL — `Failed to resolve import "./Search"`.

- [ ] **Step 4: Write the screen**

Create `web/src/Search.tsx`:

```tsx
/**
 * Search — spec C1 §11 step 9, over the endpoint that shipped in #81.
 *
 * **`?q=` is in the URL**, so a search is a link: the same reason `?root=` is
 * a query parameter on the library screen rather than component state.
 *
 * **Debounced through `debounce.ts`**, which exists and is tested, at the same
 * 250 ms the reader writes progress at. Typing 五等分の花嫁 is six keystrokes
 * and one request.
 *
 * **Clearing the box must `cancel()` and bump the sequence.** An earlier draft
 * of this plan said this screen needed neither `flush` nor `cancel` — wrong,
 * and `debounce.ts`'s own doc says why: `cancel` is "for the two acts that
 * replace the document outright rather than changing it", and emptying the
 * search box is one of those. Without it, clearing inside the 250 ms window
 * still sends the erased query, and clearing while a fetch is in flight lets
 * its results render over an empty box — because `latest.current` is bumped
 * only *inside* the debounced callback, so the guard never learns the reader
 * abandoned the request. Found by review, reproduced both ways.
 *
 * **A sequence number, because responses do not have to arrive in order.**
 * Three requests in flight and the first to return may be the first *sent*
 * rather than the last; without this the screen shows the answer to a query
 * the reader has already moved past. `cancelled` flags — the pattern
 * `Library.tsx` uses — solve unmount, not overtaking.
 *
 * **What the tokeniser makes possible here**, and what it does not: migration
 * 8's `trigram` index answers any substring of three characters or more in
 * either script, and `browse::search` answers shorter ones with `LIKE`. So
 * 花嫁 finds 五等分の花嫁 — which the shipped `unicode61` index could not do
 * at any query length, because it held the title as a single token.
 */
import { useEffect, useMemo, useRef, useState } from 'react'
import { useSearchParams } from 'react-router'
import { debounce } from './debounce'
import { EmptyState } from './EmptyState'
import { search as fetchSearch, type SearchHits } from './library'
import { SeriesGrid } from './SeriesGrid'

export function Search() {
  const [params, setParams] = useSearchParams()
  const q = params.get('q') ?? ''

  const [hits, setHits] = useState<SearchHits | undefined>()
  const [failure, setFailure] = useState<string | undefined>()
  const latest = useRef(0)

  // One debounced function for the life of the component: a fresh one per
  // render would have nothing pending to collapse into, which is the whole
  // point of debouncing.
  const run = useMemo(
    () =>
      debounce((query: string) => {
        const seq = ++latest.current
        fetchSearch(query)
          .then((next) => {
            if (seq !== latest.current) return
            setHits(next)
            setFailure(undefined)
          })
          .catch((err: unknown) => {
            if (seq !== latest.current) return
            setFailure(err instanceof Error ? err.message : String(err))
          })
      }, 250),
    [],
  )

  useEffect(() => {
    if (q === '') {
      // Both halves: drop a call still waiting out the debounce, and retire
      // the sequence so one already in flight cannot land on an empty box.
      run.cancel()
      latest.current++
      setHits(undefined)
      setFailure(undefined)
      return
    }
    run(q)
  }, [q, run])

  return (
    <>
      <label className="search-box">
        <span>Search</span>
        <input
          type="search"
          value={q}
          onChange={(e) => {
            // `replace`, not a push: typing six characters must not put six
            // entries in the reader's history for Back to walk out of.
            setParams(
              e.target.value === '' ? {} : { q: e.target.value },
              { replace: true },
            )
          }}
        />
      </label>
      {failure !== undefined && <p role="alert">{failure}</p>}
      {hits &&
        (hits.series.length === 0 && hits.unfiled.length === 0 ? (
          <EmptyState kind="no-matches" query={hits.query} />
        ) : (
          <SeriesGrid series={hits.series} unfiled={hits.unfiled} />
        ))}
    </>
  )
}
```

Append to `web/src/index.css`:

```css
.search-box {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  margin: 0 0 1rem;
}

.search-box input {
  flex: 1;
  max-width: 24rem;
  padding: 0.35rem 0.5rem;
  border: 1px solid ButtonBorder;
  border-radius: 4px;
  background: Canvas;
  color: CanvasText;
  font: inherit;
}
```

- [ ] **Step 5: Route it**

In `web/src/Routes.tsx`:

```tsx
      <Route
        path="/search"
        element={
          <Shell>
            <Search />
          </Shell>
        }
      />
```

Delete `NotBuiltYet` and the comment above it — every route is built.

- [ ] **Step 6: Run the tests**

Run: `pnpm -C web exec vitest run --project jsdom src/Search.test.tsx src/Library.test.tsx`
Expected: PASS both.

Run: `pnpm -C web run test && pnpm -C web run test:jsdom`
Expected: PASS.

- [ ] **Step 7: Prove the ordering test can fail**

Delete the two `if (seq !== latest.current) return` lines and re-run
`src/Search.test.tsx`. Expected: `keeps the answer to the last query typed`
FAILS with `expected 'first' to be 'second'`. Restore them and re-run to green.
Record both in the report. **A test for a race that cannot be made to fail is a
test of nothing**, and this one is cheap to check.

- [ ] **Step 8: Commit**

```bash
git add web/src/SeriesGrid.tsx web/src/Search.tsx web/src/Search.test.tsx web/src/Library.tsx web/src/Library.test.tsx web/src/Routes.tsx web/src/Routes.test.tsx web/src/index.css
git commit -m "search: a screen for the endpoint, and a Japanese title found by its middle"
```

---

### Task 11: The measurements, the documents, and the gate

**Files:**
- Modify: `docs/notes.md` — the tokeniser measurements
- Modify: `docs/status.md` — the five-part edits, the schema row, the counts
- Modify: `docs/superpowers/specs/2026-08-14-library-store-design.md` — §10's roadmap table
- Modify: `README.md`, `web/README.md`
- Modify: `scripts/coverage-floor.mjs` — only if a floor genuinely moved
- Modify: `.superpowers/sdd/progress-library-browse-series.md` — the ledger

**This task is not bookkeeping.** `docs/status.md`'s preamble promises every
claim there traces to a measurement in `docs/notes.md`, and the counts in it
drift silently — the closed count changes on every close while the total word
only changes on an open, so a run of closures leaves it behind. No test covers
any of it.

- [ ] **Step 1: Measure the tokeniser against a real library**

Build a scratch library from the corpus **by hardlink**, so the scan is seconds
rather than 28G and nothing can write into `corpus/`:

```bash
W="$PWD/target/plan-b-measure"
rm -rf "${W:?}" && mkdir -p "${W:?}/shelf"
for v in gotoubun-v01 gotoubun-v03 kaguya-v01; do
  cp -al "corpus/$v" "${W:?}/shelf/$v"
done
env -u TAIYAKU_CORPUS cargo run -q -p taiyaku-cli -- library scan --add "${W:?}/shelf" --db "${W:?}/lib.db"
```

Then measure, and record every figure in the scratch report:

```bash
sqlite3 "${W:?}/lib.db" "SELECT name, SUM(pgsize) FROM dbstat WHERE name LIKE '%fts%' GROUP BY name ORDER BY 2 DESC;"
sqlite3 "${W:?}/lib.db" "SELECT SUM(length(block)) FROM series_fts_data;" "SELECT SUM(length(block)) FROM work_fts_data;"
sqlite3 "${W:?}/lib.db" ".timer on" "SELECT count(*) FROM series_fts WHERE series_fts MATCH '\"の花嫁\"';"
```

**Take the `SUM(length(block))` figures, not only `dbstat`.** Task 1 measured
`dbstat` reporting an identical 32768 bytes before and after over one imported
work — SQLite's per-table minimum allocation, not index content. The block
lengths showed the real 3.1× there. Publish whichever of the two actually
varies at this corpus's size, and say which it is.

Confirm afterwards that the corpus was not written:

```bash
find corpus -newermt '-10 minutes' | head
```

Expected: no output. If there is any, stop — a scan wrote into the corpus and
that is a defect, not a measurement.

- [ ] **Step 2: Write `docs/notes.md`**

Add a dated section recording, with the SQLite version and the machine:

- the `fts5vocab` finding (the default tokeniser's whole-CJK-run tokens, listed);
- the six-row match table from Task 1, as measured;
- the index size before and after migration 8, and the ratio;
- the `の花嫁` query time on the scratch library;
- **and what was not measured**: the cold-cover timing §13 still leaves open,
  and whether anyone has ever held a root grant on a real deployment.

- [ ] **Step 3: Run the open-items sweep on the way in**

Per `docs/status.md`'s own ritual, before editing:

```sh
grep -c '^| O[0-9]' docs/status.md                 # rows
grep -c '^| O[0-9]* | \[x\]' docs/status.md        # closed
grep -nE '[0-9]{3} tests|Nine of the' docs/status.md
grep -n -A1 '^What remains:' docs/status.md        # the count word wraps
```

Record the four numbers before touching anything. If the head and the rows
already disagree, say so and fix that separately from this plan's own edits.

- [ ] **Step 4: Edit `docs/status.md`**

Nothing here **closes** an O-number — O60 and O61 are C4's and C2's — so this
is not the five-part close. What moves:

1. the `taiyaku-library` row's schema description: version 7 → 8, and the FTS
   tables' tokeniser;
2. the `web/` row of the crate table, and its file and test counts;
3. the test counts near the top, **which appear twice** — "Five crates, N tests"
   and "Nine of the N are standing checks";
4. the route list — no route is added, but `/api/library/search` stops being an
   endpoint with no caller;
5. the dependency list, if `web/package.json` changed (it should not: this plan
   adds no runtime dependency).

Then grep the bare O-numbers across `docs/` and `web/src/` and read every hit —
prose outside the table describes open items in wording no sweep can predict.

- [ ] **Step 5: Close C1's row in the store spec**

`docs/superpowers/specs/2026-08-14-library-store-design.md` §10's roadmap table
annotates shipped specs inline (`*(shipped 2026-08-18, #76)*`) and C1's row has
been unannotated while half-shipped since #81. Annotate it with all three PRs.
This was named as deliberately-left bookkeeping in Plan A's ledger; it is this
plan's to close.

- [ ] **Step 6: The READMEs**

`README.md`'s "To read in the browser" now describes a library with a series
page and a search box, not a work. `web/README.md` gains `/series/:id` and
`/search` in its route list. Both were listed in §12 as measurements this
invalidates and both were only half-done by Plan A.

- [ ] **Step 7: Run the gate**

```bash
env -u TAIYAKU_CORPUS scripts/check-all.sh
```

Read the exit status directly. Do not pipe it through `tail` and do not append
`; echo` — both mask a failure. Quote the final `all green` line in the report.

Then the corpus job on its own:

```bash
TAIYAKU_CORPUS="$PWD/corpus" cargo test --workspace --all-features corpus:: -- --nocapture --test-threads=1
```

Expected: ~85 s, twelve standing checks.

- [ ] **Step 8: Read the coverage margin, and do not lower a floor to fit**

The Rust gate is `--fail-under-lines 93`; C1's UI half left it at **93.32% with
46 covered lines of slack**, and the forge measures a few lines *worse* than
this machine — #81 failed at 92.99% where local said 93.04%. This plan adds
about 120 lines of Rust and a large `jsdom` surface.

If a floor in `scripts/coverage-floor.mjs` fires, **the fix is a test, not a
lower number.** Lowering a floor to restore a margin is how a gate stops
gating; that file says so itself. Record the aggregate and the worst five files
either way.

- [ ] **Step 9: Mutation sweep**

```bash
scripts/mutants.sh --since main
```

`--since main` reads `git diff main...HEAD`, so **commit every fix before
re-sweeping** or it re-reports what was already fixed. Survivors are findings,
not noise: Plan A's sweep found five of the whole-branch review's Importants.
Ignore any mutant that changes a SQL parameter count — `rusqlite`'s arity check
catches it mechanically and it proves nothing about semantics.

- [ ] **Step 10: Drive it in a browser, before the PR and not after**

This is the check no gate here can make: every one of them asks whether the
code is right, none asks whether it runs. Plan A's browser pass found two
defects that eleven task reviews and a whole-branch review had all missed, and
one of them — `serve --db` refusing to boot on an empty library — made two
tested, correct empty states unreachable in the product.

Use the same recipe as Step 1's scratch library, then:

```bash
env -u TAIYAKU_CORPUS cargo run -q -p taiyaku-cli -- serve --db "${W:?}/lib.db"
```

Walk, in both colour schemes, and screenshot each:

1. `/` → click a series tile → the series page loads with covers and volumes;
2. the edition selector: choose an edition, watch a volume dim rather than vanish;
3. the `i` button: the panel opens, names a verdict and a paired count;
4. `layout: table` in the sidebar settings: the dashed cell appears;
5. `/search`, typing `花嫁` **one character at a time** — the two-character
   query is the `LIKE` path and the four-character one is the index, and the
   point is that the reader cannot tell.

Check the console for failed requests and check every text/background pair for
WCAG AA. Report what was found rather than fixing it silently, and fix the
defects in a commit of their own.

- [ ] **Step 11: Write the ledger and open the PR**

Keep `.superpowers/sdd/progress-library-browse-series.md` as Plan A's ledger was
kept: what each task cost, what each review found, what was carried rather than
hidden. Then open the PR **through the gitea MCP, not `gh`** — `gh` is
authenticated against the wrong host here. The remote is squash-only, so the PR
body *is* the commit message: never hard-wrap it.

---

## Self-review

**Spec coverage.** §11 step 7 is Tasks 4–7 (grid, table, selector, panel); step
8 is Tasks 8–9 (the module, its registry, generated settings); step 9 is Tasks
1, 2 and 10 (the tokeniser, the query, the screen). §13's FTS5 unknown is
measured in Task 1 and recorded in Task 11. §3's `⇄`-on-the-cover, carried
forward from Plan A, is Task 4 step 6. §12's invalidated measurements are Task
11. Task 3 is not in the spec: it is the shell extraction three screens need,
and it is named as such.

**Not covered, deliberately, and each says why above:** `?root=` on `/search`
and on `/api/library/home`; `works_of`'s `only` by construction; O60; O61;
§13's cold-cover timing, which needs the deployment host rather than this
machine.

**Three errors this plan made and then corrected against the code**, recorded
because the next plan will be tempted to make them again:

- `fieldsOf` takes `(defaults, options)`, not a module. Written as
  `fieldsOf(browseModule)` first; `Settings.tsx:215` is the call site that
  disproves it.
- `withFieldValue` takes `(prefs, key, raw)` and returns the whole slice.
  `BrowseSettings` was written calling it with a `Field` and then coercing
  before `onChange`; the real contract is that the panel reports **raw** and
  `module.ts`'s `fieldChanged` coerces — which `Settings.test.tsx` pins by name.
- `humanize` lowercases and splits camelCase only, so `'on-select'` renders as
  `'on-select'`, not `'On select'`.

Correcting the second is what turned Task 9 from a hand-rolled preference
store into four calls to `module.ts`'s existing generic helpers, which is what
§4 was claiming all along.

**Still unverified by running, and marked rather than hidden:** every React
component and test in Tasks 3–10 is written against the idioms in
`Library.test.tsx` and `PageView.browser.test.tsx`, not against a run. Plan A's
pre-flight found two tests that could not pass as written — one asserting text
against a placeholder that renders none, one flushing a single microtask where
the effect needed two. Expect the same class of finding here and fix it in the
pre-flight commit rather than inside a task.
