-- One row per pair of files scored by content, keyed by the two digests with
-- the smaller first: a rename or a move keeps the pair, a file whose bytes
-- change is a new file, and a pair has one row whichever side was looked up.
-- Whether a pair joins is not stored, since derive decides it from `v`,
-- `state` and the rules each time. `runs` with the page counts and `floor`
-- is what derive writes an alignment from. `split_by` outlives its account as
-- NULL, so a Split stands when the admin who made it is deleted. `split_group`
-- is the pair a Split named, on every pair it set apart, so Undo restores
-- them together.
CREATE TABLE content_match (
  id          INTEGER PRIMARY KEY,
  sha_a       TEXT NOT NULL,
  sha_b       TEXT NOT NULL,
  v           REAL NOT NULL,
  long_paired INTEGER NOT NULL,
  a_slots     INTEGER NOT NULL,
  b_slots     INTEGER NOT NULL,
  a_pages     INTEGER NOT NULL,
  b_pages     INTEGER NOT NULL,
  floor       REAL,                       -- the calibration floor, where accepted
  runs        TEXT NOT NULL,              -- JSON [[a_start, a_end, b_start, b_end], ...], whole pages, a to b
  state       TEXT NOT NULL DEFAULT 'scored' CHECK (state IN ('scored', 'split')),
  split_by    INTEGER REFERENCES user(id) ON DELETE SET NULL,
  split_at    TEXT,
  split_group INTEGER,                    -- the pair the Split named, while split
  scored_at   TEXT NOT NULL DEFAULT (datetime('now')),
  CHECK (sha_a < sha_b),
  UNIQUE (sha_a, sha_b)
);
-- A file's pairs are found from either side; the UNIQUE index serves `sha_a`.
CREATE INDEX content_match_by_b ON content_match (sha_b);
-- Each digest the match pass has looked up, so a pass that stopped resumes
-- where it was and no file is looked up twice. The versions are recorded and
-- not acted on: nothing is scored again. `sha256` says NOT NULL because SQLite
-- lets a primary key that is not an INTEGER hold NULL.
CREATE TABLE match_lookup (
  sha256              TEXT PRIMARY KEY NOT NULL,
  fingerprint_version INTEGER NOT NULL,
  scorer_version      INTEGER NOT NULL,
  looked_at           TEXT NOT NULL DEFAULT (datetime('now'))
);
-- The pair that brought an edition onto its work by content: the Series
-- panel's mark, and how a later derive finds an edition a Split leaves on the
-- host. Every edition already stored was placed by its name, so each starts
-- NULL, and a pair that goes leaves its edition placed by name again.
ALTER TABLE edition ADD COLUMN joined_by INTEGER REFERENCES content_match(id) ON DELETE SET NULL;
