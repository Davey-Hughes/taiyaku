-- The fact layer -------------------------------------------------------------
--
-- What is on disk, and nothing about what it means. `derive.rs` is the only
-- module that reads these tables for structure; this one is written by a walk
-- that decides nothing (§2).
--
-- Persisted rather than streamed, because that is what buys the property a
-- direct scanner cannot have: editing a rule re-derives the whole library
-- without touching the filesystem. `pages` and `sha256` are on this table for
-- the same reason — a re-derive that had to reopen every archive to recount
-- and rehash would be a rescan wearing a different name.
CREATE TABLE scan_entry (
  id         INTEGER PRIMARY KEY,
  root_id    INTEGER NOT NULL REFERENCES root(id) ON DELETE CASCADE,
  rel_path   TEXT NOT NULL,
  kind       TEXT NOT NULL,              -- file | dir
  format     TEXT,                       -- zip | dir | epub-fixed | epub-reflow | pdf | rar | 7z | tar
  size       INTEGER NOT NULL,
  mtime      INTEGER NOT NULL,
  pages      INTEGER,
  sha256     TEXT,
  first_seen TEXT NOT NULL DEFAULT (datetime('now')),
  last_seen  TEXT NOT NULL DEFAULT (datetime('now')),
  UNIQUE (root_id, rel_path)
);

-- Per-directory state, for §8's cheap walk: an interval scan compares these
-- two numbers before doing anything expensive in a directory.
--
-- **It does not license skipping the descent.** A directory's mtime moves when
-- an entry is added, removed or renamed *in it*, and not when a file's content
-- changes or when something changes two levels down — so a walk that declined
-- to recurse into an unchanged directory would miss every deep change. What
-- these numbers save is opening containers, which is the expensive part; see
-- `scan.rs`'s doc comment.
CREATE TABLE dir_state (
  root_id     INTEGER NOT NULL REFERENCES root(id) ON DELETE CASCADE,
  rel_path    TEXT NOT NULL,
  mtime       INTEGER NOT NULL,
  entry_count INTEGER NOT NULL,
  last_walked TEXT NOT NULL DEFAULT (datetime('now')),
  PRIMARY KEY (root_id, rel_path)
) WITHOUT ROWID;

CREATE INDEX scan_entry_by_root ON scan_entry (root_id);

-- The rule layer -------------------------------------------------------------
--
-- Ordered layering, per field: every matching rule applies in `ordinal` order,
-- and a later rule overrides only the fields it actually sets (§4). So "this
-- whole root is raw" and "except these are the colour edition" are two rules
-- rather than four. First-match-wins was rejected because every exception
-- would have to restate the general case; most-specific-wins was rejected
-- because the engine would then be making a judgement the user cannot see or
-- reorder.
--
-- `root_id` NULL means every root.
--
-- **Which fields a rule sets is not all in the `set_` columns.** A `regex`
-- rule also sets whatever its named captures provide — `series`, `volume`,
-- `chapter`, `edition`, `title` — and `builtin-volume` / `builtin-chapter` set
-- exactly one field each, from `taiyaku_read::volume_number` and
-- `chapter_number`. The `set_` columns are the literal overrides, which is why
-- there is no `set_volume`: a literal volume number is a property of one file,
-- not of a rule that matches many.
CREATE TABLE rule (
  id              INTEGER PRIMARY KEY,
  ordinal         INTEGER NOT NULL,
  enabled         INTEGER NOT NULL DEFAULT 1,
  root_id         INTEGER REFERENCES root(id) ON DELETE CASCADE,
  kind            TEXT NOT NULL,          -- prefix | regex | builtin-volume | builtin-chapter
  pattern         TEXT NOT NULL,
  match_on        TEXT NOT NULL,          -- path | name | parent
  set_edition_key TEXT,
  set_kind        TEXT,
  set_language    TEXT,
  set_series      TEXT,
  note            TEXT
);

-- Proposals ------------------------------------------------------------------
--
-- The judgements §1's measurement says no rule should make. Suffix-stripping as
-- an edition signal is barely better than a coin flip, so this table proposes
-- and nothing merges.
--
-- `subject_id` keys on the SERIES ROW rather than on anything content-derived.
-- A dismissed proposal keyed on a content hash returns as a brand-new proposal
-- the moment a volume is added, and the dismissal silently stops meaning
-- anything. This is also why `derive.rs` reconciles series by `sort_title`
-- instead of deleting and re-inserting them: re-minting a series id would
-- orphan every dismissal that pointed at it, which is the same defect arriving
-- by a different road.
CREATE TABLE proposal (
  id              INTEGER PRIMARY KEY,
  kind            TEXT NOT NULL,          -- edition-of | same-series | attach-chapters | align
  subject_id      INTEGER NOT NULL REFERENCES series(id) ON DELETE CASCADE,
  target_id       INTEGER REFERENCES series(id) ON DELETE CASCADE,
  score           REAL NOT NULL,
  reason          TEXT NOT NULL,
  state           TEXT NOT NULL DEFAULT 'open',   -- open | accepted | dismissed
  created         TEXT NOT NULL DEFAULT (datetime('now')),
  -- §6.1: accepting a proposal writes a narrow `rule` rather than writing
  -- structure directly, so derivation keeps exactly one input. This column is
  -- what makes that reversible — un-accepting deletes the rule and reopens the
  -- proposal. ON DELETE SET NULL rather than CASCADE: the proposal outlives
  -- its rule, and `proposals::reconcile` reopens any accepted row whose rule
  -- has gone, so a rule deleted through C4's editor cannot leave a proposal
  -- claiming an acceptance that no longer decides anything.
  created_rule_id INTEGER REFERENCES rule(id) ON DELETE SET NULL
);

-- **An expression index, not a plain UNIQUE.** SQLite treats NULLs as distinct
-- in a UNIQUE constraint, so `UNIQUE (kind, subject_id, target_id)` would let
-- every rescan insert a second `attach-chapters` row — target NULL — for a
-- directory somebody already dismissed. §6's argument is that dismissal has to
-- persist or the inbox becomes noise and then becomes ignored, and an
-- INSERT OR IGNORE is only idempotent against a constraint that actually
-- matches.
CREATE UNIQUE INDEX proposal_identity
  ON proposal (kind, subject_id, ifnull(target_id, -1));

-- The defaults, as ordinary editable rows (§4) -------------------------------
--
-- Series defaults to the PARENT DIRECTORY rather than to a filename capture,
-- because a filename does not always repeat the series and a directory always
-- names it. Rule 4 layers a filename capture over it where one exists, which
-- is the layering model doing its job on its own defaults.
INSERT INTO rule (ordinal, kind, pattern, match_on, note) VALUES
  (1, 'regex',           '^(?<series>[^/]+)/', 'path', 'series from the first directory under the root'),
  (2, 'builtin-volume',  '',                   'name', 'volume from the name, via taiyaku_read::volume_number'),
  (3, 'builtin-chapter', '',                   'name', 'chapter from the name, via taiyaku_read::chapter_number'),
  (4, 'regex',           '^(?<series>.+?)[ _-]+v', 'name', 'series from the filename where it repeats it');
