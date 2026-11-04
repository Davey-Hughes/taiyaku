-- Spec 2026-11-02-overlapping-libraries-design.md §3.1: a library no longer
-- decides what a work or a series is, so `work` and `series` lose
-- `library_id`, and a work's content key is unique across the store again.
--
-- **Copies are reduced to the oldest row, not folded**, by the user's ruling
-- of 2026-11-02 that no store holds data worth carrying. Of each content key
-- the lowest-id work is kept, and of each `sort_title` the lowest-id series,
-- with every work re-pointed to it. The rest goes with everything that names
-- it, each table by name, because a migration runs with foreign keys off and
-- no cascade fires. The next scan writes the kept rows again from the files.
--
-- **`work` is rebuilt**, because `library_id` is part of a UNIQUE that DROP
-- COLUMN refuses. The copy keeps every id, and the index and six triggers
-- `DROP TABLE work` takes are recreated as they stood: 001's three that feed
-- `work_fts`, and 014's three that move the structure counter. `series`
-- loses its column in place.
--
-- **`member_root` is every directory holding a copy of a member**, and it is
-- what a grant, a library page and a removal read. `member.root_id` stays as
-- the directory of the copy `member.path` names, which a document's address,
-- `derive::superseded`'s key and the scan-entry joins need. The two triggers
-- keep that directory in the set, whoever writes the column. CASCADE on both
-- sides: a member's copies go with it, and a removed directory stops holding
-- anything, and neither deletes a member.
CREATE TEMP TABLE kept_series AS
SELECT s.id AS id, (SELECT min(o.id) FROM series o WHERE o.sort_title = s.sort_title) AS kept
  FROM series s;
CREATE TEMP TABLE gone_series AS SELECT id FROM kept_series WHERE id <> kept;

UPDATE work SET series_id = (SELECT kept FROM kept_series k WHERE k.id = work.series_id)
 WHERE series_id IN (SELECT id FROM gone_series);
UPDATE work SET series_id_hand = (SELECT kept FROM kept_series k WHERE k.id = work.series_id_hand)
 WHERE series_id_hand IN (SELECT id FROM gone_series);

DELETE FROM series_title    WHERE series_id IN (SELECT id FROM gone_series);
DELETE FROM series_tag      WHERE series_id IN (SELECT id FROM gone_series);
DELETE FROM external_id     WHERE series_id IN (SELECT id FROM gone_series);
DELETE FROM collection_item WHERE series_id IN (SELECT id FROM gone_series);
DELETE FROM credit          WHERE series_id IN (SELECT id FROM gone_series);
DELETE FROM proposal        WHERE subject_id IN (SELECT id FROM gone_series)
                               OR target_id IN (SELECT id FROM gone_series);
DELETE FROM series          WHERE id IN (SELECT id FROM gone_series);

CREATE TEMP TABLE gone_work AS
SELECT w.id FROM work w
 WHERE EXISTS (SELECT 1 FROM work o WHERE o.content_key = w.content_key AND o.id < w.id);
CREATE TEMP TABLE gone_edition AS
SELECT id FROM edition WHERE work_id IN (SELECT id FROM gone_work);

DELETE FROM run             WHERE alignment_id IN (SELECT id FROM alignment
                                                    WHERE work_id IN (SELECT id FROM gone_work));
DELETE FROM alignment       WHERE work_id IN (SELECT id FROM gone_work);
DELETE FROM truth_box       WHERE work_id IN (SELECT id FROM gone_work);
DELETE FROM truth_page      WHERE work_id IN (SELECT id FROM gone_work);
DELETE FROM truth_member    WHERE work_id IN (SELECT id FROM gone_work);
DELETE FROM progress        WHERE work_id IN (SELECT id FROM gone_work);
DELETE FROM note            WHERE work_id IN (SELECT id FROM gone_work);
DELETE FROM credit          WHERE work_id IN (SELECT id FROM gone_work)
                               OR edition_id IN (SELECT id FROM gone_edition);
DELETE FROM page_flag       WHERE edition_id IN (SELECT id FROM gone_edition);
DELETE FROM member          WHERE edition_id IN (SELECT id FROM gone_edition);
DELETE FROM edition         WHERE id IN (SELECT id FROM gone_edition);
DELETE FROM work_tag        WHERE work_id IN (SELECT id FROM gone_work);
DELETE FROM collection_item WHERE work_id IN (SELECT id FROM gone_work);
DELETE FROM migrated_document WHERE work_id IN (SELECT id FROM gone_work);
UPDATE series SET cover_work_id = NULL WHERE cover_work_id IN (SELECT id FROM gone_work);
DELETE FROM work            WHERE id IN (SELECT id FROM gone_work);

DROP TABLE gone_edition;
DROP TABLE gone_work;
DROP TABLE gone_series;
DROP TABLE kept_series;

CREATE TABLE work_new (
  id             INTEGER PRIMARY KEY,
  content_key    TEXT NOT NULL UNIQUE,
  slug           TEXT NOT NULL UNIQUE,
  series_id      INTEGER REFERENCES series(id) ON DELETE SET NULL,
  volume         REAL,
  title          TEXT NOT NULL,
  reading        TEXT NOT NULL,
  hub            TEXT NOT NULL,
  sidecar_path   TEXT,
  sidecar_mtime  INTEGER,
  imported       TEXT NOT NULL DEFAULT (datetime('now')),
  title_hand     TEXT,
  title_derived  TEXT,
  volume_hand    REAL,
  series_id_hand INTEGER REFERENCES series(id) ON DELETE SET NULL
);
INSERT INTO work_new (id, content_key, slug, series_id, volume, title, reading, hub,
                      sidecar_path, sidecar_mtime, imported, title_hand, title_derived,
                      volume_hand, series_id_hand)
SELECT id, content_key, slug, series_id, volume, title, reading, hub,
       sidecar_path, sidecar_mtime, imported, title_hand, title_derived,
       volume_hand, series_id_hand
  FROM work;
DROP TABLE work;
ALTER TABLE work_new RENAME TO work;

CREATE INDEX work_by_series ON work (series_id);
CREATE TRIGGER work_ai AFTER INSERT ON work BEGIN
  INSERT INTO work_fts (rowid, title) VALUES (new.id, new.title);
END;
CREATE TRIGGER work_ad AFTER DELETE ON work BEGIN
  INSERT INTO work_fts (work_fts, rowid, title) VALUES ('delete', old.id, old.title);
END;
CREATE TRIGGER work_au AFTER UPDATE ON work BEGIN
  INSERT INTO work_fts (work_fts, rowid, title) VALUES ('delete', old.id, old.title);
  INSERT INTO work_fts (rowid, title) VALUES (new.id, new.title);
END;
CREATE TRIGGER structure_work_ins AFTER INSERT ON work BEGIN
  UPDATE structure_version SET n = n + 1;
END;
CREATE TRIGGER structure_work_upd AFTER UPDATE ON work BEGIN
  UPDATE structure_version SET n = n + 1;
END;
CREATE TRIGGER structure_work_del AFTER DELETE ON work BEGIN
  UPDATE structure_version SET n = n + 1;
END;

ALTER TABLE series DROP COLUMN library_id;

CREATE TABLE member_root (
  member_id INTEGER NOT NULL REFERENCES member(id) ON DELETE CASCADE,
  root_id   INTEGER NOT NULL REFERENCES root(id) ON DELETE CASCADE,
  PRIMARY KEY (member_id, root_id)
) WITHOUT ROWID;
CREATE INDEX member_root_by_root ON member_root (root_id, member_id);

INSERT INTO member_root (member_id, root_id)
SELECT id, root_id FROM member WHERE root_id IS NOT NULL;

CREATE TRIGGER member_root_on_insert AFTER INSERT ON member
WHEN new.root_id IS NOT NULL
BEGIN
  INSERT OR IGNORE INTO member_root (member_id, root_id) VALUES (new.id, new.root_id);
END;
CREATE TRIGGER member_root_on_update AFTER UPDATE OF root_id ON member
WHEN new.root_id IS NOT old.root_id
BEGIN
  DELETE FROM member_root WHERE member_id = old.id AND root_id IS old.root_id;
  INSERT OR IGNORE INTO member_root (member_id, root_id)
  SELECT new.id, new.root_id WHERE new.root_id IS NOT NULL;
END;
