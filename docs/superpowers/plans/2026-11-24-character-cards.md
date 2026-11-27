# Character Cards Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** In the book reader's look-up mode, a tap on a character's name shows that character's card above the dictionary entry, built from haiyaku 0.2's data in taiyaku's own database and cut at the last chapter the reader has finished.

**Architecture:** `taiyaku-library` takes haiyaku 0.2 from crates.io with no default features, runs haiyaku's migrate after its own on the shared connection (a failure switches cards off and never stops the open), and gains a `characters` module that chooses the scope, folds with `haiyaku::cast_at`, narrows a series fold to the book when the caller cannot open a book it folded, runs `haiyaku::characters_named` and builds taiyaku's own JSON answer. `taiyaku-serve` adds `GET /api/works/{work}/{edition}/book/characters` beside the book's image route and marks each book edition that has character data on the book's answer. The web reader sends the characters request beside the dictionary's, holds the panel's first draw for both, and draws plain cards above the entry; the owner settles the look on a live prototype, which lands as its own commit and is then held by tests.

**Tech Stack:** Rust (toolchain 1.100.0, `rust-version` 1.91), rusqlite 0.43 bundled, haiyaku 0.2.0, axum 0.8, serde; React 19 and TypeScript, Vitest's `node`, `jsdom` and `browser` projects, biome.

**Spec:** `docs/superpowers/specs/2026-11-23-character-cards-design.md` (approved; its §5 owner decisions and §12 calls are settled and nothing here reopens them). Every executor reads it before its task.

**Worktree:** `/home/davey/projects/taiyaku/.claude/worktrees/haiyaku-cards` (below `$W`), branch `haiyaku-cards`, the design at `22f47174` on main `01b6d0ba`. `CLAUDE.md` is not in the worktree: read `/home/davey/projects/taiyaku/CLAUDE.md`. **Evidence:** `/home/davey/projects/taiyaku/.superpowers/sdd/haiyaku-cards-evidence/` (below `$EV`), in the main checkout and gitignored. Every log, mutation script and report a task names goes there, written by Bash (`>|`), never inside `$W`. It does not exist on 2026-11-24: the controller creates it (`mkdir -p`) before the first dispatch.

## haiyaku 0.2.0, as this plan uses it

Read from `/home/davey/temp/haiyaku-wt/main-ref/crates/haiyaku` (commit `0d995a4`, version 0.2.0); Task 1 checks the published crate against it. With `default-features = false` the crate builds `cast`, `fold`, `log`, `lookup` and `store` (its sidecar functions included) and nothing behind `extract`; its only dependencies are rusqlite 0.43 (feature `backup`) and sha2 0.11, both already locked.

| Item | Signature or shape |
| --- | --- |
| migrate | `haiyaku::store::migrate(conn: &Connection) -> haiyaku::Result<()>`; `Error::SchemaTooNew { found, supported }` when a later haiyaku wrote the file; never reads or writes `meta` or `user_version` |
| version | `haiyaku::store::schema_version(conn) -> haiyaku::Result<Option<u32>>` (`None`: never migrated); `haiyaku::store::SCHEMA_VERSION: u32 = 2` |
| holding | `haiyaku::store::series_holding(conn, book: &str) -> Result<Vec<Holding>>`, `Holding { series: String, orders: Vec<String> }`: series by name, only the orders holding the book, by rank; `Err(NotFound("book <sha>"))` for a book not stored, `Ok(vec![])` for one in no series |
| fold | `haiyaku::cast_at(conn, book: &str, document: &str, scope: &Scope) -> Result<Cast>`; `Scope::Book` or `Scope::Series { series: String, order: Option<String> }`; `NotFound` for an unknown book, an unknown document, a series or order not stored, and an order without the book |
| look-up | `haiyaku::characters_named<'a>(cast: &'a Cast, text: &str) -> Vec<Mention<'a>>`; `Mention { character: &Character, name: &Name, part: NamePart, len: usize }` with `len` in **bytes** of `text`; `NamePart::{Whole, Given, Surname}`; both `#[non_exhaustive]` |
| cast | `fold::Cast { characters, relations, books: Vec<String> }`; `Character { key: EntityKey, name: Name, aliases: Vec<Name>, first: ChapterRef, last: ChapterRef, merged }`; `Relation { a: EntityKey, b: EntityKey, facts: Vec<Fact> }`; `Fact { kind: RelationKind, from: Option<EntityKey>, note, quote, first, last, overturned: Option<Overturn> }`; `Overturn { at: ChapterRef, note, quote }`; `ChapterRef { book: String /* sha256 */, chapter: u32 }` carries no title. All but `ChapterRef` and `EntityKey` are `#[non_exhaustive]`: read only |
| names | `log::Name { text, given, surname, title: Option<String>, title_sex: Option<Sex> }`; `Sex::{Male, Female}` (its `as_str` is crate-private); `RelationKind::as_str()` is public (`"family"`, `"mentor"`, …), and `directed()` is true for `Mentor`, `Serves` and `Owns` |
| a log | `store::read_book(conn, sha256) -> Result<BookLog>`; `BookLog { book: BookRecord { sha256, title, language, backend, model, prompt_version, extracted_at }, chapters: Vec<Chapter { number, toc_entry, title, seconds, cost_usd, kept, rejected }>, documents: Vec<Document { ordinal, path, chapter: Option<u32> }>, entities: Vec<Entity { id, kind: EntityKind::Character }>, events: Vec<Event { seq, chapter, at: Position { document, block }, quote, body: EventBody }> }`; `EventBody::{Introduced, Named, Alias, Appeared, Relation, SameAs, Overturned}` |
| writes | `store::write_book(conn: &mut Connection, log: &BookLog) -> Result<()>`; `store::put_series(conn: &mut Connection, series: &Series) -> Result<u32>` with `Series { name, title, orders: Vec<Order { name, books: Vec<String> }> }`. Both need `PRAGMA foreign_keys` on (taiyaku's `configure` sets it) and a connection outside a transaction (`&mut *library.conn()` is one) |
| hash | `store::sha256_file(path) -> Result<String>`, lowercase hex: the digest a member's `sha256` holds |

**An invented haiyaku book needs no fixture file.** None of `BookLog`, `BookRecord`, `Chapter`, `Document`, `Entity`, `Event`, `EventBody`, `Name`, `Position`, `Series` or `Order` is `#[non_exhaustive]`, every field is public, and `write_book` and `put_series` are public without `extract`. A test builds an invented EPUB (Task 3's `epub_documents`), takes its digest (`haiyaku::store::sha256_file` in `taiyaku-library`, `crate::test_support::sha256_of` in `taiyaku-serve`) and stores a `BookLog` built in code whose `book.sha256` is that digest and whose `documents[].path` are the EPUB's spine paths as `taiyaku_read` resolves them (`OEBPS/c1.xhtml`). `write_book` refuses anything but 64 lowercase hex digits and checks the log's chapter and document rules, so a malformed fixture fails at once. A test inside `taiyaku-library` that never reaches a block needs no EPUB at all: any 64-hex string is a book id. `taiyaku-serve` never names a haiyaku type: it stores logs and builds the invented library through `taiyaku_library::test_support` (Task 2's logs, Task 3's `keepers`), so its manifest gains no haiyaku line. FLAG 1.

## Decisions the design left to the builder

- **D1, the route:** `GET /api/works/{work}/{edition}/book/characters?block=…&text=…&earlier=0|1[&before=…][&series=…][&order=…]`, beside `/api/works/{work}/{edition}/book/images/{index}`. The book's answer carries every book edition the account sees and a block index counts one edition's blocks, so the edition is in the path as it is in the image route's. FLAG 2. `before` is the block's text before the tap, at most 64 UTF-16 units (`LONGEST_BEFORE`) and refused with 400 past that, from which the server chooses where the name starts (design §7).
- **D2, the flag rides the book's answer:** `book::get_book` builds its document per request (only parsed EPUBs are cached, by digest, in `book::read`), so a fill reaches the next open. An edition with character data carries `"characters": true`; one without carries no key, as `marks` and `figures` are left out when empty. FLAG 7.
- **D3, the fold's connection:** the shared one (`state.library.conn()`) in Task 5. Task 8 measures, and FLAG 10's rule decides whether it moves to `Library::working()` and whether a fold cache is filed.
- **D4, chapter and book titles:** read once per book with `store::read_book` into a cache the `Library` owns, never from `haiyaku_chapter` by SQL. FLAG 4.
- **D5, checked:** the client's `Block.text` at index n is the server's block n: `book::document` sends `read.book.blocks` in order with `text: b.text`, and `lookUpTapped` reads `edition.blocks[point.block]`. So `length`, in UTF-16 units of `text`, lights the right characters.
- **D6, the CLI writing while the server runs:** tried on the prototype's scratch copy (Task 8) and written down (Task 10); the how-to says to stop the server either way (FLAG 14).
- **D7, tests that list tables:** `schema/tests.rs`' shape comparisons build raw `schema::migrate` databases and never meet haiyaku's tables. `scan/tests.rs::every_row_but_the_libraries` and `libraries_api/tests.rs::the_store` dump every table of a `Library` and will now include haiyaku's, empty on both sides of each comparison. Task 1 runs all three Rust crates that open a library and reports any other.
- **D8, the backup:** Task 1, FLAG 3.

## Global Constraints

- `haiyaku = { version = "0.2", default-features = false }` in `crates/taiyaku-library/Cargo.toml` alone, from crates.io, never a path or git dependency. The manifests of `taiyaku-core`, `taiyaku-wasm` and `taiyaku-serve` do not change.
- Production code reaches haiyaku's tables only through haiyaku's public API: no SQL naming a `haiyaku_` table in taiyaku outside tests. A test may write `haiyaku_schema` to plant a version, or drop a haiyaku table to make a read fail.
- taiyaku's `SCHEMA_VERSION` stays 32 and no migration file is added.
- Nothing haiyaku does stops `Library::open`, `open_in_memory` or a book from opening. With cards off nothing reads haiyaku's tables: the book's answer carries no flag and the characters route answers `known: false` without a haiyaku call.
- The cut is haiyaku's: taiyaku hands `cast_at` the tapped block's document and cuts nothing itself.
- Fixtures are invented (Task 2's books). No text of the corpus or of the test library goes into the repository, a test, a comment, a commit or a PR; prototype data stays in the controller's scratchpad.
- The JSON field names are Task 2's table, in Rust and in `web/src/book.ts` alike.
- Every new test is watched failing against the defect its step names, by a hand-applied mutation or with the code absent; the report quotes the failing line, and `git -C $W diff` shows the mutation gone afterwards. A test that asserts several things has each assertion broken by a mutation the earlier ones tolerate.
- Comments say why only (`/home/davey/projects/taiyaku/CLAUDE.md`); `env -C $W node scripts/check-comments.mjs --base origin/main` passes.
- Rust runs are throttled and build into the worktree's own target: `env -C $W CARGO_TARGET_DIR=$W/target NEXTEST_TEST_THREADS=8 RUST_TEST_THREADS=8 cargo nextest run -p <crate> >| $EV/<name>.log 2>&1; echo "exit=$?" >> $EV/<name>.log`, then the summary lines read from the log. No test or gate command is piped through `tail`. Clippy and fmt as the gate runs them: `cargo clippy --workspace --all-targets --all-features -- -D warnings`, `cargo fmt --all -- --check`.
- The Rust per-file coverage floor (`node scripts/coverage-floor.mjs target/rust-coverage.json`, which `scripts/check-all.sh` runs) applies to the two new `characters.rs` files as to every other. No task's own runs measure it and Finishing's gate is the first that does, so Tasks 2, 3 and 5 test every branch of those files rather than leave one to the sweep.
- Web: tests live in `web/tests/` (`*.test.ts` node, `*.test.tsx` jsdom, `*.browser.test.tsx` browser); components are mounted with `mount()` from `web/tests/support/mount.ts`, with `act()` from `react` and `land()` from `web/tests/support/async.ts`, as the existing tests import them, never `@testing-library`; tests are plain `async`/`await`, never `.then()` off `act`. Whole projects run (`pnpm -C $W/web run test:coverage`), then `pnpm -C $W/web typecheck` and `env -C $W node scripts/coverage-floor.mjs` (a task that adds a `web/src` file meets its per-file floor). One lane at a time runs the browser project. `web/node_modules` in the worktree is a real install (`pnpm -C $W/web install --frozen-lockfile`), with `crates/taiyaku-wasm/pkg` copied from the main checkout when absent.
- Shell: no `cd`; `git -C`, `env -C` and absolute paths; zsh noclobber (`>|`).
- Tasks commit locally, never push, never `--amend`, and add no attribution line. Docs name no branch commit; dates are the owner's Pacific day. Open-items rows are this lane's O540 to O549.

## Review Focus

1. **A tap whose 128-unit cut would split a character outside the BMP.** `encodeURIComponent` throws on a lone surrogate, so a naive cut loses the look-up; the client cuts to 127 units there. Task 6, `charactersText cuts at 128 units and never inside a pair` and `cuts the text at 128 units without splitting a character outside the BMP`.
2. **A tap on a spine item haiyaku skipped (an afterword).** The reader expects the dictionary as today, not an error: `known: true`, no card. Task 2, `a_document_haiyaku_skipped_is_known_with_no_characters`; Task 5, `the_skipped_afterword_is_known_with_no_characters`.
3. **A work with two book editions, one with character data.** A tap in the other edition sends nothing; the flag is per edition. Task 4, `the_book_says_which_editions_have_character_data`; Task 6, `asks nothing of a book edition without character data`.
4. **A second tap before the first's characters answer.** The first answer never draws on the second tap's panel. Task 7, `a later tap's panel never draws an earlier tap's cards`.
5. **haiyaku's schema newer than the build, on a book that has data.** The library opens, the book reads, and the panel shows the dictionary alone. Task 1, `a_haiyaku_schema_newer_than_this_build_leaves_the_library_open_and_cards_off`; Task 4, `with_cards_off_the_book_sends_no_flag`; Task 5, `with_cards_off_the_route_reads_nothing`.
6. **haiyaku failing under the library, not only its schema.** A haiyaku backup that cannot be taken at open, or a haiyaku read that fails while the book's answer is built, leaves the library open and the book readable, with cards off or the flag absent and a warning on stderr. Task 1, `a_haiyaku_backup_that_cannot_be_taken_leaves_the_library_open_and_cards_off`; Task 4, `a_haiyaku_read_that_fails_sends_no_flag_and_the_book_still_opens`.

## File Structure

| File | Change |
| --- | --- |
| `crates/taiyaku-library/Cargo.toml`, `Cargo.lock` | the haiyaku dependency (Task 1); `test-support` enabling `taiyaku-read/test-support` (Task 3) |
| `crates/taiyaku-library/src/lib.rs` | haiyaku's migrate at open, the haiyaku backup, `cards_off`, `titles`; `pub mod characters` |
| `crates/taiyaku-library/src/backup.rs` | `Before` and the `-before-haiyaku-v<N>` name |
| `crates/taiyaku-library/src/characters.rs` (new), `characters/tests.rs` (new) | the answer types, `Titles`, `holds`, `choose_scope`, `books_seen`, `answer` |
| `crates/taiyaku-library/src/test_support.rs` | the invented books and their series (Task 2); `keepers`, the invented library on disk (Task 3) |
| `crates/taiyaku-read/src/test_support.rs` | `epub_documents` |
| `crates/taiyaku-serve/src/book.rs`, `book/tests.rs` | the flag on the book's answer (Task 4); `document_at` (Task 5) |
| `crates/taiyaku-serve/src/characters.rs` (new), `characters/tests.rs` (new) | the route (Task 5) |
| `crates/taiyaku-serve/src/routes.rs`, `lib.rs`, `auth/tests.rs`, `routes/tests.rs`, `fingerprint_pass/tests.rs` | the route's entries in the three tables, the auth tables' counts, the two route walks, the reading routes' list (Task 5) |
| `web/src/Backup.tsx` | the Backup tab's sentence on the backups an open takes (Task 1) |
| `web/src/book.ts` | the answer types, `getCharacters`, `charactersText`, `CHARACTERS_TEXT_UNITS`, `CARDS_WAIT_MS`, `Edition.characters` |
| `web/src/modules/lookup.ts` | `includeEarlierBooks: true` |
| `web/src/BookReader.tsx`, `web/src/BookLookup.tsx`, `web/src/BookCharacters.tsx` (new), `web/src/styles/book.css` | the request, the hold, the cards |
| `web/tests/book.test.ts`, `BookReader.test.tsx`, `BookLookup.test.tsx`, `bookCharacters.browser.test.tsx` (new) | the web's tests |
| `README.md` | the backup name, the skip and the restore passages (Task 10) |
| `docs/notes.md`, `docs/status.md`, `docs/mutants-baseline.md` | Task 10 and Finishing |

---

### Task 0: Gate: haiyaku 0.2.0 on crates.io

**Who:** the controller, before any dispatch.

- [ ] **Step 1: Ask crates.io.** `env -C $W cargo info haiyaku@0.2.0`. Expected: version 0.2.0, licence GPL-3.0-or-later, `rust-version` 1.91, features `bundled`, `cli`, `extract` with `default = ["bundled", "cli"]`. On 2026-11-23 crates.io's `max_version` was 0.1.0.
- [ ] **Step 2: If 0.2.0 is not there, stop.** Tell the owner that haiyaku 0.2.0 is not on crates.io and wait for their word. Nothing in this plan proceeds on a path or git dependency, and no task is dispatched meanwhile.

### Task 1: The dependency, haiyaku's tables at open, cards off, and the backup

**Files:**
- Modify: `crates/taiyaku-library/Cargo.toml` (`[dependencies]`, with a why-comment in the manifest's own style: no default features because the server only reads logs and folds them, and taiyaku's `bundled` gives the one connection its one SQLite), `Cargo.lock`
- Modify: `crates/taiyaku-library/src/lib.rs` (the `Library` struct, `from_connection`, a new `cards_off`)
- Modify: `crates/taiyaku-library/src/backup.rs` (`Backup::before`, `stamp_of`, `name_before_migration`, `stands_for`, `opens_at`, `prune`)
- Modify: `web/src/Backup.tsx` (the help sentence naming the backups an open takes)
- Test: `crates/taiyaku-library/src/schema/tests.rs` (beside the O263 tests of the backup an open takes; find them by `backup.before == Some(SCHEMA_VERSION)`), `crates/taiyaku-library/src/backup/tests.rs`

**Interfaces:**
- Produces: `Library::cards_off(&self) -> Option<&str>`, `None` while cards are on and haiyaku's error as text when off; `pub enum backup::Before { Taiyaku(u32), Haiyaku(u32) }` (`Debug, Clone, Copy, PartialEq, Eq`) with `Backup::before: Option<Before>`; the backup name `library-<stamp>-before-haiyaku-v<N>.db`.

- [ ] **Step 1: Add the dependency.** The manifest line from the Global Constraints, then `env -C $W cargo metadata --format-version 1 >| $EV/t1-metadata.json`. `git -C $W diff --stat Cargo.lock` and the diff itself: exactly one new `[[package]]`, `haiyaku 0.2.0` from the crates.io registry, depending on the `rusqlite` and `sha2` already locked, and `haiyaku` in `taiyaku-library`'s list. Anything more is reported, not accepted.
- [ ] **Step 2: The published source is the source this plan read.** From `$EV/t1-metadata.json` take haiyaku's `manifest_path`; `diff -r <its directory>/src /home/davey/temp/haiyaku-wt/main-ref/crates/haiyaku/src` prints nothing. If it prints anything, stop and report which item of the signature table moved.
- [ ] **Step 3: Write the failing tests.** File libraries as the O263 tests make them; a database "from before haiyaku" is one built by `schema::migrate` alone on a raw connection run through `Library::configure` (private, and reachable from `schema::tests`). Each defect named is the one to apply in Step 6.

  | Test (in `schema/tests.rs`) | Asserts | Defect it is watched failing against |
  | --- | --- | --- |
  | `opening_makes_haiyakus_tables_at_its_version` | in memory and on file: `haiyaku::store::schema_version` is `Some(haiyaku::store::SCHEMA_VERSION)` and `cards_off()` is `None` | the migrate call left out |
  | `reopening_a_library_leaves_haiyakus_tables_as_they_were` | after a second `Library::open`: the version is the same, the `haiyaku_%` tables in `sqlite_master` are the same list, and no backup was taken | a backup on every open (the pending test reading `<=`) |
  | `a_haiyaku_schema_newer_than_this_build_leaves_the_library_open_and_cards_off` | a library with one taiyaku row written through the store's API, then `UPDATE haiyaku_schema SET version = 3`, reopened: `Ok`; `cards_off()` contains `"version 3"` and `"version 2"`; the taiyaku row reads back; the stored version is still 3; no backup | haiyaku's error propagated with `?` (the open fails); `cards_off` never set; a backup taken for a version above the build's |
  | `a_library_from_before_haiyaku_is_backed_up_before_its_tables_are_made` | a database from before haiyaku, opened: one backup in `backup::directory_for(db)`, `before == Some(Before::Haiyaku(2))`, its name ending `-before-haiyaku-v2.db`; that file, opened read-only, has `schema_version == None` while the library has `Some(2)` | no backup; the backup taken after the migrate (it then holds haiyaku's tables); the taiyaku name used |
  | `a_new_library_takes_no_backup_for_haiyaku` | `Library::open` on a path that does not exist: no backup listed | haiyaku's pending test without "taiyaku had a version", which backs up an empty file |
  | `one_backup_before_both_migrations_is_named_for_taiyakus` | taiyaku planted at an older version as the O263 tests plant it (`migrate_with(&conn, &MIGRATIONS[..N])`) with no haiyaku tables, opened: exactly one backup, `Before::Taiyaku(SCHEMA_VERSION)`, and haiyaku at 2 afterwards | one backup for each migration |
  | `a_haiyaku_migration_that_keeps_failing_is_backed_up_once` | taiyaku at 32 and a `haiyaku_schema` table holding 1 with no other haiyaku table (migration 2 then fails on `haiyaku_event`), opened twice the way the O263 restart test opens twice (including what it does about two names in one second): `cards_off` set both times, one `-before-haiyaku-v2` backup | haiyaku's backup not routed through `stands_for` |
  | `a_haiyaku_backup_that_cannot_be_taken_leaves_the_library_open_and_cards_off` | a database from before haiyaku holding one taiyaku row, with a file planted where `backup::directory_for(db)` would be so the backup cannot create its directory, opened: `Ok`; `cards_off()` names the backup that could not be taken; `haiyaku::store::schema_version` is still `None`; the taiyaku row reads back | the backup's error propagated with `?` (the open fails); haiyaku's migrate run after the failed backup (the version is `Some(2)`); `cards_off` left unset |

  In `backup/tests.rs`: `stamp_of` reads `library-20261110T120000Z-before-haiyaku-v2.db` as `Before::Haiyaku(2)`, and the existing refusal list gains `-before-haiyaku-v.db`, `-before-haiyaku-v02.db` and `-before-haiyaku-v2-before-v3.db` (defects: a leading zero accepted; the haiyaku tail read as taiyaku's). `prune(dir, 0)` keeps a `-before-haiyaku-v2` file beside the `-before-v19` one it already keeps (defect: `prune` keeping only `Before::Taiyaku`).

- [ ] **Step 4: Run them and see them fail.** `cargo nextest run -p taiyaku-library` into `$EV/t1-red.log`: every new test red, for its own reason.
- [ ] **Step 5: Implement.**
  - `from_connection`, after `configure`: taiyaku's pending test as now. haiyaku's is pending when taiyaku's `schema::version` is above 0 (a library with data) and `haiyaku::store::schema_version` is `Ok(v)` with `v.unwrap_or(0) < haiyaku::store::SCHEMA_VERSION`; an `Err` is not pending (the migrate will fail and switch cards off). With a path: if taiyaku has pending migrations, its own backup, as today (skipped where `stands_for` finds one that stands; its failure refuses the open, O263); else if haiyaku is pending, the haiyaku backup, `-before-haiyaku-v<N>`, skipped where `stands_for(…, Before::Haiyaku(SCHEMA_VERSION), …)` finds one that stands. A haiyaku backup that fails does not refuse the open: haiyaku's migrate is skipped and the backup's error is the reason cards are off. Then `schema::migrate(&conn)?`, then, unless the haiyaku backup failed, `haiyaku::store::migrate(&conn)`: on `Err(e)` the `Library` keeps `e.to_string()` as the reason cards are off. Whenever cards go off, one warning line, `character cards are off: <reason>`, goes to stderr with `writeln!` (`eprintln!` panics with nothing reading stderr; `scan.rs` writes its lines the same way).
  - `backup.rs`: `Before`; a constant `BEFORE_HAIYAKU = "-before-haiyaku-v"` beside `BEFORE`; `stamp_of` reads both tails with `version_of`'s one spelling; `name_before_migration` takes a `Before`; `stands_for` takes the `Before` and the current version (haiyaku's `None` as 0) and `opens_at` checks the version that `Before` names (`schema::version`, or `haiyaku::store::schema_version`); `prune` keeps every `Some(_)`. The existing `before == Some(N)` sites become `Some(Before::Taiyaku(N))`.
  - `web/src/Backup.tsx`: the help sentence that says an open's own backup ends in `-before-v` and the schema version it migrates to also names `-before-haiyaku-v` and haiyaku's schema version, which `--keep` leaves alone as well. No web test names that sentence (checked 2026-11-24); `pnpm -C $W/web exec vitest run --project jsdom tests/Backup.test.tsx` and `pnpm -C $W/web typecheck` stay green.
- [ ] **Step 6: Green, then watch each defect.** The crate green into `$EV/t1-green.log`; each defect of Step 3 applied by hand and watched red at its own assertion, the failing lines quoted in `$EV/t1-mutations.md`; the mutation reverted.
- [ ] **Step 7: The crates that open a library.** `cargo nextest run -p taiyaku-serve` and `-p taiyaku-cli` into `$EV/t1-serve.log` and `$EV/t1-cli.log`, green. Report each test that now meets haiyaku's tables (D7) and why it is unchanged or what changed.
- [ ] **Step 8: Clippy, fmt, the comment check.** All clean.
- [ ] **Step 9: Commit.** "library: haiyaku's tables beside taiyaku's, cards off when its schema is unknown, a backup before its first migration".

### Task 2: The answer for the book alone

**Files:**
- Create: `crates/taiyaku-library/src/characters.rs`, `crates/taiyaku-library/src/characters/tests.rs`
- Modify: `crates/taiyaku-library/src/lib.rs` (`pub mod characters;`; `Library` owns a `characters::Titles` and answers it from `pub fn titles(&self) -> &characters::Titles`)
- Modify: `crates/taiyaku-library/src/test_support.rs` (the invented books)

**Interfaces:**
- Consumes: `Library::cards_off` (Task 1).
- Produces:
  - `pub struct Asked<'a> { pub book: &'a str, pub document: &'a str, pub text: &'a str, pub earlier: bool, pub series: Option<&'a str>, pub order: Option<&'a str> }`
  - `pub fn answer(conn: &Connection, asked: &Asked<'_>, access: &Access, titles: &Titles) -> Result<Answer, String>`; this task folds `Scope::Book` whatever `earlier` says and names the parameter `_access`, and Task 3 adds the rest and reads it
  - `pub fn holds(conn: &Connection, sha256: &str) -> Result<bool, String>`: `series_holding` `Ok` is true, `NotFound` false, any other error `Err`
  - `pub const LONGEST_TEXT: usize = 128`, in UTF-16 units
  - `pub struct Titles` (`Debug`, `Default`): `pub(crate) fn of(&self, conn: &Connection, sha256: &str) -> Result<Arc<BookTitles>, String>` reads `store::read_book` once per book and keeps the title and the chapter titles; a `#[cfg(test)] fn reads(&self) -> usize` counts the `read_book` calls
  - the answer types below, `#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]` and `#[serde(rename_all = "camelCase")]`, and `Answer::unknown()`
  - in `taiyaku_library::test_support`: `LAMP_KEEPERS`, `SECOND_LAMP`, `LAMP_ALONE` (each `&[(&str, &[&str])]`: a spine document's path and its paragraphs) and `store_lamp_keepers`, `store_second_lamp`, `store_lamp_alone`, `store_names_abroad` (each `fn(conn: &mut Connection, sha256: &str)`), `store_keepers_series(conn: &mut Connection, keepers: &str, second: &str)`; each panics with the haiyaku error if a write is refused

**The answer.** Fixed here, mirrored in `web/src/book.ts` by Task 6:

```text
Answer       { known: bool,
               scope: { series: string, order: string } | null,  // ScopeUsed; null for the book alone
               narrowed: "not-visible" | null,                    // why a series fold fell back
               length?: number,                                   // UTF-16 units; absent when no one matched
               characters: Card[] }                               // characters_named's order
Card         { name: CardName, aliases: CardName[], matched: Matched,
               first: ChapterAt, last: ChapterAt, relations: CardRelation[] }
CardName     { text: string, given: string | null, surname: string | null,
               title: string | null, titleSex: "male" | "female" | null }
Matched      { text: string,                                      // Mention::name.text
               part: "whole" | "given" | "surname" | "other" }
ChapterAt    { book: string,                                      // haiyaku's title of ChapterRef.book
               chapter: number,
               chapterTitle: string }                             // "" for a number the log lacks
CardRelation { with: CardName,                                    // the other character's shown name
               facts: CardFact[] }
CardFact     { kind: string,                                      // RelationKind::as_str
               from: "this" | "with" | null,                      // who a directed fact runs from
               note: string, quote: string, first: ChapterAt, last: ChapterAt,
               overturned: { note: string, quote: string, at: ChapterAt } | null }   // CardOverturn
```

Rust names: `Answer`, `ScopeUsed`, `Card`, `CardName`, `Matched`, `ChapterAt`, `CardRelation`, `CardFact`, `CardOverturn`. `length` is the one key skipped when `None`; every other key is always sent, nulls included. `Answer::unknown()` is `{known: false, scope: null, narrowed: null, characters: []}`. A card's relations are `cast.relations` having the card's key as `a` or `b`, in haiyaku's order. `"other"` stands for a `NamePart` later than 0.2. FLAG 8.

**`answer`'s steps** (Task 3 adds 2 and 4):
1. `series_holding(conn, book)`: `NotFound` answers `Answer::unknown()`; another error is `Err`.
2. The scope (Task 3).
3. `cast_at(conn, book, document, &scope)`: `NotFound` here is a document haiyaku does not hold for a book it does, answered `known: true` with no characters; another error is `Err`.
4. The grant (Task 3).
5. `characters_named(&cast, text)`; `length` is the UTF-16 units of `text[..first.len]` (every mention returned has the same `len`).
6. A card per mention, its `ChapterAt`s from `titles`.

**The invented books.** The plan is the first place they exist. The paragraphs are constants so that Task 3's `keepers` builds EPUBs from the very text the logs quote; every quote is one paragraph, verbatim. Every event sits at block 0 of its chapter's document. Book records: language `en` (D: `ko`), backend `invented`, model `none`, prompt version 1, `extracted_at` `2026-11-24T00:00:00Z`; chapters' `seconds`, `kept` and `rejected` 0, `cost_usd` `None`, `toc_entry` the chapter's number.

**A, "The Lamp Keepers"** (`LAMP_KEEPERS`, `store_lamp_keepers`):

| Spine document | Paragraphs | In the log |
| --- | --- | --- |
| `OEBPS/front.xhtml` | "The Lamp Keepers" | ordinal 0, no chapter |
| `OEBPS/c1.xhtml` | "Ada Reed lit the lamp." / "Ned Reed and Tom Field kept it burning." / "Tom Field showed Ned Reed the wick." | 1, chapter 1 "The Lamp" |
| `OEBPS/c2.xhtml` | "Mara Vell came at dusk." / "Ned Reed knew his cousin at once." / "Mrs. Vell said nothing." | 2, chapter 2 "Dusk" |
| `OEBPS/c3.xhtml` | "Mara Vell came back." / "Ned Reed saw that Tom Field had lied." | 3, chapter 3 "Return" |
| `OEBPS/c4.xhtml` | "The Reeds put out the lamp." | 4, chapter 4 "Out" |
| `OEBPS/notes.xhtml` | "Afterword." | not in the log: a spine item haiyaku skipped |

Its events, `seq` 1 to 10 (¶ numbers the paragraph quoted):
1. ch 1, `Introduced ada` "Ada Reed" (given Ada, surname Reed), c1 ¶1
2. ch 1, `Introduced ned` "Ned Reed" (Ned, Reed), c1 ¶2
3. ch 1, `Introduced tom` "Tom Field" (Tom, Field), c1 ¶2
4. ch 1, `Relation` ned, tom, `Friend`, "they keep the lamp together", c1 ¶2
5. ch 1, `Relation` from tom to ned, `Mentor`, "Tom taught him the wick", c1 ¶3
6. ch 2, `Introduced mara` "Mara Vell" (Mara, Vell), c2 ¶1
7. ch 2, `Alias mara` "Mrs. Vell" (surname Vell, title "Mrs.", title_sex `Female`), c2 ¶3
8. ch 2, `Relation` ned, mara, `Family`, "cousins", c2 ¶2
9. ch 3, `Appeared mara`, c3 ¶1
10. ch 3, `Overturned` ned, tom, `Friend`, "Tom only pretended", c3 ¶2

So Ada is met first and last seen in chapter 1 while Ned is last seen in chapter 3: most recently seen first puts Ned before Ada, which meeting order and name order both reverse. Entity keys sort `mara < ned < tom`, so Ned is `b` of his pair with Mara and `a` of his pair with Tom.

**B, "The Second Lamp"** (`SECOND_LAMP`, `store_second_lamp`): `OEBPS/c1.xhtml` "Wren Hollis found the lamp." (chapter 1 "Wren"), `OEBPS/c2.xhtml` "Ned Reed wrote to Wren Hollis." (chapter 2 "Letters"); one event, ch 1 `Introduced wren` "Wren Hollis" (Wren, Hollis). B has no `ned`: A's Ned reaches a card in B only through a series fold.

**C, "A Lamp Alone"** (`LAMP_ALONE`, `store_lamp_alone`): `OEBPS/c1.xhtml` "Iris Moor waited." (chapter 1 "Waiting"), `OEBPS/c2.xhtml` "Iris Moor left." (chapter 2 "Leaving"); ch 1 `Introduced iris` "Iris Moor" (Iris, Moor). In no series.

**D, "Names Abroad"** (`store_names_abroad`, a log with no EPUB): documents `OEBPS/c1.xhtml` (chapter 1 "Arrivals") and `OEBPS/c2.xhtml` (chapter 2 "End"); ch 1 `Introduced eida` "에이다" (given "에이다") quoting "에이다는 웃었다.", and ch 1 `Introduced yoshida` "𠮷田" (surname "𠮷田") quoting "𠮷田さんは来た。". At `OEBPS/c2.xhtml`, "에이다는 웃었다." matches 에이다 in 9 bytes, 3 characters and 3 UTF-16 units, and "𠮷田さんは来た。" matches 𠮷田 in 7 bytes, 2 characters and 3 UTF-16 units (U+20BB7 takes two), so counting bytes fails both and counting characters fails the second.

**The series** (`store_keepers_series`): "keepers", title "The Keepers", orders by rank `printed` = [A, B], then `author` = [B, A]. Rank and name order disagree: `author` sorts first.

Tests in this crate use distinct invented digests (`"a1".repeat(32)` for A, `"b2"`, `"c3"`, `"d4"` likewise), never one string for two books.

- [ ] **Step 1: The fixtures.** The builder turning a compact description into a `BookLog` is the implementer's; quotes come from the constants, never retyped.
- [ ] **Step 2: Write the failing tests** in `characters/tests.rs`: in-memory library, logs stored by the fixtures, `Access::Every`, `earlier: false`, the document given directly.

  | Test | Asserts | Watched failing against |
  | --- | --- | --- |
  | `a_card_holds_only_what_the_finished_chapters_say` | A at `OEBPS/c2.xhtml`: "Mrs. Vell said nothing." gives `known: true`, no characters; "Ned Reed knew his cousin at once." gives Ned, whose relations name Tom alone, with no fact holding "cousins". At `OEBPS/c3.xhtml`: Mara's card; Ned's relations name Tom and Mara in haiyaku's order, Mara's `family` fact with note "cousins" and quote "Ned Reed knew his cousin at once." | relations filtered on `a` alone (Ned is `b` beside Mara); `with` naming the card's own character |
  | `an_overturned_fact_is_marked_once_its_chapter_is_read` | Ned's `friend` fact with Tom: `overturned: null` at `c3`; at `c4` `{note: "Tom only pretended", quote: "Ned Reed saw that Tom Field had lied.", at: {book: "The Lamp Keepers", chapter: 3, chapterTitle: "Return"}}`; his `mentor` fact `null` at both | the overturn's note and quote swapped with the fact's; `at` taken from the fact's `first` |
  | `a_directed_fact_says_which_side_it_runs_from` | at `c2`: Ned's `mentor` fact `from: "with"`, Tom's (text "Tom Field showed Ned Reed the wick.") `from: "this"`; both `friend` facts `from: null` | `"this"` and `"with"` swapped; the side read from `a`/`b` instead of the card's key |
  | `several_characters_come_most_recently_seen_first` | at `c4`, "Reeds put out the lamp." gives Ned Reed then Ada Reed, both `matched.part == "surname"` with their own names as `matched.text`, and `length == 5` | the cards re-sorted by the cast's order or by name |
  | `a_card_carries_every_name_with_its_labels` | Mara at `c3`: `name == {text: "Mara Vell", given: "Mara", surname: "Vell", title: null, titleSex: null}`; `aliases == [{text: "Mrs. Vell", given: null, surname: "Vell", title: "Mrs.", titleSex: "female"}]` | `titleSex` mapped to the other sex; an alias's labels dropped |
  | `first_and_last_name_haiyakus_book_and_chapter_titles` | Ned at `c4`: `first == {book: "The Lamp Keepers", chapter: 1, chapterTitle: "The Lamp"}`, `last == {…, chapter: 3, chapterTitle: "Return"}` | `chapters[n]` for `chapters[n - 1]` ("Dusk", "Out"); `first` and `last` swapped |
  | `length_is_counted_in_utf16_units` | D at `OEBPS/c2.xhtml`: "에이다는 웃었다." gives `length == 3`; "𠮷田さんは来た。" gives `length == 3` | `mention.len` sent as it is (9, 7); characters counted (3, 2) |
  | `no_match_sends_no_length` | A at `c4`, "The Reeds put out the lamp." from "The": `known: true`, no characters, and `serde_json::to_value` has no `length` key | `length: 0` sent |
  | `an_unknown_book_is_not_known` | a digest nothing stored: `Answer::unknown()`, and its serialized keys are exactly `known`, `scope`, `narrowed`, `characters` | `known: true` for it |
  | `a_document_haiyaku_skipped_is_known_with_no_characters` | A at `OEBPS/notes.xhtml`, text "Ned Reed": `known: true`, no characters (not `front.xhtml`: it is in A's log with no chapter, so haiyaku folds nothing there and no taiyaku defect could fail it) | `cast_at`'s `NotFound` answered as unknown, or as `Err` |
  | `the_answer_sends_exactly_these_keys` | Ned at `c4`, serialized: the key set at every level is the table's | a key renamed (`chapter_title`), or one added (the entity key, a digest) |
  | `titles_are_read_once_per_book` | two answers in A and one in D: `titles.reads() == 2` | no cache (3 reads) |
  | `holds_says_whether_haiyaku_holds_a_book` | A stored: `true`; another digest: `false` | `NotFound` read as held |

- [ ] **Step 3: Run them; all red** into `$EV/t2-red.log` (a missing module is a compile failure: say so and keep going once it compiles with `todo!()` bodies).
- [ ] **Step 4: Implement `characters.rs`** to the signatures and steps above, `Scope::Book` only (`_access` unread until Task 3).
- [ ] **Step 5: Green, then each defect watched red** (`$EV/t2-mutations.md`), reverted.
- [ ] **Step 6: Clippy, fmt, the comment check.**
- [ ] **Step 7: Commit.** "library: a character's card as of the chapter finished, from haiyaku's fold and look-up".

### Task 3: Earlier books: the scope and the grant

**Files:**
- Modify: `crates/taiyaku-library/src/characters.rs`, `characters/tests.rs`
- Modify: `crates/taiyaku-read/src/test_support.rs` (`epub_documents`)
- Modify: `crates/taiyaku-library/src/test_support.rs` (`keepers`, the invented library on disk, built once here for this crate's tests and `taiyaku-serve`'s), `crates/taiyaku-library/Cargo.toml` (`[features] test-support = ["taiyaku-read/test-support"]`, so `keepers` can write its EPUBs in any crate's test build; the comment beside the feature says so)

**Interfaces:**
- Consumes: Task 2's `answer`, `Asked`, `Answer` and fixtures.
- Produces: `pub(crate) fn choose_scope(holding: &[haiyaku::store::Holding], earlier: bool, series: Option<&str>, order: Option<&str>) -> haiyaku::Scope`; `pub(crate) fn books_seen(conn: &Connection, access: &Access, books: &[String]) -> Result<HashSet<String>, String>`; `pub fn epub_documents(dir: &Path, name: &str, dc: &str, documents: &[(&str, &[&str])]) -> PathBuf` in `taiyaku_read::test_support`; `pub fn keepers(library: &Library, dir: &Path, shelve: &[&str]) -> Keepers` in `taiyaku_library::test_support`, `Keepers` (`Debug`) holding the root id of each book shelved and the digest of each of A, B and C, by the books' letters.

**The choice,** first written here:

```text
choose_scope(holding, earlier, series, order):
  if not earlier, or holding is empty:  Scope::Book
  held  = the holding whose series is series, else holding[0]     (haiyaku lists them by name)
  named = order if held.orders lists it, else held.orders[0]       (by rank; never empty)
  Scope::Series { series: held.series, order: Some(named) }         (always by name)
```

`answer` then gains step 2, `choose_scope` over step 1's holding, and step 4: after a series fold, if any of `cast.books` is missing from `books_seen(conn, access, &cast.books)`, fold again with `Scope::Book` (its `NotFound` answered as in step 3) and answer `scope: null`, `narrowed: "not-visible"`. Otherwise `scope` is the series and order folded. `books_seen` is the digests in `books` held by a member the access sees: `member.sha256 IN (…)` with `access.grant_clause(|holes| browse::member_under("m", holes))`, the rule browse counts by. The digests' `IN (…)` comes first and the grant clause last, as `taiyaku-serve`'s `metadata.rs` appends its grant clauses, and the values bind in that order, the digests and then `access.bound()`: plain `?` holes bind in the order the text holds them. `Access::Every` adds no clause, so a book the library does not hold at all still narrows. FLAG 11.

`epub_documents`: the package at `OEBPS/content.opf`; every document path must start `OEBPS/` (it panics otherwise) and its manifest href is the rest; the spine in the order given; each paragraph one `<p>`; `dc` as `epub_with` takes it. `taiyaku_read::read_book` then gives each block the `document` the constant names.

- [ ] **Step 1: Write the failing tests.**

  `choose_scope_takes_the_named_series_and_order_and_falls_back_to_the_first` (no database). Holding, as haiyaku lists it: `anthology` with orders [`told`, `printed`], then `keepers` with [`printed`, `author`]. Every Series row asserts `order: Some(…)`.

  | earlier | series | order | Scope |
  | --- | --- | --- | --- |
  | false | `keepers` | `author` | `Book` |
  | true, holding empty | – | – | `Book` |
  | true | – | – | `anthology` / `told` |
  | true | `keepers` | – | `keepers` / `printed` |
  | true | `keepers` | `author` | `keepers` / `author` |
  | true | `keepers` | `told` | `keepers` / `printed` |
  | true | `gone` | `printed` | `anthology` / `printed` |
  | true | `gone` | `gone` | `anthology` / `told` |

  Watched failing against: the last series taken; orders taken by name (`printed` for `anthology`); an order kept that the chosen series lacks (`keepers` / `told`); `order: None` passed on.

  **The keepers library**, for the rest, built once by `taiyaku_library::test_support::keepers(library, dir, shelve)`, which Task 5's tests call too: A, B and C written with `epub_documents` (`dc` holding a `dc:title` unlike haiyaku's: "Keepers One", "Keepers Two", "Alone") at `<dir>/<Title>/<Title> v01.epub`, each in a directory of its own; each book named in `shelve` ("A", "B", "C") added as a root of its own, scanned and derived the way `browse/tests.rs::two_root_library` does; digests from `haiyaku::store::sha256_file`; every book's log, A's whether shelved or not, and the series stored with Task 2's fixtures. Here it runs on an in-memory library with all three shelved unless a test says otherwise. Taps are at B's `OEBPS/c2.xhtml` with "Ned Reed wrote to Wren Hollis." unless named.

  | Test | Asserts | Watched failing against |
  | --- | --- | --- |
  | `earlier_off_folds_the_book_alone` | no characters; `scope` and `narrowed` null | `earlier` ignored |
  | `earlier_on_folds_the_earlier_book_in_the_first_ranked_order` | Ned, `first.book == "The Lamp Keepers"`; `scope == {series: "keepers", order: "printed"}` | step 2 left out (Task 2's `Scope::Book`) |
  | `a_named_order_the_series_lacks_falls_back_to_the_first_ranked` | order `gone`: Ned; `scope.order == "printed"` | the named order passed on (`NotFound`, no characters) |
  | `an_order_that_reads_the_book_first_folds_nothing_before_it` | order `author`: no characters; `scope == {keepers, author}` | the named order ignored |
  | `a_book_in_no_series_folds_alone` | C at its `OEBPS/c2.xhtml`, "Iris Moor left.", earlier on: Iris; `scope` and `narrowed` null | a `Series` scope with no series (`NotFound`) |
  | `a_series_fold_through_a_book_the_account_cannot_see_narrows_to_the_book` | `Access::roots(vec![B's root])`: no characters, `scope: null`, `narrowed: "not-visible"`; `Access::roots(vec![A's, B's])`: Ned, `narrowed: null` | step 4 left out; `narrowed` set but the series cast kept; only the tapped book checked |
  | `a_series_fold_through_a_book_the_library_does_not_hold_narrows_for_every_account` | `keepers` shelving "B" and "C" alone (A's log still stored), `Access::Every`: `narrowed: "not-visible"`, no characters | `Every` taken as seeing every book |
  | `books_seen_is_the_grant` | `Access::roots(vec![B's root])` sees {B}; `Every` sees {A, B} and not D's digest; `Access::roots(vec![])` sees nothing | the grant clause left off |

- [ ] **Step 2: Red** into `$EV/t3-red.log`.
- [ ] **Step 3: Implement** `choose_scope`, `books_seen`, steps 2 and 4 (`_access` becomes `access`), `epub_documents` and `keepers`.
- [ ] **Step 4: Green, each defect watched red** (`$EV/t3-mutations.md`), reverted. Also `cargo nextest run -p taiyaku-read` green.
- [ ] **Step 5: Clippy, fmt, the comment check.**
- [ ] **Step 6: Commit.** "library: earlier books in the series, by the first series and its first-ranked order, narrowed to the book outside the grant".

### Task 4: The book's flag: which book editions have character data

**Files:**
- Modify: `crates/taiyaku-serve/src/book.rs` (the flag in `get_book` and `document`), `crates/taiyaku-serve/src/book/tests.rs` (the new tests, and the direct `document(…)` call in `pins_hold_only_against_the_works_own_hub`, which the new parameter changes)

**Interfaces:**
- Consumes: `taiyaku_library::characters::holds` (Task 2), `Library::cards_off` (Task 1), Task 2's fixtures (`store_lamp_alone`).
- Produces: `"characters": true` on each book edition of `GET /api/works/{work}/book` that has character data, and no key on any other edition.

**The flag:** in `get_book`'s blocking closure, under the same `conn()` that reads pins and books, the digests of the view's book editions for which `characters::holds` answers true (none while `cards_off()`, with no haiyaku call), handed to `document`, which adds `"characters": true` to those editions alone. An `Err` from `holds` writes one warning line to stderr through `routes::log_line`, as `document` logs an edition it leaves out, and leaves that edition unflagged: it never fails the book's answer.

- [ ] **Step 1: Write the failing tests** in `book/tests.rs`, through `get_book` as the file's `get` calls it. Statuses are asserted exactly.

  | Test | Asserts | Watched failing against |
  | --- | --- | --- |
  | `the_book_says_which_editions_have_character_data` | the `books()` work (`en`, `ko`, `raw`) with a log stored under `en`'s digest (`store_lamp_alone(conn, en)`: the flag asks only whether haiyaku holds the digest): `editions.en.characters == true` and `editions.ko` has no `characters` key | the flag on every edition; the flag left out |
  | `with_cards_off_the_book_sends_no_flag` | the `books()` work held in a file library with `en`'s log stored, its `haiyaku_schema` planted at 3 and the library reopened (cards off): 200, and `editions.en` has no `characters` key | the flag not checking `cards_off` (`holds` still reads the tables, and the key is sent) |
  | `a_haiyaku_read_that_fails_sends_no_flag_and_the_book_still_opens` | the `books()` work with `en`'s log stored, then `DROP TABLE haiyaku_order_book` in the test (no table references it, and `series_holding` then fails on it): 200, both book editions sent, `editions.en` has no `characters` key | `holds`'s error propagated with `?` (the book answers 500); the error read as data (the key sent) |

  `books()` builds its state on `test_support::state`'s in-memory library; the cards-off test needs the same work on a file (`Library::open` in the fixture's directory), by a parameter or a sibling of `books` that keeps one copy of its sidecar.
- [ ] **Step 2: Red** into `$EV/t4-red.log`.
- [ ] **Step 3: Implement** the flag.
- [ ] **Step 4: Green, each defect watched red** (`$EV/t4-mutations.md`), reverted; `taiyaku-serve` whole.
- [ ] **Step 5: Clippy, fmt, the comment check.**
- [ ] **Step 6: Commit.** "serve: the book's answer says which book editions have character data".

### Task 5: The characters route

**Files:**
- Create: `crates/taiyaku-serve/src/characters.rs`, `crates/taiyaku-serve/src/characters/tests.rs`
- Modify: `crates/taiyaku-serve/src/lib.rs` (`mod characters;`), `src/book.rs` (`document_at`), `src/routes.rs` (`guarded_routes`, `grant_postures`, `narrowing_postures`), `src/auth/tests.rs`, `src/routes/tests.rs` (the two route walks), `src/fingerprint_pass/tests.rs` (the reading routes' list)

**Interfaces:**
- Consumes: `taiyaku_library::characters::{answer, Asked, Answer, LONGEST_TEXT}`, `Library::{cards_off, titles}`, `taiyaku_library::test_support::keepers` (Task 3) and Task 2's fixtures, `book.rs` as Task 4 leaves it.
- Produces: `pub(crate) async fn get_characters(State<Arc<ServeState>>, AxPath<(String, String)>, Query<Vec<(String, String)>>, axum::http::Extensions) -> Response`; `book::document_at(sha256: &str, path: &Path, block: usize) -> Result<Option<String>, String>`, the block's `id.document` through `book::read`'s cache and `None` past the end.

**The handler**, in this order, as `book::get_book_image` goes:
1. `state.fingerprints.reader_seen()`; `view::of_work`; `state.works.get`; `is_missing`; `routes::refuse_if_cannot_stream`; the principal from the extensions (`routes::NoPrincipal` without one: `of_work` has already refused a request with no principal, so the branch is defensive, accepted untested, and recorded `accepted` if the sweep makes a mutant of it).
2. The edition: a book edition of the view (`book::read_books`, `book::is_book`) with one member path (`download::member_paths`), else 404 "work `…` has no book edition `…`".
3. The query, only now, so the auth tests' bare path meets the grant's 404 first. `block` a decimal number, else 400; `text` present, not empty and at most `LONGEST_TEXT` UTF-16 units, else 400 naming the bound; `earlier` absent or `1` for on, `0` for off, anything else 400; `series` and `order` optional, an empty one as absent; other keys ignored. FLAGs 5 and 6.
4. On the blocking pool: `document_at`, `None` answering 400 "block N is past the end of …". Then on `state.library.conn()`: with `cards_off()`, `Answer::unknown()` and no haiyaku call; else `characters::answer(&conn, &asked, &principal.access, state.library.titles())`.
5. 200 and the answer as JSON; an `Err` is 500 through `routes::answered_and_logged`.

**The tables:** `("/api/works/{work}/{edition}/book/characters", get(crate::characters::get_characters))` after the image route in `guarded_routes`; `Refuses` in `grant_postures`, with a comment as its neighbours have; `NoPages { reason: "a book edition is one file, seen whole or not at all, and its blocks are numbered in that file alone" }` in `narrowing_postures`. In `auth/tests.rs`: the path in `every_route_refuses_an_anonymous_request`'s stated list; in `every_route_declares_its_grant_posture` `driven` from 28 to 29, its message gaining "and the characters route to 29, `Refuses` as the book's images route beside it is"; in `every_route_declares_what_it_does_with_a_narrowed_edition` the `NoPages` count from 49 to 50.

  In `routes/tests.rs`: `an_account_granted_only_a_library_with_no_dir_sees_nothing` leaves the new route out beside the image route's filter, with its reason (the fixture holds no book, so the route answers 404 in the granted pass, and a bare GET with no `block` would be a 400, on which `what_it_shows` panics); in `every_route_naming_a_work_answers_a_hidden_one_as_a_slug_no_work_holds`, `walked` from 16 to 17, as the new path carries `{work}`. In `fingerprint_pass/tests.rs`: `each_reading_route_marks_a_reader` gains `/api/works/{book}/raw/book/characters?block=0&text=One.` (`misnamed`'s book has no log, so it answers `known: false` with 200), which holds the handler's `reader_seen()`.

**The keepers shelf** (`characters/tests.rs`'s fixture): `taiyaku_library::test_support::keepers` (Task 3) on the state's library, all three shelved, in a tree under the test's directory; the works projected (`works::from_library`) into the state, as `test_support::misnamed` builds its state; the slugs and edition keys read back from the library. A's blocks, edition-wide: 0 front; 1 to 3 `c1`; 4 to 6 `c2`; 7 and 8 `c3`; 9 `c4`; 10 `notes`. B's: 0 `c1`, 1 `c2`.

- [ ] **Step 1: Write the failing tests.** Through the handler as `book/tests.rs` calls `get_book`, or through the router where a test needs the auth layer. Statuses are asserted exactly, never with `!=`.

  | Test | Asserts | Watched failing against |
  | --- | --- | --- |
  | `the_chapter_being_read_is_left_out_to_its_last_block_and_back_from_the_next` | A block 6 (`c2`'s last), "Mrs. Vell said nothing.": no characters; block 7 (`c3`'s first), "Mara Vell came back.": Mara; block 6, "Ned Reed knew his cousin at once.": Ned without Mara | `document_at` off by one either way (block 6 reaching `c3` shows Mara; block 7 reaching `c2` hides her) |
  | `the_card_names_haiyakus_titles_not_the_works` | A block 9, "Reeds put out the lamp.": Ned then Ada; Ned's `first.book == "The Lamp Keepers"` while the work's title is "Keepers One" | the work's title sent |
  | `earlier_reaches_the_series_and_zero_keeps_to_the_book` | B block 1, "Ned Reed wrote to Wren Hollis.": `earlier=1` gives Ned and `{keepers, printed}`; `earlier=0` none; no `earlier` gives Ned; `order=author` none; `series=keepers&order=gone` Ned | a query key not passed through (`earlier` fixed, `order` dropped) |
  | `a_restricted_account_outside_the_earlier_book_gets_the_book_alone_and_is_told` | a streaming principal with `Access::roots(vec![B's root])`: `narrowed: "not-visible"`, no characters; with both roots: Ned | the route passing `Access::Every` |
  | `an_unknown_book_is_not_known` | a fourth EPUB scanned, no log: `known: false` with the four keys | a 500 for it |
  | `a_block_past_the_end_and_a_bad_query_are_400` | A: block 11, block `x`, no `block`, no `text`, a `text` of 129 UTF-16 units, `earlier=2`: each 400; a `text` of exactly 128 units: 200 | the index unchecked (a panic or 500); the bound off by one |
  | `the_skipped_afterword_is_known_with_no_characters` | A block 10, "Afterword.": `known: true`, no characters | `NotFound` answered as unknown or as 500 |
  | `an_account_that_may_not_stream_is_refused` | `can_stream: false`: 403, as `book/tests.rs::an_account_that_may_not_stream_is_refused_the_text` | the stream check left out |
  | `an_edition_that_is_no_book_is_404` | `test_support::misnamed`'s comic work and its `raw` edition: 404 | the book check left out |
  | `with_cards_off_the_route_reads_nothing` | `keepers` on a file library (`Library::open`), its `haiyaku_schema` planted at 3 and the library reopened (cards off), the works projected from it: A block 9, "Reeds put out the lamp.", answers `known: false` | the route not checking `cards_off` (`series_holding` still reads the tables: `known: true`) |

  The auth tables go red until the entries land, then their `Refuses` walk drives the new route against the withheld work. Watch it fail against the handler reading its query before `of_work` (a `Query<…>` of a struct with a required `block`): the walk's bare path then answers 400, not 404.
- [ ] **Step 2: Red** into `$EV/t5-red.log`.
- [ ] **Step 3: Implement** the route, `document_at`, the table entries and the walks' edits.
- [ ] **Step 4: Green, each defect watched red** (`$EV/t5-mutations.md`), reverted; `taiyaku-library` and `taiyaku-serve` whole.
- [ ] **Step 5: Clippy, fmt, the comment check.**
- [ ] **Step 6: Commit.** "serve: the characters a tapped name names, on a route beside the book's images".

### Task 6: The reader asks: the request and the setting

**Files:**
- Modify: `web/src/book.ts`, `web/src/modules/lookup.ts`, `web/src/BookReader.tsx`
- Test: `web/tests/book.test.ts` (node), `web/tests/BookReader.test.tsx` (jsdom, inside `describe('the look-up mode')` and beside `withDictionary`)

**Interfaces:**
- Consumes: Task 4's flag, Task 5's route; Task 2's table.
- Produces, in `book.ts` (beside `getBook` and `bookImageUrl`, where the book route's types live; FLAG 13): `Edition.characters?: true`; `Characters`, `CharacterCard`, `CharacterName`, `ChapterAt`, `CharacterRelation`, `CharacterFact`, field for field Task 2's table; `CHARACTERS_TEXT_UNITS = 128`; `CARDS_WAIT_MS = 300` (Task 7 uses it; FLAG 9); `charactersText(text: string): string`, at most 128 UTF-16 units and one fewer where the 128th is the first half of a pair; `getCharacters(work: string, edition: string, ask: { block: number; text: string; earlier: boolean }): Promise<Characters>`, asking `/api/works/{work}/{edition}/book/characters?block=…&text=…&earlier=1|0` with each part `encodeURIComponent`ed and throwing on a non-2xx as `getJson` does. Constants and functions sit in `book.ts`, not in a component module, for biome's `useComponentExportOnlyModules`.
- `lookupModule.defaults` becomes `{ on: false, includeEarlierBooks: true }`; the generated panel labels it "include earlier books".
- In `BookReader`: a look-up in an edition whose `characters` is `true` sends one characters request, started where the look-up starts (the Japanese tap, the word tap, the selection bar's Look up), tagged with that look-up's `asking` number and kept with it for Task 7. Until Task 7 adds the hold, the kept request's rejection is caught and logged with `console.warn` (design §7: the failure is logged), so a failed request is never an unhandled rejection, which would also fail a Vitest run; the tests below answer the characters route explicitly, since the reader's jsdom stub answers an unknown GET with 204, on whose empty body `getJson` throws. `text` is `charactersText` of the block from the word's start (`wordAt`), from the tapped character (Japanese), or of the selected text (the bar); `block` is the look-up's block; `earlier` is the slice's `includeEarlierBooks !== false`. Not in the `[lookingUp]` effect: the Japanese answer replaces `lookingUp` and would send a second request.

- [ ] **Step 1: Write the failing tests.**

  | Test | Asserts | Watched failing against |
  | --- | --- | --- |
  | `charactersText cuts at 128 units and never inside a pair` (node) | 200 ASCII letters give 128; 127 letters, then "😀", then more give 127 and no lone surrogate; exactly 128 come back whole; "" gives "" | `slice(0, 128)` |
  | `getCharacters asks the edition's route with block, text and earlier` (node, fetch stubbed) | `getCharacters('w x', 'en', {block: 3, text: 'Ned Reed', earlier: false})` asks `/api/works/w%20x/en/book/characters?block=3&text=Ned%20Reed&earlier=0` | `earlier` sent as `true` or left out |
  | `asks for the characters a tapped word names, from the word on, with the earlier books` | a two-edition book, `en` (flagged, not the hub) and `ko` (the hub, unflagged); a tap on a word that does not start block 2 of `en`, the block running past 128 units after it: one `/book/characters` request, edition `en`, `block=2`, `text` the block from the word's start cut to 128 units, `earlier=1` | the text from the block's start; the block of the tap ± 1; the hub's key in the path |
  | `cuts the text at 128 units without splitting a character outside the BMP` | the 128th unit after the word is the first half of "😀": one request, its decoded `text` 127 units long | no `charactersText` (`encodeURIComponent` throws, no request) |
  | `asks with the selection bar's text and the block it starts in` | "Ned Reed" selected in block 3 and looked up from the bar: `block=3&text=Ned%20Reed` | the word under the bar's start; block 0 |
  | `asks nothing of a book edition without character data` | the same tap in `ko`: a dictionary request and no characters request | the flag ignored |
  | `asks once for a Japanese tap, after its longest word too` | a flagged Japanese edition; a tap, then the `at` answer: one characters request, `text` from the tapped character | the request sent from the `[lookingUp]` effect (two) |
  | `includes the earlier books by default and asks without them once the setting is off` | `lookupModule.defaults.includeEarlierBooks === true`; a stored `{on: true, includeEarlierBooks: false}` gives `earlier=0`; a stored `{on: true}` from before gives `earlier=1` | `earlier` fixed at 1 |
  | `asks without the earlier books on the next tap once the setting is turned off in the session` | the reader open with the defaults: a tap asks `earlier=1`; "include earlier books" turned off in the reader's settings panel; the next tap asks `earlier=0` | the slice read once at mount |

  Two existing tests assert the stored slice `toEqual({ on: true })`: `turns on and off on l and from its button in the bars, remembered` and `leaves the mode off for an l typed into the search box, and turns it on for one outside it`. Their premise changed (a stored slice carries the defaults), so both now expect `{ on: true, includeEarlierBooks: true }`.
- [ ] **Step 2: Red:** `pnpm -C $W/web run test:coverage >| $EV/t6-red.log 2>&1` (whole projects).
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Green; each defect watched red** (`$EV/t6-mutations.md`), reverted. `pnpm -C $W/web typecheck`, `env -C $W node scripts/coverage-floor.mjs`.
- [ ] **Step 5: Commit.** "web: a look-up on a book with character data asks for the characters the tapped words name".

### Task 7: The cards in the panel

**Files:**
- Create: `web/src/BookCharacters.tsx`
- Modify: `web/src/BookLookup.tsx` (a `cards?: CharacterCard[]` prop drawn above the entries), `web/src/BookReader.tsx` (the hold), `web/src/styles/book.css` (only what keeps a plain card legible: spacing between cards and facts; and a rule for every class `BookCharacters.tsx` writes, since `web/tests/classNames.test.ts` (O158) fails a class no selector names, spacing counting as one; an anchor that needs no style is a data attribute instead)
- Test: `web/tests/BookLookup.test.tsx`, `web/tests/BookReader.test.tsx`

**Interfaces:**
- Consumes: Task 6's types, `CARDS_WAIT_MS` and the kept answer.
- Produces: `BookCharacters({ cards }: { cards: CharacterCard[] })`, and the plain card's anchors, which the tests and the prototype start from: `.book-lookup-characters` around the cards, absent when there are none; per card a `section.book-lookup-character` whose heading is `name.text`; `.book-lookup-aliases` with the other names, absent when there are none; `[data-met="first"]` and `[data-met="last"]`, each the book's title, the chapter's number and its title; per fact an `li.book-lookup-fact` with `data-kind`, the other character's name and the note, `data-overturned` and the overturn's note on an overturned fact, and a `Quote` button that shows `.book-lookup-quote` (the fact's quote, and the overturn's). What a card holds is the owner's (design §5); its words and look are the prototype's (FLAG 16).
- The hold, in `BookReader`: in a flagged edition the panel draws `Looking it up…` until both the dictionary's outcome (any `Looked` but `looking`) and the characters answer are in, or until `CARDS_WAIT_MS` after the dictionary's outcome, whichever comes first. A failed characters answer ends the hold at once and is logged with `console.warn`, the log Task 6 added and still the one log. An answer whose `asking` number is not current, or one that comes after the wait, is dropped. An unflagged edition holds nothing. FLAG 9.

- [ ] **Step 1: Write the failing tests.** Cards from Task 2's books (Ned at A's `c4`, Mara at `c3`), typed as `Characters`.

  | Test | Asserts | Watched failing against |
  | --- | --- | --- |
  | `draws each card above the dictionary's entries, in the answer's order` (`BookLookup.test.tsx`) | cards [Ned, Ada] and one entry: both `.book-lookup-character` precede the first `.book-lookup-entry` (`compareDocumentPosition`); headings "Ned Reed", "Ada Reed" | the cards after the entries; the cards reversed |
  | `a card names its other names and where it was first met and last seen` | Mara: "Mrs. Vell" in `.book-lookup-aliases`; `[data-met="first"]` holds "The Lamp Keepers", 2 and "Dusk", `[data-met="last"]` 3 and "Return" | first and last swapped |
  | `a fact shows its note, its quote on demand, and an overturned fact is marked` | Ned's two facts with Tom: no `.book-lookup-quote` until `Quote` is pressed inside `act`, then the quote; only the `friend` fact has `data-overturned` and "Tom only pretended" | the quote always drawn; every fact marked |
  | `no card and no room for one when the answer names no one` | `cards: []`: no `.book-lookup-characters` | an empty container drawn |
  | `holds the first draw until the characters are in` (`BookReader.test.tsx`) | the dictionary answers at once, the characters a little later but inside the wait: no entry before the characters answer, then both, the card first | the entries drawn at once |
  | `draws the dictionary when the characters miss the wait, and a late answer then moves nothing` | the characters held past `CARDS_WAIT_MS`: the entries drawn at the wait's end; the characters resolved after: no card, the entry the same DOM node | late cards drawn; no wait (held for ever) |
  | `draws the dictionary alone when the characters fail, and logs the failure` | a 500: entries, no card, no failure text, `console.warn` called once with it, the entries drawn before the wait would end | the failure drawn; not logged; the wait run out after a failure |
  | `a later tap's panel never draws an earlier tap's cards` | a tap on Ned (characters held), a tap on Mara (answered), then Ned's answer: only Mara's card | the `asking` check left out |
  | `a book without character data draws the dictionary at once` | a tap in `ko`: the entries drawn before `CARDS_WAIT_MS` has passed | the hold on every edition |

  The wait is driven with Vitest's fake timers if `land()` works under them, else with real waits of `CARDS_WAIT_MS`; the report says which.
- [ ] **Step 2: Red** (whole projects) into `$EV/t7-red.log`.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Green; each defect watched red** (`$EV/t7-mutations.md`), reverted; typecheck; coverage floor (`BookCharacters.tsx` is a new file).
- [ ] **Step 5: Commit.** "web: the characters a tap names, drawn above the dictionary's entry once both are in".

### Task 8: The prototype, with the owner (controller)

Not an implementer task. Built and reviewed before it: Tasks 0 to 7, so the panel shows cards from real data through the route, the flag and the setting. `$SP` below is the controller session's scratchpad; the recipe is the project's (memory `taiyaku-ui-work-prototype-on-a-live-server`), adapted.

- [ ] **Step 1: A scratch copy of the test library with its Books root.** `df` the scratchpad's filesystem (the Books root is about 170 MB). `sha256sum` and `stat` `/home/davey/temp/manga-test/library.db`; `touch $SP/proto/marker`; `cp` the database to `$SP/proto/library.db` (never open the original); `cp -a /home/davey/temp/manga-test/Books/Literature $SP/proto/Literature`.
- [ ] **Step 2: The branch's server.** `cargo build --release -p taiyaku-cli` in `$W` (its own target), the binary copied to `$SP/proto/bin/`; `printf 'pw\npw\n' | … library user password local --db $SP/proto/library.db`; `… library scan --add $SP/proto/Literature --root $SP/proto/Literature --db …` (never without `--root`); `UPDATE root SET paused_at = datetime('now') WHERE id = 1` in the copy. The branch's first open of the copy takes one backup beside it, named as FLAG 3 says: `-before-v32` while the copy's taiyaku schema is below 32 (likely: the file was last written on 2026-11-15, before migrations 028 to 032 landed), else `-before-haiyaku-v2`. Record the name that appears, Task 1 on real material.
- [ ] **Step 3: haiyaku's CLI at the dependency's version.** `cargo install --locked haiyaku@0.2.0 --root $SP/proto/haiyaku` (the default features build the binary). Its reply cache is `$XDG_CACHE_HOME/haiyaku`, else `~/.cache/haiyaku`, where the 2026-11-23 acceptance runs left their replies.
- [ ] **Step 4: The files that were extracted.** `sha256sum` each EPUB under `$SP/proto/Literature/J. K. Rowling/Harry Potter` and `…/Harry Potter (Korean)` against the copies haiyaku extracted under `/home/davey/temp/haiyaku-corpus/` (`hp-en/` and the Korean folder beside it). A digest that differs means a cache miss: ask the owner.
- [ ] **Step 5: Fill from the cache only.** Every CLI step runs with a `PATH` holding no `claude` program, so a cache miss fails instead of calling the model. `extract <epub> --chapters <range> --db $SP/proto/library.db` at the acceptance run's ranges (haiyaku's `spike/out/linking/acceptance-report.md` and `docs/handoffs/2026-11-23-haiyaku-linking-handoff.md`): English Sorcerer's Stone 4-20, Chamber of Secrets 4-21, Goblet of Fire 4-40; Korean 5-21, 5-22, 5-41. Copy `/home/davey/projects/haiyaku/spike/out/linking/en.toml` and `ko.toml` into `$SP/proto`, point their `[books]` at the scratch EPUBs, then `series <file> --db …` and `link <series> --db …`. A step that fails for want of `claude` is a miss: stop, tell the owner which book and what haiyaku's notes say such a run cost, and run it with the model only on their word. FLAG 15.
- [ ] **Step 6: Serve it.** `pnpm -C $W/web exec vite build --outDir $SP/proto/dist --emptyOutDir`. Check the port first (`ss -lptn 'sport = :4399'`; another port if it is taken). The controller starts the server, never a subagent: `… serve --db $SP/proto/library.db --cache $SP/proto/cache --web $SP/proto/dist --dictionaries <DIR> --addr 127.0.0.1:4399 --insecure-cookies --log-requests $SP/proto/requests.jsonl`, `<DIR>` holding the English and Korean dictionaries (ask the owner where, if no earlier prototype recorded it).
- [ ] **Step 7: Measure the fold (design §7)** before the rounds, with a headless client signed in as `local` (`POST /api/session` with an `origin` header): 20 characters requests with `earlier=1` in English Goblet of Fire's last story chapter and 20 in Sorcerer's Stone's second, the same 40 with `earlier=0`, each beside its dictionary request; `GET /api/works/{work}/book` and comic page requests in the same session; the first characters request after a start (the titles read) kept apart. `micros` from the request log. FLAG 10's rule then decides: the shared connection stays, or the fold moves to `Library::working()` in a small task before Task 9; a fold cache is opened as a row only if the rule asks. Keep the figures, the build and the date for Task 10.
- [ ] **Step 8: The CLI against the live file (D6).** With the server serving and a characters request every second, run `series <file> --db …` (no model) and one `extract --replace` of a cached book under the same `PATH`. Record what each side saw: the CLI's wait or its "in use by another run", any request in the log that was not 200, and whether a card's book and chapter titles went stale after the `--replace` (FLAG 4).
- [ ] **Step 9: The rounds.** Tell the owner the URL (`/read/<slug>`), what to look at (a name's card in English and in Korean, a name first met in the chapter being read, the setting, phone size) and what will not change; ask which browser. One thing a round, rebuilt into the same `dist`, the owner reloads. To settle: the card's look, where the quote opens, how an overturned fact is marked, whether the lit text widens to the whole name (`length`), the wait, and the card in the phone seat (O369, O370). Each round's ask and change in `$SP/proto/rounds.md`, with no book text.
- [ ] **Step 10: The approved look lands as its own commit,** "web: the character card as the owner approved it on the prototype", holding the prototype's changes alone. Then Task 9.
- [ ] **Step 11: Clean up.** Kill the server by its PID (`ss -lptn 'sport = :4399'`, `/proc/<pid>/exe`) and check the port again; `find /home/davey/temp/manga-test -newer $SP/proto/marker` prints nothing and the database's digest is unchanged. Copy `rounds.md` and the figures (no quotes from the books) to `$EV/prototype/`, then delete `$SP/proto` by its literal path in a call of its own.

### Task 9: Tests that hold the approved look

**Files:**
- Create: `web/tests/bookCharacters.browser.test.tsx`
- Modify: `web/tests/BookLookup.test.tsx` and `web/tests/BookReader.test.tsx` where the approved commit moved an anchor; any `web/src` fix the tests force

**Interfaces:**
- Consumes: the approved commit, whose diff is the list of what to hold.

- [ ] **Step 1: List what the approved commit adds,** each declaration and behaviour (the card's place, the quote's opening, the overturn's mark, the lit range if widened, the phone seat's limits, the wait), in `$EV/t9-held.md`.
- [ ] **Step 2: Write the browser tests,** in real layout, with a real mouse (`web/tests/support/mouse.ts`) and a real touch (`touch.ts`, `pointer.ts`): both pointer kinds for every case a tap drives.
  - `a real click and a real tap on a name draw its card above the entry`: the card's bottom at or above the entry's top.
  - `nothing drawn moves when the cards come in time`: the entry's top in its first drawn frame equals its top two frames later.
  - `a late characters answer moves nothing`: after an answer past the wait, the entry's top is unchanged and there is no card.
  - `the quote opens for a real click and a real tap`.
  - `at phone size the card stays inside the seat's share and scrolls inside it`, as `bookLookupSeat.browser.test.tsx`'s long-answer case does.
  - If the lit text widens: `the lit text covers the whole matched name` (the selection's text is the name, from the word tapped).
  - One case per entry of Step 1, each watched red against that declaration deleted or set to its nearest wrong value.
- [ ] **Step 3: The built app.** A `vite build`, and one browser check that the built CSS places the card as the dev build does (O218: the production CSS differs).
- [ ] **Step 4: Run** the browser project alone (no other lane running it), then node and jsdom with coverage, typecheck and the coverage floor.
- [ ] **Step 5: Commit.** "web: tests hold the character card's approved look". Report to the controller what the tests corrected in the prototype, for the owner.

### Task 10: The docs

**Files:** `docs/notes.md`, `docs/status.md`, `README.md`

**Inputs from the controller:** Task 8's figures and CLI trial, `rounds.md`, the approved commit, and the open-items numbers: O540 to O542, and O543 on for anything found (main's table and every live lane's claims checked first).

- [ ] **Step 1: `docs/notes.md`,** a section "Character cards from haiyaku", dated by the Pacific day:
  - **Filling the database, for now:** `taiyaku library backup --db <library.db>`; stop the server; run haiyaku's CLI of a version whose schema the deployed dependency knows (0.2.x, schema 2) with `--db <library.db>`: `extract` on the library's own EPUB (the file the library holds, so the digest is the member's), `series`, `link`; start the server. A newer CLI switches cards off until the dependency catches up, and the server's stderr says why. What Task 8's trial of a live write showed, and why the how-to still says to stop the server (FLAG 14). A log replaced after its titles were read shows the old ones until the server restarts (FLAG 4).
  - **What a fold costs:** Task 8's figures as measured (exact values, the build, the date), and what FLAG 10's rule decided.
  - **The prototype's rounds:** each round's ask and change, what was approved, and the wait.
- [ ] **Step 2: `README.md`.** The passages on the backup an open takes (its `-before-v<N>` name, the rule that skips a repeat, and the restore steps; about lines 218 to 227 and 830 to 850 on 2026-11-24) name the `-before-haiyaku-v<N>` backup too: taken before haiyaku's tables are first made or migrated, left alone by `--keep`, and restored by the same steps.
- [ ] **Step 3: `docs/status.md`, "What is built":** a paragraph on character cards (the route, the flag, the setting, cards off); `haiyaku` 0.2 with no default features in the dependency list, under `taiyaku-library`, with what the lock gained (Task 1 Step 1). The Rust test count at both of its sites, from the gate's JUnit (`scripts/status-test-count.mjs` checks it); the web `node` and `jsdom` counts from `npx vitest run --project node` and `--project jsdom` in `web/`, and the browser count, each measured (no gate checks them).
- [ ] **Step 4: Open O540, O541 and O542** in design §10's words, and any from O543 on, five edits each: the count word at the head of "Open items", the "leaving" sentence, the row (exactly six `|`; a `|` in a code span escaped), the prose bullet, and "What remains" (its count word wraps: grep the sentence's opener and read the next line). The ritual's sweep before and after (`grep -c '^| O[0-9]' docs/status.md` against the count word, `grep -c '^| O[0-9]* | \[x\]'` against the closed count, `grep -n -A1 '^What remains:'`), and each new number grepped bare across `docs/` and `web/src`.
- [ ] **Step 5: Every figure written is checked** against its log or request-log line; the pairs go in `$EV/t10-figures.md`.
- [ ] **Step 6: The gate,** as Finishing runs it, into `$EV/gate-t10.log`; its exit line and its `all green` line quoted.
- [ ] **Step 7: Commit.** "docs: character cards: how to fill them, what a fold costs, the prototype's rounds; opens O540 to O542".

### Finishing (controller)

- [ ] **The whole-branch review,** on the most capable model, with the design and this plan in hand: mutations of its own, different from the implementers', and a section "where I disagree with a per-task review or the ledger". Fixes go back to the implementer that wrote the task; a substantial fix round gets a review of the fixes.
- [ ] **The full gate,** from a detached REPL shell command, never from an agent: `env -C $W env -u TAIYAKU_CORPUS NEXTEST_TEST_THREADS=8 RUST_TEST_THREADS=8 CARGO_TARGET_DIR=$W/target scripts/check-all.sh >| $EV/gate-final.log 2>&1; echo "exit=$?" >> $EV/gate-final.log`. Read the exit line, the `all green` line and any `NOT MEASURED`; never through `tail`.
- [ ] **The mutation sweep,** one at a time on the machine: `env -C $W flock /run/user/1000/taiyaku-mutants.lock sh -c 'while pgrep -x cargo-mutants >/dev/null; do sleep 30; done; exec scripts/mutants.sh --since main' >| $EV/mutants.log 2>&1`. Then: no `ERROR` or `Worker thread failed` in it, and a `mutants tested` line. `target/mutants.out/outcomes.json` read for which of `characters::answer`, `choose_scope`, `books_seen`, `holds`, `Titles::of`, `from_connection`, `backup::stamp_of`, `stands_for`, `get_characters` and `document_at` got mutants, and `unviable.txt` read against them: `Answer` derives no `Default`, so `answer`'s whole-body mutant is unviable; hand-apply `answer` returning `Answer::unknown()` and watch Task 2's tests fail. The data-plumbing defects the tool cannot make, hand-applied and watched red: a JSON key renamed, `titleSex` left out, the flag's key left out, the `earlier` key read as always on. Each survivor closed with a test, or recorded `accepted` or `UNTRIAGED:`, with a dated row in `docs/mutants-baseline.md`'s run log. Afterwards `ps -eo pid,etime,args | rg mutants-scratch`, and kill what is left by PID.
- [ ] **The claim check** over `docs/notes.md`, `docs/status.md` and the PR body: every figure against its log, the open-items sweep, the bare O-numbers.
- [ ] **The PR, on the owner's word:** push, and open it through the gitea MCP. A title under 255 characters; the body one line per paragraph, no commit list, no attribution. It says what the NAS sees after the merge (design §7: haiyaku's empty tables, the setting, no card and no characters request) and names O540 to O549.

## FLAGs: assumptions the design does not settle

1. **Invented data stored in code, no sidecar file.** Design §8 has tests import an invented sidecar with `import_book`. Choice: build `BookLog` and `Series` in code and store them with `write_book` and `put_series` (public, not `#[non_exhaustive]`), so no binary fixture is checked in and each test makes its EPUB, its digest and its log together. Why: a sidecar would be a second copy of the same invented data to keep in step, and `import_book` is haiyaku's to test.
2. **The route names the edition:** `/api/works/{work}/{edition}/book/characters`, where the design's example has `/api/works/{work}/book/characters`. Why: the book's answer holds several editions, a block index counts one edition's blocks, and the image route beside it names the edition. The path is the builder's; flagged because the example differs.
3. **The backup's name and count.** `library-<stamp>-before-haiyaku-v<N>.db` when haiyaku alone has work to do. When taiyaku's own migrations are pending too, the one backup taiyaku takes today, named for taiyaku's version, covers both. A new library (taiyaku at 0) takes none, as today. `prune` keeps both kinds, and `stands_for` skips a repeat for haiyaku as for taiyaku (O263). A haiyaku backup that cannot be taken does not refuse the open, as a failed taiyaku one does: haiyaku's migrate is skipped and cards are off (ruled at the pre-flight, 2026-11-24). Why: one copy taken before either migration holds the database as it was, and the name says which migration it was taken for; and nothing haiyaku needs may keep the library from opening.
4. **Titles cached for the `Library`'s life.** `read_book` once per book at the first answer that needs it, kept by the `Library` (not a process-wide static, so tests do not share it), never refreshed. Why: §11 rules out `haiyaku_chapter`, haiyaku has no light read of titles and no log version, and the how-to stops the server for a fill, so a restart refreshes them. A log replaced under a running server after its titles were read shows the old ones until a restart; if live fills are ever allowed, a haiyaku item for a light titles read (or a log version) comes first.
5. **The text bound on both sides.** The server refuses a `text` over 128 UTF-16 units with 400, as `dictionary::lookup_at` refuses over its own bound; the client cuts to 128 units, or to 127 where the 128th starts a pair. Why: one bound, tested at both ends, and a lone surrogate makes `encodeURIComponent` throw.
6. **Query defaults.** `earlier` absent means on, the setting's default; only `0` and `1` are taken, anything else is 400; an empty `series` or `order` is absent; unknown keys are ignored. Why: a hand-typed URL behaves as the reader does, and a bad value is refused rather than guessed.
7. **The flag's shape:** `"characters": true` on an edition with data, no key otherwise. Why: the book's answer already leaves out `marks` and `figures` when empty.
8. **JSON details.** `CardName` carries `titleSex`; `from` is `"this"`, `"with"` or null; `matched.part` adds `"other"` for a `NamePart` after 0.2; `chapterTitle` is `""` for a chapter the log lacks; every key but `length` and `start` is always sent, nulls included, and `start`, where the name begins against the tapped text, is left out at 0. Why: the design carries every field of `Fact` and `Overturn` and leaves the names to the builder, and `"this"`/`"with"` gives a direction without sending haiyaku's entity ids.
9. **The hold.** 300 ms until the prototype sets it; a failed characters answer ends the hold at once and is logged with `console.warn`. Why: the design leaves the value to the prototype, and waiting out a known failure only delays the dictionary.
10. **The measurement's rules.** Connection: if the characters route's median `micros` with `earlier=1` in Goblet of Fire's last chapter is above the median of comic page requests in the same session, the fold moves to `Library::working()`. Cache: if that median is above the dictionary route's median by more than the wait (cards would be dropped), a cache keyed by book, chapter cut and scope is opened as a row and built in a task of its own. Why: the design has the builder decide "from the same measurement" without a threshold; these make the decision mechanical and checkable.
11. **"Could open", for the grant:** a digest is seen when a member holding it is one the account's access sees (`browse::member_under`), the rule browse counts by; whether its work is missing, or its edition read as a book, is not asked. Why: the member rule is the grant's boundary everywhere else.
12. **Cards off answers `known: false`,** the unknown book's shape, not a state of its own. Why: the client never asks while the book's answer carries no flag, so only a hand-typed URL, or a restart between the two requests, reaches it.
13. **The web types live in `web/src/book.ts`,** not `web/src/api.ts` as the design's file list says: `getBook`, `bookImageUrl` and `putPin` and their types are there.
14. **The how-to keeps "stop the server"** whatever Task 8's trial shows, because of FLAG 4. If the trial shows a live write is safe, Task 10 says so, and lifting the rule is the owner's call, together with a titles fix.
15. **The prototype's data:** Sorcerer's Stone, Chamber of Secrets and Goblet of Fire in both languages, at the haiyaku acceptance run's ranges, from the reply cache behind a `PATH` without `claude`, after their digests are checked against the corpus copies. Why: design §9 says "the Harry Potter books already extracted" without naming them; these are the ones the 2026-11-23 runs extracted.
16. **The plain card before the prototype:** Task 7's anchors and words are placeholders the prototype may move; Task 9 holds whatever was approved.
