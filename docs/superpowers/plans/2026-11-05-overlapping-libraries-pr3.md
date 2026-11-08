# Overlapping libraries, PR 3: the same directory in several libraries — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build §5 of `docs/superpowers/specs/2026-11-02-overlapping-libraries-design.md`: two libraries may hold the very same directory (S). One directory has one set of settings and one scan, and several libraries link it. Remove from this library unlinks it, and Remove directory removes it from every library. Removing a library removes its links, and a directory left with no link is removed. Move unlinks the directory from one library and links it to another.

**Architecture:** Migration 019 adds `library_root (library_id, root_id)` and drops `root.library_id`. Tasks 1 and 2 move every reader to the link while S stays refused, so nothing a person can do changes. Task 3 lets the add link an equal path, and the answers that name a directory's library name every library holding it. Task 4 adds the unlink and makes removals that empty several libraries name them all. Task 5 gives Move a source library and a directory scan the library it was started from. Tasks 6 to 8 are the web, and Task 9 the docs.

**Tech Stack:** Rust (`taiyaku-library`, `taiyaku-serve`, `taiyaku-cli`), SQLite (system library: 3.54 here, 3.40.1 in CI and the image), TypeScript/React (`web/`).

**Spec:** §5, with §1, §6, §7 and decisions 24–27. The code was read and run for this plan at 146054c8:
- `.superpowers/sdd/overlap-evidence/pr3/survey-store.md`, with logs `survey-store-run-*.log` (the census is `survey-store-run-3-census.log`; the migration shape that ran on both SQLite versions is §B.3, and what each trigger shape does is §B.4);
- `.superpowers/sdd/overlap-evidence/pr3/survey-serve.md`.

Locations are given by symbol. Line numbers are hints taken at 146054c8. The surveys' tables (store §A.1, serve §A and §B) are the site lists. Each task names the rows it owns.

**The user's rulings of 2026-11-05** (spec decisions 24–27):
- **Removal on a shared directory's page:** two rows, only while another library holds it. "Remove from Manga" reads "Kids keeps it; nothing is deleted.", and "Remove this directory" reads "From every library that holds it, with what it would take." A directory in one library keeps its one Remove…. Remove from this library never deletes: on the last link it is refused and points to Remove this directory.
- **Move to a library that already holds the directory** is refused ("`/m` is already in Kids"), and such a library is left out of Move's list.
- **New library's help** reads "The server checks that it can read the directory. It may be another library's directory too, or sit inside or around one."
- **A scan of a shared directory** names the library whose page started it. The other library's card reads "Manga is scanning `/m`, which Kids holds too." A scan asked for without a library names none. Decision 20's line for a nested directory stands.

**What the plan settles that the spec left open.** These are told to the user, and are not rulings:
- **The own library is named in Rust**, in the add, by the names `scan::own_library_clash` already works out. `root_gets_a_library` cannot become a trigger on the link (store survey §0.1). The code-point list duplicated between migration 016 and `trim_name` then lives in Rust alone.
- **The triggers are §B.3's three.** `root_keeps_a_library` refuses deleting a directory's last link unless the directory itself is being deleted. `library_emptied_by_unlink` and `library_emptied_by_move` delete a library left with no link. A directory committed with no link cannot be refused (§B.4), so each public writer is tested to leave every directory with a link.
- **Remove from this library refuses as Move does**: while the store's job lock is held, and while a scan is reserved in this library. So the page holds it exactly when it holds Move….
- **A link runs no watch probe and changes no setting.** The probe writes `scan_interval_secs`, which the libraries share. The add's answer says which libraries already held the directory.
- **New library over a directory another library holds links it**, as the add does.
- **`scan --add PATH` with no `--library`, over a directory another library holds, is refused as now.** It names no library to link to.
- **Move takes a required source library.** A default for a directory with one link would let a stale page move it out of a library it was linked to since.
- **The add asks W of the target library before it links.** At 146054c8, `scan::beside` returned at the first equal path, before it asked about nesting in the target. A plain relaxation would have linked `/m/kids` into a library that holds `/m` whenever `/m/kids` had the lower id (store survey §0.6).
- **A removal that empties several libraries names each of them.** Its `left_with_no_library` is every limited account whose every grant is among them. Asking `only_grant_holders` of each library in turn misses an account granted exactly those (store survey run 2).
- **Removing a library unlinks its shared directories and removes the rest.** It clears the scan slots of the removed directories only, and it says which directories it leaves to which library ("…and leaves `/m` to Kids.").
- **The directory page says the other libraries once**, in its header: "Also in Kids, which shares these settings and scans." Each name links to that library's page for the directory. The wording avoids decision 17's "Also shown in", which means nested.
- **After a link, the add says** "`/m` is also a directory of Kids. It keeps the settings it has there." The line is drawn where the picker was.

## Global Constraints

- A code comment says why: the reason, the invariant, what breaks if it changes. No history and no measurements go in a comment; they go in `docs/notes.md` or are dropped. A comment may point at a spec section or an O-number. `scripts/check-comments.mjs` enforces part of this in the gate.
- No AI attribution anywhere.
- **Crate runs** cover whole crates, never named test files. Each run is logged and read from the log:
  ```sh
  E=.superpowers/sdd/overlap-evidence/pr3; mkdir -p "$E"
  cargo nextest run -p taiyaku-library --all-features --test-threads=8 >| "$E/T<n>-library.log" 2>&1; print -r -- "exit $?" >> "$E/T<n>-library.log"
  ```
  - Do the same for `-p taiyaku-serve` and `-p taiyaku-cli`. Every Rust task runs all three crates, because a column one crate drops is read by another crate's tests.
  - Quote the summary line and the `exit` line.
  - Then run `cargo clippy --workspace --all-targets --all-features -- -D warnings` and `cargo fmt --all -- --check`, each into its own log.
- **Web runs** go from `web/`: `./node_modules/.bin/vitest run --reporter=default --project <node|jsdom|browser>`, into a log. Also run `./node_modules/.bin/tsc -b` and the biome check the gate runs.
  - Never use `pnpm exec`.
  - A browser project that fails with import or connection errors and no failed assertion is the headless renderer crashing. Check `coredumpctl` and rerun the whole project before suspecting the branch.
  - O225's Filmstrip test fails under load, on `main` too. If it is a browser run's only failure, record the load and the assertion, rerun the whole project once, and say so. It is not this branch's to fix.
- **The baseline.** Before changing anything, a task runs its crates or projects at its parent commit into `T<n>-before.log`.
  - After the change, a failing test that the task's list does not name is a regression until shown otherwise.
  - A listed test is rewritten to state the new behaviour, unless its entry says delete.
  - The task report names every rewrite and deletion, with its reason.
  - Never delete a test because it fails.
  - "Pre-existing" is established against the parent commit, never against main or another task's commit. Compare against the parent; never work from a list of failures you were told to expect.
- **Watched mutations.** For every new or changed assertion: put back by hand the defect it names, watch the test fail, restore, and check that `git diff` shows the restore. Record each one in the task report. Each task lists the mutations it must at least run.
- **S fixtures.** Until Task 3 lands, a test builds S only by inserting a second `library_root` row by SQL. From Task 3 on, tests build S through the checked add, as a person would.
- **SQL for SQLite 3.40.1.** No `RAISE()` with an expression (3.47), and no `concat()`, `string_agg` or ordered aggregates (3.44); `group_concat(… ORDER BY …)` is refused there (store survey run 4). Match a path prefix as `rel_path = ?1 OR substr(rel_path, 1, length(?1) + 1) = ?1 || '/'`, never with `LIKE`. A root's libraries are read with a second query, or sorted in Rust, never by a JOIN that returns a root once per link: the derive, the walk's skip and an unfiltered scan would then walk and derive it twice (store survey §A.1 row 1).
- **The machine is shared and heavily loaded.** Kill only processes you started, by PID. Wait for load below 24 before a browser run, and record the load in the log.
- **Stop rule.** A task that reaches about 120 tool calls stops at a green point, commits what is green, and reports what is left, so the controller can split it.
- **Test counts.** `docs/status.md`'s Rust and web figures are updated once, in the Finish, from the gate's own JUnit report.
- Evidence and reports go in `.superpowers/sdd/overlap-evidence/pr3/`, never in `target/`. Write them by Bash: the Write tool is refused under `.superpowers/`.
- Work on branch `overlapping-libraries-pr3`, in the main checkout. Make one commit per task, its subject saying what the task makes true. Nothing is pushed until the Finish.

---

### Task 1: A directory's libraries are links (S still refused)

**Files:**
- Create: `crates/taiyaku-library/src/schema/019_library_links.sql`, the shape of store survey §B.3.
- Modify: `crates/taiyaku-library/src/schema.rs` (`SCHEMA_VERSION` 19, `MIGRATIONS`), `scan.rs` (`Root`, `roots`, `add_root_in`, `beside`, `at_a_member`, `emptied_by_removing`, `apply_library_removal`), `reach.rs` (`of_libraries`), `libraries.rs` (`list`, `split_editions`), `browse.rs` (`status`), `relocate.rs` (`dir`, `resolve`, `apply_move`, module doc).
- Tests: `crates/taiyaku-library/src/schema/tests.rs` and every test or helper in the three crates that reads or writes `root.library_id` in SQL. The largest are listed in store survey §I (`library_of`, `test_support::grant_root`, `three_libraries`, `a_root`, `two_library_store`, …) and serve survey §I (`test_support.rs`, `auth/tests.rs`, `opds/tests.rs`, `routes/tests.rs`, `libraries_api/tests.rs`).

**Interfaces:**
- Produces: `scan::Root.libraries: Vec<i64>`, ascending. `Root.library_id` stays for this task only, set to the lowest link, so that serve, the CLI and every `.library_id` field read still compile. It is exact while every directory has one link.
- Produces: `library_root` with the three triggers of §B.3. A test helper `test_support::link(conn, library_id, root_id)` (or the library crate's equivalent) for the later tasks' S fixtures.

- [ ] **Step 1.** Run the three crates at the parent into `T1-before.log`.
- [ ] **Step 2.** Write the migration test: a store at 18 with two libraries, three roots (one library holding two), grants and positions. After migration 019 there is one link per old column value, `root` has no `library_id` column (`pragma_table_info`), grants are unchanged, and `root_gets_a_library`, `library_emptied_by_delete` and the old `library_emptied_by_move` are gone. Run it red.
- [ ] **Step 3.** Write the trigger tests, one per row of §B.4 that a writer relies on: (a) unlinking a directory's last link is refused; (b) linking `/m` into a second library and unlinking it from the first empties and deletes the first, grants and all; (c) `DELETE FROM root` of a directory with one link goes through and empties its library; (d) deleting a library whose directory has no other link is refused; (e) deleting a library whose directories are all linked elsewhere keeps them; (g) deleting a directory shared by two libraries, the last of one, deletes that library and keeps the other; an UPDATE of a link's library empties the source.
- [ ] **Step 4.** Write the migration and the version bump. Rewrite the six schema tests that pin the 015/016 triggers or plant the column at the current version (store survey §J: `schema/tests.rs` ~1522, ~1549, ~1695, ~1732, ~1786, ~1840) to state the same facts through the link, or through the Rust naming where the trigger named the library. Tests at partial versions (`migrate_with(&MIGRATIONS[..n])`) stay as they are.
- [ ] **Step 5.** `add_root_in` inserts the root, makes the own library in Rust where it is asked for (`InLibrary::Own`), and inserts the link, in its transaction. The naming tests in `scan/tests.rs` ~1539–1760 keep their names and expectations. Only their doc comments stop naming the trigger, and the test that pinned the trigger's `last_insert_rowid()` behaviour states what the Rust naming returns.
- [ ] **Step 6.** Every store reader in store survey §A.1 rows 1–15 reads the link. `relocate::apply_move` writes the link by UPDATE, so `library_emptied_by_move` fires. `libraries::split_editions`' `MemberDirectory.library` stays one id here (the lowest link): Task 3 changes its shape.
- [ ] **Step 7.** Every test and helper in the three crates that writes or reads `root.library_id` in SQL goes through the link. Change the helpers first; rewrite a test's own SQL only where no helper carries it.
- [ ] **Step 8.** Add a test that every public writer (`add_root_in`, `add_root_checked_in`, `create_library`, `move_root`, each removal) leaves every directory with at least one link. A store query does it.
- [ ] **Step 9.** Run the three crates, clippy and fmt. Run the migration test and `schema::tests` in `rust:1-bookworm` with SQLite 3.40.1 (the controller supplies the recipe), into `T1-bookworm.log`.
- [ ] **Watched mutations, at least:** drop the `EXISTS root` exemption from `root_keeps_a_library` (removal tests go red); drop `root_keeps_a_library` (test (a) goes red); skip the backfill INSERT (the migration test goes red); `of_libraries` filtering on the lowest link only (a Step 3 S fixture read through reach goes red: write one if none does).
- [ ] **Commit:** "A directory's libraries are links, and migration 019 drops `root.library_id`".

### Task 2: Nothing reads a directory's one library any more

**Files:**
- Modify: `crates/taiyaku-library/src/scan.rs` (drop `Root.library_id`), `relocate.rs` (`Dir.library_id`), and every compile error that follows in `taiyaku-serve` (store survey §A.1 rows 16–21: `config_api::start_scan`, `libraries_api::{dirs_of, libraries_json, nesting_json, scan_status_json, move_dir}`) and `taiyaku-cli` (row 22, `library::scan`'s `--add` line; `library/tests.rs`' `Root` literals; `user/tests.rs`).
- Tests: the field reads and the `Root` literals in the three crates (store survey §I "Compile breaks": about 66 field lines and 8 literals).

**Interfaces:**
- Consumes: Task 1's `Root.libraries`.
- Produces: no `library_id` on `Root` or `Dir`. Every JSON shape is unchanged. Where a shape holds one library today (`nesting_json`'s `library`, `MemberDirectory.library`, `start_scan`'s reserving library, the CLI's `--add` line), it takes the lowest link, which is exact while S is refused. Task 3 and Task 5 change those shapes.

- [ ] **Step 1.** Run the three crates at the parent into `T2-before.log`.
- [ ] **Step 2.** Drop the field and answer every compile error. A "library → its roots" site filters on `libraries.contains(&id)`. The CLI's `--add` line names the library the request named or made, not one read back from `Root`.
- [ ] **Step 3.** Run the three crates, clippy and fmt. The test list is unchanged: no test changes what it asserts in this task.
- [ ] **Watched mutation:** `dirs_of` filtering on `libraries.first()` instead of `contains`. No test can see it while S is refused. Record that as the expected result, and add the S fixture that sees it in Task 3.
- [ ] **Commit:** "Nothing reads a directory's one library".

### Task 3: Adding a directory another library holds links it, and every answer names each library holding a directory

**Files:**
- Modify: `crates/taiyaku-library/src/scan.rs` (`beside`, `vetted`, `clash`, `add_root_checked_in`, `create_library`, `probed`, `Added`), `libraries.rs` (`MemberDirectory`); `crates/taiyaku-serve/src/libraries_api.rs` (`add_dir`, `create_library`, `libraries_json`, `nesting_json`), `config_api.rs` (`added_json`); `crates/taiyaku-cli/src/library.rs` (the `--add` line).
- Tests: reverse `scan::tests::a_dir_another_library_holds_is_refused_by_that_librarys_name` (~1004–1035), `a_library_is_not_made_under_a_name_already_taken` (~4406–4420, its assertion over Manga's directory), and `libraries_api::tests::a_dir_is_added_to_a_library_by_the_librarys_id` (~675–750, its `:747` case). `a_root_already_held_is_refused_as_already_a_dir_of_its_library` (~869) keeps its same-library and `Own` cases.

**Interfaces:**
- Consumes: Task 1's `link` helper, Task 2's field-free `Root`.
- Produces: `scan::Added.also_in: Vec<i64>`, the other libraries that already held the directory (empty for a new directory). For a link the probe does not run, and `Added`'s probe fields are `None`.
- Produces, JSON: `added_json` gains `also_in: [name]` and has `watching`, `watch_reason` and `interval_secs` as `null` for a link. `create_library`'s `added[]` entries carry the same. `libraries_json`'s directory gains `libraries: [{id, name}]`, every library holding it, ascending by id, this one included. `nesting_json`'s `nested[].library` and `inside[].library` become `libraries: [name]`. `MemberDirectory.library` becomes `libraries: [i64]`.

- [ ] **Step 1.** Run the three crates at the parent into `T3-before.log`.
- [ ] **Step 2.** Write the failing tests:
  - Manga holds `/m`. Adding `/m` to Kids links it: one `root` row, two links, the settings unchanged (set Manga's `/m` to a non-default `scan_interval_secs`, `edition_key` and `exclude_globs` first), and `also_in` names Manga.
  - Kids holds `/m/kids` **with the lower id**, Manga holds `/m`. Adding `/m/kids` to Manga is refused as inside a directory of the same library. Build the ids in that order; a fixture where `/m` has the lower id passes against the unreordered `beside`.
  - Adding `/m` to Manga, which holds it, is still refused "already a directory of".
  - `scan --add /m` with no `--library` over Manga's `/m` is still refused (CLI test).
  - New library over Manga's `/m` links it and names its own library as the add does.
  - O246's member check is not asked of a link.
  - The list: `libraries_json` lists `/m` under both libraries, with `libraries` naming both. `nesting_json` names both libraries of a nested directory held by two. Split editions name both libraries of a member's directory.
  - Task 2's mutation (`dirs_of` on `libraries.first()`) now goes red.
- [ ] **Step 3.** Reorder `beside`: W against every directory the target library links first, then the equal path. An equal path in another library becomes a link in `add_root_checked_in`, inside the same transaction as O204's comparison. The link inserts one `library_root` row and touches nothing else.
- [ ] **Step 4.** The answers and the CLI's `--add` line ("linked `/m` into Kids; Manga holds it too, and its settings stay").
- [ ] **Step 5.** Run the three crates, clippy and fmt.
- [ ] **Watched mutations, at least:** `beside` unreordered (the lower-id W test goes red); a link that runs `probed` (the interval test goes red); a link that writes the request's settings (the settings test goes red); `libraries_json` listing only the lowest link's library.
- [ ] **Commit:** "Adding a directory another library holds links it".

### Task 4: Remove from this library unlinks, and a removal names every library it empties

**Files:**
- Modify: `crates/taiyaku-library/src/scan.rs` (a new `unlink` and its rehearsal; `emptied_by_removing`, `apply_removal`'s `left_with_no_library`, `apply_library_removal`, `Removal`); `crates/taiyaku-serve/src/libraries_api.rs` (a new `unlink_dir`; `delete_library`), `config_api.rs` (`delete_root`'s answer), `routes.rs` (the new route in `guarded_routes`, `grant_postures` and `narrowing_postures`).
- Tests: `scan/tests.rs`, `libraries_api/tests.rs` (library removal ~787–970), `config_api/tests.rs`, `auth/tests.rs` (the two enumeration tests ~1645 and ~3020 pick the route up), `libraries_api/tests.rs`' per-route reader and nobody tests (~191–256) and `an_api_key_reaches_no_libraries_route` (~1796).

**Interfaces:**
- Consumes: Task 3's links built by the checked add.
- Produces: `scan::unlink(conn, root_id, library_id, dry_run: bool) -> Result<Unlinked, …>`. `Unlinked` has `relocate::Moved`'s sight fields (`loses_sight`, `library_removed`, `grants_removed`, `left_with_no_library`). It refuses a directory the library does not link, and a directory's last link. The last-link refusal's text: "`/m` is in no other library, so it can only be removed: use Remove this directory."
- Produces: `Removal.libraries_removed: Vec<i64>`. `left_with_no_library` is the limited accounts whose every grant is among the emptied libraries.
- Produces, routes: `DELETE /api/library/libraries/{id}/dirs/{dir}` (and `?dry_run=true`), administrators only. It answers 409 while the store's job lock is held or a scan is reserved in this library, as `move_dir` does. `DELETE /api/library/roots/{id}`'s answer gains `libraries_removed: [name]`. `DELETE /api/library/libraries/{id}`'s answer gains `kept: [{path, libraries: [name]}]`, the directories it unlinks and leaves to other libraries.

- [ ] **Step 1.** Run the three crates at the parent into `T4-before.log`.
- [ ] **Step 2.** Write the failing tests:
  - Unlinking `/m` from Kids, which Manga also holds, deletes no work, position or note. Kids' accounts lose sight of it and Manga's keep it. The rehearsal says the same and writes nothing.
  - Unlinking a directory's last link is refused with the sentence above, and the store is unchanged.
  - Store survey run 2's case: `/m` is the only directory of A and of B. Removing `/m` names both in `libraries_removed`, and an account granted exactly A and B is in `left_with_no_library`, beside an account granted only A.
  - Removing a library that holds a shared `/m` and an unshared `/n`: `/n` goes with its works, `/m` stays in the other library with its positions, and `kept` names `/m` and that library.
  - `delete_library` while the other library's scan of the shared `/m` runs: refused as O196 is now. Separately, `delete_library` over a library whose shared directory has a finished scan report under the other library keeps that slot and its report. A fixture where no other slot exists passes against clearing every slot.
  - The new route: an administrator unlinks; a reader, a nobody and an API key are refused; a scan reserved in this library and a held job lock each give 409.
- [ ] **Step 3.** Implement. `apply_library_removal` removes the unshared directories before the library row (store survey §B.4 case f), then unlinks the rest. `delete_library` clears the slots of the removed directories only.
- [ ] **Step 4.** Run the three crates, clippy and fmt.
- [ ] **Watched mutations, at least:** `left_with_no_library` asked of each emptied library in turn (run 2's account goes missing); `delete_library` clearing every directory's slot; an unlink allowed on the last link; the new route left out of `narrowing_postures` (the enumeration test goes red).
- [ ] **Commit:** "Remove from this library unlinks, and a removal names every library it empties".

### Task 5: Move names the library it moves from, and a directory scan the library it was started from

**Files:**
- Modify: `crates/taiyaku-library/src/relocate.rs` (`dir`, `resolve`, `move_root`, `apply_move`, `sighted`), `crates/taiyaku-serve/src/libraries_api.rs` (`move_dir`: its body, `held_up`, its doc's reason for the job lock), `config_api.rs` (`start_scan`), `libraries_api.rs` (`scan_status_json`).
- Tests: `relocate/tests.rs` (`a_dir_or_a_library_that_is_not_there_is_missing_and_its_own_library_is_a_mistake` ~117: "already in" becomes "already in the source", and the target-holds case is new); `libraries_api/tests.rs` move tests (~2849–3388) and scan status (~3924–4076); `config_api/tests.rs`.

**Interfaces:**
- Consumes: Task 3's links, Task 4's `Unlinked` shape (the move's sight fields stay `relocate::Moved`'s).
- Produces: `relocate::move_root` takes the source library. It refuses a source that does not link the directory ("`/m` is not in Manga"), a target equal to the source (as now), and a target that already links it ("`/m` is already in Kids", decision 25). It writes by UPDATE of the source link.
- Produces, routes: `POST /api/library/roots/{id}/move` requires `from: <library id>` in its body, and answers 400 without it. `POST /api/library/roots/{id}/scan` takes an optional `library: <library id>`, which must link the directory (404 otherwise, as the unlink and Move answer it: spec decision 30, which replaced this plan's 400). Without it, a directory with one link reserves under that library, as now, and a shared one under none (decision 27).

- [ ] **Step 1.** Run the three crates at the parent into `T5-before.log`.
- [ ] **Step 2.** Write the failing tests:
  - Moving shared `/m` from Kids to Lib3: Manga still holds it, Kids no longer does, and `loses_sight` names Kids' accounts who see it through no other library.
  - Moving `/m` from Kids to Manga, which holds it, is refused with decision 25's sentence, and nothing is written. Without the refusal the UPDATE fails on the link's key, so the test asserts the sentence, not just an error.
  - A move with no `from` is a 400. A move whose `from` does not link the directory is refused.
  - `held_up` asks about the source's and the target's scans. A scan reserved in Manga does not hold a move of shared `/m` from Kids to Lib3.
  - Scanning shared `/m` with `library: Manga` reserves under Manga, so Kids' scan status says `library: "Manga"`. Without `library`, a shared directory reserves under none, and an unshared one under its one library.
- [ ] **Step 3.** Implement. Correct `move_dir`'s doc: the derive reads no library since PR 1, so name what the job lock serialises now, and quote in the report the code that reason rests on. Keep taking the lock. If no reason holds, stop and report, rather than dropping the lock.
- [ ] **Step 4.** Run the three crates, clippy and fmt.
- [ ] **Watched mutations, at least:** `from` defaulted to the lowest link; the target-holds refusal removed (the sentence test goes red); `start_scan` ignoring `library`.
- [ ] **Commit:** "Move names the library it moves from".

### Task 6: The web keys a directory by its library, and lists every library holding it

**Files:**
- Modify: `web/src/libraries.ts` (`LibraryDir.libraries`, required; `NestedDir`/`AroundDir` `libraries`), `splitEditions.ts` (`MemberDirectory.libraries`), `Libraries.tsx` (~587 `key`, ~342–375 `dirRemoved`, ~594–596 `others`), `DirPage.tsx` (~260 `key`, ~186–233 `Nesting`), `LibraryPage.tsx` (~229–247 `scansInside`; the comment at ~984–986 gives the reason that holds: a library's own directories are never the same or nested), `SplitEditions.tsx` (~41–55 `DirLink`).
- Tests: `web/tests/support/libraries.ts` (`dir()`, `manga()`), the local `LibraryDir` literals serve survey §I lists, the `library:` sites on nested, around and member literals, and a new browser test.

**Interfaces:**
- Consumes: Task 3's JSON (`libraries` per directory, plural nesting names, `MemberDirectory.libraries`), Task 4's `libraries_removed`.
- Produces: `dirRemoved(libraryId, path, how: 'unlinked' | 'removed', emptied: string[])`. It drops the path from this library alone, or from every library, and drops each library the answer says it emptied.

- [ ] **Step 1.** Run the jsdom and browser projects at the parent into `T6-before-{jsdom,browser}.log`.
- [ ] **Step 2.** Write the failing tests:
  - A browser test that opens Manga's page of `/m`, opens Remove…, then follows the "Also in" link (or the address) to Kids' page of `/m`. The dialog is gone, and Kids' page is drawn fresh. Both pages show the same root id, so the fixture must give the two libraries one root id. Watch it red against `key={root.path}`.
  - `dirRemoved` after Remove directory drops `/m` from both libraries' lists and drops a library the answer says it emptied. After an unlink it drops `/m` from this library alone.
  - Move's target list leaves out a library that already holds the directory (decision 25).
  - A nested directory held by two libraries names both in the nesting lines. Split editions link a member's directory once, under the first library the edition lists (decision 29, ruled by the user on 2026-11-07 over a link under each library).
- [ ] **Step 3.** Implement. Key `DirPage` and `RootRow` by library and path (`` `${library.id}:${root.path}` ``): the path half stays because a freed `root.id` is reissued.
- [ ] **Step 4.** Run node, jsdom and browser, `tsc -b` and biome.
- [ ] **Watched mutations, at least:** the key by path alone; `dirRemoved` filtering the page's library only after a removal; `others` including holders.
- [ ] **Commit:** "The web keys a directory by its library".

### Task 7: A shared directory's page says who else holds it, and offers both removals

**Files:**
- Modify: `web/src/RootRow.tsx` (the header facts; `RemovalSection` ~912–948; Scan directory sends the page's library), `RemoveRoot.tsx` (the last-of sentence from the dry run's `libraries_removed`, naming every library it empties), a new `UnlinkRoot.tsx` (or a mode of `RemoveRoot`: the implementer picks the one that repeats less), `MoveRoot.tsx` (`from`), `DirPage.tsx` (~276 `lastOf` goes), `Libraries.tsx` ("No such directory" ~700–703 adds "or removed from this library"), `web/src/api.ts` or wherever `moveDir`, `moveCost`, `removeRoot` and `startScan` live.
- Tests: `DirPage.test.tsx`, `RootRow.test.tsx`, `RemoveRoot.test.tsx`, `MoveRoot.test.tsx` (its `toHaveBeenCalledWith` ~362 gains `from`), `heldFocus.browser.test.tsx` for the new button.

**Interfaces:**
- Consumes: Task 4's unlink route and answers, Task 5's `from` and the scan's `library`, Task 6's `libraries` and `dirRemoved`.

- [ ] **Step 1.** Run jsdom and browser at the parent into `T7-before-{jsdom,browser}.log`.
- [ ] **Step 2.** Write the failing tests:
  - A directory in one library: the page is as it was. One Removal row, no "Also in" line.
  - Shared `/m` on Manga's page: the header line "Also in Kids, which shares these settings and scans.", with Kids linking to Kids' page of `/m`. Two rows, with decision 24's words.
  - Remove from Manga…: its confirm follows the ruled removal layout (the warning sentence, then the help text, then [Remove from Manga] [Cancel] side by side on one line, a clear gap, then the disclosure). It says who loses sight, and whether Manga goes, as Move's confirm does. The last-link refusal is drawn as a callout and names Remove this directory.
  - Remove this directory…: the confirm names every library that holds it, and every library it empties, from the dry run.
  - Holds: Remove from Manga… is held exactly when Move… is (`moveHeld`). Remove this directory… stays on `held_removals`. A held button keeps focus (`HeldButton`).
  - Move sends `from`. Scan directory sends the page's library.
- [ ] **Step 3.** Implement.
- [ ] **Step 4.** Run node, jsdom and browser, `tsc -b` and biome.
- [ ] **Watched mutations, at least:** the two rows drawn for an unshared directory; Remove from Manga… held on `held_removals` instead of `moveHeld`; `lastOf` still taken from this library's list.
- [ ] **Commit:** "A shared directory's page offers Remove from this library".

### Task 8: Adding, New library, a library's card and Remove library say what a link does

**Files:**
- Modify: the add form and New library in `web/src/Libraries.tsx` (and its `NewLibrary` part), `LibraryPage.tsx` (the card line ~946–951 and its choice ~710–712), `RemoveLibrary.tsx` (~148–158).
- Tests: `Libraries.test.tsx` ("making one" ~1649, the "It may sit inside" pin), `LibraryPage.test.tsx`, `RemoveLibrary.test.tsx`, `Config.browser.test.tsx` (its card-line cases).

**Interfaces:**
- Consumes: Task 3's `also_in`, Task 4's `kept`, Task 5's scan status.

- [ ] **Step 1.** Run jsdom and browser at the parent into `T8-before-{jsdom,browser}.log`.
- [ ] **Step 2.** Write the failing tests:
  - Adding `/m` to Kids, which Manga holds: the line "`/m` is also a directory of Kids. It keeps the settings it has there." (the library named is the one in `also_in`) is drawn where the picker was, and no watch line is drawn. Nothing below it moves (the 2026-10-21 callout ruling).
  - New library's help reads decision 26's sentence.
  - Kids' card while Manga's scan, started from Manga's page, walks shared `/m`: "Manga is scanning `/m`, which Kids holds too." Decision 20's nested line is unchanged.
  - Remove library over a library holding shared `/m`: "…and leaves `/m` to Kids."
- [ ] **Step 3.** Implement.
- [ ] **Step 4.** Run node, jsdom and browser, `tsc -b` and biome.
- [ ] **Watched mutations, at least:** the link line drawn for a new directory; the card line taken from this library's own scan; `kept` not read.
- [ ] **Commit:** "Adding, New library and Remove library say what a link does".

### Task 9: The docs

**Files:**
- Every sentence serve survey §G lists: `docs/` outside `docs/superpowers/` and `docs/notes.md`'s history, `README.md`, in-app help and CLI help that says a directory belongs to one library or that the same directory is refused.
- `docs/status.md`:
  - The libraries summary: "(its PRs 1 and 2)" becomes "(its PRs 1, 2 and 3)", and it says a directory may be in several libraries.
  - The Later entry "Libraries whose directories overlap": PR 2 merged as #226, and PR 3 is this.
  - O47's row: `--add PATH --library Other` of a held path now links (exit 0). It stays fixed: one `root` row, one walk.
  - O248's row: an unlink has the same blind spot as Move's sight lists.
  - Open a row for anything the tasks' reports opened.
- Notes, pointing at the overlapping-libraries spec §5, at the sentences spec §6 lists as reversed by PR 3: `2026-10-05-libraries-design.md` §10 (~650, ~655) and §2 (~227).
- The overlapping-libraries spec §5 gains a paragraph listing what this plan settled. Copy the bullets from this plan's header.

- [ ] **Step 1.** Edit. Then run the status sweep (`grep -c '^| O[0-9]'`, the closed count, "What remains"). Grep every O-number touched, bare, across `docs/` and `web/src/`, and read every hit.
- [ ] **Step 2.** Run the full gate, `scripts/check-all.sh`, into `$E/gate-1.log`. Do not pipe it. Quote its `all green` line and the exit status.
- [ ] **Step 3.** Commit: "The docs say a directory may be in several libraries".

### Finish (the controller)

1. **The live server for the user** (spec §7), following the memory note "UI work: prototype on a live server": a scratch copy of the test library with `/m` in two libraries. The user tries the directory page, Remove from this library, Remove this directory, Move, the add's link line and Remove library before the review. Their changes become a task.
2. **The whole-branch review.** Run it against main with Opus, in lanes (store, serve, web). Give the reviewers this plan, the spec §5 and both surveys. Ask in particular for any site that still treats a directory as having one library, for any writer that can leave a directory with no link, and for any removal or slot clearing that reaches a directory another library keeps.
3. **One fix wave.** The fix commits are reviewed too.
4. **The three crates in `rust:1-bookworm`** with SQLite 3.40.1, and the full gate on the final commit, with the test counts in `docs/status.md` from its JUnit report.
5. **A drive on a scratch copy of the test library**, following the memory note "Verifying against real material":
   - Add a series directory of Manga's to a new library Kids, through the add.
   - Read as an account granted Manga and as one granted Kids: sidebar, library pages, series page, reader, OPDS, downloads.
   - Keep a position and a note. Unlink the directory from Kids, and check that the position and the note held and nothing was counted removed.
   - Link it again, Move it from Kids to a third library, then remove it from every library.
   - Record the user-agent and the trace of each act.
6. **The claim check** on this plan's claims, the status rows and the PR body, with passes after each correction.
7. **The PR body.** Its first line is the title. It names O225's row update. Add no attribution. Ask the user before pushing.
