# Libraries, F3 (Browsing): Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Every account browses by library. The sidebar lists the libraries
the account may read, each linking to that library's page; the grid, the
continue strip and search narrow to one library; and OPDS gains a Libraries
level above the flat works feed, which stays, with a second view beside it
that lists each library's works by `work.library_id` (the user's ruling).

**Architecture:** A narrowing to one library is the grant's own machinery
pointed at fewer folders. `browse` gains one function, `library_scope`, which
answers the folders a library holds that the caller's grant allows, or `None`
when there are none. Every listing that takes `library: Option<i64>` swaps its
`roots` for those folders before it builds SQL. The single-root `?root=`
narrowing (`only_clause`, and `only` through `pair_clause`) is deleted, so no
parameter group interleaves with the roots any more. `/api/library/status`
lists libraries; `/api/library/{series,home,search}` take `?library=`; OPDS
adds `/opds/v1.2/libraries` and `/opds/v1.2/libraries/{library}/works`, and
the `/by-work` pair beside them (`browse::filed_in`). The
web app's sidebar draws LIBRARIES for everyone, `Library.tsx` reads
`?library=`, the empty states speak of libraries, and the Libraries screen
asks the shell to re-read the status after each write.

**Tech Stack:** Rust 2024, rusqlite against the system SQLite, axum 0.8;
React 19, react-router, vitest, biome, pnpm.

## Read this first

**This plan was written on 2026-10-17 against `main` at `15e03a9d`**, after
F1, F2a, F2b, O57/O60/O138, O216 and O218 merged. The spec,
`docs/superpowers/specs/2026-10-05-libraries-design.md`, was written before
all of them; where the code has moved since, this plan says so in "Spec
corrections needed" and "Decided while planning". If you find something
else `main` contradicts, stop and ask.

**What this plan says the store does was probed, not only read.** A
throwaway test built two libraries (Manga: two folders holding one series
with editions `en` and `raw`; Ecchi: one folder holding a series of the same
title and a second series), derived, and called today's `series`, `home` and
`search` with each library's folder ids as the grant. Manga answered its one
series with 2 editions; Ecchi answered its two series with 1 edition each and
a series id different from Manga's; the same title made two series and the
byte-identical volume two works. Log and source:
`.superpowers/sdd/libraries-browse-evidence/plan-probe-15e03a9d.log` and
`plan-probe-test.rs`. The narrowing itself did not exist and was not run.

## Global Constraints

- **Branch:** `libraries-browse`, cut from `main` at `15e03a9d`.
- **The spec is `docs/superpowers/specs/2026-10-05-libraries-design.md`.**
  Read §3 (Browsing), §4 (OPDS), §5 (Access), §10, §11's F3 item and §12's
  Browsing, Access and Real material bullets before Task 1, and the O191 and
  O200 rows in `docs/status.md`. Where this plan and the spec disagree, stop
  and ask. The plan's own disagreements are under "Spec corrections needed"
  and are for the user, not for you.
- **No attribution lines anywhere.**
- **Every command runs from `/home/davey/projects/taiyaku`.** Never `cd`: this
  shell's `cd` is zoxide. Web commands are `pnpm -C web …`. The Bash tool runs
  zsh with `noclobber`: write scratch output to a new file name or use `>|`.
  Never name a shell variable `path`. Brace a variable before a colon.
  `grep` is a shell function that skips logs it takes for binary: write
  `command grep -a`.
- **Throttle every Rust test run**: `cargo test … -- --test-threads=8`, and
  `NEXTEST_TEST_THREADS=8 RUST_TEST_THREADS=8` for the gate.
- **Every new test is watched failing with its defect put back by hand.** Copy
  the production file aside first, make the edit the step names, run the
  test, read the failure, restore with `cp` and prove the restore with `cmp`.
  Put the failure's first line in the task's report. If the named defect does
  **not** make the test fail, stop and report. Never `git stash`.
- **A hand defect in SQL changes what the statement means, never how many
  parameters it binds.** A defect that changes the count is refused by
  `rusqlite` and proves nothing.
- **`library` and a root id are both `i64`.** `series` and `unfiled` keep the
  type of their last argument, so a test still passing a *root* id there
  compiles and asks about a library of that number. Every such call on `main`
  is listed in Task 1 Step 6; none may survive the task.
- **A Rust task is done only when whole crates pass**:
  `cargo test -p taiyaku-library -p taiyaku-serve -p taiyaku-cli --
  --test-threads=8`, `cargo clippy --workspace --all-targets --all-features
  -- -D warnings`, `cargo fmt --all -- --check`.
- **A web task is done only when whole projects pass**: `pnpm -C web run
  lint`, `pnpm -C web run typecheck`, `pnpm -C web exec vitest run --project
  node --project jsdom --reporter=default`, `pnpm -C web run test:browser`.
  A red browser project with import or connection errors and no failed
  assertion is the headless renderer crashing: check `coredumpctl` and rerun
  the whole project before suspecting the branch.
- **A class a component writes has a stylesheet rule in the same commit.**
  `web/src/classNames.test.ts`, in the node project, fails for a class no
  selector names.
- **"Pre-existing" means it fails the same way on `main`**, checked in a
  worktree at `15e03a9d`.
- **No figure in a code comment that a later change would have to correct.**
  Comments say what the code does and why it must, in the surrounding files'
  voice, and do not narrate this plan.
- **The snippets below are a starting point.** The interfaces are exact. The
  fixtures' plumbing is yours to fix against the code as it is. If a step's
  *claim about existing code* is wrong, stop and report.
- **`derive.rs`, `import.rs`, `scan.rs` and `fold.rs` do not change.** A defect
  found in them goes in the report for `docs/status.md`.
- **Evidence goes in `.superpowers/sdd/libraries-browse-evidence/`** (`$EVID`),
  gitignored, never under `target/`. Stamp each log with its commit.
- **Never touch `/home/davey/temp/manga-test/`.** Task 6 copies its
  `library.db` and proves the original byte-identical afterwards.

## Spec corrections needed

The user should rule on these before or during execution. The plan is written
to the reading given here.

1. **OPDS narrows by the library's folders, not by `work.library_id`.** §4
   says the library feed "reads `work.library_id`". The grid, search and home
   narrow by folders (§3: "narrowing to that library's set of roots"). The
   two disagree for a work that holds an edition whose files are in another
   library's folder, which migration 15 left in stores that had one work
   across two roots (O191's shape). By the column, library A's feed would
   list that work with both its editions, one of them files in library B's
   folder, and B's feed would not list it; by folders, A's page and feed list
   it with A's edition, and B's with B's. By folders, a library's page, its
   feed and an account granted only that library all see the same thing.
2. **A status entry carries `scanned`, not a folder count, and the top-level
   `scanned` goes.** §3 gives each entry "id, name and folder count".
   Nothing draws a folder count. A library's page with nothing on it has to
   tell "not read yet" from "nothing here" for that library, and the user is
   about to make the NAS's libraries and scan them one at a time (§8). The
   top-level `scanned` was true when anything at all had been scanned, so for
   a restricted account it reported on libraries it may not see.
3. **The library feed is a navigation feed, as `/opds/v1.2/works` is.** §4
   calls it "the same acquisition feed as today's `/opds/v1.2/works`". That
   feed is served as `kind=navigation`, and each entry links to the work's
   acquisition feed. A library outside the grant answers 404 `no library N`,
   as one that does not exist does.
4. **Search's scope on screen.** §3 says `?library=` reaches search, and says
   nothing of how a person asks for it. **Recommended:** a "Search in" select
   on the Search screen, "Every library" first and then each library by
   name, drawn when the account can see two or more libraries, kept in the
   URL as `?library=`. **The alternative:** the server takes `?library=` and
   the screen never sends it; search stays across every library the account
   may read. Task 5 is written to the recommendation; under the alternative
   it shrinks to its Step 1's `library.ts` change, without the `library`
   argument.

## Decided while planning

- **A narrowing swaps the folder list; it adds no clause.** §3's "reuses the
  existing `root_clause` and `member_clause` machinery with the library's
  root ids", taken literally. So `only_clause`, `only_clause_for` and the
  `only` argument of `pair_clause` and `aligned_clause` are deleted, and the
  module header's binding-order section is rewritten: every parameter group
  is the folder list now, and nothing interleaves with it. `alignments_of`
  and `works_of` keep their signatures. Grants are whole libraries, so the
  grant's share of a library's folders is all of them or none.
- **`library_scope` never answers an empty list.** Every `browse` function
  reads an empty `roots` as "no restriction". A library outside the grant,
  one that does not exist, or one with no folder answers `None`, and each
  listing answers that as nothing found. This is the defect this task is
  most likely to ship, and Task 1 has a test for each of the three.
- **`?root=` is refused, 400, naming `library`.** `GridQuery` is the only
  query that took it. F2a refused a `PATCH` still carrying `roots` the same
  way, and a tab left open across the upgrade then says so, rather than
  drawing every library under a library's name.
- **The web keeps `/?library=N`**, as §3 has it. `NavLink` compares the path
  and not the query, so `Sidebar` marks the current library, and the Library
  entry, by hand; `shell.css` styles `[aria-current="page"]`, so nothing in
  the stylesheet changes for it.
- **LIBRARIES is drawn for every account that can see one library or more.**
  §3: "for everyone". The `browse` module's `showRoots` goes, with its
  checkbox in the sidebar's settings. A value already stored in a browser is
  left in its key and not read.
- **The empty states speak of libraries** (O200 lists `EmptyState`): "no
  roots" becomes "no libraries", "not scanned" is asked of the library on
  screen, and a library page for a library the status does not list says so.
  The words are in Task 3; the user may change them at the hand check.
- **The sidebar is re-read after the Libraries screen re-reads its list.**
  O200: after a library was removed the sidebar listed it until a reload.
  `Libraries.tsx`'s `reload` is every write's re-read, so it asks the shell
  to read the status again each time it lands.
- **Names.** The spec's `LibraryCard` for a status entry is taken: F2a's
  `web/src/LibraryCard.tsx` is the admin card. The entry is `LibraryEntry`
  in `browse` and in `library.ts` alike.
- **Libraries are listed by name, case folded, then by id** (§10: "Libraries
  list by name"; migration 16 makes names unique regardless of ASCII case).
- **A library's page is titled with its name**: the document title and the
  visually hidden `h1`. Nothing visible changes but the sidebar's mark.
- **Home's `added` and `read` lists are narrowed by the API and drawn
  nowhere**, as on `main`: `Library.tsx` draws only Continue.
- **No CLI change.** §11's F3 names none.
- **A closed decision is reversed by the spec, on purpose.**
  `browse::tests::search_spans_the_whole_grant_rather_than_one_root` pins
  "`?root=` deliberately does not reach search". §3 makes search narrow, and
  the user approved §3. The test keeps its no-narrowing half under a new
  name, and gains a narrowed half (Task 1 Step 6).

## Ruled, 2026-10-17

The user, by AskUserQuestion, before Task 1 was dispatched:

1. **By folders by default, and a second OPDS view by `work.library_id`
   beside it** ("can we do by folder by default but also export an opds
   thing that shows by work.library_id separately?"). Task 2 builds both. The
   second view, as planned here: the catalog gains a third entry,
   **Libraries (by work)**, `/opds/v1.2/libraries/by-work`, listing the same
   libraries as `/opds/v1.2/libraries`, each linking to
   `/opds/v1.2/libraries/{library}/works/by-work`: the works whose
   `work.library_id` is that library, each with the editions the caller's
   grant allows, **not** narrowed to the library's folders. A library outside
   the grant, or unknown, answers 404 there too. An imported work (no
   library) is in no such feed. For O191's shape this is the view that lists
   the work under the library it is filed in, with both editions.
2. **Per-library `scanned`**, as recommended.
3. **The "Search in" select**, as recommended. Task 5 is built as written.
4. **Subagent-driven.**
5. Correction 3 (the feed's kind, 404) was not asked separately; it is how
   both views behave.

## Execution notes

- **Two lanes.** Tasks 1–2 are the Rust lane; Tasks 3–5 the web lane, which
  mocks the API and can start at once against the Interfaces blocks below.
  Task 2 starts after Task 1 is reviewed (it calls `library_scope`). Tasks 3,
  4, 5 run in that order. Tasks 6 and 7 are the controller's, after both
  lanes land.
- **A parallel lane works in a harness worktree cut from `main`**: reset it
  to this branch's head first, copy `crates/taiyaku-wasm/pkg`, and give it
  its own `CARGO_TARGET_DIR`. A subagent inherits the controller's working
  directory: never `cd` into a worktree. A subagent cut off by a usage limit
  keeps its worktree: resume it by `SendMessage`, do not redispatch.

## File structure

**`crates/taiyaku-library/src/`**

- `browse.rs`: `LibraryScope`, `library_scope`, `narrowed`; `library:
  Option<i64>` on `series`, `unfiled`, `search`, `home`; `LibraryEntry` and
  `Status { libraries }` in place of `RootCard` and `Status { roots, scanned
  }`; `only_clause`, `only_clause_for`, `named`, `last_component` deleted.
- `browse/tests.rs`: `two_library_store`, the narrowing and status tests.

**`crates/taiyaku-serve/src/`**

- `library_api.rs`: `HomeQuery`, `GridQuery { library, root }`,
  `SearchQuery { q, library }`; the status JSON; `on_the_library` and
  `failed` become `pub(crate)`.
- `library_api/tests.rs`: the route tests.
- `opds.rs`: the catalog's two library entries, `libraries`,
  `library_works`, `libraries_by_work`, `library_works_by_work`, `works_feed`
  (the body `works` writes, shared).
- `opds/tests.rs`: the feed tests.
- `routes.rs`: two `grant_postures` entries.
- `auth/tests.rs`: the posture proofs for both routes, the counts.

**`web/src/`**

- `library.ts`: `LibraryEntry`, `LibraryStatus { libraries }`; `home`,
  `grid`, `search` take an optional library.
- `shell.ts`: `StatusRefreshContext`, `useStatusRefresh`.
- `Shell.tsx`: re-reads the status on request; passes `libraries` to the
  sidebar; no `showRoots` fold.
- `Sidebar.tsx`: LIBRARIES for everyone, the current entry marked by hand.
- `browseModule.ts`: no `showRoots`.
- `Library.tsx`, `EmptyState.tsx`: `?library=`, the empty states.
- `Libraries.tsx`: asks for a status re-read after each list read.
- `Search.tsx`: the "Search in" select (Task 5).
- `styles/shell.css`, `styles/library.css`: `.sidebar-libraries`,
  `.search-scope`.
- Tests beside each, and every test that stubs `/api/library/status`'s shape
  (Task 3 Step 1 lists them).

---

## Task 1: `browse` narrows by library, and the routes take `?library=`

**Files:**
- Modify: `crates/taiyaku-library/src/browse.rs`
- Modify: `crates/taiyaku-library/src/browse/tests.rs`
- Modify: `crates/taiyaku-serve/src/library_api.rs`
- Modify: `crates/taiyaku-serve/src/library_api/tests.rs`
- Modify: `crates/taiyaku-serve/src/auth/tests.rs` (comments only, Step 9)

**Interfaces (produced; Task 2 and the web lane rely on them):**

```rust
// taiyaku_library::browse

/// One library a listing is narrowed to: its name, and the folders of it the
/// caller's grant allows. `folders` is never empty.
#[derive(Debug, Clone, PartialEq)]
pub struct LibraryScope {
    pub name: String,
    pub folders: Vec<i64>,
}

/// `None` for a library outside the grant, one with no folder, or one that
/// does not exist.
pub fn library_scope(conn: &Connection, roots: &[i64], library: i64)
    -> Result<Option<LibraryScope>, String>;

pub fn series(conn: &Connection, roots: &[i64], user_id: i64, library: Option<i64>)
    -> Result<Vec<SeriesCard>, String>;
pub fn unfiled(conn: &Connection, roots: &[i64], user_id: i64, library: Option<i64>)
    -> Result<Vec<WorkCard>, String>;
pub fn search(conn: &Connection, roots: &[i64], user_id: i64, q: &str, library: Option<i64>)
    -> Result<SearchResults, String>;
pub fn home(conn: &Connection, roots: &[i64], user_id: i64, library: Option<i64>)
    -> Result<Home, String>;

#[derive(Debug, Clone, PartialEq)]
pub struct LibraryEntry {
    pub id: i64,
    pub name: String,
    pub scanned: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Status {
    pub libraries: Vec<LibraryEntry>,
}

pub fn status(conn: &Connection, roots: &[i64]) -> Result<Status, String>;
```

The HTTP contract (the web lane mocks exactly this):

- `GET /api/library/status` → `{"libraries":[{"id":1,"name":"Manga","scanned":true}]}`,
  libraries the caller may see, ordered by name with case folded, then id.
  No other key, at either level.
- `GET /api/library/series?library=N`, `GET /api/library/home?library=N`,
  `GET /api/library/search?q=…&library=N`: the same bodies as without it,
  narrowed. A library outside the grant or unknown answers 200 with empty
  lists.
- `GET /api/library/series?root=…` → 400, body
  ``the grid is narrowed by `library`, a library's id; `root` is no longer a parameter``.

- [ ] **Step 1: The fixture**

In `browse/tests.rs`, beside `two_root_library`, add a store of two
libraries. The names are chosen so that ordering by id (`Manga` first), by
bytes (`Manga` before `comics`) and by folded case (`comics` first) all
disagree.

```rust
/// Two libraries, scanned and derived.
///
/// - **Manga**: the English and raw folders of `two_root_library`, edition
///   keys `en` and `raw`, one series `Series` whose one work holds both
///   editions.
/// - **comics**: one folder holding a `Series` of its own (the same title, so
///   a second series row, and byte-identical pages, so a second work) and a
///   series `Other`. Its edition key is the default, `raw`.
///
/// Lower-case `comics` on purpose: listed by name with case folded it comes
/// first, by bytes or by id it comes second.
struct TwoLibraries {
    library: Library,
    manga: i64,
    comics: i64,
    en_root: i64,
    raw_root: i64,
    comics_root: i64,
    _dirs: [crate::tests::TempDir; 3],
}

fn two_library_store() -> TwoLibraries {
    let en = crate::tests::tempdir("browse-lib-en");
    write_pages(&en.path().join("Series").join("Series - v01"), 2);
    let raw = crate::tests::tempdir("browse-lib-raw");
    write_pages(&raw.path().join("Series").join("Series - v01"), 2);
    let comics = crate::tests::tempdir("browse-lib-comics");
    write_pages(&comics.path().join("Series").join("Series - v01"), 2);
    write_pages(&comics.path().join("Other").join("Other - v01"), 3);

    let library = Library::open_in_memory().expect("in-memory");
    let ids = {
        let conn = library.conn();
        let en_root = scan::add_root_in(&conn, en.path(), scan::InLibrary::Named("Manga"))
            .expect("adds en");
        let raw_root = scan::add_root_in(&conn, raw.path(), scan::InLibrary::Named("Manga"))
            .expect("adds raw");
        let comics_root =
            scan::add_root_in(&conn, comics.path(), scan::InLibrary::Named("comics"))
                .expect("adds comics");
        conn.execute("UPDATE root SET edition_key = 'en' WHERE id = ?1", [en_root.id])
            .expect("names the English folder's edition");
        conn.execute("UPDATE root SET edition_key = 'raw' WHERE id = ?1", [raw_root.id])
            .expect("names the raw folder's edition");
        for root in scan::roots(&conn).expect("re-reads the roots") {
            scan::scan_root(&conn, &root).expect("scans");
        }
        derive::derive(&conn).expect("derives");
        let library_of = |root: i64| -> i64 {
            conn.query_row("SELECT library_id FROM root WHERE id = ?1", [root], |r| r.get(0))
                .expect("the folder's library")
        };
        (library_of(en_root.id), library_of(comics_root.id), en_root.id, raw_root.id, comics_root.id)
    };
    let (manga, comics_id, en_root, raw_root, comics_root) = ids;
    TwoLibraries {
        library, manga, comics: comics_id, en_root, raw_root, comics_root,
        _dirs: [en, raw, comics],
    }
}

/// The ids of a library's series, by `series.library_id` — never by title,
/// since both libraries hold a `Series`.
fn series_of(library: &Library, library_id: i64) -> Vec<i64> {
    let conn = library.conn();
    let mut stmt = conn
        .prepare("SELECT id FROM series WHERE library_id = ?1 ORDER BY id")
        .expect("prepares");
    stmt.query_map([library_id], |r| r.get(0))
        .expect("reads")
        .collect::<rusqlite::Result<Vec<i64>>>()
        .expect("reads")
}
```

Run any one test that builds it and print what it holds before writing the
assertions below against it; if it does not hold the probe's shape (Manga one
series of 2 editions; comics two series of 1), stop and report.

- [ ] **Step 2: The narrowing tests, failing**

```rust
#[test]
fn a_library_narrows_the_grid_to_its_own_series() {
    let s = two_library_store();
    let conn = s.library.conn();

    let all = series(&conn, &[], 1, None).expect("lists");
    assert_eq!(all.len(), 3, "both libraries, unnarrowed");

    let manga = series(&conn, &[], 1, Some(s.manga)).expect("lists");
    let ids: Vec<i64> = manga.iter().map(|c| c.id).collect();
    drop(conn);
    assert_eq!(ids, series_of(&s.library, s.manga), "Manga's one series");
    assert_eq!(manga[0].editions, 2, "with both its folders' editions");

    let conn = s.library.conn();
    let comics = series(&conn, &[], 1, Some(s.comics)).expect("lists");
    let mut ids: Vec<i64> = comics.iter().map(|c| c.id).collect();
    ids.sort_unstable();
    drop(conn);
    assert_eq!(ids, series_of(&s.library, s.comics), "comics' two series, and not Manga's Series");
    assert!(comics.iter().all(|c| c.editions == 1));
}

/// **The narrowing never widens the grant.** A caller granted Manga asking
/// for comics is answered nothing, on every listing that narrows.
#[test]
fn a_library_outside_the_grant_answers_nothing() {
    let s = two_library_store();
    import_a_work(&s.library, "Imported", "/x/taiyaku.json");
    let conn = s.library.conn();
    let grant = [s.en_root, s.raw_root];
    let narrowed = Some(s.comics);
    assert!(series(&conn, &grant, 1, narrowed).expect("lists").is_empty());
    assert!(unfiled(&conn, &grant, 1, narrowed).expect("lists").is_empty());
    let hits = search(&conn, &grant, 1, "Series", narrowed).expect("searches");
    assert!(hits.series.is_empty() && hits.unfiled.is_empty());
    let home = home(&conn, &grant, 1, narrowed).expect("reads home");
    assert!(home.added.is_empty() && home.continuing.is_empty() && home.read.is_empty());
}

/// **An empty folder list is "no restriction" to every function here**, so a
/// library with no folder of its own must answer nothing rather than hand one
/// on. Unrestricted caller, so a leak would be the whole store.
#[test]
fn a_library_that_does_not_exist_answers_nothing_rather_than_everything() {
    let s = two_library_store();
    let conn = s.library.conn();
    assert!(series(&conn, &[], 1, Some(9_999)).expect("lists").is_empty());
    assert!(home(&conn, &[], 1, Some(9_999)).expect("reads").added.is_empty());
    assert!(library_scope(&conn, &[], 9_999).expect("reads").is_none());

    // A library with no folder, which nothing public makes (O205), written
    // by hand: the same answer.
    conn.execute("INSERT INTO library (name) VALUES ('Empty')", [])
        .expect("a library with no folder");
    let empty = conn.last_insert_rowid();
    assert!(series(&conn, &[], 1, Some(empty)).expect("lists").is_empty());
    assert!(library_scope(&conn, &[], empty).expect("reads").is_none());
}

/// **The grant's share, not the library's whole.** Grants are whole libraries
/// through the API, but `browse` takes any folder list; this pins the
/// intersection, which a narrowing to the library's folders alone would fail
/// (Manga's series would show both editions).
#[test]
fn the_narrowing_reads_the_folders_the_library_and_the_grant_share() {
    let s = two_library_store();
    let conn = s.library.conn();
    let cards = series(&conn, &[s.en_root, s.comics_root], 1, Some(s.manga)).expect("lists");
    assert_eq!(cards.len(), 1);
    assert_eq!(cards[0].editions, 1, "the English edition only");
    let scope = library_scope(&conn, &[s.en_root, s.comics_root], s.manga)
        .expect("reads")
        .expect("Manga, through its English folder");
    assert_eq!(scope.name, "Manga");
    assert_eq!(scope.folders, vec![s.en_root]);
}

/// Each of home's three lists narrows. Positions: continuing on Manga's work
/// and comics' `Other`; finished on comics' `Series`.
#[test]
fn every_home_list_narrows_to_the_library() {
    let s = two_library_store();
    let (manga_work, comics_series_work, other_work) = {
        let conn = s.library.conn();
        let one = |sql: &str, p: i64| -> i64 {
            conn.query_row(sql, [p], |r| r.get(0)).expect("one work")
        };
        (
            one("SELECT w.id FROM work w WHERE w.library_id = ?1", s.manga),
            one("SELECT w.id FROM work w JOIN series x ON x.id = w.series_id
                  WHERE w.library_id = ?1 AND x.sort_title = 'Series'", s.comics),
            one("SELECT w.id FROM work w JOIN series x ON x.id = w.series_id
                  WHERE w.library_id = ?1 AND x.sort_title = 'Other'", s.comics),
        )
    };
    {
        let conn = s.library.conn();
        for (work, edition, done) in [
            (manga_work, "en", None),
            (other_work, "raw", None),
            (comics_series_work, "raw", Some("2026-10-17T00:00:00Z")),
        ] {
            conn.execute(
                "INSERT INTO progress (user_id, work_id, edition, slot, updated, completed_at)
                 VALUES (1, ?1, ?2, 0, '2026-10-17T00:00:00Z', ?3)",
                rusqlite::params![work, edition, done],
            )
            .expect("a position");
        }
    }
    let conn = s.library.conn();
    let ids = |works: &[WorkCard]| works.iter().map(|w| w.id).collect::<Vec<_>>();

    let comics = home(&conn, &[], 1, Some(s.comics)).expect("reads home");
    assert_eq!(ids(&comics.continuing), vec![other_work]);
    assert_eq!(ids(&comics.read), vec![comics_series_work]);
    let mut added = ids(&comics.added);
    added.sort_unstable();
    let mut want = vec![comics_series_work, other_work];
    want.sort_unstable();
    assert_eq!(added, want);

    let manga = home(&conn, &[], 1, Some(s.manga)).expect("reads home");
    assert_eq!(ids(&manga.continuing), vec![manga_work]);
    assert_eq!(ids(&manga.added), vec![manga_work]);
}

#[test]
fn search_narrowed_to_a_library_finds_only_its_series() {
    let s = two_library_store();
    let conn = s.library.conn();
    let all = search(&conn, &[], 1, "Series", None).expect("searches");
    assert_eq!(all.series.len(), 2, "a Series in each library");
    let comics = search(&conn, &[], 1, "Series", Some(s.comics)).expect("searches");
    let ids: Vec<i64> = comics.series.iter().map(|c| c.id).collect();
    drop(conn);
    let theirs = series_of(&s.library, s.comics);
    assert_eq!(ids.len(), 1);
    assert!(theirs.contains(&ids[0]), "comics' Series, not Manga's");
}

/// An imported work is in no library: asking for one leaves the unfiled
/// bucket empty, by the grant's own rule (`IN (…)` is false for NULL).
#[test]
fn asking_for_a_library_leaves_the_unfiled_bucket_empty() {
    let s = two_library_store();
    import_a_work(&s.library, "Imported", "/x/taiyaku.json");
    let conn = s.library.conn();
    assert_eq!(unfiled(&conn, &[], 1, None).expect("lists").len(), 1);
    assert!(unfiled(&conn, &[], 1, Some(s.manga)).expect("lists").is_empty());
}
```

- [ ] **Step 3: Run them, and see them fail**

Run: `cargo test -p taiyaku-library browse::tests -- --test-threads=8`
Expected: does not compile (`search` and `home` take four arguments,
`library_scope` is not defined).

- [ ] **Step 4: `library_scope`, `narrowed`, and the four listings**

In `browse.rs`, beside `root_clause`:

```rust
/// One library a listing is narrowed to: its name, and the folders of it the
/// caller's grant allows.
#[derive(Debug, Clone, PartialEq)]
pub struct LibraryScope {
    pub name: String,
    /// Never empty: see [`library_scope`].
    pub folders: Vec<i64>,
}

/// The folders of `library` this caller may read, with the library's name.
///
/// **`None` when there are none**, whether the library is outside the grant,
/// holds no folder, or does not exist, and never a scope with no folders:
/// every function here reads an empty folder list as no restriction at all,
/// so a narrowing that matched nothing would list every library.
///
/// The grant is whole libraries, so this answers all of a granted library's
/// folders or `None`. It still intersects rather than asking, because `roots`
/// is what every query here filters on.
///
/// # Errors
///
/// Returns an error if the query fails.
pub fn library_scope(
    conn: &Connection,
    roots: &[i64],
    library: i64,
) -> Result<Option<LibraryScope>, String> {
    let mut stmt = prepare(
        conn,
        "SELECT r.id, l.name FROM root r JOIN library l ON l.id = r.library_id
          WHERE r.library_id = ? ORDER BY r.id",
    )
    .map_err(|e| format!("preparing the folders of library {library}: {e}"))?;
    let rows = stmt
        .query_map([library], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)))
        .map_err(|e| format!("reading the folders of library {library}: {e}"))?
        .collect::<rusqlite::Result<Vec<(i64, String)>>>()
        .map_err(|e| format!("reading the folders of library {library}: {e}"))?;
    let name = rows.first().map(|(_, name)| name.clone());
    let folders: Vec<i64> = rows
        .into_iter()
        .map(|(id, _)| id)
        .filter(|id| roots.is_empty() || roots.contains(id))
        .collect();
    Ok(match name {
        Some(name) if !folders.is_empty() => Some(LibraryScope { name, folders }),
        _ => None,
    })
}

/// The folders a listing reads: the grant, or the grant's share of
/// `library`'s folders. `None` means there is nothing to list.
fn narrowed(conn: &Connection, roots: &[i64], library: Option<i64>) -> Result<Option<Vec<i64>>, String> {
    match library {
        None => Ok(Some(roots.to_vec())),
        Some(id) => Ok(library_scope(conn, roots, id)?.map(|scope| scope.folders)),
    }
}
```

Then:

- `series(conn, roots, user_id, library)`: first line
  `let Some(roots) = narrowed(conn, roots, library)? else { return Ok(Vec::new()); };`
  then `let roots = roots.as_slice();`. Its SQL loses `{narrowed}`; its params
  are the roots three times, then `user_id`. Rewrite its binding comment to
  say that.
- `unfiled(conn, roots, user_id, library)`: the same first line; then
  `works_of(conn, &roots, user_id, "w.series_id IS NULL", &[], "w.title COLLATE NOCASE, w.id")`.
  Its doc's `only` paragraph says instead: a library narrowing leaves this
  empty, since an imported member's `root_id` is NULL and the narrowed list is
  never empty.
- `search(conn, roots, user_id, q, library)`: the empty-query return stays
  first; then the `narrowed` line answering an empty `SearchResults`; then
  `series(conn, &roots, user_id, None)` and `unfiled(conn, &roots, user_id, None)`.
- `home(conn, roots, user_id, library)`: the `narrowed` line answering three
  empty lists, then the body unchanged over the narrowed roots.
- `pair_clause(roots)`, `aligned_clause(roots)`: no `only`. `alignments_of`
  and `works_of` call them with `roots` alone; their binding comments lose the
  `only` sentences.
- Delete `only_clause` and `only_clause_for`.
- The module header: the alignment paragraph says "both inside the folders
  the listing reads"; "# Every placeholder…" keeps its rule and its 2026-08-20
  measurement, and its last paragraph says what is true now: each function
  pushes its parameters in the text order of the fragments it splices, every
  group a filter contributes is the folder list, and a narrowing to one
  library swaps that list (see `library_scope`) rather than adding a
  parameter of its own. The filter section gains one sentence: a listing
  narrowed to one library reads the folders that library and the grant share
  in place of the grant, so a library's page lists what an account granted
  only that library would see.

- [ ] **Step 5: Run the Step 2 tests**

Run: `cargo test -p taiyaku-library browse::tests -- --test-threads=8`
Expected: the Step 2 tests pass. The crate's other callers of `search`,
`home`, `series` and `unfiled` do not compile yet; Step 6 fixes the tests,
Step 8 the server.

- [ ] **Step 6: Every call that passed a root as a narrowing**

These pass a **root** id where a library id now goes, on `main` at
`15e03a9d`, all in `browse/tests.rs`:

| Line | Test |
| --- | --- |
| 928 | `a_root_filter_keeps_only_that_roots_editions` |
| 943 | `a_root_filter_outside_the_grant_answers_nothing` |
| 1128 | `a_root_filter_narrows_the_aligned_count_and_not_only_the_other_three` |
| 1169, 1180 | `a_root_filter_keeps_a_pair_whose_two_editions_are_both_under_it` |
| 1206, 1215 | `asking_for_a_root_leaves_the_unfiled_bucket_empty` |

- Delete the first two and the last: Step 2's tests replace them.
- **The two aligned-count tests become library tests.** In
  `two_root_library` both folders are one library, so a library narrowing
  cannot split the `en`↔`raw` pair. Give the raw folder a library of its own
  **without deriving again**, which is the shape migration 15 left (O191):
  one work, its `raw` edition's members under the other library's folder.

  ```rust
  /// The raw folder moved to a library of its own by hand, not derived
  /// again: one work keeps an edition under each library (O191's shape).
  fn raw_in_a_library_of_its_own(library: &Library, raw_root: i64) -> i64 {
      let conn = library.conn();
      conn.execute("INSERT INTO library (name) VALUES ('Raws')", [])
          .expect("a second library");
      let raws = conn.last_insert_rowid();
      conn.execute("UPDATE root SET library_id = ?1 WHERE id = ?2", [raws, raw_root])
          .expect("files the raw folder under it");
      raws
  }
  ```

  `a_library_narrows_the_aligned_count_and_not_only_the_other_three`: with
  the pair aligned and the raw folder in `Raws`, `series(&conn, &[], 1,
  Some(<the English folder's library>))` answers `works 1, editions 1,
  aligned 0`, and `None` answers `editions 2, aligned 1`.
  `a_library_keeps_a_pair_whose_two_editions_are_both_in_it`: no split; the
  pair aligned; narrowed to Manga with grant `[en_id, raw_id]` it answers
  `editions 2, aligned 1`. Rewrite their docs to say what each holds now; the
  binding paragraph ("ten placeholders in three interleaved…") goes.
- **`search_spans_the_whole_grant_rather_than_one_root`** becomes
  `search_with_no_library_spans_the_whole_grant`: the same assertions with
  `None`, and a doc saying what it holds. Its "deliberately does not reach
  search" paragraph goes; Step 2's search test is the narrowed half.
- Every other call of `search` and `home` in the file gains `, None`.

Then `command grep -a -n 'Some(en_id)\|Some(raw_id)\|Some(en_root)\|Some(raw_root)' crates/taiyaku-library/src/browse/tests.rs`
must print nothing.

- [ ] **Step 7: `status` lists libraries**

Tests first, replacing `status_names_a_root_by_its_last_path_component`,
`status_lists_only_the_roots_the_grant_names`,
`two_roots_sharing_a_last_component_fall_back_to_their_paths`,
`a_partial_collision_falls_back_only_the_colliding_pair` and
`scanned_turns_true_only_after_a_scan_of_this_library_runs`. The first four
test `named`, which goes; a library's name is the store's, and its collision
rule is held by `scan::tests::own_library_clash_refuses_exactly_what_the_trigger_cannot_name`
and `schema::tests`.

```rust
#[test]
fn status_lists_each_library_once_by_name_with_case_folded() {
    let s = two_library_store();
    let conn = s.library.conn();
    let names: Vec<String> = status(&conn, &[]).expect("reads").libraries
        .into_iter().map(|l| l.name).collect();
    assert_eq!(names, vec!["comics".to_string(), "Manga".to_string()],
        "two folders make one entry, and lower-case `comics` sorts first");
}

#[test]
fn status_lists_only_the_libraries_the_grant_names() {
    let s = two_library_store();
    let conn = s.library.conn();
    let restricted = status(&conn, &[s.en_root, s.raw_root]).expect("reads");
    let ids: Vec<i64> = restricted.libraries.iter().map(|l| l.id).collect();
    assert_eq!(ids, vec![s.manga]);
}

/// Per library: a scan of one library's folder says nothing of another's.
#[test]
fn scanned_is_true_only_for_a_library_one_of_whose_folders_was_walked() {
    let walked = crate::tests::tempdir("browse-walked");
    write_pages(&walked.path().join("Series").join("Series - v01"), 1);
    let unwalked = crate::tests::tempdir("browse-unwalked");
    write_pages(&unwalked.path().join("Other").join("Other - v01"), 1);
    let library = Library::open_in_memory().expect("in-memory");
    let conn = library.conn();
    let a = scan::add_root_in(&conn, walked.path(), scan::InLibrary::Named("Walked")).expect("adds");
    scan::add_root_in(&conn, unwalked.path(), scan::InLibrary::Named("Unwalked")).expect("adds");
    let before = status(&conn, &[]).expect("reads");
    assert!(before.libraries.iter().all(|l| !l.scanned), "nothing walked yet");
    scan::scan_root(&conn, &a).expect("scans one");
    let after = status(&conn, &[]).expect("reads");
    let flags: Vec<(String, bool)> = after.libraries.into_iter().map(|l| (l.name, l.scanned)).collect();
    assert_eq!(flags, vec![("Unwalked".to_string(), false), ("Walked".to_string(), true)]);
}
```

Then the code: delete `RootCard`, `named` and `last_component`; add
`LibraryEntry` (doc: one library as the sidebar lists it; `scanned` is
whether any of its folders has been walked, so a library page with nothing
in it can say "not read yet" rather than "nothing here") and

```rust
pub fn status(conn: &Connection, roots: &[i64]) -> Result<Status, String> {
    let granted = if roots.is_empty() {
        String::new()
    } else {
        format!(" AND r.id IN ({})", holes(roots.len()))
    };
    let sql = format!(
        "SELECT l.id, l.name,
                EXISTS (SELECT 1 FROM dir_state d JOIN root dr ON dr.id = d.root_id
                         WHERE dr.library_id = l.id)
           FROM library l
          WHERE EXISTS (SELECT 1 FROM root r WHERE r.library_id = l.id{granted})
          ORDER BY l.name COLLATE NOCASE, l.id"
    );
    let mut stmt = prepare(conn, &sql).map_err(|e| format!("preparing the library listing: {e}"))?;
    let libraries = stmt
        .query_map(params_from_iter(roots), |row| {
            Ok(LibraryEntry { id: row.get(0)?, name: row.get(1)?, scanned: row.get(2)? })
        })
        .map_err(|e| format!("listing libraries: {e}"))?
        .collect::<rusqlite::Result<Vec<LibraryEntry>>>()
        .map_err(|e| format!("reading libraries: {e}"))?;
    Ok(Status { libraries })
}
```

`Status`'s doc keeps its first-run argument (spec C1 §5.5) and says the
answer is per library now. The last sentence ("C4 still builds roots
administration") goes.

- [ ] **Step 8: The routes**

In `library_api.rs`:

```rust
/// The optional `?library=` the home lists carry.
#[derive(serde::Deserialize)]
pub(crate) struct HomeQuery {
    library: Option<i64>,
}

/// The grid's `?library=`, and the `?root=` it replaced, which is named here
/// only to be refused: a tab left open across the change would otherwise be
/// drawn every library under one library's name.
#[derive(serde::Deserialize)]
pub(crate) struct GridQuery {
    library: Option<i64>,
    root: Option<String>,
}

#[derive(serde::Deserialize)]
pub(crate) struct SearchQuery {
    #[serde(default)]
    q: String,
    library: Option<i64>,
}
```

`home` takes `Query(query): Query<HomeQuery>` and passes `query.library`;
`series` answers 400 with the sentence in the contract above when
`query.root.is_some()`, before `on_the_library`, and otherwise passes
`query.library` to `browse::series` and `browse::unfiled` (its comment about
`query.root` says the same of `query.library`); `search` passes
`query.library`. `status` answers

```rust
Json(serde_json::json!({
    "libraries": status.libraries.iter().map(|l| serde_json::json!({
        "id": l.id,
        "name": l.name,
        "scanned": l.scanned,
    })).collect::<Vec<_>>(),
}))
```

and its doc says it lists the libraries the caller may see. Make
`on_the_library` and `failed` `pub(crate)` (Task 2 uses both).

Tests in `library_api/tests.rs`. Rewrite the three `?root=` tests against
libraries, reading each library's id from the store
(`SELECT library_id FROM root WHERE id = ?1`), never assuming one:

- `a_library_that_does_not_exist_answers_an_empty_grid_rather_than_an_error`
  (`?library=999999`, 200, no series);
- `the_grid_narrows_to_the_library_the_query_names` (the material's
  library: one series; the other: none);
- `a_library_scoped_grid_carries_no_unfiled_work`.

New:

- `the_home_lists_narrow_to_the_library_the_query_names`: a position on the
  fixture's work; `?library=<its library>` lists it under `continuing`, the
  other library's does not.
- `search_narrows_to_the_library_the_query_names`, the same shape on
  `/api/library/search?q=…`.
- `a_grid_asked_for_a_root_is_refused_naming_library`: 400, the body
  contains `` `library` `` and `` `root` ``.
- `the_status_lists_libraries_and_nothing_else`: the body's top-level keys
  are exactly `["libraries"]`, and each entry's exactly
  `["id", "name", "scanned"]` (compare sorted key lists; a substring check
  cannot see an added field).

Then update the comment above the status proof in
`auth/tests.rs::every_route_declares_its_grant_posture` (it says the route
filters roots; it filters libraries, which the fixture's trigger names after
the same folders). The
proof's assertions stay as they are; run it.

- [ ] **Step 9: Hand defects, each watched failing**

| Defect (in `browse.rs` unless named) | Must fail |
| --- | --- |
| `library_scope` answers `Some(LibraryScope { name, folders })` when `folders` is empty | `a_library_outside_the_grant_answers_nothing`, `a_library_that_does_not_exist_…` |
| `library_scope`'s filter is `.filter(\|_\| true)` | `a_library_outside_the_grant_answers_nothing`, `the_narrowing_reads_the_folders_…` |
| `narrowed` answers `Ok(Some(roots.to_vec()))` for `Some(_)` | `a_library_narrows_the_grid_to_its_own_series` |
| `search` passes the caller's `roots`, not the narrowed ones, to `series` | `search_narrowed_to_a_library_finds_only_its_series` |
| `home`'s `read` list reads the caller's `roots` | `every_home_list_narrows_to_the_library` |
| `home`'s `added` list reads the caller's `roots` | `every_home_list_narrows_to_the_library` |
| `status`: `dr.library_id = l.id` → `dr.library_id IS NOT NULL` | `scanned_is_true_only_for_a_library_…` |
| `status`: `ORDER BY l.name COLLATE NOCASE, l.id` → `ORDER BY l.id` | `status_lists_each_library_once_by_name_…` |
| `status`: `r.id IN (` → `r.id NOT IN (` | `status_lists_only_the_libraries_the_grant_names` |
| `library_api::series` passes `None` to `browse::series` | `the_grid_narrows_to_the_library_the_query_names` |
| `library_api::series` passes `None` to `browse::unfiled` | `a_library_scoped_grid_carries_no_unfiled_work` |
| `library_api::home` passes `None` | `the_home_lists_narrow_to_…` |
| `library_api::search` passes `None` | `search_narrows_to_the_library_…` |
| the `?root=` refusal deleted | `a_grid_asked_for_a_root_is_refused_naming_library` |
| the status JSON gains `"folders": 0` | `the_status_lists_libraries_and_nothing_else` |

- [ ] **Step 10: Whole crates, and commit**

Run the Rust gate in Global Constraints. Then:

```bash
git add crates/taiyaku-library/src/browse.rs crates/taiyaku-library/src/browse/tests.rs \
  crates/taiyaku-serve/src/library_api.rs crates/taiyaku-serve/src/library_api/tests.rs \
  crates/taiyaku-serve/src/auth/tests.rs
git commit -m "F3: browse narrows to one library by its folders, and the browse routes take ?library="
```

---

## Task 2: OPDS has a Libraries level, by folder and by work

**Files:**
- Modify: `crates/taiyaku-library/src/browse.rs`, `crates/taiyaku-library/src/browse/tests.rs` (`filed_in`)
- Modify: `crates/taiyaku-serve/src/opds.rs`
- Modify: `crates/taiyaku-serve/src/opds/tests.rs`
- Modify: `crates/taiyaku-serve/src/routes.rs` (`grant_postures`)
- Modify: `crates/taiyaku-serve/src/auth/tests.rs`

**Interfaces:**
- Consumes: `browse::library_scope`, `browse::status`, `browse::visible_editions`,
  `library_api::on_the_library`, `library_api::failed` (Task 1).
- Produces:
  - `browse::filed_in(conn: &Connection, library: i64) -> Result<HashSet<String>, String>`:
    the slugs of the works whose `work.library_id` is `library`. One
    statement, through this module's `prepare`.
  - `GET /opds/v1.2/catalog` gains two entries after the Works entry:
    `taiyaku:libraries`, "Libraries", `rel="subsection"` to
    `/opds/v1.2/libraries`; and `taiyaku:libraries:by-work`, "Libraries (by
    work)", to `/opds/v1.2/libraries/by-work`.
  - `GET /opds/v1.2/libraries`: a navigation feed, `<id>taiyaku:libraries</id>`,
    one entry per library the caller may see, in `browse::status`'s order:
    `<id>taiyaku:library:{id}</id>`, the escaped name as its title,
    `<content type="text">The works in {name}</content>`, and
    `rel="subsection"` `kind=navigation` to `/opds/v1.2/libraries/{id}/works`.
  - `GET /opds/v1.2/libraries/{library}/works`: the body `/opds/v1.2/works`
    writes, with `<id>taiyaku:library:{id}:works</id>`, the library's escaped
    name as `<title>`, and its own `self` link, over the editions
    `browse::visible_editions` answers for `library_scope`'s folders. A library
    outside the grant, or unknown, answers 404 `no library {id}`.
  - `GET /opds/v1.2/libraries/by-work`: as `/opds/v1.2/libraries`, with
    `<id>taiyaku:libraries:by-work</id>`, title "Libraries (by work)", entry
    ids `taiyaku:library:{id}:by-work`, content `The works filed in {name}`,
    and each entry linking to `/opds/v1.2/libraries/{id}/works/by-work`.
  - `GET /opds/v1.2/libraries/{library}/works/by-work`: the same body, with
    `<id>taiyaku:library:{id}:works:by-work</id>`, the title `{name} (by
    work)`, its own `self` link, over `browse::visible_editions` for the
    caller's **grant** (not the library's folders), keeping only the slugs
    `filed_in` answers. A library `library_scope` answers `None` for answers
    404 `no library {id}`, as above.

- [ ] **Step 1: The feed tests, failing**

In `opds/tests.rs`, over the existing fixtures. In each, read library ids
from the store (`SELECT library_id FROM root WHERE id = ?1`): the fixtures
insert roots by hand and the `root_gets_a_library` trigger names a library
after each (`held`, `withheld`).

- `the_catalog_links_to_the_libraries`: the catalog holds
  `href="/opds/v1.2/libraries"` with `rel="subsection"`, and still holds
  `href="/opds/v1.2/works"`.
- `the_libraries_feed_lists_each_library_with_a_link_to_its_works`:
  `router_with_two_works_one_withheld`'s state behind `feed_router()` (no
  principal, so unrestricted): both libraries, each with its link, `held`
  before `withheld`.
- `a_restricted_reader_s_libraries_feed_names_only_its_libraries`: the same
  fixture's own router (the account granted `held`): `held` and its link, and
  neither `withheld` nor its id's link.
- `a_library_s_works_feed_lists_only_the_works_under_its_folders`: unrestricted,
  `/opds/v1.2/libraries/<withheld's id>/works` names `Hidden Work` and not
  `Visible Work`, and the other library's names the other. Its `<title>` is
  the library's name.
- `a_library_outside_the_grant_is_refused_as_one_that_does_not_exist`: the
  restricted router; `<withheld's id>` and `999999` both answer 404, and the
  two bodies are equal with the id taken out.
- `a_library_s_works_feed_counts_only_the_editions_under_its_folders`:
  `router_with_two_editions_split_across_roots`, unrestricted: the held
  library's feed carries `1 editions` for the work, `/opds/v1.2/works`
  carries `2 editions`. This is O191's shape: one work, an edition under
  each library's folder.

The by-work view. The fixtures import their works before their roots exist,
so `work.library_id` is NULL in all of them: set it by hand
(`UPDATE work SET library_id = (SELECT library_id FROM root WHERE id = 1)`)
and say so in the test.

- `the_catalog_links_to_the_libraries` also holds
  `href="/opds/v1.2/libraries/by-work"`.
- `the_by_work_libraries_feed_links_each_library_to_its_filed_works`: as the
  libraries feed test, the links ending `/works/by-work`.
- `the_by_work_feed_lists_a_work_under_the_library_it_is_filed_in_with_every_edition`:
  `router_with_two_editions_split_across_roots`, unrestricted, the work filed
  in the held folder's library. The held library's by-work feed lists it with
  `2 editions`; its by-folder feed with `1 editions`; the withheld library's
  by-work feed lists no work, while its by-folder feed lists it with
  `1 editions`. The four answers are what tells the two views apart.
- `a_restricted_reader_s_by_work_feed_counts_only_the_editions_the_grant_allows`:
  the same fixture behind its restricted router (granted the held folder's
  library): the held library's by-work feed lists the work with `1 editions`;
  the withheld library's answers 404.
- `an_imported_work_is_in_no_by_work_feed`: a work left with
  `library_id` NULL is listed by `/opds/v1.2/works` and by no library's
  by-work feed.

In `browse/tests.rs`, `filed_in_answers_the_slugs_of_the_works_filed_in_a_library`
over `two_library_store`: Manga's one slug, comics' two, and none of either
in the other's.

Run: `cargo test -p taiyaku-serve opds:: -- --test-threads=8`
Expected: the new tests fail with 404 from the router (no route).

- [ ] **Step 2: `works_feed`, and the two handlers**

Move the body of `works` after its grant read into

```rust
/// The feed `/opds/v1.2/works` writes, over `visible` (`None`: every edition
/// of every work), under the feed id, title and self link given. `id` and
/// `title` are written as given, so a caller escapes them.
fn works_feed(
    state: &ServeState,
    visible: Option<&HashMap<String, HashSet<String>>>,
    id: &str,
    title: &str,
    href: &str,
) -> Response
```

`works` calls it with `"taiyaku:works"`, `"Works"`, `"/opds/v1.2/works"`; its
output must not change by a byte (the existing works-feed tests hold it).

```rust
/// The grant of the request, `[]` (no restriction) where no principal was
/// put on it — this file's own tests build requests with none, and
/// `routes::on_the_grant` reads that the same way.
fn granted(extensions: &axum::http::Extensions) -> Vec<i64> {
    extensions
        .get::<crate::auth::Principal>()
        .map(|p| p.roots.clone())
        .unwrap_or_default()
}

/// Every library this caller may read, each linking to its works.
pub(crate) async fn libraries(
    State(state): State<Arc<ServeState>>,
    extensions: axum::http::Extensions,
) -> Response {
    let roots = granted(&extensions);
    crate::library_api::on_the_library(state, "the libraries", move |conn| {
        match taiyaku_library::browse::status(conn, &roots) {
            Ok(status) => feed(libraries_body(&status.libraries)),
            Err(why) => crate::library_api::failed("the libraries", &why),
        }
    })
    .await
}

/// One library's works, as `works` lists every work.
pub(crate) async fn library_works(
    State(state): State<Arc<ServeState>>,
    AxPath(library): AxPath<i64>,
    extensions: axum::http::Extensions,
) -> Response {
    let roots = granted(&extensions);
    let feed_state = Arc::clone(&state);
    crate::library_api::on_the_library(state, "the library's works", move |conn| {
        let scope = match taiyaku_library::browse::library_scope(conn, &roots, library) {
            Ok(Some(scope)) => scope,
            Ok(None) => return missing(format!("no library {library}")),
            Err(why) => return crate::library_api::failed("the library's works", &why),
        };
        match taiyaku_library::browse::visible_editions(conn, &scope.folders) {
            Ok(visible) => works_feed(
                &feed_state,
                Some(&visible),
                &format!("taiyaku:library:{library}:works"),
                &escape(&scope.name),
                &format!("/opds/v1.2/libraries/{library}/works"),
            ),
            Err(why) => crate::library_api::failed("the library's works", &why),
        }
    })
    .await
}
```

`works_feed` reads `state.works.snapshot()` and each work's sidecar, which
it does on the async worker in `works` today; inside `on_the_library` it runs
on the blocking pool. Write the libraries feed's body once, for both views,
with the entry shape in the Interfaces block; escape the name in `<title>`
and `<content>`:

```rust
/// Which of the two library views a feed belongs to.
#[derive(Clone, Copy)]
enum View {
    /// A library's works are those with an edition under its folders.
    Folders,
    /// A library's works are those `work.library_id` files in it.
    Filed,
}

fn libraries_body(libraries: &[taiyaku_library::browse::LibraryEntry], view: View) -> String
```

`libraries_by_work` is `libraries` with `View::Filed`. The by-work works
feed:

```rust
/// One library's works as the store files them: `work.library_id`, with
/// every edition the grant allows, wherever its files are.
pub(crate) async fn library_works_by_work(
    State(state): State<Arc<ServeState>>,
    AxPath(library): AxPath<i64>,
    extensions: axum::http::Extensions,
) -> Response {
    let roots = granted(&extensions);
    let feed_state = Arc::clone(&state);
    crate::library_api::on_the_library(state, "the library's filed works", move |conn| {
        let scope = match taiyaku_library::browse::library_scope(conn, &roots, library) {
            Ok(Some(scope)) => scope,
            Ok(None) => return missing(format!("no library {library}")),
            Err(why) => return crate::library_api::failed("the library's filed works", &why),
        };
        let filed = match taiyaku_library::browse::filed_in(conn, library) {
            Ok(filed) => filed,
            Err(why) => return crate::library_api::failed("the library's filed works", &why),
        };
        match taiyaku_library::browse::visible_editions(conn, &roots) {
            Ok(mut visible) => {
                visible.retain(|slug, _| filed.contains(slug));
                works_feed(
                    &feed_state,
                    Some(&visible),
                    &format!("taiyaku:library:{library}:works:by-work"),
                    &format!("{} (by work)", escape(&scope.name)),
                    &format!("/opds/v1.2/libraries/{library}/works/by-work"),
                )
            }
            Err(why) => crate::library_api::failed("the library's filed works", &why),
        }
    })
    .await
}
```

`visible_editions(conn, &[])` answers every edition of every work, which is
what an unrestricted caller is owed here. Add the catalog's two entries.
Register the four routes at the end of `routes()`:

```rust
("/opds/v1.2/libraries", get(libraries)),
("/opds/v1.2/libraries/{library}/works", get(library_works)),
("/opds/v1.2/libraries/by-work", get(libraries_by_work)),
("/opds/v1.2/libraries/{library}/works/by-work", get(library_works_by_work)),
```

`filed_in` in `browse.rs`, beside `visible_editions`:

```rust
/// The slugs of the works `work.library_id` files in `library`, which is the
/// store's own answer to which library a work is in. A listing narrowed to a
/// library reads its folders instead (see [`library_scope`]); this is for the
/// one view that asks the column.
///
/// # Errors
///
/// Returns an error if the query fails.
pub fn filed_in(conn: &Connection, library: i64) -> Result<HashSet<String>, String>
```

- [ ] **Step 3: The grant postures**

At the end of `routes::grant_postures`, in the same order as `routes()`:

```rust
("/opds/v1.2/libraries", Filters),
// A library outside the grant answers 404, as one that does not exist does,
// and a granted library's works are listed with only the editions under its
// folders, as `/opds/v1.2/works` lists them under the grant's.
("/opds/v1.2/libraries/{library}/works", RefusesAndFilters),
("/opds/v1.2/libraries/by-work", Filters),
// The same 404, and the works filed in the library listed with the editions
// the grant allows.
("/opds/v1.2/libraries/{library}/works/by-work", RefusesAndFilters),
```

In `auth/tests.rs::every_route_declares_its_grant_posture`:

- `driven` goes from 22 to 26; extend its message by one sentence: F3's four
  OPDS routes raised it to 26. The `GrantIndependent` count stays 36.
- Skip both `{library}` routes in **both** `Refuses` loops (the placeholder
  is a library, not a work), with a comment saying they are driven in their
  own block. That block, over `router_with_material_under_a_withheld_root`,
  for each of the two: the withheld folder's library answers 404, and the
  held folder's library answers 200 (it holds no material; the 200 is what
  shows the 404 is the grant's and not a broken route).
- Filters, over `two_works_one_withheld`: `/opds/v1.2/libraries` and
  `/opds/v1.2/libraries/by-work` each name `visible-root` and not
  `hidden-root`; `/opds/v1.2/libraries/<visible-root's library>/works` names
  `Visible Work` and not `Hidden Work`. For the by-work feed, file **both**
  works in the visible folder's library by hand
  (`UPDATE work SET library_id = …`): its by-work feed must still name
  `Visible Work` and not `Hidden Work`, which only the grant can keep out.
  Over `one_work_two_editions_split_across_roots`, with the work filed in the
  granted folder's library: that library's feed and its by-work feed each
  carry `1 editions`. Insert all four patterns into `proven_filters`.

- [ ] **Step 4: Hand defects, each watched failing**

| Defect | Must fail |
| --- | --- |
| `library_works` passes `&roots` (the grant) to `visible_editions` | `a_library_s_works_feed_lists_only_the_works_under_its_folders` |
| `library_works` passes `&[]` to `library_scope` | `a_library_outside_the_grant_is_refused_…`, the posture proof's 404 |
| `libraries` passes `&[]` to `status` | `a_restricted_reader_s_libraries_feed_names_only_its_libraries`, the posture proof |
| `works_feed` ignores `visible` (treats it as `None`) | `a_library_s_works_feed_counts_only_the_editions_…` |
| the catalog's Libraries entry deleted | `the_catalog_links_to_the_libraries` |
| `library_works_by_work` drops the `retain` | `the_by_work_feed_lists_a_work_under_the_library_it_is_filed_in_…` (the withheld library's feed lists it) |
| `library_works_by_work` reads `visible_editions(conn, &scope.folders)` | the same test (`1 editions` where `2`) |
| `library_works_by_work` reads `visible_editions(conn, &[])` | `a_restricted_reader_s_by_work_feed_…`, the posture proof's by-work Filters half |
| `filed_in`: `library_id = ?` → `library_id <> ?` | `filed_in_answers_the_slugs_…` |
| `libraries_body` links `View::Filed` entries to `/works` | `the_by_work_libraries_feed_links_each_library_…` |

- [ ] **Step 5: Whole crates, and commit**

```bash
git add crates/taiyaku-library/src/browse.rs crates/taiyaku-library/src/browse/tests.rs \
  crates/taiyaku-serve/src/opds.rs crates/taiyaku-serve/src/opds/tests.rs \
  crates/taiyaku-serve/src/routes.rs crates/taiyaku-serve/src/auth/tests.rs
git commit -m "F3: OPDS lists the libraries, and each library's works by folder and by work"
```

---

## Task 3: The browse screens know libraries

**Files:**
- Modify: `web/src/library.ts`, `web/src/library.test.ts`
- Modify: `web/src/Sidebar.tsx`, `web/src/Sidebar.test.tsx`
- Modify: `web/src/Shell.tsx`, `web/src/Shell.test.tsx`
- Modify: `web/src/browseModule.ts`, `web/src/browseModule.test.ts`
- Modify: `web/src/Library.tsx`, `web/src/Library.test.tsx`
- Modify: `web/src/EmptyState.tsx`, `web/src/EmptyState.test.tsx`
- Modify: `web/src/styles/shell.css`
- Modify: every other test that builds a status (Step 1)

**Interfaces:**
- Consumes: the HTTP contract in Task 1 (mocked; this lane does not need the
  server).
- Produces:

```ts
// library.ts
export interface LibraryEntry { id: number; name: string; scanned: boolean }
export interface LibraryStatus { libraries: LibraryEntry[] }
export const home: (library?: number) => Promise<HomeLists>
export const grid: (library?: number) => Promise<Grid>
export const search: (q: string, library?: number) => Promise<SearchHits>

// Sidebar.tsx
export interface SidebarProps {
  libraries: LibraryEntry[]
  admin?: boolean
  openProposals?: number
}

// EmptyState.tsx: 'no-roots' is renamed 'no-libraries'; 'no-library' is new.
```

- [ ] **Step 1: `library.ts`, and every status a test builds**

```ts
/**
 * One library, as `/api/library/status` lists it — spec F §3. `scanned` says
 * whether any of its folders has been walked, so a library's page with
 * nothing on it can say "not read yet" rather than "nothing here".
 */
export interface LibraryEntry {
  id: number
  name: string
  scanned: boolean
}

export interface LibraryStatus {
  libraries: LibraryEntry[]
}

/** `path` narrowed to one library, or as it is for every library. */
const narrowedTo = (path: string, library?: number) =>
  library === undefined
    ? path
    : `${path}${path.includes('?') ? '&' : '?'}library=${library}`

export const home = (library?: number) =>
  getJson<HomeLists>(narrowedTo('/api/library/home', library))

export const grid = (library?: number) =>
  getJson<Grid>(narrowedTo('/api/library/series', library))

export const search = (q: string, library?: number) =>
  getJson<SearchHits>(
    narrowedTo(`/api/library/search?q=${encodeURIComponent(q)}`, library),
  )
```

Delete `RootCard`. In `library.test.ts`, the `grid` case expects
`/api/library/series?library=7`; add the same pair for `home` and for
`search` (`/api/library/search?q=a%20b&library=7`, which also shows the `&`).

Then `pnpm -C web run typecheck` lists the files whose status no longer
type-checks; and `command grep -a -rln "roots: \[\|scanned:" web/src` lists
the ones that build it as untyped JSON, which typecheck does not see. On
`main` the files naming a status or a `roots: [` list are `Config.test.ts`,
`Gate.test.tsx`, `Libraries.shell.browser.test.tsx`, `Library.test.tsx`,
`Routes.test.tsx`, `RuleEditor.browser.test.tsx`, `RuleEditor.test.tsx`,
`Shell.browser.test.tsx` and `Shell.test.tsx`. Not every `roots: [` is a
status (the rule editor and the folder rows list folders too): read each.
Each status becomes `{ libraries: [...] }` with `scanned` on each entry,
keeping what the test was about.

- [ ] **Step 2: The sidebar, failing**

`Sidebar.test.tsx`: every `roots={…} showRoots={…}` becomes `libraries={…}`.
Replace the two roots tests and the marking test:

```tsx
it('lists every library it is given, each linking to its own page', () => {
  const host = at(
    <Sidebar
      libraries={[
        { id: 4, name: 'Manga', scanned: true },
        { id: 9, name: 'Comics', scanned: false },
      ]}
    />,
  )
  const links = [...host.querySelectorAll('.sidebar-libraries a')]
  expect(links.map((a) => [a.textContent, a.getAttribute('href')])).toEqual([
    ['Manga', '/?library=4'],
    ['Comics', '/?library=9'],
  ])
  expect(host.querySelector('h2')?.textContent).toBe('Libraries')
})

it('draws no libraries heading when there is no library', () => {
  const host = at(<Sidebar libraries={[]} />)
  expect(host.textContent).not.toContain('Libraries')
})

it('marks the screen it is on, and a library page its own library and not the library', () => {
  const on = (path: string) =>
    mount(
      <MemoryRouter initialEntries={[path]}>
        <Sidebar
          libraries={[
            { id: 4, name: 'Manga', scanned: true },
            { id: 9, name: 'Comics', scanned: true },
          ]}
          admin={true}
        />
      </MemoryRouter>,
    )
  const current = (host: HTMLElement) =>
    [...host.querySelectorAll('[aria-current="page"]')].map((a) =>
      a.getAttribute('href'),
    )
  expect(current(on('/'))).toEqual(['/'])
  expect(current(on('/?library=9'))).toEqual(['/?library=9'])
  expect(current(on('/search'))).toEqual(['/search'])
  expect(current(on('/search?library=9'))).toEqual(['/search'])
  expect(current(on('/config/libraries'))).toEqual(['/config'])
  expect(current(on('/series/3'))).toEqual([])
})
```

Run: `pnpm -C web exec vitest run --project jsdom --reporter=default src/Sidebar.test.tsx`
Expected: FAIL (type errors aside, no `.sidebar-libraries`, and `/?library=9`
marks `/`).

- [ ] **Step 3: `Sidebar.tsx`**

The props as in Interfaces. The Library entry becomes a `Link` marked by
hand; the library entries likewise:

```tsx
const { pathname } = useLocation()
const [params] = useSearchParams()
// `/?library=N` is a library's page. `NavLink` compares the path and not the
// query, so it would mark the Library entry there too: both are marked here.
const shown = pathname === '/' ? params.get('library') : null
…
<li>
  <Link
    to="/"
    aria-current={pathname === '/' && shown === null ? 'page' : undefined}
  >
    Library
  </Link>
</li>
…
{libraries.length > 0 && (
  <>
    <h2 className="sidebar-heading">Libraries</h2>
    <ul className="sidebar-libraries">
      {libraries.map((library) => (
        <li key={library.id}>
          <Link
            to={`/?library=${library.id}`}
            aria-current={shown === String(library.id) ? 'page' : undefined}
          >
            {library.name}
          </Link>
        </li>
      ))}
    </ul>
  </>
)}
```

The file's doc: the `ROOTS`-is-provisional paragraph, the `showRoots`
paragraph and the paragraph on `Library` passing `false` go. In their place,
one paragraph: the LIBRARIES list is every account's, the libraries
`/api/library/status` answers for it (spec F §3), each linking to its page,
`/?library=N`; and the `NavLink` paragraph says the Library entry and the
library entries are marked here, by path and query. `admin`'s paragraph
loses its `showRoots` comparison.

In `styles/shell.css` rename `.sidebar-roots` to `.sidebar-libraries` (both
rules), and the section-heading comment's "Roots" to "Libraries".

- [ ] **Step 4: `browseModule.ts` and `Shell.tsx`**

Delete `showRoots` from `BrowsePrefs` and the defaults, and its row from
`browseModule.test.ts`'s field list. In `Shell.tsx`, `usePrefsMap`'s loader
is `loadPrefs` itself (the admin fold goes, and the comment explaining it);
the `reset` comment about `showRoots` goes; the sidebar gets
`libraries={status?.libraries ?? []}`. The comment above `admin` says it
gates the proposals and settings entries.

`Shell.test.tsx` uses `showRoots` as its probe of the preference machinery
(the reporter, the reset, the persistence). Move each to `continueStrip`, a
boolean that defaults `true`, so each assertion's expected values flip:
read what each test claims and keep the claim. The tests whose subject was
the admin fold of `showRoots` go with it. Add:

```tsx
it('hands the sidebar every library the status lists, to any account', async () => {
  vi.mocked(library.status).mockResolvedValue({
    libraries: [
      { id: 1, name: 'Manga', scanned: true },
      { id: 2, name: 'Comics', scanned: false },
    ],
  })
  const host = await settle(<Shell>x</Shell>, who({ admin: false }))
  expect(
    [...host.querySelectorAll('.sidebar-libraries a')].map((a) => a.textContent),
  ).toEqual(['Manga', 'Comics'])
})
```

and a test that the settings section draws no "Show roots" checkbox.

- [ ] **Step 5: The library page and its empty states, failing**

`Library.test.tsx`: `defaultStatus` becomes
`{ libraries: [{ id: 1, name: 'Manga', scanned: true }] }`, so an empty grid
under it is `no-matches`, as `main`'s `{ roots: [], scanned: true }` was not
(that drew `no-roots`); check no test relied on that. Every `/?root=` becomes `/?library=`
and every "root" in a test name or comment "library". Replace the three
empty-state tests:

```tsx
it('a server with no library is told so, rather than shown an empty grid', async () => {
  const host = await show(undefined, { libraries: [] })
  expect(host.textContent).toContain('No library yet')
  expect(host.textContent).toContain('Configuration → Libraries')
})

it('a library page asks whether its own library has been read', async () => {
  const status = {
    libraries: [
      { id: 1, name: 'Manga', scanned: true },
      { id: 2, name: 'Comics', scanned: false },
    ],
  }
  const comics = await showAt('/?library=2', status)
  expect(comics.textContent).toContain('Not read yet')
  const manga = await showAt('/?library=1', status)
  expect(manga.textContent).toContain('No series match this filter.')
})

it('the page of every library is not read yet only while none is', async () => {
  const one = await show(undefined, {
    libraries: [
      { id: 1, name: 'Manga', scanned: false },
      { id: 2, name: 'Comics', scanned: true },
    ],
  })
  expect(one.textContent).toContain('No series match this filter.')
  const none = await show(undefined, {
    libraries: [{ id: 1, name: 'Manga', scanned: false }],
  })
  expect(none.textContent).toContain('Not read yet')
})

it('a library the status does not list is said to be no library here', async () => {
  const host = await showAt('/?library=7', {
    libraries: [{ id: 1, name: 'Manga', scanned: true }],
  })
  expect(host.textContent).toContain('No such library')
})

it('asks the server for the library in the URL, for the grid and for home', async () => {
  await showAt('/?library=4', { libraries: [{ id: 4, name: 'Manga', scanned: true }] })
  expect(vi.mocked(library.grid)).toHaveBeenLastCalledWith(4)
  expect(vi.mocked(library.home)).toHaveBeenLastCalledWith(4)
})

it('titles a library page with the library', async () => {
  await showAt('/?library=4', { libraries: [{ id: 4, name: 'Manga', scanned: true }] })
  expect(document.title).toContain('Manga')
})
```

`showAt(path, status)` is `show` with `MemoryRouter initialEntries={[path]}`;
add it beside `show`. Read `title.ts` for what `useDocumentTitle` writes before
asserting on `document.title`.

`EmptyState.test.tsx`: the `no-roots` case becomes `no-libraries`; add one
for `no-library`; the `not-scanned` case asserts the new words.

Run: `pnpm -C web exec vitest run --project jsdom --reporter=default src/Library.test.tsx src/EmptyState.test.tsx`
Expected: FAIL.

- [ ] **Step 6: `Library.tsx` and `EmptyState.tsx`**

`Library.tsx`:

```tsx
const libraryParam = params.get('library')
const library = libraryParam === null ? undefined : Number(libraryParam)
const status = useLibraryStatus()
/** The library this page is narrowed to, once the status names it. */
const entry =
  library === undefined
    ? undefined
    : status?.libraries.find((l) => l.id === library)
useDocumentTitle(entry?.name ?? 'Library')
…
Promise.all([fetchGrid(library), fetchHome(library)])   // effect deps [library]
…
<h1 className="visually-hidden">{entry?.name ?? 'Library'}</h1>
…
<EmptyState kind={emptyKind(status, library)} />
```

with, at module scope,

```ts
/**
 * Which first-run screen an empty grid is (spec C1 §8). A library's page asks
 * of its own library, and the page of every library of them all: one library
 * read and another not is "nothing here" on the first and "not read yet" on
 * the second.
 */
function emptyKind(status: LibraryStatus, library: number | undefined): EmptyKind {
  if (library !== undefined) {
    const entry = status.libraries.find((l) => l.id === library)
    if (entry === undefined) return 'no-library'
    return entry.scanned ? 'no-matches' : 'not-scanned'
  }
  if (status.libraries.length === 0) return 'no-libraries'
  return status.libraries.some((l) => l.scanned) ? 'no-matches' : 'not-scanned'
}
```

The file's doc: "`?root=` is a query parameter…" says the same of
`?library=`. Every comment saying "root" about the narrowing says "library".

`EmptyState.tsx`: `'no-roots'` → `'no-libraries'`, `'no-library'` added. The
words:

```tsx
if (kind === 'no-libraries') {
  return (
    <section className="empty">
      <h2>No library yet</h2>
      <p>
        Nothing has been added to this server. An administrator makes a
        library from folders of archives on Configuration → Libraries, or
        from the shell:
      </p>
      <pre>taiyaku library scan --add /path/to/manga --library Manga</pre>
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
      <p>
        This library has folders, and nothing has scanned them. Scan library,
        on Configuration → Libraries, reads them, and so does:
      </p>
      <pre>taiyaku library scan</pre>
    </section>
  )
}
if (kind === 'no-library') {
  return (
    <section className="empty">
      <h2>No such library</h2>
      <p>
        There is no library here to show: it may have been removed, or this
        account may not see it.
      </p>
    </section>
  )
}
```

The file's doc: the "No roots configured" paragraph speaks of libraries,
and the sentence that the two library states point at a shell command
because the act is not on the page says what is true now: they name the
screen that does it, and the command beside it.

- [ ] **Step 7: Hand defects, each watched failing**

| Defect | Must fail |
| --- | --- |
| `Library.tsx` fetches `fetchGrid(undefined)` | `asks the server for the library in the URL…` |
| `Library.tsx` fetches `fetchHome()` | the same |
| `emptyKind` answers `entry.scanned` from `status.libraries[0]` | `a library page asks whether its own library…` |
| `emptyKind`'s every-library branch uses `.every` for `.some` | `the page of every library is not read yet only while none is` |
| `emptyKind` answers `'no-matches'` where `entry` is undefined | `a library the status does not list…` |
| `Sidebar`'s Library entry is a `NavLink` again | `marks the screen it is on…` (`/?library=9`) |
| `Sidebar` compares `shown` with `library.id` without `String(…)` | the same |
| `Sidebar` draws the list only for `admin` | `Shell`'s `hands the sidebar every library…` |
| `narrowedTo` always uses `?` | `library.test.ts`'s search case |

- [ ] **Step 8: Whole projects, and commit**

Run the web gate in Global Constraints. Then `git add` the files above and
commit: `F3: the sidebar lists libraries for everyone, and a library's page narrows the grid and the continue strip`.

---

## Task 4: The sidebar stays current with the Libraries screen

**Files:**
- Modify: `web/src/shell.ts`, `web/src/Shell.tsx`, `web/src/Shell.test.tsx`
- Modify: `web/src/Libraries.tsx`
- Modify: `web/src/Libraries.shell.browser.test.tsx`

**Interfaces:**
- Produces: `shell.ts`'s `StatusRefreshContext` and `useStatusRefresh(): () => void`,
  a no-op where no shell sits above.

- [ ] **Step 1: The tests, failing**

In `Shell.test.tsx`: a child that calls `useStatusRefresh()` on a button
press; after the press `library.status` has been called twice, and the
sidebar draws the second answer's libraries.

In `Libraries.shell.browser.test.tsx`, beside its existing make-a-library
test: the status answers `{ libraries: [] }` first and
`{ libraries: [{ id: 3, name: 'Manga', scanned: false }] }` after; make a
library through the form; the sidebar's `.sidebar-libraries` then holds a
link named `Manga` to `/?library=3`, with no reload. A second test: a
removal re-reads the status too (the removed library leaves the sidebar).

Run them: FAIL (`useStatusRefresh` is not exported; then, the sidebar keeps
its first answer).

- [ ] **Step 2: The context, the shell, the screen**

```ts
// shell.ts
/**
 * Ask the chrome to read `/api/library/status` again.
 *
 * The shell reads it once, when it mounts, and the sidebar lists the
 * libraries from it. Configuration → Libraries makes, renames, removes and
 * scans libraries under that same sidebar, so it asks for a read after each
 * of its own reads of the list: without one, a removed library stayed in the
 * sidebar until a reload (O200).
 */
export const StatusRefreshContext = createContext<() => void>(() => {})

/** Re-read the status after a write that moved it. */
export function useStatusRefresh(): () => void {
  return useContext(StatusRefreshContext)
}
```

`Shell.tsx`: a `statusReads` counter and `refreshStatus` callback shaped as
`inboxReads`/`refreshInbox` are, the status effect depending on
`statusReads` (with the same `biome-ignore` reason the inbox effect carries),
and `StatusRefreshContext.Provider value={refreshStatus}` beside
`InboxRefreshContext`'s. A failed re-read is drawn as the first read's
failure is.

`Libraries.tsx`: `const refreshStatus = useStatusRefresh()`; in `reload`'s
`.then`, after `setLibraries`, call `refreshStatus()`; add it to `reload`'s
dependencies. The file's doc gains one sentence: each read of the list asks
the shell to read the status again, so the sidebar's libraries follow it.

- [ ] **Step 3: Hand defects, each watched failing**

| Defect | Must fail |
| --- | --- |
| the status effect's dependency list is `[]` | `Shell.test.tsx`'s re-read test |
| `Libraries.tsx` does not call `refreshStatus` | both `Libraries.shell.browser.test.tsx` tests |
| the provider is left out of `Shell.tsx` | all three |

- [ ] **Step 4: Whole projects, and commit**

`F3: the sidebar re-reads the libraries after Configuration → Libraries changes them`.

---

## Task 5: Search in one library

*Written to spec correction 4's recommendation. If the user rules for the
alternative, this task is: nothing on screen changes, and `library.ts`'s
`search` loses its `library` argument; skip to Step 5.*

**Files:**
- Modify: `web/src/Search.tsx`, `web/src/Search.test.tsx`
- Modify: `web/src/styles/library.css`

**Interfaces:**
- Consumes: `search(q, library?)`, `useLibraryStatus()` (Task 3).

- [ ] **Step 1: The tests, failing**

In `Search.test.tsx` (read how it mounts and mocks `library.search` first):

- with a status of two libraries, a select labelled "Search in" offers
  "Every library", then each library by name, in the status's order;
- with a status of one library, no select is drawn;
- at `/search?q=Series&library=2`, `search` is called with `('Series', 2)`
  and the select shows the library;
- choosing a library calls `search(q, id)` and puts `library=<id>` in the
  URL beside `q`; choosing "Every library" takes it out and calls
  `search(q, undefined)`;
- typing in the box keeps the chosen library in the URL (the box's
  `setParams({ q })` on `main` would drop it).

- [ ] **Step 2: `Search.tsx`**

Read `library` from the params as `Library.tsx` does. The debounced `run`
takes `(query, library)` and calls `fetchSearch(query, library)`; the effect
calls `run(q, library)` and depends on `[q, library, run]`. The box's
`onChange` edits a copy of the params, so `library` survives:

```tsx
onChange={(e) => {
  const next = new URLSearchParams(params)
  if (e.target.value === '') next.delete('q')
  else next.set('q', e.target.value)
  setParams(next, { replace: true })
}}
```

Under the box, while `status && status.libraries.length > 1`:

```tsx
<label className="search-scope">
  <span>Search in</span>
  <select
    value={library === undefined ? '' : String(library)}
    onChange={(e) => {
      const next = new URLSearchParams(params)
      if (e.target.value === '') next.delete('library')
      else next.set('library', e.target.value)
      setParams(next, { replace: true })
    }}
  >
    <option value="">Every library</option>
    {status.libraries.map((l) => (
      <option key={l.id} value={l.id}>
        {l.name}
      </option>
    ))}
  </select>
</label>
```

A rule for `.search-scope` in `styles/library.css`, beside `.search-box`'s,
in its tokens. The file's doc gains one paragraph: `?library=` is in the URL
for `?q=`'s reason, and the select is drawn only where there is a choice.

- [ ] **Step 3: Hand defects, each watched failing**

| Defect | Must fail |
| --- | --- |
| `run` calls `fetchSearch(query)` | the `('Series', 2)` test |
| the box's `onChange` sets `{ q }` alone | the typing test |
| the select is drawn at one library or more | the one-library test |
| the effect's dependencies are `[q, run]` | the choosing test |

- [ ] **Step 4: Whole projects, and commit**

`F3: search narrows to one library`.

- [ ] **Step 5 (alternative only)**

Take `library` out of `search` in `library.ts` and its test; whole projects;
commit.

---

## Task 6: On a live server, with the user, and on real material

The controller's, after Tasks 1–5 land on the branch. The user decides UI by
trying it: put it in front of them before the docs are written.

- [ ] **Step 1: A scratch server over a copy of the real library**

By the recipe in the memory "UI work: prototype on a live server", and F2b's
Task 7 drive: the original's digest and a marker first; a copy of
`/home/davey/temp/manga-test/library.db` in the session scratchpad; the
branch's release binary copied beside it; the account `local` / `pw` on the
copy; the built app; served with `--folders <scratch share>` on
`127.0.0.1:4399`; the PID recorded. Make a scratch share holding two
folders, each a few volumes copied from the real library: `Ecchi` (a series
the copy does not hold) and `Chapters` (a volume of a series the copy holds).

As an administrator, in a real browser: make a library `Ecchi` from the
first folder and Scan library; make `Chapters` and do not scan it. Then
record, stamped with the commit, in `$EVID/drive.log`:

1. The sidebar lists the three libraries for the administrator, by name,
   without a reload after each was made.
2. Each library's page shows its own series only; the copy's library shows
   none of Ecchi's; `Chapters` says "Not read yet"; `/?library=999` says
   "No such library".
3. The Continue strip on a library's page holds only that library's works
   (turn two pages in one work of each library first).
4. Search "a" across every library and in each.
5. An account granted only `Ecchi` (made on the Users screen, on the copy):
   its sidebar lists `Ecchi` alone; `/?library=<the copy's library>` says
   "No such library"; its search finds nothing of the copy's.
6. OPDS with that account's credentials (`curl -u`): `/opds/v1.2/catalog`,
   `/opds/v1.2/libraries` (Ecchi alone), its works feed, and the copy's
   library's id answering 404; then the same through
   `/opds/v1.2/libraries/by-work`. As the administrator, compare each
   library's two feeds over the copy (the copy's one folder came from
   migration 15, so its works are filed where their folders are, and the two
   views should agree there; say so if they do not).
7. Remove `Chapters` on Configuration → Libraries: it leaves the sidebar
   without a reload.

- [ ] **Step 2: The user's try**

Tell the user the URL and what to look at: the sidebar's LIBRARIES list,
each library's page, the empty states' words, and search's "Search in".
Say what does not change: the series page, the reader, the flat OPDS works
feed. Ask two things beside whatever they see: whether the top entry should
still read "Library" now that a list of libraries is below it, and whether a
single library should still be listed. If they have an OPDS client to hand
(Panels, on the tailnet with TLS; see the memory "Device testing"), ask
whether the Libraries level reads well there: §13 says no client has been
watched showing a level above the works feed. Make what they ask for, with
tests, before Task 7.

- [ ] **Step 3: Stop the server and prove the original untouched**

Kill by PID (check `ss -lptn 'sport = :4399'` and `/proc/<pid>/exe` first);
`find /home/davey/temp/manga-test -newer <marker>` prints nothing; the digest
is unchanged. Record both in `$EVID/drive.log`.

---

## Task 7: The docs, the sweep, the gate and the claim check

**Files:**
- Modify: `docs/status.md`, `docs/notes.md`, `docs/mutants-baseline.md`,
  `README.md` (one sentence, where the OPDS catalog is described)
- Create (gitignored): `$EVID/*`, `.superpowers/sdd/libraries-browse-pr-body.md`

- [ ] **Step 1: `docs/status.md`**

- The paragraph ending "The web app still shows folders, as roots, until F2
  and F3." says what is true: the web app and OPDS browse by library (spec
  Part F3).
- **O200** is narrowed again, with the five-part edit's narrowing form: the
  sidebar's heading and `EmptyState` say library; the sidebar no longer goes
  stale after a removal. What stays open is the rest of its list
  (`RuleForm`, `ProposalRow`, `PreviewTabs`, `AcceptForm`, the rule routes'
  sentence, `scan.rs`'s store faults).
- **O191** gains one sentence: a library's page, its OPDS feed and an
  account granted only that library list such a work under the library whose
  folder holds its edition, with that edition alone.
- Open a row for each defect a task's report set aside.
- The test counts, measured from the runs and never added up: the Rust
  figure at both its sites, and the web `node`/`jsdom`/`browser` figures,
  which no gate checks.
- The sweep:

  ```bash
  command grep -a -c '^| O[0-9]' docs/status.md
  command grep -a -c '^| O[0-9]* | \[x\]' docs/status.md
  command grep -a -n -E '[0-9]{3,} tests|Nine of the' docs/status.md
  command grep -a -n -A1 '^What remains:' docs/status.md
  ```

- `command grep -a -rn 'O200\|O191' docs web/src crates` and read every hit.

- [ ] **Step 2: `docs/notes.md`, and README**

Add "## Libraries, F3 (<date>)": the narrowing (folders swapped for the
grant's share of a library's, `library_scope`'s `None`); each spec
correction and what was built; the drive's answers; what the user said at
the hand check; counts against `main`; and what was not measured (the NAS;
an OPDS client, unless the user tried one; a store with many libraries).
Every figure is one read off a log in `$EVID`.

README, beside the catalog URL: the catalog lists every work, and each
library's.

- [ ] **Step 3: The sweep, the review, the gate**

Commit; `scripts/mutants.sh --since main` into `$EVID`, read against its
traps (`browse`'s narrowing is SQL in strings and `filter` closures; the
hand defects of Tasks 1–5 are the evidence where it makes no mutant). A
review of `git diff main...HEAD` whole by a reviewer who has not seen the
tasks, asked in particular for any listing, feed or count that can name a
work, an edition or a library outside the caller's grant once a narrowing is
in force, and for any narrowing that answers more than its library: each
claim run, not read. Fixes with tests watched failing, and the fixes
reviewed. Then:

```bash
H=$(git rev-parse --short HEAD)
env -u TAIYAKU_CORPUS NEXTEST_TEST_THREADS=8 RUST_TEST_THREADS=8 \
  scripts/check-all.sh >| "$EVID/gate-${H}.log" 2>&1; echo $? >| "$EVID/gate-${H}.exit"
command grep -a -n 'all green' "$EVID/gate-${H}.log"; cat "$EVID/gate-${H}.exit"
```

and the three crates' tests on SQLite 3.40.1 in `rust:1-bookworm` (the
memory "SQLite 3.40.1 in CI and the image"; the two `strace` tests fail
there by design): `status`'s `COLLATE NOCASE` order and the `EXISTS` in its
select list are new SQL.

- [ ] **Step 4: The PR body, the claim check, and stop**

Write `.superpowers/sdd/libraries-browse-pr-body.md`, not hard-wrapped.
Title: "Libraries, F3: the sidebar lists libraries, each library has its
page, and OPDS has a Libraries level". Copy the subagents' hand-backs out of
the session `.jsonl` into `$EVID` before the first pass. Check every factual
sentence in the status rows, the notes section, the PR body and this
branch's new comments against the code and the logs, in a separate pass by
a checker handed the files and no list of what to expect, the PR body in the
first pass. Fix by deleting a figure or a reason before correcting it twice;
grep a corrected sentence's shape across every document. Repeat on each fix
commit until a pass finds no false sentence.

Report to the user: the head commit, the gate's line, the sweep's figures,
what the drive and the hand check showed, and each open question. Ask
before pushing. Opening the PR and merging it are each a separate yes.

---

## Self-review

**Spec coverage (§3, §4, §11's F3, §12).**

| Spec | Task |
| --- | --- |
| `/api/library/status` lists libraries; a restricted account sees only its own | 1 (Step 7) |
| `RootCard` gives way to a library entry | 1, 3 (`LibraryEntry`) |
| `?library=N` replaces `?root=N`, through grid, search and home | 1 (Steps 4, 8) |
| Narrowing reuses the grant's machinery; `pair_clause`'s both-sides rule | 1 (Step 4); the aligned tests, Step 6 |
| The series page takes no parameter | unchanged (`one_series` passes `None`) |
| The sidebar's ROOTS becomes LIBRARIES, for everyone; `showRoots` goes | 3 |
| Home with no library shows all; a library's page shows its own | 1, 3 |
| Unfiled works on Home for the unrestricted, under no library | 1 (`asking_for_a_library_leaves_the_unfiled_bucket_empty`) |
| OPDS: `/opds/v1.2/libraries` and `/libraries/{id}/works`; `/works` stays | 2 |
| The user's ruling: a second OPDS view by `work.library_id` | 2 (`/by-work`) |
| OPDS narrowing on the blocking pool | 2 (`on_the_library`) |
| Libraries list by name (§10) | 1 (Step 7) |
| §12 Browsing: `?library=` narrows grid, search and home | 1 |
| §12 Access: one library's works only, in OPDS too | 1, 2, the posture proofs |
| §12 Real material: library pages and OPDS show each library's works only | 6 |

**Placeholders.** Task 2 Step 1 and Task 4 Step 1 describe their tests in
sentences where the fixtures are the existing ones: each names the fixture,
the request and the assertion. Task 5 is conditional on a ruling, and says
what it becomes either way.

**Names.** `library_scope` / `LibraryScope` (Task 1) are what Task 2 calls;
`LibraryEntry` is one name in Rust and TypeScript; `useStatusRefresh` is
Task 4's in both its producer and its consumer; `emptyKind` and the
`EmptyKind` values `'no-libraries'`, `'no-library'`, `'not-scanned'`,
`'no-matches'` agree between Task 3's Steps 5 and 6.
