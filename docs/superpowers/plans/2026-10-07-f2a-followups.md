# Libraries, F2a follow-ups: Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Close what F2a (#188) left open and the user ruled on, before F2b
(Move) builds on the same scan registry and removal code: the five defects
O196, O205, O200, O201 and O209, and the five decisions O195, O199, O198,
O207 and O206. O197 closes as accepted.

**Architecture:** Five Rust tasks, then four web tasks, then the documents.
The store learns two rules for a library's name (white space, ASCII case;
migration 16) and refuses to grant a library with no folder. The server stops
saying "root" in the sentences the Libraries screen draws, refuses a removal
while a scan of it is reserved, gives the per-folder scan job the library
job's re-read and guarded write-back, and lets the folder browser climb from
a nested prefix to the outer one. The web app asks before "Every library"
lifts a limit, renames the two scan buttons, labels the derive block as the
whole store's, draws a held button held, brings a refusal under its button
into view, and says why "Make the library" is held while a path waits in the
box.

**Tech Stack:** Rust 2024 (workspace `rust-version` 1.85), rusqlite against
the system SQLite, axum 0.8, cargo-nextest in the gate; React 19, react-router
8, vitest (projects `node`, `jsdom`, `browser`), biome, pnpm.

## The user's rulings, 2026-10-07

These are settled. Do not reopen them, and do not ask about them again.

- **O195**: the Users card asks before a limit is lifted. Choosing "Every
  library", by click or by an arrow key, on an account that holds grants asks
  first. Not a Save button.
- **O196**: refuse removing a library or a folder (409) while a scan of it is
  reserved; give the per-folder scan job (`config_api::run_scan_inner`) the
  library job's re-read under the job lock and its guarded write-back.
- **O197**: accept the folder browser's rename race as documented. No
  descriptor walk, no new dependency.
- **O198**: label the derive block of a folder's scan report as covering
  every library. Per-library figures are not part of this.
- **O199**: the library's button reads "Scan library", each folder's "Scan
  folder".
- **O200**: server refusals say "folder", not "root" or "(id N)"
  (`config_api::start_scan`'s 409, `scan::vetted`'s overlap sentences). The
  sidebar's "Roots" stays for F3, by the spec.
- **O201**: a disabled button gets a visible look of its own, in `base.css`,
  app-wide.
- **O205**: refuse granting a library that has no folder.
- **O206**: strip all surrounding white space from a library's name, not only
  spaces, and refuse a name that differs from another only by case; a
  migration with a case-insensitive unique index.
- **O207**: in the folder browser, Up from a configured folder that sits
  inside another goes to the outer folder (`canonical.parent().filter(within)`).
- **O208**: merging the code F2a wrote twice is **F2b's**, not this branch's.
- **O209**: a refusal that lands below the fold is scrolled into view; "Make
  the library" says why it is held when a path is typed but not added.
- **O202, O203, O204**: left as recorded. No action.

**Checked before this plan was written (2026-10-07):** neither local store
holds two library names that differ only in case or in surrounding white
space. The workstation's `~/.local/share/taiyaku/library.db` is at schema 15
with 13 distinct names; the real-library copy at
`/home/davey/temp/manga-test/library.db` is still at schema 13, with one
folder. The NAS store was not read; see "Finishing".

## Where a ruling covers part of a row

Four rows hold more than their ruling. The rest of each stays open, and the
row is narrowed, not closed (Task 10):

- **O199**: "Folder path", "Add folder", "Browse…" and "Name" still repeat
  in every card.
- **O200**: the sidebar's "Roots" (F3, by the spec), and the five components
  the row names that still say root (`EmptyState`, `RuleForm`, `ProposalRow`,
  `PreviewTabs`, `AcceptForm`). The rule routes' own "there is no root N",
  which the rule editor draws, stays with them.
- **O201**: no alert has a look of its own. Some of the app's
  `role="alert"` paragraphs are notices and not refusals (`RootRow`'s
  `.scan-notice`, `RuleEditor`'s `.held-edit`), so one colour for all of them
  is a design question, not this fix.
- **O209**: a library removed in another tab draws "there is no library N" on
  each press and nothing reads the list again; after a rename or a make whose
  re-read fails, the sentence says the list could not be read and not that the
  thing was done; and a library just made is drawn at the foot of the list.

## Global Constraints

- **Branch:** `f2a-followups`, cut from `main` at `8d9ea19c`. Its first
  commit is this plan. Work on it.
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
  parameters it binds.**
- **A Rust task is done only when whole crates pass**, not the files it
  touched: `cargo test -p taiyaku-library -p taiyaku-serve -p taiyaku-cli --
  --test-threads=8`, plus `cargo clippy --workspace --all-targets
  --all-features -- -D warnings` and `cargo fmt --all -- --check`.
- **A web task is done only when whole projects pass**: `pnpm -C web run lint`,
  `pnpm -C web run typecheck`,
  `pnpm -C web exec vitest run --project node --project jsdom --reporter=default`,
  and, for a task that changes a component or a stylesheet,
  `pnpm -C web run test:browser`. Under an agent vitest picks a reporter that
  hides every passing test's console output; `--reporter=default` shows it.
- **A real-key browser test focuses the page first** (a click on the control),
  since a key pressed before anything in the test frame has focus goes to the
  frame. Run each new real-key case alone with `-t` as well as in the project.
- **A class a component writes has a stylesheet rule in the same commit.**
  `web/src/classNames.test.ts`, in the node project, fails for a class no
  selector names.
- **A failure is "pre-existing" only if it fails the same way on `main` at
  `8d9ea19c`**, checked in a worktree, never against another commit on this
  branch.
- **No figure in a code comment that a later change would have to correct**:
  no counts of tests, call sites, routes or libraries.
- **Comments say what the code does and why it must.** Write them in the
  surrounding files' voice. Do not narrate this plan, its tasks or its reviews
  in them. A comment that a change makes false is corrected in the same
  commit: grep the old words (`Scan now`, `stripped of spaces`, `root {id}`)
  in the files you touch.
- **The snippets below are a starting point.** The interfaces are exact: the
  names, parameters, return types, routes, sentences and JSON keys. The
  fixtures' plumbing (helper names, imports, `expect` texts) is yours to fix
  against the code as it is. Re-read every signature a step names before
  writing against it. If a step's *claim about existing code* is wrong, stop
  and report rather than work around it.
- **Evidence goes in `.superpowers/sdd/f2a-followups-evidence/`**, each log
  stamped with the commit it ran at. `target/` is not durable.
- **Opus is unavailable to subagents until 2026-10-11 15:00 PT** (weekly
  limit); dispatch sonnet.

## File map

| File | Tasks | What changes |
|---|---|---|
| `crates/taiyaku-library/src/schema/016_library_names.sql` | 1 | new: names stripped of white space, the case index, the trigger made again |
| `crates/taiyaku-library/src/schema.rs`, `schema/tests.rs` | 1 | version 16 |
| `crates/taiyaku-library/src/libraries.rs`, `libraries/tests.rs` | 1 | `trim_name`, `id_of`, `create`, `rename` |
| `crates/taiyaku-library/src/scan.rs`, `scan/tests.rs` | 1, 3 | `own_library_clash`, `create_library`; `vetted`'s sentences, `only_grant_of`'s full stop |
| `crates/taiyaku-library/src/account.rs`, `account/tests.rs` | 2 | `ungrantable`, used by `grant_library` and `update` |
| `crates/taiyaku-serve/src/config_api.rs`, `config_api/tests.rs` | 3, 4 | sentences; removal refused while reserved; `scan_and_finish`, `finish_reservation`, the job's re-read |
| `crates/taiyaku-serve/src/libraries_api.rs`, `libraries_api/tests.rs` | 4 | removal refused while reserved; `finish_reservation` |
| `crates/taiyaku-serve/src/test_support.rs` | 4 | `HeldJob`/`holding_the_job` move here |
| `crates/taiyaku-serve/src/users_api/tests.rs` | 2 | a 409 for a folderless grant |
| `crates/taiyaku-serve/src/folders_api.rs`, `folders_api/tests.rs` | 5 | Up from a nested prefix |
| `crates/taiyaku-cli/src/library/tests.rs` | 3 | the overlap sentences |
| `web/src/UserRow.tsx`, `UserRow.test.tsx`, `UserRow.browser.test.tsx` (new) | 6 | the lift question |
| `web/src/LibraryCard.tsx`, `RootRow.tsx`, `ScanReport.tsx` and their tests | 7 | the words |
| `web/src/styles/base.css`, `web/src/buttons.browser.test.tsx` (new) | 8 | a held button's look |
| `web/src/reveal.ts` (new), `web/src/Refusal.tsx` (new), `Libraries.tsx`, `LibraryCard.tsx`, `FolderPicker.tsx` and tests | 9 | refusals into view; the held make button's sentence |
| `web/src/styles/admin.css` | 6 | `.lift-limit` |
| `README.md`, `docs/status.md`, `docs/notes.md` | 7, 10 | words, rows, counts, notes |

**Lanes.** Tasks 1–5 are the Rust lane and run in order on the branch: 1 and
3 both edit `scan.rs`, 3 and 4 both edit `config_api.rs`. Tasks 6–9 are the
web lane and run in order (7 and 9 both edit `LibraryCard.tsx`); no web task
touches a file a Rust task does, so the web lane may run beside the Rust lane
in a worktree of its own and be cherry-picked. Task 10 runs last.

---

### Task 1: A library's name: white space and case (O206)

**Files:**
- Create: `crates/taiyaku-library/src/schema/016_library_names.sql`
- Modify: `crates/taiyaku-library/src/schema.rs` (`SCHEMA_VERSION`, `MIGRATIONS`)
- Modify: `crates/taiyaku-library/src/libraries.rs` (`trim_name`, `id_of`, `create`, `rename`)
- Modify: `crates/taiyaku-library/src/scan.rs` (`own_library_clash`, `add_root_checked_in`, `create_library`)
- Test: `crates/taiyaku-library/src/libraries/tests.rs`, `schema/tests.rs`, `scan/tests.rs`

**Interfaces:**
- Produces: `libraries::trim_name(&str) -> &str` strips what `str::trim`
  strips. `libraries::id_of(conn, name)` answers the library whose name is
  `trim_name(name)` in any ASCII case. Schema version 16. A unique index
  `library_name_folded` on `library (name COLLATE NOCASE)`.
- Sentences: `rename`'s refusal is ``a library is already called `<the
  holder's name as stored>` ``; `create_library`'s is ``a library is already
  called `<the holder's name as stored>`; add the folder to it instead``.

**What the white space is.** Unicode's White_Space, which is what `str::trim`
and `char::is_whitespace` use: U+0009–U+000D, U+0020, U+0085, U+00A0,
U+1680, U+2000–U+200A, U+2028, U+2029, U+202F, U+205F, U+3000. In SQL that is
the one list, written identically everywhere it appears:

```
char(9, 10, 11, 12, 13, 32, 133, 160, 5760, 8192, 8193, 8194, 8195, 8196, 8197, 8198, 8199, 8200, 8201, 8202, 8232, 8233, 8239, 8287, 12288)
```

Below, `WS` stands for that whole expression. Write it out in full in the SQL
file; SQL has no constants inside a trigger.

- [ ] **Step 1: Write the failing tests**

In `crates/taiyaku-library/src/libraries/tests.rs`, **delete**
`a_name_finds_the_library_holding_exactly_it_before_the_one_holding_it_stripped`
(the exact-first lookup it pins goes: migration 16 leaves no stored name with
white space at its ends) and add:

```rust
/// Every white-space character Unicode names, at either end, and nothing
/// inside a name or outside that set.
#[test]
fn trim_name_strips_white_space_and_nothing_else() {
    assert_eq!(trim_name("\t\u{3000}Manga\u{a0}\n "), "Manga");
    assert_eq!(
        trim_name("\u{200B}Manga"),
        "\u{200B}Manga",
        "a zero-width space is not white space"
    );
    assert_eq!(trim_name("Light novels"), "Light novels");
}

/// Every list migration 16 strips with is the set [`trim_name`] strips, so a
/// name the trigger makes is found by a lookup made in Rust.
#[test]
fn migration_16_strips_the_characters_trim_name_strips() {
    let rust: Vec<u32> = (0..=0x10_FFFF_u32)
        .filter_map(char::from_u32)
        .filter(|c| c.is_whitespace())
        .map(u32::from)
        .collect();
    let sql = include_str!("../schema/016_library_names.sql");
    let lists: Vec<Vec<u32>> = sql
        .match_indices("char(")
        .map(|(at, _)| {
            let rest = &sql[at + "char(".len()..];
            rest[..rest.find(')').expect("a closing parenthesis")]
                .split(',')
                .map(|n| n.trim().parse().expect("a code point"))
                .collect()
        })
        .collect();
    assert!(!lists.is_empty(), "the migration strips with a list");
    for list in lists {
        assert_eq!(list, rust);
    }
}

/// A name is found stripped and in any ASCII case.
#[test]
fn a_name_finds_its_library_stripped_and_in_any_case() {
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    conn.execute_batch("INSERT INTO library (id, name) VALUES (1, 'Manga'), (3, 'Comics');")
        .expect("two libraries");

    let found = |name: &str| id_of(&conn, name).expect("reads");
    assert_eq!(found("Manga"), Some(1));
    assert_eq!(found(" MANGA\t"), Some(1));
    assert_eq!(found("\u{3000}comics\n"), Some(3));
    assert_eq!(found("Zines"), None);
}

/// Another library's name in another case is refused in the holder's own
/// spelling; a library's own name in another case is a rename.
#[test]
fn a_rename_is_refused_a_name_another_library_holds_in_another_case() {
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    conn.execute_batch("INSERT INTO library (id, name) VALUES (1, 'Manga'), (2, 'Comics');")
        .expect("two libraries");

    assert_eq!(
        rename(&conn, 2, "\tMANGA\n"),
        Err(RenameError::Unusable(
            "a library is already called `Manga`".to_string()
        ))
    );
    assert_eq!(rename(&conn, 1, " MANGA "), Ok("MANGA".to_string()));
    assert_eq!(
        rename(&conn, 2, "\u{3000}Light novels\n"),
        Ok("Light novels".to_string())
    );
}
```

In `crates/taiyaku-library/src/schema/tests.rs`, **replace**
`a_root_written_without_a_library_is_given_a_name_without_spaces` (its fourth
root, `\tTab\t`, now loses its tabs) with:

```rust
/// The trigger strips every character `libraries::trim_name` strips, each on
/// its own at both ends of a folder's name, and keeps one that is not white
/// space. A name taken in another case is taken.
#[test]
fn a_root_written_without_a_library_is_named_without_white_space_or_a_case_clash() {
    let conn = Connection::open_in_memory().expect("in-memory connection");
    migrate(&conn).expect("migrates");
    let name_of_root = |path: &str| -> String {
        conn.query_row(
            "SELECT l.name FROM root r JOIN library l ON l.id = r.library_id WHERE r.path = ?1",
            [path],
            |row| row.get(0),
        )
        .expect("the root has a library")
    };

    let spaces = (0..=0x10_FFFF_u32)
        .filter_map(char::from_u32)
        .filter(|c| c.is_whitespace());
    for (n, c) in spaces.enumerate() {
        let path = format!("/w{n}/{c}Leaf{n}{c}");
        conn.execute("INSERT INTO root (path) VALUES (?1)", [&path])
            .expect("a root");
        assert_eq!(name_of_root(&path), format!("Leaf{n}"), "U+{:04X}", u32::from(c));
    }

    conn.execute("INSERT INTO root (path) VALUES ('/z/\u{200B}Zero\u{200B}')", [])
        .expect("a root");
    assert_eq!(name_of_root("/z/\u{200B}Zero\u{200B}"), "\u{200B}Zero\u{200B}");

    conn.execute("INSERT INTO root (path) VALUES ('/x/Extras')", [])
        .expect("a root named Extras");
    conn.execute("INSERT INTO root (path) VALUES ('/y/EXTRAS')", [])
        .expect("a root whose name is taken in another case");
    assert_eq!(name_of_root("/y/EXTRAS"), "/y/EXTRAS");
}

/// **Migration 16 brings the names already stored under both rules.** A name
/// that is an older library's once stripped and folded keeps its id beside
/// it; every other name is stripped; one that is only white space is left.
/// `' Manga '` would collide with `'Manga'` on `library.name`'s own UNIQUE if
/// it were stripped first, so the order of the two steps is held too.
#[test]
fn migration_16_strips_names_and_keeps_an_id_beside_a_name_another_holds() {
    let conn = Connection::open_in_memory().expect("in-memory connection");
    conn.pragma_update(None, "foreign_keys", "ON")
        .expect("foreign keys on, as `Library::open` sets them");
    migrate_with(&conn, &MIGRATIONS[..15]).expect("migrates to 15");
    conn.execute_batch(
        "INSERT INTO library (id, name) VALUES
           (1, 'Manga'), (2, ' Manga '), (3, 'MANGA'), (4, 'Comics' || char(9)), (5, char(9));",
    )
    .expect("five libraries");

    migrate(&conn).expect("migrates to 16");

    assert_eq!(
        library_names(&conn, "SELECT id, name FROM library ORDER BY id"),
        vec![
            (1, Some("Manga".to_string())),
            (2, Some("Manga (2)".to_string())),
            (3, Some("MANGA (3)".to_string())),
            (4, Some("Comics".to_string())),
            (5, Some("\t".to_string())),
        ]
    );
    let refused = conn
        .execute("INSERT INTO library (name) VALUES ('COMICS')", [])
        .expect_err("one name in another case");
    assert!(refused.to_string().contains("UNIQUE"), "{refused}");
}
```

Every existing test whose name begins `migration_15_` and calls `migrate`
after writing its schema-14 rows changes that call to
`migrate_with(&conn, &MIGRATIONS[..15])`, so it still tests migration 15
alone: `migration_15_keeps_whole_paths_that_trimming_would_make_the_same`
would otherwise see migration 16 rename one of its two libraries. The test
that pins `SCHEMA_VERSION` to 15 pins 16.

In `crates/taiyaku-library/src/scan/tests.rs`, **replace**
`a_folder_joins_the_library_holding_exactly_the_name_given` (two libraries a
space apart, which the lookup no longer tells apart) with
`a_folder_joins_the_library_its_name_names_in_any_case`, and add two more:

```rust
/// A named add finds the library stripped and in any ASCII case, and makes no
/// second one.
#[test]
fn a_folder_joins_the_library_its_name_names_in_any_case() {
    let parent = crate::tests::tempdir("scan-library-any-case");
    let (first, second) = (parent.path().join("Volumes"), parent.path().join("Chapters"));
    for dir in [&first, &second] {
        std::fs::create_dir_all(dir).expect("mkdir");
    }
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let made = add_root_in(&conn, &first, InLibrary::Named("Manga")).expect("makes Manga");

    let joined = add_root_in(&conn, &second, InLibrary::Named("\tMANGA ")).expect("joins it");

    assert_eq!(joined.library_id, made.library_id);
    assert_eq!(crate::libraries::list(&conn).expect("lists").len(), 1);
}

/// A library is not made under another's name in another case, and the
/// refusal names the library as it is stored.
#[test]
fn a_library_is_not_made_under_a_name_another_holds_in_another_case() {
    let parent = crate::tests::tempdir("scan-create-case");
    let (first, second) = (parent.path().join("Volumes"), parent.path().join("Chapters"));
    for dir in [&first, &second] {
        std::fs::create_dir_all(dir).expect("mkdir");
    }
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    create_library(&conn, "Manga", std::slice::from_ref(&first)).expect("makes Manga");

    let why = create_library(&conn, "\tMANGA ", std::slice::from_ref(&second))
        .expect_err("the name is taken in another case");

    assert_eq!(
        why,
        AddRootError::Unusable(
            "a library is already called `Manga`; add the folder to it instead".to_string()
        )
    );
    assert_eq!(roots(&conn).expect("reads").len(), 1, "the second folder was not added");
}

/// Where both names a folder could be a library under are taken in another
/// case, the folder is refused as the caller's mistake, naming both, and not
/// left to the trigger's insert to fail on the index as a store fault.
#[test]
fn a_folder_whose_two_names_are_taken_in_another_case_is_refused_naming_both() {
    let parent = crate::tests::tempdir("scan-own-case");
    let folder = parent.path().join("Manga");
    std::fs::create_dir_all(&folder).expect("mkdir");
    let canonical = folder.canonicalize().expect("canonical");
    let whole = canonical.to_string_lossy().to_ascii_uppercase();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    conn.execute(
        "INSERT INTO library (id, name) VALUES (1, 'manga'), (2, ?1)",
        [&whole],
    )
    .expect("both names, in another case");

    let why = add_root_checked(&conn, &folder).expect_err("both names are taken");

    assert!(
        matches!(&why, AddRootError::Unusable(text) if text.contains("`Manga`")),
        "{why:?}"
    );
}
```

(If the tempdir's path is already all capitals, the third test proves
nothing; assert `whole != canonical.to_string_lossy()` first.)

- [ ] **Step 2: Run the tests to watch them fail**

Run: `cargo test -p taiyaku-library -- --test-threads=8 trim_name_strips migration_16 a_name_finds_its_library a_rename_is_refused_a_name a_root_written_without_a_library a_folder_joins_the_library_its_name a_library_is_not_made_under_a_name_another a_folder_whose_two_names`
Expected: compile failure first (`016_library_names.sql` does not exist), then
each fails on the behaviour it names.

- [ ] **Step 3: Write the migration**

`crates/taiyaku-library/src/schema/016_library_names.sql` — `WS` written out
in full each time. **The comments must not contain the text `char(`**, or the
test that reads the lists reads a comment as one.

```sql
-- O206: a library's name is stripped of white space at both ends, not of
-- spaces only, and two names that differ only in ASCII case are one name.
--
-- **The characters stripped are Unicode's White_Space**, the set Rust's
-- `str::trim` strips in `libraries::trim_name`, written below as the code
-- points of each call to `char`. Migration 15 stripped with SQLite's
-- one-argument `trim`, which takes spaces and nothing else, so a tab or a
-- newline at the end of a name stayed in it. `libraries/tests.rs` checks every
-- list here against the set Rust strips.
--
-- **Case is ASCII case**, which is what `COLLATE NOCASE` folds: `Manga` and
-- `MANGA` are one name, and either finds the library called the other. Letters
-- outside ASCII are compared as they are.
--
-- **The names already stored are brought under both rules first**, since the
-- index cannot be made over two names it would refuse. A name that is an older
-- library's once both are stripped and folded keeps its library's id beside
-- it, `Manga (7)`, and the older library keeps its name. Every other name is
-- stripped, in that order, because stripping first would put `' Manga '` on
-- `'Manga'` under `library.name`'s own UNIQUE. A name that is only white space
-- is left as it is: stripped, it would be no name. A store where a name this
-- makes is already taken, `Manga (7)` beside the library `manga` whose id is
-- 7, fails the migration and is left as it was.
UPDATE library
   SET name = trim(name, WS) || ' (' || id || ')'
 WHERE trim(name, WS) <> ''
   AND EXISTS (SELECT 1 FROM library older
                WHERE older.id < library.id
                  AND trim(older.name, WS) = trim(library.name, WS) COLLATE NOCASE);

UPDATE library
   SET name = trim(name, WS)
 WHERE name <> trim(name, WS)
   AND trim(name, WS) <> '';

CREATE UNIQUE INDEX library_name_folded ON library (name COLLATE NOCASE);

-- `root_gets_a_library` asks both rules of the name it gives a folder, or its
-- insert would fail the index as a store fault. As migration 15 made it
-- otherwise: the folder's last path component, or its whole path where the
-- component is empty or taken. `last_insert_rowid()` inside a trigger is the
-- row the trigger inserted, and reverts to the caller's when it ends.
DROP TRIGGER root_gets_a_library;
CREATE TRIGGER root_gets_a_library AFTER INSERT ON root
WHEN new.library_id IS NULL
BEGIN
  INSERT INTO library (name)
  SELECT CASE WHEN leaf = '' OR EXISTS (SELECT 1 FROM library WHERE name = leaf COLLATE NOCASE)
              THEN trim(new.path, WS) ELSE leaf END
    FROM (SELECT trim(substr(new.path,
                             length(rtrim(new.path, replace(new.path, '/', ''))) + 1),
                      WS) AS leaf);
  UPDATE root SET library_id = last_insert_rowid() WHERE id = new.id;
END;
```

In `schema.rs`: `SCHEMA_VERSION` becomes `16`, `MIGRATIONS` becomes
`[&str; 16]` with `include_str!("schema/016_library_names.sql")` appended.

- [ ] **Step 4: Change `libraries.rs`**

```rust
/// `name` with white space stripped from both ends: the characters Unicode
/// calls White_Space, which `str::trim` strips.
///
/// **One character set wherever a library name is made or looked up.**
/// Migration 16's trigger names a library with SQLite's two-argument `trim`
/// over the same characters, listed by code point, so a name made by either
/// is found by a lookup made through this one. `libraries/tests.rs` checks the
/// lists against this.
pub(crate) fn trim_name(name: &str) -> &str {
    name.trim()
}

/// The library called `name`, if there is one: the library whose name is
/// `name` stripped by [`trim_name`], in any ASCII case.
///
/// At most one answers. A name is stored stripped, and migration 16's index
/// refuses two names that differ only in ASCII case.
///
/// # Errors
///
/// Returns an error if the read fails.
pub fn id_of(conn: &Connection, name: &str) -> Result<Option<i64>, String> {
    conn.query_row(
        "SELECT id FROM library WHERE name = ?1 COLLATE NOCASE",
        [trim_name(name)],
        |r| r.get(0),
    )
    .optional()
    .map_err(|e| format!("looking up the library `{name}`: {e}"))
}
```

`create` refuses with `if trim_name(name).is_empty()` and drops the comment
about "any whitespace, not only spaces", which no longer says anything
`trim_name` does not. `rename`:

```rust
/// Rename library `id`, storing `name` stripped by [`trim_name`], and answer
/// the name stored.
///
/// **`library.name`'s UNIQUE and migration 16's index are what refuse a name
/// another library holds**, in any ASCII case. A lookup before the write
/// would be a second statement for a second writer to land between, and the
/// CLI is one. The refusal names the other library as it is stored, which is
/// not `name` where only the case differs.
///
/// # Errors
///
/// See [`RenameError`].
pub fn rename(conn: &Connection, id: i64, name: &str) -> Result<String, RenameError> {
    let name = trim_name(name);
    if name.is_empty() {
        return Err(RenameError::Unusable("a library needs a name".to_string()));
    }
    match conn.execute(
        "UPDATE library SET name = ?2 WHERE id = ?1",
        rusqlite::params![id, name],
    ) {
        Ok(0) => Err(RenameError::Missing(id)),
        Ok(_) => Ok(name.to_string()),
        Err(rusqlite::Error::SqliteFailure(e, _))
            if e.extended_code == rusqlite::ffi::SQLITE_CONSTRAINT_UNIQUE =>
        {
            let holder: Option<String> = conn
                .query_row(
                    "SELECT name FROM library WHERE name = ?1 COLLATE NOCASE AND id <> ?2",
                    rusqlite::params![name, id],
                    |r| r.get(0),
                )
                .optional()
                .map_err(|e| RenameError::Failed(format!("renaming library {id}: {e}")))?;
            Err(RenameError::Unusable(format!(
                "a library is already called `{}`",
                holder.as_deref().unwrap_or(name)
            )))
        }
        Err(e) => Err(RenameError::Failed(format!("renaming library {id}: {e}"))),
    }
}
```

- [ ] **Step 5: Change `scan.rs`**

- `own_library_clash`'s `taken` asks
  `SELECT EXISTS (SELECT 1 FROM library WHERE name = ?1 COLLATE NOCASE)`, and
  its doc says the trigger strips white space and asks in any ASCII case.
- `add_root_checked_in`'s named-library guard is
  `InLibrary::Named(name) if crate::libraries::trim_name(name).is_empty()`,
  without the comment about lookups keeping `trim_name`'s set.
- `create_library` refuses an empty name with
  `crate::libraries::trim_name(name).is_empty()`, and its in-transaction check
  names the holder as stored:

```rust
    if let Some(held) = crate::libraries::id_of(&tx, name).map_err(AddRootError::Failed)? {
        let stored = crate::libraries::name_of(&tx, held)
            .map_err(AddRootError::Failed)?
            .unwrap_or_else(|| name.to_string());
        return Err(AddRootError::Unusable(format!(
            "a library is already called `{stored}`; add the folder to it instead"
        )));
    }
```

Grep `spaces` in `libraries.rs`, `scan.rs` and the module doc of
`libraries.rs`; correct every comment that says a name is stripped of spaces.

- [ ] **Step 6: Run the tests to watch them pass**

Run: the Step 2 command. Expected: PASS.

- [ ] **Step 7: Watch each defect make its test fail**

One at a time, restoring each before the next:
1. `trim_name` back to `name.trim_matches(' ')` → `trim_name_strips_white_space_and_nothing_else`.
2. Take `8287, ` out of the trigger's first list only → `migration_16_strips_the_characters_trim_name_strips` and `a_root_written_without_a_library_is_named_without_white_space_or_a_case_clash`.
3. `id_of`'s `COLLATE NOCASE` removed → `a_name_finds_its_library_stripped_and_in_any_case`.
4. The trigger's `name = leaf COLLATE NOCASE` → `name = leaf` → the trigger test (the `/y/EXTRAS` insert fails).
5. `own_library_clash`'s `COLLATE NOCASE` removed → `a_folder_whose_two_names_are_taken_in_another_case_is_refused_naming_both`.
6. The migration's first `UPDATE` deleted → `migration_16_strips_names_and_keeps_an_id_beside_a_name_another_holds`.
7. The `CREATE UNIQUE INDEX` deleted → the migration test's `expect_err` and `a_rename_is_refused_a_name_another_library_holds_in_another_case`.
8. `rename`'s refusal formats `name` instead of `holder` → the rename test.
9. `create_library`'s refusal formats `name` instead of `stored` → `a_library_is_not_made_under_a_name_another_holds_in_another_case`.

- [ ] **Step 8: Whole crates, clippy, fmt**

Run the Global Constraints' Rust commands. Tests elsewhere that pinned a name
with a tab or a spaced lookup (the CLI's included) are updated to the new
rule, each named in the report with the reason.

- [ ] **Step 9: Commit**

```bash
git add crates/taiyaku-library
git commit -m "O206: a library's name is stripped of white space and unique in any case (schema 16)"
```

---

### Task 2: No grant of a library that has no folder (O205)

**Files:**
- Modify: `crates/taiyaku-library/src/account.rs` (`grant_library`, `update`, new private `ungrantable`)
- Test: `crates/taiyaku-library/src/account/tests.rs`, `crates/taiyaku-serve/src/users_api/tests.rs`

**Interfaces:**
- Sentences: ``the library `<name>` has no folder, so there is nothing in it
  to grant`` (`update`: `WriteError::Refused`, 409;
  `grant_library`: `Err`); `there is no library <id> to grant` (`update`:
  `WriteError::Unusable`, as now; `grant_library`: `Err`, where it was
  SQLite's foreign-key text).

- [ ] **Step 1: Write the failing tests**

`account/tests.rs`:

```rust
/// A library with no folder resolves to no folder, which the guard reads as
/// unrestricted, so granting it is refused, by the patch and by the CLI's
/// call alike, and nothing is granted.
#[test]
fn a_library_with_no_folder_is_not_granted() {
    let library = claimed_library();
    let conn = library.conn();
    a_two_folder_library_and_another(&conn);
    conn.execute("INSERT INTO library (id, name) VALUES (90, 'Empty')", [])
        .expect("a library with no folder, as only a hand can make one");
    let id = add(&conn, "reader", "pw", "user").expect("adds an account");

    let refused = update(
        &conn,
        id,
        &Patch { libraries: Some(vec![70, 90]), ..Patch::default() },
        None,
    )
    .expect_err("Empty has no folder");
    assert_eq!(
        refused,
        WriteError::Refused(
            "the library `Empty` has no folder, so there is nothing in it to grant"
                .to_string()
        )
    );
    assert!(libraries_for(&conn, id).expect("reads").is_empty(), "Shelf was not granted either");

    let why = grant_library(&conn, id, 90).expect_err("Empty has no folder");
    assert!(why.contains("`Empty` has no folder"), "{why}");
    assert!(libraries_for(&conn, id).expect("reads").is_empty());
}
```

(`Patch`'s field names and `Default` are as `account.rs` has them; read them.)

`users_api/tests.rs`: one route case. A `library` row with no folder inserted
by hand into the fixture's store; `PATCH /api/library/users/{id}` with
`{"libraries": [<that id>]}` answers 409 with the sentence above as its
`error`, and the account's grants are unchanged.

- [ ] **Step 2: Run them to watch them fail**

Run: `cargo test -p taiyaku-library -p taiyaku-serve -- --test-threads=8 no_folder`
(name the serve case so this filter reaches it). Expected: FAIL — the patch
grants, `grant_library` answers `Ok(true)`, the route answers 200.

- [ ] **Step 3: Write the check**

In `account.rs`, beside `grant_library`:

```rust
/// Why a library cannot be granted.
enum Ungrantable {
    /// No library has the id.
    Missing,
    /// The library, by name, has no folder.
    NoFolder(String),
}

/// Why library `library_id` may not be granted, if it may not.
///
/// **A library with no folder is refused**: [`roots_for`] resolves a grant to
/// its library's folders, and an account whose grants resolve to none is
/// unrestricted, so as an account's only grant it would lift the limit it was
/// meant to set. No
/// public path makes such a library (`crate::libraries`), and this holds the
/// line where a grant is written, not only where libraries are made.
fn ungrantable(conn: &Connection, library_id: i64) -> Result<Option<Ungrantable>, String> {
    conn.query_row(
        "SELECT name, EXISTS (SELECT 1 FROM root WHERE library_id = ?1)
           FROM library WHERE id = ?1",
        params![library_id],
        |row| Ok((row.get::<_, String>(0)?, row.get::<_, bool>(1)?)),
    )
    .optional()
    .map(|found| match found {
        None => Some(Ungrantable::Missing),
        Some((name, false)) => Some(Ungrantable::NoFolder(name)),
        Some((_, true)) => None,
    })
    .map_err(|e| format!("checking whether library {library_id} can be granted: {e}"))
}

fn no_folder(name: &str) -> String {
    format!(
        "the library `{name}` has no folder, so there is nothing in it to grant"
    )
}
```

`grant_library` asks it before its insert:

```rust
    match ungrantable(conn, library_id)? {
        Some(Ungrantable::Missing) => {
            return Err(format!("there is no library {library_id} to grant"));
        }
        Some(Ungrantable::NoFolder(name)) => return Err(no_folder(&name)),
        None => {}
    }
```

and its `# Errors` says so. In `update`, the loop over `wanted` replaces its
`SELECT EXISTS` with:

```rust
            match ungrantable(conn, *library_id).map_err(WriteError::Failed)? {
                Some(Ungrantable::Missing) => {
                    return Err(WriteError::Unusable(format!(
                        "there is no library {library_id} to grant"
                    )));
                }
                Some(Ungrantable::NoFolder(name)) => {
                    return Err(WriteError::Refused(no_folder(&name)));
                }
                None => {}
            }
```

keeping the comment above it about the list being checked whole before a
grant is written, and adding the folderless case to it. `roots_for`'s doc
("a granted library always has a folder") now names this check as what holds
it.

- [ ] **Step 4: Run them to watch them pass**

Run: the Step 2 command. Expected: PASS.

- [ ] **Step 5: Watch the defects make them fail**

1. `Some((name, false)) => Some(Ungrantable::NoFolder(name))` → `Some((_, false)) => None` → both new tests.
2. `update` mapping `NoFolder` to `WriteError::Unusable` → the account test's `assert_eq!` and the route's 409.

- [ ] **Step 6: Whole crates, clippy, fmt; commit**

```bash
git add crates/taiyaku-library/src/account.rs crates/taiyaku-library/src/account/tests.rs crates/taiyaku-serve/src/users_api/tests.rs
git commit -m "O205: a library with no folder is not granted"
```

---

### Task 3: The server's refusals say "folder" (O200, the server's half)

**Files:**
- Modify: `crates/taiyaku-library/src/scan.rs` (`vetted`, `only_grant_of`)
- Modify: `crates/taiyaku-serve/src/config_api.rs` (`start_scan`, `run_scan_inner`, `RootAnswer`, `root_patch_from_json`)
- Test: `crates/taiyaku-library/src/scan/tests.rs`, `crates/taiyaku-serve/src/config_api/tests.rs`, `crates/taiyaku-cli/src/library/tests.rs`

**Interfaces — the sentences, exactly:**

| Where | Was | Is |
|---|---|---|
| `scan::vetted`, inside | ``<new> is inside existing root <old> (id <n>)`` | ``<new> is inside <old>, a folder of the library `<name>` `` |
| `scan::vetted`, around | ``<new> contains existing root <old> (id <n>)`` | ``<new> contains <old>, a folder of the library `<name>` `` |
| `scan::only_grant_of` | `… or revoke this one` | `… or revoke this one.` (both advices) |
| `config_api::start_scan` 409 | `a scan of root <id> (<path>) is already running` | `a scan of <path> is already running` |
| `config_api::run_scan_inner` | `root <id> (<path>) scanned, but deriving its structure afterward failed: <why>` | `<path> was scanned, but deriving its structure afterward failed: <why>` |
| `RootAnswer::Missing` 404 | `there is no root <id>` | `there is no folder <id>` |
| `root_patch_from_json` | `a root patch must be a JSON object`; ``the root's `<field>` must be …`` | `a folder's settings must be a JSON object`; ``the folder's `<field>` must be …`` |

The rule routes' `there is no root <id>` (`check_proposed_roots`,
`create_rule`, `patch_rule`) stays: the rule editor draws it, and it goes with
the components O200 leaves for F3.

- [ ] **Step 1: Change the tests to the new sentences**

- `scan/tests.rs`: the two overlap tests near `the message must identify the
  existing root by path or id` assert the whole sentence for each direction,
  with the library's name; `a_library_is_not_made_under_a_name_already_taken`'s
  `contains("is inside existing root")` becomes `contains(" is inside ")` and
  `contains("a folder of the library")`. The four full only-grant sentences
  end with `.`.
- `config_api/tests.rs`: `a_second_scan_of_a_root_already_scanning_is_refused`
  reads the 409's body and asserts `{"error": "a scan of <path> is already
  running"}`; `patching_or_deleting_an_unknown_root_is_a_404` asserts
  `there is no folder <id>`; `a_derive_failure_after_a_successful_scan_corrects_last_scan_status`
  asserts the report's `error` begins `<path> was scanned, but deriving`;
  `patching_a_root_with_bad_fields_names_a_root_not_a_rule` asserts
  ``the folder's `<field>` `` and the rule-route cases beside it assert
  `!contains("the folder's")`.
- `taiyaku-cli/src/library/tests.rs`: the two `contains("… existing root")`
  assertions become `contains(" is inside ")` / `contains(" contains ")` with
  `contains("a folder of the library")`.

- [ ] **Step 2: Run them to watch them fail**

Run: `cargo test -p taiyaku-library -p taiyaku-serve -p taiyaku-cli -- --test-threads=8`
Expected: the changed assertions fail on the old sentences.

- [ ] **Step 3: Change the sentences**

`vetted`:

```rust
    for existing in roots(conn).map_err(AddRootError::Failed)? {
        let relation = if canonical.starts_with(&existing.path) {
            "is inside"
        } else if existing.path.starts_with(&canonical) {
            "contains"
        } else {
            continue;
        };
        // Named by its library, which is how the Libraries screen groups its
        // folders, and not by its id, which no screen shows.
        let library = crate::libraries::name_of(conn, existing.library_id)
            .map_err(AddRootError::Failed)?
            .map_or_else(String::new, |name| format!(", a folder of the library `{name}`"));
        return Err(AddRootError::Unusable(format!(
            "{} {relation} {}{library}",
            canonical.display(),
            existing.path.display()
        )));
    }
```

(Re-read `scan::Root::library_id`'s type before writing against it.) Correct
`add_root_checked_in`'s `# Errors` and the second validation's doc, which
quote the old "is inside existing root" sentence.

`only_grant_of`: both advices end `or revoke this one.`.

`config_api`: the four sentences above; the seven `"root"` nouns in
`root_patch_from_json` become `"folder"`; the doc comments on `patch_bool`,
`patch_nullable_i64` and `patch_nullable_text` that name the `"root"` noun
name `"folder"`.

- [ ] **Step 4: Run them to watch them pass; whole crates, clippy, fmt**

- [ ] **Step 5: Watch one defect per sentence**

Put each old sentence back in turn and see its test fail: the inside overlap,
the around overlap, the full stop, the 409, the derive failure, the 404, the
patch noun.

- [ ] **Step 6: Commit**

```bash
git add crates
git commit -m "O200: the refusals the Libraries screen draws say folder"
```

---

### Task 4: No removal while a scan of it is reserved; the folder's scan reads its folder again (O196)

**Files:**
- Modify: `crates/taiyaku-serve/src/config_api.rs` (`delete_root`, `start_scan`, `run_scan_inner`; new `a_scan_reserved_on`, `removal_waits_for`, `finish_reservation`, `scan_and_finish`, `FOLDER_CHANGED`)
- Modify: `crates/taiyaku-serve/src/libraries_api.rs` (`delete_library`, `spawn_library_scan`)
- Modify: `crates/taiyaku-serve/src/test_support.rs` (receives `HeldJob`/`holding_the_job` from `libraries_api/tests.rs`)
- Test: `crates/taiyaku-serve/src/config_api/tests.rs`, `crates/taiyaku-serve/src/libraries_api/tests.rs`

**Interfaces:**
- `pub(crate) fn a_scan_reserved_on<'a>(state: &crate::ServeState, roots: &'a [scan::Root]) -> Option<&'a scan::Root>`
- `fn removal_waits_for(root: &scan::Root, what: &str) -> String` — sentence:
  `a scan of <path> is running; remove <what> once it has finished`, `<what>`
  being `the folder` or `the library`. `pub(crate)` so `libraries_api` uses it.
- `pub(crate) fn finish_reservation(scans: &mut std::collections::HashMap<i64, crate::ScanProgress>, id: i64, started: std::time::Instant, report: serde_json::Value)`
- `pub(crate) fn scan_and_finish(state: &crate::ServeState, root: &scan::Root, started: std::time::Instant)`
- `pub(crate) const FOLDER_CHANGED: &str = "the folder was removed or changed while the scan waited to start, so nothing was scanned";`

**Why both halves.** The refusal is asked when the `DELETE` arrives, and a
scan can be reserved after it is asked and before the removal commits: the
check and the removal are not one step, and holding the registry's lock
across a removal's derive would stall every status read. The job's re-read
under the job lock is what makes that window harmless: a scan reserved in it
waits on the lock, reads its folder again, and walks nothing where the folder
is gone or another holds its id. The guarded write-back keeps a finished job's
report off a reservation that is not its own.

- [ ] **Step 1: Move the job holder**

Move `HeldJob`, its `Drop` and `holding_the_job` from
`libraries_api/tests.rs` to `test_support.rs` as `pub(crate)`, unchanged, and
point `libraries_api/tests.rs` at them. Run `cargo test -p taiyaku-serve --
--test-threads=8` and see it pass before going on.

- [ ] **Step 2: Write the failing tests**

`config_api/tests.rs`:

```rust
/// While a scan of a folder is reserved, neither a rehearsal nor a removal
/// of it goes ahead: 409 in a sentence naming the folder, nothing removed,
/// and the reservation left as it was. Once the scan is done, it is removed.
#[tokio::test]
async fn a_folder_is_not_removed_while_a_scan_of_it_is_reserved() {
    let (router, state, _fixture) = test_support::router_over_a_single_scanned_work();
    let root_id = only_root_id(&state);
    let path = taiyaku_library::scan::roots(&state.library.conn()).expect("roots")[0]
        .path
        .display()
        .to_string();
    let started = std::time::Instant::now();
    state.scans.lock().unwrap().insert(
        root_id,
        crate::ScanProgress { started, done: false, report: None },
    );

    for query in ["?dry_run=true", ""] {
        let response = send_json(
            &router,
            Method::DELETE,
            &format!("/api/library/roots/{root_id}{query}"),
            &serde_json::json!({}),
        )
        .await;
        assert_eq!(response.status(), StatusCode::CONFLICT, "{query}");
        let body: serde_json::Value =
            serde_json::from_str(&test_support::body_string(response).await).expect("JSON");
        assert_eq!(
            body,
            serde_json::json!({
                "error": format!("a scan of {path} is running; remove the folder once it has finished")
            })
        );
        assert_eq!(only_root_id(&state), root_id, "{query}: nothing was removed");
        let scans = state.scans.lock().unwrap();
        let held = &scans[&root_id];
        assert!(!held.done && held.started == started, "{query}: the reservation stands");
    }

    state.scans.lock().unwrap().get_mut(&root_id).expect("held").done = true;
    let response = send_json(
        &router,
        Method::DELETE,
        &format!("/api/library/roots/{root_id}"),
        &serde_json::json!({}),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
}

/// A folder's scan that waited while its folder was removed, and another
/// folder took the freed id, walks nothing: nothing of the removed folder's
/// is filed under the new one, the new one is not marked scanned, and the
/// report says why.
#[tokio::test]
async fn a_folders_scan_walks_nothing_when_its_folder_changed_while_it_waited() {
    let (_router, state, fixture) = test_support::router_over_a_single_scanned_work();
    let asked = taiyaku_library::scan::roots(&state.library.conn())
        .expect("roots")
        .remove(0);
    let started = std::time::Instant::now();
    state.scans.lock().unwrap().insert(
        asked.id,
        crate::ScanProgress { started, done: false, report: None },
    );
    {
        // Through the store, as a removal that landed between the `DELETE`'s
        // check and its commit would.
        let working = state.library.working().expect("a working connection");
        taiyaku_library::scan::remove_root(&working, asked.id).expect("removes the folder");
        let other = fixture.path().join("Other");
        std::fs::create_dir_all(&other).expect("mkdir");
        let added = taiyaku_library::scan::add_root_checked(&working, &other).expect("adds another");
        assert_eq!(added.root.id, asked.id, "the freed id went to the new folder");
    }

    let job = std::sync::Arc::clone(&state);
    let root = asked.clone();
    tokio::task::spawn_blocking(move || crate::config_api::scan_and_finish(&job, &root, started))
        .await
        .expect("the job ends");

    let conn = state.library.conn();
    let entries: i64 = conn
        .query_row("SELECT COUNT(*) FROM scan_entry WHERE root_id = ?1", [asked.id], |r| r.get(0))
        .expect("counts");
    assert_eq!(entries, 0, "nothing of the removed folder's was filed under the new one");
    let status: Option<String> = conn
        .query_row("SELECT last_scan_status FROM root WHERE id = ?1", [asked.id], |r| r.get(0))
        .expect("reads");
    assert_eq!(status, None, "the new folder was never scanned");
    drop(conn);
    assert_eq!(
        state.scans.lock().unwrap()[&asked.id].report,
        Some(serde_json::json!({ "error": crate::config_api::FOLDER_CHANGED }))
    );
}

/// A finished folder scan's report lands on its own reservation and on no
/// other: not on one another scan made on the same id, and not on an id a
/// removal cleared.
#[tokio::test]
async fn a_folders_scan_lands_only_on_its_own_reservation() {
    let (_router, state, _fixture) = test_support::router_over_a_single_scanned_work();
    let root = taiyaku_library::scan::roots(&state.library.conn())
        .expect("roots")
        .remove(0);
    let ours = std::time::Instant::now();
    let theirs = ours + std::time::Duration::from_millis(1);
    state.scans.lock().unwrap().insert(
        root.id,
        crate::ScanProgress { started: theirs, done: false, report: None },
    );

    let job = std::sync::Arc::clone(&state);
    let asked = root.clone();
    tokio::task::spawn_blocking(move || crate::config_api::scan_and_finish(&job, &asked, ours))
        .await
        .expect("the job ends");
    {
        let scans = state.scans.lock().unwrap();
        let held = &scans[&root.id];
        assert!(!held.done && held.started == theirs && held.report.is_none());
    }

    state.scans.lock().unwrap().remove(&root.id);
    let job = std::sync::Arc::clone(&state);
    let asked = root.clone();
    tokio::task::spawn_blocking(move || crate::config_api::scan_and_finish(&job, &asked, ours))
        .await
        .expect("the job ends");
    assert!(state.scans.lock().unwrap().get(&root.id).is_none(), "a cleared slot stays clear");
}
```

`libraries_api/tests.rs`, beside `a_scan_of_one_folder_refuses_the_librarys_and_reserves_nothing`:

```rust
/// While a scan of one of its folders is reserved, a library is neither
/// rehearsed nor removed: 409 naming that folder, and the store as it was.
#[tokio::test]
async fn a_library_is_not_removed_while_a_scan_of_one_of_its_folders_is_reserved() {
    let (router, state, fixture) = test_support::router_over_a_single_scanned_work();
    let (id, folders) = a_library_with_an_unscanned_second_folder(&router, &fixture).await;
    a_scan_running_on(&state, folders[1]);
    let listed = get_json(&router, "/api/library/libraries").await;
    let path = listed["libraries"][0]["folders"][1]["path"]
        .as_str()
        .expect("a path")
        .to_string();
    let stored = the_store(&state);

    for query in ["?dry_run=true", ""] {
        let (status, refused) = answered(
            &router,
            Method::DELETE,
            &format!("/api/library/libraries/{id}{query}"),
            &serde_json::json!({}),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT, "{query}: {refused}");
        assert_eq!(
            the_sentence(&refused),
            format!("a scan of {path} is running; remove the library once it has finished")
        );
        assert_eq!(the_store(&state), stored, "{query} removed something");
    }
}
```

- [ ] **Step 3: Run them to watch them fail**

Run: `cargo test -p taiyaku-serve -- --test-threads=8 removed_while a_folders_scan`
Expected: compile errors for `scan_and_finish` and `FOLDER_CHANGED`; once
those exist as stubs, each test fails on its behaviour (200 where 409 is
wanted; scan entries under the new folder; the reservation overwritten).

- [ ] **Step 4: Write the code**

In `config_api.rs`:

```rust
/// What a folder's report says of a scan that found its folder gone, or
/// another folder holding its id, once the scan could start.
pub(crate) const FOLDER_CHANGED: &str =
    "the folder was removed or changed while the scan waited to start, so nothing was scanned";

/// The first of `roots` with a scan reserved and not yet done, if any.
pub(crate) fn a_scan_reserved_on<'a>(
    state: &crate::ServeState,
    roots: &'a [scan::Root],
) -> Option<&'a scan::Root> {
    let scans = state
        .scans
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    roots
        .iter()
        .find(|root| scans.get(&root.id).is_some_and(|progress| !progress.done))
}

/// The refusal of a removal that a scan of `root` holds up. `what` is what
/// was to be removed.
pub(crate) fn removal_waits_for(root: &scan::Root, what: &str) -> String {
    format!(
        "a scan of {} is running; remove {what} once it has finished",
        root.path.display()
    )
}

/// Mark the scan reserved on `id` at `started` done, with `report`.
///
/// **Only that reservation is replaced**, which its start instant names: a
/// removal clears the entry, and a scan of a folder that took the freed id
/// holds an entry of its own. A report with no reservation of its own to land
/// on is dropped, since nothing is waiting for it.
pub(crate) fn finish_reservation(
    scans: &mut std::collections::HashMap<i64, crate::ScanProgress>,
    id: i64,
    started: Instant,
    report: serde_json::Value,
) {
    if let Some(entry) = scans.get_mut(&id)
        && entry.started == started
    {
        *entry = crate::ScanProgress {
            started,
            done: true,
            report: Some(report),
        };
    }
}

/// [`start_scan`]'s background job: scan `root`, as the request found it, and
/// land the report on the reservation made at `started`.
pub(crate) fn scan_and_finish(state: &crate::ServeState, root: &scan::Root, started: Instant) {
    let report = run_scan(&state.library, root, &state.library_job);
    let mut scans = state
        .scans
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    finish_reservation(&mut scans, root.id, started, report);
}
```

`run_scan_inner`, once it holds the lock and has its working connection:

```rust
    // **The folder is read again once the lock is held.** `root` is what the
    // request found; a folder removed while the scan waited is not the one
    // that was asked for, and its id may belong to another by now. Where it is
    // gone or its path differs nothing is walked; where it is the same, what
    // is walked is the row as it stands, with the settings it holds now.
    let root = match scan::roots(&working)?
        .into_iter()
        .find(|now| now.id == root.id)
    {
        Some(now) if now.path == root.path => now,
        _ => return Err(FOLDER_CHANGED.to_string()),
    };
```

and the rest of the function reads `&root`. Its doc gains the re-read, in the
words of `libraries_api::run_library_scan_inner`'s. `start_scan`'s spawn
becomes `tokio::task::spawn_blocking(move || scan_and_finish(&spawn_state, &root, started));`.

`delete_root`'s blocking closure finds the root rather than asking
`root_exists`, and asks the registry before the only-grant refusal:

```rust
        let working = state.library.working().map_err(RootAnswer::Failed)?;
        let Some(root) = scan::roots(&working)
            .map_err(RootAnswer::Failed)?
            .into_iter()
            .find(|root| root.id == id)
        else {
            return Err(RootAnswer::Missing(id));
        };
        if a_scan_reserved_on(&state, std::slice::from_ref(&root)).is_some() {
            return Err(RootAnswer::Refused(removal_waits_for(&root, "the folder")));
        }
```

Its doc gains a paragraph saying a removal is refused while a scan of the
folder is reserved, for the dry run and the removal alike, and why the job's
re-read is what covers a scan reserved after the check. `RootAnswer::Refused`'s
doc names both refusals. `root_exists` stays: the rule routes use it.

In `libraries_api.rs`, `delete_library` keeps the folders as roots and asks
the registry before `library_removal_refusal`:

```rust
        let folders = folders_of(&working, id)?;
        if let Some(root) = crate::config_api::a_scan_reserved_on(&state, &folders) {
            return Err(LibraryAnswer::Refused(crate::config_api::removal_waits_for(
                root,
                "the library",
            )));
        }
```

with the ids taken from `folders` where the registry slots are cleared, and
`LibraryAnswer::Refused`'s doc naming the new refusal. `spawn_library_scan`'s
write-back calls `crate::config_api::finish_reservation` for each root in
place of its own `if let … entry.started == started` block.

- [ ] **Step 5: Run them to watch them pass**

- [ ] **Step 6: Watch the defects make them fail**

1. `delete_root`'s registry check deleted → `a_folder_is_not_removed_while_a_scan_of_it_is_reserved`.
2. `delete_library`'s registry check deleted → `a_library_is_not_removed_while_a_scan_of_one_of_its_folders_is_reserved`.
3. `run_scan_inner`'s re-read deleted (walk the `root` it was given) → `a_folders_scan_walks_nothing_when_its_folder_changed_while_it_waited`.
4. `Some(now) if now.path == root.path` → `Some(now)` → the same test (the new folder's path differs; its id is the same).
5. `finish_reservation`'s guard → an unconditional `scans.insert` → `a_folders_scan_lands_only_on_its_own_reservation`.
6. `scan_and_finish` inserting directly instead of calling `finish_reservation` → the same test.

- [ ] **Step 7: Whole crates, clippy, fmt; commit**

```bash
git add crates/taiyaku-serve
git commit -m "O196: no removal while a scan of it is reserved, and a folder's scan reads its folder again"
```

---

### Task 5: Up from a configured folder inside another (O207)

**Files:**
- Modify: `crates/taiyaku-serve/src/folders_api.rs` (`list`, `Listing::parent`'s doc)
- Test: `crates/taiyaku-serve/src/folders_api/tests.rs`

- [ ] **Step 1: Change the test**

`the_parent_stops_at_a_prefix` becomes
`the_parent_stops_at_a_prefix_no_other_holds`: from the inner prefix,
`parent` is `Some(media)`; from the outer prefix, `None`; the two
`Some(manga)` cases stay.

```rust
/// Nothing leads above a prefix that no other prefix holds, and a folder one
/// level down leads to the one above it. Where one prefix lies inside
/// another, Up from the inner one leads to the outer one's folder above it.
#[test]
fn the_parent_stops_at_a_prefix_no_other_holds() {
    let fixture = test_support::Fixture::new("folders-parent");
    let (media, _elsewhere) = a_share(&fixture);
    let manga = media.join("Manga");
    let prefixes = [media.clone(), manga.clone()];

    assert_eq!(list(&prefixes, Some(&media)).expect("lists").parent, None);
    assert_eq!(
        list(&prefixes, Some(&manga)).expect("lists").parent,
        Some(media.clone()),
        "the inner prefix sits inside the outer one"
    );
    assert_eq!(
        list(std::slice::from_ref(&manga), Some(&manga)).expect("lists").parent,
        None,
        "alone, the inner prefix leads nowhere"
    );
    let volumes = manga.join("Volumes");
    assert_eq!(
        list(&prefixes, Some(&volumes)).expect("lists").parent,
        Some(manga.clone())
    );
}
```

- [ ] **Step 2: Run it to watch it fail**

Run: `cargo test -p taiyaku-serve -- --test-threads=8 the_parent_stops`
Expected: FAIL at "the inner prefix sits inside the outer one" (`None`).

- [ ] **Step 3: Change `list`**

```rust
    // Up goes to the folder above while that folder is inside a prefix: from
    // a prefix that sits inside another, to the outer one's folder, and from
    // any other prefix nowhere.
    let parent = canonical
        .parent()
        .filter(|parent| prefixes.within(parent))
        .map(Path::to_path_buf);
```

`Listing::parent`'s doc: "`None` at a prefix no other prefix holds, and where
the prefixes were listed."

- [ ] **Step 4: Run it to watch it pass; put the old `if prefixes.canonical.contains(&canonical) { None }` back and see it fail; restore**

- [ ] **Step 5: Whole crates, clippy, fmt; commit**

```bash
git add crates/taiyaku-serve/src/folders_api.rs crates/taiyaku-serve/src/folders_api/tests.rs
git commit -m "O207: Up from a folder prefix inside another goes to the outer one"
```

---

### Task 6: "Every library" asks before it lifts a limit (O195)

**Files:**
- Modify: `web/src/UserRow.tsx`
- Modify: `web/src/styles/admin.css` (`.grants .lift-limit`)
- Test: `web/src/UserRow.test.tsx`, `web/src/UserRow.browser.test.tsx` (new)

**Interfaces:**
- Markup: while asking, `<div class="lift-limit" role="group" aria-label="Lift the limit">` right after the two radios' labels, holding a `<p>` and two buttons, `name="lift-limit"` ("See every library") and `name="keep-limit"` ("Keep the limit").
- Sentence: `<name> would see every library, including any added later, and the ticks below would be cleared.`

- [ ] **Step 1: Write the failing jsdom tests**

In `UserRow.test.tsx`, `lifts the limit only by Every library, which sends
the empty list` becomes:

```tsx
  it('asks before Every library lifts a limit, and lifts it on the answer', async () => {
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
    expect(usersApi.patchUser).not.toHaveBeenCalled()
    expect(radio(host, 'only').checked).toBe(true)
    const asking = control(host, '.grants .lift-limit')
    expect(asking.getAttribute('role')).toBe('group')
    expect(asking.querySelector('p')?.textContent).toBe(
      'sam would see every library, including any added later, and the ticks below would be cleared.',
    )

    await click(control(host, 'button[name="lift-limit"]'))
    expect(usersApi.patchUser).toHaveBeenCalledWith(2, { libraries: [] })
    expect(radio(host, 'every').checked).toBe(true)
    expect(host.querySelector('.grant-list')).toBeNull()
    expect(host.querySelector('.lift-limit')).toBeNull()
  })

  it('keeps the limit when the answer is no, and puts focus back on Only these', async () => {
    const me = user()
    const other = user({
      id: 2,
      name: 'sam',
      role: 'user',
      libraries: [{ id: 70, name: 'Manga' }],
    })
    const { host } = await show(other, [me, other], { actingAs: 1 })

    await click(radio(host, 'every'))
    await click(control(host, 'button[name="keep-limit"]'))

    expect(usersApi.patchUser).not.toHaveBeenCalled()
    expect(host.querySelector('.lift-limit')).toBeNull()
    expect(radio(host, 'only').checked).toBe(true)
    expect(tick(host, 70).checked).toBe(true)
    expect(document.activeElement).toBe(radio(host, 'only'))
  })
```

`sends nothing for Only these until a library is ticked` also asserts
`host.querySelector('.lift-limit')` is null after its Every library click
(nothing to lift, nothing asked). `puts an account back to Every library
after Only these and a tick` clicks `button[name="lift-limit"]` after its
Every library click. `keeps drawing Only these when Every library is pressed
while a tick is being saved` also asserts no `.lift-limit` (a write in flight
asks nothing).

- [ ] **Step 2: Write the failing browser test**

`web/src/UserRow.browser.test.tsx` (new). Import `./index.css`; mock
`./users` as `UserRow.test.tsx` does; mount one `UserRow` per case for sam,
role `user`, granted Manga, with the libraries Manga and Comics; real keys
through `userEvent` inside `act`, as `Libraries.shell.browser.test.tsx`'s
`click` and `keys` helpers are written.

```tsx
describe('the Users card under real keys', () => {
  for (const key of ['ArrowUp', 'ArrowLeft', 'ArrowDown', 'ArrowRight']) {
    it(`asks before ${key} on the radios lifts a limit`, async () => {
      const host = await shown()
      const only = radio(host, 'only')
      await click(only) // focuses it; it is already chosen, so nothing changes
      await keys(`{${key}}`)

      expect(usersApi.patchUser).not.toHaveBeenCalled()
      expect(host.querySelector('.lift-limit')).not.toBeNull()
      expect(only.checked).toBe(true)
    })
  }

  it('lifts the limit from the keyboard: an arrow, Tab, Enter', async () => {
    const host = await shown()
    await click(radio(host, 'only'))
    await keys('{ArrowUp}')
    await keys('{Tab}')
    expect(document.activeElement?.getAttribute('name')).toBe('lift-limit')
    await keys('{Enter}')
    expect(usersApi.patchUser).toHaveBeenCalledWith(2, { libraries: [] })
  })
})
```

- [ ] **Step 3: Run them to watch them fail**

Run: `pnpm -C web exec vitest run --project jsdom --reporter=default src/UserRow.test.tsx`
and `pnpm -C web exec vitest run --project browser --reporter=default src/UserRow.browser.test.tsx`.
Expected: the jsdom cases fail on the patch sent at once; every browser case
fails with `patchUser` called with `{ libraries: [] }` (O195's own finding).

- [ ] **Step 4: Write the question**

In `UserRow.tsx`, beside `choosing`:

```tsx
  /**
   * "Every library" was chosen on an account that holds grants, and the card
   * is asking before it lifts the limit.
   *
   * **Asked, not saved at once (O195).** An arrow key on the radios chooses
   * as a click does, so one key pressed on "Only these" sent the empty list,
   * and the account saw every library with its ticks gone and nothing asked.
   */
  const [lifting, setLifting] = useState(false)
  const everyRadio = useRef<HTMLInputElement>(null)
  const onlyRadio = useRef<HTMLInputElement>(null)
```

`seeEvery` asks instead of saving:

```tsx
  const seeEvery = () => {
    if (inFlight.current) return
    if (granted.length === 0) {
      setChoosing(false)
      return
    }
    setLifting(true)
  }

  const lift = () => {
    save({ libraries: [] }, () => {
      setChoosing(false)
      setLifting(false)
      everyRadio.current?.focus()
    })
  }

  /** The question goes, and focus goes back to the choice that stands. */
  const keepLimit = () => {
    setLifting(false)
    onlyRadio.current?.focus()
  }
```

`ref={everyRadio}` and `ref={onlyRadio}` on the two radios; after the "Only
these" label:

```tsx
            {lifting && granted.length > 0 && (
              <div
                className="lift-limit"
                role="group"
                aria-label="Lift the limit"
              >
                <p>
                  {shown.name} would see every library, including any added
                  later, and the ticks below would be cleared.
                </p>
                <button
                  type="button"
                  name="lift-limit"
                  disabled={saving}
                  onClick={lift}
                >
                  See every library
                </button>
                <button
                  type="button"
                  name="keep-limit"
                  disabled={saving}
                  onClick={keepLimit}
                >
                  Keep the limit
                </button>
              </div>
            )}
```

(`granted.length > 0` closes the question when a reload brings the account
with no grant, lifted elsewhere.) The file's top doc, under "Grants are
libraries, sent whole", gains: choosing "Every library" on an account that
holds grants asks first, since an arrow key chooses as a click does.

`admin.css`, beside the other `.grants` rules:

```css
/* O195: the question "Every library" asks before it lifts a limit. Its
   sentence on a line of its own, and the two answers beside each other. */
.grants .lift-limit {
  display: flex;
  flex-wrap: wrap;
  gap: var(--space-2);
  margin: var(--space-2) 0;
}

.grants .lift-limit p {
  flex-basis: 100%;
  margin: 0;
}
```

- [ ] **Step 5: Run them to watch them pass; run the new browser cases alone with `-t` too**

- [ ] **Step 6: Watch the defects make them fail**

1. `seeEvery` back to saving `{ libraries: [] }` at once → the jsdom question case and every browser case.
2. `keepLimit` without the focus line → the keep case.
3. `lift` without `setLifting(false)` → the first jsdom case (the group stays).

- [ ] **Step 7: Whole web projects; commit**

```bash
git add web/src/UserRow.tsx web/src/UserRow.test.tsx web/src/UserRow.browser.test.tsx web/src/styles/admin.css
git commit -m "O195: Every library asks before it lifts a limit"
```

---

### Task 7: The words: Scan library, Scan folder, and the whole store's derive (O199, O198)

**Files:**
- Modify: `web/src/LibraryCard.tsx`, `web/src/RootRow.tsx`, `web/src/ScanReport.tsx`, `web/src/config.ts` (a comment), `README.md`
- Test: `web/src/LibraryCard.test.tsx`, `RootRow.test.tsx`, `Libraries.test.tsx`, `ScanReport.test.tsx`, `RemoveRoot.test.tsx`, `RemoveLibrary.test.tsx`, `Config.browser.test.tsx`, `RuleEditor.browser.test.tsx`, `fieldNames.browser.test.tsx`

**Interfaces — the words:**
- `LibraryCard`'s `button[name="scan-library"]`: `Scan library`.
- `RootRow`'s scan button: `Scan folder`.
- `LibraryCard`'s `.scanning` line: `A scan is running in this library…` and the
  elapsed seconds after it as now. It is drawn for a scan of one folder as
  well as of the library, which "Scanning this library's folders…" misstated.
- `DeriveSection`'s heading: `What derivation made of every library`. Every
  caller's derive is the whole store's: a scan's, a removal's, a rule save's.

- [ ] **Step 1: Write the failing tests**

`LibraryCard.test.tsx`, a case that mounts a card of two folders and asserts
`button[name="scan-library"]`'s text is `Scan library`, each row's scan
button's text is `Scan folder`, and, with the status read answering
`running: true`, the `.scanning` line begins `A scan is running in this library`.
`ScanReport.test.tsx`, a case that the derive section's heading is
`What derivation made of every library`.

Then move every existing test to the new words: each `'Scan now'` that names
a row's button becomes `'Scan folder'` and each that names the card's becomes
`'Scan library'` (`Libraries.test.tsx`'s held-buttons list reads
`['Scan library', 'Remove library…', 'Scan folder', 'Remove…', 'Add folder']`);
`'Scanning this library'` becomes `'A scan is running in this library'`; each
`'What derivation made of it'` becomes `'What derivation made of every library'`.

The same commit moves the web tests' copies of the sentences Task 3 reworded
(`command grep -a -rn "existing root\|a scan of root\|there is no root\|) scanned, but" web/src`):
`a scan of root 3 (/srv/manga) is already running` →
`a scan of /srv/manga is already running`; `there is no root 409` →
`there is no folder 409`; ``/media/Manga/Volumes/x is inside existing root
/media/Manga/Volumes (id 1)`` → ``/media/Manga/Volumes/x is inside
/media/Manga/Volumes, a folder of the library `Manga` ``;
`` `root 3 (${UNBROKEN_ROOT.path}) scanned, but deriving …` `` →
`` `${UNBROKEN_ROOT.path} was scanned, but deriving …` ``. `config.ts`'s doc
comment quoting `there is no root 409` quotes `there is no folder 409`.
`config.test.ts`'s made-up `sits inside the existing root` is no server's
sentence and stays.

- [ ] **Step 2: Run them to watch the new cases fail**

Run: `pnpm -C web exec vitest run --project jsdom --reporter=default src/LibraryCard.test.tsx src/ScanReport.test.tsx`
Expected: the two new cases fail on the old words.

- [ ] **Step 3: Change the words**

The three strings in `LibraryCard.tsx`, `RootRow.tsx` and `ScanReport.tsx`.
Every comment in `LibraryCard.tsx` and `RootRow.tsx` that says "Scan now"
names the button it means, "Scan library" or "Scan folder".
`README.md`'s two "Scan now" (the bold one and "Scan now on a library's
card") become "Scan library".

- [ ] **Step 4: Run the whole web projects, browser included**

- [ ] **Step 5: Watch one defect per word** (each string put back in turn)

- [ ] **Step 6: Commit**

```bash
git add web/src README.md
git commit -m "O199, O198: Scan library, Scan folder, and the derive is every library's"
```

---

### Task 8: A held button looks held (O201)

**Files:**
- Modify: `web/src/styles/base.css`
- Test: `web/src/buttons.browser.test.tsx` (new)

**The look.** `admin.css` already says how this codebase draws a control
nobody may press and a person may still be reading: `.proposal-detail
.accept:disabled` keeps the word at full contrast and says "not now" with the
page's own ground and a dashed border (its comment records why `--muted` text
failed there: 1.35:1 in dark). The same, app-wide, in `base.css`, at zero
specificity like every rule there. It sets `background` and `border-style`,
which the class rules on buttons (`.confirm`'s border colour and colour) do
not, so a held `.confirm` is drawn held too.

- [ ] **Step 1: Write the failing browser test**

```tsx
import { describe, expect, it } from 'vitest'
// **Required**: the rules under test reach the app through `index.css`.
import './index.css'
import { mount } from './testing/mount'

/**
 * O201: a held button is drawn unlike a live one, by `base.css`'s own rule,
 * and so is one a class rule colours: `.confirm` sets a border colour and a
 * text colour, and neither is what says "held". The word keeps its colour.
 */
describe('a held button', () => {
  it('is drawn on the page’s ground with a dashed border, its word unchanged', () => {
    const host = mount(
      <div className="library">
        <button type="button" name="live">
          Live
        </button>
        <button type="button" name="held" disabled>
          Held
        </button>
        <div className="remove-library">
          <button type="button" className="confirm" name="live-confirm">
            Remove
          </button>
          <button type="button" className="confirm" name="held-confirm" disabled>
            Remove
          </button>
        </div>
      </div>,
    )
    const style = (name: string) =>
      getComputedStyle(host.querySelector(`button[name="${name}"]`) as Element)
    const page = getComputedStyle(document.body).backgroundColor

    for (const [live, held] of [
      ['live', 'held'],
      ['live-confirm', 'held-confirm'],
    ]) {
      expect(style(live).borderTopStyle).toBe('solid')
      expect(style(held).borderTopStyle).toBe('dashed')
      expect(style(held).backgroundColor).toBe(page)
      expect(style(held).backgroundColor).not.toBe(style(live).backgroundColor)
      expect(style(held).color).toBe(style(live).color)
    }
  })
})
```

(`mount` may need `act`; write it as the other browser tests in this folder
mount.)

- [ ] **Step 2: Run it to watch it fail**

Run: `pnpm -C web exec vitest run --project browser --reporter=default src/buttons.browser.test.tsx`
Expected: FAIL — the held button's border is `solid` and its background the
live one's (O201's own measurement).

- [ ] **Step 3: Write the rule**

`base.css`'s `button:disabled` rule becomes:

```css
/* O201: a button nobody may press now is drawn so, on every screen these rules
   reach: the page's own ground and a dashed border, the way the proposals
   panel's Accept is drawn held, and its word at full contrast, since the
   sentence beside it that says why is read with it. A class rule that colours
   a button sets neither property, so it is drawn held too. */
:where(.library, .login) :where(button:disabled) {
  background: var(--bg);
  border-style: dashed;
  cursor: default;
}
```

- [ ] **Step 4: Run it to watch it pass; run the whole browser project**

`index.css.test.ts` measures the disabled Accept's contrast; it reads
`.proposal-detail .accept:disabled`'s own rule, which this does not change.
`base.css.test.ts`'s scope check holds the new selector to the scope.

- [ ] **Step 5: Watch the defects make it fail**

1. The `border-style` line deleted → the dashed assertions.
2. The `background` line deleted → the ground assertions.

- [ ] **Step 6: Whole web projects; commit**

```bash
git add web/src/styles/base.css web/src/buttons.browser.test.tsx
git commit -m "O201: a held button is drawn held"
```

---

### Task 9: A refusal under its button is brought into view; Make the library says why it is held (O209)

**Files:**
- Create: `web/src/reveal.ts`, `web/src/Refusal.tsx`
- Modify: `web/src/Libraries.tsx`, `web/src/LibraryCard.tsx`, `web/src/FolderPicker.tsx`
- Test: `web/src/Libraries.shell.browser.test.tsx`, `web/src/Libraries.test.tsx`, `web/src/FolderPicker.test.tsx`

**Interfaces:**
- `reveal.ts` exports `inView(el: HTMLElement): boolean` and
  `reveal(el: HTMLElement): void`, moved unchanged from `Libraries.tsx`, and
  new `bringIntoView(el: HTMLElement): void`.
- `Refusal.tsx` exports `Refusal({ children }: { children: string })`,
  drawing `<p role="alert">{children}</p>`: the markup each site draws now, so
  `admin.css`'s `.add-library > [role="alert"]` and
  `.rename-library > [role="alert"]` still reach it.
- `FolderPickerProps` gains `onPath?: (path: string) => void`: the box's text
  each time it changes, by typing, by browsing, or by emptying after a pick.
- Sentence, beneath "Make the library" while it is held by a path in the box
  and no folder on the list: `The folder in the box is not on the list yet: “Add to the list” puts it there.`
  The button's `aria-describedby` names it while it is drawn.

**Which refusals.** The three drawn *beneath* the control whose press they
answer, which is where the fold can hide one: the make form's
(`Libraries.createFailure`, under "Make the library"), a card's Add folder
(`LibraryCard.addFailure`, under the picker), and a card's rename
(`LibraryCard.renameFailure`, under Save). A card's scan refusal and a row's
are drawn above their buttons. A failure no press asked for (a poll's, the
list's re-read at the end of a scan) is not moved to, for the reason
`Libraries.tsx` gives: the page would move under whoever is typing.

**Why `block: 'nearest'`.** `reveal` puts a report's head at the top of the
view, which is right for a report taller than the window. A one-line refusal
moved to the top would push the button it answers, and the form above it,
out of view; `nearest` scrolls just far enough for the sentence.

- [ ] **Step 1: Write the failing browser tests**

In `Libraries.shell.browser.test.tsx`, inside the `for (const [width,
height] of SIZES)` block:

```tsx
    /**
     * O209: the pressed control at the very foot of the window, and its
     * refusal drawn beneath it, below the fold, is brought into view.
     */
    async function atTheFoot(el: HTMLElement) {
      el.scrollIntoView({ block: 'end' })
      await new Promise((r) => setTimeout(r, 60))
      expect(
        Math.abs(el.getBoundingClientRect().bottom - window.innerHeight),
      ).toBeLessThan(2)
    }

    it(`brings the make form’s refusal into view from the foot, at ${width}`, async () => {
      vi.mocked(librariesApi.createLibrary).mockRejectedValue(
        new Error('/api/library/libraries: 400 {"error":"refused"}'),
      )
      const host = await shell(width, height)
      const form = host.querySelector('form.add-library') as HTMLFormElement
      await fill(form.querySelector('input[name="library-name"]') as Element, 'Comics')
      await fill(form.querySelector('input[name="path"]') as Element, '/srv/comics')
      await click(form.querySelector('button[name="pick"]') as Element)
      const make = form.querySelector('button[type="submit"]') as HTMLElement
      await atTheFoot(make)
      await press(make)
      await settle()
      expectInSight(form.querySelector('[role="alert"]') as Element)
    })

    it(`brings a refused Add folder into view from the foot, at ${width}`, async () => {
      vi.mocked(librariesApi.addFolder).mockRejectedValue(
        new Error('/api/library/libraries/1/folders: 400 {"error":"refused"}'),
      )
      const host = await shell(width, height)
      const picker = card(host, 'Manga').querySelector(':scope > .add-root') as HTMLElement
      await fill(picker.querySelector('input[name="path"]') as Element, '/srv/extra')
      const pick = picker.querySelector('button[name="pick"]') as HTMLElement
      await atTheFoot(pick)
      await press(pick)
      await settle()
      expectInSight(picker.querySelector('[role="alert"]') as Element)
    })

    it(`brings a refused rename into view from the foot, at ${width}`, async () => {
      vi.mocked(librariesApi.renameLibrary).mockRejectedValue(
        new Error('/api/library/libraries/1: 400 {"error":"refused"}'),
      )
      const host = await shell(width, height)
      const manga = card(host, 'Manga')
      await click(manga.querySelector('button[name="rename-library"]') as Element)
      const form = manga.querySelector('form.rename-library') as HTMLElement
      await fill(form.querySelector('input') as Element, 'Manga 2')
      const save = form.querySelector('button[type="submit"]') as HTMLElement
      await atTheFoot(save)
      await press(save)
      await settle()
      expectInSight(form.querySelector('[role="alert"]') as Element)
    })
```

(`press` is the programmatic click from `testing/input`, which does not
scroll, so the control stays where `atTheFoot` put it. Save is put at the
foot after the rename form opens, as written. A control too near the top of
the page to be scrolled to the foot fails `atTheFoot`'s own check: report
which size, and do not move the control or loosen the check.)

- [ ] **Step 2: Write the failing jsdom tests**

`Libraries.test.tsx`:

```tsx
  it('says why Make the library is held while a path waits in the box', async () => {
    const host = await shown()
    const form = host.querySelector('form.add-library') as HTMLFormElement
    const make = form.querySelector('button[type="submit"]') as HTMLButtonElement
    await typeInto(form.querySelector('input[name="library-name"]') as HTMLInputElement, 'Comics')
    await typeInto(form.querySelector('input[name="path"]') as HTMLInputElement, '/srv/comics')

    expect(make.disabled).toBe(true)
    const why = document.getElementById(make.getAttribute('aria-describedby') ?? '')
    expect(why?.textContent).toBe(
      'The folder in the box is not on the list yet: “Add to the list” puts it there.',
    )

    await press(form.querySelector('button[name="pick"]') as Element)
    expect(make.disabled).toBe(false)
    expect(make.getAttribute('aria-describedby')).toBeNull()
    expect(form.textContent).not.toContain('is not on the list yet')
  })

  it('says nothing of the box while the box is empty', async () => {
    const host = await shown()
    const form = host.querySelector('form.add-library') as HTMLFormElement
    expect(form.textContent).not.toContain('is not on the list yet')
  })
```

`FolderPicker.test.tsx`: `onPath` is called with each text the box holds:
typed, browsed to (a listing entered), and `''` after a pick is taken.

- [ ] **Step 3: Run them to watch them fail**

Browser: each of the three refusals' bottom is below `window.innerHeight`.
jsdom: no sentence, no `aria-describedby`; `onPath` never called.

- [ ] **Step 4: Write `reveal.ts` and `Refusal.tsx`**

`web/src/reveal.ts`: `inView` and `reveal` moved from `Libraries.tsx` with
their doc comments, exported, and:

```ts
/**
 * Scrolls `el` just far enough to be seen, unless its head already is. For a
 * sentence beneath the control it answers: the control stays in view above
 * it, where `reveal`'s top-of-the-view would push it out.
 */
export function bringIntoView(el: HTMLElement) {
  if (!inView(el)) el.scrollIntoView?.({ block: 'nearest' })
}
```

`web/src/Refusal.tsx`:

```tsx
/**
 * A refusal drawn beneath the control whose press it answers, and brought
 * into view when it is drawn (O209): a control at the foot of the window
 * leaves the sentence under it below the fold, and the page does not move by
 * itself. Each refusal slot is cleared when its press is made again, so a
 * second refusal in the same words is drawn anew and brought into view anew.
 */
import { useEffect, useRef } from 'react'
import { bringIntoView } from './reveal'

export function Refusal({ children }: { children: string }) {
  const el = useRef<HTMLParagraphElement>(null)
  useEffect(() => {
    if (el.current !== null) bringIntoView(el.current)
  }, [children])
  return (
    <p role="alert" ref={el}>
      {children}
    </p>
  )
}
```

`Libraries.tsx` imports `inView`/`reveal` from `./reveal` and draws
`{createFailure !== undefined && <Refusal>{createFailure}</Refusal>}`.
`LibraryCard.tsx` draws its `addFailure` and `renameFailure` through
`Refusal`. Check first that each of the three slots is cleared when its press
is made again (`submit`, `add`, `rename` each clear theirs today); the
component's doc depends on it.

- [ ] **Step 5: Write `onPath` and the held sentence**

`FolderPicker.tsx`:

```tsx
  /**
   * The text in the box, each time it changes: typed, browsed to, or emptied
   * once a pick was taken. A caller that holds a button until something is
   * picked says why from this.
   */
  onPath?: (path: string) => void
```

```tsx
  // In a ref, so a caller's new closure on each render does not count as a
  // change of path.
  const told = useRef(onPath)
  told.current = onPath
  useEffect(() => {
    told.current?.(path)
  }, [path])
```

`Libraries.tsx`:

```tsx
  /** The text in the make form's path box, which is not a folder until added. */
  const [pending, setPending] = useState('')
  const heldWhy = useId()
  /**
   * "Make the library" is held while no folder is on the list, and a path in
   * the box looks like one that is (O209). Said only then: a held button
   * with an empty box has nothing in it to explain.
   */
  const heldByPath = folders.length === 0 && pending.trim() !== ''
```

`<FolderPicker … onPath={setPending} />`; the submit button gains
`aria-describedby={heldByPath ? heldWhy : undefined}`; after it, before the
refusal:

```tsx
        {heldByPath && (
          <p className="why" id={heldWhy}>
            The folder in the box is not on the list yet: “Add to the list”
            puts it there.
          </p>
        )}
```

- [ ] **Step 6: Run them to watch them pass; the browser cases alone with `-t` too**

- [ ] **Step 7: Watch the defects make them fail**

1. `Refusal`'s effect body deleted → the three browser cases.
2. `bringIntoView`'s `block: 'nearest'` → `'start'` → none of the three should fail on sight; the make form's button is then out of view: add `expectInSight(make)` after the alert's check, and see *that* fail with `'start'`. (If it does not fail at either size, report it: the choice of `nearest` is then unmeasured.)
3. `heldByPath` without `folders.length === 0 &&` → the jsdom case after the pick.
4. `onPath`'s effect deleted → the jsdom and `FolderPicker` cases.

- [ ] **Step 8: Whole web projects; commit**

```bash
git add web/src
git commit -m "O209: a refusal under its button is brought into view, and Make the library says why it is held"
```

---

### Task 10: The status rows, the notes and the counts

**Files:** `docs/status.md`, `docs/notes.md`, `README.md` (if a sentence
there says what changed)

Closing a row is the five-part edit, opening one its inverse; read
`docs/status.md`'s own Open items preamble and the sweep below before
editing.

- [ ] **Step 1: Rows.** For each, the row's text gains the ruling and what
  was done, and the prose that mentions it is checked (`command grep -a -n
  'O19[5-9]\|O20[0-9]' docs/ web/src crates`):
  - **Closed** `[x]`: O195, O196, O197 (accepted by the user's ruling,
    2026-10-07, as `folders_api`'s module doc already states; no code),
    O198, O205, O206, O207.
  - **Narrowed, still open**: O199, O200, O201, O209 — each with what this
    branch did and what remains, from "Where a ruling covers part of a row".
  - **Open, with the ruling recorded**: O208 — "to be merged in F2b".
  - O202, O203, O204: unchanged.
  - Every row that read "Awaiting the user's ruling" no longer does.
- [ ] **Step 2: Counts.** The "now closed — leaving N" sentence (generated
  from the table: regenerate it, do not edit it), the "What remains" line
  (its count word wraps — grep the opener and read the next line), the Rust
  test count at both its sites (from the gate's JUnit report), and the web
  counts read off `--project node`, `--project jsdom` and the browser run, not
  added by hand. Sweep:

  ```sh
  command grep -a -c '^| O[0-9]' docs/status.md
  command grep -a -c '^| O[0-9]* | \[x\]' docs/status.md
  command grep -a -nE '[0-9]{3,4} tests|Nine of the' docs/status.md
  command grep -a -n -A1 '^What remains:' docs/status.md
  ```

  Then check every changed row has exactly six `|` characters.
- [ ] **Step 3: Schema.** Every place the docs say schema 15 is the current
  version says 16 (`command grep -a -rn 'schema 15\|version 15' docs README.md`;
  historical narrative stays as written).
- [ ] **Step 4: Notes.** A `docs/notes.md` section "Libraries, F2a
  follow-ups" recording what was measured on this branch: the two databases
  checked for name clashes before migration 16 (above), each hand defect and
  the test it turned red (from the task reports), and the drive's readings
  (Finishing, below).
- [ ] **Step 5: Commit**

```bash
git add docs README.md
git commit -m "F2a follow-ups: status rows, counts and notes"
```

---

## Finishing (the controller, after Task 10)

1. **Whole-branch review**, Rust and web, of `git diff main...HEAD`, then a
   review of its fixes.
2. **Mutation sweep** over the branch: `scripts/mutants.sh --since main`
   after committing every fix. Read the unviable count against what changed:
   a clean run on sentences and SQL says little (no mutant is made for a
   string literal).
3. **SQLite 3.40.1**: the three crates' tests in `rust:1-bookworm`, as
   `taiyaku-sqlite-340-in-ci` records. Migration 16 uses two-argument `trim`,
   `char()` and a `COLLATE NOCASE` index; run it there before pushing.
4. **Migration 16 on real stores**: on a scratch copy of
   `/home/davey/temp/manga-test/library.db` (schema 13 → 16) and of the
   workstation's store (15 → 16), each library name before and after, and the
   original's digest unchanged. **The NAS store is not readable from here**:
   before the image that carries schema 16 reaches the NAS, ask the user to
   run, there, read-only:
   `sqlite3 -readonly <appdata>/library.db "SELECT a.id, a.name, b.id, b.name FROM library a JOIN library b ON a.id < b.id AND lower(trim(a.name)) = lower(trim(b.name))"`
   and say what migration 16 will rename if it answers any row.
5. **A drive in a real browser** over a scratch copy of the real library:
   O195 by click and by arrow key, the lift and the keep; O199's two buttons
   and the scanning line during a folder's scan; O198's heading; O201's held
   buttons while a library scans and in a removal dialog, both schemes; O209's
   three refusals with the control at the foot, at 390 and 1280; O207's Up
   from a nested prefix (`--folders` with one inside another); O196's 409 by
   starting a scan and removing from a second tab. Isolation proved after.
6. **The gate** at the final head, `env -u TAIYAKU_CORPUS scripts/check-all.sh`,
   quoting its `all green` line; the PR body; the claim check (PR body in pass
   one); then **ask before pushing**. The PR and the merge are each a further
   yes.

## Changed while building

(Recorded here as it happens: any departure from a step, and why.)

- **Task 1.** The trigger test also inserts `/b{n}/{c}` for each white-space
  character and expects the library `/b{n}/`: the brief's test never reached
  the trigger's whole-path list, and taking a code point out of it left the
  test green. `own_library_clash_refuses_exactly_what_the_trigger_cannot_name`
  changed with the rule (`/g/\tManga\t` is now refused; `/c/ MANGA` added). No
  separate RED run was made; the nine hand defects stand in. A review fix gave
  the migration test a second whitespace-only library (id 6, `' '`), since the
  first UPDATE's `trim(name, WS) <> ''` guard was otherwise never exercised.
- **Task 2.** The refusal reads ``the library `<name>` has no folder, so there
  is nothing in it to grant``, corrected above. The plan's first sentence, "so
  a grant of it would let the account see every library", is false for an
  account holding another grant, and `ungrantable`'s doc now says "as an
  account's only grant".
- **Task 3.** The review found more "root" on the same routes:
  `scan::check_exclude_globs`'s two sentences (the folder PATCH's 400), the
  empty-scan advice, and the `root_panicked` texts. All say folder now. The
  patch-noun test covers all six call sites, and a route test asserts the bad
  glob's whole 400. The `will not compile` sentence cannot be reached from a
  route (the `ignore` crate refuses a line at `add_line`).
- **Task 4.** Six `libraries_api` tests that sent a `DELETE` while a scan was
  reserved, to stand for a removal landing in the window, now remove through
  the store and clear the registry slots as the route does: the route refuses
  them now. `delete_root`'s doc says what the re-read does not reach, a walk
  already past it when the removal commits: its one write transaction either
  fails the foreign key or is taken by the removal's cascade, and nothing is
  filed under another folder. A test holds that the walk uses the settings the
  folder holds when it starts.
- **Task 6.** The keyboard case is arrow, Tab, Tab, Enter: after an arrow,
  focus sits on the unchecked "Every library" radio, and Chromium's first Tab
  goes to the checked one. The question no longer has a `granted.length > 0`
  render gate. `lifting` is retired in render when the account holds no
  grant, as `applied` is, which also closes the question after a lift. The
  test that named the gate went with it.
- **Task 8.** The comment says what the rule reaches. The proposals panel's
  Dismiss, Reopen and Compare set their own `background` and border, so they
  keep their own held look, a muted word. A `.login` pair is in the test.
- **Task 9.** `scroll-margin-bottom: 0.5rem` on the three refusals in
  `admin.css`, since `nearest` left a refusal's foot a fraction of a pixel
  past the window edge. `nearest` against `start` is told apart by the rename
  case alone, at both sizes; the make form and Add folder sit near the foot of
  the document, where both land the same. A test types a box of spaces alone.
- **How it ran.** The web lane ran in one worktree, and for a while two agents
  worked in it at once. Task 7's commit swept up the Task 6 fixer's unsaved
  `UserRow` edits. The lane was rebuilt by path before landing, so each commit
  holds only its own files, and the rebuilt tree was proved identical.
