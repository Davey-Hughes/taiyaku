-- Content matching's per-library switch. On by default, for each library the
-- store already holds and each one made later, so every library is matched by
-- content unless an admin turns it off.
ALTER TABLE library ADD COLUMN match_by_content INTEGER NOT NULL DEFAULT 1;

-- One row per file's content, keyed by its digest rather than its path, so a
-- rename or a copy of a file already read costs nothing. A row whose `version`
-- is not the layout this build writes is read again. `prints` holds one record
-- per page, in reading order. `build` is the server build that read the row: a
-- `partial` or `unreadable` row is read again by any other build, which may
-- decode what this one could not. `sha256` says NOT NULL because SQLite lets
-- a primary key that is not an INTEGER hold NULL.
CREATE TABLE fingerprint (
  sha256  TEXT PRIMARY KEY NOT NULL,
  version INTEGER NOT NULL,
  outcome TEXT NOT NULL CHECK (outcome IN ('whole', 'partial', 'unreadable')),
  reason  TEXT,
  build   TEXT,
  pages   INTEGER NOT NULL,
  prints  BLOB NOT NULL,
  read_at TEXT NOT NULL DEFAULT (datetime('now'))
);
