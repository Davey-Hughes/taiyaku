-- Spec 2026-11-02-overlapping-libraries-design.md §5: a directory may be in
-- several libraries, so which libraries hold it is a link table, and
-- `root.library_id` goes. Each value the column held becomes one link.
--
-- **The triggers that wrote or read the column go first**, since DROP COLUMN
-- refuses a column a trigger names. `root_gets_a_library` is not remade: a
-- trigger cannot tell a directory added with no library from one whose link
-- is about to be inserted, so `scan::add_root_in` names a library of its own
-- and links it in the same transaction.
--
-- **A library never outlives its last link.** `library_emptied_by_unlink`
-- and `library_emptied_by_move` delete a library left with no directory, and
-- its grants go with it by `user_library`'s CASCADE.
--
-- **A directory keeps at least one library.** `root_keeps_a_library` refuses
-- to delete a directory's last link, whether by an unlink or by deleting its
-- library, unless the directory itself is being deleted: its row is gone by
-- the time its links cascade, so the check passes. A directory inserted with
-- no link cannot be refused here, since nothing fires at the insert that
-- knows a link will follow; every writer inserts the directory and its link in
-- one transaction. Nor is an `UPDATE` of a link's `root_id` refused, which
-- would leave the directory it named with no link: no writer makes one, and a
-- move updates the link's `library_id`. The message is a literal, as `RAISE`
-- takes no expression on the SQLite this ships with.
CREATE TABLE library_root (
  library_id INTEGER NOT NULL REFERENCES library(id) ON DELETE CASCADE,
  root_id    INTEGER NOT NULL REFERENCES root(id) ON DELETE CASCADE,
  PRIMARY KEY (library_id, root_id)
) WITHOUT ROWID;
CREATE INDEX library_root_by_root ON library_root (root_id, library_id);

INSERT INTO library_root (library_id, root_id)
SELECT library_id, id FROM root WHERE library_id IS NOT NULL;

DROP TRIGGER root_gets_a_library;
DROP TRIGGER library_emptied_by_delete;
DROP TRIGGER library_emptied_by_move;
ALTER TABLE root DROP COLUMN library_id;

CREATE TRIGGER library_emptied_by_unlink AFTER DELETE ON library_root
WHEN NOT EXISTS (SELECT 1 FROM library_root WHERE library_id = old.library_id)
BEGIN
  DELETE FROM library WHERE id = old.library_id;
END;
CREATE TRIGGER library_emptied_by_move AFTER UPDATE OF library_id ON library_root
WHEN old.library_id IS NOT new.library_id
 AND NOT EXISTS (SELECT 1 FROM library_root WHERE library_id = old.library_id)
BEGIN
  DELETE FROM library WHERE id = old.library_id;
END;
CREATE TRIGGER root_keeps_a_library BEFORE DELETE ON library_root
WHEN EXISTS (SELECT 1 FROM root WHERE id = old.root_id)
 AND NOT EXISTS (SELECT 1 FROM library_root
                  WHERE root_id = old.root_id AND library_id <> old.library_id)
BEGIN
  SELECT RAISE(ABORT, 'a directory keeps at least one library');
END;
