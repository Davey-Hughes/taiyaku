-- O244: a reading position and a note name the page they stand on, beside
-- the slot, so a change to the edition's members or spread flags does not
-- move them to another page, unless an earlier copy of the same archive in
-- the edition goes and the copies renumber. `at_sha256` is the member's
-- digest, `at_nth` which of the edition's members with that digest it is
-- (from 0), `at_page` the page within it, and `at_half` the half of a
-- spread, NULL for a whole page. `state::` writes all four for every
-- position it writes and every note it adds or moves, and resolves them on
-- every read; `slot` stays the answer for a row whose page is gone.
--
-- **The first three are all set or all NULL.** NULL means the row named no
-- page when it was written (past the end), or was written before this
-- migration and `state::` has not anchored it since. `state::at_columns` is
-- the one place the four are made, and a stamp re-points only the digest and
-- occurrence of an anchor already set, which is what keeps that rule: a
-- CHECK across columns needs the table rebuilt, and the fold copies the four
-- as they are.
--
-- ADD COLUMN rebuilds neither table, so `note_by_work` stands.
ALTER TABLE progress ADD COLUMN at_sha256 TEXT;
ALTER TABLE progress ADD COLUMN at_nth INTEGER;
ALTER TABLE progress ADD COLUMN at_page INTEGER;
ALTER TABLE progress ADD COLUMN at_half TEXT CHECK (at_half IN ('left', 'right'));
ALTER TABLE note ADD COLUMN at_sha256 TEXT;
ALTER TABLE note ADD COLUMN at_nth INTEGER;
ALTER TABLE note ADD COLUMN at_page INTEGER;
ALTER TABLE note ADD COLUMN at_half TEXT CHECK (at_half IN ('left', 'right'));
