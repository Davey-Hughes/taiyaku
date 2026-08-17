-- Structure -----------------------------------------------------------------

-- `cover_work_id` references work(id), which references series(id). The cycle
-- is fine and is why the column is nullable: a series is inserted with no
-- cover, and the work that becomes its cover is inserted second.
CREATE TABLE series (
  id            INTEGER PRIMARY KEY,
  sort_title    TEXT NOT NULL,
  reading       TEXT,
  cover_work_id INTEGER
);

-- One series, several names. Komga's single title is the annoyance this exists
-- for, and it is also what FTS5 indexes so the romaji, the native title and the
-- English title all find the same shelf.
CREATE TABLE series_title (
  series_id  INTEGER NOT NULL REFERENCES series(id) ON DELETE CASCADE,
  title      TEXT NOT NULL,
  kind       TEXT NOT NULL DEFAULT 'alt',  -- romaji | native | english | alt
  is_primary INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (series_id, title)
);

-- `content_key` is a sha256 over the HUB EDITION'S members' sha256s, in member
-- order — that edition alone. Folding in every edition would change a work's
-- identity whenever an edition was added, which is the entire point of this
-- project. `volume` is REAL because omnibus and half-volumes exist.
CREATE TABLE work (
  id            INTEGER PRIMARY KEY,
  content_key   TEXT NOT NULL UNIQUE,
  slug          TEXT NOT NULL UNIQUE,
  series_id     INTEGER REFERENCES series(id) ON DELETE SET NULL,
  volume        REAL,
  title         TEXT NOT NULL,
  reading       TEXT NOT NULL,
  hub           TEXT NOT NULL,
  sidecar_path  TEXT,
  sidecar_mtime INTEGER,
  imported      TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE edition (
  id       INTEGER PRIMARY KEY,
  work_id  INTEGER NOT NULL REFERENCES work(id) ON DELETE CASCADE,
  key      TEXT NOT NULL,
  label    TEXT NOT NULL,
  language TEXT,
  kind     TEXT,                            -- raw | official | fan | NULL
  UNIQUE (work_id, key)
);

CREATE TABLE member (
  id               INTEGER PRIMARY KEY,
  edition_id       INTEGER NOT NULL REFERENCES edition(id) ON DELETE CASCADE,
  ordinal          INTEGER NOT NULL,
  path             TEXT NOT NULL,
  sha256           TEXT NOT NULL,
  pages            INTEGER NOT NULL,
  format           TEXT,                    -- zip | dir | epub-fixed | epub-reflow | pdf | rar | 7z
  chapter_declared REAL,
  chapter_source   TEXT,                    -- comicinfo | filename | hand
  UNIQUE (edition_id, ordinal)
);

-- The primary key is a true statement about the rows — a page is flagged or it
-- is not — and it stays. What does NOT follow from it is that a document
-- listing a page twice is malformed: `"spreads": [3, 3]` states one fact twice,
-- `Sidecar::from_json` accepts it, and `project.rs` reads these back
-- `ORDER BY page`, so this list has been a set all along. `import.rs` writes
-- them with INSERT OR IGNORE for that reason. Refusing the import instead is
-- review 2's I3, where one hand-edited document took down the whole server.
CREATE TABLE page_flag (
  edition_id INTEGER NOT NULL REFERENCES edition(id) ON DELETE CASCADE,
  kind       TEXT NOT NULL,                 -- spread | credit
  page       INTEGER NOT NULL,
  PRIMARY KEY (edition_id, kind, page)
) WITHOUT ROWID;

-- Alignment -----------------------------------------------------------------

-- `verdict` is part of the document and MUST round-trip: `Quality` derives
-- Serialize, so `to_json` writes whatever the struct holds, and a projection
-- that recomputed it instead would emit a different document from the one it
-- was given. It is still never an AUTHORITY — `Sidecar::quality_of` recomputes
-- from the runs when anyone asks a question (core/sidecar.rs:1220), so a
-- hand-edited document cannot make a stale verdict mean anything. Stored and
-- restored faithfully; consulted by nothing.
-- **No `UNIQUE (work_id, from_key, to_key)`, deliberately.** An earlier draft
-- had one, on the reasoning that two edges for one edition pair are ambiguous.
-- They are — and resolving that ambiguity is `taiyaku-core`'s business, not
-- this store's. Nothing in the format forbids the repeat: `Sidecar::from_json`
-- is a plain serde parse, every edge lookup in `core/sidecar.rs` resolves with
-- `.find(…)`/`.position(…)` — so a duplicate pair has always meant "the first
-- one" — and `serve` read exactly that meaning off the file before this crate
-- existed. The constraint turned a legal document into an import failure
-- naming a SQLite constraint rather than the document, and since `serve`
-- imports every path before serving any, one hand-edited work aborted the
-- whole server. Whole-branch review 2, I3.
--
-- So both rows are kept and `project.rs` reads them back `ORDER BY id`:
-- insertion order is document order, and the projection means what the file
-- meant. Same principle as this crate's doc comment — the rules live in
-- `taiyaku-core`, and a second implementation over rows is how two mechanisms
-- come to disagree.
CREATE TABLE alignment (
  id           INTEGER PRIMARY KEY,
  work_id      INTEGER NOT NULL REFERENCES work(id) ON DELETE CASCADE,
  from_key     TEXT NOT NULL,
  to_key       TEXT NOT NULL,
  by           TEXT NOT NULL,               -- solver | hand
  solver_hash  TEXT,
  solver_floor REAL,
  verdict      TEXT,
  slots        INTEGER,
  paired       INTEGER,
  avg_run      REAL
);

-- Four columns per side rather than the sidecar's "1-4R" string: a Slot is
-- { page, half }, and storing the rendered span would make "which runs cover
-- page 47" a text scan.
CREATE TABLE run (
  id            INTEGER PRIMARY KEY,
  alignment_id  INTEGER NOT NULL REFERENCES alignment(id) ON DELETE CASCADE,
  ordinal       INTEGER NOT NULL,
  a_start_page  INTEGER NOT NULL,
  a_start_half  TEXT,
  a_end_page    INTEGER NOT NULL,
  a_end_half    TEXT,
  b_start_page  INTEGER,
  b_start_half  TEXT,
  b_end_page    INTEGER,
  b_end_half    TEXT,
  margin        REAL,
  by            TEXT,
  note          TEXT,
  UNIQUE (alignment_id, ordinal)
);

-- People, grouping, external identity ---------------------------------------

CREATE TABLE person (id INTEGER PRIMARY KEY, name TEXT NOT NULL UNIQUE);

-- A surrogate id rather than a composite primary key over the three nullable
-- columns: SQLite permits NULLs in a non-INTEGER primary key, so a composite
-- one would silently allow duplicate credits. `edition_id` is the one that
-- earns its place — a scanlation group is a property of an edition, and Komga
-- has nowhere to put it.
CREATE TABLE credit (
  id         INTEGER PRIMARY KEY,
  person_id  INTEGER NOT NULL REFERENCES person(id) ON DELETE CASCADE,
  role       TEXT NOT NULL,                 -- author | artist | translator | group | publisher
  series_id  INTEGER REFERENCES series(id) ON DELETE CASCADE,
  work_id    INTEGER REFERENCES work(id) ON DELETE CASCADE,
  edition_id INTEGER REFERENCES edition(id) ON DELETE CASCADE
);

CREATE TABLE collection (
  id      INTEGER PRIMARY KEY,
  name    TEXT NOT NULL,
  kind    TEXT NOT NULL DEFAULT 'collection', -- collection | reading_list
  created TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE collection_item (
  collection_id INTEGER NOT NULL REFERENCES collection(id) ON DELETE CASCADE,
  ordinal       INTEGER NOT NULL,
  series_id     INTEGER REFERENCES series(id) ON DELETE CASCADE,
  work_id       INTEGER REFERENCES work(id) ON DELETE CASCADE,
  PRIMARY KEY (collection_id, ordinal)
);

CREATE TABLE tag (id INTEGER PRIMARY KEY, name TEXT NOT NULL UNIQUE);
CREATE TABLE series_tag (
  series_id INTEGER NOT NULL REFERENCES series(id) ON DELETE CASCADE,
  tag_id    INTEGER NOT NULL REFERENCES tag(id) ON DELETE CASCADE,
  PRIMARY KEY (series_id, tag_id)
) WITHOUT ROWID;
CREATE TABLE work_tag (
  work_id INTEGER NOT NULL REFERENCES work(id) ON DELETE CASCADE,
  tag_id  INTEGER NOT NULL REFERENCES tag(id) ON DELETE CASCADE,
  PRIMARY KEY (work_id, tag_id)
) WITHOUT ROWID;

-- In migration 1 despite nothing reading it yet, because it holds HAND-MATCHED
-- data: deferring the table means matching a whole library against AniList a
-- second time. `user_credential` and `sync_state` are deliberately absent —
-- machine-obtained, nothing to backfill.
CREATE TABLE external_id (
  series_id   INTEGER NOT NULL REFERENCES series(id) ON DELETE CASCADE,
  provider    TEXT NOT NULL,                -- anilist | mal | mangaupdates
  external_id TEXT NOT NULL,
  PRIMARY KEY (series_id, provider)
) WITHOUT ROWID;

-- Per-person state ----------------------------------------------------------

-- `password_hash` is nullable for exactly the window between this spec and A2,
-- which inserts the real account. A2 MUST make it NOT NULL: a nullable password
-- on an internet-facing server is one missed branch from an account that
-- authenticates with nothing.
CREATE TABLE user (
  id            INTEGER PRIMARY KEY,
  name          TEXT NOT NULL UNIQUE,
  password_hash TEXT,
  role          TEXT NOT NULL DEFAULT 'admin',
  created       TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Read status is derived — unread is no row, reading is a row, read is slot at
-- the last slot. `completed_at` and `read_count` exist only for "finished once
-- and now re-reading", which a position cannot express, and they are here
-- rather than added later because reading history is behavioural data with
-- NOTHING to backfill.
CREATE TABLE progress (
  user_id      INTEGER NOT NULL REFERENCES user(id) ON DELETE CASCADE,
  work_id      INTEGER NOT NULL REFERENCES work(id) ON DELETE CASCADE,
  edition      TEXT NOT NULL,
  slot         INTEGER NOT NULL,
  updated      TEXT NOT NULL DEFAULT (datetime('now')),
  completed_at TEXT,
  read_count   INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (user_id, work_id)
) WITHOUT ROWID;

-- `id` is the ULID the reader already mints, kept as the primary key so the
-- per-note merge in import has an identity to merge on. The region is four
-- nullable columns, all four NULL together for a whole-slot anchor.
CREATE TABLE note (
  id      TEXT PRIMARY KEY,
  user_id INTEGER NOT NULL REFERENCES user(id) ON DELETE CASCADE,
  work_id INTEGER NOT NULL REFERENCES work(id) ON DELETE CASCADE,
  edition TEXT NOT NULL,
  slot    INTEGER NOT NULL,
  rx      REAL, ry REAL, rw REAL, rh REAL,
  color   TEXT,
  body    TEXT NOT NULL,
  created TEXT NOT NULL,
  updated TEXT NOT NULL
);

-- **The three `truth_` tables below carry no `user_id`, and that is a decision,
-- not an omission.** `progress` and `note` above are keyed per person because
-- two readers of one work hold two different positions and two different sets
-- of annotations. Labelling is not that: design §4.3 records it as
-- "single-writer in practice" — a `taiyaku.truth.json` is ground truth *about
-- the pages*, one answer per page that a second labeller would be correcting
-- rather than shadowing, and the format itself has no room for a second
-- person's copy (`TruthDoc` is keyed by edition and page, nothing else).
-- Adding `user_id` here would therefore invent a distinction the document it
-- projects cannot express, and `state::truth`/`state::put_truth` take no user
-- argument for the same reason, while `state::notes`/`state::progress` do.
--
-- What this costs, stated so A2 does not have to rediscover it: once a library
-- holds more than one real account, every account shares one labelling. If
-- per-person ground truth is ever wanted, it is a migration that adds the
-- column to all three tables *and* a `taiyaku.truth.json` version bump to carry
-- it — not a schema change alone. `export.rs`'s `LOCAL_USER` comment is the
-- other half of this note: it explains why the per-person documents get away
-- with one hardcoded id today.
--
-- **A page that was looked at is a row here, whether or not it holds boxes.**
-- `TruthEdition::pages`' own doc comment (core/truth.rs:137) is explicit: "an
-- absent key means nobody has looked at that page; a present empty value means
-- somebody looked and found no text. Across 75 pages of manual work that
-- difference is the progress tracker, and it is the same distinction
-- `RegionEdition::pages` refuses to collapse."
--
-- An earlier draft of this schema had `truth_box` alone. Boxes are one row
-- each, so a page examined and found empty wrote zero rows — indistinguishable
-- from a page nobody had opened, silently destroying the labelling progress
-- record. Found in Task 9 by a test rather than by reading.
CREATE TABLE truth_page (
  work_id INTEGER NOT NULL REFERENCES work(id) ON DELETE CASCADE,
  edition TEXT NOT NULL,
  page    INTEGER NOT NULL,
  PRIMARY KEY (work_id, edition, page)
) WITHOUT ROWID;

-- The composite FK is what stops a box existing on a page nobody recorded
-- looking at. It costs nothing on the success path — `put_truth` already writes
-- the page row before its boxes, in one transaction — and nothing in index
-- overhead, since `truth_page`'s primary key already provides exactly the index
-- the FK needs. So this is not machinery for a hypothetical: it encodes an
-- invariant the write path has already committed to, into the layer that cannot
-- regress. The risk it closes is not only a future external writer but a
-- refactor *inside* `put_truth` that reorders the two inserts, which today would
-- fail silently. Same reasoning as `recursive_triggers`: a schema correct only
-- while every writer remembers something is lucky, not correct.
CREATE TABLE truth_box (
  id      INTEGER PRIMARY KEY,
  work_id INTEGER NOT NULL REFERENCES work(id) ON DELETE CASCADE,
  edition TEXT NOT NULL,
  page    INTEGER NOT NULL,
  x REAL NOT NULL, y REAL NOT NULL, w REAL NOT NULL, h REAL NOT NULL,
  axis    TEXT,
  kind    TEXT NOT NULL,
  ignore  INTEGER NOT NULL DEFAULT 0,
  text    TEXT,
  FOREIGN KEY (work_id, edition, page)
    REFERENCES truth_page (work_id, edition, page) ON DELETE CASCADE
);

-- **The archives the boxes were drawn against, stored rather than re-derived.**
-- `TruthEdition::members` (core/truth.rs:136) is provenance, and
-- `TruthEdition::is_stale_for` compares it element-wise against the digests of
-- the archives in hand now — the check `taiyaku score` and `score_corpus` both
-- refuse on, whose own doc calls stale ground truth "the sharpest form of this
-- problem in the project": it "draws a box on the wrong art on the RIGHT page
-- and then scores a detector against it."
--
-- An earlier draft of this schema had no such table and `state::truth`
-- projected `members` from the `member` rows that existed at read time. That
-- made `is_stale_for` compare a list against itself — it could never report
-- stale again — and an export after a reimport wrote a `taiyaku.truth.json`
-- asserting archives the labelling had never seen, over the reader's only copy.
-- Found in whole-branch review 2 by running it, not by reading. Same lesson as
-- `truth_page` directly above: a fact that only a document carries has to be
-- stored, because a projection of the present cannot reconstruct the past.
--
-- `ordinal` rather than a list, because member ORDER is page order and
-- `is_stale_for` compares position by position. Not a foreign key to `member`:
-- the whole point is that these digests outlive the rows they were copied from.
CREATE TABLE truth_member (
  work_id INTEGER NOT NULL REFERENCES work(id) ON DELETE CASCADE,
  edition TEXT NOT NULL,
  ordinal INTEGER NOT NULL,
  sha256  TEXT NOT NULL,
  PRIMARY KEY (work_id, edition, ordinal)
) WITHOUT ROWID;

-- **Which of a work's sibling documents have already been migrated.** One row
-- per work per document (`progress` | `notes` | `truth` | `library`), written
-- by `migrate.rs` on the first import that considers that document — whether
-- it applied a file, found live rows and left the file unread, or found no
-- file at all.
--
-- An earlier version of `migrate.rs` had no such record and inferred it from
-- content: "the database holds no note for this work" stood in for "this
-- work's notes have never been migrated". Those are different questions.
-- Delete your last note and the inference falls through, so the sibling file
-- is applied again and the note comes back — and `taiyaku serve <sidecar>...`
-- imports every path on every start, so it came back at every restart,
-- forever, with no way to make the deletion stick (`export --in-place` never
-- removes a sibling file it has nothing to write to). Whole-branch review 2,
-- C3. "Migrated once" and "currently empty" have to be distinguishable, and
-- only a stored fact can distinguish them.
--
-- A malformed file writes no row: it has not been migrated, and fixing it and
-- importing again is how a reader recovers.
CREATE TABLE migrated_document (
  work_id  INTEGER NOT NULL REFERENCES work(id) ON DELETE CASCADE,
  document TEXT NOT NULL,                  -- progress | notes | truth | library
  migrated TEXT NOT NULL DEFAULT (datetime('now')),
  PRIMARY KEY (work_id, document)
) WITHOUT ROWID;

-- Roots and search ----------------------------------------------------------

-- Written by spec B. The three edition columns are what this root's material
-- becomes by default; B's rules layer over them.
CREATE TABLE root (
  id                 INTEGER PRIMARY KEY,
  path               TEXT NOT NULL UNIQUE,
  added              TEXT NOT NULL DEFAULT (datetime('now')),
  scan_interval_secs INTEGER,
  last_scan_at       TEXT,
  last_scan_status   TEXT,
  exclude_globs      TEXT,
  edition_key        TEXT,
  default_kind       TEXT,
  default_language   TEXT
);

CREATE INDEX member_by_sha ON member (sha256);
CREATE INDEX note_by_work  ON note (work_id);
CREATE INDEX work_by_series ON work (series_id);

-- External-content FTS over the titles, so there is no second copy of the
-- strings to drift. Kept current by triggers.
CREATE VIRTUAL TABLE series_fts USING fts5 (
  title, content = 'series_title', content_rowid = 'rowid'
);
CREATE VIRTUAL TABLE work_fts USING fts5 (
  title, content = 'work', content_rowid = 'id'
);

CREATE TRIGGER series_title_ai AFTER INSERT ON series_title BEGIN
  INSERT INTO series_fts (rowid, title) VALUES (new.rowid, new.title);
END;
CREATE TRIGGER series_title_ad AFTER DELETE ON series_title BEGIN
  INSERT INTO series_fts (series_fts, rowid, title) VALUES ('delete', old.rowid, old.title);
END;
-- The update trigger is not boilerplate symmetry: without it, renaming a title
-- leaves series_fts matching the OLD title and missing the NEW one, silently.
-- An earlier draft of this schema omitted it while `work` had all three, and a
-- review reproduced the stale index. §3.5 of the spec claims these tables are
-- "kept current by triggers"; two of three triggers does not keep that promise.
CREATE TRIGGER series_title_au AFTER UPDATE ON series_title BEGIN
  INSERT INTO series_fts (series_fts, rowid, title) VALUES ('delete', old.rowid, old.title);
  INSERT INTO series_fts (rowid, title) VALUES (new.rowid, new.title);
END;
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

-- One implicit local user, so the server behaves exactly as it does today: no
-- login, no hashing, no guards. Spec A2 promotes THIS ROW rather than inserting
-- a new one — inserting would orphan every note and reading position recorded
-- between A and A2 on an account nobody can log in as, silently.
INSERT INTO user (id, name, role) VALUES (1, 'local', 'admin');
