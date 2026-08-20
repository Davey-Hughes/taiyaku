-- `note.id` was a global `TEXT PRIMARY KEY` while notes became per-user in
-- spec A2, so two readers could not hold the same id.
--
-- `state::put_notes` deletes `WHERE work_id = ?1 AND user_id = ?2` and
-- re-inserts against the global key, and the write is whole-document — so a
-- second reader submitting a document containing an id the first already held
-- had their *entire* notes document rejected, not one note. `notes::validate`
-- could not see it either: it checks for ids repeated within the submitted
-- document via a local `HashSet`, which cannot know about another user's rows.
-- The 200-versus-500 also told a caller whether an id existed anywhere in the
-- library.
--
-- Reachable without anybody acting oddly: one reader imports a
-- `taiyaku.notes.json` another exported, or two accounts receive the same
-- sidecar. Found by the whole-branch review, 2026-08-18.
--
-- **The key becomes `(user_id, work_id, id)`.**
--
-- `(user_id, id)` was the first answer and was still too wide by one column:
-- `state::put_notes` deletes `WHERE work_id = ?1 AND user_id = ?2` and then
-- inserts, so the same reader holding that id on a *different* work hit the key
-- and lost their whole document to the same 500 this migration exists to
-- remove — reachable by copying a `taiyaku.notes.json` between volumes, or
-- importing one sidecar against two works.
--
-- The scope a note id is actually unique in is the scope `put_notes` replaces:
-- one reader's notes on one work. `notes::validate` cannot narrow it either —
-- its duplicate check is a `HashSet` over the submitted document, which is word
-- for word the limitation this file cites about the old global key, and an
-- earlier draft of this comment wrongly claimed it "already refuses" the case.
-- It cannot see another work's rows any more than it could see another user's.
--
-- **Amended rather than superseded by a migration 6**, which the append-only
-- rule would otherwise require: 005 has never been merged, so no database has
-- ever applied it and there is nothing for an edit to diverge from. That is the
-- narrow exception the rule leaves — a migration is append-only once it can
-- have run somewhere, and this one cannot have.

-- **Nothing REFERENCES `note`**, so unlike migration 4's rebuild of `user` there
-- is no `ON DELETE CASCADE` to fire and no child to lose. Checked rather than
-- assumed: `grep -rn 'REFERENCES note' schema/` is empty. `without_foreign_keys`
-- still wraps this, and `pragma_foreign_key_check` still runs before the version
-- row, because the rebuild drops `note`'s *own* foreign keys onto `user` and
-- `work` and re-declares them — and a typo there is exactly what that check is
-- for.
CREATE TABLE note_new (
  user_id INTEGER NOT NULL REFERENCES user(id) ON DELETE CASCADE,
  id      TEXT NOT NULL,
  work_id INTEGER NOT NULL REFERENCES work(id) ON DELETE CASCADE,
  edition TEXT NOT NULL,
  slot    INTEGER NOT NULL,
  rx      REAL, ry REAL, rw REAL, rh REAL,
  color   TEXT,
  body    TEXT NOT NULL,
  created TEXT NOT NULL,
  updated TEXT NOT NULL,
  PRIMARY KEY (user_id, work_id, id)
);

INSERT INTO note_new (user_id, id, work_id, edition, slot, rx, ry, rw, rh, color, body, created, updated)
SELECT user_id, id, work_id, edition, slot, rx, ry, rw, rh, color, body, created, updated
FROM note;

DROP TABLE note;
ALTER TABLE note_new RENAME TO note;

-- **`note_by_work` has to come back.** `DROP TABLE` takes a table's indexes with
-- it, and nothing says so — a rebuild that forgot this would leave every notes
-- read scanning, correct and slower, with no test able to see the difference.
-- `001_initial.sql` is where it was created and this is the only other place it
-- exists.
CREATE INDEX note_by_work ON note (work_id);
