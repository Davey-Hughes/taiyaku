-- O240: a stamp gives an archive a new digest and keeps every page byte for
-- byte, and `stamp::rekey_anchors` moves each stored anchor on it to the new
-- digest. A tab or device that read the work before the stamp still names
-- the old one, so a save from it would name a page no member has. Each row
-- remembers one `(digest, occurrence)` a stamp moved, `was_sha256` and
-- `was_nth`, and the pair it moved to, `now_sha256` and `now_nth`, so the
-- save can be forwarded to the page it was written on. `stamp::remember_moves`
-- writes them in the stamp's own transaction and collapses a chain as it
-- does, so one lookup reaches the member as it is now.
--
-- **Keyed by work and edition, not by digest alone.** A digest can stand in
-- two works, and a stamp rewrites one work's archives only. And an
-- occurrence counts the members of one edition: a copy the stamp did not
-- rewrite takes a new `nth` when a copy before it leaves the digest.
--
-- A work's removal, or a fold that takes it, takes its aliases (`ON DELETE
-- CASCADE`): they forward to anchors on that work's members and mean nothing
-- without them.
CREATE TABLE stamp_alias (
  work_id    INTEGER NOT NULL REFERENCES work(id) ON DELETE CASCADE,
  edition    TEXT    NOT NULL,
  was_sha256 TEXT    NOT NULL,
  was_nth    INTEGER NOT NULL,
  now_sha256 TEXT    NOT NULL,
  now_nth    INTEGER NOT NULL,
  PRIMARY KEY (work_id, edition, was_sha256, was_nth)
) WITHOUT ROWID;
