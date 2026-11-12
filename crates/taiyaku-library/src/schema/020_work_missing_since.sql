-- A work none of whose members any directory still holds is marked rather
-- than deleted, and every listing hides it until a scan finds a member again
-- (pre-deploy spec §5). NULL is a present work; `derive::mark_missing` is the
-- one writer.
ALTER TABLE work ADD COLUMN missing_since TEXT;

-- `derive::mark_missing` and the grant ask of every member whether a directory
-- it is under holds an entry of its digest, or an entry at its path that the
-- walk did not find empty (`browse::held_by`). The UNIQUE (root_id, rel_path)
-- index answers the path; without this one, `scan_entry_by_root` answers the
-- digest by reading every entry of the directory.
CREATE INDEX scan_entry_by_digest ON scan_entry (root_id, sha256);
