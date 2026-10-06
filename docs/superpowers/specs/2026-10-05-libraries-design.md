# Libraries

**Date:** 2026-10-05. **Status:** the approach was approved in conversation on
2026-10-05; the details below are written for review. This is Part F of
`docs/superpowers/specs/2026-10-04-day-to-day-deployment-design.md`. It comes
after Part A (live works) merges and before Parts C, D and E. The first scan of
the real library waits for it.

Komga and Stump each hold about eleven libraries on the user's NAS. Each is one
folder of `/mnt/user/media 1`:

- Manga (Volumes), Manga (Chapters), Ecchi Manga, Extras;
- Comics (Collected), Comics (Issues), Graphic Novels, Webcomics;
- Light Novels, Books, Magazines.

The user wants all of them in taiyaku, managed from the web UI the way Komga
and Stump manage theirs, with one difference: **Manga Chapters belongs to the
same library as Manga Volumes**, so a chapter release folds into its volume as
another edition and can be aligned with it. That is what this project's
alignment exists for.

## Decisions the user made, 2026-10-05

Reading the code turned up six consequences the approved design did not
state. Each carried a recommendation, and the user accepted all six on
2026-10-05; each "Recommendation" below is therefore the decision.

1. **Existing roots become libraries one per folder, as approved, which splits
   any work that today spans two roots.** `derive_with` groups entries from
   every root into one `Group` keyed on series title and volume
   (`derive::GroupKey`). A volume in one root and its chapter release in
   another therefore become one work today. After a one-library-per-root
   migration, the next derive gives each library its own work. Reading
   positions are kept per edition (migration 9), and `fold::absorb` moves a
   whole superseded work, not one edition of it. A position on the split-off
   edition is therefore at risk.
   - **Recommendation: keep one library per root**, as approved. The NAS
     database is new and empty, and the only existing databases are this
     workstation's, which hold test data (spec §1 of the deployment design).
     Migration 15 should say so in its comment rather than handle the split.
2. **Byte-identical volumes in two libraries become two works.** `work.content_key`
   is `UNIQUE` across the whole store (migration 1), and `import::scanned`
   finds an existing work by `content_key` first. Left alone, a copy of a
   volume in Ecchi and in Volumes would land on one work row and leak across
   libraries.
   - **Recommendation: separate works.** `work` gains `library_id`, the unique
     key becomes `(library_id, content_key)`, and the content-key lookup is
     scoped to the library. Positions and notes do not carry between the two
     copies. `content_key` itself is unchanged, so no hand document moves its
     address (O81's mechanism is untouched).
3. **Proposals stay inside a library.** `proposals::raise` pairs every series
   with every other through `all_series` (`raise_edition_of`,
   `raise_same_series`), and `raise_attach_chapters` looks across the whole
   store.
   - **Recommendation: pair only series of the same library.** An Ecchi series
     is never offered as an edition of a Volumes series.
4. **Rules keep their two scopes, a folder or every folder.** `rule.root_id`
   is a folder scope (`rules.rs` skips a rule whose `root_id` names another
   root), and NULL means every root, which is how migration 3's four defaults
   apply. An accepted proposal writes a rule scoped to its subject's one root
   (`proposals::common_directory_pattern`, which refuses a subject spread over two roots).
   - **Recommendation: no library scope for rules in F.** A global rule
     applies to every library, a folder rule to its folder. The rule editor
     names a rule's folder, and with it, now, its library.
5. **Moving a folder from one library to another.** Without a move, the only
   route is "remove folder, add it elsewhere". `scan::remove_root` deletes the
   works that folder alone accounted for, positions and notes with them, so
   that route loses reading history.
   - **Recommendation: a Move action in F's admin UI.** The re-derive after a
     move writes the folder's works under the new library. `derive::superseded`
     names the old works because the new ones hold the same members
     (`(root_id, path, sha256)`), and `fold::absorb` carries positions and
     notes across. One test pins it.
6. **Where Part C's schedule lives.**
   - **Recommendation: at the library**, as Komga and Stump keep it: one scan
     interval and one watch switch per library. "Scan now" on a library walks
     each of its folders and derives once. The watch probe stays per folder,
     because whether events arrive is a fact about a mount, not about a name.
     A library counts as watched only if every folder's probe succeeded or
     was left unverified with the switch on. Otherwise it uses the short
     interval.

**Further decisions, 2026-10-05, made while planning F1** (the F1 plan,
`docs/superpowers/plans/2026-10-05-libraries-f1-model.md`, records them with its
eight spec corrections, which are adopted): a library is deleted when its last
folder goes; `scan --add PATH` without `--library` always makes a new library;
and removing or moving away a library's last folder is refused while some
account's grants consist of that library alone, naming those accounts, so that
no account silently widens to every library.

## 1. What was read

All of it at `main` 953b2486 unless a branch is named.

- **A root is a folder with its settings, and nothing above it.** Its settings
  are `root.path`, `scan_interval_secs`, `last_scan_at`, `last_scan_status`,
  `exclude_globs`, `edition_key`, `default_kind`, `default_language` (migration
  1) and `keeps_documents` (migration 12).
  - Roots may not nest or overlap: `scan::add_root_checked` refuses a path
    inside, or containing, an existing root.
  - A root's display name exists only in `browse::status`. It is `named`: the
    last path component, or the whole path when two components collide.
- **Grants are per root.** `user_root (user_id, root_id)` (migration 4) is
  written by `account::grant_root`, by the CLI's `library user grant-root`,
  and by the Users screen through `Patch::roots` (`users_api.rs`, `Users.tsx`).
  `Principal::from_account` copies a non-admin's granted root ids into
  `Principal.roots` and gives an admin an empty list. **Empty means
  unrestricted** (`browse`'s module doc, spec C1 §5.1). `browse` applies the
  grant in SQL, to editions through `root_clause`/`root_clause_for` and to
  members through `member_clause`, on `member.root_id` (migration 6).
- **Derivation is one pass over every root.**
  - `derive_with` reads every root's `scan_entry` rows and gathers them into
    groups keyed on `GroupKey = (series title, volume bits, rel_path when there
    is no volume)`. The comment on `Group` says so: "every entry across every
    root".
  - Series are reconciled on `sort_title` alone (`reconcile_series`).
  - A volume's existing work is found on `(series_id, volume)` (`held_work`),
    and `import::scanned` looks a work up by `content_key` first.
  - So a series, and a work, can span roots today. A member records its own
    root (`Candidate::root_id`, "a member has one; a work does not").
- **Proposals pair series across the whole store** (`proposals::raise`:
  `all_series`, `raise_edition_of`, `raise_same_series`,
  `raise_attach_chapters`).
- **Browsing.**
  - The grid (`browse::series`, `browse::unfiled`) takes an optional root,
    `only`, from `?root=N` (`library_api::GridQuery`).
  - The series page, search (`browse::search`) and home (`browse::home`,
    including the continue strip) take no narrowing at all.
  - `GET /api/library/status` lists the roots (`browse::status`, `RootCard`).
  - The sidebar draws them as a provisional ROOTS list linking to `/?root=N`
    (`Sidebar.tsx`, whose doc calls it provisional: "no navigation path may
    require it"). It is shown by the `browse` module's `showRoots`, which
    defaults to an account's admin bit (`Shell.tsx`).
  - `Library.tsx` reads `?root=` for the grid only. Its home fetch is never
    narrowed.
- **Configuration → Roots** (`Roots.tsx`, `RootRow.tsx`, `RemoveRoot.tsx`, over
  `config_api.rs`):
  - `GET/POST /api/library/roots`: `create_root` takes a typed path and runs
    the watch probe.
  - `PATCH /api/library/roots/{id}` (`patch_root`) edits the per-folder
    fields.
  - `POST`/`GET /api/library/roots/{id}/scan` (`start_scan`, `scan_status`)
    walk one root and then derive the whole library (`run_scan_inner`).
  - `DELETE /api/library/roots/{id}` with `?dry_run` (`delete_root`,
    `DeleteRootQuery`, `scan::remove_root`, `Removal { works, positions, notes }`)
    reports what a removal would delete before it deletes.
- **OPDS** is flat: `/opds/v1.2/catalog` links to `/opds/v1.2/works`, which
  lists every work the grant leaves visible, and each work has its own feed
  (`opds::catalog`, `opds::works`, `opds::work_feed`).
  - On `live-works` (abdbe06a), `opds::works` iterates
    `state.works.snapshot()`, the Part A registry.
  - There is no series or library level.
- **Formats.** `taiyaku_read::format` sniffs EPUB as `epub-fixed`/`epub-reflow`
  and PDF as `pdf`. Derivation admits only `zip` and `dir`
  (`derive::is_readable`), so a folder of EPUBs or PDFs scans, is counted, and
  derives no work.
- **Part A's registry** (`live-works`, abdbe06a) projects works from six
  tables. Its triggers (migration 14, eighteen `CREATE TRIGGER`s) bump
  `structure_version` on writes to `work`, `edition`, `member`, `page_flag`,
  `run` and `alignment`. A `work.library_id` write moves the counter like any
  other `work` write.

## 2. The model

**A library is a name and one or more folders.** A folder is today's root,
unchanged: its path, its own settings, and its own scan facts. Nothing about a
root moves except that it now belongs to exactly one library.

### 2.1 Migration 15

Part A's migration is 14 and lands first.

```sql
CREATE TABLE library (
  id    INTEGER PRIMARY KEY,
  name  TEXT NOT NULL UNIQUE,
  added TEXT NOT NULL DEFAULT (datetime('now'))
);
ALTER TABLE root   ADD COLUMN library_id INTEGER REFERENCES library(id);
ALTER TABLE series ADD COLUMN library_id INTEGER REFERENCES library(id) ON DELETE CASCADE;
ALTER TABLE work   ADD COLUMN library_id INTEGER REFERENCES library(id) ON DELETE CASCADE;
CREATE TABLE user_library (
  user_id    INTEGER NOT NULL REFERENCES user(id) ON DELETE CASCADE,
  library_id INTEGER NOT NULL REFERENCES library(id) ON DELETE CASCADE,
  PRIMARY KEY (user_id, library_id)
) WITHOUT ROWID;
```

**Backfill, in the migration:**
1. One library per existing root, named by `browse::status`'s rule: the last
   path component, or the whole path when two would collide.
2. `root.library_id` from that.
3. `series.library_id` and `work.library_id` from their members' roots.
4. `user_library` from `user_root`, which is then dropped. A grant on a root
   becomes a grant on that root's library.

**A work or series under no root stays `NULL`.** That covers an imported
`taiyaku.json` work, whose members have `member.root_id` NULL. It belongs to no
library, which is today's behaviour for an imported work: visible only to an
unrestricted account, and listed as unfiled.

**`work.content_key` loses its global `UNIQUE`** for `UNIQUE (library_id,
content_key)` (decision 2). SQLite needs the table rebuilt for that, so the
migration recreates `work` and **must recreate migration 14's three `work`
triggers with it**. Migration 14's own comment warns of exactly this, and
`structure::tests` runs the whole migration list and would catch a missing one.
`series` gains `UNIQUE (library_id, sort_title)` the same way. With NULLs
allowed, two unfiled series still cannot share a title in practice, because
derivation never creates a series without a root.

`root.library_id` is nullable in the column but never NULL in practice: the
backfill fills it, and every writer of `root` sets it from F on. It is left
nullable only because SQLite cannot add a `NOT NULL` column without a default
to a table that has rows.

### 2.2 Derivation, scoped by library

"Derived within a library" changes four things in `derive.rs` and two in
`proposals.rs`. Nothing else in the pass changes.

1. **`GroupKey` gains the library id**, taken from the entry's root, so a group
   never spans libraries. Two folders of one library (Volumes and Chapters)
   still group together exactly as two roots do today.
2. **`reconcile_series` reconciles on `(library_id, sort_title)`.** The same
   title in two libraries is two series.
3. **`held_work` and `import::scanned`'s content-key lookup are scoped to the
   group's library.** `write_work` stamps `work.library_id`.
4. **`superseded` and `fold::absorb` are unchanged.** A fold moves material
   between works. Within a library that is today's case. Across libraries it
   is the folder move (decision 5), and there the fold is wanted.
5. **`proposals::raise` pairs series only within a library**, and
   `raise_attach_chapters` offers only targets in the chapters' own library.
6. **`proposals::common_directory_pattern` is unchanged.** It already scopes to one root,
   and a root is in one library.

The derive pass still runs over **every** library after any scan, not just the
one scanned. At 0.38–0.41 s for 1,896 works (`deployable-evidence/spec-readings/`)
there is nothing to save by narrowing it, and one pass keeps "the store is
consistent after a derive" true without a second code path. If the real
library's size makes that false, narrowing is a later change.

## 3. Browsing

- **`GET /api/library/status` lists libraries, not roots.** Each entry gives the
  library's id, name and folder count. `RootCard` gives way to a `LibraryCard`.
  A restricted account sees only granted libraries.
- **`?library=N` replaces `?root=N`**, and reaches the grid (`browse::series`,
  `browse::unfiled`), search (`browse::search`) and home (`browse::home`).
  Today only the grid narrows. Narrowing by library is narrowing to that
  library's set of roots, so it reuses the existing `root_clause` and
  `member_clause` machinery with the library's root ids. `pair_clause`'s
  "both sides inside" rule holds unchanged. The series page needs no
  parameter, since a series is in exactly one library.
- **The sidebar's provisional ROOTS list becomes LIBRARIES, for everyone.** It
  is no longer an admin-default preference: every account sees the libraries
  it may read, as in Komga. Each links to that library's grid,
  `/?library=N`. The `browse` module's `showRoots` preference goes.
- **Home with no library shows all**: every granted library's continue strip,
  "added" and "read". **A library's page shows its own**, the same three
  lists and the grid narrowed to it.
- **Unfiled (imported) works** appear on Home for an unrestricted account, as
  now, and under no library.

## 4. OPDS

The catalog gains a **Libraries** navigation entry,
`/opds/v1.2/libraries`, listing the libraries the grant allows. Each links to
`/opds/v1.2/libraries/{id}/works`, the same acquisition feed as today's
`/opds/v1.2/works` narrowed to one library. The flat `/opds/v1.2/works` stays,
because existing Panels and Chunky setups point at the catalog and must keep
working.

The narrowing reads `work.library_id` from the store on the blocking pool,
beside `visible_editions_by_work`'s own read. The registry's `Work` does not
carry a library, and does not need to.

## 5. Access

- **An account is granted libraries.** `user_library` replaces `user_root`.
- **`Principal.roots` is still the thing `browse` filters on.** The guard
  resolves it as the union of the granted libraries' roots, so every SQL filter
  in `browse` and every `edition_visible` check is unchanged.
- **An empty grant is still unrestricted, and an admin is still unrestricted.**
- **CLI:** `library user grant-root`/`revoke-root` become
  `grant-library`/`revoke-library`, naming a library by name. The Users
  screen's root checkboxes become library checkboxes (`Users.tsx`,
  `Patch::roots` becomes `Patch::libraries`).
- A library added later is invisible to a restricted account until granted,
  which is what a grant has always meant here.

## 6. The admin UI

**Configuration → Libraries replaces Configuration → Roots.** It is the same
screen grown one level: today's `RootRow` becomes a folder row inside a
library card.

- **The list**: each library's name, folders, last scan per folder, and the
  works it holds. A library whose folders hold only formats taiyaku cannot read
  yet says so (§7).
- **Create**: a name, and one or more folders picked with a **folder browser**,
  below. The typed-path field stays, for a path the browser cannot reach.
- **Edit**:
  - rename;
  - add a folder;
  - each folder's settings, today's `RootRow` fields (edition key, default
    kind, default language, exclude globs, keeps documents);
  - remove a folder, with today's `?dry_run` count first;
  - move a folder to another library (decision 5).
- **Scan now on a library** walks each of its folders in turn and derives once.
  It reuses `run_scan_inner`'s walk per root, holding `library_job` across the
  whole library. Progress is reported per folder, through today's per-root
  `scans` registry. A second Scan now on a library already scanning answers
  409, as a second scan of one root does today.
- **Remove a library**: the sum of its folders' `Removal`s first, then
  `scan::remove_root` for each folder and the library row.
- **The API:**
  - `GET/POST /api/library/libraries`;
  - `PATCH`/`DELETE /api/library/libraries/{id}` (`DELETE` with `?dry_run`);
  - `POST/GET /api/library/libraries/{id}/scan`;
  - `POST /api/library/libraries/{id}/folders` (add), and
    `POST /api/library/roots/{id}/move` (move).

  `PATCH`/`DELETE /api/library/roots/{id}` stay for the folder rows. All of
  these are admin-only, through `administering_or_forbidden`.
- **The CLI keeps parity**:
  - `taiyaku library scan --add PATH [--library NAME]` adds a folder,
    creating the library when the name is new. The name defaults to the
    folder's last component.
  - `taiyaku library libraries` lists them.
  - `scan --library NAME` scans one library.

### 6.1 The folder browser

`GET /api/library/folders?path=…` lists the **subdirectories** of `path`:
names only, sorted, dot-entries skipped, no files, no sizes. It is
admin-only. It answers only beneath configured **mount prefixes**:

- The prefixes come from `TAIYAKU_FOLDERS`, a list like
  `TAIYAKU_TRUSTED_PROXY`, and from a `--folders` flag that replaces it. The
  image sets `TAIYAKU_FOLDERS=/media`.
- With no prefix configured, the browser answers an empty list and the screen
  falls back to the typed path.
- `path` is canonicalised (resolving `..`, encoded forms and symlinks) and must
  then start with a canonicalised prefix, or the answer is 403. A symlink
  inside `/media` that points outside it therefore lists nothing.
- With no `path`, it lists the prefixes themselves.
- Reading a directory runs on the blocking pool. A sleeping disk can take
  seconds to answer, and the request must not hold a runtime worker.

Adding a folder still goes through `add_root_checked`. The browser is a way to
find a path, not a second way to add one, so the nesting refusal and the probe
are unchanged.

## 7. Libraries of formats taiyaku cannot read yet

Books, Light Novels and Magazines hold EPUB and PDF. They can be created and
scanned now:
- the walk records every file;
- derivation admits none (`derive::is_readable`);
- the scan report counts them under the existing unreadable figures.

The library's card says "no readable files yet: EPUB and PDF arrive with the
books work", rather than showing an empty library with no explanation. When
the parked books work (`later-editions`) lands, these libraries need only a
rescan.

## 8. The NAS

The stack mounts the whole media share instead of one folder:
`"/mnt/user/media 1:/media"`. The image sets `TAIYAKU_FOLDERS=/media`. The
README's compose file and its "Running it on a server" section change with F.

The user creates the libraries in the UI before the first scan:
1. **Manga**: `/media/Manga/Volumes` and `/media/Manga/Chapters`.
2. **Ecchi Manga**: `/media/Manga/Ecchi`.
3. **Extras**: `/media/Manga/Extras`.
4. **Comics**, one library or four, as in Komga: Collected, Issues, Graphic
   Novels, Webcomics. The UI makes either possible; it is the user's call
   when creating them.
5. **Light Novels**, **Books** and **Magazines** (§7).

## 9. Interaction with the other parts

- **Part A (live works)** needs nothing new. Library writes are `work` writes,
  which move the counter, and the registry re-reads. A folder move re-derives,
  and the moved works are new rows that the registry picks up within one
  refresh. O184 (a write within one refresh of a deletion resurrects a work)
  is reachable through a move exactly as through any deletion; Part D fixes it.
- **Part C (scheduled scans)**: the schedule is per library (decision 6). C's
  roots-screen settings (§4.3 of the deployment spec) become library settings.
  Its scheduler walks a library's folders in turn, as Scan now does.
- **Part D (missing works)**: the missing list groups by library, and a
  library's removal counts include its missing works. A partial mount is per
  folder, and that is unchanged.
- **Part E (pending writes)**: the pending list groups by library. Nothing
  else changes, since writes are per folder.

## 10. What this does not do

- **No library-scoped rules** (decision 4).
- **No per-library reading settings** (direction, layout). The reader's
  settings stay per account and per work.
- **No library ordering or icons.** Libraries list by name.
- **No change to how a folder is walked**, or to the nesting rule. Two
  libraries cannot share a folder, and one library's folder cannot sit inside
  another's.
- **No EPUB or PDF reading** (§7).
- **No narrowing of the derive pass to one library** (§2.2).

## 11. PRs

F is too large for one PR. It splits where each piece is usable on its own:

1. **F1, the model**: migration 15, library-scoped derivation and proposals,
   grants through libraries (`Principal.roots` from `user_library`), and the
   CLI (`--library`, `libraries`, `grant-library`). Afterwards the store and
   the CLI know libraries, while the web UI still shows roots, which still
   work.
2. **F2, the admin UI**: the Libraries screen, the folder browser and its API,
   Scan now per library, remove with counts, move a folder, and the Users
   screen's library grants. After F2 the user can create the libraries and
   run the first scan.
3. **F3, browsing**: `?library=` through grid, search and home; the sidebar's
   LIBRARIES list; OPDS's Libraries navigation; and the README and NAS mount
   change.

F2 is what the first scan waits on. F3 can follow it without any rescan.

## 12. Testing

- **Derivation:**
  - the same title in two libraries makes two series and two works;
  - Volumes and Chapters as two folders of one library still fold a chapter
    release into its volume's work;
  - byte-identical copies in two libraries make two works (decision 2);
  - proposals never pair across libraries;
  - a folder moved to another library keeps its works' positions and notes
    through the fold, held by one test, watched failing with the fold
    bypassed.
- **Migration 15:**
  - on a store with two roots and a grant, each root becomes a library named
    by `named`'s rule;
  - `user_root` rows become `user_library` rows;
  - series and works carry the right `library_id`;
  - an imported work stays NULL;
  - Part A's triggers survive the `work` rebuild, which
    `structure::tests`'s every-table case already checks;
  - collision naming (two folders called `Volumes`).
- **Access:** an account granted one library sees only that library's works on
  the grid, in search, on home, in OPDS, and on page and download routes. An
  empty grant sees all.
- **Browsing:** `?library=` narrows grid, search and home, and keeps
  `pair_clause`'s both-sides rule.
- **The folder browser:**
  - non-admin 403;
  - `..`, an encoded `%2e%2e`, and a symlink pointing outside the prefix are
    each refused or list nothing outside;
  - files are never listed;
  - no prefix configured gives an empty answer;
  - the listing is sorted and skips dot-entries.
- **The admin UI:**
  - Scan now on a two-folder library walks both folders and derives once;
  - 409 on a second start;
  - remove's dry run sums both folders;
  - the web tests for the Libraries screen and the Users grants.
- **Real material:** a scratch copy of the real-library database with two
  folders made into one library, and a third into another. The library pages
  and OPDS are checked to show each library's works only.

## 13. What could not be measured from here

- **The size of a first scan of the whole media share** on the NAS: every
  library's archives are fingerprinted once, and nothing has walked
  `/mnt/user/media 1` beyond counting the Manga share's 67,925 files.
- **How many titles collide across libraries, and how many byte-identical
  copies sit in two libraries.** Decisions 2 and 3 matter in proportion to
  these, and nothing has counted them.
- **How long a directory listing takes on a sleeping disk**, which the folder
  browser will meet.
- **Whether OPDS clients (Panels, Chunky) show a navigation level above the
  works feed well.** Komga's feeds have one, which is evidence but not a
  measurement.
