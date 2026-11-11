# Before the NAS deploy: five lanes

**Date:** 2026-11-10. **Status:** the decisions are the owner's rulings of
2026-11-10 and the calls told to the owner that day without objection. Each
decision says which it is: "ruled", "told to the owner", or "the spec's own
call" where the rulings are silent, with the pre-flight it follows when one
recommended it. Each lane's plan is written from this document. Sites are
named by symbol.

The rulings and the pre-flights that carry line numbers at `main` 6ecd66e3
(`formats-preflight.md`, `o244-survey.md`, `daily-preflight.md`) are in
`.superpowers/sdd/pre-deploy-evidence/`, which is local and not committed.
Everything a plan needs from them is restated here.

## 1. Why, the lanes, and how they land

taiyaku will run on the owner's NAS for daily reading, and nothing is deployed
yet, so a breaking change costs nothing now. Two kinds of work are cheaper
before the first real data than after it. A format a deployed store keeps (a
slug, a stored reading position, a document's version) cannot be changed later
without breaking what was written under it. A gap in daily use (no scheduled
scan, works whose files are gone, a cache nothing bounds) is cheaper to close
before the first scan of the real library than after.

Five lanes do this work in parallel, each in its own worktree and PR:

| Lane | What | Rows |
| --- | --- | --- |
| 1. Formats | the slug rule, a backup before every migration, document version guards, the stamp's staging name | closes O262, O263, O264, O185 |
| 2. Anchors | positions and notes anchored to a member's page (approach A), and a version on the notes document | narrows O244 (§3, decision 8) |
| 3. Scheduled scans | Part C's scheduler, per directory, without the watcher; the status of a cut derive | closes O186; narrows O51 |
| 4. Missing works | Part D, as two PRs; O184 | closes O91, O48, O184 |
| 5. Cache cleanup | an orphan sweep and a size cap | narrows O11 to its Stump half (§6) |

**Merge order.** Lane 1 merges first. Its backup before a migration has to be
in the binary that is running before any later migration meets a deployed
database, and lanes 2 and 4 each add one. Lanes 2 to 5 merge in any order
after it. Deploy day opens a fresh database at version 0, which needs no
backup.

**Lane 2's notes-version task waits for lane 1 on `main`.** It extends lane 1's
version helper and in-place export guard (§2, §3). The rest of lane 2 runs in
parallel with lane 1.

**Coordination.**

- **A migration takes the next free number when its lane rebases.**
  `schema.rs`'s `MIGRATIONS` list and `SCHEMA_VERSION` are shared by lanes 2
  and 4. The second to merge renumbers its file, its `include_str!` and the
  version, and runs the schema tests again. Lane 1 edits `schema.rs` too, in
  another place (§2).
- **An O-number a lane opens takes the next free number at rebase**, as a
  migration does.
- **Each lane edits `docs/status.md` only in its last task**, and redoes the
  count sentences (the total at the head of Open items, "leaving N", "What
  remains") whenever it rebases onto a lane that moved them.
- **Lanes 3 and 5 both add startup work to `taiyaku_serve::serve`**
  (`crates/taiyaku-serve/src/lib.rs`). Each spawns a task beside
  `registry::spawn_refresher`, for a live registry only. Lane 3 also rewrites a
  status before the bind, beside the session sweeps. The second to merge
  rebases over the first's lines.
- **No lane adds a field to `ServeState`.** It is built by a struct literal at
  many sites, and two lanes adding a field would conflict at all of them.
  A task is handed what it needs. Lane 5 adds one field to `ServeOptions`.
- **Other files two lanes share**, mostly in different functions, so their
  conflicts are text only:
  - lanes 1 and 2: `crates/taiyaku-library/src/lib.rs` (lane 1's
    `Library::from_connection`; lane 2's `Note` and `Notes`),
    `schema/tests.rs` (lane 1's backup tests, which need the private
    `migrate_with` and `MIGRATIONS`; lane 2's migration test), `state.rs` (a
    doc comment; the progress and notes functions) and `export.rs` (lane 1's
    guard; lane 2 extends it);
  - lanes 3 and 4: library `scan.rs` (the status writers and `probed`; the
    missing-works list and removal), `config_api.rs` (`root_json` and the
    patch check; the missing routes) and web `config.ts` (`Root` and
    `regimeFacts`; `TABS`);
  - lanes 1 and 4: serve `opds.rs`, where they meet inside
    `opds::works_feed`'s loop a few lines apart (lane 1's comment on the
    href; lane 4's skip of a missing work).
- Lane 4 adds a taker to `LibraryJob`'s doc list in `library_job.rs`.
- `README.md` is touched by lanes 1, 3, 4 and 5. Its conflicts are text only.

## 2. Lane 1: formats

### Decisions

1. **The slug rule** (ruled 2026-11-10). Keep Unicode letters and digits,
   lowercased. Collapse each run of other characters to one `-`, trimmed at
   both ends. Drop a trailing archive extension first. Keep the `-2`, `-3`
   suffixes. One rule makes work slugs and the edition key an edition-of
   proposal's accept writes: `proposals::slugify`'s, moved where both callers
   reach it.
2. **The series-cover redirect encodes its `Location`** (ruled with 1).
3. **A backup before every migration** (told to the owner). It is
   `library-<stamp>-before-v<N>.db` in the existing backups directory. It is
   listed in the Backup tab and never deleted automatically. The migration is
   refused if the backup fails. It is skipped for an in-memory database, a
   fresh one (version 0) and one already up to date.
4. **Readers refuse a newer document** (told to the owner). `LibraryDoc`,
   `TruthDoc` and `RegionDoc` refuse a version above their own, as
   `Sidecar::readable_version` does.
5. **An older binary leaves a newer hand document alone** (told to the owner).
   `hand::save` neither writes over it nor removes it, and answers
   `persisted: false` with the reason.
6. **O185: the stamp stages under a dot name**, so the walk skips a leftover,
   with no cleanup (the spec's own call, from the formats pre-flight §4; the
   rulings put O185 in this lane). The residual is stated in O185's close.
7. **A hand document's version is bumped for any new field** (the spec's own
   call, from the formats pre-flight). It follows from decision 5: `save`
   rewrites the file from rows, so an older binary drops any field it has no
   column for.
8. **A tagged backup is exempt from `--keep`** (the spec's own call, reading
   decision 3's "never deleted automatically"). `backup::prune` runs only when
   `library backup --keep N` asks, but a cron entry runs it unattended, so it
   neither counts nor deletes a tagged backup. The owner removes one by hand.
9. **The Backup tab's text, its doc comment and the README change with 3 and
   8** (the spec's own call). The tab says "The server backs up only when
   asked", its doc comment that "`--keep` is where retention lives", and the
   README that `--keep 7` deletes "all but the seven newest"; none holds once
   3 and 8 ship.
10. **The in-place export guard covers every versioned sibling** (the spec's
    own call, widening the formats pre-flight's guard for `library.json`):
    `taiyaku.json`, `library.json`,
    `truth.json` and `progress.json`, and `notes.json` once lane 2 versions it.
11. **A guarded `save` leaves the work's document marker as it was** (the
    spec's own call). See "Document version guards".

### The slug rule

- `import::slugify` drops a trailing archive extension, case-insensitively
  (`.cbz`, `.cbr`, `.cb7`, `.cbt`, `.zip`, `.rar`, `.7z`, `.tar`, `.pdf`,
  `.epub`; the list is the spec's own call), then applies the shared rule,
  then falls back to `work` when nothing is left. `import::unique_slug` keeps
  its suffixes.
- The shared rule is `proposals::slugify`'s body, moved to one place both
  reach; the plan names it. Its edition-key caller keeps refusing an empty
  token. Only the work-slug path falls back to `work`.
- What the rule gives, from the formats pre-flight's table:

  | File | Today | After |
  | --- | --- | --- |
  | `Spy × Family - v01 (2020).cbz` | `spy---family---v01--2020--cbz` | `spy-family-v01-2020` |
  | `Kabushikigaisha Magi-Lumière - v02 (2024).cbz` | `kabushikigaisha-magi-lumi-re---v02--2024--cbz` | `kabushikigaisha-magi-lumière-v02-2024` |

- A store's slug is minted by `import::insert_work` and never made again.
  `works::row_id_for_document` also inserts a work row, under a slug the
  registry already holds: that of a work with no row, as
  `test_support::state`'s fixtures and `corpus.rs` serve, or a deleted work's
  own (O184, lane 4). The CLI's argv mode imports every sidecar first, so its
  slugs come from `insert_work` too. A dev database keeps its old slugs until
  it is rebuilt.
- `derive::title_of` keeps the extension in the title. Whether a title loses
  it is a title decision, outside this lane.
- `works::work_id`, which only `load_works` calls, keeps its own rule. Five
  comments that say the two rules agree, or that a slug is ASCII, are
  reworded: `unique_slug`'s doc
  ("the same shape"), `state::notes`' doc ("the same shape `taiyaku-serve`'s
  own work id is slugified to"), the two in `opds.rs` (`works_feed`: "slugified
  by construction … needs no encoding"; `work_feed`: "already URL-safe by
  construction"), and the comment on the `★★★` test in `import/tests.rs`
  ("nothing ASCII-alphanumeric").
- **Where a non-ASCII slug travels.** The web encodes every API segment
  (`api.ts`'s `seg`). OPDS hrefs go through `opds::percent_encode`. The tile
  address, `library_api::cover_address`, encodes with `uri_component`.
  `library_api::cover` builds its 307 `Location` from the slug and the edition
  key unencoded; it encodes both with `uri_component`, as `cover_address` does.
  An edition key can already hold a non-ASCII letter under today's rule. The
  web builds reader links unencoded (`/read/${w.slug}`) and the router decodes
  the parameter (unverified; a test below settles it).
- **Residuals.** No Unicode normalisation crate is in the workspace, so a
  filename in NFD and the same name in NFC slug differently; a slug is minted
  once, so this is harmless. A slug freed by a deleted work can be minted
  again for a new one; lane 4's removal is a new way to free one. Suffixes
  follow the order works arrive in.

### The backup before a migration

- A new crate-private helper in `schema.rs` answers `(current, target)` when a
  migration is pending. It reads the version as `migrate_with` does, after
  `ensure_meta`. It answers nothing for version 0, for a database at
  `SCHEMA_VERSION`, and for a newer database, which `migrate` goes on refusing
  before any backup is taken.
- `Library::from_connection` calls it after `configure`. When it answers and
  `path` is `Some`, it takes the backup into `backup::directory_for(path)`,
  then calls `schema::migrate`. An in-memory database has no path.
- **The name** is `library-<stamp>-before-v<N>.db`. The stamp comes from
  SQLite's clock, as `backup::take` mints it. `N` is the version the open
  migrates to, `SCHEMA_VERSION`. The file holds the version the database was
  at, so an image whose schema version is at least that and below `N` can
  open it. The README's restore step says so.
- The step is a crate-private function handed the name, as `backup::take` has
  `take_named`. `from_connection` calls it with the name minted from SQLite's
  clock; a test calls it with a name already taken.
- `backup::stamp_of` accepts that tail and nothing near it. `Backup` carries
  the `N` its name holds. `backup::list` lists it, so the Backup tab draws it
  and its download works with no change to the list or the route.
  `backup::prune` neither counts it nor deletes it (decision 8).
- In `Backup.tsx`, the "How backups work" text stops saying the server backs
  up only when asked, and its doc comment's "There is no schedule here … and
  `--keep` is where retention lives" changes with it: both say a backup is
  taken before each migration and that `--keep` leaves those alone. The
  README's `--keep 7` line says the same (decision 9).
- **`TakeError::Exists` is success.** A second process opening the same
  database in the same second has taken the same backup. It could snapshot
  the database after the first process migrated it (inferred); in the
  container the server opens first and alone.
- **Any other failure refuses the open.** `from_connection` returns the error,
  naming the backup, and the database is not migrated. Under Docker that is a
  restart loop with the message, as for a newer database today.
- `backup::take`'s rule against the server's own connection is about stalling
  requests. At open time nothing is served yet, so the backup runs on the one
  connection there is.
- Existing tests that open an older file database through `Library::open` now
  leave a backup beside it. The plan lists them.
- **Docs.** README's manual "run `library backup` before merging a change that
  migrates" becomes a note that the backup is taken for you, and the restore
  steps name the tagged file. `backup.rs`'s module doc says so too.

### Document version guards

- `LibraryDoc::from_json`, `TruthDoc::from_json` and `RegionDoc::from_json`
  refuse a version above their `VERSION`, in `readable_version`'s words, and
  accept every version up to it.
- An older binary then reports a newer hand document through derive's
  read-back as it reports any unreadable one: in
  `DeriveReport::unreadable_hand`, unabsorbed, with a fold onto that work
  refused. `hand::title_in` and `hand::reconcile` already pass over a document
  they cannot read.
- **A residual, stated rather than fixed** (the spec's own call). The
  read-back runs only while the work's document marker is unset. A newer
  document beside a work already absorbed (a backup restored into an older
  image, say) is read by nothing that reports it: `reconcile` passes over it,
  and the fold's carry does not read `Saved`.
- **One helper reads a document's version.** It is handed the file and the
  key (`taiyaku`, `taiyaku_library`, `taiyaku_truth`, `taiyaku_progress`, and
  `taiyaku_notes` from lane 2), reads the file as a JSON object, and answers
  that key's number, or nothing.
- `hand::save` asks it before it writes or removes. A file declaring a newer
  `taiyaku_library` is left as it is, and `Saved` answers `persisted: false`
  with a `why` saying a newer taiyaku wrote it. The rows are written
  regardless, as they are for a directory that cannot be written. The metadata
  routes' existing `why` channel (`edit_work_raw_applying`) carries the reason
  to the caller. Every caller of `save` gets the guard, the fold's carry in
  `derive` included.
- **A guarded `save` returns before `migrate::record_absorbed`** (decision 11),
  so the marker stays as the read-back left it: an older binary goes on
  reporting the document in `unreadable_hand` at every derive, and a binary
  that can read it reads it back.
- **An in-place `library export`** keeps a versioned sibling that declares a
  newer version than this build writes, and lists it among the files it kept,
  as it keeps one it never absorbed (`migrate::absorbed_for`). Both of its
  loops ask: the one that writes (`taiyaku.json` and the documents with
  content) and the one that removes a document the store holds nothing for
  (`truth.json`, `progress.json`, `library.json`). Lane 2 adds `notes.json`
  when it versions that document.
- `librarydoc::VERSION`'s doc comment changes from "bumped when an existing
  field would mean something else" to "bumped for any new field" (decision 7).

### O185: the staging name

- `stamp::staging_path` stages `v01.cbz` as `.v01.cbz.taiyaku-stamp`, in the
  same directory, so the rename into place stays a rename and O104's syncs
  stand.
- Both places the walk asks skip a dot name: `scan::measure_dir` and
  `Excluder::keeps_out` (`KeptOut::DotName`). Part C's watcher is designed to
  ignore dot paths too.
- **The residual.** A stamp cut while staging leaves a hidden, full-size copy
  beside the archive until that archive is stamped again, when `File::create`
  truncates the same name. It costs disk only. A scan that deleted it would be
  a new writer into the library, which the README rules out. A leftover under
  the old name, written before this change, is still scanned.

### Boundaries

- No existing work is given a new slug, and no title changes.
- No retention rule for a backup taken before a migration, and no restore route.
- `save` does not carry unknown keys across a rewrite; decision 7 covers it.
- Part E's flush will need the newer-file guard too. It is not built here.

### Testing

Each check is watched failing against the defect named after it, applied by
hand, before it is trusted.

- **Slugs.** Unit tests on `import::slugify` and `unique_slug`: the two files
  in the table; 「五等分の花嫁 第01巻」 gives `五等分の花嫁-第01巻`; `V01.CBZ`
  loses its extension; `★★★` gives `work`; a second work of one title gets
  `-2`. *Fails on:* the ASCII test restored (Lumière, the CJK title); runs
  left uncollapsed (Spy × Family); the extension kept, or kept only in upper
  case; the `work` fallback dropped.
- **Edition keys.** The test pinning the refusal of an empty edition key stays
  green; the plan finds it or writes it. *Fails on:* the `work` fallback moved
  into the shared function.
- **Pinned literals.** `alone---v01` in the derive tests and `series---v01` in
  the browse tests are the old rule's pins and change with it.
- **The cover redirect.** A serve test over a work with a non-ASCII slug and
  edition key: `GET /api/library/series/{id}/cover` answers 307 with a
  percent-encoded `Location`, and following it answers the tile. *Fails on:*
  `cover` formatting the raw slug again.
- **OPDS.** The same work's href in a feed is percent-encoded and resolves.
  *Fails on:* that href skipping `percent_encode`.
- **The reader route.** A web test opens the percent-encoded path a browser
  delivers (`/read/lumi%C3%A8re-v02`), and the reader asks the API for that
  work encoded exactly once. A raw `/read/lumière-v02` entry never exercises
  the decoding. *Fails on:* `seg` no longer encoding (the raw path is asked
  for); the parameter encoded again (`%25C3` is asked for).
- **Backup taken.** A file database built one version short (`migrate_with`
  over a prefix of `MIGRATIONS`, as `schema/tests.rs` builds older databases),
  then `Library::open`: a `-before-v<N>` file exists, opens, and reads the
  older version, and the database reads `N`. *Fails on:* the backup taken
  after `migrate`, or not at all.
- **Skips.** An up-to-date file database opened again, and a fresh one, leave
  no backup; an in-memory open writes nothing. *Fails on:* the up-to-date skip
  or the version-0 skip removed.
- **A newer database** is refused with no backup taken. *Fails on:* the backup
  taken before the newer-version check.
- **Failure.** With `backups` made a plain file, the open is refused naming the
  backup, and the database still reads the older version. *Fails on:* a failed
  backup warned about and the migration run anyway.
- **Exists.** The backup step handed a name already taken answers success,
  and the migration that follows runs. *Fails on:* `Exists` treated as a
  failure.
- **Names and pruning.** `stamp_of` accepts the tagged name and refuses near
  misses: an empty or non-numeric `N`, a doubled suffix, a path separator.
  *Fails on:* the tail accepted loosely. `prune` over two plain backups and
  one tagged with the newest stamp, keeping one, deletes the older plain
  backup, keeps the newer, and keeps the tagged one. *Fails on:* `prune`
  counting tagged backups (it keeps the tagged one as the newest and deletes
  both plain ones); `prune` deleting tagged backups (run with the tagged one
  oldest).
- **The Backup tab**, jsdom: "How backups work" says a backup is taken before
  each migration and that `--keep` leaves those alone. *Fails on:* the old
  "only when asked" text.
- **Readers.** Each of the three `from_json` accepts `VERSION` and refuses
  `VERSION + 1` in `readable_version`'s words. *Fails on:* the comparison made
  `>=` (the accept case), or removed (the refuse case).
- **Read-back.** Derive over a newer hand document lists it in
  `unreadable_hand` and leaves the rows as they were. *Fails on:* the version
  check removed from `LibraryDoc::from_json`.
- **`hand::save`**, over a newer file, for a projection with content and for an
  empty one: the file's bytes are unchanged, `persisted` is false, `why` says a
  newer taiyaku wrote it, and the rows changed. *Fails on:* the guard covering
  only the write branch (the empty case), or removed (both).
- **The marker.** A work whose newer document the read-back refused is given
  a hand value (a metadata edit, as below), so `save` projects content and is
  guarded; the next derive still lists the document in `unreadable_hand`.
  With an empty projection the mutant would record `false` and pass. *Fails
  on:* the guard running `record_absorbed`.
- **Through the API.** A metadata edit of a work whose document is newer
  answers `persisted: false` with that reason. *Fails on:* as above.
- **Export.** Both loops already keep a file whose document is not absorbed
  (`migrate::absorbed_for`), and an import with no `library.json` or
  `truth.json` beside the sidecar records each as settled and not absorbed.
  So the test arms the markers first: an in-place export of a work imported
  from a `taiyaku.json`, given a hand value and truth rows, writes `library.json` and `truth.json`, recording
  both absorbed; the truth rows are then removed, and newer `taiyaku.json`,
  `library.json` and `truth.json` put in place. The in-place export under
  test keeps all three and lists them as kept. *Fails on:* the write loop not
  asking the helper (`taiyaku.json`, `library.json`); the removal loop not
  asking it (`truth.json`).
- **O185.** In `stamp/tests.rs`, a valid archive written at `staging_path`'s
  name for `v01.cbz`, beside it, and the directory scanned: one entry. *Fails
  on:* `staging_path` back to the undotted name.
  `a_rewrite_returns_the_new_digest_and_leaves_no_temp_behind` also asserts
  the staging name starts with `.`; its `contains("taiyaku-stamp")` filter
  passes with either name.

## 3. Lane 2: anchors (O244)

### Decisions

1. **Approach A** (ruled 2026-11-10). `progress` and `note` keep `slot` and
   gain an anchor beside it: the member's `sha256`, its occurrence among the
   edition's members with that digest, the page within the member, and the
   half. Every write fills it. Every read resolves the anchor to a slot, and
   falls back to the stored slot.
2. **A lost file** (ruled 2026-11-10). When the anchor no longer resolves, a
   position falls back to its stored slot, clamped into the edition's range. A
   note is kept at its stored slot and the API marks it `stale`, with no new UI
   now. The row is never rewritten, so the anchor resolves again if the file
   returns.
3. **A moved file** (ruled 2026-11-10). When a rule edit moves a member to
   another edition or work, the row stays with its own edition and resolves as
   a lost file. The fold keeps its explicit carry.
   - A rule edit that moves every member of an edition, on a work that keeps
     other members, leaves that edition standing, members and all: under
     `Scope::Scan`, `import::scanned` keeps an edition the derived document no
     longer names, and counts it in `DeriveReport::kept_editions`. Rows on it
     resolve as before. Where the edit empties the work, the fold carries the
     rows instead.
4. **The web sends anchors in a follow-up PR** (ruled 2026-11-10), which closes
   O240. This lane changes no client behaviour.
5. **The notes document gets a version**, `taiyaku_notes: 1` (ruled
   2026-11-10, moved here from lane 1: it changes the same files).
6. **A half the layout no longer holds** (the spec's own call). A position
   goes to the first slot of its page in reading order; a note resolves as a
   lost file. A half anchor on a page that is no longer a spread, or a
   whole-page anchor on a page that now is one, is the case.
7. **Anchors stay off the wire and out of the exported documents** (the
   spec's own call). The follow-up puts them on the wire and settles the
   documents' shape with it. An import computes each row's anchor from its
   slot, through the store's own write path. `stale` stays out of the exported
   `notes.json` too.
8. **The removal rehearsal's counts are unchanged** (the spec's own call).
   Counting the rows whose anchor would stop resolving goes with the
   follow-up, which has a screen to show it. O244 is narrowed to that count.
9. **The fold plans from resolved slots** (the spec's own call). It reads
   each row's slot as `state::` answers it, not the stored number, so a stored
   slot gone stale neither refuses a fold nor is re-placed as the wrong page.
   The anchor columns travel unchanged.
10. **A row on an edition key the work no longer has** (the spec's own call)
    has no layout to resolve against. It answers its stored slot, unclamped,
    as today, and a note there is `stale`. `import::sidecar` of a document
    that no longer lists the edition reaches this case (`Scope::Document`
    deletes it). `library import` goes through `import::reimport`, which
    keeps every edition the store holds, and reaches it only for a document
    whose hub is a key the store does not hold.
11. **An occurrence that renumbers is accepted** (the spec's own call). Remove
    the first of two copies of one archive in an edition, and a row on the
    second copy (occurrence 1) no longer resolves and falls back, though its
    page is still there. An archive listed twice in one edition is rare.

### The schema

- One migration (020, or the next free number at rebase) adds four nullable
  columns to `progress` and to `note`: `at_sha256 TEXT`, `at_nth INTEGER`,
  `at_page INTEGER` and `at_half TEXT CHECK (at_half IN ('left','right'))`.
- `ALTER TABLE … ADD COLUMN`, so neither table is rebuilt and `note_by_work`
  stands. That SQLite accepts it on `progress`, a `WITHOUT ROWID` table, is
  unverified; the schema test settles it.
- The first three are all set or all NULL. NULL means the row named no page
  when it was written (past the end) or predates the migration. `at_half` is
  NULL for a whole page. One store function writes all four from one value,
  and a test pins the rule.
- The `at_` names keep clear of `notes::same_anchor`, which already says
  "anchor" for an edition, slot and region, and of `taiyaku-core`'s exported
  `Anchor` (where an alignment may start). The core type for a page anchor
  takes another name too; the plan picks it.
- One `schema/tests.rs` case pins the migration over rows written at the
  version before it.

### The core

One function pair in `taiyaku-core`, built on `Edition::slots`,
`Edition::locate` and `Half`:

- **Slot to anchor.** The slot's page and half from `slots(reading)`; the
  member and the page within it from `locate`; the member's digest and its
  occurrence among the edition's members with that digest. `None` past the
  end.
- **Anchor to slot.** The member at that occurrence of that digest; its first
  page's index plus the page within it; the slot naming that page and half.
  `None` where that occurrence is gone or the page is past the member's end.
  Decision 6 where the half is not held.

Resolution lives in the core alone. Serve's `View` does not resolve.

### Writes

- `state::put_progress_at` computes the anchor from the edition's current
  layout and writes it with the slot, on insert and on conflict both.
- The layout comes from one lean store query for a work's edition: its
  members' `sha256` and page counts in ordinal order, its spread pages, and the
  work's reading direction. Not `project::sidecar`, which also reads
  alignments.
- **`state::put_notes` keeps the stored anchor of a note that has not moved.**
  It is delete-then-reinsert. Before the delete it reads the stored rows,
  resolved. An incoming note whose id, edition, slot and region equal its
  stored note's resolved ones is written back with the stored anchor columns
  unchanged. Any other note gets an anchor computed from its slot. Without
  this, saving any note re-anchors a lost file's note at its fallback slot,
  and it can never resolve back.
- **That comparison moves into `taiyaku-library`.** `notes::same_anchor` is a
  private function of `taiyaku-serve`, which the library cannot call. It
  becomes a public method on the library's `Note` (edition, slot and region
  equal), and serve's validation calls it in its place.
- **`fold::move_positions` names the four columns** in its `INSERT … SELECT`
  and in its `DO UPDATE SET`. Its column list would drop them silently.
  `fold::move_notes` is an `UPDATE` and keeps them.
- **The fold plans from resolved slots** (decision 9). `fold::unplaceable`,
  `fold::resequence_rows` and `fold::shared_note_ids` each read `slot` by SQL
  today. They read each row's resolved slot instead, through the resolution
  `state::` uses, so `plan_slots`' re-placement starts from the page the row
  names in the source's current layout. `resequence_rows` then writes the
  re-placed number into `slot` and leaves the anchor columns alone.
- `migrate::apply_progress` and `migrate::apply_notes` write through
  `state::put_progress_at` and `state::put_notes`, so an import fills anchors
  with no change of its own.

### Reads

- `state::progress` and `state::notes` resolve each row's anchor against its
  own edition's current layout and answer the resolved slot.
- Every caller of those two then sees slots in the current layout: the
  routes, narrowing in `view.rs` (stored positions stay in the full numbering
  and are translated at the server's edge, overlapping-libraries decision 7),
  export and import. The fold reads rows by SQL and resolves them itself
  (decision 9).
- A NULL anchor answers the stored slot, as today.
- An anchor that does not resolve takes decision 2. A position's stored slot
  past the end is clamped to the edition's last slot; a note keeps its stored
  slot. A note's `stale` is set on the answer only and is never stored.
- Resolution searches the row's own edition only (decision 3). A digest held
  by another edition of the work does not count. A row on an edition key the
  work no longer has takes decision 10.

### The notes document

- `Notes` gains `taiyaku_notes`, read as 1 when absent, so every existing file
  and every body the web sends (it builds `{work, notes}`) still reads. A
  version above 1 is refused in `readable_version`'s words. The notes `GET` and
  `library export` write it.
- `Note` gains `stale`, written only when true. `Note` is
  `deny_unknown_fields`, so the field has to exist for the web's echo of a
  `GET` to be accepted.
- **`stale` is cleared on the way in.** `Note` derives `PartialEq`, and
  `notes::as_stored` refuses a change to a note the account is not shown by
  comparing whole notes. So the notes `PUT` handler clears `stale` on every
  incoming note, and on the stored document it compares against, before
  `notes::as_stored` and `notes::validate_against` run. `library import` clears it
  too. `library export` writes `notes.json` with `stale` cleared (decision 7).
- Lane 1's export guard gains the notes key.
- The bump rule is the sidecar's (`docs/sidecar.md`): only a newer version is
  refused, and whoever bumps it owns what reading a 1 means.

### Edges the plan settles

- `fold::shared_note_ids` decides whether two copies of a note are one (O78)
  from what `note_content` reads, after re-placing the slot. The plan states,
  with a test, whether the anchor columns join that comparison. The survey
  reads them as equal by construction on editions of identical digests
  (inferred).
- An image-folder member keeps its digest when its contents change in place
  (O56), so a page-within-member anchor can drift there. Left open with O56.

### Boundaries

- The web half is a follow-up PR: anchors on the wire, the server preferring a
  valid anchor and refusing one that names a member the account is not sent,
  and `stale` drawn. It closes O240.
- `page_flag`, `run` and `truth_page` are keyed by page and drift the same
  way. A scan-only library holds no flags or alignments, and holds
  `truth_page` rows only where someone labels pages (`PUT
  /api/works/{work}/truth`). None is anchored here.
- No count changes (decision 8).

### Testing

Tests read a position's or a note's slot through `state::progress`,
`state::notes` and the routes, never by raw SQL: a raw read of `slot` passes
whether resolution works or not, as
`taking_over_a_nested_directory_leaves_the_rows_an_in_root_rule_edit_leaves`
does today. Only the checks of the anchor columns themselves ("Writes") read
the columns.

- **Round trip.** Every slot of an edition of three members goes to an anchor
  and back. *Fails on:* the page within the member off by one.
- **Occurrence.** One archive listed twice in an edition; a position on the
  second copy's page resolves there. *Fails on:* the occurrence ignored.
- **O244's own case.** c001 (3 pages) and c002 (4 pages) in one edition, a
  position on c002's second page; remove c001's directory: the position
  answers c002's second page, through `state::progress` and
  `GET /api/works/{work}/progress`. *Fails on:* resolution disabled, so the
  stored slot is answered.
- **A lost file.** The same edition, a position on c002's last page (stored
  slot 6) and a note there; remove c002's directory, leaving c001's three
  slots. The position answers slot 2, the clamp, and the note answers slot 6
  with `stale: true`. Add the directory back and scan: both answer c002's last
  page again, without `stale`. A position on c001 cannot test the clamp: its
  stored slot stays in range of c002's four. *Fails on:* the clamp dropped
  (6 is answered); `stale` not set; the clamp applied to the note (2 is
  answered); a read writing the fallback back to the row (the return case).
- **A moved file.** A rule edit moves c001 to another edition: the note stays
  on its own edition, `stale`, and nothing appears on the other edition.
  *Fails on:* resolution searching the work's other editions.
- **A held anchor.** With a lost file's note stored, save the document with
  another note edited, add the directory back and scan: the lost note answers
  its own page again. *Fails on:* every note's anchor recomputed on write.
- **Writes.** A progress `PUT`, then another on another page: the anchor names
  the second page. *Fails on:* the anchor written on insert only. A position
  past the end is stored with all four columns NULL, and every write path
  leaves the first three all set or all NULL.
- **The fold carries anchors.** After a fold, change the layout so the moved
  position's slot drifts: it still answers its page. Run it twice, once with
  no position on the target's edition (the insert) and once with an older one
  there (the update). *Fails on:* the columns dropped from `move_positions`'
  `INSERT … SELECT` (the first run) or from its `DO UPDATE SET` (the second).
- **The fold plans from resolved slots.** A fold pair whose source row's
  stored slot, read in the source's current layout, names a half the twin does
  not hold, while its anchor names a whole page the twin holds: the fold is
  not refused, and the row answers that page afterwards. *Fails on:*
  `unplaceable` reading the stored slot (the fold is refused).
- **A half.** A work that reads right to left (every scanned work does), a
  position on the left half of a spread, which is second in that order, and a
  note there; the page's flag is then cleared. The position answers that
  page's one slot; the note answers its stored slot, `stale`. On the right
  half, first in that order, the stored slot already is the page's slot, so
  the right half cannot tell the two treatments apart. *Fails on:* the
  position treated as lost (the next page's slot is answered); the note placed
  on the page.
- **A vanished edition key.** A position and a note on an edition key the
  work no longer has, at a slot above 0, made through `import::sidecar` as
  `reimporting_with_fewer_editions_removes_the_ones_that_went` in
  `import/tests.rs` makes one; the plan builds the fixture. The position
  answers its stored slot and the note answers `stale`. *Fails on:* the
  position clamped against an edition with no slots (0 is answered); `stale`
  not set where no layout is found.
- **The notes version.** `GET` answers `taiyaku_notes: 1`; a `PUT` without it
  is accepted; one with 2 is refused, and so is a `library import` of such a
  file. *Fails on:* the check removed.
- **`stale` on the way in.** For an account narrowed so a lost note is held
  on a page it is not shown, a `PUT` that echoes `stale: true` and one that
  drops it are both accepted, and neither stores it. *Fails on:* `stale` left
  on either side of `as_stored`'s comparison (one of the two is refused);
  `stale` stored. `library export` writes `notes.json` with no `stale`.
  *Fails on:* export writing `state::notes`' answer as it stands.
- **Import.** A progress and a notes document imported over a store leave
  anchors that answer the right pages after a layout change. *Fails on:* the
  import writing around `state::`.

## 4. Lane 3: scheduled scans and O186

Part C is §4 of `docs/superpowers/specs/2026-10-04-day-to-day-deployment-design.md`.
This lane builds its scheduler, its startup scan and its settings, and leaves
the watcher. What follows is what changed since Part C was written and what its
pre-flight settled; everything else is Part C's.

### Decisions

1. **The schedule is per directory** (ruled 2026-11-10). One setting, shared by
   every library that holds the directory, as `root.scan_interval_secs` already
   is. The libraries design's decision 6 (2026-10-05) put it at the library and
   is superseded: with overlapping libraries, a shared or nested directory
   would be walked once for each library and hold several intervals, against
   "one set of settings and one scan".
2. **Off is stored as 0** (told to the owner). Automatic stays NULL (Part C
   §4.3). Any other value is "Every". A negative value is refused.
3. **Every is at least 5 minutes, enforced by the server** (told to the owner).
   Each scan holds the job lock through a whole-store derive, so a shorter
   interval would keep rule saves, stamps and moves refused.
4. **A tick batches the due directories into one job**, through the existing
   scan registry and job lock (the spec's own call, from the daily
   pre-flight's C-5).
5. **The watcher waits for a NAS measurement** (told to the owner). The NAS
   library cannot be written by the container's user, so its probe would answer
   *unverified* and the watcher would run untested.
6. **O186: the server's scan job writes `deriving` before the derive, and
   startup rewrites it to `interrupted`** (the spec's own call, from the
   formats pre-flight's option a; the rulings put O186 in this lane). The
   startup scan heals it.
7. **The new status words join `WIDEST_ENDS`** (the spec's own call, reading
   O253's ruling of 2026-11-09, which names `failed` and `refused`).
8. **An Off directory is not in the startup scan** (the spec's own call). Off
   means no scan the owner did not ask for. An `interrupted` status on it stays
   until Scan is pressed.
9. **A batch goes on past a directory it cannot walk** (the spec's own call),
   as the CLI's `library scan` does. One unreachable directory does not hold
   back the others, or the derive.
10. **The CLI's `library scan` writes no `deriving`** (the spec's own call). It
    keeps today's statuses; a CLI scan cut during its derive still reads
    `ok`, and the scheduler's next scan of it heals the derive. The scheduler
    replaces a host cron of `library scan`, so the server's job is where O186
    is closed.
11. **The probe's interval stays in what reports the probe** (the spec's own
    call): the add's answer (`added_json`'s `interval_secs`) and the CLI's
    `library scan` line ("scanning every 24h" for a watchable directory). They
    say what the probe implies, which the watcher makes true. Until it lands
    they overstate a watchable directory's interval; this is a stated residual.
    The CLI line also pins that the probe gets time to answer (`tests/cli.rs`).

### The setting

- `scan_interval_secs`: NULL is Automatic, 0 is Off, 300 or more is Every.
- `config_api::root_patch_from_json` refuses a negative value and one from 1 to
  299 with a 400 that states the minimum. The minimum is one constant in the
  library, read by this check and by the scheduler.
- The scheduler reads a stored value from 1 to 299 (a dev database) as the
  minimum, and a stored negative value, which nothing can write, as Off.
- **Automatic is `watch.rs`'s unwatched interval for every directory** until
  the watcher ships (the spec's own call, from the daily pre-flight's C-2).
  Part C §4.3 gives that to a directory nothing watches, and nothing does.
- **The effective interval** is the stored one, the minimum for a stored value
  under it, or Automatic's. The scheduler and `next_scan_at` read it.
- `scan::probed` stops writing the probe's interval into the row (Part C
  §4.3). The add still probes, and `added_json` still reports the probe
  (decision 11).

### The scheduler

- A new module in `taiyaku-serve` holds one task, spawned in `serve` beside
  `registry::spawn_refresher`, for a live registry only.
- It wakes on a fixed tick, shorter than the minimum interval. A `PATCH`, an add
  or a removal is read at the next tick, so nothing has to wake it.
- **Each tick:**
  - It reads every directory's interval and `last_scan_at`, through a new
    library function handed `now`, so a test sets the clock.
  - A directory is due when it is not Off, and it has never been scanned or
    its `last_scan_at` plus its effective interval is at or before `now`.
  - It drops each due directory whose registry slot is reserved and not done.
  - It reserves the rest at once with `scan_jobs::reserve` (`library: None`),
    sorted by path as `reach::walked_by` sorts, and runs them as one job with
    `scan_jobs::spawn` and a new `Reread::Batch`.
- `reserve` refuses every directory when one is busy. A refusal (a press that
  raced the tick) leaves the batch to the next tick.
- **`Reread::Batch`** (decision 9). `Reread::Dir` and `Reread::Library` end
  the whole job at the first walk that fails, mark the directories walked
  before it `failed`, derive nothing, and give every slot one sentence; a
  directory changed while the job waited cancels them all. A batch instead:
  - drops a directory that changed while the job waited for the lock, and
    answers its slot with `Reread::Dir`'s words for it;
  - records a walk that fails as `scan_root` already does (`failed`), answers
    that directory's slot with its own error, and walks the next;
  - derives once over what it walked, and answers each walked directory's slot
    with its own report;
  - words a failed derive without naming one directory, and answers every
    walked directory's slot with it.

  `run` therefore answers each directory's report separately for a batch,
  which `scan_jobs::spawn_job` already lands slot by slot.
- A directory due while its slot is reserved is passed over, and the scan that
  held it writes `last_scan_at`, which satisfies the timer. A timer never needs
  a pending rescan; only watcher events would.
- **The first tick is the startup scan.** It takes every directory that is not
  Off, due or not, and runs after the bind, so the server answers while it
  runs.
- **A scheduled scan is a scan like any other.** Its slots carry
  `library: None`, which `libraries_api::scan_status_json` already reads as a
  slot no request reserved. While it runs, what it holds answers 409 in the
  words each refusal already uses:
  - a rule save, a stamp, and a move or an unlink in a library none of whose
    directories it reserved: O245's (`library_job::waits_for_the_job`), since
    it holds the job lock;
  - a move or an unlink in a library holding a directory it reserved:
    `scan_jobs::waits_for`'s, naming that directory, since
    `libraries_api::relinking` asks the reservation before the lock;
  - a directory's or a library's removal: `scan_jobs::removal_waits_for`'s;
  - a Scan press on one of its directories: "already running", and `RootRow`
    takes up the running scan.

  A page that did not draw the scan is O253's accepted remainder.

### The facts and the screen

- `config_api::root_json` gains `next_scan_at`: `last_scan_at` plus the
  effective interval. It is NULL for Off, and NULL for a directory never
  scanned, which is due at the next tick. The page draws the first as "off"
  and the second as "due" (`scan_interval_secs` tells them apart).
- `RootRow`'s "Scan interval (seconds)" box becomes Part C §4.3's schedule
  control: Automatic, Every (a number with a minutes or hours unit), or Off.
  It refuses less than 5 minutes, as the server does (overlapping-libraries
  decision 22: a page greys out only what the server refuses), and draws the
  server's words when the server refuses.
- `config.ts`'s `regimeFacts` reads "scan every" as Automatic, Off or the
  interval, and adds "next scan". `LibraryPage.tsx`'s "Scan every" column reads
  the same words.
- The README's "(Nothing schedules scans yet; …)" aside, at the end of the
  paragraph on `keeps_documents`, says what the scheduler does instead.

### O186

- `scan::scan_root` keeps writing `ok`, `refused` or `failed` after the walk.
  New functions in `scan.rs` keep its rule that one module writes
  `last_scan_status`:
  - one marks a directory `deriving` where its walk wrote `ok`;
  - one marks a job's directories `ok` where they read `deriving`;
  - one rewrites every `deriving` to `interrupted`.
- `scan_jobs::run` marks each directory `deriving` right after its walk, so a
  stop between two walks reads `interrupted` too, and marks them `ok` once
  `derive::derive` returns.
- A failed derive still marks them `failed` through
  `scan::mark_derive_failed`, which now writes only where the status reads
  `deriving`: a refused walk keeps `refused`, and a failed walk `failed`.
  Today it overwrites any status.
- `run` gains a form that takes its walk and its derive as arguments, as
  `migrate_with` takes its migrations, so a test can read the statuses from
  inside the second walk and inside the derive.
- The CLI's `library scan` writes no `deriving` (decision 10).
- `serve` rewrites `deriving` to `interrupted` before the bind, beside the
  session sweeps. It is reported and never fatal.
- Only the server writes `deriving`, and it rewrites before its own first
  scan can begin.
- The startup scan walks every directory that is not Off and derives the
  whole store, which heals the cut derive and writes the walked directories'
  status again.
- `config.ts`'s `lastScan` draws the new words as it draws `failed`.
  `WIDEST_ENDS` gains `deriving` and `interrupted`.

### Boundaries

- No watcher, no three-state probe kept in memory, no `root.watch` column:
  that is Part C's watcher half, after the NAS measurement. This lane adds no
  migration.
- No per-library schedule, and no library-page control that sets every
  directory at once.
- O260 (Scan library not held while an outer scan runs) is not decided here,
  though scheduled scans make its situation common.
- O51 is narrowed to the watcher and the probe kept at startup.

### Measured on the NAS later

Part C §4.4 stands: whether the probe succeeds through `shfs`, how long a
rescan takes warm and with disks asleep, and page latency during a scan (O52,
O42). The unwatched interval against the disks' spin-down is settled there, on
the screen, with no code change.

### Testing

- **The due rule**, on a clock the test sets: a directory never scanned is due;
  one second before `last_scan_at` plus its interval it is not, and at it, it
  is; Off is never due; Automatic uses the unwatched interval. *Fails on:* `<=`
  made `<`; 0 read as Automatic; Automatic reading the probe.
- **The minimum.** A `PATCH` with 299 and one with -1 answer 400; 300, 0 and
  null answer 204 and are stored. *Fails on:* the check made `<= 0`.
- **A stored value under the minimum** (60) is scheduled at the minimum, and
  a stored -1 is never scheduled. *Fails on:* the stored value used; a
  negative read as due at once.
- **`next_scan_at`** for an Automatic directory is `last_scan_at` plus the
  unwatched interval, and null for one never scanned. *Fails on:* the stored
  NULL read as no interval (null for Automatic).
- **One job.** Two due directories are reserved with one start instant and
  derived once. *Fails on:* each spawned alone.
- **A busy directory.** One due directory reserved by a press and one free: the
  tick runs the free one, and the pressed one is not scanned again once its
  scan ends. *Fails on:* both handed to `reserve`, so nothing runs; or
  `last_scan_at` ignored after the press.
- **Startup.** The first tick scans a directory that is not due, and does not
  scan an Off one. *Fails on:* startup using the due rule; startup ignoring
  Off.
- **A change between ticks.** A `PATCH` to Off: the next tick does not scan it.
  *Fails on:* the scheduler keeping the directories it read at startup.
- **The add.** `adding_a_root_writes_the_probes_interval_onto_the_stored_row`
  and `adding_a_root_returns_the_same_interval_it_wrote_to_the_row` flip to "no
  interval written". *Fails on:* the write restored.
- **A batch past a failed walk.** Two due directories, the first in path
  order unreadable: the second is walked, the derive runs, the first's slot
  reports its own error and the second's its scan. *Fails on:* the batch
  stopping at the first failed walk, as `Reread::Dir` does (the second is not
  walked).
- **O186 in the job.** Inside the injected second walk, the first directory
  reads `deriving`; inside the injected derive, each walked directory reads
  `deriving`; after `run`, `ok`. *Fails on:* `deriving` written for all once
  the walks are done (the first test); `deriving` written after the derive;
  `ok` never written back.
- **A failed derive.** A refused walk and an ok walk, then an injected derive
  that fails: the first reads `refused`, the second `failed`. *Fails on:*
  `mark_derive_failed` writing whatever the status reads, as today.
  `mark_derive_failed_corrects_a_status_a_successful_scan_already_wrote`
  marks the directory `deriving` first.
  `mark_derive_failed_leaves_the_dir_that_took_a_removed_ones_id_alone` marks
  the new directory `deriving` before the call and asserts it still reads
  `deriving` after, since a status left NULL passes it with the id-and-path
  guard removed.
- **O186 at startup.** A store with one directory at `deriving` and one at
  `ok`, served: the first reads `interrupted`, the second is untouched. *Fails
  on:* the rewrite skipped, or made unconditional.
- **The control**, jsdom: it saves null, 0, and minutes or hours as seconds,
  and refuses less than 5 minutes. `RootRow.test.tsx`'s "refuses to store an
  interval of zero or less" flips for 0. `regimeFacts` reads Automatic, Off
  and next scan (`config.test.ts`), and `LibraryPage.test.tsx` the column's
  words. *Fails on:* Off saved as null.
- **`WIDEST_ENDS`.** A row drawn scanning holds unseen the `lastScan` words for
  `deriving` and for `interrupted`. *Fails on:* either left out.

## 5. Lane 4: missing works and O184

Part D is §5 of the deployment spec. What follows is what changed since it was
written and what its pre-flight settled; everything else is Part D's.

### Decisions

1. **Two PRs** (rulings, 2026-11-10): mark, hide and bring back; then list and
   remove.
2. **The presence test asks every directory a member names** (the spec's own
   call, from the daily pre-flight's D-1). A work is present while any of its
   members has a `scan_entry` with the member's `sha256` in any directory the
   member's `member_root` rows name: the join `scan::repoint_kept_members`
   already uses to find a surviving copy. It does not depend on the primary
   having been re-pointed first. For a work this derive writes, `write_roots`
   has already pointed the primary at a surviving copy, so there the primary
   path would give the same answer. Not "written this pass", which hides
   O243's works.
3. **The hide is carried on the registry's work** (the spec's own call, from
   D-2). `Access::Every` skips `browse::visible_editions`, so a predicate in
   the grant clause alone would still list missing works to the owner in
   `/api/works` and OPDS.
4. **A removal takes the library job lock and answers 409 in O245's words**
   (told to the owner).
5. **O184 is folded in** (rulings, 2026-11-10), in the second PR.
6. **A removal is one work or all of them, and each row names the libraries
   that reach it** (the spec's own call, from D-4). The libraries column
   stands in for the libraries design's §9 grouping; the tab has no filter. A
   work is addressed by its slug, in the routes and the CLI, as every other
   work route addresses one.
7. **A removal leaves a deleted work's hand document where it is, unlisted**
   (the spec's own call), as a directory removal leaves a work it deletes
   (`scan::take_root`). Where that directory keeps documents, the removal's
   reconciling derive reports it among `orphan_documents` (inferred: `hand::reconcile`
   finds no work with its content key). The pre-flight's D-6 recommended
   listing them, on the reading that O247's removals do; they list only the
   documents of works that survive.
8. **The mark is per work, not per account** (the spec's own call). A work
   with one edition under library A and another under B, whose A files are
   gone, stays present through B. An account granted only A still lists it,
   with every edition it can see missing; `browse::missing_by_edition` counts
   those members, as today.
9. **The mark is counted** (the spec's own call). `DeriveReport` gains how many
   works it marked and how many it cleared, and the scan report carries both,
   so a partial mount that hides hundreds of works says so.
10. **A work with no member is never marked** (the spec's own call), like an
    imported one: no scan is the authority for it.

### PR 1: mark, hide, bring back

**The mark.**

- One migration (the next free number at rebase) adds `work.missing_since
  TEXT`, NULL for a present work.
- `derive::derive_with` runs one pass after the fold loop, so folded works are
  gone first, and after the digests are written (the plan checks the order).
  It sets `missing_since = datetime('now')` on a work none of whose members is
  present and whose mark is NULL, and clears it on a marked work that is
  present again. It runs under both `Documents` modes, since a rolled-back
  preview predicts it.
- **The pass writes only rows whose mark changes.** Every `UPDATE` of `work`
  re-indexes its title (`work_au`) and bumps `structure_version`
  (`structure_work_upd`, which has no `WHEN`). `write_work` already bumps the
  counter for every work it writes, so what the condition saves is a
  re-index of every missing work's title on every derive, and a bump on a
  store whose every work is missing or imported.
- A work with a member that has no `member_root` row is treated as imported and
  never marked (Part D; O54). O249's leftover reads that way too, as its
  pre-flight accepts. A work with no member is never marked (decision 10).
- The pass's counts go into `DeriveReport` and the scan report (decision 9).
- O243's unfolded works hold files that are present, so they stay unmarked.
- An exclude pattern's losses mark too: `forget_unseen` deletes the
  `scan_entry` rows a pattern now covers, and clearing the pattern brings them
  back (O92).
- **A returning file rejoins its work.** `derive::held_work` finds a volume's
  work by `(series, volume)` and a volume-less one by `member.path`, and the
  missing work's rows were never deleted (inferred; the first test settles
  it). A volume-less work whose file returns at another path mints a new work,
  and the old one stays missing (inferred).
- A returning file is read and hashed again once (Part D §5.3).

**The hide.**

- `browse`'s listings take `missing_since IS NULL`. The plan enumerates them
  from the code before the first change, and the review checks the enumeration
  (Part D §5.2), and says why each entry takes the predicate. The
  pre-flight's list, less `browse::status`, which reads libraries and no work
  row, is `home`, `series`, `unfiled`, `one_series`, `search`, `cover_work`
  and `visible_editions`.
- `browse::work` and `browse::visible_members` stay unfiltered, so the reader
  can answer with a reason. `browse::missing_by_edition` goes on counting
  missing members.
- `registry::work_rows` reads the mark, and `WorkRow` and the registry's
  `Work` carry it. `routes::list_works` and `opds::works_feed` skip a missing
  work for every access, `Access::Every` included.
- `routes::get_sidecar` answers a missing work 404 with words saying its files
  are missing. The reader's `LoadFailure` gains that kind, and `ReaderFailed`
  says it. Every other work route answers as today, so a missing work's
  positions and notes stay readable.

### PR 2: list and remove

- **A Missing works tab** in Configuration (`config.ts`'s `TABS`), for
  administrators only. One row per missing work: its title, the libraries that
  reach it, the path last seen, missing since, and its positions and notes.
  Remove one, or remove all, each behind a dialog that shows the counts first.
- **Routes**, administrator-gated and in the route table's admin list:
  - `GET /api/library/missing` lists them;
  - `DELETE /api/library/missing/{work}`, `{work}` the slug (decision 6), and
    `DELETE /api/library/missing` remove one or all;
  - each `DELETE` takes `?dry_run=true` (`config_api::DryRunQuery`) and answers
    the same body as `config_api::delete_root`:
    `{"removed": {works, positions, notes}, "derive": …, "dry_run": …}`.
    `removed` holds the three counts of `scan::Removal` that a removal of
    works has (its directory fields do not apply). `derive` is the report of
    the derive below, or for a dry run the one from the transaction it rolls
    back, as `scan::removal_cost` answers it.
- **The lock.** A removal takes `library_job` with `try_lock`. While it is
  held, the removal answers 409 in `library_job::waits_for_the_job`'s words.
  The dry run takes no lock (the spec's own call), as `config_api::delete_root`'s
  does not, so the dialog's counts still answer during a scheduled scan.
- **What a removal deletes.** Only works still marked when it runs, read inside
  its transaction: a work whose files came back since the list was read stays.
- **Its derives** are a directory removal's two (`scan::remove_root`): one
  under `Documents::Skip` inside the transaction, and one under
  `Documents::Reconcile` after the commit, whose report the answer carries.
- **Documents.** A deleted work's hand document stays where it is, unlisted
  (decision 7).
- **The CLI.** `taiyaku library missing` lists; `--remove SLUG…` and `--all`
  remove. Part D wrote `--remove ID…`; this spec changes it to slugs
  (decision 6). Like every CLI command it runs in its own process and takes
  no server lock.
- **O184.** With a live registry (`Works::is_live`), `works::row_id_for_write`
  and `works::row_id_for_document` never mint a `bridge:` row. A progress,
  notes or truth `PUT`, or an alignments `PATCH` (through `alignments::store`),
  for a work whose row is gone answers 404 for a work it does not hold. A fixed
  registry still mints: the CLI's argv mode imports every sidecar before it
  serves, so its works have rows, but `test_support::state`'s fixtures and
  `corpus.rs` pair a fixed registry with an empty library. `alignments::store`'s doc
  comment says so.

### Boundaries

- No threshold for a partial mount: O48 closes by consequence (Part D §5.2).
- No change to what a directory removal does.
- A removal frees its works' slugs for later works (lane 1's residual).

### Testing

- **Part D's central claim.** On a real filesystem: delete a volume's file,
  scan, restore it, scan. The work is hidden between the scans, and after the
  second it is listed with the same id, its progress and its notes. Written
  and watched failing before derive changes. *Fails on:* the mark never
  cleared.
- **Copies** get no test of their own. A work held under two directories with
  one copy deleted is a work the derive writes, and `write_roots` re-points
  its primary at the surviving copy before the mark pass runs, so no
  plausible defect in the presence test reads it as missing.
- **O243.** `removing_the_older_directory_of_a_split_volume_leaves_its_work_unfolded`
  and `removing_the_directory_of_one_of_two_editions_keeps_its_stale_edition_beside_the_copys_work`
  also assert the works stay unmarked. *Fails on:* presence read as "written
  this pass".
- **Imported.** A work whose member has no `member_root` row is never marked,
  and nor is a work with no member. *Fails on:* either condition dropped.
- **Counts.** A scan that hides two works and one that brings one back carry
  those numbers in `DeriveReport` and the scan report. *Fails on:* the counts
  not incremented.
- **A partial mount** (O48's scenario): half a directory's subdirectories gone
  hides that half, and remounting brings it back. *Fails on:* the mark never
  cleared.
- **An exclude pattern** hides its works, and clearing it brings them back.
- **Quiet passes**, measured on the mark pass alone, not a whole derive
  (`write_work` bumps the counter for every work it writes): on a store whose
  works are all already marked or imported, the pass leaves
  `structure_version` where it was. *Fails on:* an unconditional `UPDATE`.
- **Every listing.** One test per listing the plan enumerated, each `browse`
  query, `/api/works` and an OPDS feed, the last two under `Access::Every` and
  under a grant. *Fails on:* that listing's predicate removed, one at a time,
  so a listing added later without it fails its own test.
- **The reader.** The sidecar `GET` of a missing work answers 404 in the
  missing words, and `ReaderFailed` draws them. *Fails on:* the sidecar served.
- **The list** carries each row's fields, the libraries included.
- **Dry run.** Its counts equal what the removal then deletes, and it leaves
  the store unchanged. *Fails on:* the dry run committing.
- **A returned work.** A removal of a work whose mark cleared since the list
  deletes nothing. *Fails on:* the mark not checked.
- **The lock.** With the lock held by the test, a removal answers 409 in O245's
  words, and its dry run answers its counts. *Fails on:* the lock not taken;
  the dry run taking it.
- **Documents.** A deleted work's hand document under a directory that keeps
  documents is still on disk afterwards, and the answer's reconciling report
  names it among `orphan_documents`. *Fails on:* the removal skipping its
  `Reconcile` derive.
- **The CLI.** `library missing` prints and deletes nothing; `--remove` and
  `--all` remove.
- **O184.** A progress, a notes and a truth `PUT` and an alignments `PATCH`
  for a work deleted since the registry's last refresh answer 404, and the
  store holds no new work row. *Fails on:* the mint restored.
- **The tab**, jsdom: its rows, its dialog's counts, its empty state, and its
  absence for an account that does not administer.

## 6. Lane 5: cache cleanup (O11)

### Decisions

1. **An orphan sweep at startup and every 24 h** (ruled 2026-11-10). It deletes
   the cache entries no member's `sha256` holds.
2. **A default 10 GiB cap, least recently used first** (ruled 2026-11-10), set
   by `--cache-max` or `TAIYAKU_CACHE_MAX`.
3. **Use is a file's mtime, refreshed on a server-side hit at most once a
   day** (the spec's own call, from the daily pre-flight's option B). Not
   atime, which depends on how `/cache` is mounted.
4. **A cap of 0 means no cap** (the spec's own call), and a size is a byte
   count with an optional binary unit.
5. **A staging leftover is an orphan** (the spec's own call): the sweep
   deletes a temporary file a crashed write left.
6. **O11 is narrowed to its Stump half** (the spec's own call). O11 covers two
   caches: the cover and page caches this lane cleans, and the Stump
   resolution cache (`stump/cache.rs`, `taiyaku.stump.json`), which this lane
   leaves as it is, with `--refresh-stump` as its escape hatch.

### The sweep

- It covers both cache directories under the cache root, `covers` and
  `thumbs` (`ServeState::thumb_cache_dir`). Both are flat, and every name
  `cover::cache_path`, `cover::thumb_cache_path` and `cover::page_cache_path`
  mint starts with the member's `sha256`.
- **In order**, on the blocking pool:
  1. It lists both directories.
  2. It reads the digests the store holds (`member.sha256`). After the listing,
     so a file written during the sweep is either unlisted or covered.
  3. It deletes each listed file whose whole name parses as a cache key and
     whose digest is not held. A key is parsed by its shape, with any long
     edge and any format version: `{sha256}-{edge}v{n}.jpg` (a cover),
     `{sha256}-p{offset}-{edge}v{n}.jpg` (a tile) and
     `{sha256}-p{offset}-r{edge}v{n}.jpg` (a reading page). So a file of an
     old version or an old rung is a keyed file, and the cap can evict it.
  4. It deletes a staging leftover, a key's name followed by
     `.tmp.<pid>.<n>` as `taiyaku_read::atomic::temp_path` mints it, last
     modified before this process started. One modified since may be a write
     in progress. In the container the server is PID 1 on every start, so
     the pid cannot tell a leftover from a live write; the time can.
  5. It leaves a file it cannot parse, and counts it.
  6. When a cap is set and the files left total more than it, it deletes keyed
     files, oldest mtime first, until the total is at or under the cap or no
     keyed file is left. The total counts every regular file in both
     directories, unkeyed and staging files included, since they take the
     disk too; only keyed files are evicted.
- It runs in the background once after the bind and every 24 h after that, as a
  task spawned in `serve` beside `registry::spawn_refresher`, for a live
  registry only. `test_support::state`'s fixtures and `corpus.rs` pair a
  fixed registry with an empty library, whose renderings no `member` row
  holds.
  The CLI's argv mode also serves a fixed registry, so it is not swept
  either; it is not how the server is deployed.
- It writes one line to stderr per sweep: the files and bytes it deleted as
  orphans, as leftovers and as evictions, and the total left. A failure is
  reported and never fatal.
- Works lane 4 marks missing keep their renderings: their members' digests
  are still held.
- A request racing a deletion has either read the file already or misses and
  generates it again. A deleted file is a miss, never an error.

### Use

`cover::cached`, on a hit, sets the file's mtime to now when it is more than a
day old (`File::set_modified`, stable since Rust 1.75; the workspace requires
1.88). A failure to set it is ignored.

So the cap tracks the server's reads, not a reader's use. A grid cover is sent
`private, max-age=31536000, immutable` (`routes::COVER_CACHE_CONTROL`), so a
browser that holds one does not ask again, and a matching `If-None-Match` is
answered 304 before the cache file is read. The covers browsers hold are
therefore evicted first. That costs a device that never held one a
regeneration, and nothing else.

### The cap

- `--cache-max SIZE` and `TAIYAKU_CACHE_MAX`, read in the CLI's
  `serve_options` beside `--cache`. The flag beats the variable. With neither,
  the cap is 10 GiB. 0 means no cap. SIZE is a byte count with an optional
  binary unit, `K`, `M`, `G` or `T`. A value that does not parse refuses to
  start.
- `ServeOptions` gains the cap. `ServeState` gains nothing.
- The cap holds at each sweep. Between sweeps the cache can grow past it by
  what a day's reading writes.
- `serve_refuses_an_unknown_flag` pins the flag alphabet and changes with it.
  The README's `/cache` row in the mounts table, and the setup lines below it,
  state the cap and the sweep.

### Boundaries

- No sharding of `thumbs`, and no sweep on the request path.
- Files of an old format version or an old ladder rung are not swept by name.
  Nothing reads them again, so they age out under the cap.
- An image-folder volume edited in place keeps its digest and its stale
  renderings (O56). Clearing the cache by hand stays the answer.
- The Stump resolution cache is a server cache this lane leaves alone
  (decision 6). The CLI's `.taiyaku-cache` belongs to `align` and `verify`
  and is not a server cache.

### Measured on the NAS later

- The cache's growth after a few weeks of real use, split into reading pages
  and tiles. It says whether 10 GiB is the right default.
- How long a sweep's listing takes on the mapped `/cache`. If it is slow,
  `/cache` maps to the pool path (a deployment note), or `thumbs` is sharded.

### Testing

- **Orphans.** A cover, a tile and a reading page for a held digest and for an
  unheld one: the unheld three go and the held three stay. *Fails on:* the
  digest parsed from the wrong part of the name.
- **A member whose file has gone** keeps its renderings: its `scan_entry` row
  deleted (`forget_unseen`), its `member` row standing. This is lane 4's
  missing work without needing lane 4's column. *Fails on:* digests read from
  `scan_entry.sha256` instead of `member.sha256`.
- **Leftovers.** A staging-shaped file older than the process goes; one newer
  stays. *Fails on:* the age check dropped.
- **Unparsable.** A file with no key's shape stays. *Fails on:* every unkeyed
  file deleted.
- **The cap.** Files of set sizes and mtimes over a cap: the oldest go until
  the total is at or under it, and a total exactly at the cap is left. *Fails
  on:* eviction newest first; eviction one file past the cap.
- **Old keys.** A file named with an old format version is evicted under the
  cap like any keyed file. *Fails on:* the parser accepting only the current
  version (the file is left as unparsable).
- **A cap held up by unkeyed files.** Unkeyed files alone over the cap: every
  keyed file is evicted and the sweep ends. *Fails on:* eviction looping
  while the total stays over.
- **No cap.** A cap of 0 evicts nothing. *Fails on:* 0 read as zero bytes.
- **Listing first.** A hook between the listing and the digest read writes a
  file and adds its digest to `member`: the file stays. *Fails on:* the digests
  read before the listing.
- **Use.** A hit on a file more than a day old sets its mtime to now; a hit on
  one younger than a day leaves it. *Fails on:* every hit touching the file;
  no hit touching it.
- **The schedule.** On a clock the test controls, the task sweeps at start and
  again 24 h later, and not between. *Fails on:* a single sweep.
- **The option.** `--cache-max 5G`, `TAIYAKU_CACHE_MAX`, the flag over the
  variable, a bad value refused, and the 10 GiB default. *Fails on:* the
  variable ignored.
- **One integration test** runs a sweep against a real `ServeState` over a
  scanned scratch store.

## 7. What this does not do

- **Part E** (pending library writes, O82). Hand documents stay off unless a
  directory asks for them, and their address under `<root>/.taiyaku/` does not
  change. When Part E is built, its flush needs lane 1's newer-file guard.
- **The migration squash**, dropped (told to the owner). A forward migration is
  not a breaking change, so a squash buys nothing for a database that does not
  exist yet, and it costs the tests that pin the chain. Dead columns, such as
  `member.format` (O69), go in an ordinary migration later.
- **The web half of O244**: anchors on the wire, O240, and `stale` drawn. A
  follow-up PR after lane 2.
- **The watcher**: Part C's three-state probe kept in memory, `root.watch`, and
  the recursive watch. After the NAS measurement.
- **NAS setup**: the proxy's block and `TAIYAKU_TRUSTED_PROXY`, the WUD labels,
  the container's mounts and where `/config` and `/cache` live on the array, an
  edition key per directory before the first scan, and the first scan itself.
  These are steps, not code; `deploy-report.md`, in the same local evidence
  directory, lists them.
- The survey's other contracts that nobody ruled on for this work, a ComicInfo
  stamp's lack of a taiyaku marker and a directory's path that cannot be
  changed in place, stay as they are.
