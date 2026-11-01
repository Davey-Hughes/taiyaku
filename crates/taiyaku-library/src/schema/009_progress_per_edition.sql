-- **A reading position per edition, not per work.** O73's fold — accepting an
-- `edition-of` proposal and rescanning — moves a volume's material onto
-- another work, and the reader rows hanging off the work it left have to move
-- with it. `UPDATE progress SET work_id = <target>` is refused by the old key
-- whenever the reader held a position on both works, which is the ordinary
-- case rather than the corner (`docs/notes.md`).
--
-- **What the widening also fixes, which is older than O73.** Under the old key
-- a work holds one position per reader, so a reader at slot 50 of one edition
-- who opened a sibling edition and turned one page lost the first position,
-- silently. After this they do not. A test that pins the old behaviour is
-- pinning a defect.
--
-- **Nothing is dropped or merged by the copy.** Every row already satisfies
-- the wider key, the narrow one being a prefix of it.
--
-- `completed_at` and `read_count` come across and are per edition, which is
-- what 001 describes them as: "finished once and now re-reading" is a fact
-- about a rendition, not about a volume.
--
-- **Nothing REFERENCES `progress`** — `grep -rn 'REFERENCES progress' schema/`
-- is empty — so unlike migration 4's rebuild of `user` there is no child to
-- lose. `without_foreign_keys` still wraps this and `pragma_foreign_key_check`
-- still runs before the version row, because the rebuild drops `progress`'s
-- *own* foreign keys onto `user` and `work` and re-declares them.
--
-- **There is no index to restore.** 005's note about `DROP TABLE` taking a
-- table's indexes with it is why that is said out loud rather than left to be
-- noticed: `progress` has none, its primary key being the only access path.
CREATE TABLE progress_new (
  user_id      INTEGER NOT NULL REFERENCES user(id) ON DELETE CASCADE,
  work_id      INTEGER NOT NULL REFERENCES work(id) ON DELETE CASCADE,
  edition      TEXT NOT NULL,
  slot         INTEGER NOT NULL,
  updated      TEXT NOT NULL DEFAULT (datetime('now')),
  completed_at TEXT,
  read_count   INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (user_id, work_id, edition)
) WITHOUT ROWID;

INSERT INTO progress_new (user_id, work_id, edition, slot, updated, completed_at, read_count)
SELECT user_id, work_id, edition, slot, updated, completed_at, read_count FROM progress;

DROP TABLE progress;
ALTER TABLE progress_new RENAME TO progress;
