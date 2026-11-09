# Overlapping libraries, PR 2: nested directories across libraries — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build §4 of `docs/superpowers/specs/2026-11-02-overlapping-libraries-design.md`: a directory may sit inside another library's directory (N). Each file is scanned once, under the deepest directory that holds it, and named by that directory's rules and settings. The outer library shows it too, once, with one reading position. A directory inside or around another of the same library (W) stays refused.

**Architecture:** The add and Move checks compare inside and around only against the target library, and the add's comparison moves inside its transaction (Task 1, closing O204). Reach, meaning the directories a library shows, is one pure function over the `root` table. `Access` carries an account's granted libraries beside the directories it reaches, so grants decide which libraries an account lists and opens, and reach decides which books (Task 2). Every site that built a library's directory set itself goes through reach, and "Scan library" walks what the library shows (Task 3). The outer walk skips nested directories, and an inner directory's scan takes over the outer's rows and members under it (Task 4). Removing a directory hands its files to the directories around and inside it before anything is counted as removed (Task 5). A work's hand document moves with it both ways (Task 6). The directory page says what each directory governs (Task 7), and the docs close it (Task 8).

**Tech Stack:** Rust (`taiyaku-library`, `taiyaku-serve`, `taiyaku-cli`), SQLite (system library: 3.54 here, 3.40.1 in CI and the image), TypeScript/React (`web/`).

**Spec:** §4, with §1, §6 and §7. The code was read and run for this plan at af107c43:
- `.superpowers/sdd/overlap-evidence/pr2/survey-store.md`, with logs `survey-store-run-*.log`;
- `.superpowers/sdd/overlap-evidence/pr2/survey-serve.md`;
- `.superpowers/sdd/overlap-evidence/pr2/handover-experiment.md`, with logs `handover-run-*.log`.

Locations are given by symbol; line numbers are hints taken at af107c43.

**The user's ruling of 2026-11-04:**
- **"Scan library" walks every directory the library shows.** That is its own directories and each nested directory it reaches, each scanned as its own root by its own rules. The same holds for the CLI's `scan --library` (Task 3).

**What the plan settles that the spec left open.** These are told to the user, and are not rulings:
- **Grants decide which libraries an account lists and opens. Reach decides which books it sees.** The libraries design of 2026-10-05 §3, "a restricted account sees only granted libraries", holds. An account granted Manga sees Kids' books on Manga's page, but its sidebar does not list Kids and `?library=<Kids>` does not open. An account granted Kids cannot open Manga's page or OPDS feed by id (Task 2).
- **An equal directory stays refused across libraries until PR 3.** Without that, S reaches `root.path UNIQUE` as a 500. W stays refused, as the spec says. PR 1's copy collapse already stops W doubling pages (store survey §0), but one file would still get two directories' settings inside one library (Task 1).
- **Reach is decided by the library's own directory alone.** It uses that directory's `exclude_globs`, asked for each ancestor prefix of the nested directory's relative path, outermost first, exactly as the walk asks. It does not use `excludes_with_parents`, which is more permissive than the walk. It also applies the walk's rule that skips dot-named directories. It does not chain through intermediate directories, which is the spec's wording taken literally. Reach is computed per request, so a glob edit changes reach at once (Task 2).
- **O204.** Only the comparison with stored roots moves inside the transaction. Canonicalising and `read_dir` stay outside it, for the reason `create_library`'s comment gives (Task 1).
- **A directory inside another root's directory member is refused at add.** A directory member is a folder of page images read as one volume. Without the refusal, the store counts the member without the nested directory's pages and the reader serves them anyway (store survey §3) (Task 1).
- **The walk compares a symlinked child directory by its canonical path** when it skips nested directories. Otherwise a symlink to the inner directory hands its files back to the outer's rules (Task 4).
- **Move's "who gains and loses sight" is computed by reach,** before and after the move (Task 3).
- **A hand-over does what an in-root rule edit does: no better and no worse** (experiment cases 2 and 3).
  - The positions stay where they were, and the series follows the inner's rules.
  - An edition the inner's rules name differently is kept beside the new one, as the derive keeps an edition a rule edit renames, and is counted in `kept_editions`.
  - A volume straddling the boundary leaves later positions in the old numbering (O244's shape).
  - None of this is fixed here (Task 4).
- **The hand-back copies the inner's rows rather than walking the disk again.** The spec says "the outer subtree is rescanned". A walk cannot run inside the removal's transaction (`scan_root` opens its own), and walking outside it leaves the rehearsal unable to count without reading the disk. So the removal copies the inner's `scan_entry` rows into the outer through the outer's globs. A file the inner excluded and the outer would not exclude appears at the outer's next scan, as a new file would (Task 5). *(Superseded by decision 21 and spec §4: a file the inner excluded, at the hand-over or after, is recorded again under the work it had at the outer's next scan, where the outer's patterns admit it.)*
- **An outer directory's file-level glob governs nothing under a nested directory.** Reach is per directory, so Manga shows such a file while Kids holds it, and the glob applies again when the file is handed back (Task 5).

## Global Constraints

- A code comment says why: the reason, the invariant, what breaks if it changes. No history and no measurements go in a comment; they go in `docs/notes.md` or are dropped. A comment may point at a spec section or an O-number. `scripts/check-comments.mjs` enforces part of this in the gate.
- No AI attribution anywhere.
- **Crate runs** cover whole crates, never named test files. Each run is logged and read from the log:
  ```sh
  E=.superpowers/sdd/overlap-evidence/pr2; mkdir -p "$E"
  cargo nextest run -p taiyaku-library --all-features --test-threads=8 >| "$E/T<n>-library.log" 2>&1; print -r -- "exit $?" >> "$E/T<n>-library.log"
  ```
  - Do the same for `-p taiyaku-serve` and `-p taiyaku-cli` where a task names them.
  - Quote the summary line and the `exit` line.
  - Then run `cargo clippy -p <crate> --all-targets --all-features -- -D warnings` and `cargo fmt --all -- --check`, each into its own log.
- **Web runs** go from `web/`: `./node_modules/.bin/vitest run --reporter=default --project <node|jsdom|browser>`, into a log.
  - Never use `pnpm exec`.
  - A browser project that fails with import or connection errors and no failed assertion is the headless renderer crashing. Check `coredumpctl` and rerun the whole project before suspecting the branch.
- **The baseline.** Before changing anything, a task runs its crates at its parent commit into `T<n>-before.log`.
  - After the change, a failing test that the task's list does not name is a regression until shown otherwise.
  - A listed test is rewritten to state the new behaviour, unless its entry says delete.
  - The task report names every rewrite and deletion, with its reason.
  - Never delete a test because it fails.
  - "Pre-existing" is established against the parent commit, never against main or another task's commit. Compare against the parent; never work from a list of failures you were told to expect.
- **Watched mutations.** For every new or changed assertion: put back by hand the defect it names, watch the test fail, restore, and check that `git diff` shows the restore. Record each one in the task report.
- **Nested fixtures.** Until Task 1 lands, only the unchecked `scan::add_root` builds N. From Task 1 on, tests build N through the checked add, as a person would.
- **SQL for SQLite 3.40.1.** No `RAISE()` with an expression (3.47), and no `concat()`, `string_agg` or ordered aggregates (3.44). Match a path prefix as `rel_path = ?1 OR substr(rel_path, 1, length(?1) + 1) = ?1 || '/'`, never with `LIKE`, since `_` and `%` occur in names. Task 7 runs the three crates in `rust:1-bookworm` (the recipe is in the memory note "SQLite 3.40.1 in CI and the image"; the controller supplies it).
- **The machine is shared and heavily loaded.** Kill only processes you started, by PID. Wait for load below 24 before a browser run, and record the load in the log.
- **Stop rule.** A task that reaches about 120 tool calls stops at a green point, commits what is green, and reports what is left, so the controller can split it.
- **Test counts.** `docs/status.md`'s two Rust figures ("Six crates, N tests" and "of the N are standing checks") are updated once, in Task 7, from the gate's own JUnit report.
- Evidence and reports go in `.superpowers/sdd/overlap-evidence/pr2/`, never in `target/`.
- Work on branch `overlapping-libraries-pr2`, in the main checkout. Make one commit per task, its subject saying what the task makes true. Nothing is pushed until the Finish.

---

### Task 1: A directory is refused only beside the same library's; the add compares inside its transaction

**Files:**
- `crates/taiyaku-library/src/scan.rs`:
  - `vetted` (~435-477), split into a disk half and a store half;
  - `add_root_checked_in` (~394-429) and `create_library` (~515-570);
  - the docs at ~290-300, ~340-357, ~432-434, ~515-529 and ~562-566.
- `crates/taiyaku-library/src/relocate.rs`: `apply_move` (~167), after `resolve`; the module doc.
- `crates/taiyaku-cli/src/library.rs`: the comment at ~345-348. It names a route that no longer exists and says the derive doubles pages, which PR 1 made untrue.
- Tests: `scan/tests.rs`, `relocate/tests.rs`, `taiyaku-cli/src/library/tests.rs`, `taiyaku-serve/src/libraries_api/tests.rs`.

**Interfaces:**
```rust
// scan.rs
/// Canonicalises `path` and reads it as a directory. Disk only: called before any transaction.
fn readable_dir(path: &Path) -> Result<PathBuf, AddRootError>;
/// Refuses `dir` against the stored roots, as the transaction sees them:
/// - equal to any library's directory;
/// - inside or around a directory of `library` (None: a library being made);
/// - inside any root's directory member (a `scan_entry` of kind `dir`).
pub(crate) fn clash(conn: &Connection, dir: &Path, library: Option<i64>) -> Result<(), String>;
```
`relocate::apply_move` calls `clash(conn, &dir.path, Some(target_id))` for `Target::Library`, skipping the directory itself. `Target::New` makes an empty library, so it needs no check. The refusal is `MoveError::Unusable`, which serve already answers as a 400 with the sentence.

**The rules.**
- **Equal**: refused against every library, in today's words: "`{chosen}` is already a directory of the library `{name}`".
- **Inside and around**: refused only against the target library's directories. Keep today's sentences and end them "of the same library, `{name}`" (spec §4).
- **Directory member**: refused against every root: "`{chosen}` is inside `{member}`, which the library `{name}` reads as the pages of one volume". The check knows only what the last scan recorded. Task 7 opens an O-row for a member that appears around an existing nested directory later, and for the `exclude_globs` shape the store survey §3 found.
- **The transaction.** `add_root_checked_in` opens one `Nested` around three steps: resolving the target library (so `InLibrary::Named`'s new library is read inside it), `clash`, and `add_root_in`. `add_root_in`'s own `Nested` then becomes a savepoint. `create_library` calls `clash` inside its transaction, after `Nested::open`, with `library: None`. Its comparison of the given paths with each other stays.
- `vetted`'s name goes. Its two halves are called in that order everywhere it was called.

**Tests to rewrite** (each N case becomes an accepted case; where the test is about the refusal, it moves inside one library as W):
- `scan/tests.rs`:
  - `a_root_inside_an_existing_root_is_refused` (~775), `a_root_containing_an_existing_root_is_refused` (~811) and `a_containing_roots_refusal_names_a_deeper_held_dir_by_its_whole_relative_path` (~847) become W, in the new words;
  - `a_root_reached_only_through_a_symlink_is_still_recognised_as_nested` (~923) becomes W;
  - `a_refused_dir_names_no_library_into_being` (~1383) refuses through W or an equal path, whichever keeps its point (no library is named into being);
  - the first half of `a_library_is_not_made_under_a_name_already_taken` (~4190-4206), the same way.
- `taiyaku-cli` `library/tests.rs`: `scan_add_refuses_a_root_inside_a_known_root` (~2269) and `scan_add_refuses_a_root_containing_a_known_root` (~2306). These are O47's pins.
  - With `--library` naming one library for both, refused.
  - With the default (a library per directory), accepted. The work then has one member per file and its page total counts each page once. PR 1's copy collapse holds that now, and Task 4's walk skip holds it after.
- `taiyaku-serve` `libraries_api/tests.rs`: `a_dir_is_added_to_a_library_by_the_librarys_id` (~669). Its inside case (~720-728) adds root-a's path to root-b, which is N: it is now accepted. Add the same case into root-a as W, refused in the new words. Its equal case (~729-735) stays.
- These stay as they are: `a_root_already_held_is_refused_as_already_a_dir_of_its_library` (~872), `a_library_is_refused_whole_for_one_bad_dir` (~4098), and the sibling test (~902).
- The two web tests mock the sentence and pin nothing (`Libraries.shell.browser.test.tsx` ~549, `LibraryPage.test.tsx` ~697). Leave them.

- [ ] **Step 1.** Baseline: run `taiyaku-library`, `taiyaku-serve` and `taiyaku-cli` at the parent.
- [ ] **Step 2: Write the failing tests.**
  - N is accepted through `InLibrary::Id`, `InLibrary::Named`, `InLibrary::Own` and `create_library`, inner first and outer first.
  - W is refused both ways in the new words.
  - An equal path held by another library is refused with that library's name. Through serve, `POST /api/library/libraries/{id}/dirs` answers 400 with the sentence, not 500.
  - A directory inside a `dir` member of another library's root is refused, naming the member and the library.
  - **O204's race.**
    1. Connection A opens an `IMMEDIATE` transaction and inserts library X's root `/t/x`.
    2. A thread then calls `add_root_checked_in(/t/x/inner, InLibrary::Id(X))` on connection B.
    3. A sleeps one second and commits.
    4. B must answer the W refusal.

    A defect that compares before B's transaction reads the roots without `/t/x` and accepts. Load can make this test miss the defect (B starting after A commits). It cannot make it fail on correct code. Say so in its doc comment.
  - Move: moving Kids' `/m/kids` into Manga, which holds `/m`, is refused by `move_cost` and by `move_root` in the same words, and changes nothing. Moving `/m` into a library holding `/m/kids` is refused. Moving `/m/kids` to a library holding `/x` is accepted.
- [ ] **Step 3.** Implement.
- [ ] **Step 4: Watched mutations.** Make each change, watch the named test fail, restore:
  - Drop the library filter from inside/around (N refused again).
  - Limit equal to the same library (the serve test gets a 500).
  - Call `clash` before the `Nested` in `add_root_checked_in` (the race test).
  - Drop Move's call.
  - Drop the directory-member branch.
- [ ] **Step 5.** Crate runs (three crates), clippy, fmt. Commit: "A directory is refused only beside the same library's directories, and the add compares the stored roots inside its transaction (O204)".

### Task 2: Reach, and grants beside it

**Files:**
- Create `crates/taiyaku-library/src/reach.rs` and `crates/taiyaku-library/src/reach/tests.rs`, following the crate's `src/<mod>/tests.rs` layout. Register the module in `lib.rs`.
- `crates/taiyaku-library/src/scan.rs`: `Excluder` (~1180-1240) becomes `pub(crate)`, and gains `visits`.
- `crates/taiyaku-library/src/account.rs`: `Access` (~123-138), `allows`, `access_for` (~187-206) and the docs (~166).
- `crates/taiyaku-library/src/browse.rs`: `library_scope` (~388-418), `narrowed` (~420-430), `status` (~1740-1770) and their docs.
- `crates/taiyaku-serve/src/auth.rs` (~391-422) and `crates/taiyaku-serve/src/opds.rs` (~242-298), where `Access` is built or matched.
- Every test that builds `Access::Only(..)`, about 120 sites, most in `browse/tests.rs` and `account/tests.rs`. Change them mechanically (see the rules).

**Interfaces:**
```rust
// scan.rs
impl Excluder {
    /// Whether this root's walk would reach `rel` (relative to the root, `/`-separated):
    /// no component starts with `.`, and `excludes` answers false for each ancestor prefix,
    /// outermost first, and for `rel` itself with `is_dir`. The walk's own question, top-down.
    pub(crate) fn visits(&self, rel: &str, is_dir: bool) -> bool;
}
// reach.rs
/// Each root in `of`, and every root nested strictly inside one of them whose path, relative
/// to that one, its `Excluder::visits` as a directory. Ascending, deduplicated.
/// No chaining: a root nested two deep is asked of each outer root in `of` directly.
pub fn reach(roots: &[scan::Root], of: &[i64]) -> Vec<i64>;
// account.rs
pub enum Access { Every, Only { roots: Vec<i64>, libraries: Vec<i64> } }
impl Access {
    pub fn allows(&self, root: i64) -> bool;      // roots: the directories reached
    pub fn grants(&self, library: i64) -> bool;   // libraries: the libraries granted
}
```

**The rules.**
- Nesting is `Path::starts_with` on canonical `root.path` (component-wise), as `vetted` compared. Never use a string prefix: `/mx` is not inside `/m`.
- `access_for` returns `Only { roots: reach(&scan::roots(conn)?, granted_roots), libraries: granted_library_ids }`, both ascending. An account granted a library with no directory still has that library in `libraries`.
- `status` lists the libraries `access.grants`, not the libraries whose roots it reaches. `Every` lists every library, as now.
- `library_scope(conn, access, library)`:
  - answers `None` unless `access.grants(library)`;
  - its `dirs` are `reach` of the library's own roots, filtered by `access.allows`;
  - it answers `None` when none are left, as now.
- `narrowed` builds `Only { roots: scope.dirs, libraries: vec![library] }`. It must not expand again: under the no-chaining rule a second expansion can add a root that the first one left out.
- `Principal::from_account` replaces an administrator's access with `Every`, as now.
- **The mechanical test change.** Add a test-only constructor `Access::roots(Vec<i64>)` that grants no library. Rewrite `Access::Only(v)` in tests as `Access::roots(v)`, except where the test reads `status` or `library_scope`: there, name the libraries.

- [ ] **Step 1.** Baseline: run `taiyaku-library`, `taiyaku-serve` and `taiyaku-cli`.
- [ ] **Step 2: Write the failing tests.** Fixture: Manga holds `/m`, Kids holds `/m/kids`, with a book in each.
  - **`reach` unit tests:**
    - A nested root is included; a sibling `/mx` is not.
    - A root excluded by the outer's glob `kids/` is left out.
    - With `private/` excluded and `!private/kids` whitelisted, `/m/private/kids` is still left out. That is the walk's answer, and `excludes_with_parents` would give the other one.
    - A dot-named `/m/.kids` is left out.
    - Two deep, `/m`, `/m/kids` and `/m/kids/tiny`:
      - `/m` excluding `kids/tiny`: Manga does not reach tiny, and Kids does.
      - Kids excluding `tiny`: Kids does not reach it, and Manga does.
    - The output is ascending and deduplicated.
  - **`access_for`:** an account granted Manga reaches Kids' book. An account granted Kids does not reach Manga's.
  - **`status`:** the Manga account lists Manga alone, and the Kids account lists Kids alone. Today's `status_lists_only_the_libraries_the_grant_names` (`browse/tests.rs` ~1785) has no nesting, so this is a new test.
  - **`library_scope`:**
    - The Manga account opening Kids answers `None`, and opening Manga lists both books.
    - The Kids account opening Manga answers `None`.
    - With `/m` excluding `kids/`, Manga's page lists Manga's book alone, and the Kids account still sees Kids' book.
  - **Serve:** the Kids account's `GET /opds/v1.2/libraries/<Manga>/works` is a 404, and its feed carries no Manga title. The Manga account's sidebar (`GET` of the status route) lists only Manga.
- [ ] **Step 3.** Implement.
- [ ] **Step 4: Watched mutations.** Make each change, watch the named test fail, restore:
  - Drop the glob test from `reach`.
  - Use `excludes_with_parents` in `visits` (the whitelist case).
  - Drop the dot rule.
  - Make `status` list by reach.
  - Drop the grant check from `library_scope`.
  - Make `narrowed` expand again (the two-deep case through a library page).
- [ ] **Step 5.** Crate runs (three crates), clippy, fmt. Commit: "A library reaches the directories nested in its own, and an account lists and opens only the libraries it is granted".

### Task 3: Every library-wide set goes through reach, and "Scan library" walks what the library shows

**Files:**
- `crates/taiyaku-library/src/browse.rs`: `held_in` (~1685-1700, the OPDS by-work feed) and its doc (~1671-1683).
- `crates/taiyaku-library/src/libraries.rs`:
  - `list` (~35-78), the works count;
  - `books` (~217-232), "no readable files yet";
  - `split_editions` (~300-399), the per-library tally (~311-323, ~341-344, ~378-399);
  - the module doc (~1-11) and `Listed::works`'s doc (~24-27).
- `crates/taiyaku-library/src/relocate.rs`: `granted_only` (~139-160) and its use (~173-174).
- `crates/taiyaku-serve/src/libraries_api.rs`: `dirs_of` (~84-96) and `scan_dirs` (~414-422). `crates/taiyaku-serve/src/scan_jobs.rs` (~69, Reread).
- `crates/taiyaku-cli/src/library.rs`: `select_roots`' `--library` branch (~461-464).
- Tests beside each.

**The rules.**
- A library's directory set is `reach::reach(&roots, its own root ids)` wherever these sites read `root.library_id = ?`.
- `list` counts the works with a member under the library's reach. `books` reads `scan_entry` under its reach. `held_in` lists the works with a member under its reach.
- `split_editions`:
  - It tallies each library by its reach, so a member under `/m/kids` counts for Manga and for Kids.
  - An edition with `c001` under `/m` and `c002` under `/m/kids` is whole for Manga and split for Kids.
  - A byte-identical copy is still not a case.
- Move's sight lists name, in name order, the accounts with role `user` and `every_library = 0`:
  - `gains_sight`: those that reach the moved directory after the move and not before;
  - `loses_sight`: those that reach it before and not after.

  Each account's reach is computed by `access_for`'s rule, before the `UPDATE` and after it, inside the same transaction. An account granted Manga does not lose sight of `/m/kids` when it moves from Kids to another library.
- "Scan library", the scan job's Reread, and `taiyaku library scan --library NAME` walk every directory in the library's reach, each as its own root (the user's ruling). Order them by path, so an outer directory is walked before the directories inside it.

- [ ] **Step 1.** Baseline.
- [ ] **Step 2: Write the failing tests** (Manga `/m`, Kids `/m/kids`, a book in each):
  - `list` counts Kids' work for Manga and for Kids.
  - `books` answers non-empty for a Manga whose only files are under `/m/kids`.
  - `held_in(Manga)` names Kids' work.
  - `split_editions` gives the case above.
  - Moving `/m/kids` from Kids to Other leaves a Manga-and-Kids account in neither list, puts a Kids-only account in `loses_sight` and an Other-only account in `gains_sight`.
  - Manga's scan job, and the CLI's `scan --library Manga`, walk `/m` and `/m/kids`, while Kids' walk only `/m/kids`. With `/m` excluding `kids/`, Manga's walk only `/m`.
- [ ] **Step 3.** Implement.
- [ ] **Step 4: Watched mutations.** Put each site's `root.library_id = ?` back, one at a time, and watch its test fail. Count sight by grants, as before.
- [ ] **Step 5.** Crate runs (three crates), clippy, fmt. Commit: "Library counts, feeds, split editions, Move's sight lists and Scan library go through what each library reaches".

### Task 4: The outer walk skips nested directories, and an inner directory's scan takes over the outer's rows under it

**Files:**
- `crates/taiyaku-library/src/scan.rs`:
  - the walk: `scan_root_inner` (~1474), `measure` (~2038) and `measure_dir` (~2056; the dot skip ~2090, `is_dir` ~2094, the glob ~2109);
  - the scan transaction (~1570-1619): `forget_unseen` (~1906-1938) and the §8.1 empty-scan refusal (~1486-1494);
  - `ScanReport` (~1277);
  - the module doc (~8-33).
- `crates/taiyaku-cli/src/library.rs`: the scan line that prints a `ScanReport`.
- `crates/taiyaku-serve`: the scan report's JSON (grep `ScanReport` in `config_api.rs` and `scan_jobs.rs`).
- Tests: `scan/tests.rs`, and `derive/tests.rs` for the comparison test.
- Start from `.superpowers/sdd/overlap-evidence/pr2/handover-experiment-code.diff`. It is the experiment's throwaway code: read it for the shape, never apply it as is.

**Interfaces:**
```rust
pub struct ScanReport { /* … */ pub taken_over: usize /* rows taken from directories around this one */ }
```

**The rules.**
- **The skip set** is the roots strictly inside this root (`Path::starts_with` on canonical paths), read from `scan::roots` before the walk. `measure_dir` skips a child directory in the set. A child that `entry.file_type()` reports as a symlink is canonicalised and compared by its target, so a symlink to the inner directory is skipped too.
- **The guard.** Neither `forget_unseen` nor §8.1's held count acts on a row at or under a directory the walk skipped. That is the set the walk used, whether or not it is still a root when the transaction opens: a removal can hand rows back in between (Task 5). So an outer scanned before the inner's first scan leaves those rows for the inner, counts nothing gone, and refuses nothing.
- **The hand-over.** It runs in root R's scan transaction, after R's rows are recorded. For each root E strictly around R, with `rel` = R's path relative to E:
  - Delete E's `scan_entry` and `dir_state` rows at or under `rel`, except those at or under a root nested inside R. Those belong to that root's own hand-over.
  - `UPDATE member SET root_id = R` for E's members whose absolute `member.path` is under R and not under a root nested in R. Stale-edition members move too. *(Superseded by decision 21: only the members of files R's scan recorded move.)*
  - Migration 018's `member_root_on_update` trigger moves the `member_root` row, so nothing else writes `member_root`.
  - Count the deleted `scan_entry` rows in `taken_over`. `gone` never includes them.
- **What the derive then does is an in-root rule edit's equal** (experiment cases 2 and 3). Cases 2 and 3 compared the hand-over with an in-root rule change and found the same rows, root id aside. That is no better and no worse, and nothing is added to make it better:
  - The series is named by R's rules.
  - An edition R's rules name differently is kept beside the new one, counted in `kept_editions` (the stated cost in `DeriveReport::kept_editions`'s doc). The hub and the reader's position stay on the kept edition.
  - A volume that straddles R's boundary leaves positions after the moved member in the old numbering, uncounted. That is O244's shape; Task 8 records it there.
- **The CLI and serve** carry `taken_over`. The CLI prints it as "N taken over from the directory around it" when it is not zero. Task 7 draws it in the web.

- [ ] **Step 1.** Baseline: run `taiyaku-library`, `taiyaku-serve` and `taiyaku-cli`.
- [ ] **Step 2: Write the failing tests.** Fixture: Manga holds `m/` with `m/kids/Bleach/v01.cbz`, `v02.cbz` and `m/Naruto/Naruto v01.cbz`, scanned and derived. A position and a note sit on Bleach v01, and a hand-set title is on it.
  - **Adding Kids at `m/kids` and scanning Kids:**
    - each Bleach file has one `scan_entry`, Kids';
    - `member.root_id` is Kids and `member_root` is {Kids};
    - the work ids, the position, the note and the title are kept;
    - the series is `Bleach`;
    - `taken_over` is 4 (two files, two directories);
    - Manga's next scan reports `gone` 0.
  - **Order:** Manga scanned after Kids is added and before Kids' first scan refuses nothing, counts nothing gone and leaves the rows. Kids' scan then takes them over. An outer whose only content is the nested directory scans without the §8.1 refusal.
  - **The comparison test** (`derive/tests.rs`):
    - In store A, Kids has `edition_key = en` and takes over.
    - In store B, one root has a rule scoped to `kids/**` that sets edition `en`.
    - The two dumps of `work`, `edition`, `member`, `progress` and `note` agree, root ids aside.
    - Name the test for that claim. It is what pins "an in-root rule edit's equal".
  - **Three levels:** `m`, `m/kids` and `m/kids/little`, with Kids scanned first. Little's files stay Manga's rows and members until Little's scan, which takes them from Manga. Kids never holds them.
  - **Symlink:** `m/alias -> m/kids`. Manga's walk skips `alias`, and the files are named by Kids' rules (series `Bleach`, not `alias`).
  - **Sibling:** `m/kidsx` is not skipped by Kids' walk or treated as nested.
  - Tighten Task 1's two CLI O47 pins to one `scan_entry` per file.
  - **Run and report, without asserting:** Kids holding a chapter subdirectory of a directory Manga's rules claim as one volume (`derive::nested_in_claim` ~1200). If a file is counted twice, or a member's stored pages disagree with `taiyaku_read::member_pages`, stop and report. The controller decides whether Task 1's refusal extends to it.
- [ ] **Step 3.** Implement.
- [ ] **Step 4: Watched mutations.** Make each change, watch the named test fail, restore:
  - Compare the symlinked child by path (the symlink test).
  - Drop the guard from `forget_unseen` (the order test's `gone`).
  - Drop it from the §8.1 count (the order test's refusal).
  - Leave out the nested-of-R exclusion (three levels).
  - Drop the re-point (`member.root_id` stays Manga).
  - Count the taken-over rows in `gone`.
- [ ] **Step 5.** Crate runs (three crates), clippy, fmt. Commit: "Each file is scanned once, under the deepest directory that holds it: the outer walk skips nested directories, and an inner directory's scan takes over the outer's rows and members".

### Task 5: Removing a directory hands its files to the directories around and inside it before anything is counted

**Files:**
- `crates/taiyaku-library/src/scan.rs`:
  - `remove_root` (~1069), `apply_removal` (~957), `take_root` (~800);
  - `removal_cost` (~1014), `apply_library_removal` (~1091-1118), `library_removal_cost`;
  - `DOOMED_WORKS` (~740) and its doc.
- `crates/taiyaku-serve/src/config_api.rs`: `delete_root` (~1867-1896), its scan-reserved check (~1889).
- `crates/taiyaku-serve/src/libraries_api.rs`: the library removal route (~376-378).
- Tests: `scan/tests.rs`, `config_api/tests.rs`, `libraries_api/tests.rs`.

**Interfaces:**
```rust
/// Inside the caller's transaction, before `take_root(tx, removed)`:
/// - copy `removed`'s rows under each root nested in it to that root;
/// - copy its own rows to the nearest root around it;
/// - re-point the members that follow.
/// Returns the rows copied.
fn hand_back(tx: &Connection, removed: i64) -> Result<usize, String>;
```

**The rules.**
- **To the nearest root around it.** E is the deepest root strictly around the removed root R; with none, nothing is copied.
  - Copy R's `scan_entry` rows into E, with `rel_path` prefixed by R's path relative to E and `sha256` as is.
  - Leave out each row E's `Excluder::visits(rel, is_dir)` answers false for, and each row at or under a root nested in R.
  - Do not copy `dir_state`: E's next walk lists those directories whole.
  - Re-point R's members under R, and not under a root nested in R, to E.
- **To each root nested in it.** These are rows R holds under a nested root N that N has not taken over, because N was added and never scanned.
  - Copy them into N, with the prefix stripped.
  - Re-point those members to N.
  - Removing Manga before Kids' first scan therefore loses nothing under `m/kids`.
- Then `take_root` runs as now. It dooms only what nothing took: rows E's globs exclude, or R with no root around it.
- **The rehearsal does the same.** `removal_cost` and `library_removal_cost` run `hand_back` inside their rolled-back transaction, so the rehearsal counts what the removal does.
- **`apply_library_removal`** runs `hand_back` before each `take_root`. Within one library, directories never nest (W), so a root that receives rows is always another library's.
- **Serve refuses a removal** while a scan of R, of a root around it, or of a root nested in it is reserved. The answer is a 409 in the scan-reserved words already used. Both routes refuse.
- **The outer's glob is per file.** A file-level glob on E, such as one naming `kids/Bleach/v02.cbz`, governs nothing while Kids holds the file: Manga shows it, since reach is per directory (spec §4). On the hand-back the glob applies, and the rehearsal counts that file's work as removed (experiment, contradiction 5). This is the spec as written: test it, and do not change it.

- [ ] **Step 1.** Baseline.
- [ ] **Step 2: Write the failing tests.** Use Task 4's fixture after the hand-over.
  - Removing Kids: the rehearsal and the removal count works 0, positions 0, notes 0. The work ids, the position, the note and the title are kept. `member.root_id` is Manga. Manga holds `scan_entry` rows at `kids/…` with their `sha256`. The series is `kids` again.
  - With `m` excluding `kids/`, removing Kids counts Kids' works as removed. The rehearsal equals the removal.
  - With `m` excluding `kids/Bleach/v02.cbz`: Manga's page shows v02 while Kids holds it. Removing Kids counts v02's work as removed.
  - Three levels: removing Kids hands Kids' own files to Manga and leaves Little's with Little.
  - Removing Manga before Kids' first scan keeps everything under `m/kids`, counted as nothing removed. Kids' first scan then finds its rows already there.
  - Removing Kids' whole library gives the same as removing its directory.
  - Serve: `DELETE` of `m/kids`'s directory while Manga's scan is reserved is a 409, for the rehearsal and the removal, and for the library route.
  - **The guard of Task 4, at unit level.** `forget_unseen`, given a skip set naming `kids` that is no longer a root, leaves the rows under `kids`.
- [ ] **Step 3.** Implement.
- [ ] **Step 4: Watched mutations.** Make each change, watch the named test fail, restore:
  - Skip `hand_back` in the removal (counts 2/1/1).
  - Skip it in the rehearsal only.
  - Copy without the glob filter.
  - Copy rows under a root nested in R.
  - Drop the nested-direction copy.
  - Drop the enclosing-root check from serve's refusal.
- [ ] **Step 5.** Crate runs (three crates), clippy, fmt. Commit: "Removing a directory hands its files to the directories around and inside it first, so only what none of them holds is removed".

### Task 6: A work's hand document follows its address across a hand-over

**Files:**
- `crates/taiyaku-library/src/hand.rs`: `rename_for_rekey` (~356) and its doc (~330-355); `path_for` (~60).
- `crates/taiyaku-library/src/scan.rs`: Task 4's hand-over and Task 5's `hand_back`, which collect the documents to move, and the code after each commit.
- Tests: `hand/tests.rs` and `scan/tests.rs`.

**Interfaces:**
```rust
/// Moves the document at `old` to `work_id`'s address, when that address's root keeps
/// documents and nothing is there. Never overwrites, as `rename_for_rekey` never does.
pub fn rename_from(conn: &Connection, work_id: i64, old: &Path) -> Result<bool, String>;
// rename_for_rekey(conn, work_id, old_key) becomes rename_from(conn, work_id, &<its old path>).
```

**The rules.**
- Inside the transaction, before any member is re-pointed (and, in a removal, before its `Skip` derive, which can rekey), the hand-over and `hand_back` collect `(work_id, path_for(work_id))` for each work whose hub's first member they move.
- After the commit, and before the derive that follows, each pair is renamed with `rename_from`. That covers the scan's commit in Task 4, and the removal's commit, before its `Reconcile` derive, in Task 5.
- So a removal that rekeys the work carries the directory and the key in one rename. That fixes, for handed-back works, the stranding the store survey §6 found.
- A receiving directory that keeps no documents leaves the document where it was. `reconcile` then reports it as an orphan, as it does today. *(True of a hand-over only: a hand-back leaves it in the removed directory's `.taiyaku`, which no root reconciles, so nothing reports it; `docs/status.md`, O247.)* *O247 has since closed, on 2026-11-08 — see its entry in `docs/status.md`.* Where an address is occupied, the file is not moved, and `reconcile` reports both.

- [ ] **Step 1.** Baseline: run `taiyaku-library`.
- [ ] **Step 2: Write the failing tests.** Fixture: Manga keeps documents, with a hand-set title on Bleach v01.
  - **Kids keeps documents:**
    - After the hand-over, the document is at `m/kids/.taiyaku/<key>`, nothing is at `m/.taiyaku/<key>`, and `reconcile` reports no orphan for either root.
    - After the hand-back, it is at `m/.taiyaku/<key>`, and nothing is left under `m/kids/.taiyaku`.
  - **Kids keeps none:** the document stays at `m/.taiyaku/<key>` and is reported as an orphan with its title. The title is kept in the store.
  - **An occupied address** is not overwritten.
  - **The straddle** (Task 4's fixture, with the work rekeyed by the hand-back): the document ends at the new key's address under Manga.
- [ ] **Step 3.** Implement.
- [ ] **Step 4: Watched mutations.** Make each change, watch the named test fail, restore:
  - Drop the renames (the orphan comes back).
  - Overwrite an occupied address.
  - Collect after the `Skip` derive (the straddle's old key is lost).
- [ ] **Step 5.** Crate run, clippy, fmt. Commit: "A work's hand document moves with it when a directory takes its files over or hands them back".

### Task 7: The directory page says what each directory governs, and a scan says what it took over

**Files:**
- `crates/taiyaku-serve/src/libraries_api.rs`: `libraries_json` (~100-120). Leave `config_api::root_json` (~1197) alone: it is also `GET /api/library/roots`' shape.
- `web/src/libraries.ts` (~62, the `Library` type and its `dirs`) and `web/src/config.ts` (`Root`, ~74-93).
- `web/src/RootRow.tsx`: the header (~724-773). Also `config.ts`'s `regimeFacts` (~423-452) if the line goes there, and `web/src/DirPage.tsx` (~142-182).
- `web/src/ScanReport.tsx` (~291, beside `Gone`).
- Tests: `libraries_api/tests.rs`, plus the web tests beside each component (jsdom, and a browser test where the line changes layout).

**Interfaces** (each directory in `GET /api/library/libraries`):
```json
"nested": [{ "path": "/m/kids", "library": "Kids", "shown": true }],
"inside": [{ "path": "/m", "library": "Manga", "shows": true }]
```
- `nested` lists the directories nested in this one, whose settings govern their own files. `shown` says whether this directory's library shows them, which is reach.
- `inside` lists the directories this one sits in (a list, since `/m/kids/tiny` sits in two). `shows` says whether that library shows this directory's books.
- Both are computed in `libraries_json` from the roots it already reads, through `reach::reach`.
- The web type for a library's directory extends `Root` with the two fields. The C4 screens spec §3.2 and §8 rule out working nesting out in the browser.

**The copy** is a first draft for the user to try:
- **Outer:** "`/m/kids` has settings of its own (Kids). Manga shows its books." Or, when `shown` is false: "… Its exclusions here keep those books out of Manga."
- **Inner:** "Inside `/m` (Manga). Manga shows these books too, named by this directory's settings." Or, when `shows` is false: "… Manga's exclusions keep these books out of it."
- **Scan report:** a `Taken over` count beside `Gone`, drawn only when it is not zero.

- [ ] **Step 1.** Baseline: run `taiyaku-serve`, and the web `node`, `jsdom` and `browser` projects.
- [ ] **Step 2: Write the failing tests.**
  - `libraries_json` gives Manga's `/m` `nested` [Kids' `/m/kids`, shown], and gives Kids' `/m/kids` `inside` [`/m`, shows].
  - With `m` excluding `kids/`, `shown` and `shows` are both false.
  - Two deep, `inside` lists both directories around it.
  - Web: each directory page draws its line. No line is drawn for a directory with neither field. `Taken over` is drawn only when it is not zero.
- [ ] **Step 3.** Implement.
- [ ] **Step 4: Watched mutations.**
  - Compute `shown` without the glob.
  - Draw the line from the wrong field.
  - Draw `Taken over` at 0.
- [ ] **Step 5.** Crate run, web projects, `pnpm` lint and typecheck as `scripts/check-all.sh` runs them. Commit: "A directory's page says which directories around and inside it govern their own books, and a scan report says what it took over".
- [ ] **Step 6: The user tries it.** The controller does this step, before the task's review. Serve the branch on a scratch copy of the test library, following the memory note "UI work: prototype on a live server". Make Manga and Kids nest there. The user tries the directory pages, an add refused as W, and a scan that takes over. A copy change the user asks for is a fix commit on this task.

### Task 8: The docs

**Files:**
- `docs/status.md`:
  - the libraries summary (~129-162): ~137-138 "under its libraries' directories" becomes "under or nested in"; ~157 the sidebar lists the libraries it is granted; ~159-160 the OPDS sentence; ~162 "(its PR 1)" becomes "(its PRs 1 and 2)".
  - the Later entry "Libraries whose directories overlap" (~5919-5932): PR 1 merged as #225; PR 2 is this.
  - Close O204 (all five parts).
  - O47's row: its `--add` recipe is now N and is accepted. It stays fixed by the walk skip and by PR 1's copy collapse.
  - O244's row: a rule edit or a hand-over that moves a member out of an edition leaves the positions after it in the old numbering, uncounted (experiment case 4).
  - O238's row: the walk skip compares symlinked children canonically. That settles the second case only where the symlink points at a nested root.
- **Open, each with its row and bullet:**
  - A directory member that appears around an existing nested directory after it was added. Task 1's refusal knows only the last scan, and `exclude_globs` over a subdirectory of a directory member has the same shape on main (store survey §3).
  - A hand-back to a directory that keeps no documents leaves the inner's document under the inner's `.taiyaku`, unreported.
  - Anything the tasks' reports opened.
- Notes pointing at the overlapping-libraries spec §4, at the sentences spec §6 lists as reversed:
  - `2026-10-05-libraries-design.md`: §10 (~641-643) and §6.1 (~478-480). Add a "holds" note at §3 (~312): an account still sees only the libraries it is granted.
  - `2026-08-14-library-configuration-design.md`: §2 (~78-80) and §7 (~199).
  - `2026-09-02-library-configuration-build-design.md`: §5 (~398-402) and §7 (~450).
  - `2026-09-04-c4-screens-design.md`: §3.2 (~190) and §7 (~618).
- The overlapping-libraries spec §4 gains a paragraph listing what this plan settled. Copy the bullets from this plan's header, plus Task 4's "an in-root rule edit's equal" and Task 5's per-file glob.
- `README.md`:
  - ~66-69: the CLI's `--library` comment says a directory with no `--library` is a library of its own, and so can nest in another's.
  - ~186-190: editions "under or nested in" a granted library's directory.
  - ~277-278: the refusal list says "inside another of the same library".
  - ~286-287: "Scan library" walks each directory the library shows.
- In-app copy: re-read `web/src/SplitEditions.tsx` (~74-78), `RemoveLibrary.tsx` (~168-171) and `RemoveRoot.tsx` for "hold" that should now say "show" or "reach". Change them only where they became untrue.
- The test counts, from the gate's JUnit report. Measure the web counts.

- [ ] **Step 1.** Edit. Then run the status sweep (`grep -c '^| O[0-9]'`, closed count, "What remains"). Grep every O-number touched, bare, across `docs/` and `web/src/`, and read every hit.
- [ ] **Step 2.** Run the three crates in `rust:1-bookworm` with SQLite 3.40.1 (the controller supplies the recipe). Record the summary lines.
- [ ] **Step 3.** Run the full gate, `scripts/check-all.sh`, into `$E/gate-1.log`. Do not pipe it. Quote its `all green` line and the exit status.
- [ ] **Step 4.** Commit: "The docs say directories nest across libraries, and O204 closes".

### Finish (the controller)

1. **The whole-branch review.** Run it against main with Opus. Give the reviewer this plan, the spec §4 and both surveys. Ask it in particular for any site that still builds a library's directory set from `root.library_id`, and for any path the hand-over or the hand-back misses (Move, library removal, the CLI).
2. **One fix wave.** The fix commits are reviewed too.
3. **A drive on a scratch copy of the test library**, following the memory note "Verifying against real material":
   - Put a series directory of Manga's in a new library Kids.
   - Scan Manga, then Kids.
   - Read as an account granted Manga and as one granted Kids: sidebar, library pages, series page, reader, OPDS, downloads.
   - Keep a position and a note across the add.
   - Remove Kids and check that nothing was counted removed and the position held.
   - Record the user-agent and the trace of each act.
4. **The claim check** on this plan's claims, the status rows and the PR body, with passes after each correction.
5. **The PR body.** Its first line is the title. Add no attribution. Ask the user before pushing.
