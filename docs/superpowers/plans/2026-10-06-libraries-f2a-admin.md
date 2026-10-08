# Libraries, F2a (the admin screen): Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** An administrator creates, renames, scans and removes libraries from
Configuration → Libraries, picks folders in a browser limited to the mounts
the server was told about, and grants libraries, not folders, on the Users
screen. After it merges the user can create the NAS's libraries and run the
first scans from the web app.

**Architecture:** The store gains what the screen needs and nothing about how
works are derived: a library made with its folders in one transaction, a
folder added to a library by id, a rename, a library removed with one summed
count, and grants written by library id. A new `libraries_api` module serves
`/api/library/libraries*` over those, beside `config_api`, whose folder routes
stay for the folder rows. A library's scan walks each folder, derives once,
and reports through the per-root `scans` registry the folder rows already
poll. A new `folders_api` module lists subdirectories beneath configured
prefixes. In the web app `Libraries` replaces `Roots`: a card per library
holding today's `RootRow` per folder.

**Tech Stack:** Rust 2024 (workspace `rust-version` 1.85), rusqlite against
the system SQLite, axum 0.8, cargo-nextest in the gate; React 19, react-router
8, vitest (projects `node`, `jsdom`, `browser`), biome, pnpm.

## Global Constraints

- **Branch:** `libraries-admin`, which already holds the spec commit
  `19aa301c` and this plan, cut from `main` at `d963659a`. Work on it.
- **The spec is `docs/superpowers/specs/2026-10-05-libraries-design.md`.**
  Read "Decisions the user made, 2026-10-06, for F2", §5, §6, §6.1, §7, §8,
  §11 and §12 before Task 1. §6.2 (Move) is F2b's and is not built here.
  Where this plan and the spec disagree, stop and ask; do not pick one. The
  plan's own disagreements are under "Spec corrections needed" below and are
  for the user, not for you.
- **No attribution lines anywhere**: not in commit messages, code comments or
  documentation.
- **Every command runs from `/home/davey/projects/taiyaku`.** Never `cd`: this
  shell's `cd` is zoxide and has landed in another repository. Web commands
  are `pnpm -C web …`. The Bash tool runs zsh with `noclobber`, so write
  scratch output to a new file name or use `>|`. Quote globs
  (`--include='*.rs'`). Never name a shell variable `path` (zsh ties it to
  `PATH`). Brace a variable before a colon (`${S}:x`). `grep` is a shell
  function here that skips logs it takes for binary: write `command grep -a`.
- **Throttle every Rust test run**: `cargo test … -- --test-threads=8`, and
  `NEXTEST_TEST_THREADS=8 RUST_TEST_THREADS=8` for the gate.
- **Every new test is watched failing with its defect put back by hand.** Copy
  the production file aside first (`cp X "$SCRATCH/X.orig"`), make the edit
  the step names, run the test, read the failure, restore with `cp` and prove
  the restore with `cmp`. Put the failure's first line in the task's report.
  If the named defect does **not** make the test fail, stop and report: do not
  weaken the defect or change the test to make it fail. Never `git stash`.
- **A hand defect in SQL changes what the statement means, never how many
  parameters it binds.** Write `AND (library_id = ?2 OR 1 = 1)`, not
  `AND 1 = 1` with `?2` still bound.
- **A Rust task is done only when whole crates pass**, not the files it
  touched: `cargo test -p taiyaku-library -p taiyaku-serve -p taiyaku-cli --
  --test-threads=8`, plus `cargo clippy --workspace --all-targets
  --all-features -- -D warnings` and `cargo fmt --all -- --check`.
- **A web task is done only when whole projects pass**: `pnpm -C web run lint`,
  `pnpm -C web run typecheck`,
  `pnpm -C web exec vitest run --project node --project jsdom --reporter=default`,
  and, for a task that changes a component or a stylesheet,
  `pnpm -C web run test:browser`. Under an agent vitest picks a reporter that
  hides every passing test's console output; `--reporter=default` is what
  shows it.
- **A class a component writes has a stylesheet rule in the same commit.**
  `web/src/classNames.test.ts`, in the node project, fails for a class no
  selector names. Tasks 8, 9 and 10 each add a starting rule for the classes
  they introduce, and Task 11 corrects those rules by measurement.
- **A failure is "pre-existing" only if it fails the same way on `main` at
  `d963659a`**, checked in a worktree, never against another commit on this
  branch.
- **No figure in a code comment that a later change would have to correct**:
  no counts of tests, call sites, routes or libraries.
- **Comments say what the code does and why it must.** Write them in the
  surrounding files' voice. Do not narrate this plan, its tasks or its reviews
  in them.
- **The snippets below are a starting point.** The interfaces are exact: the
  names, parameters, return types, routes and JSON keys. The fixtures'
  plumbing (helper names, imports, `expect` texts) is yours to fix against the
  code as it is. Re-read every signature a step names before writing against
  it. If a step's *claim about existing code* is wrong, stop and report rather
  than work around it.
- **A field added to `ServeState` goes in every literal that builds one.**
  `command grep -a -rn 'library_job:' crates/taiyaku-serve/src` lists them.
- **A guarded route is listed in four places**, and a test fails for each one
  missed: `routes::guarded_routes`, `routes::grant_postures`, and in
  `auth/tests.rs` the guarded-paths array and `REFUSE_A_DEVICE_CREDENTIAL`.
- **Evidence goes in `.superpowers/sdd/libraries-f2a-evidence/`**, which is
  gitignored, never under `target/`. The commands below call it `$EVID`:
  `EVID=.superpowers/sdd/libraries-f2a-evidence; mkdir -p "$EVID"`. Stamp
  each log with the commit it was taken at.
- **Never touch `/home/davey/temp/manga-test/`.** Task 12 copies its
  `library.db` into your session scratchpad and proves afterwards that the
  original is byte-identical.
- **Line numbers given are at `d963659a` and are hints only.** Find the words.
  If the words are not there at all, stop and report.
- **Nothing outside F2a changes.** No Move, no `derive` change, no
  `/api/library/status`, sidebar, grid or OPDS change. A defect found on the
  way goes in the task's report for Task 12's `docs/status.md` edit; do not
  fix it unless it blocks a step here.

## Spec corrections needed

The user should rule on these before or during execution. The plan is written
to the reading given here.

1. **§6.1: `TAIYAKU_FOLDERS` is split on `:` only.** The spec says "a list
   like `TAIYAKU_TRUSTED_PROXY`", which splits on commas and whitespace. A
   folder's path can hold either: the NAS share is `/mnt/user/media 1`. `:` is
   what `PATH` uses. `--folders DIR` is repeatable and replaces the variable.
2. **§6: a library's removal is not "the sum of its folders' `Removal`s" taken
   one folder at a time.** `scan::DOOMED_WORKS` counts a work for a folder
   only when every member of it is under that folder, so a work with an
   edition in each of a library's two folders is in neither folder's own
   count. The plan removes the folders one after another inside one
   transaction and sums as it goes: such a work is counted when its last
   folder goes.
3. **§6: "progress is reported per folder" is a running flag per folder, then
   every folder's report at once.** A folder's report carries the derive's
   report, and the derive runs once, after the last walk. So each folder's
   registry entry is running until the whole scan ends.
4. **§7's sentence is drawn on a count the store keeps.** The card says "no
   readable files yet" when the library has no work and at least one scanned
   entry whose format is `epub-fixed`, `epub-reflow` or `pdf`.

## Decided while planning

- **A taken library name answers 400**, at create and at rename, as a taken
  account name does (`users_api::create_user`): the caller's own mistake.
- **`GET /api/library/libraries`** answers
  `{"libraries": [{"id", "name", "works", "books", "folders": [root…]}]}`,
  by name, each folder in `root_json`'s shape, oldest first.
- **The add banner goes.** `Roots.tsx` drew one sentence for a folder just
  added ("the watcher is running, so it will be scanned every 24h"). The
  folder's own row draws the same two facts (`regimeFacts`), so the Libraries
  screen keeps the probe for the row and draws no banner.
- **`account::granted_roots` is deleted.** Its only production caller was
  `users_api::assembled`, which sends libraries now.
- **`Roots.tsx` outlives its route by four tasks.** The browser project's
  tests mount it, and they move in Task 11, which deletes it.
- **`RootRow` and `RemoveRoot` keep their names and CSS classes.** What they
  say on screen changes from "root" to "folder".

## Changed while building

Task 8's review changed three things this plan's Task 8 code does not show.
The interface block at the head of Task 8 is corrected; the step-by-step code
beneath it is as first written, and the tree is what was built.

- **`FolderPicker.onPick` is typed `(path: string) => unknown`.** The lint
  refuses `void` in a union (`void | Promise<unknown>` raised the warning
  count), and what the picker needs is only that a returned promise is
  awaited.
- **`FolderPicker` holds one pick at a time.** A ref guards the pick and a
  `picking` state disables the button while it is pending; the box is emptied
  only if it still holds the path that was picked; a rejection or a throw
  settles the picker and keeps the typed path. As first written a second press
  sent a second request.
- **The re-derive report of a removed library is drawn by the Libraries
  screen.** As first written, `LibraryCard` dropped the `RemovalAnswer` and
  reloaded, so a removal whose re-derive lost a position or could not write a
  document told nobody: the card that asked is not drawn once the list
  reloads. `LibraryCard` now takes `onRemoved(name, answer)` and `Libraries`
  draws the report (`.removal-report`, a `DeriveSection`) until a library is
  made or another is removed. `LibraryCard` also keeps a scan and a write
  apart (two in-flight refs), so a scan in progress does not hold a rename.

## File structure

**`crates/taiyaku-library/src/`**

- `libraries.rs`: `name_of`, `rename`, `books` join `list`, `id_of`, `create`.
- `scan.rs`: `InLibrary::Id`; `create_library`; `take_root` cut out of
  `apply_removal`; `library_removal_refusal`, `library_removal_cost`,
  `remove_library`.
- `account.rs`: `Patch::libraries` replaces `Patch::roots`; `granted_roots`
  goes.

**`crates/taiyaku-serve/src/`**

- `libraries_api.rs` (new) and `libraries_api/tests.rs`: the
  `/api/library/libraries*` handlers. It reads `config_api`'s JSON helpers,
  which become `pub(crate)`.
- `folders_api.rs` (new) and `folders_api/tests.rs`: `GET /api/library/folders`.
- `config_api.rs`: `create_root` goes; five helpers become `pub(crate)`;
  `roots_with_last_scan` and `panic_message` are cut out for sharing.
- `users_api.rs`: `libraries` in the patch and in `user_json`.
- `routes.rs`, `auth/tests.rs`: the route lists.
- `lib.rs`: `ServeState::folders`, `ServeOptions::folders`, two `mod` lines.

**`crates/taiyaku-cli/src/main.rs`**: `--folders`, `TAIYAKU_FOLDERS`, usage.

**`Dockerfile`, `README.md`**: `TAIYAKU_FOLDERS=/media` and the whole-share
mount.

**`web/src/`**

- `libraries.ts` (new): the client for every route above.
- `message.ts`: `sentenceOf`.
- `Libraries.tsx`, `LibraryCard.tsx`, `RemoveLibrary.tsx`, `FolderPicker.tsx`
  (new), each with a `.test.tsx`.
- `RootRow.tsx`, `RemoveRoot.tsx`: an `adopt` prop, a `lastOf` prop, "folder"
  for "root" on screen.
- `config.ts`, `Config.tsx`: the `libraries` tab.
- `users.ts`, `Users.tsx`, `UserRow.tsx`: grants by library.
- `styles/admin.css` and its two `.css.test.ts` files.
- `Config.browser.test.tsx`, `fieldNames.browser.test.tsx`: mount `Libraries`.
- `Roots.tsx`, `Roots.test.tsx`: deleted in Task 11.

---

## Task 1: The store makes a library with its folders, adds to one by id, and renames one

**Files:**
- Modify: `crates/taiyaku-library/src/libraries.rs`
- Modify: `crates/taiyaku-library/src/scan.rs` (`InLibrary`, `add_root_in`,
  `add_root_checked_in`, near lines 127–447)
- Test: `crates/taiyaku-library/src/libraries/tests.rs`,
  `crates/taiyaku-library/src/scan/tests.rs`

**Interfaces:**
- Consumes: `libraries::{id_of, create, trim_name}`, `scan::{add_root_in,
  roots, update_root, RootPatch, Added, AddRootError}`, `crate::Nested`,
  `crate::watch::{probe, interval_for}`.
- Produces:
  ```rust
  // libraries.rs
  pub fn name_of(conn: &Connection, id: i64) -> Result<Option<String>, String>;
  #[derive(Debug, Clone, PartialEq, Eq)]
  pub enum RenameError { Missing(i64), Unusable(String), Failed(String) }
  /// The name as stored.
  pub fn rename(conn: &Connection, id: i64, name: &str) -> Result<String, RenameError>;
  pub fn books(conn: &Connection, id: i64) -> Result<usize, String>;

  // scan.rs
  pub enum InLibrary<'a> { Own, Named(&'a str), Id(i64) }
  #[derive(Debug, Clone, PartialEq, Eq)]
  pub struct Created { pub library_id: i64, pub name: String, pub folders: Vec<Added> }
  pub fn create_library(conn: &Connection, name: &str, paths: &[PathBuf])
      -> Result<Created, AddRootError>;
  ```

- [ ] **Step 1: Write the failing tests for `libraries.rs`**

Append to `crates/taiyaku-library/src/libraries/tests.rs`:

```rust
#[test]
fn a_library_is_named_by_its_id() {
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let id = create(&conn, "Manga").expect("creates");
    assert_eq!(name_of(&conn, id).expect("reads").as_deref(), Some("Manga"));
    assert_eq!(name_of(&conn, id + 1).expect("reads"), None);
}

#[test]
fn a_rename_stores_the_name_stripped_and_answers_it() {
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let id = create(&conn, "Manga").expect("creates");
    assert_eq!(rename(&conn, id, "  Comics ").expect("renames"), "Comics");
    assert_eq!(name_of(&conn, id).expect("reads").as_deref(), Some("Comics"));
    assert_eq!(
        rename(&conn, id, "Comics").expect("its own name is not taken"),
        "Comics"
    );
}

#[test]
fn a_rename_is_refused_as_the_callers_mistake_or_as_no_such_library() {
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let manga = create(&conn, "Manga").expect("creates");
    create(&conn, "Comics").expect("creates");

    assert!(matches!(
        rename(&conn, manga, " \t"),
        Err(RenameError::Unusable(why)) if why.contains("needs a name")
    ));
    assert!(matches!(
        rename(&conn, manga, "Comics"),
        Err(RenameError::Unusable(why)) if why.contains("already called `Comics`")
    ));
    assert_eq!(
        name_of(&conn, manga).expect("reads").as_deref(),
        Some("Manga"),
        "a refused rename wrote nothing"
    );
    assert_eq!(rename(&conn, 999, "Other"), Err(RenameError::Missing(999)));
}

/// Two libraries, so a count that forgot the library reads the other's rows.
#[test]
fn books_counts_the_librarys_own_epub_and_pdf_entries() {
    let parent = crate::tests::tempdir("libraries-books");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let mut roots = Vec::new();
    for name in ["Books", "Manga"] {
        let dir = parent.path().join(name);
        std::fs::create_dir_all(&dir).expect("mkdir");
        roots.push(add_root_in(&conn, &dir, InLibrary::Named(name)).expect("adds"));
    }
    for (root, rel_path, format) in [
        (&roots[0], "a.epub", Some("epub-reflow")),
        (&roots[0], "b.epub", Some("epub-fixed")),
        (&roots[0], "c.pdf", Some("pdf")),
        (&roots[0], "d.cbz", Some("zip")),
        (&roots[0], "e.bin", None),
        (&roots[1], "f.pdf", Some("pdf")),
    ] {
        conn.execute(
            "INSERT INTO scan_entry (root_id, rel_path, kind, format, size, mtime)
             VALUES (?1, ?2, 'file', ?3, 1, 1)",
            rusqlite::params![root.id, rel_path, format],
        )
        .expect("a scanned entry");
    }

    assert_eq!(books(&conn, roots[0].library_id).expect("counts"), 3);
    assert_eq!(books(&conn, roots[1].library_id).expect("counts"), 1);
}
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p taiyaku-library libraries:: -- --test-threads=8`
Expected: FAIL to compile, `cannot find function `name_of``.

- [ ] **Step 3: Implement `name_of`, `rename` and `books`**

In `crates/taiyaku-library/src/libraries.rs`, after `create`:

```rust
/// The name of library `id`, if there is one.
///
/// # Errors
///
/// Returns an error if the read fails.
pub fn name_of(conn: &Connection, id: i64) -> Result<Option<String>, String> {
    conn.query_row("SELECT name FROM library WHERE id = ?1", [id], |r| r.get(0))
        .optional()
        .map_err(|e| format!("looking up library {id}: {e}"))
}

/// Why [`rename`] would not rename a library.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenameError {
    /// No library has this id.
    Missing(i64),
    /// The caller's own mistake: no name once trimmed, or one another library
    /// holds.
    Unusable(String),
    /// A store fault.
    Failed(String),
}

/// Rename library `id`, storing `name` stripped by [`trim_name`], and answer
/// the name stored.
///
/// **`library.name`'s UNIQUE is what refuses a name another library holds.**
/// A lookup before the write would be a second statement for a second writer
/// to land between, and the CLI is one.
///
/// # Errors
///
/// See [`RenameError`].
pub fn rename(conn: &Connection, id: i64, name: &str) -> Result<String, RenameError> {
    if name.trim().is_empty() {
        return Err(RenameError::Unusable("a library needs a name".to_string()));
    }
    let name = trim_name(name);
    match conn.execute(
        "UPDATE library SET name = ?2 WHERE id = ?1",
        rusqlite::params![id, name],
    ) {
        Ok(0) => Err(RenameError::Missing(id)),
        Ok(_) => Ok(name.to_string()),
        Err(rusqlite::Error::SqliteFailure(e, _))
            if e.code == rusqlite::ErrorCode::ConstraintViolation =>
        {
            Err(RenameError::Unusable(format!(
                "a library is already called `{name}`"
            )))
        }
        Err(e) => Err(RenameError::Failed(format!("renaming library {id}: {e}"))),
    }
}

/// How many entries the scans of library `id`'s folders recorded in a format
/// derivation does not admit yet and the books work will: EPUB and PDF.
///
/// The Libraries screen says "no readable files yet" on this, for a library
/// that holds such files and no work.
///
/// # Errors
///
/// Returns an error if the read fails.
pub fn books(conn: &Connection, id: i64) -> Result<usize, String> {
    use taiyaku_read::Format;
    conn.query_row(
        "SELECT count(*) FROM scan_entry e JOIN root r ON r.id = e.root_id
          WHERE r.library_id = ?1 AND e.format IN (?2, ?3, ?4)",
        rusqlite::params![
            id,
            Format::EpubFixed.token(),
            Format::EpubReflow.token(),
            Format::Pdf.token(),
        ],
        |r| r.get::<_, i64>(0),
    )
    .map(|n| usize::try_from(n).unwrap_or(usize::MAX))
    .map_err(|e| format!("counting the books of library {id}: {e}"))
}
```

- [ ] **Step 4: Run them to verify they pass**

Run: `cargo test -p taiyaku-library libraries:: -- --test-threads=8`
Expected: PASS.

- [ ] **Step 5: Watch each fail against its defect**

One at a time, restoring between:

1. `rename`: replace the `ConstraintViolation` arm's `RenameError::Unusable`
   with `RenameError::Failed`. Expect
   `a_rename_is_refused_as_the_callers_mistake_or_as_no_such_library` to fail.
2. `rename`: replace `let name = trim_name(name);` with `let name = name;`.
   Expect `a_rename_stores_the_name_stripped_and_answers_it` to fail.
3. `books`: write `WHERE (r.library_id = ?1 OR 1 = 1) AND …`. Expect
   `books_counts_the_librarys_own_epub_and_pdf_entries` to fail with 4 for 3.

- [ ] **Step 6: Write the failing tests for `scan.rs`**

Append to `crates/taiyaku-library/src/scan/tests.rs`, beside the other
`a_folder_…` tests. `write_archive` and `library_of` are that file's own.

```rust
fn two_folders(parent: &Path) -> (PathBuf, PathBuf) {
    let (a, b) = (parent.join("Volumes"), parent.join("Chapters"));
    for dir in [&a, &b] {
        std::fs::create_dir_all(dir).expect("mkdir");
    }
    (a, b)
}

#[test]
fn a_library_is_made_with_each_of_its_folders() {
    let parent = crate::tests::tempdir("create-library");
    let (a, b) = two_folders(parent.path());
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();

    let made = create_library(&conn, " Manga ", &[a.clone(), b.clone()]).expect("creates");

    assert_eq!(made.name, "Manga", "the name as stored");
    assert_eq!(
        crate::libraries::name_of(&conn, made.library_id).expect("reads").as_deref(),
        Some("Manga")
    );
    let held: Vec<(PathBuf, i64)> = roots(&conn)
        .expect("lists")
        .into_iter()
        .map(|r| (r.path, r.library_id))
        .collect();
    assert_eq!(
        held,
        vec![
            (a.canonicalize().expect("canonical"), made.library_id),
            (b.canonicalize().expect("canonical"), made.library_id),
        ]
    );
    assert_eq!(made.folders.len(), 2);
    for added in &made.folders {
        assert_eq!(
            added.root.scan_interval_secs,
            Some(added.interval.as_secs() as i64),
            "each folder was probed and its interval recorded"
        );
    }
}

fn libraries_and_roots(conn: &Connection) -> (i64, i64) {
    let count = |table: &str| -> i64 {
        conn.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
            .expect("counts")
    };
    (count("library"), count("root"))
}

#[test]
fn a_library_is_refused_whole_for_one_bad_folder() {
    let parent = crate::tests::tempdir("create-library-refused");
    let (a, b) = two_folders(parent.path());
    let inside = a.join("inside");
    std::fs::create_dir_all(&inside).expect("mkdir");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();

    for (paths, says) in [
        (vec![a.clone(), parent.path().join("missing")], "missing"),
        (vec![a.clone(), inside.clone()], "overlap"),
        (vec![a.clone(), a.clone()], "overlap"),
        (Vec::new(), "at least one folder"),
    ] {
        let why = create_library(&conn, "Manga", &paths).expect_err("refuses");
        assert!(
            matches!(&why, AddRootError::Unusable(text) if text.contains(says)),
            "{why:?}"
        );
        assert_eq!(libraries_and_roots(&conn), (0, 0), "nothing was made: {paths:?}");
    }
    assert!(matches!(
        create_library(&conn, "  ", &[b.clone()]),
        Err(AddRootError::Unusable(why)) if why.contains("needs a name")
    ));
    assert_eq!(libraries_and_roots(&conn), (0, 0));
}

/// The second folder's insert is refused by a trigger planted for it, after
/// both paths were vetted. The library and the first folder must go with it.
#[test]
fn a_library_whose_second_folder_fails_at_the_insert_leaves_nothing() {
    let parent = crate::tests::tempdir("create-library-insert");
    let (a, b) = two_folders(parent.path());
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    conn.execute_batch(
        "CREATE TEMP TRIGGER refuse_a_second_root BEFORE INSERT ON root
         WHEN (SELECT count(*) FROM root) >= 1
         BEGIN SELECT RAISE(ABORT, 'planted'); END;",
    )
    .expect("plants the refusal");

    let why = create_library(&conn, "Manga", &[a, b]).expect_err("refuses");

    assert!(
        matches!(&why, AddRootError::Failed(text) if text.contains("planted")),
        "{why:?}"
    );
    assert_eq!(libraries_and_roots(&conn), (0, 0));
}

#[test]
fn a_library_is_not_made_under_a_name_already_taken() {
    let parent = crate::tests::tempdir("create-library-taken");
    let (a, b) = two_folders(parent.path());
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    add_root_in(&conn, &a, InLibrary::Named("Manga")).expect("Manga exists");

    for name in ["Manga", " Manga "] {
        let why = create_library(&conn, name, &[b.clone()]).expect_err("refuses");
        assert!(
            matches!(&why, AddRootError::Unusable(text) if text.contains("already called `Manga`")),
            "{why:?}"
        );
    }
    assert_eq!(libraries_and_roots(&conn), (1, 1));
}

#[test]
fn a_folder_joins_a_library_by_its_id_and_no_library_by_an_id_none_has() {
    let parent = crate::tests::tempdir("add-by-id");
    let (a, b) = two_folders(parent.path());
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let first = add_root_in(&conn, &a, InLibrary::Named("Manga")).expect("Manga exists");

    let why = add_root_checked_in(&conn, &b, InLibrary::Id(first.library_id + 1))
        .expect_err("no library has that id");
    assert!(
        matches!(&why, AddRootError::Unusable(text) if text.contains("there is no library")),
        "{why:?}"
    );
    assert_eq!(libraries_and_roots(&conn), (1, 1));

    let added = add_root_checked_in(&conn, &b, InLibrary::Id(first.library_id)).expect("joins");
    assert_eq!(added.root.library_id, first.library_id);
    assert_eq!(libraries_and_roots(&conn), (1, 2));
}
```

- [ ] **Step 7: Run them to verify they fail**

Run: `cargo test -p taiyaku-library scan::tests::a_library -- --test-threads=8`
Expected: FAIL to compile, `cannot find function `create_library``.

- [ ] **Step 8: Implement `InLibrary::Id` and `create_library`**

In `crates/taiyaku-library/src/scan.rs`:

Add the variant to `InLibrary`:

```rust
    /// The library with this id. [`add_root_checked_in`] refuses an id no
    /// library holds; [`add_root_in`] validates nothing, here as elsewhere.
    Id(i64),
```

and its arm to `add_root_in`'s `match library`:

```rust
        InLibrary::Id(id) => Some(id),
```

Cut `add_root_checked_in`'s first two validations and its probe into two
functions, moving the existing lines unchanged, and call them from it. Its doc
comment's numbered list stays on `add_root_checked_in`.

```rust
/// `path` in canonical form, having checked that it exists, that this
/// process can list it, and that it is neither inside nor around a root
/// already held: [`add_root_checked_in`]'s first two validations.
fn vetted(conn: &Connection, path: &Path) -> Result<PathBuf, AddRootError> {
    let canonical = path
        .canonicalize()
        .map_err(|e| AddRootError::Unusable(format!("{}: {e}", path.display())))?;

    std::fs::read_dir(&canonical).map_err(|e| {
        AddRootError::Unusable(format!("{}: cannot be read: {e}", canonical.display()))
    })?;

    for existing in roots(conn).map_err(AddRootError::Failed)? {
        if canonical.starts_with(&existing.path) {
            return Err(AddRootError::Unusable(format!(
                "{} is inside existing root {} (id {})",
                canonical.display(),
                existing.path.display(),
                existing.id
            )));
        }
        if existing.path.starts_with(&canonical) {
            return Err(AddRootError::Unusable(format!(
                "{} contains existing root {} (id {})",
                canonical.display(),
                existing.path.display(),
                existing.id
            )));
        }
    }
    Ok(canonical)
}

/// [`add_root_checked_in`]'s third validation, for a root just inserted: the
/// watch probe, and the interval it implies written to the row.
fn probed(conn: &Connection, root: Root) -> Result<Added, AddRootError> {
    let probe = crate::watch::probe(&root.path, PROBE_TIMEOUT);
    let interval = crate::watch::interval_for(&probe);
    let interval_secs = interval.as_secs() as i64;
    update_root(
        conn,
        root.id,
        &RootPatch {
            scan_interval_secs: Some(Some(interval_secs)),
            ..Default::default()
        },
    )
    .map_err(AddRootError::Failed)?;

    Ok(Added {
        root: Root {
            scan_interval_secs: Some(interval_secs),
            ..root
        },
        probe,
        interval,
    })
}
```

`add_root_checked_in`'s body becomes:

```rust
    let canonical = vetted(conn, path)?;

    match library {
        // Any whitespace, not only spaces: refusing a name that would be
        // invisible is not stripping one, so lookups keep `trim_name`'s set.
        InLibrary::Named(name) if name.trim().is_empty() => {
            return Err(AddRootError::Unusable(format!(
                "{}: a library needs a name",
                canonical.display()
            )));
        }
        InLibrary::Named(_) => {}
        InLibrary::Own => {
            if let Some(why) = own_library_clash(conn, &canonical).map_err(AddRootError::Failed)? {
                return Err(AddRootError::Unusable(why));
            }
        }
        InLibrary::Id(id) => {
            if crate::libraries::name_of(conn, id)
                .map_err(AddRootError::Failed)?
                .is_none()
            {
                return Err(AddRootError::Unusable(format!(
                    "{}: there is no library {id}",
                    canonical.display()
                )));
            }
        }
    }

    let root = add_root_in(conn, &canonical, library).map_err(AddRootError::Failed)?;
    probed(conn, root)
```

Then, after `add_root_checked_in`:

```rust
/// What [`create_library`] made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Created {
    pub library_id: i64,
    /// The name as stored.
    pub name: String,
    /// Each folder with its watch probe, in the order given.
    pub folders: Vec<Added>,
}

/// A new library called `name` holding each of `paths`: every folder, or no
/// library and no folder.
///
/// Every path is vetted before anything is written, against the roots held
/// and against the other paths given. The library and its folders are then
/// inserted in one transaction. The watch probes run after it commits,
/// because each waits up to [`PROBE_TIMEOUT`] and a write transaction held
/// that long would stall every other writer.
///
/// # Errors
///
/// Returns [`AddRootError::Unusable`] for no path, a name that is empty once
/// trimmed or that a library already holds, a path [`add_root_checked_in`]
/// would refuse, or two of the paths overlapping. Returns
/// [`AddRootError::Failed`] if the store refuses; a fault while recording a
/// probe's interval is one, and by then the library exists.
pub fn create_library(
    conn: &Connection,
    name: &str,
    paths: &[PathBuf],
) -> Result<Created, AddRootError> {
    if paths.is_empty() {
        return Err(AddRootError::Unusable(
            "a library needs at least one folder".to_string(),
        ));
    }
    if name.trim().is_empty() {
        return Err(AddRootError::Unusable("a library needs a name".to_string()));
    }
    let name = crate::libraries::trim_name(name);
    if crate::libraries::id_of(conn, name)
        .map_err(AddRootError::Failed)?
        .is_some()
    {
        return Err(AddRootError::Unusable(format!(
            "a library is already called `{name}`; add the folder to it instead"
        )));
    }

    let mut canonical: Vec<PathBuf> = Vec::with_capacity(paths.len());
    for path in paths {
        let vetted = vetted(conn, path)?;
        if let Some(earlier) = canonical
            .iter()
            .find(|earlier| vetted.starts_with(earlier) || earlier.starts_with(&vetted))
        {
            return Err(AddRootError::Unusable(format!(
                "{} and {} overlap; a library's folders may not be the same folder or sit \
                 inside one another",
                earlier.display(),
                vetted.display()
            )));
        }
        canonical.push(vetted);
    }

    let tx = crate::Nested::open(conn, "create_library")
        .map_err(|e| AddRootError::Failed(format!("starting to create `{name}`: {e}")))?;
    let library_id = crate::libraries::create(&tx, name).map_err(AddRootError::Failed)?;
    let mut inserted = Vec::with_capacity(canonical.len());
    for path in &canonical {
        inserted
            .push(add_root_in(&tx, path, InLibrary::Id(library_id)).map_err(AddRootError::Failed)?);
    }
    tx.commit()
        .map_err(|e| AddRootError::Failed(format!("committing the library `{name}`: {e}")))?;

    let mut folders = Vec::with_capacity(inserted.len());
    for root in inserted {
        folders.push(probed(conn, root)?);
    }
    Ok(Created {
        library_id,
        name: name.to_string(),
        folders,
    })
}
```

- [ ] **Step 9: Run them to verify they pass**

Run: `cargo test -p taiyaku-library scan:: -- --test-threads=8`
Expected: PASS, the existing `add_root_checked` tests among them: the cut
moved lines and changed none.

- [ ] **Step 10: Watch each fail against its defect**

One at a time, restoring between:

1. `create_library`: delete the `if let Some(earlier) = … { return Err(…) }`
   block. Expect `a_library_is_refused_whole_for_one_bad_folder` to fail at
   the `overlap` case.
2. `create_library`: commit `tx` right after `libraries::create`, insert the
   folders on `conn`, and drop the commit after them. Expect
   `a_library_whose_second_folder_fails_at_the_insert_leaves_nothing` to fail
   with `(1, 1)` for `(0, 0)`. (Passing `conn` where `&tx` is passed proves
   nothing: `Nested` opens its transaction on that same connection, so a
   write through either rolls back with it.)
3. `create_library`: delete the `id_of` check. Expect
   `a_library_is_not_made_under_a_name_already_taken` to fail.
4. `add_root_checked_in`: delete the body of the `InLibrary::Id(id)` arm,
   leaving `InLibrary::Id(_) => {}`. Expect
   `a_folder_joins_a_library_by_its_id_and_no_library_by_an_id_none_has` to
   fail.

- [ ] **Step 11: Whole crates, then commit**

Run: `cargo test -p taiyaku-library -p taiyaku-serve -p taiyaku-cli -- --test-threads=8`
Run: `cargo clippy --workspace --all-targets --all-features -- -D warnings`
Run: `cargo fmt --all -- --check`
Expected: all pass. If clippy names a `match` on `InLibrary` that this task
did not touch, add the `Id` arm there and say so in the report.

```bash
git add crates/taiyaku-library/src/libraries.rs crates/taiyaku-library/src/libraries/tests.rs \
        crates/taiyaku-library/src/scan.rs crates/taiyaku-library/src/scan/tests.rs
git commit -m "A library made with its folders, a folder added by library id, and a rename"
```

---

## Task 2: The store removes a library, with one count

**Files:**
- Modify: `crates/taiyaku-library/src/scan.rs` (the removal section, near
  lines 543–901)
- Test: `crates/taiyaku-library/src/scan/tests.rs`

**Interfaces:**
- Consumes: `scan::{Removal, removal_refusal, removal_cost, remove_root}`,
  `DOOMED_WORKS`, `crate::Nested`, `derive::{derive, derive_with, Documents,
  DeriveReport}`.
- Produces:
  ```rust
  pub fn library_removal_refusal(conn: &Connection, library_id: i64)
      -> Result<Option<String>, String>;
  pub fn library_removal_cost(conn: &Connection, library_id: i64)
      -> Result<(Removal, crate::derive::DeriveReport), String>;
  pub fn remove_library(conn: &Connection, library_id: i64)
      -> Result<(Removal, crate::derive::DeriveReport), String>;
  ```
  A `library_id` naming no library removes nothing and answers a zero
  `Removal`; the route asks whether the library exists first.

- [ ] **Step 1: Write the failing tests**

Append to `crates/taiyaku-library/src/scan/tests.rs`, after
`a_removal_that_may_go_ahead_has_no_refusal`. `three_libraries`,
`an_account_granted` and `what_a_removal_touches` are that file's own.

```rust
fn library_called(conn: &Connection, name: &str) -> i64 {
    crate::libraries::id_of(conn, name)
        .expect("reads")
        .expect("the library exists")
}

#[test]
fn removing_a_library_takes_each_folders_works_and_the_library() {
    let parent = crate::tests::tempdir("remove-library");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    three_libraries(parent.path(), &conn);
    let shelf = library_called(&conn, "Shelf");

    let (removal, _derived) = remove_library(&conn, shelf).expect("removes");

    assert_eq!(removal.works, 2, "one work under each of Shelf's folders");
    assert_eq!(crate::libraries::name_of(&conn, shelf).expect("reads"), None);
    let left: Vec<String> = conn
        .prepare("SELECT l.name FROM work w JOIN library l ON l.id = w.library_id ORDER BY 1")
        .expect("prepares")
        .query_map([], |r| r.get(0))
        .expect("reads")
        .collect::<rusqlite::Result<_>>()
        .expect("reads");
    assert_eq!(left, vec!["Lone", "Solo"], "the other libraries' works stand");
    assert_eq!(
        conn.query_row("SELECT count(*) FROM root", [], |r| r.get::<_, i64>(0))
            .expect("counts"),
        2
    );
}

/// One work with an edition under each folder of one library, and a position
/// on it. Neither folder alone accounts for the work, so each folder's own
/// dry run counts none; the library's counts it once.
#[test]
fn a_library_removal_counts_a_work_that_spans_its_folders_once() {
    let parent = crate::tests::tempdir("remove-library-spanning");
    let (hub, other) = (parent.path().join("hub"), parent.path().join("other"));
    write_archive(&hub.join("Series").join("Series - v01.cbz"), 2);
    write_archive(&other.join("Series").join("Series - v01.cbz"), 3);
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let made = create_library(&conn, "Manga", &[hub, other]).expect("creates");
    update_root(
        &conn,
        made.folders[0].root.id,
        &RootPatch {
            edition_key: Some(Some("en".to_string())),
            ..Default::default()
        },
    )
    .expect("the two folders give two editions of the one volume");
    for root in roots(&conn).expect("lists") {
        scan_root(&conn, &root).expect("scans");
    }
    crate::derive::derive(&conn).expect("derives");
    let works: Vec<i64> = conn
        .prepare("SELECT id FROM work")
        .expect("prepares")
        .query_map([], |r| r.get(0))
        .expect("reads")
        .collect::<rusqlite::Result<_>>()
        .expect("reads");
    assert_eq!(works.len(), 1, "the fixture: one work across the two folders");
    conn.execute(
        "INSERT INTO progress (user_id, work_id, edition, slot) VALUES (1, ?1, 'en', 1)",
        [works[0]],
    )
    .expect("a position");
    for folder in &made.folders {
        assert_eq!(
            removal_cost(&conn, folder.root.id).expect("costs").0.works,
            0,
            "the fixture: neither folder alone accounts for the work"
        );
    }

    let (cost, _derived) = library_removal_cost(&conn, made.library_id).expect("costs");

    assert_eq!((cost.works, cost.positions), (1, 1));
}

#[test]
fn the_cost_of_removing_a_library_leaves_the_database_as_it_found_it() {
    let parent = crate::tests::tempdir("remove-library-cost");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    three_libraries(parent.path(), &conn);
    let before = what_a_removal_touches(&conn);

    let (cost, _derived) =
        library_removal_cost(&conn, library_called(&conn, "Shelf")).expect("costs");

    assert_eq!(cost.works, 2);
    assert_eq!(what_a_removal_touches(&conn), before);
}

/// `Shelf` has two folders, so no single folder of it is "the last folder of
/// a library" and the per-folder refusal is never asked. The library's own is.
#[test]
fn removing_a_library_that_is_an_accounts_only_grant_is_refused_by_name() {
    let parent = crate::tests::tempdir("remove-library-refused");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    three_libraries(parent.path(), &conn);
    an_account_granted(&conn, "bo", "user", &["Shelf"]);
    an_account_granted(&conn, "alex", "user", &["Shelf"]);
    an_account_granted(&conn, "both", "user", &["Shelf", "Solo"]);
    let shelf = library_called(&conn, "Shelf");
    let before = what_a_removal_touches(&conn);

    let refusal = library_removal_refusal(&conn, shelf)
        .expect("reads")
        .expect("refuses");
    assert!(
        refusal.contains("`Shelf`") && refusal.contains("`alex` and `bo`"),
        "{refusal}"
    );
    assert!(!refusal.contains("`both`"), "{refusal}");

    for attempt in [library_removal_cost(&conn, shelf), remove_library(&conn, shelf)] {
        assert_eq!(attempt.expect_err("refuses"), refusal);
    }
    assert_eq!(what_a_removal_touches(&conn), before);
    assert_eq!(
        library_removal_refusal(&conn, library_called(&conn, "Lone")).expect("reads"),
        None
    );
}
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p taiyaku-library scan::tests -- --test-threads=8`
Expected: FAIL to compile, `cannot find function `remove_library``.

- [ ] **Step 3: Implement**

In `crates/taiyaku-library/src/scan.rs`.

Cut the counting and deleting out of `apply_removal` into `take_root`, moving
the lines unchanged. `apply_removal` keeps its doc comment and becomes the
refusal, `take_root`, and the derive:

```rust
/// Count what root `id` alone accounts for, then delete it: its works, the
/// members it supplied to works that survive it, and the root. No refusal is
/// asked and nothing is derived; [`apply_removal`] and
/// [`apply_library_removal`] do both around it.
fn take_root(conn: &Connection, id: i64) -> Result<Removal, String> {
    let works: i64 = conn
        .query_row(
            &format!("SELECT COUNT(*) FROM ({DOOMED_WORKS})"),
            [id],
            |r| r.get(0),
        )
        .map_err(|e| format!("counting the works root {id} alone accounts for: {e}"))?;
    let positions: i64 = conn
        .query_row(
            &format!("SELECT COUNT(*) FROM progress WHERE work_id IN ({DOOMED_WORKS})"),
            [id],
            |r| r.get(0),
        )
        .map_err(|e| format!("counting the reading positions root {id} would take: {e}"))?;
    let notes: i64 = conn
        .query_row(
            &format!("SELECT COUNT(*) FROM note WHERE work_id IN ({DOOMED_WORKS})"),
            [id],
            |r| r.get(0),
        )
        .map_err(|e| format!("counting the notes root {id} would take: {e}"))?;

    conn.execute(
        &format!("DELETE FROM work WHERE id IN ({DOOMED_WORKS})"),
        [id],
    )
    .map_err(|e| format!("deleting the works root {id} alone accounts for: {e}"))?;
    conn.execute("DELETE FROM member WHERE root_id = ?1", [id])
        .map_err(|e| {
            format!("clearing what root {id} alone still held on a work that survives it: {e}")
        })?;
    conn.execute("DELETE FROM root WHERE id = ?1", [id])
        .map_err(|e| format!("removing root {id}: {e}"))?;

    Ok(Removal {
        works: usize::try_from(works).unwrap_or(usize::MAX),
        positions: usize::try_from(positions).unwrap_or(usize::MAX),
        notes: usize::try_from(notes).unwrap_or(usize::MAX),
    })
}
```

```rust
fn apply_removal(
    conn: &Connection,
    id: i64,
) -> Result<(Removal, crate::derive::DeriveReport), String> {
    if let Some(refusal) = removal_refusal(conn, id)? {
        return Err(refusal);
    }
    let removal = take_root(conn, id)?;
    let derived = crate::derive::derive_with(conn, crate::derive::Documents::Skip)?;
    Ok((removal, derived))
}
```

Cut the two sentences `removal_refusal` builds from its account names into a
function both refusals call. `removal_refusal`'s `(accounts, advice)` becomes
`only_grant_of(&names)` over the account names it read, and its message is
unchanged:

```rust
/// "`a` is granted" or "`a`, `b` and `c` are granted", with the advice that
/// goes with one account or several. `names` is not empty.
fn only_grant_of(names: &[String]) -> (String, &'static str) {
    let (last, others) = names.split_last().expect("at least one account");
    if others.is_empty() {
        (
            format!("`{last}` is granted"),
            "Removing it would delete that grant, and an account with no grant sees every \
             library. First grant it another library, or revoke this one",
        )
    } else {
        let others: Vec<String> = others.iter().map(|account| format!("`{account}`")).collect();
        (
            format!("{} and `{last}` are granted", others.join(", ")),
            "Removing it would delete those grants, and an account with no grant sees every \
             library. First grant each another library, or revoke this one",
        )
    }
}
```

Then, after `remove_root`:

```rust
/// Why library `library_id` may not be removed, if it may not: it is the only
/// library some account is granted.
///
/// [`removal_refusal`]'s rule, asked of a whole library. A library of two
/// folders is never refused by that one, which speaks only for a library's
/// last folder, so this is asked before the first folder goes.
///
/// # Errors
///
/// Returns an error if the read fails. An id naming no library is `Ok(None)`.
pub fn library_removal_refusal(
    conn: &Connection,
    library_id: i64,
) -> Result<Option<String>, String> {
    let mut statement = conn
        .prepare(
            "SELECT l.name, u.name
               FROM library l
               JOIN user_library ul ON ul.library_id = l.id
               JOIN user u ON u.id = ul.user_id
              WHERE l.id = ?1
                AND NOT EXISTS (SELECT 1 FROM user_library held
                                 WHERE held.user_id = ul.user_id
                                   AND held.library_id <> ul.library_id)
              ORDER BY u.name",
        )
        .map_err(|e| format!("checking whose only grant library {library_id} is: {e}"))?;
    let restricted = statement
        .query_map([library_id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })
        .map_err(|e| format!("checking whose only grant library {library_id} is: {e}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("checking whose only grant library {library_id} is: {e}"))?;
    let Some((library, _)) = restricted.first() else {
        return Ok(None);
    };
    let names: Vec<String> = restricted.iter().map(|(_, account)| account.clone()).collect();
    let (accounts, advice) = only_grant_of(&names);
    Ok(Some(format!(
        "`{library}` is the only library {accounts}. {advice}"
    )))
}

/// Remove each folder of `library_id` in turn, summing what each takes, then
/// derive once.
///
/// **One folder after another inside the caller's transaction, never one dry
/// run per folder.** [`DOOMED_WORKS`] names a work for a folder only when
/// every member of it is under that folder. A work with an edition under each
/// of two folders is therefore named for neither while both stand, and for
/// the second once [`take_root`] has deleted the first's members.
fn apply_library_removal(
    conn: &Connection,
    library_id: i64,
) -> Result<(Removal, crate::derive::DeriveReport), String> {
    if let Some(refusal) = library_removal_refusal(conn, library_id)? {
        return Err(refusal);
    }
    let folders: Vec<i64> = conn
        .prepare("SELECT id FROM root WHERE library_id = ?1 ORDER BY id")
        .map_err(|e| format!("listing the folders of library {library_id}: {e}"))?
        .query_map([library_id], |r| r.get(0))
        .map_err(|e| format!("listing the folders of library {library_id}: {e}"))?
        .collect::<Result<_, _>>()
        .map_err(|e| format!("listing the folders of library {library_id}: {e}"))?;
    let mut total = Removal::default();
    for folder in folders {
        let taken = take_root(conn, folder)?;
        total.works += taken.works;
        total.positions += taken.positions;
        total.notes += taken.notes;
    }
    let derived = crate::derive::derive_with(conn, crate::derive::Documents::Skip)?;
    Ok((total, derived))
}

/// What removing library `library_id` would cost, found by doing it inside a
/// transaction this rolls back: [`removal_cost`], for a library.
///
/// # Errors
///
/// As [`removal_cost`]. A removal [`library_removal_refusal`] refuses is an
/// error in the refusal's words.
pub fn library_removal_cost(
    conn: &Connection,
    library_id: i64,
) -> Result<(Removal, crate::derive::DeriveReport), String> {
    let tx = crate::Nested::open(conn, "library_removal_cost")
        .map_err(|e| format!("starting the library-removal-cost transaction: {e}"))?;
    let removal = apply_library_removal(&tx, library_id)?;
    // Not committed: dropping `tx` is the rollback, as in `removal_cost`.
    drop(tx);
    Ok(removal)
}

/// Remove library `library_id`: each of its folders, the works they alone
/// accounted for, and the library, which migration 15 deletes with its last
/// folder. Then derive again with documents reconciled, after the commit, for
/// [`remove_root`]'s reason.
///
/// # Errors
///
/// As [`remove_root`]. A removal [`library_removal_refusal`] refuses is an
/// error in the refusal's words, and nothing is removed.
pub fn remove_library(
    conn: &Connection,
    library_id: i64,
) -> Result<(Removal, crate::derive::DeriveReport), String> {
    let tx = crate::Nested::open(conn, "remove_library")
        .map_err(|e| format!("starting the library-removal transaction: {e}"))?;
    let (removal, _predicted) = apply_library_removal(&tx, library_id)?;
    tx.commit()
        .map_err(|e| format!("committing the removal of library {library_id}: {e}"))?;
    let derived = crate::derive::derive(conn)
        .map_err(|e| format!("reconciling documents after removing library {library_id}: {e}"))?;
    Ok((removal, derived))
}
```

- [ ] **Step 4: Run them to verify they pass**

Run: `cargo test -p taiyaku-library scan:: -- --test-threads=8`
Expected: PASS, every existing removal test among them.

- [ ] **Step 5: Watch each fail against its defect**

One at a time, restoring between:

1. `apply_library_removal`: replace the loop's `take_root(conn, folder)?` with
   `removal_cost(conn, folder)?.0`. Expect
   `a_library_removal_counts_a_work_that_spans_its_folders_once` to fail with
   `(0, 0)` for `(1, 1)`.
2. `library_removal_cost`: replace `drop(tx);` with
   `tx.commit().map_err(|e| e.to_string())?;`. Expect
   `the_cost_of_removing_a_library_leaves_the_database_as_it_found_it` to fail.
3. `apply_library_removal`: delete the `library_removal_refusal` check. Expect
   `removing_a_library_that_is_an_accounts_only_grant_is_refused_by_name` to
   fail: the error is then migration 15's trigger's sentence, which names no
   account.
4. `library_removal_refusal`: write
   `AND (held.library_id <> ul.library_id OR 1 = 1)`. Expect the same test to
   fail at `expect("refuses")`.

- [ ] **Step 6: Whole crates, then commit**

Run the three commands of Task 1's Step 11. Expected: all pass.

```bash
git add crates/taiyaku-library/src/scan.rs crates/taiyaku-library/src/scan/tests.rs
git commit -m "A library removed whole, counted once and refused by name where it is an only grant"
```

---

## Task 3: Grants are written by library id

**Files:**
- Modify: `crates/taiyaku-library/src/account.rs` (`Patch`, `update`,
  `granted_roots`; near lines 228–251, 756–766 and 954–1012)
- Modify: `crates/taiyaku-serve/src/users_api.rs` (`user_json`, `assembled`,
  `user_patch_from_json`)
- Test: `crates/taiyaku-library/src/account/tests.rs`,
  `crates/taiyaku-serve/src/users_api/tests.rs`

**Interfaces:**
- Consumes: `account::{libraries_for, grant_library, revoke_library,
  granted_libraries}`.
- Produces:
  ```rust
  pub struct Patch {
      pub name: Option<String>,
      pub role: Option<String>,
      pub can_download: Option<bool>,
      pub can_stream: Option<bool>,
      pub can_stamp: Option<bool>,
      /// Library ids. Replaces the account's grants.
      pub libraries: Option<Vec<i64>>,
      pub password: Option<String>,
  }
  ```
  `PATCH /api/library/users/{id}` reads `"libraries": [id, …]` and answers
  400 for a body carrying `"roots"`. Every account in `GET
  /api/library/users` and in a patch's answer carries
  `"libraries": [{"id": 70, "name": "Shelf"}, …]`, by name, and no `"roots"`.

- [ ] **Step 1: Rewrite the store's grant tests to send libraries**

In `crates/taiyaku-library/src/account/tests.rs`, each test that builds a
`Patch` with `roots:` sends `libraries:` instead. The fixtures stay:
`a_two_folder_library_and_another` files roots 7 and 8 under library 70 and
root 9 under 80; `two_accounts_granted_libraries` uses libraries 60, 70 and
80; `two_roots` makes roots 7 and 8, each a library of its own, read with
`library_of`.

Replace `a_roots_list_naming_one_folder_of_a_library_keeps_the_whole_library`
with:

```rust
/// A list names libraries. `7` is a folder of library 70 and no library's id,
/// so a writer that still read the list as folders would grant 70 for it.
#[test]
fn a_libraries_list_names_libraries_and_never_a_folder() {
    let library = claimed_library();
    let conn = library.conn();
    a_two_folder_library_and_another(&conn);
    let id = add(&conn, "reader", "pw", "user").expect("adds an account");

    update(
        &conn,
        id,
        &Patch {
            libraries: Some(vec![70]),
            ..Patch::default()
        },
        None,
    )
    .expect("updates");
    assert_eq!(libraries_for(&conn, id).expect("reads"), vec![70]);
    assert_eq!(
        roots_for(&conn, id).expect("reads"),
        vec![7, 8],
        "the library, with both its folders"
    );

    let why = update(
        &conn,
        id,
        &Patch {
            libraries: Some(vec![7]),
            ..Patch::default()
        },
        None,
    )
    .expect_err("there is no library 7");
    assert!(
        matches!(&why, WriteError::Unusable(text) if text.contains("there is no library 7")),
        "{why:?}"
    );
    assert_eq!(
        libraries_for(&conn, id).expect("reads"),
        vec![70],
        "the refusal wrote nothing"
    );

    update(
        &conn,
        id,
        &Patch {
            libraries: Some(vec![80]),
            ..Patch::default()
        },
        None,
    )
    .expect("updates");
    assert_eq!(
        libraries_for(&conn, id).expect("reads"),
        vec![80],
        "replaced, not appended"
    );
}
```

In the others, change only the field and the ids, and each name's `roots` to
`libraries`:

| Test (new name) | Was | Becomes |
| --- | --- | --- |
| `a_libraries_list_replaces_the_grants_rather_than_adding_to_them` | `roots: Some(vec![second])` | `libraries: Some(vec![library_of(&conn, second)])` |
| `a_libraries_list_across_several_libraries_revokes_and_grants_each` | `roots: Some(vec![7])`, then `Some(vec![6, 9])` | `libraries: Some(vec![70])`, then `Some(vec![60, 80])` |
| `a_patch_without_a_libraries_key_leaves_the_grants_alone` | no `roots` key | unchanged body |
| `an_empty_libraries_list_makes_an_account_unrestricted_rather_than_blind` | `roots: Some(Vec::new())` | `libraries: Some(Vec::new())` |
| `a_refused_role_change_writes_no_grants_either` | `roots: Some(vec![second])` | `libraries: Some(vec![library_of(&conn, second)])` |

Delete `granted_roots_names_the_paths_for_display`. In
`a_granted_library_resolves_to_every_one_of_its_folders` and
`granted_folders_are_read_per_account_and_in_order`, delete each
`granted_roots` assertion and keep the rest; if either test then asserts
nothing its name says, delete it and name it in the report.

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p taiyaku-library account:: -- --test-threads=8`
Expected: FAIL to compile, `struct `Patch` has no field named `libraries``.

- [ ] **Step 3: Implement in `account.rs`**

In `Patch`, replace the `roots` field and its comment:

```rust
    /// Library ids. Replaces the account's grants: `Some(vec![])` takes them
    /// all, which is unrestricted, and `None` leaves them alone.
    pub libraries: Option<Vec<i64>>,
```

In `update`, replace the whole `if let Some(wanted) = &patch.roots { … }`
block. Keep the two comment paragraphs above it ("Replace-all, diffed rather
than deleted and reinserted" and "`Some(vec![])` is not the same instruction
as `None`"):

```rust
    if let Some(wanted) = &patch.libraries {
        let mut wanted_libraries = std::collections::BTreeSet::new();
        for library_id in wanted {
            // **An id no library holds is the caller's mistake, not a
            // fault.** `INSERT OR IGNORE` does not suppress a foreign-key
            // violation, so left to `grant_library` this reaches the client
            // as a 500 carrying SQLite's own text.
            let exists: bool = conn
                .query_row(
                    "SELECT EXISTS (SELECT 1 FROM library WHERE id = ?1)",
                    params![library_id],
                    |row| row.get(0),
                )
                .map_err(|e| {
                    WriteError::Failed(format!(
                        "checking whether library {library_id} exists: {e}"
                    ))
                })?;
            if !exists {
                return Err(WriteError::Unusable(format!(
                    "there is no library {library_id} to grant"
                )));
            }
            wanted_libraries.insert(*library_id);
        }
        let held: std::collections::BTreeSet<i64> = libraries_for(conn, id)
            .map_err(WriteError::Failed)?
            .into_iter()
            .collect();
        for library in held.difference(&wanted_libraries) {
            revoke_library(conn, id, *library).map_err(WriteError::Failed)?;
        }
        for library in wanted_libraries.difference(&held) {
            grant_library(conn, id, *library).map_err(WriteError::Failed)?;
        }
    }
```

Delete `granted_roots` and its doc comment. `roots_for` stays: the guard reads
it.

- [ ] **Step 4: Run them to verify they pass**

Run: `cargo test -p taiyaku-library account:: -- --test-threads=8`
Expected: PASS. `taiyaku-serve` does not build yet; Step 6 fixes it.

- [ ] **Step 5: Rewrite the route's grant tests**

In `crates/taiyaku-serve/src/users_api/tests.rs`:

- `the_user_list_carries_sessions_keys_and_grants`: the key it expects and
  reads is `"libraries"`, and it asserts the object has no `"roots"` key
  (`first.get("roots").is_none()`).
- `a_roots_list_in_the_patch_body_writes_the_grants` becomes
  `a_libraries_list_in_the_patch_body_writes_the_grants`: send
  `{"libraries": [library]}`, where `library` is the fixture root's
  `library_id` read from the `root` row, and assert the answer's
  `user.libraries` is `[{"id": library, "name": <that library's name>}]`.
- `a_roots_list_naming_a_root_that_does_not_exist_is_the_callers_mistake`
  becomes
  `a_libraries_list_naming_a_library_that_does_not_exist_is_the_callers_mistake`:
  send `{"libraries": [999]}`, expect 400 and an `error` containing
  `there is no library 999`.
- The test near line 1193 that reads `["roots"]` and patches `{"roots": [1]}`:
  read `["libraries"]`, and patch `{"libraries": [<the fixture root's
  library_id>]}`.

Add:

```rust
/// The screen sent folder ids under `roots` until it sent library ids under
/// `libraries`. The old key is refused by name, never dropped: dropped, the
/// request would answer 200 having changed no grant.
#[tokio::test]
async fn a_roots_key_in_the_patch_body_is_refused_naming_libraries() {
    let (router, state, _fixture) = test_support::router_over_a_single_scanned_work();
    let reader = {
        let conn = state.library.conn();
        taiyaku_library::account::add(&conn, "reader", "pw", "user").expect("adds")
    };

    let response = send_json(
        &router,
        Method::PATCH,
        &format!("/api/library/users/{reader}"),
        &serde_json::json!({ "roots": [1] }),
    )
    .await;

    let status = response.status();
    let text = test_support::body_string(response).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{text}");
    assert!(text.contains("`libraries`"), "{text}");
    let conn = state.library.conn();
    assert!(
        taiyaku_library::account::libraries_for(&conn, reader)
            .expect("reads")
            .is_empty()
    );
}
```

- [ ] **Step 6: Implement in `users_api.rs`**

`user_json` takes `libraries: &[(i64, String)]` where it took `roots`, and
sends:

```rust
        "libraries": libraries.iter().map(|(id, name)| serde_json::json!({
            "id": id,
            "name": name,
        })).collect::<Vec<_>>(),
```

Its doc comment's "its root grants" becomes "the libraries it is granted".
`assembled` reads them:

```rust
    let libraries = account::granted_libraries(conn, account.id)?;
    Ok(user_json(account, can_log_in, &sessions, &keys, &libraries))
```

In `user_patch_from_json`, replace the `roots` block, and its doc comment's
`roots` with `libraries`:

```rust
    if body.get("roots").is_some() {
        return Err(
            "`roots` is no longer accepted: send `libraries`, a list of library ids".to_string(),
        );
    }
    if let Some(v) = body.get("libraries") {
        let listed = v
            .as_array()
            .ok_or_else(|| "`libraries` must be an array of library ids".to_string())?;
        let mut ids = Vec::with_capacity(listed.len());
        for entry in listed {
            ids.push(
                entry
                    .as_i64()
                    .ok_or_else(|| "`libraries` must be an array of library ids".to_string())?,
            );
        }
        patch.libraries = Some(ids);
    }
```

- [ ] **Step 7: Run them to verify they pass**

Run: `cargo test -p taiyaku-serve users_api:: -- --test-threads=8`
Expected: PASS.

- [ ] **Step 8: Watch each fail against its defect**

One at a time, restoring between:

1. `account::update`: replace the `EXISTS` query's SQL with
   `SELECT EXISTS (SELECT 1 FROM root WHERE id = ?1)`. Expect
   `a_libraries_list_names_libraries_and_never_a_folder` to fail: library 70
   is then refused, or 7 accepted.
2. `users_api::user_patch_from_json`: delete the `roots` refusal. Expect
   `a_roots_key_in_the_patch_body_is_refused_naming_libraries` to fail with
   200 for 400.

- [ ] **Step 9: Whole crates, then commit**

Run the three commands of Task 1's Step 11. Expected: all pass. A doc comment
elsewhere that links `account::granted_roots` fails `cargo doc` in the gate:
`command grep -a -rn 'granted_roots' crates docs/*.md` and reword each hit in
`crates/`; leave closed rows of `docs/status.md` as written.

```bash
git add crates/taiyaku-library/src/account.rs crates/taiyaku-library/src/account/tests.rs \
        crates/taiyaku-serve/src/users_api.rs crates/taiyaku-serve/src/users_api/tests.rs
git commit -m "Grants are sent and written by library id; the roots key is refused by name"
```

---

## Task 4: The libraries routes: list, create, rename, add a folder, remove

**Files:**
- Create: `crates/taiyaku-serve/src/libraries_api.rs`,
  `crates/taiyaku-serve/src/libraries_api/tests.rs`
- Modify: `crates/taiyaku-serve/src/config_api.rs` (helpers made
  `pub(crate)`; `roots_with_last_scan` cut out of `list_roots`; `create_root`
  and `root_path_from_json` deleted)
- Modify: `crates/taiyaku-serve/src/config_api/tests.rs` (the add-a-root
  tests move)
- Modify: `crates/taiyaku-serve/src/routes.rs` (`guarded_routes`,
  `grant_postures`), `crates/taiyaku-serve/src/auth/tests.rs` (two lists),
  `crates/taiyaku-serve/src/lib.rs` (`mod libraries_api;`),
  `crates/taiyaku-serve/src/test_support.rs` (`write_pages` becomes
  `pub(crate)`)

**Interfaces:**
- Consumes: Task 1's `libraries::{name_of, rename, RenameError, books}`,
  `scan::{create_library, Created, InLibrary::Id}`; Task 2's
  `scan::{library_removal_refusal, library_removal_cost, remove_library}`;
  `libraries::{list, Listed}`; `session::administering_or_forbidden`.
- Produces, all admin-only:

  | Route | Body | Answers |
  | --- | --- | --- |
  | `GET /api/library/libraries` | | `{"libraries": [Library]}`, by name |
  | `POST /api/library/libraries` | `{"name": "Manga", "folders": ["/p", …]}` | 200 `{"library": Library, "added": [Added]}`; 400 |
  | `PATCH /api/library/libraries/{id}` | `{"name": "…"}` | 200 `{"id", "name"}`; 400; 404 |
  | `DELETE /api/library/libraries/{id}[?dry_run=true]` | | 200 `{"removed": {"works","positions","notes"}, "derive": {…}, "dry_run"}`; 404; 409 |
  | `POST /api/library/libraries/{id}/folders` | `{"path": "/p"}` | 200 `Added`; 400; 404 |

  `Library` is `{"id", "name", "works", "books", "folders": [Root]}`. `Root`
  and `Added` are `config_api::root_json`'s and `added_json`'s shapes,
  unchanged. Every refusal's body is `{"error": "<sentence>"}`.
  `POST /api/library/roots` no longer exists.
- Produces for Task 5, in `config_api.rs`:
  ```rust
  pub(crate) fn roots_with_last_scan(conn: &rusqlite::Connection)
      -> Result<Vec<(scan::Root, Option<String>, Option<String>)>, String>;
  pub(crate) fn finished_scan_json(scan_report: &scan::ScanReport,
      derive_report: &derive::DeriveReport) -> serde_json::Value;
  ```
  and in `libraries_api.rs`, private to it: `LibraryAnswer`,
  `library_panicked`, `folders_of`.

- [ ] **Step 1: Write the failing tests**

Create `crates/taiyaku-serve/src/libraries_api/tests.rs`. Make
`test_support::write_pages` `pub(crate)` for it.

```rust
use crate::test_support::{self, get_json, send_json};
use axum::http::{Method, StatusCode};

/// Sends, and answers the status with the body parsed as JSON.
async fn answered(
    router: &axum::Router,
    method: Method,
    path: &str,
    body: &serde_json::Value,
) -> (StatusCode, serde_json::Value) {
    let response = send_json(router, method, path, body).await;
    let status = response.status();
    let text = test_support::body_string(response).await;
    let json = serde_json::from_str(&text).unwrap_or(serde_json::Value::String(text));
    (status, json)
}

/// A new directory under `fixture` holding one two-page volume of `series`.
fn a_folder_of(fixture: &test_support::Fixture, name: &str, series: &str) -> String {
    let dir = fixture.path().join(name);
    test_support::write_pages(&dir.join(series).join(format!("{series} - v01")), 2);
    dir.to_string_lossy().into_owned()
}

fn the_library<'a>(listed: &'a serde_json::Value, name: &str) -> &'a serde_json::Value {
    listed["libraries"]
        .as_array()
        .expect("a libraries array")
        .iter()
        .find(|l| l["name"] == name)
        .unwrap_or_else(|| panic!("no library called {name}: {listed}"))
}

#[tokio::test]
async fn every_libraries_route_refuses_a_reader() {
    let (router, _state, _fixture) = test_support::router_over_a_single_scanned_work_as_a_reader();
    let body = serde_json::json!({});
    for (method, path) in [
        (Method::GET, "/api/library/libraries"),
        (Method::POST, "/api/library/libraries"),
        (Method::PATCH, "/api/library/libraries/1"),
        (Method::DELETE, "/api/library/libraries/1"),
        (Method::POST, "/api/library/libraries/1/folders"),
    ] {
        let response = send_json(&router, method.clone(), path, &body).await;
        assert_eq!(
            response.status(),
            StatusCode::FORBIDDEN,
            "{method} {path} answered a reader"
        );
    }
}

/// Two libraries of one folder each, so a list that gave every library every
/// folder shows two folders where one is held.
#[tokio::test]
async fn the_list_gives_each_library_its_own_folders_works_and_books() {
    let (router, state, _fixture) = test_support::state_with_two_scanned_roots();
    {
        let conn = state.library.conn();
        conn.execute(
            "INSERT INTO scan_entry (root_id, rel_path, kind, format, size, mtime)
             SELECT id, 'a.pdf', 'file', 'pdf', 1, 1 FROM root ORDER BY id LIMIT 1",
            [],
        )
        .expect("one PDF under the first folder");
    }

    let listed = get_json(&router, "/api/library/libraries").await;

    let names: Vec<&str> = listed["libraries"]
        .as_array()
        .expect("a libraries array")
        .iter()
        .map(|l| l["name"].as_str().expect("a name"))
        .collect();
    assert_eq!(names, vec!["root-a", "root-b"], "by name: {listed}");
    let a = the_library(&listed, "root-a");
    assert_eq!(a["works"], 1, "{listed}");
    assert_eq!(a["books"], 1, "{listed}");
    let folders = a["folders"].as_array().expect("folders");
    assert_eq!(folders.len(), 1, "{listed}");
    assert!(folders[0]["path"].as_str().expect("a path").ends_with("root-a"));
    assert_eq!(folders[0]["last_scan_status"], "ok", "{listed}");
    assert_eq!(the_library(&listed, "root-b")["books"], 0, "{listed}");
}

#[tokio::test]
async fn a_library_is_created_with_its_folders_and_then_listed() {
    let (router, _state, _held) = test_support::router_over_a_single_scanned_work();
    let fixture = test_support::Fixture::new("create-library-route");
    let folders = [
        a_folder_of(&fixture, "Volumes", "Alpha"),
        a_folder_of(&fixture, "Chapters", "Beta"),
    ];

    let (status, made) = answered(
        &router,
        Method::POST,
        "/api/library/libraries",
        &serde_json::json!({ "name": " Manga ", "folders": folders }),
    )
    .await;

    assert_eq!(status, StatusCode::OK, "{made}");
    assert_eq!(made["library"]["name"], "Manga");
    assert_eq!(made["library"]["folders"].as_array().expect("folders").len(), 2);
    let added = made["added"].as_array().expect("added");
    assert_eq!(added.len(), 2, "{made}");
    for one in added {
        assert!(one["watching"].is_boolean(), "{made}");
        assert!(one["interval_secs"].is_number(), "{made}");
    }
    let listed = get_json(&router, "/api/library/libraries").await;
    assert_eq!(the_library(&listed, "Manga")["id"], made["library"]["id"]);
}

#[tokio::test]
async fn a_library_is_refused_whole_and_as_the_callers_mistake() {
    let (router, _state, _held) = test_support::router_over_a_single_scanned_work();
    let fixture = test_support::Fixture::new("create-library-refused");
    let good = a_folder_of(&fixture, "Volumes", "Alpha");
    let before = get_json(&router, "/api/library/libraries").await;

    for (body, says) in [
        (
            serde_json::json!({ "name": "Manga", "folders": [good, "/no/such/taiyaku-test-folder"] }),
            "taiyaku-test-folder",
        ),
        (serde_json::json!({ "name": "Manga", "folders": [] }), "at least one folder"),
        (serde_json::json!({ "name": "Manga" }), "`folders`"),
        (serde_json::json!({ "folders": [good] }), "`name`"),
        (serde_json::json!({ "name": "Manga", "folders": [7] }), "`folders`"),
    ] {
        let (status, refused) =
            answered(&router, Method::POST, "/api/library/libraries", &body).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{body}: {refused}");
        assert!(
            refused["error"].as_str().expect("an error").contains(says),
            "{body}: {refused}"
        );
    }
    assert_eq!(get_json(&router, "/api/library/libraries").await, before);

    let taken = before["libraries"][0]["name"].as_str().expect("a name");
    let (status, refused) = answered(
        &router,
        Method::POST,
        "/api/library/libraries",
        &serde_json::json!({ "name": taken, "folders": [good] }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{refused}");
    assert!(refused["error"].as_str().expect("an error").contains("already called"));
}

#[tokio::test]
async fn a_library_is_renamed_by_its_id() {
    let (router, _state, _fixture) = test_support::state_with_two_scanned_roots();
    let listed = get_json(&router, "/api/library/libraries").await;
    let id = the_library(&listed, "root-a")["id"].as_i64().expect("an id");
    let path = format!("/api/library/libraries/{id}");

    let (status, renamed) =
        answered(&router, Method::PATCH, &path, &serde_json::json!({ "name": " Manga " })).await;
    assert_eq!(status, StatusCode::OK, "{renamed}");
    assert_eq!(renamed, serde_json::json!({ "id": id, "name": "Manga" }));

    let (status, refused) =
        answered(&router, Method::PATCH, &path, &serde_json::json!({ "name": "root-b" })).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{refused}");
    assert!(refused["error"].as_str().expect("an error").contains("already called `root-b`"));

    let (status, refused) =
        answered(&router, Method::PATCH, &path, &serde_json::json!({})).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{refused}");

    let (status, missing) = answered(
        &router,
        Method::PATCH,
        "/api/library/libraries/4242",
        &serde_json::json!({ "name": "Other" }),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{missing}");
    assert!(missing["error"].as_str().expect("an error").contains("4242"));
}

#[tokio::test]
async fn a_folder_is_added_to_a_library_by_the_librarys_id() {
    let (router, state, _held) = test_support::state_with_two_scanned_roots();
    let fixture = test_support::Fixture::new("add-folder-route");
    let extra = a_folder_of(&fixture, "Extra", "Gamma");
    let listed = get_json(&router, "/api/library/libraries").await;
    let id = the_library(&listed, "root-a")["id"].as_i64().expect("an id");
    let path = format!("/api/library/libraries/{id}/folders");

    let (status, added) =
        answered(&router, Method::POST, &path, &serde_json::json!({ "path": extra })).await;
    assert_eq!(status, StatusCode::OK, "{added}");
    assert!(added["watching"].is_boolean(), "{added}");
    let listed = get_json(&router, "/api/library/libraries").await;
    assert_eq!(
        the_library(&listed, "root-a")["folders"].as_array().expect("folders").len(),
        2,
        "{listed}"
    );
    assert_eq!(listed["libraries"].as_array().expect("libraries").len(), 2);

    for (body, status, says) in [
        (
            serde_json::json!({ "path": test_support::a_path_inside_the_first_root(&state) }),
            StatusCode::BAD_REQUEST,
            "inside",
        ),
        (
            serde_json::json!({ "path": "/no/such/taiyaku-test-folder" }),
            StatusCode::BAD_REQUEST,
            "taiyaku-test-folder",
        ),
        (serde_json::json!({}), StatusCode::BAD_REQUEST, "`path`"),
    ] {
        let (got, refused) = answered(&router, Method::POST, &path, &body).await;
        assert_eq!(got, status, "{body}: {refused}");
        assert!(refused["error"].as_str().expect("an error").contains(says), "{refused}");
    }

    let (status, missing) = answered(
        &router,
        Method::POST,
        "/api/library/libraries/4242/folders",
        &serde_json::json!({ "path": extra }),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{missing}");
}

#[tokio::test]
async fn a_librarys_dry_run_counts_and_removes_nothing_and_the_removal_removes() {
    let (router, state, _fixture) = test_support::state_with_two_scanned_roots();
    let listed = get_json(&router, "/api/library/libraries").await;
    let b = the_library(&listed, "root-b");
    let id = b["id"].as_i64().expect("an id");
    let folder = b["folders"][0]["id"].as_i64().expect("a folder id");
    state.scans.lock().unwrap().insert(
        folder,
        crate::ScanProgress {
            started: std::time::Instant::now(),
            done: true,
            report: Some(serde_json::json!({})),
        },
    );
    let path = format!("/api/library/libraries/{id}");

    let (status, rehearsed) =
        answered(&router, Method::DELETE, &format!("{path}?dry_run=true"), &serde_json::json!({}))
            .await;
    assert_eq!(status, StatusCode::OK, "{rehearsed}");
    assert_eq!(rehearsed["dry_run"], true);
    assert_eq!(rehearsed["removed"]["works"], 1, "{rehearsed}");
    assert!(rehearsed["derive"].is_object(), "{rehearsed}");
    assert_eq!(get_json(&router, "/api/library/libraries").await, listed);
    assert!(state.scans.lock().unwrap().contains_key(&folder));

    let (status, misspelt) =
        answered(&router, Method::DELETE, &format!("{path}?dryrun=true"), &serde_json::json!({}))
            .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{misspelt}");
    assert_eq!(get_json(&router, "/api/library/libraries").await, listed);

    let (status, removed) =
        answered(&router, Method::DELETE, &path, &serde_json::json!({})).await;
    assert_eq!(status, StatusCode::OK, "{removed}");
    assert_eq!(removed["dry_run"], false);
    assert_eq!(removed["removed"]["works"], 1, "{removed}");
    let after = get_json(&router, "/api/library/libraries").await;
    assert_eq!(after["libraries"].as_array().expect("libraries").len(), 1, "{after}");
    assert!(
        !state.scans.lock().unwrap().contains_key(&folder),
        "a removed folder's id can be given to the next one added"
    );

    let (status, missing) =
        answered(&router, Method::DELETE, &path, &serde_json::json!({})).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{missing}");
}

#[tokio::test]
async fn removing_an_accounts_only_library_is_a_409_in_the_stores_words() {
    let (router, state, _fixture) = test_support::state_with_two_scanned_roots();
    let listed = get_json(&router, "/api/library/libraries").await;
    let id = the_library(&listed, "root-b")["id"].as_i64().expect("an id");
    {
        let conn = state.library.conn();
        let reader = taiyaku_library::account::add(&conn, "reader", "pw", "user").expect("adds");
        taiyaku_library::account::grant_library(&conn, reader, id).expect("grants");
    }

    for path in [
        format!("/api/library/libraries/{id}?dry_run=true"),
        format!("/api/library/libraries/{id}"),
    ] {
        let (status, refused) =
            answered(&router, Method::DELETE, &path, &serde_json::json!({})).await;
        assert_eq!(status, StatusCode::CONFLICT, "{path}: {refused}");
        let why = refused["error"].as_str().expect("an error");
        assert!(why.contains("`root-b`") && why.contains("`reader`"), "{why}");
    }
    assert_eq!(get_json(&router, "/api/library/libraries").await, listed);
}

/// A folder is added to a library, or made with one. Nothing adds one alone.
#[tokio::test]
async fn the_roots_route_no_longer_adds_a_folder() {
    let (router, _state, _fixture) = test_support::router_over_a_single_scanned_work();
    let response = send_json(
        &router,
        Method::POST,
        "/api/library/roots",
        &serde_json::json!({ "path": "/srv" }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
}
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p taiyaku-serve libraries_api:: -- --test-threads=8`
Expected: FAIL to compile, the module does not exist.

- [ ] **Step 3: Open `config_api`'s helpers**

In `crates/taiyaku-serve/src/config_api.rs`:

- `root_json`, `added_json`, `removal_json`, `derive_report_json` and
  `finished_scan_json` become `pub(crate) fn`.
- Cut the read `list_roots` makes into a function, and call it from
  `list_roots`:

  ```rust
  /// Every root with the two scan columns [`scan::Root`] does not carry, in
  /// [`scan::roots`]'s order: the root, `last_scan_at`, `last_scan_status`.
  pub(crate) fn roots_with_last_scan(
      conn: &rusqlite::Connection,
  ) -> Result<Vec<(scan::Root, Option<String>, Option<String>)>, String> {
      let roots = scan::roots(conn)?;
      // One extra query for both columns, not one per root.
      let mut stmt = conn
          .prepare("SELECT id, last_scan_at, last_scan_status FROM root")
          .map_err(|e| format!("reading scan status: {e}"))?;
      let mut meta: std::collections::HashMap<i64, (Option<String>, Option<String>)> = stmt
          .query_map([], |r| Ok((r.get::<_, i64>(0)?, (r.get(1)?, r.get(2)?))))
          .map_err(|e| format!("reading scan status: {e}"))?
          .collect::<Result<_, _>>()
          .map_err(|e| format!("reading scan status: {e}"))?;
      Ok(roots
          .into_iter()
          .map(|root| {
              let (at, status) = meta.remove(&root.id).unwrap_or_default();
              (root, at, status)
          })
          .collect())
  }
  ```

  `list_roots`'s closure body becomes:

  ```rust
          let conn = state.library.conn();
          let roots = roots_with_last_scan(&conn).map_err(RootAnswer::Failed)?;
          Ok::<_, RootAnswer>(serde_json::json!({
              "roots": roots
                  .iter()
                  .map(|(root, at, status)| root_json(root, at.as_deref(), status.as_deref()))
                  .collect::<Vec<_>>(),
          }))
  ```

- Delete `create_root` and `root_path_from_json`. `list_roots`'s doc comment
  and `RootAnswer`'s lose their mentions of adding; `added_json`'s says it is
  `libraries_api`'s answer for a folder added.

In `crates/taiyaku-serve/src/config_api/tests.rs`, delete the tests that
drive `POST /api/library/roots`
(`a_root_can_be_added_and_then_appears_in_the_list`,
`adding_a_root_at_a_path_that_does_not_exist_is_a_400`,
`adding_a_nested_root_is_refused_with_the_reason`,
`creating_a_root_with_no_path_names_a_root_not_a_rule`), and take its line
out of `every_roots_route_refuses_a_reader`. Step 1's tests hold each case
against the new routes. Move
`a_store_fault_creating_a_root_is_a_500_not_a_400` into
`libraries_api/tests.rs`, driving
`POST /api/library/libraries/{id}/folders` with the fault it already
arranges, and keep its assertion: 500, not 400.

- [ ] **Step 4: Write `libraries_api.rs`**

Create `crates/taiyaku-serve/src/libraries_api.rs`:

```rust
//! The `/api/library/libraries*` handlers: Configuration → Libraries, §6 of
//! `docs/superpowers/specs/2026-10-05-libraries-design.md`.
//!
//! **A library that exists is addressed by its id**, in the path. A name is
//! read only to make a library or to rename one: a name migration 15's
//! backfill kept can carry spaces at its ends, and an id is never given to a
//! later library.
//!
//! **The folder rows inside a library's card are still `config_api`'s.** A
//! folder's settings, its own scan and its removal are routes under
//! `/api/library/roots/{id}`. This module makes libraries, adds folders to
//! them, and scans and removes them whole.
//!
//! Every handler reads or writes on the blocking pool, and one that derives
//! or probes takes [`taiyaku_library::Library::working`], for the reasons
//! `config_api`'s own doc gives.

use std::path::PathBuf;
use std::sync::Arc;

use axum::{
    Json,
    extract::{Extension, Path as AxPath, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use taiyaku_library::{libraries, scan};

use crate::config_api::{
    added_json, derive_report_json, removal_json, root_json, roots_with_last_scan,
};
use crate::{ServeState, auth::Principal};

/// Why one of these routes did not answer with what was asked for.
#[derive(Debug)]
enum LibraryAnswer {
    /// `id` names no library: 404.
    Missing(i64),
    /// The caller's own mistake: 400, in the store's sentence.
    Unusable(String),
    /// Well-formed, names a real library, and still not allowed: 409, in
    /// [`scan::library_removal_refusal`]'s words.
    Refused(String),
    /// A store failure: 500.
    Failed(String),
}

impl IntoResponse for LibraryAnswer {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            Self::Missing(id) => (StatusCode::NOT_FOUND, format!("there is no library {id}")),
            Self::Unusable(why) => (StatusCode::BAD_REQUEST, why),
            Self::Refused(why) => (StatusCode::CONFLICT, why),
            Self::Failed(why) => (StatusCode::INTERNAL_SERVER_ERROR, why),
        };
        (status, Json(serde_json::json!({ "error": message }))).into_response()
    }
}

/// The one failure `spawn_blocking` itself can hand back, as JSON like every
/// other refusal here.
fn library_panicked(what: &str, why: &tokio::task::JoinError) -> Response {
    LibraryAnswer::Failed(format!("{what} panicked: {why}")).into_response()
}

fn added_error(error: scan::AddRootError) -> LibraryAnswer {
    match error {
        scan::AddRootError::Unusable(why) => LibraryAnswer::Unusable(why),
        scan::AddRootError::Failed(why) => LibraryAnswer::Failed(why),
    }
}

/// The folders of library `id`, oldest first, or [`LibraryAnswer::Missing`]
/// where there is no such library.
fn folders_of(conn: &rusqlite::Connection, id: i64) -> Result<Vec<scan::Root>, LibraryAnswer> {
    if libraries::name_of(conn, id)
        .map_err(LibraryAnswer::Failed)?
        .is_none()
    {
        return Err(LibraryAnswer::Missing(id));
    }
    Ok(scan::roots(conn)
        .map_err(LibraryAnswer::Failed)?
        .into_iter()
        .filter(|root| root.library_id == id)
        .collect())
}

/// Every library as the screen draws it, by name: its id, name, works, the
/// EPUB and PDF entries its scans recorded, and each folder in
/// [`root_json`]'s shape, oldest first.
fn libraries_json(conn: &rusqlite::Connection) -> Result<Vec<serde_json::Value>, String> {
    let roots = roots_with_last_scan(conn)?;
    let mut rendered = Vec::new();
    for listed in libraries::list(conn)? {
        let folders: Vec<serde_json::Value> = roots
            .iter()
            .filter(|(root, _, _)| root.library_id == listed.id)
            .map(|(root, at, status)| root_json(root, at.as_deref(), status.as_deref()))
            .collect();
        rendered.push(serde_json::json!({
            "id": listed.id,
            "name": listed.name,
            "works": listed.works,
            "books": libraries::books(conn, listed.id)?,
            "folders": folders,
        }));
    }
    Ok(rendered)
}

/// `GET /api/library/libraries`.
pub(crate) async fn list_libraries(
    State(state): State<Arc<ServeState>>,
    Extension(principal): Extension<Principal>,
    extensions: axum::http::Extensions,
) -> Response {
    if let Some(refusal) = crate::session::administering_or_forbidden(&principal, &extensions) {
        return refusal;
    }
    let built = tokio::task::spawn_blocking(move || {
        let conn = state.library.conn();
        libraries_json(&conn).map_err(LibraryAnswer::Failed)
    })
    .await;

    match built {
        Ok(Ok(libraries)) => Json(serde_json::json!({ "libraries": libraries })).into_response(),
        Ok(Err(no_answer)) => no_answer.into_response(),
        Err(why) => library_panicked("listing libraries", &why),
    }
}

/// `name` and `folders`, as [`create_library`]'s body is read.
fn new_library_from_json(body: &serde_json::Value) -> Result<(String, Vec<PathBuf>), String> {
    let name = body
        .get("name")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| "a library needs a `name`, a string".to_string())?;
    let listed = body
        .get("folders")
        .and_then(serde_json::Value::as_array)
        .ok_or_else(|| "a library needs `folders`, an array of paths".to_string())?;
    let mut folders = Vec::with_capacity(listed.len());
    for entry in listed {
        folders.push(PathBuf::from(entry.as_str().ok_or_else(|| {
            "a library's `folders` must be an array of paths".to_string()
        })?));
    }
    Ok((name.to_string(), folders))
}

/// `POST /api/library/libraries`: a library and its folders, all of them or
/// nothing. Answers the library as the list would, and each folder's watch
/// probe, which runs here and nowhere else.
pub(crate) async fn create_library(
    State(state): State<Arc<ServeState>>,
    Extension(principal): Extension<Principal>,
    extensions: axum::http::Extensions,
    Json(body): Json<serde_json::Value>,
) -> Response {
    if let Some(refusal) = crate::session::administering_or_forbidden(&principal, &extensions) {
        return refusal;
    }
    let (name, folders) = match new_library_from_json(&body) {
        Ok(read) => read,
        Err(why) => return LibraryAnswer::Unusable(why).into_response(),
    };

    let created = tokio::task::spawn_blocking(move || {
        let working = state.library.working().map_err(LibraryAnswer::Failed)?;
        let made = scan::create_library(&working, &name, &folders).map_err(added_error)?;
        let library = libraries_json(&working)
            .map_err(LibraryAnswer::Failed)?
            .into_iter()
            .find(|library| library["id"] == made.library_id)
            .ok_or_else(|| {
                LibraryAnswer::Failed(format!(
                    "library {} was made and is not listed",
                    made.library_id
                ))
            })?;
        Ok::<_, LibraryAnswer>(serde_json::json!({
            "library": library,
            "added": made.folders.iter().map(added_json).collect::<Vec<_>>(),
        }))
    })
    .await;

    match created {
        Ok(Ok(body)) => Json(body).into_response(),
        Ok(Err(no_answer)) => no_answer.into_response(),
        Err(why) => library_panicked("creating a library", &why),
    }
}

/// `PATCH /api/library/libraries/{id}`: rename. Answers the name as stored.
pub(crate) async fn patch_library(
    State(state): State<Arc<ServeState>>,
    Extension(principal): Extension<Principal>,
    extensions: axum::http::Extensions,
    AxPath(id): AxPath<i64>,
    Json(body): Json<serde_json::Value>,
) -> Response {
    if let Some(refusal) = crate::session::administering_or_forbidden(&principal, &extensions) {
        return refusal;
    }
    let Some(name) = body
        .get("name")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
    else {
        return LibraryAnswer::Unusable("renaming a library needs a `name`, a string".to_string())
            .into_response();
    };

    let renamed = tokio::task::spawn_blocking(move || {
        let conn = state.library.conn();
        libraries::rename(&conn, id, &name).map_err(|error| match error {
            libraries::RenameError::Missing(id) => LibraryAnswer::Missing(id),
            libraries::RenameError::Unusable(why) => LibraryAnswer::Unusable(why),
            libraries::RenameError::Failed(why) => LibraryAnswer::Failed(why),
        })
    })
    .await;

    match renamed {
        Ok(Ok(name)) => Json(serde_json::json!({ "id": id, "name": name })).into_response(),
        Ok(Err(no_answer)) => no_answer.into_response(),
        Err(why) => library_panicked("renaming a library", &why),
    }
}

/// `POST /api/library/libraries/{id}/folders`: add a folder to this library,
/// through [`scan::add_root_checked_in`]'s three validations.
pub(crate) async fn add_folder(
    State(state): State<Arc<ServeState>>,
    Extension(principal): Extension<Principal>,
    extensions: axum::http::Extensions,
    AxPath(id): AxPath<i64>,
    Json(body): Json<serde_json::Value>,
) -> Response {
    if let Some(refusal) = crate::session::administering_or_forbidden(&principal, &extensions) {
        return refusal;
    }
    let Some(path) = body
        .get("path")
        .and_then(serde_json::Value::as_str)
        .map(PathBuf::from)
    else {
        return LibraryAnswer::Unusable("adding a folder needs a `path`, a string".to_string())
            .into_response();
    };

    let added = tokio::task::spawn_blocking(move || {
        let working = state.library.working().map_err(LibraryAnswer::Failed)?;
        folders_of(&working, id)?;
        scan::add_root_checked_in(&working, &path, scan::InLibrary::Id(id)).map_err(added_error)
    })
    .await;

    match added {
        Ok(Ok(added)) => Json(added_json(&added)).into_response(),
        Ok(Err(no_answer)) => no_answer.into_response(),
        Err(why) => library_panicked("adding a folder", &why),
    }
}

/// The one query parameter [`delete_library`] reads.
///
/// `deny_unknown_fields`, for the reason `config_api::DeleteRootQuery`
/// carries: a misspelt `dry_run` must be refused, not dropped, on a route
/// whose other reading removes a library.
#[derive(Debug, Default, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DeleteLibraryQuery {
    #[serde(default)]
    dry_run: bool,
}

/// `DELETE /api/library/libraries/{id}`: remove a library, or with
/// `?dry_run=true` answer what that would cost. The body is
/// `config_api::delete_root`'s, so one dialog's types read both.
///
/// **The refusal is asked here, before either store call**, and answered 409
/// in its words. Both store calls refuse too, as an error with nothing to
/// tell it from a store fault.
pub(crate) async fn delete_library(
    State(state): State<Arc<ServeState>>,
    Extension(principal): Extension<Principal>,
    extensions: axum::http::Extensions,
    AxPath(id): AxPath<i64>,
    Query(query): Query<DeleteLibraryQuery>,
) -> Response {
    if let Some(refusal) = crate::session::administering_or_forbidden(&principal, &extensions) {
        return refusal;
    }
    let dry_run = query.dry_run;

    let scans_state = Arc::clone(&state);
    let removed = tokio::task::spawn_blocking(move || {
        let working = state.library.working().map_err(LibraryAnswer::Failed)?;
        let folders: Vec<i64> = folders_of(&working, id)?.iter().map(|root| root.id).collect();
        if let Some(refusal) =
            scan::library_removal_refusal(&working, id).map_err(LibraryAnswer::Failed)?
        {
            return Err(LibraryAnswer::Refused(refusal));
        }
        let (removal, derived) = if dry_run {
            scan::library_removal_cost(&working, id)
        } else {
            scan::remove_library(&working, id)
        }
        .map_err(LibraryAnswer::Failed)?;
        Ok((removal, derived, folders))
    })
    .await;

    // A freed `root.id` is given to the next folder added, so a removal that
    // committed clears each folder's slot in the scan registry, as
    // `config_api::delete_root` clears one.
    if !dry_run && let Ok(Ok((_, _, folders))) = &removed {
        let mut scans = scans_state
            .scans
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        for folder in folders {
            scans.remove(folder);
        }
    }

    match removed {
        Ok(Ok((removal, derived, _))) => Json(serde_json::json!({
            "removed": removal_json(&removal),
            "derive": derive_report_json(&derived),
            "dry_run": dry_run,
        }))
        .into_response(),
        Ok(Err(no_answer)) => no_answer.into_response(),
        Err(why) => library_panicked("removing a library", &why),
    }
}

#[cfg(test)]
mod tests;
```

Add `mod libraries_api;` to `crates/taiyaku-serve/src/lib.rs`, in the
alphabetical list.

- [ ] **Step 5: List the routes**

In `crates/taiyaku-serve/src/routes.rs`, `guarded_routes`: `/api/library/roots`
loses `.post(crate::config_api::create_root)` and its comment's "adds", and
after the roots scan route add:

```rust
        // The libraries an administrator makes, renames and removes on
        // Configuration → Libraries. Admin-gated inside each handler, like
        // the roots routes above, whose folder rows these cards hold.
        (
            "/api/library/libraries",
            get(crate::libraries_api::list_libraries).post(crate::libraries_api::create_library),
        ),
        (
            "/api/library/libraries/{id}",
            axum::routing::patch(crate::libraries_api::patch_library)
                .delete(crate::libraries_api::delete_library),
        ),
        (
            "/api/library/libraries/{id}/folders",
            axum::routing::post(crate::libraries_api::add_folder),
        ),
```

In `grant_postures`, reword `/api/library/roots`'s reason to "admin-only;
lists scan roots, …" and add, after the roots scan entry:

```rust
        (
            "/api/library/libraries",
            GrantIndependent {
                reason: "admin-only; lists and makes libraries, and what an \
                          account is granted is a library, so filtering this \
                          list by the administrator's own grant would hide \
                          the libraries being administered",
            },
        ),
        (
            "/api/library/libraries/{id}",
            GrantIndependent {
                reason: "renames or removes one library, a write to the \
                          whole store with no account-scoped meaning to \
                          filter against",
            },
        ),
        (
            "/api/library/libraries/{id}/folders",
            GrantIndependent {
                reason: "adds a folder to one library, the standing \
                          /api/library/roots held while it was the route \
                          that added one",
            },
        ),
```

In `crates/taiyaku-serve/src/auth/tests.rs`, add the three paths to the
guarded-paths array after `"/api/library/roots/{id}/scan"`, and to
`REFUSE_A_DEVICE_CREDENTIAL` after the same entry.

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cargo test -p taiyaku-serve -- --test-threads=8`
Expected: PASS. If a route-list test fails, it names the list that is short.

- [ ] **Step 7: Watch each fail against its defect**

One at a time, restoring between:

1. `libraries_json`: delete the `.filter(|(root, _, _)| …)` line. Expect
   `the_list_gives_each_library_its_own_folders_works_and_books` to fail with
   2 folders for 1.
2. `delete_library`: swap the two arms, so `dry_run` calls
   `scan::remove_library`. Expect
   `a_librarys_dry_run_counts_and_removes_nothing_and_the_removal_removes` to
   fail.
3. `delete_library`: delete the `library_removal_refusal` block. Expect
   `removing_an_accounts_only_library_is_a_409_in_the_stores_words` to fail
   with 500 for 409.
4. `delete_library`: delete the loop that clears `scans`. Expect the dry-run
   test to fail at its registry assertion.
5. `add_folder`: delete `folders_of(&working, id)?;`. Expect
   `a_folder_is_added_to_a_library_by_the_librarys_id` to fail with 400 for
   404.
6. `patch_library`: delete the `administering_or_forbidden` check. Expect
   `every_libraries_route_refuses_a_reader` to fail naming `PATCH`.

- [ ] **Step 8: Whole crates, then commit**

Run the three commands of Task 1's Step 11. Expected: all pass.

```bash
git add crates/taiyaku-serve/src
git commit -m "The libraries routes: list, create with folders, rename, add a folder, remove with counts"
```

---

## Task 5: Scan now walks each folder of a library and derives once

**Files:**
- Modify: `crates/taiyaku-serve/src/libraries_api.rs`,
  `crates/taiyaku-serve/src/libraries_api/tests.rs`
- Modify: `crates/taiyaku-serve/src/config_api.rs` (`panic_message` cut out of
  `run_scan`)
- Modify: `crates/taiyaku-serve/src/routes.rs`,
  `crates/taiyaku-serve/src/auth/tests.rs`

**Interfaces:**
- Consumes: Task 4's `folders_of`, `LibraryAnswer`, `library_panicked`,
  `config_api::finished_scan_json`; `ServeState::{scans, library_job}`,
  `ScanProgress`; `scan::{scan_root, mark_derive_failed}`, `derive::derive`.
- Produces, admin-only:

  | Route | Answers |
  | --- | --- |
  | `POST /api/library/libraries/{id}/scan` | 202 `{"library_id", "status_url"}`; 404; 409 `{"error"}` while a scan of one of its folders runs |
  | `GET /api/library/libraries/{id}/scan` | 200 `{"library_id", "running", "elapsed_secs": number or null, "folders": [{"root_id", "running"}]}`; 404 |

  While a library's scan runs, each of its folders has a not-done entry in
  `ServeState::scans`, so `GET` and `POST /api/library/roots/{id}/scan`
  answer for it as for a scan of that folder. When it ends, each folder's
  entry is done and holds `{"scan": <that folder's walk>, "derive": <the one
  derive>}`, or `{"error": …}`.
- Produces in `config_api.rs`:
  ```rust
  pub(crate) fn panic_message(payload: &(dyn std::any::Any + Send)) -> String;
  ```

- [ ] **Step 1: Write the failing tests**

Append to `crates/taiyaku-serve/src/libraries_api/tests.rs`:

```rust
/// The fixture's one library, given a second folder holding another series
/// that no scan has walked. Answers the library's id and its two folder ids,
/// oldest first.
async fn a_library_with_an_unscanned_second_folder(
    router: &axum::Router,
    fixture: &test_support::Fixture,
) -> (i64, [i64; 2]) {
    let listed = get_json(router, "/api/library/libraries").await;
    let id = listed["libraries"][0]["id"].as_i64().expect("an id");
    let (status, added) = answered(
        router,
        Method::POST,
        &format!("/api/library/libraries/{id}/folders"),
        &serde_json::json!({ "path": a_folder_of(fixture, "Second", "Second Series") }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{added}");
    let listed = get_json(router, "/api/library/libraries").await;
    let folders = listed["libraries"][0]["folders"].as_array().expect("folders");
    assert_eq!(folders.len(), 2, "{listed}");
    (
        id,
        [
            folders[0]["id"].as_i64().expect("an id"),
            folders[1]["id"].as_i64().expect("an id"),
        ],
    )
}

/// Polls, as `config_api`'s scan tests do, and fails if the scan outlasts the
/// ceiling.
async fn until_the_library_scan_ends(router: &axum::Router, id: i64) -> serde_json::Value {
    for _ in 0..400 {
        let status = get_json(router, &format!("/api/library/libraries/{id}/scan")).await;
        if status["running"] == false {
            return status;
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    panic!("the library's scan did not end");
}

/// The first folder's report carries the derive's counts. Had the derive run
/// after the first walk, it would count one work: the second folder's volume
/// had not been walked yet.
#[tokio::test]
async fn scan_now_walks_each_folder_and_derives_once() {
    let (router, _state, _held) = test_support::router_over_a_single_scanned_work();
    let fixture = test_support::Fixture::new("library-scan");
    let (id, folders) = a_library_with_an_unscanned_second_folder(&router, &fixture).await;

    let (status, started) = answered(
        &router,
        Method::POST,
        &format!("/api/library/libraries/{id}/scan"),
        &serde_json::json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED, "{started}");
    assert_eq!(started["status_url"], format!("/api/library/libraries/{id}/scan"));
    let ended = until_the_library_scan_ends(&router, id).await;
    assert_eq!(ended["folders"].as_array().expect("folders").len(), 2, "{ended}");
    assert!(ended["elapsed_secs"].is_null(), "{ended}");

    for folder in folders {
        let status = get_json(&router, &format!("/api/library/roots/{folder}/scan")).await;
        assert_eq!(status["running"], false, "{status}");
        assert!(
            status["report"]["scan"]["files"].as_u64().expect("a walk") > 0,
            "folder {folder} was walked: {status}"
        );
        assert_eq!(
            status["report"]["derive"]["works"], 2,
            "folder {folder} carries the one derive, run after both walks: {status}"
        );
        assert_eq!(status["last_scan_status"], "ok", "{status}");
    }
    let listed = get_json(&router, "/api/library/libraries").await;
    assert_eq!(listed["libraries"][0]["works"], 2, "{listed}");
}

fn a_scan_running_on(state: &crate::ServeState, folder: i64) {
    state.scans.lock().unwrap().insert(
        folder,
        crate::ScanProgress {
            started: std::time::Instant::now(),
            done: false,
            report: None,
        },
    );
}

#[tokio::test]
async fn a_librarys_scan_and_a_scan_of_one_of_its_folders_refuse_each_other() {
    let (router, state, _held) = test_support::router_over_a_single_scanned_work();
    let fixture = test_support::Fixture::new("library-scan-409");
    let (id, folders) = a_library_with_an_unscanned_second_folder(&router, &fixture).await;
    let scan = format!("/api/library/libraries/{id}/scan");

    // One folder is being scanned on its own: the library's scan is refused,
    // names the folder, and reserves nothing for the other.
    a_scan_running_on(&state, folders[1]);
    let (status, refused) = answered(&router, Method::POST, &scan, &serde_json::json!({})).await;
    assert_eq!(status, StatusCode::CONFLICT, "{refused}");
    assert!(refused["error"].as_str().expect("an error").contains("Second"), "{refused}");
    assert!(!state.scans.lock().unwrap().contains_key(&folders[0]));
    let running = get_json(&router, &scan).await;
    assert_eq!(running["running"], true, "{running}");
    assert!(running["elapsed_secs"].is_number(), "{running}");
    assert_eq!(
        running["folders"],
        serde_json::json!([
            { "root_id": folders[0], "running": false },
            { "root_id": folders[1], "running": true },
        ])
    );

    // The library's scan holds every folder's slot, so a folder's own scan is
    // refused by the check it always made.
    a_scan_running_on(&state, folders[0]);
    for folder in folders {
        let response = send_json(
            &router,
            Method::POST,
            &format!("/api/library/roots/{folder}/scan"),
            &serde_json::json!({}),
        )
        .await;
        assert_eq!(response.status(), StatusCode::CONFLICT, "folder {folder}");
    }
}

#[tokio::test]
async fn a_scan_of_a_library_this_store_does_not_hold_is_a_404() {
    let (router, _state, _held) = test_support::router_over_a_single_scanned_work();
    for method in [Method::GET, Method::POST] {
        let (status, missing) = answered(
            &router,
            method,
            "/api/library/libraries/4242/scan",
            &serde_json::json!({}),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{missing}");
    }
}
```

Add both scan lines to `every_libraries_route_refuses_a_reader`:

```rust
        (Method::GET, "/api/library/libraries/1/scan"),
        (Method::POST, "/api/library/libraries/1/scan"),
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p taiyaku-serve libraries_api:: -- --test-threads=8`
Expected: FAIL: the scan tests answer 404 for every request, no route.

- [ ] **Step 3: Implement**

In `crates/taiyaku-serve/src/config_api.rs`, cut the payload reading out of
`run_scan` and call it there:

```rust
/// What a caught panic said, or that it said nothing readable.
pub(crate) fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    payload
        .downcast_ref::<&str>()
        .map(|s| (*s).to_string())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "the scan panicked with a payload that names no message".to_string())
}
```

`run_scan`'s last arm becomes:

```rust
        Err(payload) => serde_json::json!({
            "error": format!("the scan panicked: {}", panic_message(payload.as_ref())),
        }),
```

In `crates/taiyaku-serve/src/libraries_api.rs`, add `use std::time::Instant;`,
`use taiyaku_library::derive;`, `finished_scan_json` and `panic_message` to
the `config_api` import, and before `#[cfg(test)]`:

```rust
/// Walk each of `roots` in turn and derive once: one report per root, in
/// order, each its own walk with the one derive.
///
/// Holds [`ServeState::library_job`] across the walks and the derive, as
/// `config_api::run_scan_inner` holds it across one of each.
///
/// # Errors
///
/// Returns the sentence every folder's report then carries. Where a walk or
/// the derive fails, each folder already walked has its `last_scan_status`
/// corrected by [`scan::mark_derive_failed`]: its walk wrote `ok`, and what
/// it found was not derived.
fn run_library_scan_inner(
    library: &taiyaku_library::Library,
    roots: &[scan::Root],
    job: &std::sync::Mutex<()>,
) -> Result<Vec<serde_json::Value>, String> {
    let _job = job
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let working = library.working()?;
    let mut walked = Vec::with_capacity(roots.len());
    for root in roots {
        match scan::scan_root(&working, root) {
            Ok(report) => walked.push(report),
            Err(why) => {
                for earlier in &roots[..walked.len()] {
                    let _ = scan::mark_derive_failed(&working, earlier.id);
                }
                return Err(format!("scanning {}: {why}", root.path.display()));
            }
        }
    }
    let derived = match derive::derive(&working) {
        Ok(report) => report,
        Err(why) => {
            for root in roots {
                let _ = scan::mark_derive_failed(&working, root.id);
            }
            return Err(format!(
                "the library's folders were scanned, but deriving its structure afterward \
                 failed: {why}"
            ));
        }
    };
    Ok(walked
        .iter()
        .map(|walk| finished_scan_json(walk, &derived))
        .collect())
}

/// [`run_library_scan_inner`], answering one report per root whatever
/// happened: a failure or a panic is every folder's `{"error": …}`, so no
/// folder's registry entry is left running for good.
fn run_library_scan(
    library: &taiyaku_library::Library,
    roots: &[scan::Root],
    job: &std::sync::Mutex<()>,
) -> Vec<serde_json::Value> {
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        run_library_scan_inner(library, roots, job)
    }));
    let why = match outcome {
        Ok(Ok(reports)) => return reports,
        Ok(Err(why)) => why,
        Err(payload) => format!("the scan panicked: {}", panic_message(payload.as_ref())),
    };
    roots
        .iter()
        .map(|_| serde_json::json!({ "error": why }))
        .collect()
}

/// `POST /api/library/libraries/{id}/scan`: walk each folder of the library
/// in the background, derive once, and answer at once.
///
/// **The scan holds the registry slot of each folder.** The check and the
/// reservations are one lock: a scan already running on one folder refuses
/// the library's, naming the folder, with no slot taken for the others. And
/// while the library's runs, `config_api::start_scan` refuses each folder by
/// the check it always made.
pub(crate) async fn start_library_scan(
    State(state): State<Arc<ServeState>>,
    Extension(principal): Extension<Principal>,
    extensions: axum::http::Extensions,
    AxPath(id): AxPath<i64>,
) -> Response {
    if let Some(refusal) = crate::session::administering_or_forbidden(&principal, &extensions) {
        return refusal;
    }

    let lookup_state = Arc::clone(&state);
    let found = tokio::task::spawn_blocking(move || {
        let conn = lookup_state.library.conn();
        folders_of(&conn, id)
    })
    .await;
    let roots = match found {
        Ok(Ok(roots)) => roots,
        Ok(Err(no_answer)) => return no_answer.into_response(),
        Err(why) => return library_panicked("looking up a library to scan", &why),
    };

    let started = Instant::now();
    {
        let mut scans = state
            .scans
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(busy) = roots
            .iter()
            .find(|root| scans.get(&root.id).is_some_and(|progress| !progress.done))
        {
            return LibraryAnswer::Refused(format!(
                "a scan of {} is already running",
                busy.path.display()
            ))
            .into_response();
        }
        for root in &roots {
            scans.insert(
                root.id,
                crate::ScanProgress {
                    started,
                    done: false,
                    report: None,
                },
            );
        }
    }

    let spawn_state = Arc::clone(&state);
    tokio::task::spawn_blocking(move || {
        let reports = run_library_scan(&spawn_state.library, &roots, &spawn_state.library_job);
        let mut scans = spawn_state
            .scans
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        for (root, report) in roots.iter().zip(reports) {
            scans.insert(
                root.id,
                crate::ScanProgress {
                    started,
                    done: true,
                    report: Some(report),
                },
            );
        }
    });

    (
        StatusCode::ACCEPTED,
        Json(serde_json::json!({
            "library_id": id,
            "status_url": format!("/api/library/libraries/{id}/scan"),
        })),
    )
        .into_response()
}

/// `GET /api/library/libraries/{id}/scan`: whether a scan of any of the
/// library's folders is running, and which. A folder's report is read from
/// that folder's own status route.
pub(crate) async fn library_scan_status(
    State(state): State<Arc<ServeState>>,
    Extension(principal): Extension<Principal>,
    extensions: axum::http::Extensions,
    AxPath(id): AxPath<i64>,
) -> Response {
    if let Some(refusal) = crate::session::administering_or_forbidden(&principal, &extensions) {
        return refusal;
    }

    let lookup_state = Arc::clone(&state);
    let found = tokio::task::spawn_blocking(move || {
        let conn = lookup_state.library.conn();
        folders_of(&conn, id)
    })
    .await;
    let roots = match found {
        Ok(Ok(roots)) => roots,
        Ok(Err(no_answer)) => return no_answer.into_response(),
        Err(why) => return library_panicked("reading a library's scan status", &why),
    };

    let folders: Vec<(i64, Option<std::time::Duration>)> = {
        let scans = state
            .scans
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        roots
            .iter()
            .map(|root| {
                let running = scans
                    .get(&root.id)
                    .filter(|progress| !progress.done)
                    .map(|progress| progress.started.elapsed());
                (root.id, running)
            })
            .collect()
    };
    let elapsed = folders.iter().filter_map(|(_, running)| *running).max();

    Json(serde_json::json!({
        "library_id": id,
        "running": elapsed.is_some(),
        "elapsed_secs": elapsed.map(|e| e.as_secs_f64()),
        "folders": folders
            .iter()
            .map(|(root_id, running)| serde_json::json!({
                "root_id": root_id,
                "running": running.is_some(),
            }))
            .collect::<Vec<_>>(),
    }))
    .into_response()
}
```

In `routes.rs`, `guarded_routes`, after the folders route:

```rust
        (
            "/api/library/libraries/{id}/scan",
            get(crate::libraries_api::library_scan_status)
                .post(crate::libraries_api::start_library_scan),
        ),
```

and in `grant_postures`:

```rust
        (
            "/api/library/libraries/{id}/scan",
            GrantIndependent {
                reason: "starts or reports on a scan of one library's \
                          folders, the standing /api/library/roots/{id}/scan \
                          holds for one folder",
            },
        ),
```

Add the path to both lists in `auth/tests.rs`.

- [ ] **Step 4: Run them to verify they pass**

Run: `cargo test -p taiyaku-serve -- --test-threads=8`
Expected: PASS.

- [ ] **Step 5: Watch each fail against its defect**

One at a time, restoring between:

1. `run_library_scan_inner`: move the derive inside the loop, after each
   walk, and build each report from its own:
   `walked.push(finished_scan_json(&report, &derive::derive(&working)?))`,
   returning `walked`. Expect `scan_now_walks_each_folder_and_derives_once` to
   fail for the first folder with 1 for 2.
2. `start_library_scan`: delete the `if let Some(busy) = … { return … }`
   block. Expect
   `a_librarys_scan_and_a_scan_of_one_of_its_folders_refuse_each_other` to
   fail with 202 for 409.
3. `start_library_scan`: move the `for root in &roots { scans.insert(…) }`
   loop above the `busy` check. Expect the same test to fail: every scan is
   then refused by its own reservation, so
   `scan_now_walks_each_folder_and_derives_once` fails too, with 409.
4. `library_scan_status`: delete `.filter(|progress| !progress.done)`. Expect
   `scan_now_walks_each_folder_and_derives_once` to fail: the poll never sees
   the scan end.

- [ ] **Step 6: Whole crates, then commit**

Run the three commands of Task 1's Step 11. Expected: all pass.

```bash
git add crates/taiyaku-serve/src
git commit -m "Scan now on a library walks each folder and derives once, through the per-folder registry"
```

---

## Task 6: The folder browser, `--folders` and `TAIYAKU_FOLDERS`

**Files:**
- Create: `crates/taiyaku-serve/src/folders_api.rs`,
  `crates/taiyaku-serve/src/folders_api/tests.rs`
- Modify: `crates/taiyaku-serve/src/lib.rs` (`ServeState::folders`,
  `ServeOptions::folders`, `serve`, `mod folders_api;`), and every
  `ServeState` literal in the crate
- Modify: `crates/taiyaku-serve/src/routes.rs`,
  `crates/taiyaku-serve/src/auth/tests.rs`
- Modify: `crates/taiyaku-cli/src/main.rs` (`serve`, `serve_folders`, usage)
  and its tests
- Modify: `Dockerfile`, `README.md`

**Interfaces:**
- Produces:
  ```rust
  // lib.rs
  pub struct ServeState { /* … */ pub folders: Vec<std::path::PathBuf> }
  pub struct ServeOptions { /* … */ pub folders: Vec<std::path::PathBuf> }

  // folders_api.rs
  #[derive(Debug, PartialEq, Eq)]
  pub(crate) struct Listing {
      pub path: Option<PathBuf>,
      pub parent: Option<PathBuf>,
      /// (name, path), by name.
      pub folders: Vec<(String, PathBuf)>,
  }
  #[derive(Debug, PartialEq, Eq)]
  pub(crate) enum Refusal { Outside(String), Unreadable(String) }
  pub(crate) fn list(prefixes: &[PathBuf], path: Option<&Path>) -> Result<Listing, Refusal>;
  ```
  `GET /api/library/folders[?path=P]`, admin-only, answers
  `{"path": P or null, "parent": P or null, "folders": [{"name", "path"}]}`.
  With no `path` the folders are the configured prefixes that exist, each
  named by its whole path. 403 `{"error"}` for a path outside every prefix;
  400 `{"error"}` for one that does not exist or cannot be listed.
- `taiyaku serve --folders DIR` is repeatable. `TAIYAKU_FOLDERS` is a
  `:`-separated list, read only when no `--folders` is given.

- [ ] **Step 1: Write the failing tests for `list`**

Create `crates/taiyaku-serve/src/folders_api/tests.rs`:

```rust
use super::*;
use crate::test_support::{self, send_json};
use axum::http::{Method, StatusCode};

/// `media/Manga/{Volumes,Chapters,.hidden}`, a file beside them, and
/// `elsewhere/Private`. Answers the canonical `media` and `elsewhere`.
fn a_share(fixture: &test_support::Fixture) -> (PathBuf, PathBuf) {
    for dir in [
        "media/Manga/Volumes",
        "media/Manga/Chapters",
        "media/Manga/.hidden",
        "elsewhere/Private",
    ] {
        std::fs::create_dir_all(fixture.path().join(dir)).expect("mkdir");
    }
    std::fs::write(fixture.path().join("media/Manga/notes.txt"), b"x").expect("a file");
    (
        fixture.path().join("media").canonicalize().expect("canonical"),
        fixture.path().join("elsewhere").canonicalize().expect("canonical"),
    )
}

fn names(listing: &Listing) -> Vec<&str> {
    listing.folders.iter().map(|(name, _)| name.as_str()).collect()
}

#[test]
fn with_no_path_it_lists_the_prefixes_that_exist_and_nothing_with_none() {
    let fixture = test_support::Fixture::new("folders-prefixes");
    let (media, _elsewhere) = a_share(&fixture);

    assert_eq!(
        list(&[], None),
        Ok(Listing { path: None, parent: None, folders: Vec::new() })
    );
    let listed = list(&[media.clone(), fixture.path().join("not-mounted")], None).expect("lists");
    assert_eq!(
        listed,
        Listing {
            path: None,
            parent: None,
            folders: vec![(media.to_string_lossy().into_owned(), media)],
        }
    );
}

#[test]
fn it_lists_subdirectories_by_name_and_nothing_else() {
    let fixture = test_support::Fixture::new("folders-list");
    let (media, _elsewhere) = a_share(&fixture);
    let manga = media.join("Manga");

    let listed = list(std::slice::from_ref(&media), Some(&manga)).expect("lists");

    assert_eq!(names(&listed), vec!["Chapters", "Volumes"], "no file, no dot-entry");
    assert_eq!(listed.folders[0].1, manga.join("Chapters"));
    assert_eq!(listed.path.as_deref(), Some(manga.as_path()));
    assert_eq!(listed.parent.as_deref(), Some(media.as_path()));
    assert_eq!(
        list(std::slice::from_ref(&media), Some(&media)).expect("lists").parent,
        None,
        "a prefix has no parent to go up to"
    );
}

#[test]
fn a_path_that_leaves_every_prefix_is_refused_however_it_is_spelt() {
    let fixture = test_support::Fixture::new("folders-outside");
    let (media, elsewhere) = a_share(&fixture);
    let prefixes = [media.clone()];

    for outside in [
        elsewhere.clone(),
        media.join("Manga/../../elsewhere"),
        media.join(".."),
    ] {
        assert!(
            matches!(list(&prefixes, Some(&outside)), Err(Refusal::Outside(_))),
            "{}",
            outside.display()
        );
    }
    assert!(matches!(
        list(&prefixes, Some(&media.join("Manga/Missing"))),
        Err(Refusal::Unreadable(_))
    ));
    assert!(
        matches!(list(&[], Some(&media)), Err(Refusal::Outside(_))),
        "with no prefix configured nothing may be browsed"
    );
}

#[cfg(unix)]
#[test]
fn a_symlink_out_of_the_prefix_is_neither_listed_nor_followed() {
    let fixture = test_support::Fixture::new("folders-symlink");
    let (media, elsewhere) = a_share(&fixture);
    std::os::unix::fs::symlink(&elsewhere, media.join("Manga/Escape")).expect("a link out");
    std::os::unix::fs::symlink(media.join("Manga/Volumes"), media.join("Manga/Alias"))
        .expect("a link that stays inside");
    let prefixes = [media.clone()];

    let listed = list(&prefixes, Some(&media.join("Manga"))).expect("lists");
    assert_eq!(names(&listed), vec!["Alias", "Chapters", "Volumes"]);
    assert!(matches!(
        list(&prefixes, Some(&media.join("Manga/Escape"))),
        Err(Refusal::Outside(_))
    ));
}
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p taiyaku-serve folders_api:: -- --test-threads=8`
Expected: FAIL to compile, the module does not exist.

- [ ] **Step 3: Write `folders_api.rs`**

Create `crates/taiyaku-serve/src/folders_api.rs`, and add `mod folders_api;`
to `lib.rs`:

```rust
//! `GET /api/library/folders`: the folder browser behind "add a folder", §6.1
//! of `docs/superpowers/specs/2026-10-05-libraries-design.md`.
//!
//! **It answers only beneath the prefixes the server was started with**
//! (`--folders`, `TAIYAKU_FOLDERS`), and names directories only: no file, no
//! size, no dot-entry. With no prefix it lists nothing, and the screen falls
//! back to a typed path.
//!
//! **A path is compared in canonical form**, so `..`, an encoded form and a
//! symlink are each resolved before the comparison. A symlink beneath a
//! prefix that points outside it is not listed and cannot be listed.
//!
//! **It is a way to find a path, not a way to add one.** Adding still goes
//! through `scan::add_root_checked_in`, whose refusals are unchanged.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::{
    Json,
    extract::{Extension, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};

use crate::{ServeState, auth::Principal};

/// The subdirectories of one path, or the prefixes themselves.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Listing {
    /// The path listed, canonical. `None` where the prefixes were listed.
    pub path: Option<PathBuf>,
    /// Where "up" goes. `None` at a prefix, and where the prefixes were
    /// listed.
    pub parent: Option<PathBuf>,
    /// Each directory's name and path, by name. A prefix is named by its
    /// whole path.
    pub folders: Vec<(String, PathBuf)>,
}

/// Why a path was not listed.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Refusal {
    /// Beneath no prefix: 403.
    Outside(String),
    /// It does not exist, or this process cannot list it: 400.
    Unreadable(String),
}

/// List `path`'s subdirectories, or the prefixes where `path` is `None`.
///
/// A prefix that cannot be canonicalised, an unmounted share for one, is
/// left out and is beneath nothing.
pub(crate) fn list(prefixes: &[PathBuf], path: Option<&Path>) -> Result<Listing, Refusal> {
    let prefixes: Vec<PathBuf> = prefixes
        .iter()
        .filter_map(|prefix| prefix.canonicalize().ok())
        .collect();
    let within = |candidate: &Path| prefixes.iter().any(|prefix| candidate.starts_with(prefix));

    let Some(path) = path else {
        return Ok(Listing {
            path: None,
            parent: None,
            folders: prefixes
                .iter()
                .map(|prefix| (prefix.to_string_lossy().into_owned(), prefix.clone()))
                .collect(),
        });
    };

    let canonical = path
        .canonicalize()
        .map_err(|e| Refusal::Unreadable(format!("{}: {e}", path.display())))?;
    if !within(&canonical) {
        return Err(Refusal::Outside(format!(
            "{} is not inside a folder this server may browse",
            canonical.display()
        )));
    }
    let entries = std::fs::read_dir(&canonical).map_err(|e| {
        Refusal::Unreadable(format!("{}: cannot be read: {e}", canonical.display()))
    })?;

    let mut folders = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        let child = entry.path();
        // A symlink is a directory only if what it names is one, and it is
        // offered only if what it names is beneath a prefix too.
        let is_folder = if kind.is_symlink() {
            child
                .canonicalize()
                .is_ok_and(|target| target.is_dir() && within(&target))
        } else {
            kind.is_dir()
        };
        if is_folder {
            folders.push((name, child));
        }
    }
    folders.sort();

    let parent = if prefixes.contains(&canonical) {
        None
    } else {
        canonical.parent().map(Path::to_path_buf)
    };
    Ok(Listing {
        path: Some(canonical),
        parent,
        folders,
    })
}

/// The one query parameter [`list_folders`] reads. `deny_unknown_fields`, so
/// a misspelt `path` is refused and not answered as a list of the prefixes.
#[derive(Debug, Default, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct FoldersQuery {
    #[serde(default)]
    path: Option<String>,
}

/// `GET /api/library/folders[?path=…]`. The read runs on the blocking pool: a
/// sleeping disk can take seconds to list a directory.
pub(crate) async fn list_folders(
    State(state): State<Arc<ServeState>>,
    Extension(principal): Extension<Principal>,
    extensions: axum::http::Extensions,
    Query(query): Query<FoldersQuery>,
) -> Response {
    if let Some(refusal) = crate::session::administering_or_forbidden(&principal, &extensions) {
        return refusal;
    }

    let listed = tokio::task::spawn_blocking(move || {
        list(&state.folders, query.path.as_deref().map(Path::new))
    })
    .await;

    let refused = |status: StatusCode, why: String| {
        (status, Json(serde_json::json!({ "error": why }))).into_response()
    };
    match listed {
        Ok(Ok(listing)) => Json(serde_json::json!({
            "path": listing.path.map(|p| p.to_string_lossy().into_owned()),
            "parent": listing.parent.map(|p| p.to_string_lossy().into_owned()),
            "folders": listing
                .folders
                .iter()
                .map(|(name, path)| serde_json::json!({
                    "name": name,
                    "path": path.to_string_lossy(),
                }))
                .collect::<Vec<_>>(),
        }))
        .into_response(),
        Ok(Err(Refusal::Outside(why))) => refused(StatusCode::FORBIDDEN, why),
        Ok(Err(Refusal::Unreadable(why))) => refused(StatusCode::BAD_REQUEST, why),
        Err(why) => refused(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("listing folders panicked: {why}"),
        ),
    }
}

#[cfg(test)]
mod tests;
```

- [ ] **Step 4: Give the server its prefixes**

In `crates/taiyaku-serve/src/lib.rs`:

```rust
    /// The directories `GET /api/library/folders` may list beneath:
    /// `--folders`, or `TAIYAKU_FOLDERS`. Empty lists nothing.
    pub folders: Vec<std::path::PathBuf>,
```

on `ServeState` after `library_job`, and on `ServeOptions` after `web` with
the doc "`--folders`: what the folder browser may list beneath". `serve`
destructures it and passes it into its `ServeState`.

Add `folders: Vec::new(),` after `library_job:` in every other `ServeState`
literal. `command grep -a -rn 'library_job:' crates/taiyaku-serve/src` lists
them; `cargo check -p taiyaku-serve --all-targets` names any missed.

In `routes.rs`, `guarded_routes`, after the libraries scan route:

```rust
        (
            "/api/library/folders",
            get(crate::folders_api::list_folders),
        ),
```

and in `grant_postures`:

```rust
        (
            "/api/library/folders",
            GrantIndependent {
                reason: "admin-only; names directories beneath the prefixes \
                          the server was started with, which are a fact \
                          about the deployment and hold nothing any \
                          account's grant could scope",
            },
        ),
```

Add the path to both lists in `auth/tests.rs`.

- [ ] **Step 5: Write the failing route tests**

Append to `folders_api/tests.rs`. The fixture is built from an existing one,
whose state is replaced by one that differs only in `folders`: read
`test_support::state_with_two_scanned_roots` and build the `ServeState`
literal the same way, with `folders: vec![media.clone()]`, then
`test_support::authenticating(crate::build_router(state.clone()), &state.library)`.

```rust
#[tokio::test]
async fn the_route_lists_beneath_a_prefix_and_refuses_outside_it() {
    let fixture = test_support::Fixture::new("folders-route");
    let (media, elsewhere) = a_share(&fixture);
    let router = a_router_browsing(&fixture, vec![media.clone()]);
    let at = |path: &Path| {
        format!(
            "/api/library/folders?path={}",
            path.to_string_lossy().replace('/', "%2F")
        )
    };

    let prefixes = test_support::get_json(&router, "/api/library/folders").await;
    assert_eq!(
        prefixes,
        serde_json::json!({
            "path": null,
            "parent": null,
            "folders": [{ "name": media.to_string_lossy(), "path": media.to_string_lossy() }],
        })
    );

    let manga = test_support::get_json(&router, &at(&media.join("Manga"))).await;
    assert_eq!(manga["parent"], media.to_string_lossy().as_ref());
    assert_eq!(
        manga["folders"]
            .as_array()
            .expect("folders")
            .iter()
            .map(|f| f["name"].as_str().expect("a name"))
            .collect::<Vec<_>>(),
        vec!["Chapters", "Volumes"]
    );

    for (path, status) in [
        (at(&elsewhere), StatusCode::FORBIDDEN),
        (
            format!("{}%2F%2e%2e%2F%2e%2e%2Felsewhere", at(&media.join("Manga"))),
            StatusCode::FORBIDDEN,
        ),
        (at(&media.join("Missing")), StatusCode::BAD_REQUEST),
        ("/api/library/folders?paht=%2F".to_string(), StatusCode::BAD_REQUEST),
    ] {
        let response = test_support::get(&router, &path).await;
        assert_eq!(response.status(), status, "{path}");
    }
}

#[tokio::test]
async fn the_folders_route_refuses_a_reader() {
    let (router, _state, _fixture) = test_support::router_over_a_single_scanned_work_as_a_reader();
    let response = send_json(
        &router,
        Method::GET,
        "/api/library/folders",
        &serde_json::json!({}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}
```

Write `a_router_browsing(fixture, folders) -> axum::Router` in the test file
as described above.

- [ ] **Step 6: Run the crate's tests to verify they pass**

Run: `cargo test -p taiyaku-serve -- --test-threads=8`
Expected: PASS.

- [ ] **Step 7: Watch each fail against its defect**

One at a time, restoring between:

1. `list`: replace `if !within(&canonical)` with `if !within(path)`. Expect
   `a_path_that_leaves_every_prefix_is_refused_however_it_is_spelt` to fail at
   `media/Manga/../../elsewhere`.
2. `list`: replace the symlink arm with `child.is_dir()`. Expect
   `a_symlink_out_of_the_prefix_is_neither_listed_nor_followed` to fail,
   listing `Escape`.
3. `list`: delete the `if name.starts_with('.') { continue; }` block. Expect
   `it_lists_subdirectories_by_name_and_nothing_else` to fail.
4. `list`: replace `kind.is_dir()` with `true`. Expect the same test to fail,
   listing `notes.txt`.
5. `list`: delete `folders.sort();` and, if the test still passes because the
   directory happened to list in order, say so in the report and leave the
   line: `read_dir`'s order is the filesystem's.
6. `list_folders`: delete the `administering_or_forbidden` check. Expect
   `the_folders_route_refuses_a_reader` to fail.

- [ ] **Step 8: The CLI's flag and variable**

In `crates/taiyaku-cli/src/main.rs`, write the test first, beside
`serve_auth`'s tests:

```rust
#[test]
fn serve_folders_reads_each_flag_and_the_variable_only_without_one() {
    let args = |words: &[&str]| words.iter().map(|w| (*w).to_string()).collect::<Vec<_>>();
    let paths = |words: &[&str]| words.iter().map(PathBuf::from).collect::<Vec<_>>();

    assert_eq!(serve_folders(&args(&[]), None), Ok(Vec::new()));
    assert_eq!(
        serve_folders(&args(&[]), Some("/media:/mnt/user/media 1::/a,b")),
        Ok(paths(&["/media", "/mnt/user/media 1", "/a,b"])),
        "split on colons alone: a path may hold a space or a comma"
    );
    assert_eq!(
        serve_folders(
            &args(&["--folders", "/one", "--db", "x.db", "--folders", "/two"]),
            Some("/media")
        ),
        Ok(paths(&["/one", "/two"])),
        "a flag replaces the variable's list"
    );
    assert!(serve_folders(&args(&["--folders"]), None).is_err());
    assert!(serve_folders(&args(&["--folders", "--db"]), None).is_err());
}
```

Run: `cargo test -p taiyaku-cli serve_folders -- --test-threads=8`
Expected: FAIL to compile.

Then, after `serve_auth`:

```rust
/// `--folders DIR`, repeatable, or `TAIYAKU_FOLDERS`: the directories the
/// Libraries screen's folder browser may list beneath.
///
/// `from_environment` is `TAIYAKU_FOLDERS`, read by the caller as
/// `serve_auth`'s is. It is a `:`-separated list, as `PATH` is, because a
/// path can hold a space or a comma. Any `--folders` replaces it, the
/// precedence `--trusted-proxy` holds over its variable.
///
/// Nothing is checked here. A prefix that does not exist when a request
/// arrives, an unmounted share for one, lists nothing then.
///
/// # Errors
///
/// Returns an error if a `--folders` has no value.
fn serve_folders(args: &[String], from_environment: Option<&str>) -> Result<Vec<PathBuf>, String> {
    let mut folders = Vec::new();
    for (index, arg) in args.iter().enumerate() {
        if arg != "--folders" {
            continue;
        }
        let value = args
            .get(index + 1)
            .filter(|next| !next.starts_with("--"))
            .ok_or("--folders needs a directory")?;
        folders.push(PathBuf::from(value));
    }
    if folders.is_empty()
        && let Some(listed) = from_environment
    {
        folders.extend(
            listed
                .split(':')
                .filter(|value| !value.is_empty())
                .map(PathBuf::from),
        );
    }
    Ok(folders)
}
```

In `serve`: add `"--folders"` to `reject_unknown_flags`'s list, in its
alphabetical place; after the `serve_auth` line add

```rust
    let folders = serve_folders(args, std::env::var("TAIYAKU_FOLDERS").ok().as_deref())?;
```

and pass `folders` in `ServeOptions`. In the usage text, add `[--folders
DIR]...` to the `taiyaku serve` line after `[--web DIR]`.

Run: `cargo test -p taiyaku-cli -- --test-threads=8`
Expected: PASS. A usage test that holds the synopsis moves with it.

Watch it fail: in `serve_folders`, replace `if folders.is_empty() && let …`
with `if let …`. Expect the test to fail at "a flag replaces the variable's
list".

- [ ] **Step 9: The image and the README**

In `Dockerfile`, the runtime stage's `ENV`:

```dockerfile
# The database lands at /config/taiyaku/library.db and its backups beside it;
# covers and tiles under /cache/taiyaku. Map /config somewhere backed up and
# /cache somewhere that is not: everything in it is regenerable. /media is
# where the Libraries screen's folder browser starts: map the share that
# holds every library's folders there.
ENV XDG_DATA_HOME=/config \
    XDG_CACHE_HOME=/cache \
    TAIYAKU_FOLDERS=/media
```

In `README.md`, "Running it on a server":

- The table's third row becomes
  `| `/media` | the share your libraries' folders are in | it is your library |`.
- The compose file's last volume becomes `- "/mnt/user/media 1:/media"`.
- The paragraph "Give the first account a password from the host, then add
  the library:", its code block and the `--library` paragraph after it become:

  ````markdown
  Give the first account a password from the host:

  ```sh
  docker exec -it taiyaku taiyaku library user promote davey
  ```

  Then sign in and make the libraries on Configuration → Libraries. Each is a
  name and one or more folders, picked in a browser that starts at `/media`:
  the image sets `TAIYAKU_FOLDERS=/media`, and the browser lists nothing
  outside it. Two folders in one library, `/media/Manga/Volumes` and
  `/media/Manga/Chapters` say, are derived together, so a chapter release
  joins its volume as another edition. Scan now on a library's card walks its
  folders and derives once.

  The same from the host, for a script:

  ```sh
  docker exec taiyaku taiyaku library scan --add /media/Manga/Volumes --library Manga
  ```

  `--library` names the library the folder is filed under, and creates it.
  Left out, the folder becomes a library named for the folder, `Volumes` here.
  ````

- In the section that lists `taiyaku serve`'s flags, if there is one beside
  `--web`, add `--folders DIR` with one sentence: the directories the folder
  browser may list beneath, repeatable, `TAIYAKU_FOLDERS` (`:`-separated)
  without it.

Run: `command grep -a -n '/library' README.md` and read each hit: a path
under `/library` that the compose file no longer maps is reworded to
`/media/…`.

- [ ] **Step 10: Whole crates, then commit**

Run the three commands of Task 1's Step 11. Expected: all pass.

```bash
git add crates Dockerfile README.md
git commit -m "The folder browser: directories beneath --folders or TAIYAKU_FOLDERS, and the image's /media"
```

---

## Task 7: The web client, a refusal's sentence, and the folder row's two new props

**Files:**
- Create: `web/src/libraries.ts`, `web/src/libraries.test.ts`,
  `web/src/testing/input.ts`, `web/src/testing/derive.ts`
- Modify: `web/src/message.ts`, `web/src/message.test.ts`
- Modify: `web/src/RootRow.tsx`, `web/src/RemoveRoot.tsx`,
  `web/src/ScanReport.tsx` and their `.test.tsx` files

**Interfaces:**
- Consumes: Tasks 4–6's routes; `config.ts`'s `Root`, `Added`, `Probe`,
  `RemovalAnswer`; `api.ts`'s `getJson`, `sendJson`.
- Produces:
  ```ts
  // libraries.ts
  export interface Library { id: number; name: string; works: number; books: number; folders: Root[] }
  export interface Created { library: Library; added: Added[] }
  export interface LibraryScanStatus {
    library_id: number
    running: boolean
    elapsed_secs: number | null
    folders: { root_id: number; running: boolean }[]
  }
  export interface FolderListing {
    path: string | null
    parent: string | null
    folders: { name: string; path: string }[]
  }
  export const listLibraries: () => Promise<{ libraries: Library[] }>
  export const createLibrary: (name: string, folders: string[]) => Promise<Created>
  export const renameLibrary: (id: number, name: string) => Promise<{ id: number; name: string }>
  export const addFolder: (id: number, path: string) => Promise<Added>
  export const libraryRemovalCost: (id: number) => Promise<RemovalAnswer>
  export const removeLibrary: (id: number) => Promise<RemovalAnswer>
  export const startLibraryScan: (id: number) => Promise<{ library_id: number; status_url: string }>
  export const libraryScanStatus: (id: number) => Promise<LibraryScanStatus>
  export const listFolders: (path?: string) => Promise<FolderListing>

  // message.ts
  export function sentenceOf(err: unknown): string

  // RootRow.tsx
  export interface RootRowProps {
    root: Root
    probe?: Probe
    /** Bumped by the library's card when it starts a scan of the library. */
    adopt?: number
    /** The library's name, where this is its only folder. */
    lastOf?: string
    onChanged: () => void
  }
  // RemoveRoot.tsx: RemoveRootProps gains `lastOf?: string`

  // testing/input.ts
  export async function typeInto(box: HTMLInputElement, value: string): Promise<void>
  export async function press(el: Element): Promise<void>
  export async function submit(form: Element): Promise<void>

  // testing/derive.ts
  export const emptyDerive: Derived
  ```

- [ ] **Step 1: Write the failing tests for `sentenceOf`**

Append to `web/src/message.test.ts`, importing `sentenceOf`:

```ts
describe('sentenceOf', () => {
  it("answers the refusal's own sentence where the body carries one", () => {
    const thrown = new Error(
      '/api/library/libraries/3: 409 {"error":"`Manga` is the only library `sam` is granted."}',
    )
    expect(sentenceOf(thrown)).toBe(
      '`Manga` is the only library `sam` is granted.',
    )
  })

  it('keeps the whole message where there is no sentence to take', () => {
    for (const message of [
      '/api/library/roots/3: 400 the path does not exist',
      '/api/library/libraries: 500 {"detail":"no error key"}',
      '/api/library/libraries: 500 {"error":7}',
      '/api/library/libraries: 502 {not json}',
      'Failed to fetch',
    ]) {
      expect(sentenceOf(new Error(message))).toBe(message)
    }
  })

  it('reads a thrown value that is not an Error as messageOf does', () => {
    expect(sentenceOf('plain')).toBe('plain')
  })
})
```

Run: `pnpm -C web exec vitest run --project node --reporter=default src/message.test.ts`
Expected: FAIL, `sentenceOf` is not exported.

- [ ] **Step 2: Implement `sentenceOf`**

Append to `web/src/message.ts`:

```ts
/**
 * A refusal's own sentence, where the server sent one.
 *
 * `api.ts` throws `path: status body`, and the administration routes answer
 * a refusal as `{"error": "<sentence>"}`. This answers the sentence for
 * those and `messageOf`'s whole string for everything else: a failure that
 * carries no sentence keeps its path and status, which are then all there is
 * to go on.
 *
 * A caller that needs the status reads it from `messageOf`, with
 * `config.ts`'s `statusOf`.
 */
export function sentenceOf(err: unknown): string {
  const message = messageOf(err)
  const found = /^\S+: \d{3} (\{[\s\S]*\})$/.exec(message)
  if (found === null) return message
  try {
    const body: unknown = JSON.parse(found[1])
    if (
      typeof body === 'object' &&
      body !== null &&
      'error' in body &&
      typeof body.error === 'string'
    ) {
      return body.error
    }
  } catch {
    // Not JSON after all, so the whole message stands.
  }
  return message
}
```

Run the same command. Expected: PASS. Watch it fail: delete the
`typeof body.error === 'string'` condition; expect the "keeps the whole
message" case to fail on `{"error":7}`.

- [ ] **Step 3: Write the failing tests for the client**

Create `web/src/libraries.test.ts`:

```ts
import { afterEach, describe, expect, it, vi } from 'vitest'
import {
  addFolder,
  createLibrary,
  libraryRemovalCost,
  libraryScanStatus,
  listFolders,
  listLibraries,
  removeLibrary,
  renameLibrary,
  startLibraryScan,
} from './libraries'

const answering = (body: unknown) => {
  const fetchMock = vi.fn().mockImplementation(() =>
    Promise.resolve(
      new Response(JSON.stringify(body), {
        status: 200,
        headers: { 'content-type': 'application/json' },
      }),
    ),
  )
  vi.stubGlobal('fetch', fetchMock)
  return fetchMock
}

/** What the first request was: its path, its method, and its body parsed. */
const sent = (fetchMock: ReturnType<typeof vi.fn>) => {
  const [path, init] = fetchMock.mock.calls[0] as [string, RequestInit?]
  return {
    path,
    method: init?.method ?? 'GET',
    body:
      typeof init?.body === 'string' ? (JSON.parse(init.body) as unknown) : undefined,
  }
}

afterEach(() => {
  vi.unstubAllGlobals()
})

describe('the libraries client', () => {
  it.each([
    ['listLibraries', () => listLibraries(), 'GET', '/api/library/libraries', undefined],
    [
      'createLibrary',
      () => createLibrary('Manga', ['/media/a', '/media/b']),
      'POST',
      '/api/library/libraries',
      { name: 'Manga', folders: ['/media/a', '/media/b'] },
    ],
    [
      'renameLibrary',
      () => renameLibrary(7, 'Comics'),
      'PATCH',
      '/api/library/libraries/7',
      { name: 'Comics' },
    ],
    [
      'addFolder',
      () => addFolder(7, '/media/c'),
      'POST',
      '/api/library/libraries/7/folders',
      { path: '/media/c' },
    ],
    [
      'libraryRemovalCost',
      () => libraryRemovalCost(7),
      'DELETE',
      '/api/library/libraries/7?dry_run=true',
      undefined,
    ],
    ['removeLibrary', () => removeLibrary(7), 'DELETE', '/api/library/libraries/7', undefined],
    [
      'startLibraryScan',
      () => startLibraryScan(7),
      'POST',
      '/api/library/libraries/7/scan',
      undefined,
    ],
    [
      'libraryScanStatus',
      () => libraryScanStatus(7),
      'GET',
      '/api/library/libraries/7/scan',
      undefined,
    ],
    ['listFolders, at the prefixes', () => listFolders(), 'GET', '/api/library/folders', undefined],
    [
      'listFolders, at a path',
      () => listFolders('/mnt/user/media 1/Manga & more'),
      'GET',
      '/api/library/folders?path=%2Fmnt%2Fuser%2Fmedia%201%2FManga%20%26%20more',
      undefined,
    ],
  ])('%s asks the route it is named for', async (_name, call, method, path, body) => {
    const fetchMock = answering({})
    await call()
    expect(sent(fetchMock)).toEqual({ path, method, body })
  })
})
```

Run: `pnpm -C web exec vitest run --project node --reporter=default src/libraries.test.ts`
Expected: FAIL, the module does not exist.

- [ ] **Step 4: Write the client**

Create `web/src/libraries.ts`:

```ts
/**
 * The `/api/library/libraries*` and folder-browser client: Configuration →
 * Libraries.
 *
 * **A library that exists is addressed by its id.** A name is sent only to
 * make one or to rename one.
 *
 * **A folder's own routes are still `config.ts`'s.** A library's card holds a
 * `RootRow` per folder, and that row patches, scans and removes its folder
 * through `/api/library/roots/{id}`. What a library does whole is here.
 */
import { getJson, sendJson } from './api'
import type { Added, RemovalAnswer, Root } from './config'

/** One library, as `GET /api/library/libraries` sends it. */
export interface Library {
  id: number
  name: string
  /** The works filed under it. */
  works: number
  /** EPUB and PDF entries its scans recorded, which nothing reads yet. */
  books: number
  /** Its folders, oldest first. */
  folders: Root[]
}

/** `POST /api/library/libraries`'s answer: the library, and each folder's probe. */
export interface Created {
  library: Library
  added: Added[]
}

/** `GET /api/library/libraries/{id}/scan`. */
export interface LibraryScanStatus {
  library_id: number
  /** Whether a scan of any of its folders is running. */
  running: boolean
  /** `null` when nothing is running. */
  elapsed_secs: number | null
  folders: { root_id: number; running: boolean }[]
}

/** `GET /api/library/folders`: one directory's subdirectories, or the prefixes. */
export interface FolderListing {
  /** `null` where the prefixes themselves were listed. */
  path: string | null
  /** Where "up" goes. `null` at a prefix. */
  parent: string | null
  folders: { name: string; path: string }[]
}

/** `sendJson` resolves `undefined` for a 204, which no route here answers. */
async function expectBody<T>(sent: Promise<T | undefined>): Promise<T> {
  const body = await sent
  if (body === undefined)
    throw new Error('expected a JSON body, got an empty 204')
  return body
}

export const listLibraries = () =>
  getJson<{ libraries: Library[] }>('/api/library/libraries')

/** A library and its folders: all of them, or a refusal and nothing made. */
export const createLibrary = (
  name: string,
  folders: string[],
): Promise<Created> =>
  expectBody(
    sendJson<Created>('/api/library/libraries', 'POST', { name, folders }),
  )

/** Answers the name as stored, which is the name sent with its spaces stripped. */
export const renameLibrary = (id: number, name: string) =>
  expectBody(
    sendJson<{ id: number; name: string }>(
      `/api/library/libraries/${id}`,
      'PATCH',
      { name },
    ),
  )

export const addFolder = (id: number, path: string): Promise<Added> =>
  expectBody(
    sendJson<Added>(`/api/library/libraries/${id}/folders`, 'POST', { path }),
  )

/** What removing library `id` would cost. `?dry_run=true`, spelt exactly. */
export const libraryRemovalCost = (id: number): Promise<RemovalAnswer> =>
  expectBody(
    sendJson<RemovalAnswer>(
      `/api/library/libraries/${id}?dry_run=true`,
      'DELETE',
    ),
  )

export const removeLibrary = (id: number): Promise<RemovalAnswer> =>
  expectBody(
    sendJson<RemovalAnswer>(`/api/library/libraries/${id}`, 'DELETE'),
  )

/**
 * Start a scan of every folder of library `id`. Answers 409 while a scan of
 * any one of them is running.
 */
export const startLibraryScan = (
  id: number,
): Promise<{ library_id: number; status_url: string }> =>
  expectBody(
    sendJson<{ library_id: number; status_url: string }>(
      `/api/library/libraries/${id}/scan`,
      'POST',
    ),
  )

export const libraryScanStatus = (id: number) =>
  getJson<LibraryScanStatus>(`/api/library/libraries/${id}/scan`)

/** The subdirectories of `path`, or the prefixes when it is left out. */
export const listFolders = (path?: string) =>
  getJson<FolderListing>(
    path === undefined
      ? '/api/library/folders'
      : `/api/library/folders?path=${encodeURIComponent(path)}`,
  )
```

Run the same command. Expected: PASS.

- [ ] **Step 5: The shared input helpers**

Create `web/src/testing/input.ts`:

```ts
import { act } from 'react'
import { flush } from './async'

/**
 * Type `value` into a controlled box.
 *
 * Through the prototype's setter, because React tracks a controlled input's
 * value on the element and ignores an assignment that goes through its own
 * override.
 */
export async function typeInto(box: HTMLInputElement, value: string) {
  await act(async () => {
    Object.getOwnPropertyDescriptor(
      HTMLInputElement.prototype,
      'value',
    )?.set?.call(box, value)
    box.dispatchEvent(new Event('input', { bubbles: true }))
  })
}

/** Click, and let what the click started settle. */
export async function press(el: Element) {
  await flush(() => {
    ;(el as HTMLElement).click()
  })
}

/** Submit a form, and let what the submit started settle. */
export async function submit(form: Element) {
  await flush(() => {
    form.dispatchEvent(new Event('submit', { bubbles: true, cancelable: true }))
  })
}
```

Create `web/src/testing/derive.ts`, for the two dialogs that draw a whole
derive report:

```ts
import type { Derived } from '../config'

/**
 * A whole `DeriveReport` that found nothing. Whole, because the dialogs
 * that take one draw every field, and a cast over `{}` would lie about that.
 */
export const emptyDerive: Derived = {
  series: 0,
  works: 0,
  editions: 0,
  members: 0,
  unattached_chapter_dirs: 0,
  unreadable: {
    total: 0,
    refused_format: 0,
    by_format: {},
    uncounted: [],
    vanished: [],
    no_pages: 0,
  },
  suppressed: 0,
  rekeyed: 0,
  kept_editions: 0,
  emptied_works: 0,
  unfolded_works: 0,
  unfoldable: [],
  unreadable_hand: [],
  repaired_documents: 0,
  unwritable_documents: [],
  orphan_documents: [],
  declined_documents: [],
  stale_documents: [],
  moved_positions: 0,
  lost_positions: 0,
  moved_notes: 0,
  dropped_notes: 0,
  moved_truth_pages: 0,
  moved_truth_collisions: 0,
  discarded_rows: 0,
  uncarried_alignments: 0,
  series_holding_no_work: 0,
  merged: 0,
  unrecognised_kinds: 0,
  raised: 0,
  root_removed_mid_derive: 0,
}
```

- [ ] **Step 6: Write the failing tests for the folder row and its dialog**

In `web/src/RootRow.test.tsx`, beside the scan-poll tests, using that file's
`settle`, its fake timers and its mocks:

```tsx
  it('polls its own status when the library starts a scan of it', async () => {
    vi.mocked(config.scanStatus).mockResolvedValue({
      root_id: 3,
      running: false,
      report: null,
      last_scan_at: null,
      last_scan_status: null,
    })
    const changed = vi.fn()
    const host = await settle({}, changed)
    expect(host.querySelector('.scanning')).toBeNull()
    vi.mocked(config.scanStatus).mockClear()

    await act(async () => {
      rerender(<RootRow root={root()} adopt={1} onChanged={changed} />)
      await Promise.resolve()
    })
    expect(host.querySelector('.scanning')).not.toBeNull()
    expect(config.startScan).not.toHaveBeenCalled()

    await act(async () => {
      vi.advanceTimersByTime(config.SCAN_POLL_MS)
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(config.scanStatus).toHaveBeenCalledWith(3)
  })

  it('does not start polling for an adopt it was mounted with', async () => {
    let host: HTMLElement | undefined
    await act(async () => {
      host = mount(<RootRow root={root()} adopt={4} onChanged={vi.fn()} />)
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(host?.querySelector('.scanning')).toBeNull()
  })

  it("draws a refusal's sentence, not the response it came in", async () => {
    vi.mocked(config.startScan).mockRejectedValue(
      new Error(
        '/api/library/roots/3/scan: 409 {"error":"a scan of root 3 (/srv/manga) is already running"}',
      ),
    )
    const host = await settle()
    const scan = [...host.querySelectorAll('button')].find(
      (b) => b.textContent === 'Scan now',
    )
    await act(async () => {
      scan?.click()
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(host.querySelector('[role="alert"]')?.textContent).toBe(
      'a scan of root 3 (/srv/manga) is already running',
    )
    // The 409 is still read from the message, so the row still polls.
    expect(host.querySelector('.scanning')).not.toBeNull()
  })
```

In `web/src/RemoveRoot.test.tsx`, using that file's mount helper and its
`removalCost` mock:

```tsx
  it('says the library goes too where this is its only folder', async () => {
    const host = await shown({ lastOf: 'Manga' })
    expect(host.querySelector('.last-folder')?.textContent).toBe(
      'This is the only folder of the library Manga, so removing it removes the library too.',
    )
    expect(
      host.querySelector('[role="group"]')?.getAttribute('aria-label'),
    ).toBe('Remove the folder at /srv/manga')
  })

  it('says nothing about a library where other folders stay', async () => {
    const host = await shown({})
    expect(host.querySelector('.last-folder')).toBeNull()
  })
```

`shown` here stands for that file's own mount helper, given a way to pass
`lastOf`; extend the helper it has. In the existing tests of both files and
of `ScanReport.test.tsx`, each assertion on a sentence Step 7 rewords takes
the new words.

Run: `pnpm -C web exec vitest run --project jsdom --reporter=default src/RootRow.test.tsx src/RemoveRoot.test.tsx`
Expected: FAIL on the new cases.

- [ ] **Step 7: Implement in `RootRow`, `RemoveRoot` and `ScanReport`**

`web/src/RootRow.tsx`:

- Import `sentenceOf` beside `messageOf`. Add `adopt` and `lastOf` to
  `RootRowProps` with the doc comments in this task's Interfaces, and to the
  destructured parameters.
- Every `setFailure(messageOf(err))` becomes `setFailure(sentenceOf(err))`.
  In `begin`'s catch, `const why = messageOf(err)` stays for
  `statusOf(why) === 409` and the slot takes `sentenceOf(err)`. The poll's
  `setScanNotice(messageOf(err))` becomes `sentenceOf(err)`.
- After the mount-read effect:

  ```tsx
  // **A scan this row did not start.** The library's own Scan now holds this
  // folder's registry slot, and its card bumps `adopt` when one starts. The
  // row then polls as it does after its own button. The value it was mounted
  // with is not a start: a card mounted mid-scan is adopted by the mount read
  // above.
  const adopted = useRef(adopt)
  useEffect(() => {
    if (adopt === adopted.current) return
    adopted.current = adopt
    setFailure(undefined)
    setScanNotice(undefined)
    setReport(null)
    setRemovalDerive(null)
    setElapsed(undefined)
    misses.current = 0
    setRunning(true)
  }, [adopt])
  ```

- `<RemoveRoot root={root} lastOf={lastOf} …>`.

`web/src/RemoveRoot.tsx`:

- `lastOf?: string` on `RemoveRootProps`: "The library's name, where this is
  its only folder: the library goes with it, and the dialog says so."
- `aria-label={`Remove the folder at ${root.path}`}`.
- `setFailure(sentenceOf(err))` in both catches.
- After the `.counts` paragraph:

  ```tsx
          {lastOf !== undefined && (
            <p className="last-folder">
              This is the only folder of the library {lastOf}, so removing it
              removes the library too.
            </p>
          )}
  ```

- The `<summary>` reads "What re-deriving without this folder would find".
- In the file's doc comment and in `.irreversible`'s sentence, "root" where
  it means the thing on screen becomes "folder".

`web/src/ScanReport.tsx`: the two labels become "Documents this folder no
longer keeps" and "Folders removed mid-derive".

- [ ] **Step 8: Run the projects, watch the defects, commit**

Run: `pnpm -C web run lint`
Run: `pnpm -C web run typecheck`
Run: `pnpm -C web exec vitest run --project node --project jsdom --reporter=default`
Run: `pnpm -C web run test:browser`
Expected: all pass. A browser case that asserted "root" in a sentence this
task reworded takes the new word.

Watch each fail, restoring between:

1. `RootRow`: initialise `const adopted = useRef<number | undefined>(undefined)`.
   Expect "does not start polling for an adopt it was mounted with" to fail.
2. `RootRow`: delete `setRunning(true)` from the adopt effect. Expect "polls
   its own status when the library starts a scan of it" to fail.
3. `RootRow`: in `begin`'s catch, read the status from the sentence,
   `statusOf(sentenceOf(err))`. Expect "draws a refusal's sentence" to fail at
   its last assertion.

```bash
git add web/src
git commit -m "The libraries client, a refusal drawn as its sentence, and a folder row a library's scan can adopt"
```

---

## Task 8: The Libraries screen

**Files:**
- Create: `web/src/Libraries.tsx`, `web/src/LibraryCard.tsx`,
  `web/src/RemoveLibrary.tsx`, `web/src/FolderPicker.tsx`, and a `.test.tsx`
  for each
- Modify: `web/src/config.ts` (`TABS`), `web/src/Config.tsx`,
  `web/src/Config.test.tsx`, `web/src/config.test.ts`
- Modify: `web/src/styles/admin.css`,
  `web/src/styles/admin-config.css.test.ts` (Step 6)

**Interfaces:**
- Consumes: Task 7's client, `sentenceOf`, `RootRow`'s `adopt` and `lastOf`,
  `testing/input.ts`; `config.ts`'s `Probe`, `Added`, `SCAN_POLL_MS`,
  `statusOf`.
- Produces:
  ```tsx
  // FolderPicker.tsx
  export interface FolderPickerProps {
    /** What the button says. */
    action: string
    /** The picked path. A rejected promise keeps the typed path for a retry. */
    onPick: (path: string) => unknown
  }
  export function FolderPicker(props: FolderPickerProps): React.ReactNode

  // LibraryCard.tsx
  export interface LibraryCardProps {
    library: Library
    /** Watch probes of folders added in this session, by path. */
    probes: Record<string, Probe>
    onAdded: (added: Added[]) => void
    onChanged: () => void
    /** The library is gone; its re-derive's report is the screen's to draw. */
    onRemoved: (name: string, answer: RemovalAnswer) => void
  }
  // RemoveLibrary.tsx
  export interface RemoveLibraryProps {
    library: Library
    onCancel: () => void
    onRemoved: (answer: RemovalAnswer) => void
  }
  // Libraries.tsx
  export function Libraries(): React.ReactNode
  ```
  `/config/libraries` draws `Libraries`; `/config/roots` redirects to it.
  Class names Task 11 styles: `.libraries`, `.library-list`, `.library-card`,
  `.library-card .holds`, `.no-readable`, `.rename-library`, `.add-library`,
  `.chosen-folders`, `.folder-picker`, `.remove-library`.

- [ ] **Step 1: `FolderPicker`, test first**

Create `web/src/FolderPicker.test.tsx`:

```tsx
import { describe, expect, it, vi } from 'vitest'
import { FolderPicker } from './FolderPicker'
import { flush } from './testing/async'
import { press, typeInto } from './testing/input'
import { mount } from './testing/mount'

const box = (host: HTMLElement) =>
  host.querySelector('input[name="path"]') as HTMLInputElement
const pick = (host: HTMLElement) =>
  host.querySelector('button[name="pick"]') as HTMLButtonElement

describe('FolderPicker', () => {
  it('hands over the typed path and empties the box once it was taken', async () => {
    const onPick = vi.fn()
    const host = mount(<FolderPicker action="Add folder" onPick={onPick} />)
    expect(pick(host).textContent).toBe('Add folder')
    expect(pick(host).disabled).toBe(true)

    await typeInto(box(host), '/media/Manga/Volumes')
    await press(pick(host))

    expect(onPick).toHaveBeenCalledWith('/media/Manga/Volumes')
    expect(box(host).value).toBe('')
  })

  it('keeps the typed path when the pick was refused', async () => {
    const onPick = vi.fn().mockRejectedValue(new Error('refused'))
    const host = mount(<FolderPicker action="Add folder" onPick={onPick} />)
    await typeInto(box(host), '/media/typo')
    await press(pick(host))
    expect(box(host).value).toBe('/media/typo')
  })

  it('picks on Enter, and keeps Enter from the form around it', async () => {
    const onPick = vi.fn()
    const host = mount(<FolderPicker action="Add to the list" onPick={onPick} />)
    await typeInto(box(host), '/media/a')
    // jsdom submits no form for a key event, so what is asserted is the
    // event's own default being prevented, which is what stops a browser.
    const enter = new KeyboardEvent('keydown', {
      key: 'Enter',
      bubbles: true,
      cancelable: true,
    })
    await flush(() => {
      box(host).dispatchEvent(enter)
    })
    expect(onPick).toHaveBeenCalledWith('/media/a')
    expect(enter.defaultPrevented).toBe(true)

    const other = new KeyboardEvent('keydown', {
      key: 'a',
      bubbles: true,
      cancelable: true,
    })
    await flush(() => {
      box(host).dispatchEvent(other)
    })
    expect(other.defaultPrevented).toBe(false)
    expect(onPick).toHaveBeenCalledTimes(1)
  })
})
```

Run: `pnpm -C web exec vitest run --project jsdom --reporter=default src/FolderPicker.test.tsx`
Expected: FAIL, the module does not exist.

Create `web/src/FolderPicker.tsx`:

```tsx
/**
 * Choosing one folder by its path.
 *
 * **A field and a button, not a form.** It is drawn inside the form that
 * makes a library, and a form inside a form is not one. Enter in the box
 * picks, and is kept from submitting whatever form is around it.
 *
 * **The box empties when the pick was taken, and not before.** A caller that
 * adds the folder at once answers a promise; a refusal rejects it, and the
 * typed path stays for the correction. A caller that only collects paths
 * answers nothing, and the box empties at once.
 *
 * No path is checked here. Whether the server can read a folder is a question
 * a browser cannot ask, and the refusal's own sentence says which check
 * failed.
 */
import { useState } from 'react'

export interface FolderPickerProps {
  /** What the button says. */
  action: string
  /** The picked path. A rejected promise keeps the typed path for a retry. */
  onPick: (path: string) => void | Promise<unknown>
}

export function FolderPicker({ action, onPick }: FolderPickerProps) {
  const [path, setPath] = useState('')

  const pick = () => {
    if (path === '') return
    Promise.resolve(onPick(path)).then(
      () => setPath(''),
      () => {
        // The caller drew the refusal. The path stays to be corrected.
      },
    )
  }

  return (
    <div className="folder-picker">
      {/* O175: the `aria-label` repeats the label's words, which `.field`
          draws in capitals. */}
      <label className="field">
        <span>Folder path</span>
        <input
          type="text"
          name="path"
          aria-label="Folder path"
          value={path}
          placeholder="/media/Manga/Volumes"
          onChange={(e) => setPath(e.target.value)}
          onKeyDown={(e) => {
            if (e.key !== 'Enter') return
            e.preventDefault()
            pick()
          }}
        />
      </label>
      <button type="button" name="pick" onClick={pick} disabled={path === ''}>
        {action}
      </button>
    </div>
  )
}
```

Run the same command. Expected: PASS. Watch it fail: replace the second
`.then` argument with `() => setPath('')`; expect "keeps the typed path" to
fail.

- [ ] **Step 2: `RemoveLibrary`, test first**

Create `web/src/RemoveLibrary.test.tsx`:

```tsx
import { act } from 'react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import type * as config from './config'
import * as api from './libraries'
import { RemoveLibrary } from './RemoveLibrary'
import { deferred, flush } from './testing/async'
import { emptyDerive } from './testing/derive'
import { press } from './testing/input'
import { mount } from './testing/mount'

vi.mock('./libraries', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./libraries')>()),
  libraryRemovalCost: vi.fn(),
  removeLibrary: vi.fn(),
}))

const folder = (id: number, path: string): config.Root => ({
  id,
  path,
  edition_key: null,
  default_kind: null,
  default_language: null,
  exclude_globs: null,
  scan_interval_secs: null,
  keeps_documents: false,
  last_scan_at: null,
  last_scan_status: null,
})

const library: api.Library = {
  id: 7,
  name: 'Manga',
  works: 12,
  books: 0,
  folders: [
    folder(1, '/media/Manga/Volumes'),
    folder(2, '/media/Manga/Chapters'),
  ],
}

const answer = (over: Partial<config.Removal> = {}): config.RemovalAnswer => ({
  removed: { works: 12, positions: 1, notes: 0, ...over },
  derive: emptyDerive,
  dry_run: true,
})

async function shown(onRemoved = vi.fn(), onCancel = vi.fn()) {
  let host: HTMLElement | undefined
  await act(async () => {
    host = mount(
      <RemoveLibrary library={library} onCancel={onCancel} onRemoved={onRemoved} />,
    )
    await Promise.resolve()
    await Promise.resolve()
  })
  if (!host) throw new Error('nothing mounted')
  return host
}

beforeEach(() => {
  vi.clearAllMocks()
  vi.mocked(api.libraryRemovalCost).mockResolvedValue(answer())
})

describe('RemoveLibrary', () => {
  it('states what goes before it offers to remove anything', async () => {
    const held = deferred<config.RemovalAnswer>()
    vi.mocked(api.libraryRemovalCost).mockReturnValue(held.promise)
    const host = await shown()
    expect(host.querySelector('.confirm')).toBeNull()
    expect(host.textContent).toContain('Working out what this would remove')

    await flush(() => held.resolve(answer()))

    expect(host.querySelector('.counts')?.textContent).toBe(
      'Removing Manga removes its 2 folders, 12 works, 1 in-progress position and 0 notes.',
    )
    expect(host.querySelector('.confirm')?.textContent).toBe('Remove Manga')
    expect(api.libraryRemovalCost).toHaveBeenCalledWith(7)
  })

  it("draws the refusal's sentence and offers no confirm", async () => {
    vi.mocked(api.libraryRemovalCost).mockRejectedValue(
      new Error(
        '/api/library/libraries/7?dry_run=true: 409 {"error":"`Manga` is the only library `sam` is granted."}',
      ),
    )
    const host = await shown()
    expect(host.querySelector('[role="alert"]')?.textContent).toBe(
      '`Manga` is the only library `sam` is granted.',
    )
    expect(host.querySelector('.confirm')).toBeNull()
    expect(host.textContent).not.toContain('Working out')
  })

  it('sends one removal for two presses, and hands back what it did', async () => {
    const real = { ...answer({ works: 11 }), dry_run: false }
    const held = deferred<config.RemovalAnswer>()
    vi.mocked(api.removeLibrary).mockReturnValue(held.promise)
    const onRemoved = vi.fn()
    const host = await shown(onRemoved)
    const confirm = host.querySelector('.confirm') as HTMLButtonElement

    act(() => {
      confirm.click()
      confirm.click()
    })
    await flush(() => held.resolve(real))

    expect(api.removeLibrary).toHaveBeenCalledTimes(1)
    expect(onRemoved).toHaveBeenCalledWith(real)
  })

  it('keeps the dialog and says why when the removal fails', async () => {
    vi.mocked(api.removeLibrary).mockRejectedValue(
      new Error('/api/library/libraries/7: 500 the disk is full'),
    )
    const onRemoved = vi.fn()
    const host = await shown(onRemoved)
    await press(host.querySelector('.confirm') as Element)
    expect(host.querySelector('[role="alert"]')?.textContent).toContain(
      'the disk is full',
    )
    expect(onRemoved).not.toHaveBeenCalled()
    expect((host.querySelector('.confirm') as HTMLButtonElement).disabled).toBe(false)
  })
})
```

Run it. Expected: FAIL, the module does not exist.

Create `web/src/RemoveLibrary.tsx`:

```tsx
/**
 * Removing a library, and what it costs.
 *
 * `RemoveRoot`'s rules, one level up: the counts are stated before the
 * confirm exists, zero is drawn, the real `DELETE` is sent from here and
 * nowhere else, and the re-derive's own report is drawn beneath the counts.
 *
 * **The counts are the library's, not its folders' added up.** A work with
 * an edition in each of two folders is in neither folder's own count, and
 * the server counts it once for the library.
 *
 * **A refusal is the sentence and nothing else.** A library that is some
 * account's only grant is not removed, and the server names the accounts.
 */
import { useEffect, useRef, useState } from 'react'
import type { RemovalAnswer } from './config'
import { type Library, libraryRemovalCost, removeLibrary } from './libraries'
import { sentenceOf } from './message'
import { DeriveSection } from './ScanReport'

/** `n` of a thing, in the number the count is. Zero takes the plural. */
function count(n: number, one: string, many: string): string {
  return `${n} ${n === 1 ? one : many}`
}

export interface RemoveLibraryProps {
  library: Library
  onCancel: () => void
  /** Called once the library is gone, with what the removal did. */
  onRemoved: (answer: RemovalAnswer) => void
}

export function RemoveLibrary({
  library,
  onCancel,
  onRemoved,
}: RemoveLibraryProps) {
  const [cost, setCost] = useState<RemovalAnswer | undefined>()
  const [failure, setFailure] = useState<string | undefined>()
  const [removing, setRemoving] = useState(false)
  /** A ref as well as `removing`: two presses before a render both read false. */
  const inFlight = useRef(false)

  useEffect(() => {
    let cancelled = false
    libraryRemovalCost(library.id)
      .then((answer) => {
        if (cancelled) return
        setCost(answer)
        setFailure(undefined)
      })
      .catch((err: unknown) => {
        if (cancelled) return
        setFailure(sentenceOf(err))
      })
    return () => {
      cancelled = true
    }
  }, [library.id])

  const confirm = () => {
    if (inFlight.current) return
    inFlight.current = true
    setRemoving(true)
    removeLibrary(library.id)
      .then((answer) => onRemoved(answer))
      .catch((err: unknown) => {
        inFlight.current = false
        setRemoving(false)
        setFailure(sentenceOf(err))
      })
  }

  return (
    <div
      className="remove-library"
      role="group"
      aria-label={`Remove the library ${library.name}`}
    >
      {failure !== undefined && <p role="alert">{failure}</p>}
      {cost === undefined ? (
        failure === undefined && <p>Working out what this would remove…</p>
      ) : (
        <>
          <p className="counts">
            Removing {library.name} removes its{' '}
            {count(library.folders.length, 'folder', 'folders')},{' '}
            {count(cost.removed.works, 'work', 'works')},{' '}
            {count(
              cost.removed.positions,
              'in-progress position',
              'in-progress positions',
            )}{' '}
            and {count(cost.removed.notes, 'note', 'notes')}.
          </p>
          <p className="irreversible">
            Nothing here can be undone from this screen. The files on disk are
            untouched; what goes is everything this library derived from them,
            and everything anybody recorded against it.
          </p>
          <button
            type="button"
            className="confirm"
            onClick={confirm}
            disabled={removing}
          >
            {removing ? 'Removing…' : `Remove ${library.name}`}
          </button>
          <details className="removal-derive">
            <summary>What re-deriving without this library would find</summary>
            <DeriveSection derive={cost.derive} />
          </details>
        </>
      )}
      <button type="button" onClick={onCancel} disabled={removing}>
        Cancel
      </button>
    </div>
  )
}
```

Run it. Expected: PASS. Watch it fail: delete `if (inFlight.current) return`;
expect "sends one removal for two presses" to fail with 2 calls.

- [ ] **Step 3: `LibraryCard`, test first**

Create `web/src/LibraryCard.test.tsx`:

```tsx
import { act } from 'react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import * as config from './config'
import * as api from './libraries'
import { LibraryCard } from './LibraryCard'
import { flush } from './testing/async'
import { emptyDerive } from './testing/derive'
import { press, submit, typeInto } from './testing/input'
import { mount } from './testing/mount'

vi.mock('./config', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./config')>()),
  patchRoot: vi.fn(),
  removalCost: vi.fn(),
  removeRoot: vi.fn(),
  scanStatus: vi.fn(),
  startScan: vi.fn(),
}))
vi.mock('./libraries', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./libraries')>()),
  addFolder: vi.fn(),
  libraryRemovalCost: vi.fn(),
  libraryScanStatus: vi.fn(),
  removeLibrary: vi.fn(),
  renameLibrary: vi.fn(),
  startLibraryScan: vi.fn(),
}))

const folder = (id: number, path: string): config.Root => ({
  id,
  path,
  edition_key: null,
  default_kind: null,
  default_language: null,
  exclude_globs: null,
  scan_interval_secs: 86400,
  keeps_documents: false,
  last_scan_at: null,
  last_scan_status: null,
})

const manga = (over: Partial<api.Library> = {}): api.Library => ({
  id: 7,
  name: 'Manga',
  works: 12,
  books: 0,
  folders: [folder(1, '/media/Manga/Volumes'), folder(2, '/media/Manga/Chapters')],
  ...over,
})

const idle = (id: number): api.LibraryScanStatus => ({
  library_id: id,
  running: false,
  elapsed_secs: null,
  folders: [],
})

async function shown(library = manga()) {
  const onAdded = vi.fn()
  const onChanged = vi.fn()
  let host: HTMLElement | undefined
  await act(async () => {
    host = mount(
      <ul>
        <LibraryCard library={library} probes={{}} onAdded={onAdded} onChanged={onChanged} />
      </ul>,
    )
    await Promise.resolve()
    await Promise.resolve()
  })
  if (!host) throw new Error('nothing mounted')
  return { host, onAdded, onChanged }
}

const button = (host: HTMLElement, name: string) =>
  host.querySelector(`button[name="${name}"]`) as HTMLButtonElement

beforeEach(() => {
  vi.clearAllMocks()
  vi.useFakeTimers({ shouldAdvanceTime: true })
  vi.mocked(api.libraryScanStatus).mockResolvedValue(idle(7))
  vi.mocked(config.scanStatus).mockImplementation((id) =>
    Promise.resolve({
      root_id: id,
      running: false,
      report: null,
      last_scan_at: null,
      last_scan_status: null,
    }),
  )
})

afterEach(() => {
  vi.useRealTimers()
})

describe('LibraryCard', () => {
  it('names the library, what it holds, and a row per folder', async () => {
    const { host } = await shown()
    expect(host.querySelector('.library-card > h3')?.textContent).toBe('Manga')
    expect(host.querySelector('.holds')?.textContent).toBe('12 works in 2 folders')
    expect(
      [...host.querySelectorAll('.root-row code')].map((c) => c.textContent),
    ).toEqual(['/media/Manga/Volumes', '/media/Manga/Chapters'])
    expect(host.querySelector('.no-readable')).toBeNull()
  })

  it('says so when it holds books and no work, and only then', async () => {
    const books = await shown(manga({ works: 0, books: 3, folders: [folder(1, '/media/Books')] }))
    expect(books.host.querySelector('.holds')?.textContent).toBe('0 works in 1 folder')
    expect(books.host.querySelector('.no-readable')?.textContent).toContain(
      'No readable files yet: its scans found 3 EPUB or PDF files',
    )
    const mixed = await shown(manga({ works: 2, books: 3 }))
    expect(mixed.host.querySelector('.no-readable')).toBeNull()
    const empty = await shown(manga({ works: 0, books: 0 }))
    expect(empty.host.querySelector('.no-readable')).toBeNull()
  })

  it('tells a folder it is the last one, and no folder of two', async () => {
    vi.mocked(config.removalCost).mockResolvedValue({
      removed: { works: 0, positions: 0, notes: 0 },
      derive: emptyDerive,
      dry_run: true,
    })
    const removeFolder = (host: HTMLElement) =>
      [...host.querySelectorAll('.root-row button')].find(
        (b) => b.textContent === 'Remove…',
      ) as Element

    const one = await shown(manga({ folders: [folder(1, '/media/Manga/Volumes')] }))
    await press(removeFolder(one.host))
    expect(one.host.querySelector('.last-folder')?.textContent).toContain(
      'the only folder of the library Manga',
    )

    const two = await shown()
    await press(removeFolder(two.host))
    expect(two.host.querySelector('.remove-root .counts')).not.toBeNull()
    expect(two.host.querySelector('.last-folder')).toBeNull()
  })

  it('renames by id and reloads', async () => {
    vi.mocked(api.renameLibrary).mockResolvedValue({ id: 7, name: 'Comics' })
    const { host, onChanged } = await shown()
    await press(button(host, 'rename-library'))
    const box = host.querySelector('input[name="library-name"]') as HTMLInputElement
    expect(box.value).toBe('Manga')

    await typeInto(box, 'Comics')
    await submit(host.querySelector('form.rename-library') as Element)

    expect(api.renameLibrary).toHaveBeenCalledWith(7, 'Comics')
    expect(onChanged).toHaveBeenCalled()
    expect(host.querySelector('form.rename-library')).toBeNull()
  })

  it('keeps the rename open and says why when the name is taken', async () => {
    vi.mocked(api.renameLibrary).mockRejectedValue(
      new Error('/api/library/libraries/7: 400 {"error":"a library is already called `Comics`"}'),
    )
    const { host, onChanged } = await shown()
    await press(button(host, 'rename-library'))
    await typeInto(host.querySelector('input[name="library-name"]') as HTMLInputElement, 'Comics')
    await submit(host.querySelector('form.rename-library') as Element)

    expect(host.querySelector('.library-card > [role="alert"]')?.textContent).toBe(
      'a library is already called `Comics`',
    )
    expect(host.querySelector('form.rename-library')).not.toBeNull()
    expect(onChanged).not.toHaveBeenCalled()
  })

  it('adds a folder to this library, hands its probe up, and reloads', async () => {
    const added: config.Added = {
      root: folder(3, '/media/Manga/Extras'),
      watching: true,
      watch_reason: null,
      interval_secs: 86400,
    }
    vi.mocked(api.addFolder).mockResolvedValue(added)
    const { host, onAdded, onChanged } = await shown()
    const picker = host.querySelector('.library-card > .add-root') as HTMLElement

    await typeInto(picker.querySelector('input[name="path"]') as HTMLInputElement, '/media/Manga/Extras')
    await press(picker.querySelector('button[name="pick"]') as Element)

    expect(api.addFolder).toHaveBeenCalledWith(7, '/media/Manga/Extras')
    expect(onAdded).toHaveBeenCalledWith([added])
    expect(onChanged).toHaveBeenCalled()
  })

  it("draws a refused folder's sentence and keeps what was typed", async () => {
    vi.mocked(api.addFolder).mockRejectedValue(
      new Error(
        '/api/library/libraries/7/folders: 400 {"error":"/media/Manga/Volumes/x is inside existing root /media/Manga/Volumes (id 1)"}',
      ),
    )
    const { host, onChanged } = await shown()
    const picker = host.querySelector('.library-card > .add-root') as HTMLElement
    const box = picker.querySelector('input[name="path"]') as HTMLInputElement

    await typeInto(box, '/media/Manga/Volumes/x')
    await press(picker.querySelector('button[name="pick"]') as Element)

    expect(host.querySelector('.library-card > [role="alert"]')?.textContent).toContain(
      'is inside existing root',
    )
    expect(box.value).toBe('/media/Manga/Volumes/x')
    expect(onChanged).not.toHaveBeenCalled()
  })

  it('starts one scan of the library, and its folder rows poll for themselves', async () => {
    vi.mocked(api.startLibraryScan).mockResolvedValue({
      library_id: 7,
      status_url: '/api/library/libraries/7/scan',
    })
    const { host, onChanged } = await shown()
    vi.mocked(config.scanStatus).mockClear()
    // Each folder's own status says running for as long as this test runs,
    // so no row reports a finished scan: the reload asserted at the end is
    // the card's.
    vi.mocked(config.scanStatus).mockImplementation((id) =>
      Promise.resolve({
        root_id: id,
        running: true,
        elapsed_secs: 2.4,
        report: null,
        last_scan_at: null,
        last_scan_status: null,
      }),
    )
    vi.mocked(api.libraryScanStatus).mockResolvedValue({
      ...idle(7),
      running: true,
      elapsed_secs: 2.4,
    })

    act(() => {
      button(host, 'scan-library').click()
      button(host, 'scan-library').click()
    })
    await flush()
    expect(api.startLibraryScan).toHaveBeenCalledTimes(1)
    expect(api.startLibraryScan).toHaveBeenCalledWith(7)
    expect(button(host, 'scan-library').disabled).toBe(true)
    expect(button(host, 'remove-library').disabled).toBe(true)
    expect(config.startScan).not.toHaveBeenCalled()

    await act(async () => {
      vi.advanceTimersByTime(config.SCAN_POLL_MS)
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(host.querySelector('.library-card > .scanning')?.textContent).toContain('2s')
    expect(vi.mocked(config.scanStatus).mock.calls.map(([id]) => id).sort()).toEqual([1, 2])
    expect(onChanged).not.toHaveBeenCalled()

    vi.mocked(api.libraryScanStatus).mockResolvedValue(idle(7))
    await act(async () => {
      vi.advanceTimersByTime(config.SCAN_POLL_MS)
      await Promise.resolve()
      await Promise.resolve()
    })
    expect(host.querySelector('.library-card > .scanning')).toBeNull()
    expect(button(host, 'scan-library').disabled).toBe(false)
    expect(onChanged).toHaveBeenCalledTimes(1)
  })

  it('takes up a scan that was running when it was drawn', async () => {
    vi.mocked(api.libraryScanStatus).mockResolvedValue({
      ...idle(7),
      running: true,
      elapsed_secs: 41,
    })
    const { host } = await shown()
    expect(host.querySelector('.library-card > .scanning')?.textContent).toContain('41s')
    expect(button(host, 'scan-library').disabled).toBe(true)
  })

  it('reads a 409 as a scan in flight: the sentence, and a poll', async () => {
    vi.mocked(api.startLibraryScan).mockRejectedValue(
      new Error(
        '/api/library/libraries/7/scan: 409 {"error":"a scan of /media/Manga/Chapters is already running"}',
      ),
    )
    const { host } = await shown()
    await press(button(host, 'scan-library'))
    expect(host.querySelector('.library-card > [role="alert"]')?.textContent).toBe(
      'a scan of /media/Manga/Chapters is already running',
    )
    expect(host.querySelector('.library-card > .scanning')).not.toBeNull()
  })

  it('opens the removal dialog, and reloads once the library is gone', async () => {
    vi.mocked(api.libraryRemovalCost).mockReturnValue(new Promise(() => {}))
    const { host } = await shown()
    await press(button(host, 'remove-library'))
    expect(
      host.querySelector('.remove-library')?.getAttribute('aria-label'),
    ).toBe('Remove the library Manga')
    expect(button(host, 'scan-library').disabled).toBe(true)
  })
})
```

Run it. Expected: FAIL, the module does not exist.

Create `web/src/LibraryCard.tsx`:

```tsx
/**
 * One library: its name, what it holds, a row per folder, and what is done
 * to it whole.
 *
 * **The card owns the library's writes and its folder rows own theirs.** A
 * rename, an added folder, a scan of the whole library and its removal are
 * sent from here. A folder's settings, its own scan and its removal are
 * `RootRow`'s, unchanged.
 *
 * **A scan of the library is watched twice, and that is the design.** The
 * server holds each folder's slot in its scan registry while the library's
 * scan runs, so each row's own status route answers for it. The card bumps
 * `adopt`, each row polls and draws its own report, and the card polls the
 * library's status for the one thing only it draws: whether Scan now and
 * Remove are live.
 *
 * **It asks the library's status once on mount**, for `RootRow`'s reason: a
 * scan started before this card was drawn is still running.
 *
 * **A 409 from Scan now is proof a scan is in flight**, of the library or of
 * one folder, so the card draws the sentence and polls.
 */
import { useEffect, useRef, useState } from 'react'
import { type Added, type Probe, SCAN_POLL_MS, statusOf } from './config'
import { FolderPicker } from './FolderPicker'
import {
  addFolder,
  type Library,
  libraryScanStatus,
  renameLibrary,
  startLibraryScan,
} from './libraries'
import { messageOf, sentenceOf } from './message'
import { RemoveLibrary } from './RemoveLibrary'
import { RootRow } from './RootRow'

export interface LibraryCardProps {
  library: Library
  /** Watch probes of folders added in this session, by path. */
  probes: Record<string, Probe>
  /** A folder was added here: its probe is the screen's to keep. */
  onAdded: (added: Added[]) => void
  /** The list reloads on a rename, an added folder, a finished scan or a removal. */
  onChanged: () => void
}

/** How many consecutive failed polls a running scan survives, as `RootRow`'s. */
const POLL_MISSES_ALLOWED = 3

/** `n` of a thing, in the number the count is. Zero takes the plural. */
function count(n: number, one: string, many: string): string {
  return `${n} ${n === 1 ? one : many}`
}

export function LibraryCard({
  library,
  probes,
  onAdded,
  onChanged,
}: LibraryCardProps) {
  /** Why the card's last write, or its scan poll, failed. */
  const [failure, setFailure] = useState<string | undefined>()
  const [renaming, setRenaming] = useState(false)
  const [name, setName] = useState(library.name)
  const [saving, setSaving] = useState(false)
  const [removing, setRemoving] = useState(false)
  const [running, setRunning] = useState(false)
  const [starting, setStarting] = useState(false)
  const [elapsed, setElapsed] = useState<number | null>(null)
  /** Bumped when this card starts a scan. See `RootRow`'s `adopt`. */
  const [adopt, setAdopt] = useState(0)
  /** A ref as well as the flags: two presses before a render both read false. */
  const inFlight = useRef(false)
  const misses = useRef(0)
  // Held in a ref for the reason `RootRow` holds its own: the poll's interval
  // must not restart each time the parent hands over a fresh closure.
  const changed = useRef(onChanged)
  changed.current = onChanged

  useEffect(() => {
    let cancelled = false
    libraryScanStatus(library.id)
      .then((status) => {
        if (cancelled || !status.running) return
        setElapsed(status.elapsed_secs)
        setRunning(true)
      })
      .catch(() => {
        // Silent, as `RootRow`'s mount read is: pressing Scan now is what
        // surfaces a real fault.
      })
    return () => {
      cancelled = true
    }
  }, [library.id])

  useEffect(() => {
    if (!running) return
    let cancelled = false
    const timer = setInterval(() => {
      libraryScanStatus(library.id)
        .then((status) => {
          if (cancelled) return
          misses.current = 0
          setElapsed(status.elapsed_secs)
          if (status.running) return
          setRunning(false)
          changed.current()
        })
        .catch((err: unknown) => {
          if (cancelled) return
          misses.current += 1
          if (misses.current < POLL_MISSES_ALLOWED) return
          setRunning(false)
          setFailure(sentenceOf(err))
        })
    }, SCAN_POLL_MS)
    return () => {
      cancelled = true
      clearInterval(timer)
    }
  }, [running, library.id])

  const scan = () => {
    if (inFlight.current) return
    inFlight.current = true
    setStarting(true)
    setFailure(undefined)
    misses.current = 0
    startLibraryScan(library.id)
      .then(() => {
        inFlight.current = false
        setStarting(false)
        setRunning(true)
        setAdopt((n) => n + 1)
      })
      .catch((err: unknown) => {
        inFlight.current = false
        setStarting(false)
        setFailure(sentenceOf(err))
        if (statusOf(messageOf(err)) === 409) setRunning(true)
      })
  }

  const rename = (e: React.FormEvent) => {
    e.preventDefault()
    if (name === '' || inFlight.current) return
    inFlight.current = true
    setSaving(true)
    setFailure(undefined)
    renameLibrary(library.id, name)
      .then(() => {
        inFlight.current = false
        setSaving(false)
        setRenaming(false)
        onChanged()
      })
      .catch((err: unknown) => {
        inFlight.current = false
        setSaving(false)
        setFailure(sentenceOf(err))
      })
  }

  /** Rejects on a refusal, which is what keeps the picker's typed path. */
  const add = (path: string) =>
    addFolder(library.id, path).then(
      (added) => {
        setFailure(undefined)
        onAdded([added])
        onChanged()
      },
      (err: unknown) => {
        setFailure(sentenceOf(err))
        throw err
      },
    )

  const busy = running || starting || removing

  return (
    <li className="library-card">
      <h3>{library.name}</h3>
      <p className="holds">
        {count(library.works, 'work', 'works')} in{' '}
        {count(library.folders.length, 'folder', 'folders')}
      </p>
      {library.works === 0 && library.books > 0 && (
        <p className="no-readable">
          No readable files yet: its scans found{' '}
          {count(library.books, 'EPUB or PDF file', 'EPUB or PDF files')}, and
          reading those arrives with the books work. A rescan is all this
          library will need then.
        </p>
      )}
      {failure !== undefined && <p role="alert">{failure}</p>}

      <div className="actions">
        <button
          type="button"
          name="scan-library"
          onClick={scan}
          disabled={busy}
        >
          Scan now
        </button>
        <button
          type="button"
          name="rename-library"
          onClick={() => {
            setName(library.name)
            setRenaming(true)
          }}
          disabled={renaming}
        >
          Rename…
        </button>
        <button
          type="button"
          name="remove-library"
          onClick={() => setRemoving(true)}
          disabled={busy}
        >
          Remove library…
        </button>
      </div>
      {running && (
        <p className="scanning">
          Scanning this library's folders…
          {elapsed === null ? '' : ` ${Math.round(elapsed)}s`}
        </p>
      )}

      {renaming && (
        <form className="rename-library" onSubmit={rename}>
          {/* O175: the `aria-label` repeats the label's words. */}
          <label className="field">
            <span>Name</span>
            <input
              type="text"
              name="library-name"
              aria-label="Name"
              value={name}
              onChange={(e) => setName(e.target.value)}
            />
          </label>
          <button type="submit" disabled={saving || name === ''}>
            {saving ? 'Saving…' : 'Save'}
          </button>
          <button
            type="button"
            onClick={() => setRenaming(false)}
            disabled={saving}
          >
            Cancel
          </button>
        </form>
      )}

      {removing && (
        <RemoveLibrary
          library={library}
          onCancel={() => setRemoving(false)}
          onRemoved={() => {
            setRemoving(false)
            onChanged()
          }}
        />
      )}

      <ul className="root-list">
        {library.folders.map((root) => (
          <RootRow
            // By path, not id: a freed `root.id` is given to the next folder
            // added, and a path cannot collide, since no two folders overlap.
            key={root.path}
            root={root}
            probe={probes[root.path]}
            adopt={adopt}
            lastOf={library.folders.length === 1 ? library.name : undefined}
            onChanged={onChanged}
          />
        ))}
      </ul>

      <div className="add-root">
        <FolderPicker action="Add folder" onPick={add} />
      </div>
    </li>
  )
}
```

Run it. Expected: PASS. Watch each fail, restoring between:

1. Delete `setAdopt((n) => n + 1)`. Expect "starts one scan of the library,
   and its folder rows poll for themselves" to fail: no row asks its status.
2. In `add`, delete `throw err`. Expect "draws a refused folder's sentence
   and keeps what was typed" to fail on the box's value.
3. Replace `library.works === 0 && library.books > 0` with
   `library.books > 0`. Expect "says so when it holds books and no work, and
   only then" to fail.
4. In `scan`'s catch, delete the `409` line. Expect "reads a 409 as a scan in
   flight" to fail.

- [ ] **Step 4: `Libraries`, test first**

Create `web/src/Libraries.test.tsx`:

```tsx
import { act } from 'react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import * as config from './config'
import { Libraries } from './Libraries'
import * as api from './libraries'
import { deferred, flush } from './testing/async'
import { press, submit, typeInto } from './testing/input'
import { mount } from './testing/mount'

vi.mock('./config', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./config')>()),
  scanStatus: vi.fn(),
}))
vi.mock('./libraries', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./libraries')>()),
  createLibrary: vi.fn(),
  libraryScanStatus: vi.fn(),
  listLibraries: vi.fn(),
  renameLibrary: vi.fn(),
}))

const folder = (id: number, path: string): config.Root => ({
  id,
  path,
  edition_key: null,
  default_kind: null,
  default_language: null,
  exclude_globs: null,
  scan_interval_secs: 86400,
  keeps_documents: false,
  last_scan_at: null,
  last_scan_status: null,
})

const library = (id: number, name: string, paths: string[]): api.Library => ({
  id,
  name,
  works: 0,
  books: 0,
  folders: paths.map((p, i) => folder(id * 10 + i, p)),
})

async function shown() {
  let host: HTMLElement | undefined
  await act(async () => {
    host = mount(<Libraries />)
    await Promise.resolve()
    await Promise.resolve()
  })
  if (!host) throw new Error('nothing mounted')
  return host
}

const form = (host: HTMLElement) =>
  host.querySelector('form.add-library') as HTMLFormElement

async function choose(host: HTMLElement, path: string) {
  await typeInto(form(host).querySelector('input[name="path"]') as HTMLInputElement, path)
  await press(form(host).querySelector('button[name="pick"]') as Element)
}

beforeEach(() => {
  vi.clearAllMocks()
  vi.mocked(api.listLibraries).mockResolvedValue({ libraries: [] })
  vi.mocked(api.libraryScanStatus).mockImplementation((id) =>
    Promise.resolve({ library_id: id, running: false, elapsed_secs: null, folders: [] }),
  )
  vi.mocked(config.scanStatus).mockImplementation((id) =>
    Promise.resolve({
      root_id: id,
      running: false,
      report: null,
      last_scan_at: null,
      last_scan_status: null,
    }),
  )
})

describe('Libraries', () => {
  it('heads itself, and says so when there is no library yet', async () => {
    const host = await shown()
    expect(host.querySelector('.libraries > h2')?.textContent).toBe('Libraries')
    expect(host.querySelector('.empty')?.textContent).toContain('No libraries yet')
    expect(host.querySelector('.library-list')).toBeNull()
  })

  it('draws a card per library, in the order sent', async () => {
    vi.mocked(api.listLibraries).mockResolvedValue({
      libraries: [
        library(2, 'Comics', ['/media/Comics']),
        library(1, 'Manga', ['/media/Manga/Volumes', '/media/Manga/Chapters']),
      ],
    })
    const host = await shown()
    expect(
      [...host.querySelectorAll('.library-card > h3')].map((h) => h.textContent),
    ).toEqual(['Comics', 'Manga'])
    expect(host.querySelector('.empty')).toBeNull()
  })

  it('makes a library from a name and the folders chosen, then reloads', async () => {
    const made = library(1, 'Manga', ['/media/Manga/Volumes', '/media/Manga/Chapters'])
    vi.mocked(api.createLibrary).mockResolvedValue({
      library: made,
      added: [
        { root: made.folders[0], watching: true, watch_reason: null, interval_secs: 86400 },
        { root: made.folders[1], watching: false, watch_reason: 'no events', interval_secs: 3600 },
      ],
    })
    const host = await shown()
    const make = form(host).querySelector('button[type="submit"]') as HTMLButtonElement
    expect(make.disabled).toBe(true)

    await typeInto(form(host).querySelector('input[name="library-name"]') as HTMLInputElement, 'Manga')
    expect(make.disabled).toBe(true)
    await choose(host, '/media/Manga/Volumes')
    await choose(host, '/media/Manga/Chapters')
    await choose(host, '/media/Manga/Volumes')
    expect(
      [...form(host).querySelectorAll('.chosen-folders code')].map((c) => c.textContent),
    ).toEqual(['/media/Manga/Volumes', '/media/Manga/Chapters'])

    vi.mocked(api.listLibraries).mockResolvedValue({ libraries: [made] })
    await submit(form(host))

    expect(api.createLibrary).toHaveBeenCalledWith('Manga', [
      '/media/Manga/Volumes',
      '/media/Manga/Chapters',
    ])
    expect(host.querySelector('.library-card > h3')?.textContent).toBe('Manga')
    expect(form(host).querySelector('.chosen-folders')).toBeNull()
    // The probe each folder was added with is kept for its row.
    expect(
      [...host.querySelectorAll('.root-row .regime dd')].map((d) => d.textContent),
    ).toEqual(
      expect.arrayContaining(['watching', 'probe failed: no events']),
    )
  })

  it('takes a chosen folder back off the list', async () => {
    const host = await shown()
    await choose(host, '/media/a')
    await choose(host, '/media/b')
    await press(form(host).querySelector('button[name="unchoose"]') as Element)
    expect(
      [...form(host).querySelectorAll('.chosen-folders code')].map((c) => c.textContent),
    ).toEqual(['/media/b'])
  })

  it("draws a refusal's sentence and keeps what was entered", async () => {
    vi.mocked(api.createLibrary).mockRejectedValue(
      new Error('/api/library/libraries: 400 {"error":"/media/typo: No such file or directory (os error 2)"}'),
    )
    const host = await shown()
    await typeInto(form(host).querySelector('input[name="library-name"]') as HTMLInputElement, 'Manga')
    await choose(host, '/media/typo')
    await submit(form(host))

    expect(host.querySelector('.libraries > [role="alert"]')?.textContent).toBe(
      '/media/typo: No such file or directory (os error 2)',
    )
    expect(
      (form(host).querySelector('input[name="library-name"]') as HTMLInputElement).value,
    ).toBe('Manga')
    expect(form(host).querySelectorAll('.chosen-folders li')).toHaveLength(1)
  })

  it('keeps a create refusal when a card reloads the list', async () => {
    vi.mocked(api.listLibraries).mockResolvedValue({
      libraries: [library(2, 'Comics', ['/media/Comics'])],
    })
    vi.mocked(api.createLibrary).mockRejectedValue(
      new Error('/api/library/libraries: 400 {"error":"a library is already called `Comics`"}'),
    )
    vi.mocked(api.renameLibrary).mockResolvedValue({ id: 2, name: 'Comics Two' })
    const host = await shown()
    await typeInto(form(host).querySelector('input[name="library-name"]') as HTMLInputElement, 'Comics')
    await choose(host, '/media/x')
    await submit(form(host))
    expect(host.querySelector('.libraries > [role="alert"]')).not.toBeNull()

    // A card's rename reloads the list. That read must not clear the form's
    // refusal.
    vi.mocked(api.listLibraries).mockResolvedValue({
      libraries: [library(2, 'Comics Two', ['/media/Comics'])],
    })
    await press(host.querySelector('button[name="rename-library"]') as Element)
    await typeInto(
      host.querySelector('form.rename-library input[name="library-name"]') as HTMLInputElement,
      'Comics Two',
    )
    await submit(host.querySelector('form.rename-library') as Element)

    expect(host.querySelector('.library-card > h3')?.textContent).toBe('Comics Two')
    expect(host.querySelector('.libraries > [role="alert"]')?.textContent).toContain(
      'already called',
    )
  })

  it('draws the newest list when two reads answer out of order', async () => {
    const first = deferred<{ libraries: api.Library[] }>()
    vi.mocked(api.listLibraries).mockReturnValueOnce(first.promise)
    const made = library(1, 'Manga', ['/media/Manga'])
    vi.mocked(api.createLibrary).mockResolvedValue({ library: made, added: [] })
    const host = await shown()
    vi.mocked(api.listLibraries).mockResolvedValue({ libraries: [made] })
    await typeInto(form(host).querySelector('input[name="library-name"]') as HTMLInputElement, 'Manga')
    await choose(host, '/media/Manga')
    await submit(form(host))
    expect(host.querySelector('.library-card > h3')?.textContent).toBe('Manga')

    await flush(() => first.resolve({ libraries: [] }))
    expect(host.querySelector('.library-card > h3')?.textContent).toBe('Manga')
  })

  it('says why when the list cannot be read', async () => {
    vi.mocked(api.listLibraries).mockRejectedValue(
      new Error('/api/library/libraries: 500 the store is locked'),
    )
    const host = await shown()
    expect(host.querySelector('.libraries > [role="alert"]')?.textContent).toContain(
      'the store is locked',
    )
  })
})
```

Run it. Expected: FAIL, the module does not exist.

Create `web/src/Libraries.tsx`:

```tsx
/**
 * The libraries screen: Configuration → Libraries.
 *
 * **The list owns which libraries there are; each card owns what it is
 * doing**, as `Users` and `UserRow` split it.
 *
 * **A library is made with its folders.** There is no empty library: a grant
 * names a library and resolves to its folders, and one with none would read
 * as a grant on everything. So the form collects a name and one or more
 * folders and sends them together, and the server makes all of it or none.
 *
 * **Two failure slots.** `reload` is every card's `onChanged`, so one slot
 * would let a card's rename erase the form's refusal, which is the one
 * message naming which folder was refused and why.
 *
 * **The probe from an add is kept, by the folder's path.** The watch probe
 * runs when a folder is added and at no other time, so a folder added in
 * this session has a watch state and one read from the list does not.
 *
 * **Reads carry a sequence number.** Several are routinely in flight, and
 * they need not answer in order.
 */
import { useCallback, useEffect, useRef, useState } from 'react'
import type { Added, Probe } from './config'
import { FolderPicker } from './FolderPicker'
import { createLibrary, type Library, listLibraries } from './libraries'
import { LibraryCard } from './LibraryCard'
import { sentenceOf } from './message'

export function Libraries() {
  const [libraries, setLibraries] = useState<Library[] | undefined>()
  /** Why the list could not be read. */
  const [failure, setFailure] = useState<string | undefined>()
  /** Why the last library was not made. Cleared only by the next attempt. */
  const [createFailure, setCreateFailure] = useState<string | undefined>()
  const [name, setName] = useState('')
  const [folders, setFolders] = useState<string[]>([])
  const [creating, setCreating] = useState(false)
  const [probes, setProbes] = useState<Record<string, Probe>>({})
  /** A ref as well as `creating`: two submits before a render both read false. */
  const inFlight = useRef(false)
  const latest = useRef(0)

  const reload = useCallback(() => {
    const seq = ++latest.current
    listLibraries()
      .then((body) => {
        if (seq !== latest.current) return
        setLibraries(body.libraries)
        setFailure(undefined)
      })
      .catch((err: unknown) => {
        if (seq !== latest.current) return
        setFailure(sentenceOf(err))
      })
  }, [])

  useEffect(() => {
    reload()
  }, [reload])

  const remember = useCallback((added: Added[]) => {
    setProbes((held) => {
      const next = { ...held }
      for (const one of added) {
        next[one.root.path] = {
          watching: one.watching,
          watch_reason: one.watch_reason,
        }
      }
      return next
    })
  }, [])

  const submit = (e: React.FormEvent) => {
    e.preventDefault()
    if (name === '' || folders.length === 0) return
    if (inFlight.current) return
    inFlight.current = true
    setCreating(true)
    setCreateFailure(undefined)
    createLibrary(name, folders)
      .then((made) => {
        inFlight.current = false
        setCreating(false)
        setName('')
        setFolders([])
        remember(made.added)
        reload()
      })
      .catch((err: unknown) => {
        inFlight.current = false
        setCreating(false)
        setCreateFailure(sentenceOf(err))
      })
  }

  return (
    <section className="libraries">
      <h2>Libraries</h2>
      {failure !== undefined && <p role="alert">{failure}</p>}

      {libraries !== undefined &&
        (libraries.length === 0 ? (
          <p className="empty">
            No libraries yet. Make one below: a name, and the folder or folders
            its files are in. The server checks that it can read each folder
            and that none sits inside another.
          </p>
        ) : (
          <ul className="library-list">
            {libraries.map((library) => (
              <LibraryCard
                // By id: a library's id is never given to a later library.
                key={library.id}
                library={library}
                probes={probes}
                onAdded={remember}
                onChanged={reload}
              />
            ))}
          </ul>
        ))}

      {createFailure !== undefined && <p role="alert">{createFailure}</p>}
      <form onSubmit={submit} className="add-library">
        <h3>Make a library</h3>
        {/* O175: the `aria-label` repeats the label's words. */}
        <label className="field">
          <span>Name</span>
          <input
            type="text"
            name="library-name"
            aria-label="Name"
            value={name}
            onChange={(e) => setName(e.target.value)}
          />
        </label>
        {folders.length > 0 && (
          <ul className="chosen-folders">
            {folders.map((folder) => (
              <li key={folder}>
                <code>{folder}</code>
                <button
                  type="button"
                  name="unchoose"
                  aria-label={`Take ${folder} off the list`}
                  onClick={() =>
                    setFolders((held) => held.filter((f) => f !== folder))
                  }
                >
                  Take off
                </button>
              </li>
            ))}
          </ul>
        )}
        <FolderPicker
          action="Add to the list"
          onPick={(path) =>
            setFolders((held) => (held.includes(path) ? held : [...held, path]))
          }
        />
        <button
          type="submit"
          disabled={creating || name === '' || folders.length === 0}
        >
          {creating ? 'Checking…' : 'Make the library'}
        </button>
        <p className="why">
          Folders in one library are derived together: a chapter release in one
          folder joins its volume in another as a second edition.
        </p>
      </form>
    </section>
  )
}
```

Run it. Expected: PASS. Watch each fail, restoring between:

1. `reload`: delete both `if (seq !== latest.current) return` lines. Expect
   "draws the newest list when two reads answer out of order" to fail.
2. `reload`'s `.then`: add `setCreateFailure(undefined)`. Expect "keeps a
   create refusal when a card reloads the list" to fail.
3. `submit`'s `.then`: delete `remember(made.added)`. Expect "makes a library
   from a name and the folders chosen" to fail at the watch facts.

- [ ] **Step 5: The tab**

In `web/src/config.ts`, `TABS`' second entry becomes
`{ path: 'libraries', label: 'Libraries' }`, and the module's doc comment
says the Libraries screen's own client is `libraries.ts`.

In `web/src/Config.tsx`: import `Libraries`; `SCREENS`' `roots: <Roots />`
becomes `libraries: <Libraries />`; drop the `Roots` import; and before the
catch-all route add:

```tsx
        {/* The Libraries screen took the Roots screen's place. A bookmark to
            the old tab goes to it, not to the leading tab. */}
        <Route
          path="roots"
          element={<Navigate to="/config/libraries" replace />}
        />
```

In `web/src/Config.test.tsx`: mock `./libraries` (`listLibraries` resolving
`{ libraries: [] }`) beside the others; each `at('/config/roots')` becomes
`at('/config/libraries')`; and add:

```tsx
  it('sends the old roots tab to the libraries screen', async () => {
    const host = await at('/config/roots')
    expect(host.querySelector('.libraries')).not.toBeNull()
    expect(document.title).toBe('Libraries — taiyaku')
  })
```

Any test elsewhere that names the tab's label or path (`config.test.ts`,
`App.test.tsx`) takes `Libraries` and `libraries`:
`command grep -a -rn -E "config/roots|'Roots'" web/src` and read each hit.
`Roots.tsx`, `Roots.test.tsx` and `config.ts`'s `addRoot` stay until Task 11.

- [ ] **Step 6: The rules these classes need**

This task's components write eleven new classes, and `classNames.test.ts`
fails for a class no rule names. Add the rules in
`web/src/styles/admin.css`. They are starting values: Task 11 measures them
in a browser and corrects them, and its static checks and browser cases are
not part of this step.

**Leave every `.roots` rule as it is.** `Roots.tsx` still writes that class
until Task 11.

1. In the rule that sets `--config-measure`, `.libraries,` joins `.roots,`.
   In the rule that gives `.root-row, .user-row, .add-root, .add-user` their
   `max-width`, `.library-card,` and `.add-library,` join.
2. `.libraries .library-list` and `.library-card .root-list` join
   `.roots .root-list` (the list reset).
3. `.libraries p` joins `.roots p` (`overflow-wrap: anywhere`).
4. In each of the three field selector lists (the `.field` rule, its
   `input`/`select`/`textarea` rule, and the `@media (pointer: coarse)` copy),
   `.add-library .field` and `.rename-library .field` join `.add-root .field`,
   with ` input` in the two control lists.
5. `.remove-library` joins `.remove-root, .remove-user` in the strip rule,
   the `.counts` rule, the `.irreversible` rule and the `.confirm` rule;
   `.remove-library .confirm` joins `.remove-root .confirm`'s
   `overflow-wrap`; `.remove-library` takes `.remove-root`'s `margin-top`.
   `admin-config.css.test.ts` finds two of those rules by their exact
   prelude: update the two strings it looks for to
   `'.remove-root, .remove-user, .remove-library'` and
   `'.remove-root .confirm, .remove-user .confirm, .remove-library .confirm'`,
   and nothing else in that file.
6. After the `.root-row` rules, add:

```css
/* A library is a heading and what is done to it whole, over its folders'
   cards. It draws no box of its own: the folder rows inside it already do,
   and a box in a box costs each row the width of two paddings at 390px. A
   rule above each library after the first is what tells one from the
   next. */
.library-card {
  margin-bottom: var(--space-5);
}

.library-card + .library-card {
  border-top: 1px solid var(--line);
  padding-top: var(--space-4);
}

/* A library is named by a person, and a name need not hold a space. */
.library-card > h3 {
  margin: 0;
  overflow-wrap: anywhere;
}

.library-card .holds,
.library-card .no-readable {
  color: var(--muted);
  font-size: var(--text-meta);
  margin: var(--space-1) 0 0;
  max-width: 60ch;
}

.library-card > .actions,
.rename-library {
  display: flex;
  flex-wrap: wrap;
  align-items: flex-end;
  gap: var(--space-2);
  margin: var(--space-3) 0;
}

.add-library {
  display: grid;
  gap: var(--space-3);
  margin-top: var(--space-5);
}

.add-library h3 {
  margin: 0;
}

.chosen-folders {
  list-style: none;
  margin: 0;
  padding: 0;
}

.chosen-folders li {
  display: flex;
  align-items: baseline;
  gap: var(--space-2);
}

.chosen-folders code {
  min-width: 0;
  overflow-wrap: anywhere;
}

/* The box and its button on one line where they fit. */
.folder-picker {
  display: flex;
  flex-wrap: wrap;
  align-items: flex-end;
  gap: var(--space-2);
}

.folder-picker .field {
  flex: 1 1 14rem;
}
```

Check that each token exists before using it. In the report, list every
class this task's components write with the selector that names it.

- [ ] **Step 7: Run the projects and commit**

Run the four commands of Task 7's Step 8. Expected: all pass. The browser
project still mounts `Roots` directly and is unchanged by this task.

```bash
git add web/src
git commit -m "The Libraries screen: a card per library, made with its folders, renamed, scanned and removed"
```

---

## Task 9: The folder browser in the picker

**Files:**
- Modify: `web/src/FolderPicker.tsx`, `web/src/FolderPicker.test.tsx`
- Modify: `web/src/styles/admin.css` (Step 3)

**Interfaces:**
- Consumes: Task 7's `listFolders`, `FolderListing`, `sentenceOf`.
- Produces: `FolderPicker` gains a Browse… button
  (`button[name="browse"]`) that opens `.folder-browser`. Its props are
  unchanged. Entering a folder puts that folder's path in the box; the
  action button is still what picks it.

- [ ] **Step 1: Write the failing tests**

In `web/src/FolderPicker.test.tsx`, mock the client and append:

```tsx
vi.mock('./libraries', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./libraries')>()),
  listFolders: vi.fn(),
}))
```

```tsx
const PREFIXES: api.FolderListing = {
  path: null,
  parent: null,
  folders: [{ name: '/media', path: '/media' }],
}
const MEDIA: api.FolderListing = {
  path: '/media',
  parent: null,
  folders: [
    { name: 'Comics', path: '/media/Comics' },
    { name: 'Manga', path: '/media/Manga' },
  ],
}
const MANGA: api.FolderListing = {
  path: '/media/Manga',
  parent: '/media',
  folders: [{ name: 'Volumes', path: '/media/Manga/Volumes' }],
}

const browse = (host: HTMLElement) =>
  host.querySelector('button[name="browse"]') as HTMLButtonElement
const entries = (host: HTMLElement) =>
  [...host.querySelectorAll('.folder-list button')] as HTMLButtonElement[]

describe('FolderPicker, browsing', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    vi.mocked(api.listFolders).mockImplementation((path) =>
      Promise.resolve(
        path === undefined ? PREFIXES : path === '/media' ? MEDIA : MANGA,
      ),
    )
  })

  it('asks for nothing until it is opened, then lists the prefixes', async () => {
    const host = mount(<FolderPicker action="Add folder" onPick={vi.fn()} />)
    expect(api.listFolders).not.toHaveBeenCalled()
    expect(host.querySelector('.folder-browser')).toBeNull()

    await press(browse(host))

    expect(api.listFolders).toHaveBeenCalledWith(undefined)
    expect(browse(host).getAttribute('aria-expanded')).toBe('true')
    expect(entries(host).map((b) => b.textContent)).toEqual(['/media'])
    expect(host.querySelector('button[name="up"]')).toBeNull()
    expect(box(host).value).toBe('')
  })

  it('enters a folder, puts its path in the box, and still picks by the button', async () => {
    const onPick = vi.fn()
    const host = mount(<FolderPicker action="Add folder" onPick={onPick} />)
    await press(browse(host))
    await press(entries(host)[0])
    expect(host.querySelector('.where')?.textContent).toBe('/media')
    expect(box(host).value).toBe('/media')
    expect(entries(host).map((b) => b.textContent)).toEqual(['Comics', 'Manga'])

    await press(entries(host)[1])
    expect(api.listFolders).toHaveBeenLastCalledWith('/media/Manga')
    expect(box(host).value).toBe('/media/Manga')
    expect(onPick).not.toHaveBeenCalled()

    await press(pick(host))
    expect(onPick).toHaveBeenCalledWith('/media/Manga')
  })

  it('goes up to the parent, and from a prefix to the prefixes', async () => {
    const host = mount(<FolderPicker action="Add folder" onPick={vi.fn()} />)
    await press(browse(host))
    await press(entries(host)[0])
    await press(entries(host)[1])

    await press(host.querySelector('button[name="up"]') as Element)
    expect(api.listFolders).toHaveBeenLastCalledWith('/media')
    expect(box(host).value).toBe('/media')

    await press(host.querySelector('button[name="up"]') as Element)
    expect(api.listFolders).toHaveBeenLastCalledWith(undefined)
    expect(entries(host).map((b) => b.textContent)).toEqual(['/media'])
    expect(box(host).value).toBe('/media')
  })

  it('says to type the path when the server may browse nothing', async () => {
    vi.mocked(api.listFolders).mockResolvedValue({ path: null, parent: null, folders: [] })
    const host = mount(<FolderPicker action="Add folder" onPick={vi.fn()} />)
    await press(browse(host))
    expect(host.querySelector('.folder-browser .empty')?.textContent).toContain(
      'TAIYAKU_FOLDERS',
    )
  })

  it('says a folder holds none, without that sentence', async () => {
    vi.mocked(api.listFolders).mockResolvedValue({
      path: '/media/Empty',
      parent: '/media',
      folders: [],
    })
    const host = mount(<FolderPicker action="Add folder" onPick={vi.fn()} />)
    await press(browse(host))
    expect(host.querySelector('.folder-browser .empty')?.textContent).toBe(
      'No folders in here.',
    )
  })

  it("draws a refusal's sentence and keeps the list it had", async () => {
    const host = mount(<FolderPicker action="Add folder" onPick={vi.fn()} />)
    await press(browse(host))
    vi.mocked(api.listFolders).mockRejectedValue(
      new Error(
        '/api/library/folders?path=%2Fmedia: 403 {"error":"/elsewhere is not inside a folder this server may browse"}',
      ),
    )
    await press(entries(host)[0])
    expect(host.querySelector('.folder-browser [role="alert"]')?.textContent).toBe(
      '/elsewhere is not inside a folder this server may browse',
    )
    expect(entries(host).map((b) => b.textContent)).toEqual(['/media'])
  })

  it('draws the folder asked for last when two answers cross', async () => {
    const slow = deferred<api.FolderListing>()
    const host = mount(<FolderPicker action="Add folder" onPick={vi.fn()} />)
    await press(browse(host))
    await press(entries(host)[0])
    vi.mocked(api.listFolders).mockReturnValueOnce(slow.promise)
    await press(entries(host)[0])
    await press(host.querySelector('button[name="up"]') as Element)
    expect(host.querySelector('.where')?.textContent).toBe(
      'Folders this server may browse',
    )

    await flush(() => slow.resolve(MANGA))
    expect(host.querySelector('.where')?.textContent).toBe(
      'Folders this server may browse',
    )
  })
})
```

Add the imports the new cases need (`beforeEach`, `deferred`, `* as api from
'./libraries'`).

Run: `pnpm -C web exec vitest run --project jsdom --reporter=default src/FolderPicker.test.tsx`
Expected: FAIL: there is no Browse… button.

- [ ] **Step 2: Implement**

In `web/src/FolderPicker.tsx`, add to the doc comment:

```tsx
 * **Browse… is a way to find a path, not a second way to choose one.** It
 * lists directories beneath the prefixes the server was started with, and
 * entering one writes its path into the box. The button beside the box is
 * still what picks it, so a browsed path and a typed one take the same
 * route and meet the same refusals. Nothing is asked of the server until the
 * browser is opened. With no prefix configured it says so, and the box is
 * what is left.
```

and to the component, with `useRef` imported, `listFolders` and
`FolderListing` from `./libraries`, and `sentenceOf` from `./message`:

```tsx
  const [open, setOpen] = useState(false)
  const [listing, setListing] = useState<FolderListing | undefined>()
  const [browseFailure, setBrowseFailure] = useState<string | undefined>()
  /** The newest listing asked for. Answers need not arrive in order. */
  const latest = useRef(0)

  const go = (to?: string) => {
    const seq = ++latest.current
    setBrowseFailure(undefined)
    listFolders(to)
      .then((answer) => {
        if (seq !== latest.current) return
        setListing(answer)
        if (answer.path !== null) setPath(answer.path)
      })
      .catch((err: unknown) => {
        if (seq !== latest.current) return
        setBrowseFailure(sentenceOf(err))
      })
  }

  const toggle = () => {
    if (!open && listing === undefined) go()
    setOpen(!open)
  }
```

and, after the pick button:

```tsx
      <button
        type="button"
        name="browse"
        aria-expanded={open}
        onClick={toggle}
      >
        Browse…
      </button>
      {open && (
        <div
          className="folder-browser"
          role="group"
          aria-label="Browse for a folder"
        >
          {browseFailure !== undefined && <p role="alert">{browseFailure}</p>}
          {listing === undefined ? (
            browseFailure === undefined && <p>Reading folders…</p>
          ) : (
            <>
              <p className="where">
                {listing.path === null ? (
                  'Folders this server may browse'
                ) : (
                  <code>{listing.path}</code>
                )}
              </p>
              {listing.path !== null && (
                <button
                  type="button"
                  name="up"
                  onClick={() => go(listing.parent ?? undefined)}
                >
                  Up
                </button>
              )}
              {listing.folders.length === 0 ? (
                <p className="empty">
                  {listing.path === null
                    ? 'This server was not told which folders it may browse ' +
                      '(--folders, or TAIYAKU_FOLDERS). Type the path instead.'
                    : 'No folders in here.'}
                </p>
              ) : (
                <ul className="folder-list">
                  {listing.folders.map((folder) => (
                    <li key={folder.path}>
                      <button
                        type="button"
                        name="enter"
                        onClick={() => go(folder.path)}
                      >
                        {folder.name}
                      </button>
                    </li>
                  ))}
                </ul>
              )}
            </>
          )}
        </div>
      )}
```

- [ ] **Step 3: The rules the browser's classes need**

`classNames.test.ts` fails for a class no rule names. After
`.folder-picker`'s rules in `web/src/styles/admin.css`, add these starting
values, which Task 11 measures and corrects:

```css
/* The browser, when open, on a line of its own beneath the box. */
.folder-browser {
  flex-basis: 100%;
  border: 1px solid var(--line);
  border-radius: var(--radius-control);
  padding: var(--space-3);
}

.folder-browser .where {
  min-width: 0;
  overflow-wrap: anywhere;
}

.folder-list {
  list-style: none;
  margin: var(--space-2) 0 0;
  padding: 0;
  display: grid;
  gap: var(--space-1);
}

.folder-list button {
  width: 100%;
  text-align: left;
  overflow-wrap: anywhere;
}
```

If `admin-config.css.test.ts`'s "sets the measure once" case objects to the
`width` on `.folder-list button`, say so in the report and do not widen its
pattern: a list of buttons that each fill their row is not a width per
control.

- [ ] **Step 4: Run, watch the defects, commit**

Run the four commands of Task 7's Step 8. Expected: all pass.

Watch each fail, restoring between:

1. `go`: delete both `if (seq !== latest.current) return` lines. Expect
   "draws the folder asked for last when two answers cross" to fail.
2. `go`: delete `if (answer.path !== null) setPath(answer.path)`. Expect
   "enters a folder, puts its path in the box" to fail.
3. `toggle`: call `go()` unconditionally on mount instead, in a
   `useEffect(() => { go() }, [])`. Expect "asks for nothing until it is
   opened" to fail.

```bash
git add web/src/FolderPicker.tsx web/src/FolderPicker.test.tsx web/src/styles/admin.css
git commit -m "The folder picker browses beneath the server's prefixes and writes the path it finds"
```

---

## Task 10: The Users card grants libraries, and names the unrestricted state

**Files:**
- Modify: `web/src/users.ts`, `web/src/users.test.ts`
- Modify: `web/src/Users.tsx`, `web/src/Users.test.tsx`
- Modify: `web/src/UserRow.tsx`, `web/src/UserRow.test.tsx`
- Modify: `web/src/styles/admin.css` (the one new class, Step 5)

**Interfaces:**
- Consumes: Task 3's wire shape (`libraries` on an account and in a patch);
  Task 7's `listLibraries`, `Library`, `sentenceOf`.
- Produces:
  ```ts
  // users.ts
  export interface Grant { id: number; name: string }
  export interface User { /* … */ libraries: Grant[] }        // was `roots`
  export interface UserPatch { /* … */ libraries?: number[] } // was `roots`

  // UserRow.tsx
  export interface UserRowProps {
    user: User
    users: User[]
    /** Every library, for the grant editor. `undefined` where the list has not been read. */
    libraries?: Library[]
    actingAs?: number
    onChanged: () => unknown
  }
  ```
  The grant editor is `fieldset.grants` holding two radio buttons named
  `sees-<account id>` with values `every` and `only`, and under `only` a
  `ul.grant-list` with one `input[name="library-<id>"]` per library.

- [ ] **Step 1: The client's types**

In `web/src/users.ts`: `Grant` becomes `{ id: number; name: string }`, "One
library this account is granted, as `user_json`'s `libraries` sends it".
`User.roots` becomes `libraries: Grant[]` and keeps its "**Empty means
unrestricted, not blind**" comment. `UserPatch.roots` becomes:

```ts
  /**
   * The whole list of granted libraries, by id, replacing what is there.
   * `[]` grants nothing, which is unrestricted; the card sends it only for
   * its "Every library" choice.
   */
  libraries?: number[]
```

The module's doc comment says the libraries list is `libraries.ts`'s, fetched
once by `Users.tsx`. In `web/src/users.test.ts`, each fixture's `roots: []`
becomes `libraries: []`.

- [ ] **Step 2: Rewrite the card's grant tests**

In `web/src/UserRow.test.tsx`: the `user` fixture's `roots: []` becomes
`libraries: []`; the `root` fixture and the `Root` import go; `show`'s
`over.roots` becomes `over.libraries` (`null` for the unread state, absent
for the two libraries below), passed as the `libraries` prop. Add:

```tsx
import type { Library } from './libraries'

const lib = (id: number, name: string, paths: string[]): Library => ({
  id,
  name,
  works: 0,
  books: 0,
  folders: paths.map((path, i) => ({
    id: id * 10 + i,
    path,
    edition_key: null,
    default_kind: null,
    default_language: null,
    exclude_globs: null,
    scan_interval_secs: null,
    keeps_documents: false,
    last_scan_at: null,
    last_scan_status: null,
  })),
})

const MANGA = lib(70, 'Manga', ['/media/Manga/Volumes', '/media/Manga/Chapters'])
const COMICS = lib(80, 'Comics', ['/media/Comics'])

const radio = (host: HTMLElement, value: 'every' | 'only') =>
  control<HTMLInputElement>(host, `.grants input[type="radio"][value="${value}"]`)
const tick = (host: HTMLElement, id: number) =>
  control<HTMLInputElement>(host, `input[name="library-${id}"]`)
```

`show` defaults `libraries` to `[MANGA, COMICS]`.

Replace the grant tests. Each `it` named below takes the place of the one
whose job it now does; `click`, `control` and `show` are that file's own.

```tsx
  // Replaces 'says that nothing ticked means unrestricted'.
  it('draws an account with no grant as seeing every library, with no boxes', async () => {
    const me = user()
    const other = user({ id: 2, name: 'sam', role: 'user' })
    const { host } = await show(other, [me, other], { actingAs: 1 })
    expect(control(host, '.grants legend').textContent).toBe(
      'Libraries this account may see',
    )
    expect(radio(host, 'every').checked).toBe(true)
    expect(radio(host, 'only').checked).toBe(false)
    expect(host.querySelector('.grant-list')).toBeNull()
  })

  it('draws a granted account as Only these, a box per library with its folders', async () => {
    const me = user()
    const other = user({
      id: 2,
      name: 'sam',
      role: 'user',
      libraries: [{ id: 70, name: 'Manga' }],
    })
    const { host } = await show(other, [me, other], { actingAs: 1 })
    expect(radio(host, 'only').checked).toBe(true)
    expect(tick(host, 70).checked).toBe(true)
    expect(tick(host, 80).checked).toBe(false)
    const manga = tick(host, 70).closest('li') as HTMLElement
    expect(manga.querySelector('label span')?.textContent).toBe('Manga')
    expect(
      [...manga.querySelectorAll('.grant-folders code')].map((c) => c.textContent),
    ).toEqual(['/media/Manga/Volumes', '/media/Manga/Chapters'])
  })

  // O193: a library of two folders is one box, and unticking it revokes it.
  // Replaces 'sends the grant list without the root that was unticked'.
  it('revokes a library of two folders by its one box', async () => {
    const me = user()
    const other = user({
      id: 2,
      name: 'sam',
      role: 'user',
      libraries: [
        { id: 70, name: 'Manga' },
        { id: 80, name: 'Comics' },
      ],
    })
    const { host } = await show(other, [me, other], { actingAs: 1 })
    await click(tick(host, 70))
    expect(usersApi.patchUser).toHaveBeenCalledWith(2, { libraries: [80] })
  })

  // Replaces 'sends the whole grant list when a root is ticked…'.
  it('sends the whole list when a library is ticked', async () => {
    const me = user()
    const other = user({
      id: 2,
      name: 'sam',
      role: 'user',
      libraries: [{ id: 70, name: 'Manga' }],
    })
    vi.mocked(usersApi.patchUser).mockResolvedValue({
      ...other,
      libraries: [
        { id: 70, name: 'Manga' },
        { id: 80, name: 'Comics' },
      ],
    })
    const { host } = await show(other, [me, other], { actingAs: 1 })
    await click(tick(host, 80))
    expect(usersApi.patchUser).toHaveBeenCalledWith(2, { libraries: [70, 80] })
  })

  it('will not clear the last tick, and says what lifts the limit', async () => {
    const me = user()
    const other = user({
      id: 2,
      name: 'sam',
      role: 'user',
      libraries: [{ id: 70, name: 'Manga' }],
    })
    const { host } = await show(other, [me, other], { actingAs: 1 })
    expect(tick(host, 70).getAttribute('aria-disabled')).toBe('true')
    expect(control(host, '.grants').textContent).toContain(
      'The last tick cannot be cleared here',
    )

    await click(tick(host, 70))

    expect(usersApi.patchUser).not.toHaveBeenCalled()
    expect(tick(host, 70).checked).toBe(true)
  })

  it('lifts the limit only by Every library, which sends the empty list', async () => {
    const me = user()
    const other = user({
      id: 2,
      name: 'sam',
      role: 'user',
      libraries: [{ id: 70, name: 'Manga' }],
    })
    vi.mocked(usersApi.patchUser).mockResolvedValue({ ...other, libraries: [] })
    const { host } = await show(other, [me, other], { actingAs: 1 })
    await click(radio(host, 'every'))
    expect(usersApi.patchUser).toHaveBeenCalledWith(2, { libraries: [] })
    expect(radio(host, 'every').checked).toBe(true)
    expect(host.querySelector('.grant-list')).toBeNull()
  })

  it('sends nothing for Only these until a library is ticked', async () => {
    const me = user()
    const other = user({ id: 2, name: 'sam', role: 'user' })
    vi.mocked(usersApi.patchUser).mockResolvedValue({
      ...other,
      libraries: [{ id: 80, name: 'Comics' }],
    })
    const { host } = await show(other, [me, other], { actingAs: 1 })

    await click(radio(host, 'only'))
    expect(usersApi.patchUser).not.toHaveBeenCalled()
    expect(radio(host, 'only').checked).toBe(true)
    expect(control(host, '.grants').textContent).toContain(
      'Nothing is saved until a library is ticked',
    )

    await click(radio(host, 'every'))
    expect(usersApi.patchUser).not.toHaveBeenCalled()
    expect(host.querySelector('.grant-list')).toBeNull()

    await click(radio(host, 'only'))
    await click(tick(host, 80))
    expect(usersApi.patchUser).toHaveBeenCalledTimes(1)
    expect(usersApi.patchUser).toHaveBeenCalledWith(2, { libraries: [80] })
  })

  it('says an administrator sees everything whatever is chosen', async () => {
    const me = user()
    const admin = user({
      id: 2,
      name: 'sam',
      role: 'admin',
      libraries: [{ id: 70, name: 'Manga' }],
    })
    const { host } = await show(admin, [me, admin], { actingAs: 1 })
    expect(control(host, '.grants').textContent).toContain(
      'An administrator sees every library',
    )
    const reader = user({ id: 3, name: 'kim', role: 'user' })
    const other = await show(reader, [me, reader], { actingAs: 1 })
    expect(control(other.host, '.grants').textContent).not.toContain(
      'An administrator sees every library',
    )
  })

  // Replaces 'says the roots list has not been read, and names what is granted'.
  it('says the libraries list has not been read, and names what is granted', async () => {
    const me = user()
    const other = user({
      id: 2,
      name: 'sam',
      role: 'user',
      libraries: [{ id: 70, name: 'Manga' }],
    })
    const { host } = await show(other, [me, other], { actingAs: 1, libraries: null })
    const grants = control(host, '.grants').textContent ?? ''
    expect(grants).toContain('has not been read')
    expect(grants).toContain('Manga')
    expect(grants).not.toContain('no libraries yet')
    expect(host.querySelector('.grants input')).toBeNull()
  })

  // Replaces 'says the library has no roots rather than drawing an empty editor'.
  it('says there are no libraries rather than drawing an empty editor', async () => {
    const me = user()
    const other = user({ id: 2, name: 'sam', role: 'user' })
    const { host } = await show(other, [me, other], { actingAs: 1, libraries: [] })
    expect(control(host, '.grants').textContent).toContain('There are no libraries yet')
    expect(host.querySelector('.grants input')).toBeNull()
  })
```

In 'computes the next grant list from the answer when the reload brings
nothing': the fixture holds `libraries: [{ id: 70, name: 'Manga' }]`, the
first click ticks 80, the answer carries both, and the second call asserted
is `{ libraries: [80] }` after unticking 70. Keep what the test is about: the
second list is computed from the answer, not from the stale prop.

In the removal block's 'states all five counts and the name…', the sentence
ends "and 1 library grant." for `grants: 1`.

In 'draws the server sentence when a patch is refused', make the body JSON,
as the route sends it, and assert the whole alert:

```tsx
    vi.mocked(usersApi.patchUser).mockRejectedValue(
      new Error(
        '/api/library/users/2: 409 {"error":"demoting `sam` would leave this library with no administrator who can log in"}',
      ),
    )
    // …
    expect(host.querySelector('[role="alert"]')?.textContent).toBe(
      'demoting `sam` would leave this library with no administrator who can log in',
    )
```

Run: `pnpm -C web exec vitest run --project jsdom --reporter=default src/UserRow.test.tsx`
Expected: FAIL to typecheck in the test (`libraries` is not a prop), or on
the first new case.

- [ ] **Step 3: Implement in `UserRow.tsx`**

- Imports: `type Library` from `./libraries` in place of `type Root` from
  `./config`; `sentenceOf` from `./message` in place of `messageOf`.
- `UserRowProps.roots` becomes `libraries?: Library[]`. Its doc comment keeps
  the "Three states rather than two" argument, with "libraries" for "roots":
  what the card can always say is which libraries the account is granted,
  because `user.libraries` carries their names.
- `failed` and the dry run's catch take `sentenceOf(err)`.
- Replace `granted` and `toggleGrant`:

  ```tsx
  const granted = shown.libraries.map((g) => g.id)
  /**
   * "Only these" was chosen on an account that holds no grant yet.
   *
   * Local, because there is nothing to save: an empty list is unrestricted,
   * so the choice is not a fact about the account until a library is ticked.
   * Choosing "Every library" again puts it back, and sends nothing either.
   */
  const [choosing, setChoosing] = useState(false)
  const restricted = granted.length > 0 || choosing

  const toggleGrant = (id: number, on: boolean) => {
    const next = on ? [...granted, id] : granted.filter((held) => held !== id)
    // **The last tick is not cleared here.** An empty list is unrestricted,
    // and each tick is saved at once: unticking the last library on the way
    // to ticking another would leave the account seeing every library until
    // the second click, and for good if that click never came. "Every
    // library" is the control that says so.
    if (next.length === 0) return
    save({ libraries: next })
  }

  const seeEvery = () => {
    if (granted.length === 0) {
      setChoosing(false)
      return
    }
    save({ libraries: [] }, () => setChoosing(false))
  }
  ```

- Replace the `fieldset.grants` block:

  ```tsx
      <fieldset className="grants">
        <legend>Libraries this account may see</legend>
        {libraries === undefined ? (
          <p className="unknown">
            The libraries list has not been read, so there is nothing to offer
            here.{' '}
            {shown.libraries.length === 0
              ? 'This account is granted none, which is unrestricted.'
              : `This account is granted ${shown.libraries
                  .map((g) => g.name)
                  .join(', ')}.`}
          </p>
        ) : libraries.length === 0 ? (
          <p className="empty">There are no libraries yet.</p>
        ) : (
          <>
            <label>
              <input
                type="radio"
                name={`sees-${user.id}`}
                value="every"
                checked={!restricted}
                aria-disabled={saving}
                onChange={seeEvery}
              />
              <span>Every library, including any added later</span>
            </label>
            <label>
              <input
                type="radio"
                name={`sees-${user.id}`}
                value="only"
                checked={restricted}
                aria-disabled={saving}
                onChange={() => setChoosing(true)}
              />
              <span>Only these</span>
            </label>
            {restricted && (
              <ul className="grant-list">
                {libraries.map((library) => {
                  const last = granted.length === 1 && granted[0] === library.id
                  return (
                    <li key={library.id}>
                      <label>
                        <input
                          type="checkbox"
                          name={`library-${library.id}`}
                          checked={granted.includes(library.id)}
                          aria-disabled={saving || last}
                          onChange={(e) =>
                            toggleGrant(library.id, e.target.checked)
                          }
                        />
                        <span>{library.name}</span>
                      </label>
                      <ul className="grant-folders">
                        {library.folders.map((folder) => (
                          <li key={folder.path}>
                            <code>{folder.path}</code>
                          </li>
                        ))}
                      </ul>
                    </li>
                  )
                })}
              </ul>
            )}
            {restricted && granted.length === 0 && (
              <p className="why">Nothing is saved until a library is ticked.</p>
            )}
            {granted.length === 1 && (
              <p className="why">
                The last tick cannot be cleared here. Choose “Every library” to
                lift the limit.
              </p>
            )}
            {shown.role === 'admin' && (
              <p className="why">
                An administrator sees every library whatever is chosen here.
                The choice takes effect if the role becomes user.
              </p>
            )}
          </>
        )}
      </fieldset>
  ```

- The removal sentence's last count is
  `count(cost.removed.grants, 'library grant', 'library grants')`.
- In the file's doc comment, the "**Grants are sent whole.**" paragraph
  becomes:

  ```tsx
 * **Grants are libraries, sent whole, and the unrestricted state has a
 * name.** `account::Patch::libraries` replaces the list, an absent key
 * leaves it alone, and `[]` means unrestricted, not blind. The card draws
 * that as a choice between "Every library" and "Only these", and will not
 * clear the last tick: each tick is saved at once, so an account moved from
 * one library to another by unticking first would see every library in
 * between. The server would accept that request. This is the direction the
 * paragraph above allows: the card may refuse what the server permits.
  ```

  and the `aria-disabled` comment's "the grant list below" stays true.

- [ ] **Step 4: `Users.tsx` reads the libraries**

In `web/src/Users.tsx`: `listRoots`/`Root` give way to `listLibraries` and
`Library` from `./libraries`; `roots`/`rootsFailure` become
`libraries`/`librariesFailure`; the effect stores `body.libraries` and takes
`sentenceOf(err)`; the alert reads "The libraries list could not be read
({librariesFailure}), so the grant editors below show nothing rather than
what there is."; `<UserRow … libraries={libraries} …>`. The add form's and
the list's own failures take `sentenceOf` too. The doc comment's "The roots
list is fetched here, once" paragraph says libraries, from `libraries.ts`.

In `web/src/Users.test.tsx`: mock `./libraries`' `listLibraries` in place of
`./config`'s `listRoots`; fixtures' `roots: []` become `libraries: []`; and
'says so when the roots list could not be read…' asserts "libraries list
could not be read" and that `.grants` does not contain "no libraries yet".

- [ ] **Step 5: Run, watch the defects, commit**

The card writes one new class, `grant-folders`, and `classNames.test.ts`
fails for a class no rule names. Beside `.user-row .grant-list`'s rules in
`web/src/styles/admin.css`, add this starting value, which Task 11 measures
and corrects:

```css
.grant-folders {
  list-style: none;
  margin: 0;
  padding: 0 0 0 var(--space-5);
  color: var(--muted);
  font-size: var(--text-meta);
}

.grant-folders code {
  min-width: 0;
  overflow-wrap: anywhere;
}
```

Run the four commands of Task 7's Step 8. Expected: all pass. The browser
project's `fieldNames.browser.test.tsx` passes `roots=` to `UserRow` and
fails to typecheck: change that one prop to `libraries={[]}` now; Task 11
does the rest of that file.

Watch each fail, restoring between:

1. `toggleGrant`: delete `if (next.length === 0) return`. Expect "will not
   clear the last tick" to fail: a patch with `{ libraries: [] }` is sent.
2. `seeEvery`: delete the `granted.length === 0` branch, so it always saves.
   Expect "sends nothing for Only these until a library is ticked" to fail.
3. Replace `checked={restricted}` on the `only` radio with
   `checked={granted.length > 0}`. Expect the same test to fail at its first
   `radio(host, 'only').checked`.
4. Replace `shown.role === 'admin'` with `true`. Expect "says an
   administrator sees everything whatever is chosen" to fail.

```bash
git add web/src
git commit -m "The Users card grants libraries, and lifting the limit is a choice with a name (O193)"
```

---

## Task 11: Styles, the browser project, and the end of `Roots.tsx`

**Files:**
- Modify: `web/src/styles/admin.css`,
  `web/src/styles/admin-config.css.test.ts`,
  `web/src/styles/admin-proposals.css.test.ts`
- Modify: `web/src/RootRow.tsx` (its heading), `web/src/RootRow.test.tsx`
- Modify: `web/src/Config.browser.test.tsx`,
  `web/src/fieldNames.browser.test.tsx`
- Modify: `web/src/config.ts`, `web/src/config.test.ts` (`addRoot` goes)
- Delete: `web/src/Roots.tsx`, `web/src/Roots.test.tsx`

**Interfaces:**
- Consumes: the class names Tasks 8–10 draw.
- Produces: nothing a later task reads. After it, nothing imports `Roots`.

This task is measured in a browser, not reasoned about. Every rule below is
a starting point; the browser cases and your own screenshots at 390px and at
desktop width, in the light and the dark scheme, decide. Put the screenshots
in the evidence directory and look at each one before the report.

- [ ] **Step 1: A folder's heading goes down a level**

A folder row is now inside a library's card, whose name is an `h3`. In
`web/src/RootRow.tsx` the row's `<h3>` becomes `<h4>`. In `admin.css` each
`.root-row h3` and `.root-row > h3` selector becomes `h4`, and in
`admin-config.css.test.ts` the `it.each` entry `'.root-row > h3'` becomes
`'.root-row > h4'`. `command grep -a -rn "root-row > h3\|root-row h3\|querySelector('h3')" web/src`
and read each hit; a `RootRow.test.tsx` case that finds the path by `h3`
finds it by `h4`.

- [ ] **Step 2: Write the failing static checks**

In `web/src/styles/admin-config.css.test.ts`:

- The measure test's pattern becomes
  `/\.(root-row|user-row|add-user|add-root|library-card|add-library|rename-library|folder-picker)\b/`.
- The `it.each` list of selectors that break a long path gains
  `'.library-card > h3'`, `'.libraries p'` in place of `'.roots p'`,
  `'.remove-library .confirm'`, `'.chosen-folders code'`,
  `'.folder-browser .where'`, `'.folder-list button'` and
  `'.grant-folders code'`.
- The two tests that find the danger strip and the confirm button by an
  exact prelude take the preludes Step 3 writes:
  `'.remove-root, .remove-user, .remove-library'` and
  `'.remove-root .confirm, .remove-user .confirm, .remove-library .confirm'`.
- Add:

  ```ts
  describe('admin.css styles the field labels of the libraries screen', () => {
    it.each(['.add-library', '.rename-library'])(
      'scopes the field rules to %s',
      (container) => {
        const preludes = rules(adminSource).flatMap(({ prelude }) =>
          selectors(prelude),
        )
        expect(preludes).toContain(`${container} .field`)
        expect(preludes).toContain(`${container} .field input`)
      },
    )
  })
  ```

In `web/src/styles/admin-proposals.css.test.ts`, wherever a list or a
constant names `'.user-row .grant-list label'`, the two radio labels join it
as `'.user-row .grants > label'`: they are the same kind of row, a box and
its words, and take the same tap target.

Run: `pnpm -C web exec vitest run --project node --reporter=default src/styles`
Expected: FAIL on each entry whose rule Tasks 7 to 10 did not already add.
Say in the report which entries passed at once.

- [ ] **Step 3: The stylesheet**

Tasks 7 to 10 added a starting rule for each class they introduced, so
several of the edits below are already in the file. Where one is, check it
against the browser and correct it; do not add it twice. What only this task
does: the `.roots` selectors go, since Step 5 deletes the component that
wrote the class; the comments; `.user-row .grants > label`; and whatever the
measurements change.

In `web/src/styles/admin.css`:

- The measure: `.roots,` becomes `.libraries,` in the `--config-measure`
  rule, and `.library-card,` and `.add-library,` join
  `.root-row, .user-row, .add-root, .add-user`.
- `.roots .root-list` becomes `.libraries .library-list, .library-card .root-list`.
- `.roots p` becomes `.libraries p`, and its comment says which sentences
  this screen draws outside a card: a list that could not be read and a
  library that was not made.
- In each of the three field selector lists (the `.field` rule, its
  `input`/`select`/`textarea` rule, and the `@media (pointer: coarse)` copy),
  `.add-library .field` and `.rename-library .field` join `.add-root .field`,
  with ` input` in the two control lists.
- `.remove-library` joins `.remove-root, .remove-user` in the strip rule, the
  `.counts` rule, the `.irreversible` rule and the `.confirm` rule, and
  `.remove-library .confirm` joins `.remove-root .confirm`'s
  `overflow-wrap`. `.remove-library` takes `.remove-root`'s `margin-top`.
- Wherever a selector list holds `.user-row .grant-list label`,
  `.user-row .grants > label` joins it.
- Each comment that says `.roots` or "the roots screen" says what is true
  now. Do not correct a figure a comment gives; delete a sentence whose
  figure you did not re-measure.
- Add, after the `.root-row` rules:

  ```css
  /* A library is a heading and what is done to it whole, over its folders'
     cards. It draws no box of its own: the folder rows inside it already do,
     and a box in a box costs each row the width of two paddings at 390px. A
     rule above each library after the first is what tells one from the
     next. */
  .library-card {
    margin-bottom: var(--space-5);
  }

  .library-card + .library-card {
    border-top: 1px solid var(--line);
    padding-top: var(--space-4);
  }

  /* A library is named by a person, and a name need not hold a space. */
  .library-card > h3 {
    margin: 0;
    overflow-wrap: anywhere;
  }

  .library-card .holds,
  .library-card .no-readable {
    color: var(--muted);
    font-size: var(--text-meta);
    margin: var(--space-1) 0 0;
    max-width: 60ch;
  }

  .library-card > .actions,
  .rename-library {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-end;
    gap: var(--space-2);
    margin: var(--space-3) 0;
  }

  .add-library {
    display: grid;
    gap: var(--space-3);
    margin-top: var(--space-5);
  }

  .add-library h3 {
    margin: 0;
  }

  .chosen-folders,
  .folder-list,
  .grant-folders {
    list-style: none;
    margin: 0;
    padding: 0;
  }

  .chosen-folders li {
    display: flex;
    align-items: baseline;
    gap: var(--space-2);
  }

  .chosen-folders code,
  .folder-browser .where,
  .grant-folders code {
    min-width: 0;
    overflow-wrap: anywhere;
  }

  /* The box and its two buttons on one line where they fit; the browser,
     when open, on a line of its own beneath them. */
  .folder-picker {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-end;
    gap: var(--space-2);
  }

  .folder-picker .field {
    flex: 1 1 14rem;
  }

  .folder-browser {
    flex-basis: 100%;
    border: 1px solid var(--line);
    border-radius: var(--radius-control);
    padding: var(--space-3);
  }

  .folder-list {
    display: grid;
    gap: var(--space-1);
    margin-top: var(--space-2);
  }

  .folder-list button {
    width: 100%;
    text-align: left;
    overflow-wrap: anywhere;
  }

  .grant-folders {
    color: var(--muted);
    font-size: var(--text-meta);
    padding-left: var(--space-5);
  }
  ```

  `.folder-list button` sets a `width`, and the measure test's pattern does
  not name `.folder-list`: a list of buttons that each fill their row is not
  a width per control. If the test objects, say so in the report; do not
  widen the pattern to make it pass.

Run the static checks again. Expected: PASS.

- [ ] **Step 4: Move the browser cases to the Libraries screen**

In `web/src/Config.browser.test.tsx`:

- Mock `./libraries` beside `./config`: `listLibraries`, `createLibrary`,
  `libraryScanStatus`, `libraryRemovalCost`. `beforeEach` gives
  `libraryScanStatus` an idle answer
  (`{ library_id: 1, running: false, elapsed_secs: null, folders: [] }`) and
  `listLibraries` an empty list.
- Add the fixture every roots case below uses:

  ```tsx
  /** One library holding `folders`, as the list route sends it. */
  const holding = (...folders: config.Root[]) => ({
    libraries: [{ id: 1, name: 'Manga', works: 0, books: 0, folders }],
  })
  ```

- In each case, `vi.mocked(config.listRoots).mockResolvedValue({ roots: [X] })`
  becomes `vi.mocked(librariesApi.listLibraries).mockResolvedValue(holding(X))`,
  and `mount(inShell(<Roots />))` becomes `mount(inShell(<Libraries />))`.
  The row-level selectors (`.root-row`, `.remove-root`, `.regime dd`) are
  unchanged. The cases, by their names at `d963659a`:

  | Case | Beyond the two substitutions |
  | --- | --- |
  | does not scroll sideways on the roots screen | rename it "…on the libraries screen" |
  | keeps a root's own path inside its card when the path has nowhere to break | none |
  | keeps the removal's confirm button inside the card for the same path | none |
  | keeps the screen's own sentences about a root inside the window | see below |
  | keeps a refused scan's sentence inside the window | none |
  | keeps a failed watch probe's reason inside the card | see below |
  | keeps a scan that ran and failed inside the window | none |
  | keeps a turned-down "Scan now"'s sentence inside the window | `.root-row > [role="alert"]` is still where a folder's own refusal is drawn |
  | puts each root fact on its own row beside its label | none |

  The cases that build a bare `<ul className="root-list"><li className="root-row">`
  by hand mount no screen and are unchanged, save a wrapper they give the
  measure through: `.roots` there becomes `.libraries`.

- "keeps the screen's own sentences about a root inside the window" was
  about the add banner and the add refusal, both outside any card. The
  banner is gone. Rewrite it over the one sentence the screen still draws
  there: make `createLibrary` reject with
  `new Error(`/api/library/libraries: 400 {"error":"${UNBROKEN_ROOT.path}: cannot be read: Permission denied (os error 13)"}`)`,
  type a name, pick `UNBROKEN_ROOT.path`, submit `form.add-library`, and
  assert no sideways scroll and that
  `.libraries > [role="alert"]` ends inside the window, as the case asserted
  of `.roots > [role="alert"]`.
- "keeps a failed watch probe's reason inside the card" got its probe from
  `addRoot`. Get it from `createLibrary`: resolve it with
  `{ library: holding(UNBROKEN_ROOT).libraries[0], added: [{ root: UNBROKEN_ROOT, watching: false, watch_reason: <the long reason the case already uses>, interval_secs: 3600 }] }`,
  have `listLibraries` answer empty first and `holding(UNBROKEN_ROOT)` after,
  make the library through the form, and keep the case's assertion on
  `.root-row .regime dd`.
- Add three cases to the 390px block, each ending in
  `assertNoSidewaysScroll(host)`:
  1. "keeps a library whose name has nowhere to break inside the window": a
     library named with sixty characters and no space.
  2. "keeps the folder browser inside the card": mock `listFolders` to answer
     `{ path: UNBROKEN_ROOT.path, parent: '/', folders: [{ name: 'x'.repeat(80), path: `${UNBROKEN_ROOT.path}/${'x'.repeat(80)}` }] }`,
     press `button[name="browse"]` in the card's picker, and also assert the
     `.folder-list button`'s right edge is not past its `.folder-browser`'s.
  3. "keeps the library's removal inside the window": open
     `button[name="remove-library"]` with `libraryRemovalCost` resolved, for
     the sixty-character name.
- "does not scroll sideways on the users screen": mock `listLibraries` with
  a library holding `UNBROKEN_ROOT` and give the account
  `libraries: [{ id: 1, name: 'Manga' }]`, so the grant editor draws its
  boxes and a folder path.

In `web/src/fieldNames.browser.test.tsx`:

- "names the add-root form's box as written" becomes "names the make-a-library
  form's boxes as written": mount `<Libraries />` over an empty list and
  expect `drawn` to equal `[['Name', 'uppercase'], ['Folder path', 'uppercase']]`
  and `controls` to equal `[['textbox', 'Name'], ['textbox', 'Folder path']]`.
- `UserRow` takes `libraries={[]}` (Task 10 changed the prop), and the mock
  of `listRoots` for `<Users />` becomes one of `listLibraries`.
- Where the file reads the named groups and headings of a card (O182), the
  user card's group is now named "Libraries this account may see", and a
  library's card has two headings: its name at level 3 and each folder's
  path at level 4. Add a case that mounts `<Libraries />` over one library
  of one folder and reads the headings Chromium's own tree holds, in order:
  "Libraries" (2), the library's name (3), the folder's path (4), "Make a
  library" (3).

Run: `pnpm -C web run test:browser`
Expected: PASS. A case that fails on a measurement is a finding about the
stylesheet: fix the rule, not the case, and say what the measurement was.

- [ ] **Step 5: Remove what nothing reads**

Delete `web/src/Roots.tsx` and `web/src/Roots.test.tsx`. In
`web/src/config.ts` delete `addRoot`, and in `config.test.ts` its cases and
import. `Added` and `Probe` stay: `libraries.ts` and the row read them, and
their doc comments name `POST /api/library/libraries` and
`POST /api/library/libraries/{id}/folders` as the two routes that probe.

`command grep -a -rn -E "Roots\b|addRoot|roots screen|listRoots" web/src`
and read every hit. `listRoots` stays where `RuleEditor.tsx` reads it. A doc
comment that points at `Roots.tsx` for a reason points at `Libraries.tsx`
where that file carries the reason, and otherwise states the reason itself.

- [ ] **Step 6: Look at it**

Build the app and serve it over a scratch database, as Task 12's Step 1
sets up, or over a scratch library of two small folders. Take these
screenshots at 390px and at 1280px, in the light scheme and the dark:

1. Configuration → Libraries with two libraries, one of two folders.
2. The same with a library's removal dialog open.
3. The make-a-library form with two folders chosen and Browse… open.
4. Configuration → Users with one account set to Only these.

Read each one. Write in the report, for each, one sentence on what a person
reading the screen would take the hierarchy to be: which line is the
library, which the folder, which the action. If that sentence is not what
the markup says, fix the stylesheet.

- [ ] **Step 7: Run everything and commit**

Run the four commands of Task 7's Step 8. Expected: all pass.

```bash
git add -A web/src
git commit -m "The Libraries screen's styles, its browser cases, and the Roots screen removed"
```

---

## Task 12: On real material, the docs, the sweep, the gate and the claim check

**Files:**
- Modify: `docs/status.md`, `docs/notes.md`, `docs/mutants-baseline.md`
- Create (gitignored): `.superpowers/sdd/libraries-f2a-evidence/*`,
  `.superpowers/sdd/libraries-f2a-pr-body.md`

**Interfaces:**
- Consumes: every earlier task, and each task's report of what it found on
  the way.
- Produces: a branch ready to offer the user. Nothing is pushed.

- [ ] **Step 1: Drive it over a copy of the real library**

Read `.superpowers/sdd/libraries-f1-evidence/` for how F1's Task 7 made its
scratch copy, its accounts and its server; do the same. In outline:

```bash
S="<your session scratchpad>/f2a-drive"; mkdir -p "$S/share"
sha256sum /home/davey/temp/manga-test/library.db >| "$S/original.sha256"
touch "$S/marker"
cp /home/davey/temp/manga-test/library.db "$S/library.db"
cargo build --release -p taiyaku-cli
pnpm -C web run build
target/release/taiyaku library libraries --db "$S/library.db"
```

Make two folders under `$S/share`, `Extras A` and `Extras B`, each holding
one small volume copied from the real library's folder (reading it is
allowed; nothing is written there). Give one of them a volume the copy
already holds, under the same series directory name, and the other a PDF.

Serve the copy on a free port with `--db "$S/library.db" --web web/dist
--folders "$S/share"`, and record the server's PID. Then, in a real browser,
signed in as an administrator:

1. `/config/libraries` lists the copy's one library with its work count, and
   `/config/roots` lands there.
2. Make a library `Drive Extras` from both folders, found with Browse…. Note
   what Browse… lists at the top, and that it will not go above `$S/share`.
3. Scan now on `Drive Extras`. Both folder rows show Scanning…, then a
   report each; the card's work count moves. Note the elapsed time.
4. Press Scan now on one folder row while the library's scan runs, with a
   larger folder if the scan is too quick to catch, and read the refusal.
5. The volume the copy already held is a work of `Drive Extras` too, and the
   first library's work count has not moved.
6. Rename `Drive Extras`; try to rename it to the first library's name and
   read the refusal.
7. On `/config/users`, make an account, choose Only these, tick the renamed
   library. As that account, in a private window, the grid shows that
   library's works and none of the first's. Back as the administrator, the
   last tick will not clear; Every library does.
8. Grant the account the renamed library alone again, then Remove library…
   on it: read the refusal. Grant it the first library too, and Remove
   library… again: read the counts, confirm, and read the list.
9. Make a library holding only the PDF folder, scan it, and read its card.

Record each answer, with the URL and what was on screen, in
`$EVID/drive.log`, stamped with the commit. Then:

```bash
kill <the server's PID>
sha256sum -c "$S/original.sha256"
find /home/davey/temp/manga-test -newer "$S/marker" | head
```

Expected: the digest matches and `find` prints nothing.

If any step shows something a person would call wrong, stop and report it
before the docs are written.

- [ ] **Step 2: `docs/status.md`**

Closing an O-row is five edits, and missing one leaves the document
contradicting itself: the row's checkbox; the row's text, saying what was
done and what remains, with what was filed kept and marked as filed; the
test count near the top, which appears twice; the "N are now closed, leaving
M" sentence; and the "What remains … M open items" line near the bottom,
whose count word wraps across a line break. Opening one moves the count word
at the head of the section, "leaving N", adds a row, and moves "What
remains". A row has exactly six `|` characters: a `|` inside a code span
still ends a cell, so write `\|`.

- Close **O193**: tick the row, and rewrite its prose to say what was done
  (the card grants libraries; the last tick cannot be cleared; "Every
  library" sends the empty list; a `roots` key is refused) and what was run
  (Step 1's item 7). Leave what was filed, marked as filed.
- **O192** stays open and gains one sentence: F2a adds no writer of
  `root.library_id` either, and F2b is where Move is built.
- Open a row for each defect a task's report set aside, each with the run or
  the reading it rests on, and "read" or "run" said.
- The synopsis block's `serve` line gains `[--folders DIR]...`.
- The paragraph that says what the web app's Configuration area holds says
  Libraries, not Roots.
- The Rust test count, at both of its sites, is the number the gate's JUnit
  report holds. The web's three counts are measured, not added to:
  `pnpm -C web exec vitest run --project node --reporter=default`, then
  `--project jsdom`, then `pnpm -C web run test:browser`, and each run's own
  total.
- Run the sweep, and read its four answers against the prose:

  ```bash
  command grep -a -c '^| O[0-9]' docs/status.md
  command grep -a -c '^| O[0-9]* | \[x\]' docs/status.md
  command grep -a -n -E '[0-9]{3,} tests|Nine of the' docs/status.md
  command grep -a -n -A1 '^What remains:' docs/status.md
  ```

- Then `command grep -a -rn 'O193' docs web/src crates` and read every hit:
  prose outside the table describes open items too, in words no sweep
  predicts. A historical section is left as written and marked
  (`*O193 has since closed; see its row.*`).

- [ ] **Step 3: `docs/notes.md`**

Add "## Libraries, F2a (<date>)" after "Libraries, F1 (2026-10-06)":

- **What changed**, in the terms of the spec's §6: the routes, the screen,
  the folder browser, the grants.
- **The four spec corrections** this plan lists, each with what was built.
- **What a library's removal counts**, with Task 2's spanning-work test named.
- **Scan now**: that each folder's report carries the one derive, and why a
  folder's progress is a running flag.
- **On a copy of the real library**: Step 1's answers, each as a sentence
  with its figure, and "not measured" for what was not.
- **Counts**, each against `main`.
- **Not measured**: the NAS; a directory listing on a sleeping disk (spec
  §13); a screen reader on the new controls; iOS Safari.

Every figure in it is one you read off a log in the evidence directory.

- [ ] **Step 4: The mutation sweep**

Commit first: `scripts/mutants.sh --since main` reads `git diff main...HEAD`.

```bash
git add docs && git commit -m "F2a: status, notes"
scripts/mutants.sh --since main >| "$EVID/mutants-$(git rev-parse --short HEAD).log" 2>&1
```

Read the result against its own traps: run a control first if the runner
reports every mutant red; check the unviable count against the functions
this branch changed; and remember that it makes no mutant for a string
literal, a struct field initialiser or a `json!` key, which is most of
`libraries_api.rs`. For those, the hand defects in Tasks 4 to 6 are the
evidence. A survivor is fixed with a test that fails against it, watched.
Record the run in `docs/mutants-baseline.md` as F1's row does.

- [ ] **Step 5: A review of the whole branch**

Ask a reviewer who has not seen the tasks to read `git diff main...HEAD`
whole, against the spec's §5, §6 and §6.1, with these questions: what does a
person reach on screen that no test drove; where does a route answer 500 for
a caller's mistake or 4xx for a fault; which sentence in a comment or a doc
did nobody check. Fix what it finds, with a test watched failing for each
code fix, and have the fixes reviewed too: a fix round is where the next
finding comes from.

- [ ] **Step 6: The gate**

```bash
H=$(git rev-parse --short HEAD)
env -u TAIYAKU_CORPUS NEXTEST_TEST_THREADS=8 RUST_TEST_THREADS=8 \
  scripts/check-all.sh >| "$EVID/gate-${H}.log" 2>&1; echo $? >| "$EVID/gate-${H}.exit"
command grep -a -n 'all green' "$EVID/gate-${H}.log"; cat "$EVID/gate-${H}.exit"
```

Expected: exit 0 and the `all green` line, with what it names as not
measured. Do not pipe the script: a pipe hides its exit status.

The three crates' tests also run on SQLite 3.40.1, which CI and the image
link where this workstation has 3.53:

```bash
H=$(git rev-parse --short=8 HEAD)
mkdir -p "target/bookworm/src-${H}" target/bookworm/target target/bookworm/cargo-home
git archive HEAD | tar -x -C "target/bookworm/src-${H}"
docker run --rm --user "$(id -u):$(id -g)" \
  -v "$PWD/target/bookworm/src-${H}:/src" -v "$PWD/target/bookworm/target:/t" \
  -v "$PWD/target/bookworm/cargo-home:/c" \
  -e CARGO_TARGET_DIR=/t -e CARGO_HOME=/c -e HOME=/c \
  -e RUSTUP_TOOLCHAIN=<the toolchain `rustup toolchain list` names inside the image> \
  -w /src rust:1-bookworm \
  cargo test --locked --no-fail-fast -p taiyaku-library -p taiyaku-serve -p taiyaku-cli \
  -- --test-threads=6 >| "$EVID/bookworm-${H}.log" 2>&1; echo $? >| "$EVID/bookworm-${H}.exit"
```

Two tests fail there by design, because they trace with `strace` and the
container has none: `stamp::tests::a_rewrite_syncs_…` and
`opds::tests::the_work_feed_stats_its_members_on_the_blocking_pool`. Any
other failure is real. `target/bookworm/` is build cache: leave it, and say
in the report that it is there.

- [ ] **Step 7: The PR body and the claim check**

Write `.superpowers/sdd/libraries-f2a-pr-body.md`: what F2a is, what it
closes (O193) and opens, what was run, what was not measured. No line is
hard-wrapped; it is the squash commit's message. Title: "Libraries, F2a: the
Libraries screen, the folder browser, and grants by library (closes O193)".

Then check every factual sentence in the status row, the notes section, the
PR body and this branch's new comments against the code, the logs and the
evidence, in a separate pass by a checker who is handed the files and not a
list of what to expect. Fix by deleting a figure or a reason before
correcting it a second time. Run the pass again on the fix commit, and again
until a pass finds no false sentence. After each docs fix, the gate runs
again if a count moved.

- [ ] **Step 8: Stop**

Report to the user: the head commit, the gate's line, the sweep's figures,
what the drive showed, each open question. Ask before pushing. Opening the PR
(through the gitea MCP, with the body above) and merging it are each a
separate yes.

---

## Self-review

**Spec coverage.**

| Spec | Task |
| --- | --- |
| §5: `Patch::libraries`, `roots` refused, `user_json` sends `libraries` | 3 |
| §5: the card's named choice; last tick; "Only these" sends nothing | 10 |
| §6: the list, with works and folders | 4, 8 |
| §6: create, a taken name refused, all folders or none | 1, 4, 8 |
| §6: rename; add a folder; folder settings; remove a folder | 1, 4, 8 (rows unchanged) |
| §6: libraries addressed by id | 1, 4, 7 |
| §6: Scan now, derives once, 409s both ways | 5, 8 |
| §6: each folder row keeps its Scan button | 7, 8 (`RootRow` unchanged) |
| §6: remove a library, counts first, refusal asked first | 2, 4, 8 |
| §6: a refusal drawn as its sentence | 7 and every screen after |
| §6: `POST /api/library/roots` removed | 4 |
| §6.1: the folder browser, prefixes, canonical comparison, symlinks | 6, 9 |
| §7: "no readable files yet" | 1 (`books`), 4, 8 |
| §8, §11: `TAIYAKU_FOLDERS` in the image, the README's mount | 6 |
| §12: the admin UI's and the grants' tests | 1 to 10 |
| §12: real material | 12 |
| §6.2, Move | not here: F2b |

**Not covered, on purpose.** The CLI gains no `library rename` or
`library remove`: the spec's CLI parity bullet names `scan --add`,
`libraries` and `scan --library`, which F1 shipped. `/api/library/status`
and the sidebar still speak of roots until F3.

**Names used across tasks.** `libraries::{name_of, rename, RenameError,
books}` (1 → 4); `scan::{InLibrary::Id, create_library, Created}` (1 → 4);
`scan::{library_removal_refusal, library_removal_cost, remove_library}`
(2 → 4); `account::Patch::libraries` (3); `config_api::{root_json,
added_json, removal_json, derive_report_json, finished_scan_json,
roots_with_last_scan, panic_message}` (4, 5); `libraries_api::{LibraryAnswer,
library_panicked, folders_of}` (4 → 5); `ServeState::folders`,
`ServeOptions::folders` (6); `libraries.ts`'s nine functions and four types
(7 → 8, 9, 10); `sentenceOf` (7 → 8, 9, 10); `RootRow`'s `adopt` and `lastOf`
(7 → 8); `FolderPicker`'s `action` and `onPick` (8 → 9); `UserRow`'s
`libraries` (10 → 11).
