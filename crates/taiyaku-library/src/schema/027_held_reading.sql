-- The reading the walk replaced when it found an entry changed, kept until a
-- derive not frozen for the entry's directory drops it (`derive::drop_held`,
-- `derive::digest`). A paused directory's derive reads an entry by it, so a
-- changed file keeps the reading its works were written from (O320). The walk
-- writes the four together, so a digest is never paired with another
-- reading's pages. NULL where nothing is held.
ALTER TABLE scan_entry ADD COLUMN held_sha256 TEXT;
ALTER TABLE scan_entry ADD COLUMN held_format TEXT;
ALTER TABLE scan_entry ADD COLUMN held_pages INTEGER;
ALTER TABLE scan_entry ADD COLUMN held_title TEXT;

-- No walk before this version held anything, yet one may have cleared a
-- changed file's digest, and a stamp or a derive cancelled while writing may
-- have left an entry on a digest its member does not hold; frozen, it would
-- wait and its work be rewritten without it. So each entry a member names,
-- through a directory of that member, holds the member's digest and pages
-- where its own differ, in every directory, since a cancel can pause one
-- later. Where two members name one path, the latest written is the one its
-- works hold. No older format or title is stored, so those are the entry's.
UPDATE scan_entry
   SET held_sha256 = m.sha256,
       held_format = scan_entry.format,
       held_pages = m.pages,
       held_title = scan_entry.title
  FROM (SELECT e.id AS entry_id, max(m.id) AS member_id
          FROM member m
          JOIN member_root mr ON mr.member_id = m.id
          JOIN root r ON r.id = mr.root_id
          JOIN scan_entry e
            ON e.root_id = r.id AND e.rel_path = substr(m.path, length(r.path) + 2)
         WHERE substr(m.path, 1, length(r.path) + 1) = r.path || '/'
         GROUP BY e.id) AS named
  JOIN member m ON m.id = named.member_id
 WHERE scan_entry.id = named.entry_id
   AND scan_entry.held_sha256 IS NULL
   AND scan_entry.sha256 IS NOT m.sha256;
