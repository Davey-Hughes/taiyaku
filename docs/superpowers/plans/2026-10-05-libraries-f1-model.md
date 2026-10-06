# Libraries, F1 (the model): Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The store and the CLI know libraries. A library is a name and one
or more folders (today's `root` rows). Derivation and proposals stay inside a
library, and a grant names a library. The server, the web app and OPDS behave
exactly as today for a library of one folder, which is what every existing
root becomes.

**Architecture:** Migration 15 adds `library`, files every root under a
library of its own, rebuilds `work` so `content_key` is unique within a
library rather than across the store, gives `series` a library, and replaces
`user_root` with `user_library`. Three triggers keep the invariant "a library
always has a folder": a root inserted without a library gets one named for its
folder, and a library whose last root is deleted or moved away is deleted.
Derivation adds the library to `GroupKey` and scopes every lookup that finds
an existing series or work (`reconcile_series`, `held_work`,
`import::find_by_key`, `librarydoc::find_series_by_title`) to the group's
library. Proposals pair only series of one library. `Principal.roots` keeps
its meaning: `account::roots_for` resolves it as every folder of every
granted library, so every filter in `browse` and every route is unchanged.
The CLI gains `library libraries`, `scan --library NAME` and
`user grant-library`/`revoke-library`.

**Tech Stack:** Rust 2024 (workspace `rust-version` 1.85), rusqlite 0.38
against the system SQLite (3.53 on this machine), axum 0.8, cargo-nextest in
the gate. No web change.

## Global Constraints

- **Branch:** `libraries-f1`, cut from the head of `libraries` (the commit
  that adds this plan). If `libraries` has merged by the time you start, cut
  from `main` instead. Task 8's PR body says which.
- **The spec is `docs/superpowers/specs/2026-10-05-libraries-design.md`.**
  Read the decisions, §1, §2, §5, §6's CLI bullet, §11 and §12 before Task 1.
  Where this plan and the spec disagree, stop and ask; do not pick one. The
  plan's own disagreements are listed under "Spec corrections needed" below
  and are for the user, not for you.
- **No attribution lines anywhere**: not in commit messages, code comments or
  documentation.
- **Every command runs from `/home/davey/projects/taiyaku`.** Never `cd`: this
  shell's `cd` is zoxide and has landed in another repository. The Bash tool
  runs zsh with `noclobber`, so write scratch output to a new file name or use
  `>|`. Quote globs (`--include='*.rs'`). Never name a shell variable `path`
  (zsh ties it to `PATH`). Brace a variable before a colon (`${S}:x`).
- **Throttle every Rust test run**: `cargo test … -- --test-threads=8`, and
  `NEXTEST_TEST_THREADS=8 RUST_TEST_THREADS=8` for the gate. An unthrottled
  run on this 32-core machine has been killed for memory.
- **Every new test is watched failing with its defect put back by hand.** Copy
  the production file aside first (`cp X "$SCRATCH/X.orig"`), make the edit
  the step names, run the test, read the failure, restore with `cp` and prove
  the restore with `cmp`. Put the failure's first line in the task's report.
  If the named defect does **not** make the test fail, stop and report: do not
  weaken the defect or change the test to make it fail. Never `git stash`.
- **A hand defect in SQL changes what the statement means, never how many
  parameters it binds.** Turning `AND library_id = ?2` into `AND 1 = 1` while
  `?2` is still bound fails every test on the binding error and proves
  nothing. Write `AND (library_id = ?2 OR 1 = 1)` instead.
- **A task is done only when whole crates pass**, not the files it touched:
  `cargo test -p taiyaku-library -p taiyaku-serve -p taiyaku-cli --
  --test-threads=8`, plus `cargo clippy --workspace --all-targets
  --all-features -- -D warnings` and `cargo fmt --all -- --check`. A failure
  is "pre-existing" only if it fails the same way on `main` at `d1630cbb`,
  checked in a worktree, never against another commit on this branch.
- **No figure in a code comment that a later change would have to correct**:
  no counts of tests, call sites, works or libraries. A migration file is
  append-only, so a count of what that one file creates may stand.
- **The snippets below are a starting point.** The interfaces are exact; the
  test fixtures' plumbing (helper names, imports, `expect` texts) is yours to
  fix against the code as it is. Re-read every signature a step names before
  writing against it. If a step's *claim about existing code* is wrong, stop
  and report rather than work around it.
- **Evidence goes in `.superpowers/sdd/libraries-f1-evidence/`**, which is
  gitignored, never under `target/`. `target/` has been emptied mid-session
  before.
- **Never touch `/home/davey/temp/manga-test/`.** Task 7 copies its
  `library.db` into your session scratchpad and proves afterwards that the
  original is byte-identical. Its digest starts `d5ebc0c237865035`.
- **Line numbers given are at `d1630cbb` and are hints only.** Find the words.
  If the words are not there at all, stop and report.
- **Nothing outside F1 changes.** No web file, no route, no JSON shape. A
  defect found on the way goes in the task's report for Task 8's
  `docs/status.md` edit; do not fix it unless it blocks a step here.

## Spec corrections needed

The user should rule on these before or during execution. The plan is written
to the reading given here.

1. **§2.1: no `UNIQUE (library_id, sort_title)` on `series`.** The column has
   never been unique (`derive::reconcile_series`'s doc says so and why that is
   safe), so a database already holding two series of one title in one root
   would fail migration 15 and not open. It would also need `series` rebuilt,
   with `series_fts`'s triggers on `series_title` checked around it.
   `reconcile_series` is the only writer that mints a series from a scan, and
   it looks before it inserts. Proposed wording: "`series` gains `library_id`;
   `derive::reconcile_series` finds a series on `(library_id, sort_title)`".
2. **§2.1: `ON DELETE SET NULL`, not `CASCADE`, for `series.library_id` and
   `work.library_id`.** A library is deleted only when its last root goes, and
   by then `scan::apply_removal` has deleted every work that root alone
   accounted for and counted their positions and notes. A work still carrying
   the library at that point has material elsewhere (an imported edition), and
   CASCADE would delete it, positions and notes included, uncounted.
3. **§2.1: `UNIQUE (content_key, library_id)`, key first.** The same
   constraint as the spec's `(library_id, content_key)`, written key-first so
   the index it creates is the one `import::find_by_key` searches. Key-second,
   every lookup by key scans `work`, once per work a derive writes.
4. **§2.1's backfill needs two rules the spec does not give.** A work is filed
   under its **hub edition's** library: `hand::path_for` addresses a work's
   document off the hub edition's own root, so the work stays with its
   document. A series is filed where **most of its works** are, the lowest
   library id on a tie; a series holding no work stays NULL.
5. **§2.2 misses one series writer.** `librarydoc::apply_series` finds a
   series by any of its titles across the whole store
   (`find_series_by_title`) and mints one with no library. A hand document on
   a work in one library would join a same-titled series of another. F1
   scopes the lookup to the work's library and mints into it (Task 3).
6. **§2.2 point 5 needs a guard for proposals raised before F1, or left by
   F2's folder move.** A proposal whose subject and target are in different
   libraries is refused at accept, with a message saying so (Task 4).
7. **§2.1 and §5 need a rule for an empty library.** Migration 15's triggers
   delete a library when its last root is deleted or moved away, taking its
   grants with it, and give a root inserted without a library one of its own.
   `Principal.roots` empty means unrestricted, so a grant on a library with no
   folders would read as a grant on everything. See also decision A below.
8. **§6's `scan --add PATH [--library NAME]`.** Read literally, `--add
   /media/Comics/Volumes` with no `--library` would join an existing library
   called `Volumes`. F1 reads it as: without `--library`, the folder becomes a
   library of its own, named for its folder (or by its whole path when that
   name is taken); with `--library NAME`, it joins `NAME`, which is created if
   no library has that name. See decision B below.

## Decided by the user, 2026-10-05

A and B were put to the user with the recommendations below, and both were
accepted. C was raised by the controller from the consequence A's text names,
and the user chose to refuse. The eight spec corrections above are adopted.

- **A. A library is deleted when its last folder goes.** Recommendation: yes,
  as Komga and Stump do (each library there has a folder). The alternative, an
  empty library that keeps its name and grants, needs `roots_for` to fail
  closed for a grant that resolves to no folder, and F2's screen then has a
  library card with nothing to scan. Consequence either way, and already
  true of roots: removing a restricted account's only granted library widens
  that account to everything (`account::revoke_root`'s doc says so today).
- **B. `scan --add PATH` with no `--library` makes a new library.**
  Recommendation: yes (correction 8). Joining a same-named library silently
  would merge unrelated folders.
- **C. Removing a library that is a restricted account's only grant is
  refused.** Decided 2026-10-05. A's consequence (the account widening to
  everything) is not allowed to happen silently: removing or moving away the
  last folder of a library, when some account's grants consist of that library
  alone, is refused with a message naming those accounts, so an administrator
  re-grants them first. Task 5, Step 2b.

## Decided while planning

- **Each migrated library takes its root's id.** So `root.library_id` is
  `root.id` and `user_root`'s rows move across with no join. Every test that
  asserts the mapping does so by name, never by id, because the two ids are
  equal by construction.
- **`root.library_id` stays nullable**, as §2.1 says, and is never NULL after
  any write: the `root_gets_a_library` trigger files a root inserted without
  one. That keeps every direct `INSERT INTO root (id, path)` in the test suite
  working unchanged, each root a library of its own, and leaves
  `scan::add_root`'s SQL as it is. `scan::roots` reads the column as `i64`, so a NULL there is a
  loud error, not a silent unrestricted grant.
- **A library is created only together with a folder.** `libraries::create`
  is `pub(crate)`. The public way to make one is `scan::add_root_in` or
  `scan::add_root_checked_in` with `InLibrary::Named`, which create the
  library and insert the root in one transaction, after the checked path's
  validations have passed. F2's create-library route should go through the
  same call.
- **Rows in no library are adopted.** `reconcile_series`, `held_work` and
  `find_by_key` prefer their own library's row and then accept one in no
  library, stamping it with theirs. Migration 15 leaves workless series NULL,
  and a library emptied by a folder move leaves its works NULL (`SET NULL`).
  Adoption keeps their ids, so a reader's positions, notes and the slug stay
  where they are.
- **`import::scanned` keeps its signature** and means "a scanned work in no
  library", with today's store-wide content lookup. Its callers outside
  `derive` are test fixtures building works from a document, in all three
  crates. `derive` calls the new `import::scanned_in`.
- **A document import keeps a store-wide content lookup.** Two libraries can
  now hold a work with one key; `find_by_key` with no library prefers a work
  in a library over one in none, then the oldest.
- **`GroupKey`'s library is its last element**, and `named_series` is keyed
  `(title, library)`. A library's groups then sort exactly as a store's did,
  so series and work rows are minted in the order they always were, and
  fixtures that rely on that order (`router_over_two_scanned_volumes_of_one_series_across_two_roots_and_an_unrelated_series`
  is one) keep relying on it.
- **Slugs stay unique across the whole store.** `/api/works/{slug}` and every
  OPDS link name a work by slug alone. `import::unique_slug` already checks
  every work, and the rebuilt `work` keeps `slug UNIQUE`. Two byte-identical
  volumes in two libraries get two slugs (Task 3 pins it).
- **`account::grant_root` and `revoke_root` go**, replaced by
  `grant_library`, `revoke_library`, `libraries_for` and
  `granted_libraries`. `roots_for` and `granted_roots` stay, resolved through
  libraries, because the guard and the Users API read them.
- **The Users API keeps its shape** (`roots` in, `roots` out), and
  `account::update` diffs at the library level. This is the one behaviour
  change the web can see, and it needs a library with two folders, which only
  the CLI can make in F1: ticking one folder of such a library grants the
  whole library, and the screen then shows both folders ticked. A root-level
  diff would instead revoke the whole library when one of its folders was
  unticked.
- **Metadata and stamp routes pass `principal.roots` and get no grant test.**
  Every handler in `metadata.rs` and `stamp.rs` that reads it is behind
  `session::administering_or_forbidden`, and an administrator's roots are
  always empty (O86).
- **`--root` and `--library` refuse each other.** Each chooses which folders a
  scan walks.
- **`grant-root` and `revoke-root` answer with their replacement's name**
  rather than "no subcommand".

## File structure

| File | Change | Responsibility |
| --- | --- | --- |
| `crates/taiyaku-library/src/schema/015_libraries.sql` | create | `library`, `user_library`, the `work` rebuild and its six recreated triggers, the backfill, three new triggers on `root` |
| `crates/taiyaku-library/src/schema.rs` | modify | `SCHEMA_VERSION` 14 → 15, `MIGRATIONS` |
| `crates/taiyaku-library/src/schema/tests.rs` | modify | migration 15's tests; version fifteen; migration 4's test stops at 4 |
| `crates/taiyaku-library/src/libraries.rs` | create | `Listed`, `list`, `id_of`, `create` (crate), `unfiled_works` |
| `crates/taiyaku-library/src/libraries/tests.rs` | create | listing and naming |
| `crates/taiyaku-library/src/lib.rs` | modify | `pub mod libraries;` |
| `crates/taiyaku-library/src/scan.rs` | modify | `Root::library_id`, `InLibrary`, `add_root_in`, `add_root_checked_in`; `apply_removal`'s doc |
| `crates/taiyaku-library/src/scan/tests.rs` | modify | the add-to-a-library cases |
| `crates/taiyaku-library/src/derive.rs` | modify | `GroupKey`, `Group`, `Sorting`, `reconcile_series`, `held_work`, `write_work`, `rel_paths_*` |
| `crates/taiyaku-library/src/derive/tests.rs` | modify | the library cases; two-folder fixtures into one library |
| `crates/taiyaku-library/src/import.rs` | modify | `scanned_in`, `find_by_key`, `insert_work`, `update_work` |
| `crates/taiyaku-library/src/import/tests.rs` | modify | the scoped lookup's cases |
| `crates/taiyaku-library/src/librarydoc.rs` | modify | `apply_series`, `find_series_by_title` |
| `crates/taiyaku-library/src/proposals.rs` | modify | `SeriesRow`, the three raisers, `series_id_by_sort_title`, `series_reached_by`, `preview`, `evidence`, the cross-library refusal |
| `crates/taiyaku-library/src/proposals/tests.rs` | modify | the within-a-library cases |
| `crates/taiyaku-library/src/account.rs` | modify | `user_library`; the library-level grant functions; `update`'s diff |
| `crates/taiyaku-library/src/account/tests.rs` | modify | grants through libraries |
| `crates/taiyaku-library/src/rules/tests.rs` | modify | `fn root()` gains `library_id` |
| `crates/taiyaku-serve/src/test_support.rs` | modify | `grant_root` onto `user_library`; two-folder fixtures into one library; the two-folder grant fixture |
| `crates/taiyaku-serve/src/{auth,library_api,opds,routes,users_api}/tests.rs` | modify | `user_library` in four auth sites; one grant test per family |
| `crates/taiyaku-cli/src/library.rs`, `library/tests.rs` | modify | `libraries`, `scan --library`, `select_roots`, dispatch text |
| `crates/taiyaku-cli/src/user.rs`, `user/tests.rs` | modify | `grant-library`/`revoke-library` |
| `crates/taiyaku-cli/src/main.rs` | modify | usage lines |
| `crates/taiyaku-cli/tests/cli.rs` | modify | `libraries`, `scan --library`, usage |
| comments naming `user_root` (`auth.rs`, `routes.rs`, `browse.rs`, `scan.rs`) | modify | say `user_library` |
| `docs/status.md`, `docs/notes.md`, `README.md`, `docs/mutants-baseline.md` | modify | synopsis, O54/O60, the head paragraph, evidence, sweep row |

---

## Task 1: Migration 15

**Files:**
- Create: `crates/taiyaku-library/src/schema/015_libraries.sql`
- Modify: `crates/taiyaku-library/src/schema.rs`
- Modify: `crates/taiyaku-library/src/schema/tests.rs`
- Modify: `crates/taiyaku-library/src/account.rs` (five statements: `roots_for`, `grant_root`, `revoke_root`, `granted_roots`, `count_for`)
- Modify: `crates/taiyaku-library/src/account/tests.rs` (`roots_are_read_per_user`'s insert; the table list in `removing_an_account_takes_its_positions_notes_sessions_keys_and_grants`)
- Modify: `crates/taiyaku-serve/src/test_support.rs` (`grant_root`)
- Modify: `crates/taiyaku-serve/src/auth/tests.rs` (four `INSERT INTO user_root`)

**Interfaces:**
- Produces: tables `library (id, name UNIQUE, added)` and `user_library
  (user_id, library_id)`; columns `root.library_id`, `series.library_id`,
  `work.library_id`; `work` unique on `(content_key, library_id)` and on
  `slug`; triggers `root_gets_a_library`, `library_emptied_by_delete`,
  `library_emptied_by_move`; `SCHEMA_VERSION == 15`. `user_root` no longer
  exists.
- `account::grant_root(conn, user, root)` now grants the root's library and
  `revoke_root` revokes it: identical behaviour while every library has one
  folder. Task 5 replaces both.

- [ ] **Step 1: Write the migration**

Create `crates/taiyaku-library/src/schema/015_libraries.sql`. The SQL below
was run while planning, inside one transaction with foreign keys off as
`migrate_with` runs it, over a seeded schema-14 database and over a copy of
the real library brought from 13 to 15: `pragma_foreign_key_check` was empty,
every work's id, slug, key and title came across unchanged, and
`work_fts`'s `integrity-check` passed.

```sql
-- Part F1 of docs/superpowers/specs/2026-10-05-libraries-design.md (§2.1): a
-- library is a name and one or more folders, and a folder is the `root` row it
-- always was.
--
-- **One library per existing root**, spec decision 1. A work that gathers
-- editions from two roots today is split by the next derive, one work per
-- library; the decision accepts that and says why. Each library takes its
-- root's id, so `root.library_id` is `root.id` and `user_root`'s rows move
-- across unchanged.
--
-- **A library is named by `browse::status`'s rule**: the folder's last path
-- component, or the whole path where two roots share one. `rtrim(path, <every
-- character of the path except '/'>)` leaves the path up to and including its
-- last '/', and what follows is the component. A path with nothing after its
-- last '/' — only `/` itself — is named by the whole path.
--
-- **`work` is rebuilt**, because SQLite cannot drop a column's UNIQUE in
-- place. `content_key` was unique across the store and is now unique within a
-- library (spec decision 2), so a byte-identical volume in two libraries is two
-- works. Written key first, so the index the constraint builds is also the one
-- `import::find_by_key` searches. `slug` stays unique across the store:
-- `/api/works/{slug}` and every OPDS link name a work by slug alone. The copy
-- keeps every id, and the index and six triggers `DROP TABLE work` takes with
-- it are recreated below: migration 008's three that feed `work_fts`, and
-- migration 014's three that move the structure counter. `work_fts` reads
-- `work` by id as an external-content table, so its index stays valid.
--
-- **A work is filed under its hub edition's library.** `hand::path_for`
-- addresses a work's document off the hub edition's own root, so the work stays
-- with its document. A hub with no member under a root falls back to the work's
-- lowest root; a work with no member under any root — an imported
-- `taiyaku.json` — stays NULL, in no library, which is how `browse` already
-- treats it.
--
-- **A series is filed where most of its works are**, the lowest library id on
-- a tie. A series holding no work stays NULL, and `derive::reconcile_series`
-- adopts it into the first library whose files name it.
--
-- **No UNIQUE on `(library_id, sort_title)`.** `sort_title` has never been
-- unique (`derive::reconcile_series` says why that is safe), and a database
-- holding two series of one title would fail this migration rather than open.
--
-- **`ON DELETE SET NULL`, not CASCADE, on `work` and `series`.** A library goes
-- only with its last root (the triggers at the end), and `scan::apply_removal`
-- has by then deleted every work that root alone accounted for and counted what
-- went with them. A work still carrying the library has material elsewhere, an
-- imported edition, and CASCADE would delete it uncounted.
--
-- **`root.library_id` stays nullable**, because SQLite cannot add a NOT NULL
-- column without a default to a table that has rows. It is never NULL after a
-- write: `root_gets_a_library` files a root inserted without a library under a
-- library of its own, named by the rule above.
--
-- **A library never outlives its last root.** The two `library_emptied_*`
-- triggers delete it when its last root is deleted or moved away, and its
-- grants go with it through `user_library`'s CASCADE. A grant naming a library
-- with no root would resolve to no roots, and `Principal.roots` reads no roots
-- as unrestricted.
CREATE TABLE library (
  id    INTEGER PRIMARY KEY,
  name  TEXT NOT NULL UNIQUE,
  added TEXT NOT NULL DEFAULT (datetime('now'))
);

WITH named AS (
  SELECT id, path, added,
         substr(path, length(rtrim(path, replace(path, '/', ''))) + 1) AS leaf
    FROM root
)
INSERT INTO library (id, name, added)
SELECT id,
       CASE WHEN leaf = ''
              OR (SELECT count(*) FROM named other WHERE other.leaf = named.leaf) > 1
            THEN path ELSE leaf END,
       added
  FROM named;

ALTER TABLE root ADD COLUMN library_id INTEGER REFERENCES library(id);
UPDATE root SET library_id = id;

ALTER TABLE series ADD COLUMN library_id INTEGER REFERENCES library(id) ON DELETE SET NULL;

CREATE TABLE work_new (
  id             INTEGER PRIMARY KEY,
  content_key    TEXT NOT NULL,
  slug           TEXT NOT NULL UNIQUE,
  series_id      INTEGER REFERENCES series(id) ON DELETE SET NULL,
  volume         REAL,
  title          TEXT NOT NULL,
  reading        TEXT NOT NULL,
  hub            TEXT NOT NULL,
  sidecar_path   TEXT,
  sidecar_mtime  INTEGER,
  imported       TEXT NOT NULL DEFAULT (datetime('now')),
  title_hand     TEXT,
  title_derived  TEXT,
  volume_hand    REAL,
  series_id_hand INTEGER REFERENCES series(id) ON DELETE SET NULL,
  library_id     INTEGER REFERENCES library(id) ON DELETE SET NULL,
  UNIQUE (content_key, library_id)
);
INSERT INTO work_new (id, content_key, slug, series_id, volume, title, reading, hub,
                      sidecar_path, sidecar_mtime, imported, title_hand, title_derived,
                      volume_hand, series_id_hand, library_id)
SELECT w.id, w.content_key, w.slug, w.series_id, w.volume, w.title, w.reading, w.hub,
       w.sidecar_path, w.sidecar_mtime, w.imported, w.title_hand, w.title_derived,
       w.volume_hand, w.series_id_hand,
       COALESCE(
         (SELECT r.library_id
            FROM edition e
            JOIN member m ON m.edition_id = e.id
            JOIN root r ON r.id = m.root_id
           WHERE e.work_id = w.id AND e.key = w.hub
           ORDER BY m.ordinal
           LIMIT 1),
         (SELECT r.library_id
            FROM edition e
            JOIN member m ON m.edition_id = e.id
            JOIN root r ON r.id = m.root_id
           WHERE e.work_id = w.id
           ORDER BY r.id
           LIMIT 1))
  FROM work w;
DROP TABLE work;
ALTER TABLE work_new RENAME TO work;

CREATE INDEX work_by_series ON work (series_id);
CREATE TRIGGER work_ai AFTER INSERT ON work BEGIN
  INSERT INTO work_fts (rowid, title) VALUES (new.id, new.title);
END;
CREATE TRIGGER work_ad AFTER DELETE ON work BEGIN
  INSERT INTO work_fts (work_fts, rowid, title) VALUES ('delete', old.id, old.title);
END;
CREATE TRIGGER work_au AFTER UPDATE ON work BEGIN
  INSERT INTO work_fts (work_fts, rowid, title) VALUES ('delete', old.id, old.title);
  INSERT INTO work_fts (rowid, title) VALUES (new.id, new.title);
END;
CREATE TRIGGER structure_work_ins AFTER INSERT ON work BEGIN
  UPDATE structure_version SET n = n + 1;
END;
CREATE TRIGGER structure_work_upd AFTER UPDATE ON work BEGIN
  UPDATE structure_version SET n = n + 1;
END;
CREATE TRIGGER structure_work_del AFTER DELETE ON work BEGIN
  UPDATE structure_version SET n = n + 1;
END;

UPDATE series SET library_id = (
  SELECT w.library_id
    FROM work w
   WHERE w.series_id = series.id AND w.library_id IS NOT NULL
   GROUP BY w.library_id
   ORDER BY count(*) DESC, w.library_id
   LIMIT 1
);

CREATE TABLE user_library (
  user_id    INTEGER NOT NULL REFERENCES user(id) ON DELETE CASCADE,
  library_id INTEGER NOT NULL REFERENCES library(id) ON DELETE CASCADE,
  PRIMARY KEY (user_id, library_id)
) WITHOUT ROWID;
INSERT INTO user_library (user_id, library_id)
SELECT ur.user_id, r.library_id FROM user_root ur JOIN root r ON r.id = ur.root_id;
DROP TABLE user_root;

-- `last_insert_rowid()` inside a trigger is the row the trigger inserted, and
-- reverts to the caller's when the trigger ends, so `scan::add_root`'s own
-- `last_insert_rowid()` after `INSERT INTO root` is still the root's id.
CREATE TRIGGER root_gets_a_library AFTER INSERT ON root
WHEN new.library_id IS NULL
BEGIN
  INSERT INTO library (name) VALUES (
    CASE
      WHEN substr(new.path, length(rtrim(new.path, replace(new.path, '/', ''))) + 1) = ''
        OR EXISTS (SELECT 1 FROM library
                    WHERE name = substr(new.path,
                                        length(rtrim(new.path, replace(new.path, '/', ''))) + 1))
      THEN new.path
      ELSE substr(new.path, length(rtrim(new.path, replace(new.path, '/', ''))) + 1)
    END
  );
  UPDATE root SET library_id = last_insert_rowid() WHERE id = new.id;
END;

CREATE TRIGGER library_emptied_by_delete AFTER DELETE ON root
WHEN old.library_id IS NOT NULL
 AND NOT EXISTS (SELECT 1 FROM root WHERE library_id = old.library_id)
BEGIN
  DELETE FROM library WHERE id = old.library_id;
END;
CREATE TRIGGER library_emptied_by_move AFTER UPDATE OF library_id ON root
WHEN old.library_id IS NOT NULL
 AND old.library_id IS NOT new.library_id
 AND NOT EXISTS (SELECT 1 FROM root WHERE library_id = old.library_id)
BEGIN
  DELETE FROM library WHERE id = old.library_id;
END;
```

In `crates/taiyaku-library/src/schema.rs`: `pub const SCHEMA_VERSION: u32 = 15;`,
`const MIGRATIONS: [&str; 15]`, and after the `014_structure_version.sql` line:

```rust
    include_str!("schema/015_libraries.sql"),
```

- [ ] **Step 2: Move the five `user_root` statements in `account.rs`**

Each keeps its signature. While every library has one folder, each answers
exactly what it answered before.

- `roots_for`:
  ```sql
  SELECT r.id FROM user_library ul JOIN root r ON r.library_id = ul.library_id
   WHERE ul.user_id = ?1 ORDER BY r.id
  ```
- `grant_root`: first read the root's library, refusing a root no row holds
  (the old statement refused it through the foreign key):
  ```rust
  let library: Option<i64> = conn
      .query_row("SELECT library_id FROM root WHERE id = ?1", params![root_id], |row| row.get(0))
      .optional()
      .map_err(|e| format!("granting root {root_id} to user {user_id}: {e}"))?;
  let Some(library) = library else {
      return Err(format!("granting root {root_id} to user {user_id}: there is no such root"));
  };
  let changed = conn
      .execute(
          "INSERT OR IGNORE INTO user_library (user_id, library_id) VALUES (?1, ?2)",
          params![user_id, library],
      )
      .map_err(|e| format!("granting root {root_id} to user {user_id}: {e}"))?;
  Ok(changed == 1)
  ```
- `revoke_root`:
  ```sql
  DELETE FROM user_library
   WHERE user_id = ?1 AND library_id = (SELECT library_id FROM root WHERE id = ?2)
  ```
- `granted_roots`:
  ```sql
  SELECT r.id, r.path FROM user_library ul JOIN root r ON r.library_id = ul.library_id
   WHERE ul.user_id = ?1 ORDER BY r.path
  ```
- `count_for`: `(SELECT COUNT(*) FROM user_library WHERE user_id = ?1)`.

Update the doc comments of all five where they name `user_root`, and the
comment in `update`'s roots branch that does ("while `user_root` carries no
such foreign key", "`user_root.root_id` REFERENCES `root(id)`"): those
sentences now describe `user_library`, and the existence check they explain
is still needed. Task 5 rewrites this block; keep the edit small.

- [ ] **Step 3: Move the tests that write `user_root` directly**

- `test_support::grant_root`: replace the insert with
  ```rust
  let granted = conn
      .execute(
          "INSERT INTO user_library (user_id, library_id)
           SELECT 1, library_id FROM root WHERE id = ?1",
          [root_id],
      )
      .expect("grants the fixture account the root's library");
  assert_eq!(
      granted, 1,
      "root {root_id} names no root, so nothing was granted and the account \
       is unrestricted, which no grant test can mean"
  );
  ```
  and add to its doc: "A root is a folder of a library, and a grant names the
  library. Every fixture root is a library of its own unless it was filed
  under a named one, so for those this grants exactly that root."
- `auth/tests.rs`, four sites of
  `INSERT INTO user_root (user_id, root_id) VALUES (1, 1)`: replace each with
  `INSERT INTO user_library (user_id, library_id) SELECT 1, library_id FROM root WHERE id = 1`.
  Grep `user_root` under `crates/` afterwards; code hits must be zero, and
  comment hits are Task 8's sweep.
- `account/tests.rs`: `roots_are_read_per_user` writes the grant the same
  way, for root 7; the removal test's table list says `user_library` where it
  said `user_root`.

- [ ] **Step 4: Write the migration's tests**

In `crates/taiyaku-library/src/schema/tests.rs`:

1. Rename `the_schema_version_is_fourteen` to `the_schema_version_is_fifteen`
   and assert `15`.
2. `migration_4_creates_every_table_spec_a2_names` runs every migration and
   then expects `user_root`, which migration 15 drops. Make it stop where its
   name says: `migrate_with(&conn, &MIGRATIONS[..4])` in place of
   `migrate(&conn)`. Its `can_download`/`can_stream` read is valid at 4.
3. Add the migration-15 cases. They plant rows at schema 14 and migrate
   after, the convention `migration_9_rebuild_carries_a_pre_existing_position_across_intact`
   follows, because the migration's risk is what happens to rows that existed
   before it.

```rust
/// A schema-14 library for migration 15 to file, chosen so each backfill rule
/// gives a different answer from the rule it could be mistaken for.
///
/// - Roots 1 and 2 share the last component `Volumes`, so both are named by
///   their whole path; root 3's `Chapters` is not shared.
/// - Work 20's hub (`raw`) is under root 2 and its `en` edition under root 1:
///   the hub rule says root 2's library, the lowest-root rule says root 1's.
/// - Work 60's hub member is under no root (an imported edition) and its
///   `scan` edition is under root 1: only the fallback files it.
/// - Work 50 is wholly imported and stays in no library.
/// - Series 2 holds work 20 (root 2) and works 30 and 40 (root 3): its
///   first work's library and its lowest library are root 2's, and the
///   library most of its works are in is root 3's.
/// - Series 3 holds nothing.
const SCHEMA_14_LIBRARY: &str = "
    INSERT INTO user (id, name, password_hash, role) VALUES (2, 'reader', '!', 'user');
    INSERT INTO root (id, path) VALUES
      (1, '/srv/a/Volumes'), (2, '/srv/b/Volumes'), (3, '/srv/Chapters');
    INSERT INTO series (id, sort_title) VALUES (1, 'Alpha'), (2, 'Beta'), (3, 'Orphan');
    INSERT INTO work (id, content_key, slug, series_id, volume, title, reading, hub, sidecar_path)
    VALUES (10, 'k-alpha-1', 'alpha-v01', 1, 1, 'Alpha v01', 'rtl', 'raw', NULL),
           (20, 'k-beta-1',  'beta-v01',  2, 1, 'Beta v01',  'rtl', 'raw', NULL),
           (30, 'k-beta-2',  'beta-v02',  2, 2, 'Beta v02',  'rtl', 'raw', NULL),
           (40, 'k-beta-3',  'beta-v03',  2, 3, 'Beta v03',  'rtl', 'raw', NULL),
           (50, 'k-imported', 'imported', NULL, NULL, 'Imported', 'ltr', 'raw',
                '/docs/imported/taiyaku.json'),
           (60, 'k-merged',  'merged',    1, 2, 'Alpha v02', 'rtl', 'raw',
                '/docs/merged/taiyaku.json');
    INSERT INTO edition (id, work_id, key, label) VALUES
      (100, 10, 'raw', 'raw'),
      (200, 20, 'raw', 'raw'), (201, 20, 'en', 'en'),
      (300, 30, 'raw', 'raw'),
      (400, 40, 'raw', 'raw'),
      (500, 50, 'raw', 'raw'),
      (600, 60, 'raw', 'raw'), (601, 60, 'scan', 'scan');
    INSERT INTO member (id, edition_id, ordinal, path, sha256, pages, root_id) VALUES
      (1, 100, 0, '/srv/a/Volumes/Alpha/v01.cbz',    'a1',  10, 1),
      (2, 200, 0, '/srv/b/Volumes/Beta/v01.cbz',     'b1',  10, 2),
      (3, 201, 0, '/srv/a/Volumes/Beta/v01-en.cbz',  'b1e', 10, 1),
      (4, 300, 0, '/srv/Chapters/Beta/v02.cbz',      'b2',  10, 3),
      (5, 400, 0, '/srv/Chapters/Beta/v03.cbz',      'b3',  10, 3),
      (6, 500, 0, '/docs/imported/v.cbz',            'i1',  10, NULL),
      (7, 600, 0, '/docs/merged/v.cbz',              'm1',  10, NULL),
      (8, 601, 0, '/srv/a/Volumes/Alpha/v02.cbz',    'm1s', 10, 1);
    INSERT INTO progress (user_id, work_id, edition, slot) VALUES
      (1, 10, 'raw', 3), (1, 20, 'en', 4), (2, 50, 'raw', 5);
    INSERT INTO note (user_id, id, work_id, edition, slot, body, created, updated)
    VALUES (1, 'n1', 30, 'raw', 2, 'hello', 'now', 'now');
    INSERT INTO user_root (user_id, root_id) VALUES (2, 1), (2, 3);
";

/// [`SCHEMA_14_LIBRARY`] at version 14, foreign keys on as `Library::open`
/// sets them.
fn a_schema_14_library() -> Connection {
    let conn = Connection::open_in_memory().expect("in-memory connection");
    conn.pragma_update(None, "foreign_keys", "ON")
        .expect("foreign keys on, as `Library::open` sets them");
    migrate_with(&conn, &MIGRATIONS[..14]).expect("migrates to 14");
    conn.execute_batch(SCHEMA_14_LIBRARY)
        .expect("seeds a schema-14 library");
    conn
}

/// Every row of `sql`'s answer, each column rendered as text, so two reads
/// can be compared whole.
fn rows_of(conn: &Connection, sql: &str) -> Vec<Vec<String>> {
    let mut stmt = conn.prepare(sql).expect(sql);
    let width = stmt.column_count();
    stmt.query_map([], |row| {
        (0..width)
            .map(|i| {
                let value: rusqlite::types::Value = row.get(i)?;
                Ok(format!("{value:?}"))
            })
            .collect::<rusqlite::Result<Vec<_>>>()
    })
    .expect(sql)
    .collect::<rusqlite::Result<Vec<_>>>()
    .expect(sql)
}

/// The library each named thing is filed under, by name; `None` for none.
fn library_names(conn: &Connection, sql: &str) -> Vec<(i64, Option<String>)> {
    let mut stmt = conn.prepare(sql).expect(sql);
    stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .expect(sql)
        .collect::<rusqlite::Result<Vec<_>>>()
        .expect(sql)
}

#[test]
fn migration_15_files_every_root_under_a_library_named_for_its_folder() {
    let conn = a_schema_14_library();
    migrate(&conn).expect("migrates to 15");
    assert_eq!(
        library_names(
            &conn,
            "SELECT r.id, l.name FROM root r LEFT JOIN library l ON l.id = r.library_id ORDER BY r.id"
        ),
        vec![
            (1, Some("/srv/a/Volumes".to_string())),
            (2, Some("/srv/b/Volumes".to_string())),
            (3, Some("Chapters".to_string())),
        ],
        "two folders called Volumes are named by their whole paths; Chapters by its own"
    );
}

#[test]
fn migration_15_files_each_work_under_its_hub_editions_library() {
    let conn = a_schema_14_library();
    migrate(&conn).expect("migrates to 15");
    assert_eq!(
        library_names(
            &conn,
            "SELECT w.id, l.name FROM work w LEFT JOIN library l ON l.id = w.library_id ORDER BY w.id"
        ),
        vec![
            (10, Some("/srv/a/Volumes".to_string())),
            (20, Some("/srv/b/Volumes".to_string())),
            (30, Some("Chapters".to_string())),
            (40, Some("Chapters".to_string())),
            (50, None),
            (60, Some("/srv/a/Volumes".to_string())),
        ],
        "20 follows its hub, not its lowest root; 60 falls back to its scanned edition; \
         50 is imported and in no library"
    );
}

#[test]
fn migration_15_files_each_series_where_most_of_its_works_are() {
    let conn = a_schema_14_library();
    migrate(&conn).expect("migrates to 15");
    assert_eq!(
        library_names(
            &conn,
            "SELECT s.id, l.name FROM series s LEFT JOIN library l ON l.id = s.library_id ORDER BY s.id"
        ),
        vec![
            (1, Some("/srv/a/Volumes".to_string())),
            (2, Some("Chapters".to_string())),
            (3, None),
        ],
        "Beta's first work and its lowest library are /srv/b/Volumes; two of its three \
         works are in Chapters"
    );
}

#[test]
fn migration_15_keeps_every_work_position_and_note_where_it_was() {
    let conn = a_schema_14_library();
    let works = "SELECT id, content_key, slug, series_id, volume, title, reading, hub, \
                 sidecar_path, title_hand, title_derived, volume_hand, series_id_hand \
                 FROM work ORDER BY id";
    let progress = "SELECT user_id, work_id, edition, slot FROM progress ORDER BY user_id, work_id";
    let notes = "SELECT user_id, id, work_id, edition, slot, body FROM note ORDER BY id";
    let before = (rows_of(&conn, works), rows_of(&conn, progress), rows_of(&conn, notes));

    migrate(&conn).expect("migrates to 15");

    assert_eq!(rows_of(&conn, works), before.0, "the rebuild changed a work");
    assert_eq!(rows_of(&conn, progress), before.1, "a reading position moved or went");
    assert_eq!(rows_of(&conn, notes), before.2, "a note moved or went");
}

#[test]
fn migration_15_moves_each_root_grant_to_that_roots_library() {
    let conn = a_schema_14_library();
    migrate(&conn).expect("migrates to 15");
    assert_eq!(
        library_names(
            &conn,
            "SELECT ul.user_id, l.name FROM user_library ul JOIN library l ON l.id = ul.library_id \
             ORDER BY l.name"
        ),
        vec![
            (2, Some("/srv/a/Volumes".to_string())),
            (2, Some("Chapters".to_string())),
        ]
    );
    assert!(
        !table_names(&conn).iter().any(|name| name == "user_root"),
        "user_root is gone"
    );
}

/// `DROP TABLE work` takes migration 008's search triggers and migration 014's
/// counter triggers with it. `structure::tests` checks the counter's on a
/// fresh database; this checks both over rows that predate the rebuild.
#[test]
fn migration_15_leaves_search_and_the_structure_counter_working_on_work() {
    let conn = a_schema_14_library();
    migrate(&conn).expect("migrates to 15");

    let matching = |term: &str| -> Vec<i64> {
        let mut stmt = conn
            .prepare("SELECT rowid FROM work_fts WHERE work_fts MATCH ?1 ORDER BY rowid")
            .expect("prepares");
        stmt.query_map([term], |r| r.get(0))
            .expect("searches")
            .collect::<rusqlite::Result<_>>()
            .expect("reads")
    };
    assert_eq!(matching("Beta"), vec![20, 30, 40], "the index survived the rebuild");

    let before = crate::structure::version(&conn).expect("reads the counter");
    conn.execute("UPDATE work SET title = 'Gamma v02' WHERE id = 30", [])
        .expect("renames a work");
    assert_eq!(matching("Gamma"), vec![30], "work_au is back");
    assert!(
        crate::structure::version(&conn).expect("reads the counter") > before,
        "structure_work_upd is back"
    );
    conn.execute(
        "INSERT INTO work_fts (work_fts) VALUES ('integrity-check')",
        [],
    )
    .expect("the search index agrees with work");
}

/// Ids for the trigger cases, with library ids that differ from root ids so a
/// statement joining the wrong pair cannot pass by coincidence.
const TWO_FOLDER_LIBRARY: &str = "
    INSERT INTO library (id, name) VALUES (70, 'Shelf'), (80, 'Other');
    INSERT INTO root (id, path, library_id) VALUES
      (7, '/srv/manga', 70), (8, '/srv/raw', 70), (9, '/srv/other', 80);
";

#[test]
fn a_root_written_without_a_library_is_filed_under_one_of_its_own() {
    let conn = Connection::open_in_memory().expect("in-memory connection");
    migrate(&conn).expect("migrates");
    conn.execute_batch(
        "INSERT INTO root (path) VALUES ('/x/Extras');
         INSERT INTO root (path) VALUES ('/y/Extras');
         INSERT INTO root (path) VALUES ('/');",
    )
    .expect("three roots, two sharing a last component");
    assert_eq!(
        library_names(
            &conn,
            "SELECT r.id, l.name FROM root r LEFT JOIN library l ON l.id = r.library_id ORDER BY r.id"
        ),
        vec![
            (1, Some("Extras".to_string())),
            (2, Some("/y/Extras".to_string())),
            (3, Some("/".to_string())),
        ],
        "a taken name falls back to the whole path, and so does a path with no last component"
    );
}

#[test]
fn a_library_goes_with_its_last_root_and_takes_its_grants_with_it() {
    let library = crate::Library::open_in_memory().expect("library");
    let conn = library.conn();
    conn.execute_batch(TWO_FOLDER_LIBRARY).expect("seeds");
    conn.execute("INSERT INTO user_library (user_id, library_id) VALUES (1, 70)", [])
        .expect("grants Shelf");

    conn.execute("DELETE FROM root WHERE id = 7", []).expect("removes one folder");
    let shelf = |conn: &Connection| -> (i64, i64) {
        conn.query_row(
            "SELECT (SELECT count(*) FROM library WHERE id = 70),
                    (SELECT count(*) FROM user_library WHERE library_id = 70)",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .expect("reads")
    };
    assert_eq!(shelf(&conn), (1, 1), "a library with a folder left keeps itself and its grant");

    conn.execute("DELETE FROM root WHERE id = 8", []).expect("removes the last folder");
    assert_eq!(shelf(&conn), (0, 0), "the library and its grant went with its last folder");
}

#[test]
fn a_library_goes_when_its_last_root_moves_away_and_its_works_stay() {
    let library = crate::Library::open_in_memory().expect("library");
    let conn = library.conn();
    conn.execute_batch(TWO_FOLDER_LIBRARY).expect("seeds");
    conn.execute(
        "INSERT INTO work (id, content_key, slug, title, reading, hub, library_id)
         VALUES (1, 'k', 's', 'T', 'rtl', 'raw', 80)",
        [],
    )
    .expect("a work in Other");

    conn.execute("UPDATE root SET library_id = 70 WHERE id = 9", [])
        .expect("moves Other's one folder into Shelf");

    // Counted with its NULL in the predicate: `SELECT library_id FROM work
    // WHERE id = 1` reads NULL for a deleted work too, and could not tell SET
    // NULL from CASCADE.
    let (libraries, unfiled): (i64, i64) = conn
        .query_row(
            "SELECT (SELECT count(*) FROM library WHERE id = 80),
                    (SELECT count(*) FROM work WHERE id = 1 AND library_id IS NULL)",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .expect("reads");
    assert_eq!(libraries, 0, "Other had no folder left");
    assert_eq!(unfiled, 1, "its work is kept, in no library, for the next derive to adopt");
}

#[test]
fn a_content_key_is_unique_within_a_library_and_a_slug_across_all_of_them() {
    let conn = Connection::open_in_memory().expect("in-memory connection");
    conn.pragma_update(None, "foreign_keys", "ON").expect("foreign keys");
    migrate(&conn).expect("migrates");
    conn.execute_batch(TWO_FOLDER_LIBRARY).expect("seeds");
    let insert = |key: &str, slug: &str, library: i64| {
        conn.execute(
            "INSERT INTO work (content_key, slug, title, reading, hub, library_id)
             VALUES (?1, ?2, 'T', 'rtl', 'raw', ?3)",
            rusqlite::params![key, slug, library],
        )
    };
    insert("same", "in-shelf", 70).expect("the first copy");
    insert("same", "in-other", 80).expect("the same content in another library is its own work");
    let twice = insert("same", "in-shelf-again", 70)
        .expect_err("the same content twice in one library");
    assert!(twice.to_string().contains("work.content_key"), "{twice}");
    let slug = insert("fresh", "in-shelf", 80)
        .expect_err("a slug names one work across every library");
    assert!(slug.to_string().contains("work.slug"), "{slug}");
}
```

`table_names` already exists in this file.

- [ ] **Step 5: Run the tests**

```bash
cargo test -p taiyaku-library schema:: structure:: account:: -- --test-threads=8
```

Expected: PASS.

- [ ] **Step 6: Watch each new test fail against its own defect**

Copy `015_libraries.sql` aside first. One defect at a time, restoring between:

1. In the library backfill, replace the `CASE` with plain `leaf`. Run
   `migration_15_files_every_root_under_a_library_named_for_its_folder`.
   Expected FAIL: migrate refuses, naming `UNIQUE constraint failed: library.name`.
2. Delete the first `COALESCE` argument (the hub subquery and its comma).
   Expected FAIL in `…files_each_work_under_its_hub_editions_library`: work 20
   reads `/srv/a/Volumes`.
3. Delete the second `COALESCE` argument instead. Expected FAIL: work 60 reads `None`.
4. In the series backfill, change `ORDER BY count(*) DESC, w.library_id` to
   `ORDER BY w.library_id`. Expected FAIL in `…where_most_of_its_works_are`:
   series 2 reads `/srv/b/Volumes`.
5. In `INSERT INTO work_new … SELECT`, replace `w.series_id,` (the fourth
   column) with `NULL,`. Expected FAIL in `…keeps_every_work_position_and_note…`.
6. Delete the `INSERT INTO user_library … FROM user_root` statement. Expected
   FAIL in `…moves_each_root_grant…`.
7. Delete `CREATE TRIGGER work_au … END;`. Expected FAIL in
   `…search_and_the_structure_counter…`: `Gamma` matches nothing.
8. Delete `CREATE TRIGGER structure_work_upd … END;`. Expected FAIL there, on
   the counter. Then run `cargo test -p taiyaku-library structure::` with the
   same defect: Expected FAIL `the counter did not move for: ["update on work"]`.
   Report both; migration 14's warning says `structure::tests` would catch a
   missing trigger, and this is where that is checked.
9. In `root_gets_a_library`, delete the `OR EXISTS (…)` clause. Expected FAIL
   in `a_root_written_without_a_library…`: the second insert refused on
   `library.name`.
10. In `library_emptied_by_delete`, delete the `AND NOT EXISTS (…)` line.
    Expected FAIL in `a_library_goes_with_its_last_root…`: `(0, 0)` where
    `(1, 1)` was expected.
11. Delete `library_emptied_by_move`. Expected FAIL in
    `a_library_goes_when_its_last_root_moves_away…`: `1` library left.
12. Change `work_new`'s `library_id` to `ON DELETE CASCADE`. Expected FAIL
    there: `unfiled` reads `0`, the work deleted with its library.
13. Change `UNIQUE (content_key, library_id)` to `UNIQUE (content_key)`.
    Expected FAIL in `a_content_key_is_unique_within_a_library…`: the second
    copy refused.

Restore with `cp`, prove it with `cmp`, and run Step 5 again green.

- [ ] **Step 7: Whole crates, lint, format**

```bash
cargo fmt --all
cargo test -p taiyaku-library -p taiyaku-serve -p taiyaku-cli -- --test-threads=8
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
```

Expected: all pass. Grep `SCHEMA_VERSION`, `MIGRATIONS` and `user_root`
across `crates/` and report every hit that is not a comment.

- [ ] **Step 8: Commit**

```bash
git add crates/taiyaku-library/src/schema/015_libraries.sql crates/taiyaku-library/src/schema.rs crates/taiyaku-library/src/schema/tests.rs crates/taiyaku-library/src/account.rs crates/taiyaku-library/src/account/tests.rs crates/taiyaku-serve/src/test_support.rs crates/taiyaku-serve/src/auth/tests.rs
git commit -m "Migration 15: libraries, each root filed under one, and grants on libraries"
```

---

## Task 2: `Root::library_id`, the `libraries` module, and adding a folder to a library

**Files:**
- Create: `crates/taiyaku-library/src/libraries.rs`, `crates/taiyaku-library/src/libraries/tests.rs`
- Modify: `crates/taiyaku-library/src/lib.rs` (after `pub mod librarydoc;`: `pub mod libraries;`)
- Modify: `crates/taiyaku-library/src/scan.rs`
- Modify: `crates/taiyaku-library/src/scan/tests.rs`
- Modify: `crates/taiyaku-library/src/rules/tests.rs` (`fn root()`), `crates/taiyaku-cli/src/library/tests.rs` (`fn a_root`)

**Interfaces:**
- Produces:
  - `scan::Root { …, pub library_id: i64 }`, read by `scan::roots`.
  - `pub enum scan::InLibrary<'a> { Own, Named(&'a str) }`.
  - `scan::add_root_in(conn, path: &Path, library: InLibrary<'_>) -> Result<Root, String>`;
    `scan::add_root(conn, path)` is `add_root_in(conn, path, InLibrary::Own)`.
  - `scan::add_root_checked_in(conn, path, library) -> Result<Added, AddRootError>`;
    `add_root_checked(conn, path)` is `…_in(conn, path, InLibrary::Own)`.
  - `libraries::Listed { pub id: i64, pub name: String, pub folders: Vec<PathBuf>, pub works: usize }`,
    `libraries::list(conn) -> Result<Vec<Listed>, String>` (by name),
    `libraries::id_of(conn, name: &str) -> Result<Option<i64>, String>`,
    `libraries::unfiled_works(conn) -> Result<usize, String>`,
    `pub(crate) libraries::create(conn, name: &str) -> Result<i64, String>`.

- [ ] **Step 1: The module**

Create `crates/taiyaku-library/src/libraries.rs`:

```rust
//! Libraries: a name and the folders filed under it. Migration 15, and Part F
//! of `docs/superpowers/specs/2026-10-05-libraries-design.md`.
//!
//! **A library is born with a folder and goes with its last one.** There is no
//! public way to make an empty one: [`crate::scan::add_root_in`] creates a
//! named library in the same transaction as the folder it is created for, and
//! migration 15's triggers file a root inserted without a library under one of
//! its own and delete a library whose last root goes. A grant names a library
//! and `account::roots_for` resolves it to that library's folders, so an empty
//! library would be a grant resolving to no folder, which `Principal.roots`
//! reads as unrestricted.

use std::path::PathBuf;

use rusqlite::{Connection, OptionalExtension as _};

/// One library, as `taiyaku library libraries` lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listed {
    pub id: i64,
    pub name: String,
    /// Its folders' paths, oldest first.
    pub folders: Vec<PathBuf>,
    /// The works filed under it.
    pub works: usize,
}

/// Every library, by name.
///
/// # Errors
///
/// Returns an error if a read fails.
pub fn list(conn: &Connection) -> Result<Vec<Listed>, String> {
    let mut libraries = conn
        .prepare(
            "SELECT l.id, l.name, (SELECT count(*) FROM work w WHERE w.library_id = l.id)
               FROM library l ORDER BY l.name, l.id",
        )
        .map_err(|e| format!("listing libraries: {e}"))?;
    let rows = libraries
        .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?)))
        .map_err(|e| format!("listing libraries: {e}"))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("listing libraries: {e}"))?;
    let mut folders = conn
        .prepare("SELECT path FROM root WHERE library_id = ?1 ORDER BY id")
        .map_err(|e| format!("listing a library's folders: {e}"))?;
    let mut listed = Vec::with_capacity(rows.len());
    for (id, name, works) in rows {
        let paths = folders
            .query_map([id], |r| r.get::<_, String>(0))
            .map_err(|e| format!("listing the folders of `{name}`: {e}"))?
            .map(|row| row.map(PathBuf::from))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("listing the folders of `{name}`: {e}"))?;
        listed.push(Listed {
            id,
            name,
            folders: paths,
            works: usize::try_from(works).unwrap_or(usize::MAX),
        });
    }
    Ok(listed)
}

/// The library called `name`, if there is one. Compared as stored: names are
/// trimmed on the way in by [`create`].
///
/// # Errors
///
/// Returns an error if the read fails.
pub fn id_of(conn: &Connection, name: &str) -> Result<Option<i64>, String> {
    conn.query_row("SELECT id FROM library WHERE name = ?1", [name.trim()], |r| r.get(0))
        .optional()
        .map_err(|e| format!("looking up the library `{name}`: {e}"))
}

/// A new library called `name`, trimmed. **Crate-only**, so a library is only
/// ever made together with its first folder; see this module's doc.
///
/// # Errors
///
/// Returns an error if `name` is empty once trimmed, or taken.
pub(crate) fn create(conn: &Connection, name: &str) -> Result<i64, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("a library needs a name".to_string());
    }
    conn.execute("INSERT INTO library (name) VALUES (?1)", [name])
        .map_err(|e| format!("creating the library `{name}`: {e}"))?;
    Ok(conn.last_insert_rowid())
}

/// How many works are filed under no library: an imported `taiyaku.json`'s,
/// and a row `serve` minted for one.
///
/// # Errors
///
/// Returns an error if the read fails.
pub fn unfiled_works(conn: &Connection) -> Result<usize, String> {
    conn.query_row("SELECT count(*) FROM work WHERE library_id IS NULL", [], |r| {
        r.get::<_, i64>(0)
    })
    .map(|n| usize::try_from(n).unwrap_or(usize::MAX))
    .map_err(|e| format!("counting works in no library: {e}"))
}

#[cfg(test)]
mod tests;
```

- [ ] **Step 2: `scan.rs`**

1. `Root` gains, after `keeps_documents`:
   ```rust
   /// The library this folder is filed under — migration 15. Every root has
   /// one; see [`crate::libraries`].
   pub library_id: i64,
   ```
   `roots` adds `library_id` as the ninth selected column and reads it into
   the field.
2. Add, above `add_root`:
   ```rust
   /// Which library a folder joins when it is added.
   #[derive(Debug, Clone, Copy, PartialEq, Eq)]
   pub enum InLibrary<'a> {
       /// A library of its own, named for the folder by migration 15's
       /// `root_gets_a_library`: its last path component, or its whole path
       /// when a library already has that name.
       Own,
       /// The library with this name, created if there is none.
       Named(&'a str),
   }
   ```
3. Replace `add_root`'s body with a call to the new `add_root_in`, and give
   `add_root_in` the old body with two changes: the library is resolved inside
   one `crate::Nested` transaction with the insert, and the read-back reads
   `library_id` too.
   ```rust
   pub fn add_root(conn: &Connection, path: &Path) -> Result<Root, String> {
       add_root_in(conn, path, InLibrary::Own)
   }

   /// [`add_root`], filing the folder under `library`.
   ///
   /// **A named library that does not exist yet is created here, in the same
   /// transaction as the folder**, so a refused insert leaves no empty library
   /// behind. See [`crate::libraries`] for why an empty one must not exist.
   ///
   /// # Errors
   ///
   /// As [`add_root`], and if `library` names nothing once trimmed.
   pub fn add_root_in(conn: &Connection, path: &Path, library: InLibrary<'_>) -> Result<Root, String> {
       let canonical = path
           .canonicalize()
           .map_err(|e| format!("{}: {e}", path.display()))?;
       let tx = crate::Nested::open(conn, "add_root")
           .map_err(|e| format!("starting to add root {}: {e}", canonical.display()))?;
       let chosen = match library {
           InLibrary::Own => None,
           InLibrary::Named(name) => Some(match crate::libraries::id_of(&tx, name)? {
               Some(id) => id,
               None => crate::libraries::create(&tx, name)?,
           }),
       };
       tx.execute(
           "INSERT INTO root (path, library_id) VALUES (?1, ?2)",
           rusqlite::params![canonical.to_string_lossy(), chosen],
       )
       .map_err(|e| format!("adding root {}: {e}", canonical.display()))?;
       let id = tx.last_insert_rowid();
       // Read back rather than hard-coded, so the column's default — migration
       // 012's `DEFAULT 0` — stays the one place that decides it, and so a
       // library migration 15's trigger chose is the one recorded.
       let (keeps_documents, library_id) = tx
           .query_row(
               "SELECT keeps_documents, library_id FROM root WHERE id = ?1",
               [id],
               |r| Ok((r.get(0)?, r.get(1)?)),
           )
           .map_err(|e| format!("reading root {}: {e}", canonical.display()))?;
       tx.commit()
           .map_err(|e| format!("committing root {}: {e}", canonical.display()))?;
       Ok(Root {
           id,
           path: canonical,
           edition_key: None,
           default_kind: None,
           default_language: None,
           exclude_globs: None,
           scan_interval_secs: None,
           keeps_documents,
           library_id,
       })
   }
   ```
4. Do the same for the checked path: `add_root_checked(conn, path)` becomes
   `add_root_checked_in(conn, path, InLibrary::Own)`, and
   `add_root_checked_in` is the old body with `add_root(conn, &canonical)`
   replaced by `add_root_in(conn, &canonical, library)`. Its doc gains one
   paragraph: "**The library is created only once every validation has
   passed**, by [`add_root_in`], so a refused path names no library into
   being."
5. `apply_removal`'s doc: "`scan_entry`, `dir_state`, `rule` and `user_root`
   all cascade with the root" becomes "`scan_entry`, `dir_state` and `rule`
   cascade with the root, and a library left with no root goes with it
   (migration 15's `library_emptied_by_delete`), taking its `user_library`
   grants".
6. `rules/tests.rs`'s `fn root()` and `taiyaku-cli/src/library/tests.rs`'s
   `fn a_root` gain `library_id: 1`.

- [ ] **Step 3: Write the tests**

In `crates/taiyaku-library/src/scan/tests.rs`:

```rust
/// What `library_id` each of these roots carries, by library name.
fn library_of(conn: &Connection, root: &Root) -> String {
    conn.query_row("SELECT name FROM library WHERE id = ?1", [root.library_id], |r| r.get(0))
        .expect("the root's library exists")
}

#[test]
fn a_folder_added_on_its_own_is_a_library_named_for_it() {
    let parent = crate::tests::tempdir("scan-own-library");
    let a = parent.path().join("a").join("Volumes");
    let b = parent.path().join("b").join("Volumes");
    std::fs::create_dir_all(&a).expect("mkdir");
    std::fs::create_dir_all(&b).expect("mkdir");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();

    let first = add_root(&conn, &a).expect("adds a");
    let second = add_root(&conn, &b).expect("adds b");

    assert_ne!(first.library_id, second.library_id, "each is a library of its own");
    assert_eq!(library_of(&conn, &first), "Volumes");
    assert_eq!(
        library_of(&conn, &second),
        b.canonicalize().expect("canonical").to_string_lossy(),
        "a taken name falls back to the whole path"
    );
    assert_eq!(
        roots(&conn).expect("lists").iter().map(|r| r.library_id).collect::<Vec<_>>(),
        vec![first.library_id, second.library_id],
        "roots reads back what add_root returned"
    );
}

#[test]
fn a_folder_added_to_a_named_library_joins_it_or_creates_it() {
    let parent = crate::tests::tempdir("scan-named-library");
    let (volumes, chapters, other) = (
        parent.path().join("Volumes"),
        parent.path().join("Chapters"),
        parent.path().join("Other"),
    );
    for dir in [&volumes, &chapters, &other] {
        std::fs::create_dir_all(dir).expect("mkdir");
    }
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();

    let first = add_root_in(&conn, &volumes, InLibrary::Named("Manga")).expect("creates Manga");
    let second = add_root_in(&conn, &chapters, InLibrary::Named(" Manga ")).expect("joins Manga");
    let third = add_root(&conn, &other).expect("a library of its own");

    assert_eq!(first.library_id, second.library_id, "the second folder joined, by trimmed name");
    assert_ne!(third.library_id, first.library_id);
    assert_eq!(library_of(&conn, &first), "Manga");
}

#[test]
fn a_refused_folder_names_no_library_into_being() {
    let dir = crate::tests::tempdir("scan-refused-library");
    let nested = dir.path().join("inner");
    std::fs::create_dir(&nested).expect("mkdir");
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    add_root(&conn, dir.path()).expect("the outer root");

    add_root_checked_in(&conn, &nested, InLibrary::Named("Never"))
        .expect_err("a root inside another is refused");

    assert_eq!(
        crate::libraries::id_of(&conn, "Never").expect("reads"),
        None,
        "the refusal left no library behind"
    );
}
```

Create `crates/taiyaku-library/src/libraries/tests.rs`:

```rust
use super::*;
use crate::Library;
use crate::scan::{InLibrary, add_root, add_root_in};

#[test]
fn listing_names_each_library_with_its_folders_and_works() {
    let parent = crate::tests::tempdir("libraries-list");
    let dirs: Vec<_> = ["Volumes", "Chapters", "Comics"]
        .iter()
        .map(|name| {
            let dir = parent.path().join(name);
            std::fs::create_dir_all(&dir).expect("mkdir");
            dir.canonicalize().expect("canonical")
        })
        .collect();
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let comics = add_root(&conn, &dirs[2]).expect("Comics, a library of its own");
    add_root_in(&conn, &dirs[0], InLibrary::Named("Manga")).expect("Manga");
    add_root_in(&conn, &dirs[1], InLibrary::Named("Manga")).expect("Manga again");
    conn.execute(
        "INSERT INTO work (content_key, slug, title, reading, hub, library_id)
         VALUES ('k', 's', 'T', 'rtl', 'raw', ?1)",
        [comics.library_id],
    )
    .expect("one work in Comics");

    let listed = list(&conn).expect("lists");

    assert_eq!(
        listed
            .iter()
            .map(|l| (l.name.as_str(), l.folders.clone(), l.works))
            .collect::<Vec<_>>(),
        vec![
            ("Comics", vec![dirs[2].clone()], 1),
            ("Manga", vec![dirs[0].clone(), dirs[1].clone()], 0),
        ],
        "by name, each folder once, oldest first, and each library's own works"
    );
}

#[test]
fn a_library_needs_a_name_and_one_of_its_own() {
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    assert!(create(&conn, "   ").unwrap_err().contains("needs a name"));
    create(&conn, "Manga").expect("creates");
    assert!(create(&conn, " Manga").unwrap_err().contains("UNIQUE"));
}

#[test]
fn unfiled_works_counts_only_works_in_no_library() {
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let id = create(&conn, "Manga").expect("creates");
    conn.execute_batch(&format!(
        "INSERT INTO work (content_key, slug, title, reading, hub, library_id)
         VALUES ('a', 'a', 'A', 'rtl', 'raw', {id}), ('b', 'b', 'B', 'rtl', 'raw', NULL),
                ('c', 'c', 'C', 'rtl', 'raw', NULL);"
    ))
    .expect("works");
    assert_eq!(unfiled_works(&conn).expect("counts"), 2);
}
```

- [ ] **Step 4: Run, and watch each fail against its defect**

`cargo test -p taiyaku-library scan:: libraries:: -- --test-threads=8`. Then:

1. In `add_root_in`, bind `None::<i64>` in place of `chosen`. Expected FAIL in
   `a_folder_added_to_a_named_library_joins_it_or_creates_it`: the two
   library ids differ.
2. In `add_root_checked_in`, move the nesting loop below the `add_root_in`
   call, so the folder and its new library are committed before the overlap
   is found. Expected FAIL in `a_refused_folder_names_no_library_into_being`:
   `Some(…)` where `None` was expected.
3. In `list`, drop `WHERE library_id = ?1` to `WHERE (library_id = ?1 OR 1 = 1)`.
   Expected FAIL: `Comics` lists three folders.
4. In `unfiled_works`, `IS NULL` → `IS NOT NULL`. Expected FAIL: `1`, not `2`.

- [ ] **Step 5: Whole crates, lint, format, and commit**

Run Task 1 Step 7's four commands. Then:

```bash
git add crates/taiyaku-library/src/libraries.rs crates/taiyaku-library/src/libraries/tests.rs crates/taiyaku-library/src/lib.rs crates/taiyaku-library/src/scan.rs crates/taiyaku-library/src/scan/tests.rs crates/taiyaku-library/src/rules/tests.rs crates/taiyaku-cli/src/library/tests.rs
git commit -m "Libraries: a root knows its library, and a folder can be added to a named one"
```

---

## Task 3: Derivation within a library

**Files:**
- Modify: `crates/taiyaku-library/src/derive.rs`
- Modify: `crates/taiyaku-library/src/import.rs`
- Modify: `crates/taiyaku-library/src/librarydoc.rs`
- Modify: `crates/taiyaku-library/src/derive/tests.rs`, `crates/taiyaku-library/src/import/tests.rs`
- Modify: fixtures whose premise is one library of two folders (Step 1 lists them), in any crate

**Interfaces:**
- Consumes: `Root::library_id` (Task 2).
- Produces:
  - `type GroupKey = (String, Option<u64>, Option<String>, i64)`, library last.
  - `import::scanned_in(conn, doc, library_id: i64, known: Option<i64>) -> Result<i64, String>`.
    `import::scanned(conn, doc, known)` is unchanged in signature and files the
    work under no library.
  - Every work `derive` writes carries its group's library; every series it
    reconciles carries the library its files were walked from.

- [ ] **Step 1: List the fixtures whose premise this changes, before changing anything**

With Task 2 in place every `add_root` is a library of its own, so any test
that derives one work or one series from two roots is about to become two.
Make the old behaviour say so instead of guessing which tests those are.

Add, temporarily, to `derive_with` just before the root loop:

```rust
let mut premise: BTreeMap<(String, Option<u64>, Option<String>), i64> = BTreeMap::new();
let mut premise_series: BTreeMap<String, i64> = BTreeMap::new();
```

thread both into `Sorting` as two more `&mut` fields, and in `classify`, at
the two places `named_series.insert` runs and where `key` is built:

```rust
if let Some(seen) = into.premise_series.insert(series.clone(), root.library_id)
    && seen != root.library_id
{
    panic!("PREMISE: series `{series}` is named from libraries {seen} and {}", root.library_id);
}
// …and beside `let key = …`:
if let Some(seen) = into.premise.insert(key.clone(), root.library_id)
    && seen != root.library_id
{
    panic!("PREMISE: group `{}` gathers libraries {seen} and {}", key.0, root.library_id);
}
```

Run all three crates with `--no-fail-fast` and keep the output:

```bash
cargo test -p taiyaku-library -p taiyaku-serve -p taiyaku-cli --no-fail-fast -- --test-threads=8 >| .superpowers/sdd/libraries-f1-evidence/premise-derive.log 2>&1; echo "exit $?"
grep -B2 'PREMISE' .superpowers/sdd/libraries-f1-evidence/premise-derive.log | grep -E "^---- |PREMISE" | sort -u
```

Remove the probe (`git diff` on `derive.rs` must be empty afterwards). The
list is the set of tests to look at in Step 6. Expect at least the two-root
cases named in the derive, proposals and browse tests and in `test_support`'s
`router_over_two_scanned_volumes_of_one_series_across_two_roots*`. Put the
list in the report.

- [ ] **Step 2: `import.rs`**

1. `import_with` gains `library: Option<i64>` after `scope`, and passes it to
   `find_by_key`, `update_work` and `insert_work`. `sidecar` passes `None`,
   `scanned` passes `None`.
2. Add after `scanned`:
   ```rust
   /// [`scanned`] for a work derived from a library's folders, which is every
   /// work `derive` writes.
   ///
   /// **Content decides within `library` — Part F1.** A byte-identical volume
   /// in two libraries is two works, so the content probe looks only at this
   /// library's works, and at works in no library, which it adopts: a work
   /// migration 15 could not file, or one whose library went when its last
   /// folder moved away. Adopting keeps its id, its slug and everything a
   /// reader has written against it.
   ///
   /// # Errors
   ///
   /// As [`scanned`].
   pub fn scanned_in(
       conn: &Connection,
       doc: &Sidecar,
       library_id: i64,
       known: Option<i64>,
   ) -> Result<i64, String> {
       import_with(conn, doc, None, Scope::Scan, Some(library_id), known)
   }
   ```
3. `find_by_key`:
   ```rust
   /// The work holding `key`. Within `library` and among works in no library,
   /// its own first; with no library — a document import, or [`scanned`] — any
   /// work, one in a library before one in none, then the oldest, because two
   /// libraries can each hold a work with this key since F1.
   fn find_by_key(conn: &Connection, key: &str, library: Option<i64>) -> Result<Option<i64>, String> {
       conn.query_row(
           "SELECT id FROM work
             WHERE content_key = ?1
               AND (?2 IS NULL OR library_id = ?2 OR library_id IS NULL)
             ORDER BY library_id IS NULL, id
             LIMIT 1",
           rusqlite::params![key, library],
           |row| row.get(0),
       )
       .optional()
       .map_err(|e| format!("looking up a work by content key: {e}"))
   }
   ```
   `reimport` calls `find_by_key(conn, &key, None)`.
4. `update_work`'s `Scope::Scan` statement becomes
   `UPDATE work SET content_key = ?1, library_id = COALESCE(?3, library_id) WHERE id = ?2`,
   binding `library` third, with a comment: "the adoption [`scanned_in`]
   describes; a work already in a library keeps it, since the lookup that
   found it was scoped to this one". The `Scope::Document` statement is
   unchanged.
5. `insert_work` takes `library: Option<i64>` and inserts it as `library_id`.

- [ ] **Step 3: `derive.rs`**

1. `type GroupKey = (String, Option<u64>, Option<String>, i64);`. Add to its
   doc: "**The fourth element is the library — Part F1.** Last, so a library's
   groups sort by title, volume and path as a store's did, and the rows a pass
   mints come out in the order they always have. Two folders of one library
   group together exactly as two roots did before F1; a folder of another
   library never joins them."
2. `Group` gains `library_id: i64`; its doc says "every entry across every
   root *of one library*".
3. `Sorting::named_series` becomes `&'a mut BTreeSet<(String, i64)>`. In
   `classify`, both `named_series.insert` calls insert
   `(series…, root.library_id)`; `key` gains `root.library_id` last; the
   `Group` literal sets `library_id: root.library_id`.
4. In `derive_with`, `named_series: BTreeSet<(String, i64)>`,
   `series_ids: BTreeMap<(String, i64), i64>`, the reconcile loop is
   `for (title, library_id) in named_series { let id = reconcile_series(conn, library_id, &title)?; series_ids.insert((title, library_id), id); }`,
   and the group loop looks up `&(group.series.clone(), group.library_id)`.
   Update `derive_with`'s doc ("series are reconciled on `sort_title`") to
   "on `(library, sort_title)`, and works on `(series_id, volume)` within the
   library".
5. `reconcile_series(conn, library_id: i64, sort_title)`:
   ```rust
   let held: Option<(i64, Option<i64>)> = conn
       .query_row(
           "SELECT id, library_id FROM series
             WHERE sort_title = ?1 AND (library_id = ?2 OR library_id IS NULL)
             ORDER BY library_id IS NULL, id
             LIMIT 1",
           rusqlite::params![sort_title, library_id],
           |r| Ok((r.get(0)?, r.get(1)?)),
       )
       .optional()
       .map_err(|e| format!("looking up the series `{sort_title}`: {e}"))?;
   let series_id = match held {
       Some((id, Some(_))) => id,
       Some((id, None)) => {
           conn.execute(
               "UPDATE series SET library_id = ?1 WHERE id = ?2",
               rusqlite::params![library_id, id],
           )
           .map_err(|e| format!("filing the series `{sort_title}` under its library: {e}"))?;
           id
       }
       None => {
           conn.execute(
               "INSERT INTO series (sort_title, library_id) VALUES (?1, ?2)",
               rusqlite::params![sort_title, library_id],
           )
           .map_err(|e| format!("inserting the series `{sort_title}`: {e}"))?;
           conn.last_insert_rowid()
       }
   };
   ```
   Add to its doc: "**Within a library — Part F1.** The same title in two
   libraries is two series. A series in no library is adopted: one migration
   15 found no work for, or one `librarydoc::apply` minted for an imported
   work. The first library whose files name it takes it, as this function
   took any row of that title before F1, so a proposal pointing at it keeps
   pointing at it."
6. `held_work`: both statements gain `AND (work.library_id = ?3 OR work.library_id IS NULL)`
   (the first may say `library_id`) and bind `group.library_id` third, with a
   comment: "the group's library, and a work in none, which `scanned_in`
   adopts. A hand-set `series_id_hand` can point a work at a series of another
   library, and without this the address probe would hand that work this
   library's volume."
7. `write_work` calls `crate::import::scanned_in(conn, &doc, group.library_id, held.map(|(id, _, _)| id))`.
   Update the comments above it that name `import::scanned`.

- [ ] **Step 4: `librarydoc.rs`**

In `apply_series`, read the work's library before resolving:

```rust
let library: Option<i64> = conn
    .query_row("SELECT library_id FROM work WHERE id = ?1", [work_id], |row| row.get(0))
    .map_err(|e| format!("reading the library of work {work_id}: {e}"))?;
```

`find_series_by_title(conn, library, &series.titles)`, with its query:

```sql
SELECT st.series_id FROM series_title st JOIN series s ON s.id = st.series_id
 WHERE st.title = ?1 AND (?2 IS NULL OR s.library_id = ?2 OR s.library_id IS NULL)
 ORDER BY s.library_id IS NULL, st.series_id
 LIMIT 1
```

and the mint is `INSERT INTO series (sort_title, reading, reading_hand, library_id) VALUES (?1, ?2, ?2, ?3)`
binding `library`. Doc on `find_series_by_title`: "**Within the work's
library — Part F1**, and among series in none. A work in no library (an
imported one) still finds a series in any, as before F1."

- [ ] **Step 5: Write the tests**

Add a section to `crates/taiyaku-library/src/derive/tests.rs`. Each test names
the defect that must turn it red.

```rust
// --- Part F1: within a library ----------------------------------------------

/// Copy every page of `from` into a new directory `to`, byte for byte.
/// `write_pages` bakes its own directory into each page, so two calls never
/// collide; a copy is how a test asks for the same bytes twice.
fn copy_pages(from: &std::path::Path, to: &std::path::Path) {
    std::fs::create_dir_all(to).expect("mkdir");
    for entry in std::fs::read_dir(from).expect("lists") {
        let entry = entry.expect("entry");
        std::fs::copy(entry.path(), to.join(entry.file_name())).expect("copies a page");
    }
}

fn count(conn: &Connection, sql: &str) -> i64 {
    conn.query_row(sql, [], |r| r.get(0)).expect(sql)
}

#[test]
fn the_same_title_in_two_libraries_makes_two_series_and_two_works() {
    let ecchi = crate::tests::tempdir("derive-lib-ecchi");
    write_pages(&ecchi.path().join("Series").join("Series - v01"), 2);
    let volumes = crate::tests::tempdir("derive-lib-volumes");
    write_pages(&volumes.path().join("Series").join("Series - v01"), 3);
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    for dir in [&ecchi, &volumes] {
        let root = scan::add_root(&conn, dir.path()).expect("a library of its own");
        scan::scan_root(&conn, &root).expect("scans");
    }

    derive::derive(&conn).expect("derives");

    assert_eq!(
        count(&conn, "SELECT count(DISTINCT library_id) FROM series WHERE sort_title = 'Series'"),
        2,
        "one series of that title per library"
    );
    assert_eq!(count(&conn, "SELECT count(*) FROM work"), 2, "one volume 1 per library");
    assert_eq!(
        count(
            &conn,
            "SELECT count(*) FROM work w JOIN series s ON s.id = w.series_id \
             WHERE w.library_id IS NOT s.library_id"
        ),
        0,
        "every work sits in a series of its own library"
    );
}

#[test]
fn two_folders_of_one_library_still_make_one_work_of_a_volume_and_its_other_release() {
    let volumes = crate::tests::tempdir("derive-lib-one-volumes");
    write_pages(&volumes.path().join("Series").join("Series - v01"), 2);
    let chapters = crate::tests::tempdir("derive-lib-one-chapters");
    write_pages(&chapters.path().join("Series").join("Series - v01"), 3);
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let v = scan::add_root_in(&conn, volumes.path(), scan::InLibrary::Named("Manga")).expect("adds");
    let c = scan::add_root_in(&conn, chapters.path(), scan::InLibrary::Named("Manga")).expect("adds");
    scan::update_root(
        &conn,
        c.id,
        &scan::RootPatch { edition_key: Some(Some("en".to_string())), ..Default::default() },
    )
    .expect("the second folder is another edition");
    scan::scan_root(&conn, &v).expect("scans");
    scan::scan_root(&conn, &scan::roots(&conn).expect("lists")[1]).expect("scans");

    derive::derive(&conn).expect("derives");

    assert_eq!(count(&conn, "SELECT count(*) FROM work"), 1, "one work across both folders");
    assert_eq!(edition_keys(&conn), vec!["en", "raw"], "with each folder's edition");
}

#[test]
fn byte_identical_volumes_in_two_libraries_are_two_works_with_two_slugs() {
    let ecchi = crate::tests::tempdir("derive-lib-copy-a");
    let first = ecchi.path().join("Series").join("Series - v01");
    write_pages(&first, 3);
    let volumes = crate::tests::tempdir("derive-lib-copy-b");
    copy_pages(&first, &volumes.path().join("Series").join("Series - v01"));
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    for dir in [&ecchi, &volumes] {
        let root = scan::add_root(&conn, dir.path()).expect("adds");
        scan::scan_root(&conn, &root).expect("scans");
    }

    derive::derive(&conn).expect("derives");
    let ids = |conn: &Connection| -> Vec<(i64, String, String)> {
        conn.prepare("SELECT id, slug, content_key FROM work ORDER BY id")
            .expect("prepares")
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .expect("reads")
            .collect::<rusqlite::Result<_>>()
            .expect("reads")
    };
    let first_pass = ids(&conn);
    derive::derive(&conn).expect("derives again");

    assert_eq!(first_pass.len(), 2, "two works");
    assert_eq!(first_pass[0].2, first_pass[1].2, "with one content key between them");
    assert_ne!(first_pass[0].1, first_pass[1].1, "and two slugs, since a slug names one work");
    assert_eq!(ids(&conn), first_pass, "a second pass finds each library's own again");
}

/// Spec decision 5: a folder moved into another library takes its reading
/// history with it. Its library keeps another folder, so the old works stay
/// in it until the fold carries them across.
#[test]
fn a_folder_moved_to_another_library_carries_positions_and_notes_through_the_fold() {
    let kept = crate::tests::tempdir("derive-lib-move-kept");
    write_pages(&kept.path().join("Kept").join("Kept - v01"), 2);
    let moved = crate::tests::tempdir("derive-lib-move-moved");
    write_pages(&moved.path().join("Moved").join("Moved - v01"), 3);
    let elsewhere = crate::tests::tempdir("derive-lib-move-elsewhere");
    write_pages(&elsewhere.path().join("Else").join("Else - v01"), 4);
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let k = scan::add_root_in(&conn, kept.path(), scan::InLibrary::Named("A")).expect("adds");
    let m = scan::add_root_in(&conn, moved.path(), scan::InLibrary::Named("A")).expect("adds");
    let e = scan::add_root_in(&conn, elsewhere.path(), scan::InLibrary::Named("B")).expect("adds");
    for root in [&k, &m, &e] {
        scan::scan_root(&conn, root).expect("scans");
    }
    derive::derive(&conn).expect("derives");
    let before: i64 = conn
        .query_row("SELECT id FROM work WHERE title LIKE 'Moved%'", [], |r| r.get(0))
        .expect("the moved folder's work");
    conn.execute(
        "INSERT INTO progress (user_id, work_id, edition, slot) VALUES (1, ?1, 'raw', 2)",
        [before],
    )
    .expect("a position");
    conn.execute(
        "INSERT INTO note (user_id, id, work_id, edition, slot, body, created, updated)
         VALUES (1, 'n', ?1, 'raw', 1, 'kept', 'now', 'now')",
        [before],
    )
    .expect("a note");

    conn.execute("UPDATE root SET library_id = ?1 WHERE id = ?2", [e.library_id, m.id])
        .expect("moves the folder into B");
    let report = derive::derive(&conn).expect("re-derives");

    let (after, library_id): (i64, i64) = conn
        .query_row(
            "SELECT w.id, w.library_id FROM work w
               JOIN edition ed ON ed.work_id = w.id JOIN member mb ON mb.edition_id = ed.id
              WHERE mb.root_id = ?1",
            [m.id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .expect("the moved folder's material has one work");
    assert_eq!(library_id, e.library_id, "filed under B");
    assert_eq!(report.emptied_works, 1, "the old work was folded into the new one");
    assert_eq!(count(&conn, &format!("SELECT count(*) FROM work WHERE id = {before}")), 0);
    assert_eq!(
        count(&conn, &format!("SELECT count(*) FROM progress WHERE work_id = {after} AND slot = 2")),
        1,
        "the position came with it"
    );
    assert_eq!(
        count(&conn, &format!("SELECT count(*) FROM note WHERE work_id = {after}")),
        1,
        "and the note"
    );
}

/// The other half of a move: a folder alone in its library empties it, the
/// library goes (migration 15's trigger) and its works are left in none. The
/// next derive adopts them, so nothing is minted and nothing folded.
#[test]
fn a_folder_moved_out_of_a_library_it_was_alone_in_keeps_its_works() {
    let moved = crate::tests::tempdir("derive-lib-alone-moved");
    write_pages(&moved.path().join("Moved").join("Moved - v01"), 3);
    let elsewhere = crate::tests::tempdir("derive-lib-alone-elsewhere");
    write_pages(&elsewhere.path().join("Else").join("Else - v01"), 4);
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let m = scan::add_root(&conn, moved.path()).expect("adds");
    let e = scan::add_root(&conn, elsewhere.path()).expect("adds");
    for root in [&m, &e] {
        scan::scan_root(&conn, root).expect("scans");
    }
    derive::derive(&conn).expect("derives");
    let before: (i64, String) = conn
        .query_row("SELECT id, slug FROM work WHERE title LIKE 'Moved%'", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .expect("the work");

    conn.execute("UPDATE root SET library_id = ?1 WHERE id = ?2", [e.library_id, m.id])
        .expect("moves the folder");
    let report = derive::derive(&conn).expect("re-derives");

    let after: (i64, String, i64) = conn
        .query_row("SELECT id, slug, library_id FROM work WHERE title LIKE 'Moved%'", [], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })
        .expect("the work");
    assert_eq!((after.0, after.1.clone()), before, "the same row and the same slug");
    assert_eq!(after.2, e.library_id);
    assert_eq!(report.emptied_works, 0, "nothing needed folding");
    assert_eq!(count(&conn, "SELECT count(*) FROM series WHERE library_id IS NULL"), 0);
}

#[test]
fn a_hand_document_finds_its_series_in_its_own_library() {
    let first = crate::tests::tempdir("derive-lib-doc-a");
    write_pages(&first.path().join("Series").join("Series - v01"), 2);
    let second = crate::tests::tempdir("derive-lib-doc-b");
    write_pages(&second.path().join("Series").join("Series - v02"), 3);
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    for dir in [&first, &second] {
        let root = scan::add_root(&conn, dir.path()).expect("adds");
        scan::scan_root(&conn, &root).expect("scans");
    }
    derive::derive(&conn).expect("derives");
    let (work, own_series): (i64, i64) = conn
        .query_row("SELECT id, series_id FROM work WHERE volume = 2.0", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .expect("the second library's volume");
    let doc = crate::librarydoc::LibraryDoc::from_json(
        r#"{"taiyaku_library":1,"updated":"2026-10-05T00:00:00Z",
            "series":{"chosen":true,"titles":[{"title":"Series","kind":"romaji","primary":true}]}}"#,
    )
    .expect("parses");

    crate::librarydoc::apply(&conn, work, &doc).expect("applies");

    let series: i64 = conn
        .query_row("SELECT series_id FROM work WHERE id = ?1", [work], |r| r.get(0))
        .expect("reads");
    assert_eq!(series, own_series, "the title also names the first library's series");
}

#[test]
fn a_series_and_a_work_in_no_library_are_adopted_by_the_next_derive() {
    let dir = crate::tests::tempdir("derive-lib-adopt");
    write_pages(&dir.path().join("Series").join("Series - v01"), 2);
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    let root = scan::add_root(&conn, dir.path()).expect("adds");
    scan::scan_root(&conn, &root).expect("scans");
    derive::derive(&conn).expect("derives");
    let before = (
        count(&conn, "SELECT id FROM series"),
        count(&conn, "SELECT id FROM work"),
    );
    conn.execute_batch("UPDATE work SET library_id = NULL; UPDATE series SET library_id = NULL;")
        .expect("as migration 15 leaves a workless series, or a move leaves both");

    derive::derive(&conn).expect("derives again");

    assert_eq!(count(&conn, "SELECT count(*) FROM series"), 1, "no second series");
    assert_eq!(count(&conn, "SELECT count(*) FROM work"), 1, "no second work");
    assert_eq!(
        (count(&conn, "SELECT id FROM series"), count(&conn, "SELECT id FROM work")),
        before,
        "the same rows"
    );
    assert_eq!(
        count(&conn, &format!("SELECT count(*) FROM work WHERE library_id = {}", root.library_id)),
        1
    );
}
```

In `crates/taiyaku-library/src/import/tests.rs`, beside the other `scanned`
cases (reuse its `doc("aa")` helper):

```rust
#[test]
fn scanned_in_finds_its_own_librarys_work_and_not_anothers() {
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    conn.execute_batch("INSERT INTO library (id, name) VALUES (70, 'A'), (80, 'B');")
        .expect("two libraries");

    let in_a = scanned_in(&conn, &doc("aa"), 70, None).expect("into A");
    let in_b = scanned_in(&conn, &doc("aa"), 80, None).expect("into B");
    let again = scanned_in(&conn, &doc("aa"), 70, None).expect("into A again");

    assert_ne!(in_a, in_b, "the same content in another library is another work");
    assert_eq!(again, in_a, "and A finds its own");
    let library_of = |id: i64| -> i64 {
        conn.query_row("SELECT library_id FROM work WHERE id = ?1", [id], |r| r.get(0))
            .expect("reads")
    };
    assert_eq!((library_of(in_a), library_of(in_b)), (70, 80));
}

#[test]
fn scanned_in_adopts_a_work_in_no_library() {
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    conn.execute_batch("INSERT INTO library (id, name) VALUES (70, 'A');").expect("a library");
    let unfiled = scanned(&conn, &doc("aa"), None).expect("a work in no library");

    let filed = scanned_in(&conn, &doc("aa"), 70, None).expect("into A");

    assert_eq!(filed, unfiled, "the same row");
    let library_id: Option<i64> = conn
        .query_row("SELECT library_id FROM work WHERE id = ?1", [filed], |r| r.get(0))
        .expect("reads");
    assert_eq!(library_id, Some(70), "now filed");
}

#[test]
fn a_document_import_finds_a_scanned_work_in_any_library() {
    let library = Library::open_in_memory().expect("open");
    let conn = library.conn();
    conn.execute_batch("INSERT INTO library (id, name) VALUES (70, 'A');").expect("a library");
    let scanned_id = scanned_in(&conn, &doc("aa"), 70, None).expect("into A");

    let imported = sidecar(&conn, &doc("aa"), Path::new("/x/taiyaku.json")).expect("imports");

    assert_eq!(imported, scanned_id, "content still decides for a document, across libraries");
}
```

- [ ] **Step 6: Bring the listed fixtures onto one library**

Run all three crates. For each failing test, and for each test Step 1 listed
even if it passes:

- **Its premise is one library of two folders** (a work or a series spanning
  two roots, a second edition arriving from another root, "two roots sharing a
  directory name" in one store): add both roots with
  `scan::add_root_in(…, InLibrary::Named("<one name>"))`. In
  `test_support.rs`, both `router_over_two_scanned_volumes_of_one_series_across_two_roots`
  fixtures are this; in the `…_and_an_unrelated_series` one, file
  `root_unrelated` under its own library (plain `add_root`), and check its
  "lower id" assertion still holds: `GroupKey`'s library is last, so it should.
- **Its premise is two separate stores of material** (two unrelated series
  that happen to sit in two roots, a root-removal test): leave it.
- **Neither, or unsure**: stop and report the test and why.

Report every fixture you changed, with one line each on which case it was.
Never change an assertion to make a test pass; a fixture brought onto one
library must pass with its assertions as they were.

- [ ] **Step 7: Run, and watch each new test fail against its defect**

`cargo test -p taiyaku-library derive:: import:: -- --test-threads=8`. Then,
one at a time, restoring between:

1. `GroupKey`'s `root.library_id` → `0` in `classify` (both the key and
   `named_series`). Expected FAIL in
   `the_same_title_in_two_libraries_makes_two_series_and_two_works`: `1` work
   and `1` series.
2. Key the group on `root.id` instead of `root.library_id` (the key only).
   Expected FAIL in `two_folders_of_one_library_still_make_one_work…`: `2`
   works. This is the asymmetry the test exists for: root and library differ
   there and nowhere else.
3. `reconcile_series`'s `(library_id = ?2 OR library_id IS NULL)` →
   `(library_id = ?2 OR 1 = 1)`. Expected FAIL in the same-title test: series
   count `1`, or a work in another library's series.
4. `find_by_key`'s `library_id = ?2 OR library_id IS NULL` → `1 = 1 OR …` (keep
   `?2` bound). Expected FAIL in
   `byte_identical_volumes_in_two_libraries_are_two_works_with_two_slugs` and
   `scanned_in_finds_its_own_librarys_work_and_not_anothers`.
5. Remove `OR library_id IS NULL` from `find_by_key` only. Expected FAIL in
   `scanned_in_adopts_a_work_in_no_library`.
6. Remove `OR library_id IS NULL` from `find_by_key`, `held_work` and
   `reconcile_series` together. Expected FAIL in
   `a_folder_moved_out_of_a_library_it_was_alone_in_keeps_its_works` and
   `a_series_and_a_work_in_no_library_are_adopted_by_the_next_derive`. Then
   each of the three alone: report which tests each alone fails. If one alone
   fails nothing, say so; that is a finding, not a reason to add a test here.
7. Comment out the `crate::fold::absorb` call's match in `derive_with` (leave
   the loop reading `superseded`). Expected FAIL in
   `a_folder_moved_to_another_library_carries_positions_and_notes_through_the_fold`.
8. In `librarydoc::find_series_by_title`, `s.library_id = ?2 OR s.library_id IS NULL`
   → `1 = 1 OR …`. Expected FAIL in `a_hand_document_finds_its_series_in_its_own_library`.
9. In `update_work`'s `Scope::Scan` statement, replace
   `COALESCE(?3, library_id)` with
   `CASE WHEN ?3 IS NULL THEN library_id ELSE library_id END` (never stamps,
   `?3` still bound). Expected FAIL in `scanned_in_adopts_a_work_in_no_library`:
   `None` where `Some(70)` was expected.

- [ ] **Step 8: Whole crates, lint, format, and commit**

Run Task 1 Step 7's four commands. Then commit `derive.rs`, `import.rs`,
`librarydoc.rs`, the two test files and every fixture Step 6 changed:

```bash
git commit -m "Derivation within a library: series, works and content found in their own library"
```

---

## Task 4: Proposals within a library

**Files:**
- Modify: `crates/taiyaku-library/src/proposals.rs`
- Modify: `crates/taiyaku-library/src/derive.rs` (`rel_paths_by_series`, `rel_paths_for_series`)
- Modify: `crates/taiyaku-library/src/proposals/tests.rs`, and fixtures Step 1 lists

**Interfaces:**
- `derive::rel_paths_by_series(conn, series: &BTreeSet<(Option<i64>, &str)>) -> Result<BTreeMap<(Option<i64>, String), Vec<(i64, String)>>, String>`:
  keyed by `(library, title)`, each answered from that library's folders only;
  a `None` library has no folder and answers empty.
- `derive::rel_paths_for_series(conn, library: Option<i64>, series: &str)`.
- `proposals::series_id_by_sort_title(conn, library: Option<i64>, sort_title)` (private).

- [ ] **Step 1: List the tests whose premise this changes**

As Task 3 Step 1: temporarily, in `raise_edition_of` after the prefix check
and in `raise_same_series` once `owner` is found, panic with `PREMISE:` when
the two series' libraries differ (read `library_id` into `SeriesRow` first,
Step 2's item 1). Run the three crates `--no-fail-fast`, keep the log as
`premise-proposals.log`, list the tests, remove the probe.

- [ ] **Step 2: The changes**

1. `SeriesRow` gains `library_id: Option<i64>`; `all_series` selects it.
2. `raise_edition_of`: after `if suffixed.id == base.id { continue; }`:
   ```rust
   // **Within one library — spec decision 3.** An Ecchi series is never
   // offered as an edition of a Volumes one.
   if suffixed.library_id != base.library_id {
       continue;
   }
   ```
3. `raise_same_series`'s owner query:
   ```sql
   SELECT st.series_id FROM series_title st JOIN series s ON s.id = st.series_id
    WHERE st.title = ?1 AND st.series_id != ?2 AND s.library_id IS ?3
    ORDER BY st.series_id LIMIT 1
   ```
   binding `candidate.library_id` third.
4. `series_id_by_sort_title(conn, library: Option<i64>, sort_title)`:
   `SELECT id FROM series WHERE sort_title = ?1 AND library_id IS ?2 ORDER BY id LIMIT 1`.
5. `raise_attach_chapters`: map each directory's root to its library
   (`crate::scan::roots(conn)?` into a `BTreeMap<i64, i64>`), count per
   `(series_name, library_id)`, and look the subject up with
   `series_id_by_sort_title(conn, Some(library_id), &series_name)`.
6. `series_reached_by`: the memo becomes `BTreeMap<(i64, String), Option<i64>>`
   keyed by `(root.library_id, current)`, and the lookup passes
   `Some(root.library_id)`.
7. `derive::rel_paths_by_series` takes `&BTreeSet<(Option<i64>, &str)>`, keys
   `found` by `(Option<i64>, String)`, and in the loop looks up
   `(Some(root.library_id), admitted.series)`. `rel_paths_for_series(conn,
   library, series)` wraps it for one key. Keep the single sweep and its
   `REL_PATHS_BY_SERIES_CALLS` counter.
8. A helper `fn series_library(conn, series_id) -> Result<Option<i64>, String>`.
   `preview` passes the subject's library to `rel_paths_for_series`;
   `evidence` builds its set from `(subject_library, subject_title)` and
   `(target_library, target_title)` and reads `paths_of` by the same pairs.
9. The cross-library refusal:
   ```rust
   /// Refuse a proposal whose two series are in different libraries — one
   /// raised before F1, or left behind by a folder moved between libraries.
   /// A rule moves material within the library its folder is in, so accepting
   /// would mint a same-titled series in the subject's library rather than
   /// join the target.
   fn across_libraries(
       conn: &Connection,
       proposal: &Proposal,
       target_id: i64,
   ) -> Result<Option<String>, String> {
       let subject = series_library(conn, proposal.subject_id)?;
       let target = series_library(conn, target_id)?;
       Ok((subject != target).then(|| {
           format!(
               "proposal {}: its two series are in different libraries, and a rule moves \
                material only within one. Dismiss it; the next raise proposes only within a \
                library.",
               proposal.id
           )
       }))
   }
   ```
   Call it in `acceptable_proposal` (read the proposal with `one`, run
   `acceptable`, then this) and in `offered_rule` right after `acceptable`.
   Update `acceptable`'s doc: its three refusals need no read, and this fourth
   one does, so it lives beside them in the two callers that hold a
   connection.

- [ ] **Step 3: Write the tests**

In `crates/taiyaku-library/src/proposals/tests.rs`, reuse the file's own
scanning helpers. Each test builds one library where the proposal *is* raised
and checks the same shape split across two is not, so the test cannot pass by
raising nothing at all.

1. `an_edition_of_is_raised_within_a_library_and_never_across_two`:
   `Go-Toubun no Hanayome/… - v01` and `Go-Toubun no Hanayome - Full Color Edition/… - v01`
   scanned as two folders of one named library: one `edition-of`. The same two
   as two libraries (two `add_root`): none.
2. `a_same_series_is_raised_within_a_library_and_never_across_two`: series
   `Kaguya-sama` gains a `series_title` row `Kaguya` (direct insert, `hand =
   1`), and a series `Kaguya` exists. Same library: one `same-series`. Two
   libraries: none.
3. `attach_chapters_names_the_series_in_the_chapters_own_library`: library A
   holds `Kaguya-sama/Kaguya-sama - v01` (derived first, so its series has
   the lower id); library B holds `Kaguya-sama/Kaguya-sama - c001` and
   `- c002`. The one `attach-chapters` proposal's `subject_id` is B's series.
4. `accepting_reads_only_the_subjects_own_library`: an `edition-of` raised in
   library B, with a series of the subject's title also present in library A
   (its own folder, same directory name). `preview` offers a rule rather than
   refusing "spelled the same in two roots", and the rule's `root_id` is B's
   folder.
5. `a_proposal_across_two_libraries_is_refused_at_accept`: two libraries,
   series `Base` in A and `Base - Full Color` in B, and a proposal inserted
   directly (`INSERT INTO proposal (kind, subject_id, target_id, score,
   reason) VALUES ('edition-of', <B's>, <A's>, 1, 'x')`). `preview` refuses,
   naming "different libraries"; `accept` refuses with the same words; the
   proposal stays `open`.
6. `a_dry_run_counts_each_librarys_series_apart`: two libraries each with a
   series `Series`, and a candidate `ProposedRule` matching both folders' entries
   (a `prefix` rule on `Series/` with `root_id: None` and `set_series:
   Some("Elsewhere")`, so the candidate decides something and `series_reached_by`
   counts its matches). It reaches two series ids, not one.

- [ ] **Step 4: Run, and watch each fail against its defect**

1. Delete item 2's `continue`. Expected FAIL in test 1.
2. `s.library_id IS ?3` → `(s.library_id IS ?3 OR 1 = 1)`. Expected FAIL in test 2.
3. In `raise_attach_chapters`, pass `None` as the library. Expected FAIL in
   test 3 (`None` names no series and the proposal is not raised), or the
   subject is A's: report which.
4. In `rel_paths_by_series`, look up `(found keys)` ignoring the library: key
   the loop's lookup on any library with that title. Expected FAIL in test 4.
5. Delete both `across_libraries` calls. Expected FAIL in test 5.
6. Key `series_reached_by`'s memo on `current` alone. Expected FAIL in test 6.

Also confirm `REL_PATHS_BY_SERIES_CALLS`'s existing test still passes: one
sweep per `evidence` call.

- [ ] **Step 5: Bring the listed fixtures onto one library**, by Task 3 Step 6's rule.

- [ ] **Step 6: Whole crates, lint, format, and commit**

```bash
git commit -m "Proposals within a library: pairs, chapters and accepts never cross one"
```

---

## Task 5: Grants through libraries

**Files:**
- Modify: `crates/taiyaku-library/src/account.rs`, `account/tests.rs`
- Modify: `crates/taiyaku-cli/src/user.rs`, `user/tests.rs`
- Modify: `crates/taiyaku-serve/src/test_support.rs` (the two-folder grant fixture)
- Modify: `crates/taiyaku-serve/src/{auth,library_api,opds,routes,users_api}/tests.rs`

**Interfaces:**
- Produces in `account`:
  - `libraries_for(conn, user_id) -> Result<Vec<i64>, String>` (ascending),
  - `grant_library(conn, user_id, library_id) -> Result<bool, String>` (`true` if new),
  - `revoke_library(conn, user_id, library_id) -> Result<bool, String>`,
  - `granted_libraries(conn, user_id) -> Result<Vec<(i64, String)>, String>` (by name).
  - `roots_for` and `granted_roots` unchanged in signature: every folder of every granted library.
  - `grant_root` and `revoke_root` are deleted.
- CLI: `taiyaku library user grant-library|revoke-library <user> <library> [--db PATH]`.

- [ ] **Step 1: The `Principal.roots` families, enumerated**

```bash
grep -rn "\.roots\b" --include='*.rs' crates/taiyaku-serve/src | grep -v "/tests.rs:\|test_support" >| .superpowers/sdd/libraries-f1-evidence/principal-roots.txt
grep -rn "roots_for(\|granted_roots(" --include='*.rs' crates/taiyaku-serve/src | grep -v "/tests.rs:" >> .superpowers/sdd/libraries-f1-evidence/principal-roots.txt
cat .superpowers/sdd/libraries-f1-evidence/principal-roots.txt
```

At `d1630cbb` the families were:

| Family | Sites | Test in Step 5 |
| --- | --- | --- |
| The principal: `roots_for` | `auth.rs` (basic, key), `session.rs` | `account::tests` + one `auth::tests` case |
| Browse | `library_api.rs`: home, series grid, unfiled, one series, work, search, cover, status | `library_api::tests` |
| OPDS | `opds.rs` `visible_editions` | `opds::tests` |
| Reader and material | `routes.rs`: works list, edition refusal, work lookup, alignment sides | `routes::tests` |
| Users API | `users_api.rs`: `granted_roots`, `Patch::roots` | `users_api::tests` |
| Metadata and stamp | `metadata.rs`, `stamp.rs` | none: every handler is behind `administering_or_forbidden`, and an administrator's roots are empty (O86). Confirm each site is behind it and say so |

If the grep finds a site in none of these, stop and report it.

- [ ] **Step 2b: Refuse emptying a library that is someone's only grant (decision C)**

Wherever a folder is removed (`scan::remove_root`, its dry run, and whatever
F1 adds that moves a root between libraries), before anything is deleted:
when the folder is its library's last, and one or more accounts hold grants
consisting of that library alone, refuse with an error that names each such
account and says to grant them another library (or make them unrestricted)
first. The dry run reports the same refusal rather than counts. Back it with
a `BEFORE DELETE ON root` / `BEFORE UPDATE OF library_id ON root` trigger that
`RAISE(ABORT, …)`s on the same condition, so a path that bypasses the Rust
check cannot widen an account either (SQLite's `RAISE` message is a literal,
so the trigger's text cannot name accounts; the Rust check does).

Tests, each watched failing against its defect (copy aside, edit, run,
restore, `cmp`):
- removing the last folder of a library that is an account's only grant is
  refused, the message names the account, and nothing was deleted (the
  folder, its works, the account's grant all remain);
- the same removal succeeds when that account also holds another library;
- the same removal succeeds when no account is restricted to it;
- the dry run reports the refusal;
- the trigger alone refuses a raw `DELETE FROM root` on that condition.

- [ ] **Step 2: `account.rs`**

Replace `grant_root` and `revoke_root` with the four library functions, and
rewrite `roots_for`'s and `granted_roots`' docs to say what they resolve:

```rust
/// The libraries this account was granted, ascending.
pub fn libraries_for(conn: &Connection, user_id: i64) -> Result<Vec<i64>, String> {
    let mut statement = conn
        .prepare("SELECT library_id FROM user_library WHERE user_id = ?1 ORDER BY library_id")
        .map_err(|e| format!("preparing the granted-libraries lookup: {e}"))?;
    let rows = statement
        .query_map(params![user_id], |row| row.get(0))
        .map_err(|e| format!("reading granted libraries: {e}"))?;
    rows.collect::<rusqlite::Result<Vec<i64>>>()
        .map_err(|e| format!("reading granted libraries: {e}"))
}

/// Grant `user_id` sight of `library_id`: every folder filed under it.
///
/// **Granting is restricting** (keep the paragraph `grant_root` had). The
/// first grant an account receives is the moment it stops seeing everything.
///
/// # Errors
///
/// Returns an error if the insert fails — which includes a `library_id` no
/// library row has, since the column REFERENCES `library(id)`.
pub fn grant_library(conn: &Connection, user_id: i64, library_id: i64) -> Result<bool, String> {
    let changed = conn
        .execute(
            "INSERT OR IGNORE INTO user_library (user_id, library_id) VALUES (?1, ?2)",
            params![user_id, library_id],
        )
        .map_err(|e| format!("granting library {library_id} to user {user_id}: {e}"))?;
    Ok(changed == 1)
}

/// Take back a grant, reporting whether there was one. **Revoking the last
/// grant makes the account unrestricted again** (keep `revoke_root`'s
/// paragraph, saying library where it said root).
pub fn revoke_library(conn: &Connection, user_id: i64, library_id: i64) -> Result<bool, String> {
    let changed = conn
        .execute(
            "DELETE FROM user_library WHERE user_id = ?1 AND library_id = ?2",
            params![user_id, library_id],
        )
        .map_err(|e| format!("revoking library {library_id} from user {user_id}: {e}"))?;
    Ok(changed == 1)
}

/// The libraries granted to `user_id`, as `(id, name)`, by name.
pub fn granted_libraries(conn: &Connection, user_id: i64) -> Result<Vec<(i64, String)>, String> {
    let mut statement = conn
        .prepare(
            "SELECT l.id, l.name FROM user_library ul JOIN library l ON l.id = ul.library_id
              WHERE ul.user_id = ?1 ORDER BY l.name",
        )
        .map_err(|e| format!("preparing the granted-libraries listing: {e}"))?;
    let rows = statement
        .query_map(params![user_id], |row| Ok((row.get(0)?, row.get(1)?)))
        .map_err(|e| format!("listing granted libraries: {e}"))?;
    rows.collect::<rusqlite::Result<Vec<(i64, String)>>>()
        .map_err(|e| format!("listing granted libraries: {e}"))
}
```

`roots_for`'s doc gains: "**Every folder of every library this account was
granted — Part F1.** A grant names a library, and the guard filters on
folders, so this is where one becomes the other. An empty answer still means
granted nothing, and so unrestricted: a granted library always has a folder
(see `crate::libraries`)."

`update`'s roots branch becomes a library-level diff:

```rust
if let Some(wanted) = &patch.roots {
    // **Diffed by library, not by root — Part F1.** The Users screen still
    // sends folders, and a grant names a library. A root-level diff would
    // revoke a whole library for one folder of it left unticked.
    let mut wanted_libraries = std::collections::BTreeSet::new();
    for root_id in wanted {
        // (keep the existing comment on why a missing root is the caller's
        // mistake, saying `user_library` where it says `user_root`)
        let library: Option<i64> = conn
            .query_row("SELECT library_id FROM root WHERE id = ?1", params![root_id], |row| {
                row.get(0)
            })
            .optional()
            .map_err(|e| {
                WriteError::Failed(format!("checking whether root {root_id} exists: {e}"))
            })?;
        let Some(library) = library else {
            return Err(WriteError::Unusable(format!("there is no root {root_id} to grant")));
        };
        wanted_libraries.insert(library);
    }
    let held: std::collections::BTreeSet<i64> =
        libraries_for(conn, id).map_err(WriteError::Failed)?.into_iter().collect();
    for library in held.difference(&wanted_libraries) {
        revoke_library(conn, id, *library).map_err(WriteError::Failed)?;
    }
    for library in wanted_libraries.difference(&held) {
        grant_library(conn, id, *library).map_err(WriteError::Failed)?;
    }
}
```

`Patch::roots`' doc says it is a list of folders whose libraries are granted.

- [ ] **Step 3: `account/tests.rs`**

Every `grant_root(&conn, id, root)` becomes `grant_library(&conn, id,
library_of(&conn, root))` with a helper reading `root.library_id`, and the two
`revoke_root` calls likewise. Rename the three tests named for roots
(`granting_a_root_is_idempotent…`, `revoking_a_root_says…`,
`granted_roots_names_the_paths_for_display`) only where the subject changed,
and add:

```rust
/// Ids that differ between libraries and roots, so a join on the wrong pair
/// cannot pass by coincidence.
fn a_two_folder_library_and_another(conn: &rusqlite::Connection) {
    conn.execute_batch(
        "INSERT INTO library (id, name) VALUES (70, 'Shelf'), (80, 'Other');
         INSERT INTO root (id, path, library_id) VALUES
           (7, '/srv/manga', 70), (8, '/srv/raw', 70), (9, '/srv/other', 80);",
    )
    .expect("a library of two folders and one of one");
}

#[test]
fn a_granted_library_resolves_to_every_one_of_its_folders() {
    let library = claimed_library();
    let conn = library.conn();
    a_two_folder_library_and_another(&conn);
    let id = add(&conn, "reader", "pw", "user").expect("adds an account");

    assert!(grant_library(&conn, id, 70).expect("grants"), "a new grant");
    assert!(!grant_library(&conn, id, 70).expect("grants again"), "and not a second one");

    assert_eq!(roots_for(&conn, id).expect("reads"), vec![7, 8], "both folders, not Other's");
    assert_eq!(libraries_for(&conn, id).expect("reads"), vec![70]);
    assert_eq!(granted_libraries(&conn, id).expect("reads"), vec![(70, "Shelf".to_string())]);
}

#[test]
fn a_roots_list_naming_one_folder_of_a_library_keeps_the_whole_library() {
    let library = claimed_library();
    let conn = library.conn();
    a_two_folder_library_and_another(&conn);
    let id = add(&conn, "reader", "pw", "user").expect("adds an account");
    grant_library(&conn, id, 70).expect("grants Shelf");

    update(&conn, id, &Patch { roots: Some(vec![7]), ..Patch::default() }, None)
        .expect("updates");

    assert_eq!(libraries_for(&conn, id).expect("reads"), vec![70], "Shelf is still granted");
    assert_eq!(roots_for(&conn, id).expect("reads"), vec![7, 8]);

    update(&conn, id, &Patch { roots: Some(vec![9]), ..Patch::default() }, None)
        .expect("updates");
    assert_eq!(libraries_for(&conn, id).expect("reads"), vec![80], "replaced, not appended");
}
```

- [ ] **Step 4: The CLI's `grant-library` and `revoke-library`**

In `crates/taiyaku-cli/src/user.rs`:

1. `command` dispatches `grant-library` and `revoke-library`, and answers
   `grant-root` and `revoke-root` with
   `"`{verb}` is now `{new}`: a grant names a library. `taiyaku library libraries` lists them"`.
   The two error texts listing subcommands say `grant-library and
   revoke-library`.
2. `ROOT_USAGE` becomes
   ```rust
   const LIBRARY_USAGE: &str = "usage: taiyaku library user grant-library <user> <library> [--db PATH]\n \
                                taiyaku library user revoke-library <user> <library> [--db PATH]";
   ```
3. `root_by_path` goes. `grant_target` resolves the second positional with
   `taiyaku_library::libraries::id_of`, refusing with
   `"no library named `{name}`; `taiyaku library libraries` lists them"`.
4. `grant_library`/`revoke_library` keep the old functions' shape:
   `account::grant_library`, `account::granted_libraries` for the narrowing
   count, "granted sight of {name}", "that account could already see {name};
   nothing to do", "revoked sight of {name}", "that account had no grant for
   {name}; nothing to revoke", and "That account now holds no grants, and so
   sees every library again." The unrooted note and its count stay.
5. `narrowed_note` says "restricted to that one library. An account with no
   grants at all sees every library." `admin_note` says "an administrator
   always sees every library".

In `user/tests.rs`, rewrite each grant-root test as its grant-library
counterpart, keeping what each one pins:

| Was | Becomes | Pins |
| --- | --- | --- |
| `granting_a_root_by_path_resolves_it_to_the_row` | `granting_a_library_by_name_resolves_it_to_the_row` | `libraries_for == [the id]` |
| `revoking_a_root_takes_the_grant_back` | `revoking_a_library_takes_the_grant_back` | empty afterwards |
| `granting_a_root_this_library_does_not_hold_names_the_path` | `granting_a_library_that_does_not_exist_names_it` | the name in the error |
| `granting_a_root_to_an_account_that_does_not_exist_names_the_account` | same, for a library | the account in the error |
| `granting_a_root_twice_says_nothing_changed` | `granting_a_library_twice_says_nothing_changed` | one grant |
| `revoking_a_root_that_was_never_granted_is_not_an_error` | for a library | `Ok` |
| `revoking_a_root_whose_directory_is_gone_still_works` | deleted | a library is named, not canonicalised; say so in the commit's report |
| `the_root_subcommands_state_their_usage` | `the_library_subcommands_state_their_usage` | `grant-library`, `<library>` |
| `granting_names_the_root_it_was_given_and_not_another` | `granting_names_the_library_it_was_given_and_not_another` | two libraries, the named one granted |
| `the_narrowing_note_…`, `the_admin_note_…` | the same names | the new wording, `"every library"` |

Add `the_old_grant_root_names_its_replacement`: `command(["grant-root", "davey", "x"])`
errs containing `grant-library`. And `a_missing_subcommand_names_the_ones_that_exist`'s
expected list says `grant-library and revoke-library`.

Build the two-library fixtures with `taiyaku_library::scan::add_root_in(…,
InLibrary::Named("Manga"))`.

- [ ] **Step 5: One router test per family**

Add to `crates/taiyaku-serve/src/test_support.rs`, modelled on
`routes::tests::router_with_two_works_one_withheld` (three works imported with
`import::scanned`, each member then filed under a root by `UPDATE member`):

```rust
/// Three works: one under each folder of the two-folder library `Shelf`, and
/// one under `Other`. The fixture account is granted `Shelf` through
/// [`grant_root`] on its first folder, so a test reads both of Shelf's works
/// and neither of Other's.
///
/// **Library ids differ from root ids** (70 and 80 against 1, 2 and 3), so a
/// grant that resolved a library id as if it were a root id would see one of
/// Shelf's works and not the other.
pub(crate) fn router_over_a_granted_two_folder_library(
    dir: &std::path::Path,
) -> (axum::Router, std::sync::Arc<ServeState>)
```

with roots and libraries written as

```sql
INSERT INTO library (id, name) VALUES (70, 'Shelf'), (80, 'Other');
INSERT INTO root (id, path, library_id) VALUES
  (1, '/shelf-a', 70), (2, '/shelf-b', 70), (3, '/other', 80);
```

and works titled `First Folder Work`, `Second Folder Work`, `Other Library
Work` under roots 1, 2 and 3. Then one test in each family's test file:

1. `auth::tests`: `a_principal_holds_every_folder_of_a_granted_library` — the
   principal built for the fixture account (through whatever `auth::tests`
   already uses to build one from the key) has `roots == [1, 2]`.
2. `library_api::tests`: `the_grid_shows_every_folder_of_a_granted_library` —
   `GET /api/library/search?q=Work` (or the grid, whichever lists works by
   title in this file's other grant tests) names the first two works and not
   the third.
3. `opds::tests`: `the_works_feed_names_every_folder_of_a_granted_library` —
   `GET /opds/v1.2/works` has the first two titles and not the third.
4. `routes::tests`: `the_work_list_names_every_folder_of_a_granted_library` —
   `GET /api/works` likewise, and `GET /api/works/<third slug>` answers what
   `the_work_list_does_not_name_a_work_outside_the_grant`'s neighbours expect
   for a work outside the grant.
5. `users_api::tests`: `the_users_screen_lists_and_keeps_a_libraries_folders`
   — `GET /api/library/users` shows the fixture account's `roots` as `[1, 2]`;
   `PATCH` with `{"roots": [1]}` and the list still shows `[1, 2]`.

- [ ] **Step 6: Run, and watch each fail against its defect**

1. `roots_for`'s join `r.library_id = ul.library_id` → `r.id = ul.library_id`.
   Expected FAIL: `a_granted_library_resolves_to_every_one_of_its_folders`
   (`[]` or wrong ids), and each of Step 5's five. Report every test it fails;
   a family test that passes is a test that does not reach `roots_for`.
2. `roots_for` with `LIMIT 1` added. Expected FAIL: the same tests, each
   showing only the first folder's work.
3. `update` diffed by root (restore the old loop over `roots_for`, calling
   `revoke_library` with the root's library). Expected FAIL in
   `a_roots_list_naming_one_folder_of_a_library_keeps_the_whole_library`, and
   in `the_users_screen_lists_and_keeps_a_libraries_folders`.
4. In `grant_target`, resolve the library by `name LIKE ?1` against `"%"`.
   Expected FAIL in `granting_names_the_library_it_was_given_and_not_another`.

- [ ] **Step 7: Whole crates, lint, format, and commit**

```bash
git commit -m "Grants through libraries: an account is granted libraries, and sees every folder of each"
```

---

## Task 6: The CLI — `libraries`, `scan --library`, usage

**Files:**
- Modify: `crates/taiyaku-cli/src/library.rs`, `library/tests.rs`
- Modify: `crates/taiyaku-cli/src/main.rs` (`print_usage`)
- Modify: `crates/taiyaku-cli/tests/cli.rs`

**Interfaces:**
- `taiyaku library libraries [--db PATH]`.
- `taiyaku library scan [--add PATH] [--library NAME | --root PATH] [--no-derive] [--db PATH]`.
- `fn select_roots(roots: Vec<Root>, root: Option<&str>, library: Option<i64>) -> Result<Vec<Root>, String>`.
- `fn library_line(listed: &Listed) -> String`, `fn unfiled_line(works: usize) -> Option<String>` (pure, for their tests).

- [ ] **Step 1: `libraries`**

`command` dispatches `Some("libraries") => libraries(&args[1..])`, and both
error texts list `libraries` after `scan`.

```rust
/// `taiyaku library libraries [--db PATH]`: every library, its folders, and
/// how many works each holds, then how many works are in none.
fn libraries(args: &[String]) -> Result<(), String> {
    reject_unknown_flags("library libraries", args, &["--db"])?;
    reject_positional_arguments("library libraries", args, &["--db"], "it takes flags only")?;
    let db = resolve_db_path(option_value(args, "--db", "a path")?)?;
    let library = Library::open(&db).map_err(|e| format!("opening the library: {e}"))?;
    let conn = library.conn();
    let listed = taiyaku_library::libraries::list(&conn)?;
    if listed.is_empty() {
        println!("no libraries yet; `taiyaku library scan --add PATH` adds a folder as one");
    }
    for one in &listed {
        println!("{}", library_line(one));
        for folder in &one.folders {
            println!("  {}", folder.display());
        }
    }
    if let Some(line) = unfiled_line(taiyaku_library::libraries::unfiled_works(&conn)?) {
        println!("{line}");
    }
    Ok(())
}

/// One library's heading line.
fn library_line(listed: &taiyaku_library::libraries::Listed) -> String {
    format!(
        "{}: {} folder(s), {} work(s)",
        listed.name,
        listed.folders.len(),
        listed.works
    )
}

/// The works in no library, said only when there are some. Extracted for the
/// reason `user::unrooted_note` was: a comparison inside a `println!`
/// survives mutation.
fn unfiled_line(works: usize) -> Option<String> {
    (works > 0).then(|| {
        format!("{works} work(s) in no library: imported from a `taiyaku.json`, not scanned")
    })
}
```

- [ ] **Step 2: `scan --library`**

In `scan`: accept `--library` (a value flag, in both flag lists), refuse it
with `--root`:

```rust
let library_name = option_value(args, "--library", "a library name")?;
if root_filter.is_some() && library_name.is_some() {
    return Err(
        "`--root` and `--library` each choose which folders are walked; pass one of them"
            .to_string(),
    );
}
```

`--add` files the folder under `InLibrary::Named(name)` when `--library` was
given and `InLibrary::Own` otherwise, through `add_root_checked_in`. Then
resolve the filter after the add (so `--add X --library New` walks `New`):

```rust
let library_filter = match library_name {
    Some(name) => Some(
        taiyaku_library::libraries::id_of(&library.conn(), name)?
            .ok_or_else(|| format!("no library named `{name}`; `taiyaku library libraries` lists them"))?,
    ),
    None => None,
};
let selected = select_roots(roots, root_filter, library_filter)?;
```

`select_roots` keeps its two existing refusals and, with a library, keeps
only that library's roots. Update its doc and `scan`'s doc ("every known
root, or one library's, or one root").

In `main.rs`'s `print_usage`:

```rust
println!("       taiyaku library libraries [--db PATH]");
println!(
    "       taiyaku library scan  [--add PATH] [--library NAME | --root PATH] [--no-derive] \
     [--db PATH]  (--add files a folder under --library, creating it; alone, as a library of its own)"
);
println!("       taiyaku library user grant-library|revoke-library <user> <library> [--db PATH]");
```

replacing the old `library scan` line.

- [ ] **Step 3: Tests**

In `library/tests.rs`: the four existing `select_roots` calls gain `None`, and add

```rust
#[test]
fn select_roots_keeps_only_the_named_librarys_folders() {
    let dir = TempDir::new("select-roots-library");
    let roots = vec![
        taiyaku_library::scan::Root { library_id: 70, ..a_root(1, &dir.join("a")) },
        taiyaku_library::scan::Root { library_id: 80, ..a_root(2, &dir.join("b")) },
        taiyaku_library::scan::Root { library_id: 70, ..a_root(3, &dir.join("c")) },
    ];
    let selected = select_roots(roots, None, Some(70)).expect("selects");
    assert_eq!(selected.iter().map(|r| r.id).collect::<Vec<_>>(), vec![1, 3]);
}

#[test]
fn the_unfiled_line_appears_only_when_works_are_in_no_library() {
    assert_eq!(unfiled_line(0), None);
    assert!(unfiled_line(3).expect("says it").contains('3'));
}
```

(Use whatever `TempDir`/path helper the neighbouring `select_roots` tests use;
`select_roots` with no filter never touches the disk.)

In `tests/cli.rs`:

```rust
#[test]
fn library_libraries_lists_each_library_with_its_folders() {
    let tree = TempDir::new("cli-libraries");
    let (volumes, chapters, comics) = (tree.join("Volumes"), tree.join("Chapters"), tree.join("Comics"));
    for dir in [&volumes, &chapters, &comics] {
        std::fs::create_dir_all(dir).expect("mkdir");
        std::fs::write(dir.join("placeholder.txt"), b"x").expect("a file, so the root is not empty");
    }
    let db_dir = TempDir::new("cli-libraries-db");
    let db = db_path(&db_dir);
    for (dir, library) in [(&volumes, Some("Manga")), (&chapters, Some("Manga")), (&comics, None)] {
        let mut args = vec!["library", "scan", "--add", dir.to_str().unwrap()];
        if let Some(name) = library {
            args.extend(["--library", name]);
        }
        args.extend(["--no-derive", "--db", db.as_str()]);
        taiyaku_output(&args);
    }

    let out = taiyaku_output(&["library", "libraries", "--db", &db]);

    assert!(out.contains("Comics: 1 folder(s), 0 work(s)"), "{out}");
    assert!(out.contains("Manga: 2 folder(s), 0 work(s)"), "{out}");
    let manga = out.find("Manga:").expect("Manga listed");
    let canonical = |dir: &std::path::Path| dir.canonicalize().expect("canonical").display().to_string();
    assert!(out[manga..].contains(&format!("  {}", canonical(&volumes))), "{out}");
    assert!(out[manga..].contains(&format!("  {}", canonical(&chapters))), "{out}");
}

#[test]
fn library_scan_with_a_library_walks_only_its_folders() {
    // Two libraries; `scan --library A` prints a watch report line for A's
    // folder (`root <path>`) and none for B's.
}

#[test]
fn library_scan_refuses_a_library_it_does_not_hold_and_root_with_library() {
    // `scan --library Nope` fails naming `Nope` and `library libraries`;
    // `scan --root X --library Y` fails naming both flags.
}
```

Write the last two in full in the style of the first. `taiyaku_output`
panics on a non-zero exit; for the refusals, use `taiyaku()` and read
`status` and `stderr`, as this file's other refusal tests do. And extend
`the_usage_lists_library_scan_and_rules` with `taiyaku library libraries` and
`grant-library`.

- [ ] **Step 4: Run, and watch each fail against its defect**

1. In `select_roots`, ignore the library (return every root). Expected FAIL:
   `select_roots_keeps_only…` and `library_scan_with_a_library_walks_only_its_folders`.
2. In `scan`, file `--add` under `InLibrary::Own` whatever `--library` says.
   Expected FAIL in `library_libraries_lists…`: `Manga` missing.
3. `unfiled_line`'s `> 0` → `>= 0`. Expected FAIL in its test.
4. Delete the `--root`/`--library` refusal. Expected FAIL in the refusal test.

- [ ] **Step 5: Whole crates, lint, format, and commit**

```bash
git commit -m "CLI: library libraries, scan --library, and the usage that names them"
```

---

## Task 7: On a copy of the real library

**Files:**
- Create: `.superpowers/sdd/libraries-f1-evidence/drive.log` and siblings (gitignored)

This proves on real data that every work survives migration 15 into one
library, that a derive over it changes none of them, that the same title in a
second library is a second series, and that the server, OPDS and a grant
behave as before. Nothing in it is committed. Figures measured while
planning, on a copy brought from 13 to 15 by the raw SQL: one library named
`Volumes`, all 1896 works in it, 134 series in it and 4 holding no work in
none.

- [ ] **Step 1: Set up, and record the original's digest**

Set `SCRATCH` to your session's scratchpad directory. Then:

```bash
S="$SCRATCH/libraries-f1-drive"
E=.superpowers/sdd/libraries-f1-evidence
mkdir -p "$S" "$E"
sha256sum /home/davey/temp/manga-test/library.db >| "$E/original-sha-before.txt"
cat "$E/original-sha-before.txt"
cp /home/davey/temp/manga-test/library.db "$S/library.db"
touch "$S/marker"
cargo build --release -p taiyaku-cli
sqlite3 "$S/library.db" "SELECT value FROM meta WHERE key = 'schema_version'"
sqlite3 "$S/library.db" "SELECT id, slug, content_key, title, series_id FROM work ORDER BY id" >| "$E/works-before.txt"
wc -l < "$E/works-before.txt"
```

Expected: the digest starts `d5ebc0c237865035`; version `13`; `1896` lines.

- [ ] **Step 2: Migrate by listing, and compare**

```bash
target/release/taiyaku library libraries --db "$S/library.db" | tee "$E/libraries-1.txt"
sqlite3 "$S/library.db" "SELECT value FROM meta WHERE key = 'schema_version'"
sqlite3 "$S/library.db" "SELECT id, slug, content_key, title, series_id FROM work ORDER BY id" | cmp - "$E/works-before.txt" && echo "works unchanged"
sqlite3 "$S/library.db" "SELECT library_id, count(*) FROM work GROUP BY library_id; SELECT library_id IS NULL, count(*) FROM series GROUP BY 1; INSERT INTO work_fts (work_fts) VALUES ('integrity-check'); SELECT 'fts ok';" | tee -a "$E/drive.log"
```

Expected: `Volumes: 1 folder(s), 1896 work(s)` and the folder's path;
version `15`; `works unchanged`; `1|1896`; series `0|134` and `1|4`; `fts ok`.

- [ ] **Step 3: A second library holding a title the first already has**

```bash
T=$(sqlite3 "$S/library.db" "SELECT sort_title FROM series s WHERE EXISTS (SELECT 1 FROM work w WHERE w.series_id = s.id) ORDER BY id LIMIT 1")
echo "title: $T" | tee -a "$E/drive.log"
mkdir -p "$S/other/$T/$T - v01"
for p in 000 001; do printf 'not really a jpeg: libraries drive %s\n' "$p" >| "$S/other/$T/$T - v01/$p.jpg"; done
R=$(realpath "$S/other")
target/release/taiyaku library scan --add "$R" --library "Drive Other" --db "$S/library.db" 2>&1 | tee -a "$E/drive.log"
sqlite3 "$S/library.db" "SELECT count(*) FROM work; SELECT l.name, count(*) FROM work w JOIN library l ON l.id = w.library_id GROUP BY l.name ORDER BY l.name; SELECT count(*) FROM series WHERE sort_title = (SELECT sort_title FROM series WHERE id = (SELECT min(id) FROM series s WHERE EXISTS (SELECT 1 FROM work w WHERE w.series_id = s.id)));" | tee -a "$E/drive.log"
sqlite3 "$S/library.db" "SELECT id, slug, content_key, title, series_id FROM work WHERE library_id = 1 ORDER BY id" | cmp - "$E/works-before.txt" && echo "Volumes' works unchanged by the derive"
```

`--library "Drive Other"` keeps the walk to the scratch folder: without it,
`library scan` walks every root, including
`/home/davey/temp/manga-test/Volumes`, and its watch probe writes into that
tree. The derive still runs over every library. Expected: the scan's report
names only `$R`; `1897` works; `Drive Other|1` and `Volumes|1896`; `2` series
of that title.

**If `Volumes' works unchanged` does not print**, do not stop yet: build
`main` at `d1630cbb` in a worktree, run the same Step 3 scan without
`--library` on a second fresh copy, and compare that copy's dump to
`works-before.txt` the same way. Only a difference `main` does not show is
this branch's. Report both diffs either way.

- [ ] **Step 4: The server, OPDS and a grant**

```bash
ss -ltn 'sport = :18486'
printf 'drive-pw\ndrive-pw\n' | target/release/taiyaku library user password local --db "$S/library.db"
printf 'reader-pw\nreader-pw\n' | target/release/taiyaku library user add reader --db "$S/library.db"
target/release/taiyaku library user grant-library reader "Drive Other" --db "$S/library.db" | tee -a "$E/drive.log"
```

With the Bash tool's `run_in_background`:

```bash
target/release/taiyaku serve --db "$S/library.db" --cache "$S/cache" --addr 127.0.0.1:18486 >| "$E/serve.log" 2>&1
```

Wait until `grep -q 'listening' "$E/serve.log"` succeeds. Then:

```bash
for who in local:drive-pw reader:reader-pw; do
  echo "== $who" | tee -a "$E/drive.log"
  curl -s -u "$who" http://127.0.0.1:18486/api/works | jq length | tee -a "$E/drive.log"
  curl -s -u "$who" http://127.0.0.1:18486/opds/v1.2/works | grep -c '<entry>' | tee -a "$E/drive.log"
  curl -s -u "$who" http://127.0.0.1:18486/api/library/status | jq '.roots | length' | tee -a "$E/drive.log"
done
```

Expected: the administrator `1897`, `1897`, `2`; the reader `1`, `1`, and
whatever `browse::status` answers a restricted account (report it). If
`user add` prompts differently or `ss` shows the port taken, adapt and report.

- [ ] **Step 5: Stop the server by PID and prove isolation**

```bash
ss -lptn 'sport = :18486'
kill <the pid ss printed>
sha256sum /home/davey/temp/manga-test/library.db | tee -a "$E/drive.log"
find /home/davey/temp/manga-test -newer "$S/marker" | tee -a "$E/drive.log" | wc -l
tail -20 "$E/serve.log"
```

Kill by PID, never by pattern. Expected: the digest unchanged, `find` prints
`0`, and `serve.log` holds no "could not refresh".

- [ ] **Step 6: Report**

Every figure above, as measured, for Task 8's notes entry. If any
expectation failed, stop and report it; do not continue to Task 8.

---

## Task 8: Mutation sweep, docs, the gate, and the claim check

**Files:**
- Modify: `docs/status.md`, `docs/notes.md`, `README.md`, `docs/mutants-baseline.md`, and the comments Step 3 finds

- [ ] **Step 1: Mutation sweep on what this branch touched**

Commit everything first: `--since main` reads `git diff main...HEAD`.

```bash
git status --porcelain
NEXTEST_TEST_THREADS=8 RUST_TEST_THREADS=8 scripts/mutants.sh --since main >| .superpowers/sdd/libraries-f1-evidence/mutants.log 2>&1; echo "exit $?"
tail -30 .superpowers/sdd/libraries-f1-evidence/mutants.log
```

For every survivor in the files this branch changed: hand-apply it, run all
three crates (a mutant only another crate's tests catch reads MISSED here,
O87), then kill it with a test watched failing first, or record it in
`docs/mutants-baseline.md` with its reason. Read the unviable count (O88).

**cargo-mutants never mutates a string literal**, and most of F1 is SQL inside
one: migration 15, and the `WHERE` clauses of `find_by_key`,
`reconcile_series`, `held_work`, `roots_for`, `find_series_by_title` and
`raise_same_series`. The hand-applied defects in Tasks 1 to 6 are those
statements' only mutation evidence; say so in the log row. Add the row to
the baseline's run log with the date, `libraries-f1, --since main (N mutants,
M m)` and the result. Commit as `Mutation sweep of the libraries-f1 branch`.

- [ ] **Step 2: Sweep the counts on the way in**

```bash
grep -c '^| O[0-9]' docs/status.md                 # expected 186
grep -c '^| O[0-9]* | \[x\]' docs/status.md        # expected 120
grep -nE '[0-9]{4} tests|standing checks gated' docs/status.md
grep -n -A1 '^What remains:' docs/status.md
```

If a figure differs, stop and report.

- [ ] **Step 3: Comments that name what F1 replaced**

```bash
grep -rn "user_root\|grant-root\|revoke-root\|grant_root\|revoke_root\|root_by_path" --include='*.rs' crates | grep -v "schema/00\|schema/01[0-4]"
```

Each hit outside a migration file either describes the present (fix it to say
`user_library`, `grant-library`, a library) or is a test's history (leave it,
and say so in the report). Known at `d1630cbb`: `auth.rs` (the `Principal`
doc and `from_account`'s), `routes.rs` (one comment), `browse.rs` (module doc
and `root_clause`'s), `scan.rs` (`apply_removal`, done in Task 2),
`account/tests.rs`, `schema/tests.rs`. Do not edit an applied migration file.

- [ ] **Step 4: `docs/status.md`**

1. The synopsis block: replace
   `taiyaku library user grant-root|revoke-root <name> <root-path> [--db PATH]`
   with `taiyaku library user grant-library|revoke-library <name> <library> [--db PATH]`;
   replace the `library scan` line with
   `taiyaku library scan  [--add PATH] [--library NAME | --root PATH] [--no-derive] [--db PATH]`;
   add `taiyaku library libraries                     [--db PATH]` after it.
2. After the "**It can be deployed.**" paragraph, add:

   > **The store keeps libraries.** A library is a name and one or more
   > folders (migration 15), and a grant names a library rather than a folder.
   > Derivation and proposals stay inside one library: the same title in two
   > libraries is two series, and a byte-identical volume in two is two works.
   > `taiyaku library libraries` lists them, `library scan --add PATH --library
   > NAME` files a folder under one, and `library user grant-library` grants
   > one. The web app still shows folders, as roots, until the next part.
   > Spec `2026-10-05-libraries-design.md`, Part F1.

3. **O60 (open)**: in its What column, "A reader restricted to one root can
   still `PUT` … against a work in a root they cannot see" becomes "…
   restricted to some libraries can still `PUT` … against a work in a library
   they cannot see". Add at the end of the What column: "*Since F1 a grant
   names a library and resolves to its folders; the gap is unchanged.*"
4. **O54 (closed)**: its prose stays as written. Append one marked sentence at
   the end of its What column: "*Since migration 15 (F1), a grant names a
   library: `user_library` replaced `user_root`, `grant-library` replaced
   `grant-root`, and `Principal.roots` is every folder of every granted
   library.*"
5. Grep the bare words `grant-root` and `user_root` across `docs/` and
   `web/src/`; read every hit. Closed rows (O64, O86 and others) keep their
   wording; any present-tense statement outside a closed row is corrected.
6. A defect found on the way (any task's report) opens a row by the opening
   ritual (count word, "leaving N", row, "What remains"). None is expected; if
   none was found, say so.
7. The Rust test count at both sites: run the gate once (Step 7);
   `status-test-count` fails naming the real figure. Write that figure at both
   sites, never a sum. The web counts are unchanged, since no web file changed:
   confirm with `git diff main --stat -- web/` printing nothing.

- [ ] **Step 5: README and notes**

`README.md`:

- Under `taiyaku library scan --add /srv/manga`, extend the comment:
  ```
  # --library NAME files the folder under that library, creating it, so two
  # folders (Volumes and Chapters, say) derive as one shelf; without it, a
  # folder is a library of its own. `taiyaku library libraries` lists them
  ```
- The sharing block: `grant-root alice /shelf/manga` and `revoke-root …`
  become `grant-library alice Manga` and `revoke-library alice Manga`; "sees
  every root" becomes "sees every library"; "editions whose files sit under a
  root it was granted" becomes "…under a folder of a library it was granted";
  "`grant-root` says so" becomes "`grant-library` says so".

`docs/notes.md`: a section `## Libraries, F1 (<date>)` covering what changed
(migration 15 and its triggers, scoping, grants, the CLI); why `work` is
filed under its hub's library; why `SET NULL`; adoption; Task 7's figures
exactly as reported; the mutation sweep's result; and what was **not**
measured: the NAS; a library larger than the 1,896-work copy; how many titles
or byte-identical volumes collide across the NAS's libraries (spec §13); and
that a folder moved into a library that keeps another folder changes its
works' slugs (Task 3's fold test mints a new row), which F2's move action
should state.

- [ ] **Step 6: Sweep again**

Re-run Step 2's commands. Expected: `186` rows and `120` closed if no row was
opened; otherwise the counts Step 4's ritual says, every count sentence
agreeing.

- [ ] **Step 7: The gate, and the corpus checks**

```bash
env -u TAIYAKU_CORPUS NEXTEST_TEST_THREADS=8 RUST_TEST_THREADS=8 scripts/check-all.sh >| .superpowers/sdd/libraries-f1-evidence/gate.log 2>&1; echo "exit $?"
tail -5 .superpowers/sdd/libraries-f1-evidence/gate.log
TAIYAKU_CORPUS="$PWD/corpus" cargo test --workspace --all-features corpus:: -- --nocapture --test-threads=1 >| .superpowers/sdd/libraries-f1-evidence/corpus.log 2>&1; echo "exit $?"
```

Expected: `exit 0` for both, and the gate log's `all green` line, quoted in
the report. Do not pipe the gate through `tail` before reading its exit
status.

- [ ] **Step 8: Commit**

```bash
git add docs/status.md docs/notes.md README.md <the comment files>
git commit -m "Status: libraries in the store and the CLI (F1)"
```

- [ ] **Step 9: The claim check, before anyone says "ready"**

Draft the PR body at `.superpowers/sdd/libraries-f1-pr-body.md`, not
hard-wrapped, and include it in the **first** pass. It says, at the top:
**this change migrates the database (schema 14 to 15); take `docker exec
taiyaku taiyaku library backup` before the NAS runs it.** Read every factual
sentence the branch added and check each against its source (the code, a
test's output, Task 7's log, the mutation log):

- the PR body;
- the status paragraph, the O54 and O60 sentences, the synopsis;
- the notes section and the README lines;
- every new or changed code comment and SQL comment:
  `git diff main -- crates | grep '^+.*\(//\|--\|\*\)'`.

**Prefer deleting a figure to correcting it.** A corrected *reason* is a new
claim, so check it again. Fix what is wrong in a new commit, check that
commit the same way, and repeat until a pass finds nothing. Report each
pass's count.

---

## Self-review

- **Spec coverage.** §2.1 → Task 1 (with corrections 1–4, 7). §2.2 points 1–3
  → Task 3; point 4 (fold unchanged) → Task 3's move test; points 5–6 →
  Task 4. §5 → Tasks 1 and 5. §6's CLI bullet → Task 6. §11's F1 line → all.
  §12: derivation (same title, Volumes and Chapters, byte-identical copies,
  proposals, the move with the fold bypassed) → Tasks 3 and 4; migration
  (two roots and a grant, `user_root` → `user_library`, series and works,
  imported NULL, triggers, collision naming) → Task 1; access → Task 5's
  families, without search and home narrowing by library, which is F3's;
  real material → Task 7, without library pages, which are F3's.
- **Executed while planning.** Migration 15's SQL, over `SCHEMA_14_LIBRARY`
  (the same rows, in `sqlite3` 3.53.4) and over a copy of the real library
  brought from 13 to 15; the three triggers' behaviour, including
  `last_insert_rowid()` after a root insert and `recursive_triggers` on; the
  backfill's three rules each against the rule it could be mistaken for. The
  real library's original digest was unchanged afterwards. **Not executed**:
  any Rust in this plan, the CLI tests, and the serve fixtures.
- **Placeholders.** Task 5 Step 5 and Task 6 Step 3 give two fixtures and
  three CLI tests as specifications rather than code, because each must use
  helpers in files this plan did not read whole; each names what it asserts
  and the defect that must turn it red.
- **Type consistency.** `InLibrary`, `add_root_in`, `add_root_checked_in`,
  `scanned_in`, `libraries::{Listed, list, id_of, create, unfiled_works}`,
  `account::{libraries_for, grant_library, revoke_library, granted_libraries}`
  are named the same in every task that uses them.
