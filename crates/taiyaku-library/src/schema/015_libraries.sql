-- Part F1 of docs/superpowers/specs/2026-10-05-libraries-design.md (§2.1): a
-- library is a name and one or more folders, and a folder is the `root` row it
-- always was.
--
-- **One library per existing root**, spec decision 1. A work that gathers
-- editions from two roots today is split by the next derive: the library the
-- work is not filed under gets a work of its own. The decision accepts the
-- split and says why. Each library takes its root's id, so `root.library_id`
-- is `root.id` and `user_root`'s rows move across unchanged.
--
-- **A library is named by `browse::status`'s rule, stripped of spaces**: the
-- folder's last path component, or the whole path where two roots share one.
-- `rtrim(path, <every character of the path except '/'>)` leaves the path up to
-- and including its last '/', and what follows is the component. Names go
-- through the one-argument `trim`, which strips spaces from both ends and
-- nothing else, as `libraries::trim_name` strips a name in Rust. Components
-- are compared once stripped. A path whose component is empty once stripped
-- — `/` itself, or a folder named only spaces — is named by the whole path.
-- Where two whole paths would be the same once stripped (they differ only in
-- surrounding spaces), both keep their paths as they are, spaces and all,
-- where the stripped ones would fail the migration on `library.name`.
--
-- **`work` is rebuilt**, because SQLite cannot drop a column's UNIQUE in
-- place. `content_key` was unique across the store and is now unique within a
-- library (spec decision 2), so a byte-identical volume in two libraries is
-- two works. A work in no library is held to nothing by it: SQLite takes each
-- NULL `library_id` as distinct. Written key first, so the index the
-- constraint builds is also the one `import::find_by_key` searches. `slug`
-- stays unique across the store: `/api/works/{slug}` and every OPDS link name
-- a work by slug alone. The copy keeps every id, and the index and six
-- triggers `DROP TABLE work` takes with it are recreated below: migration
-- 001's three that feed `work_fts`, and migration 014's three that move the
-- structure counter. `work_fts` reads `work` by id as an external-content
-- table, so its index stays valid.
--
-- **A work is filed under its hub edition's library.** `hand::path_for`
-- addresses a work's document off the hub edition's own root, so the work stays
-- with its document. A hub with no member under a root falls back to the work's
-- lowest root; a work with no member under any root — an imported
-- `taiyaku.json` — stays NULL, in no library, which is how `browse` already
-- treats it.
--
-- **A series is filed where most of its works are**, the lowest library id on
-- a tie. A series none of whose works is in a library stays NULL, and
-- `derive::reconcile_series` may file it later, under a library whose files
-- name it.
--
-- **No UNIQUE on `(library_id, sort_title)`.** `sort_title` has never been
-- unique (`derive::reconcile_series` says why that is safe), and a database
-- holding two series of one title would fail this migration rather than open.
--
-- **`ON DELETE SET NULL`, not CASCADE, on `work` and `series`.** A library goes
-- only with its last root (the triggers at the end). Where that root is
-- removed, `scan::apply_removal` has by then deleted every work the root alone
-- accounted for and counted what went with them, and a work still carrying the
-- library has material elsewhere: an imported edition, or an edition under a
-- folder of another library. Where the root is moved to another library, the
-- works under it still carry the old one, and `SET NULL` leaves them in no
-- library. CASCADE would delete either, uncounted.
--
-- **`root.library_id` stays nullable**, because SQLite cannot add a NOT NULL
-- column without a default to a table that has rows. It is never NULL after an
-- insert: `root_gets_a_library` files a root inserted without a library under a
-- library of its own. That library is named for the folder's last path
-- component, or by its whole path when the component is empty or already a
-- library's name, each stripped of spaces as above. Unlike the backfill, the
-- trigger leaves the library already holding the short name as it is.
--
-- **A library never outlives its last root.** The two `library_emptied_*`
-- triggers delete it when its last root is deleted or moved away, and its
-- grants go with it through `user_library`'s CASCADE. A grant naming a library
-- with no root would resolve to no roots, and `Principal.roots` reads no roots
-- as unrestricted.
--
-- **A library's id is never given to another library** (`AUTOINCREMENT`), so
-- whatever names a library by id cannot come to name a different one once
-- that library is deleted. The backfill's ids, taken from the roots, count:
-- the next library is numbered after the highest of them.
CREATE TABLE library (
  id    INTEGER PRIMARY KEY AUTOINCREMENT,
  name  TEXT NOT NULL UNIQUE,
  added TEXT NOT NULL DEFAULT (datetime('now'))
);

WITH named AS (
  SELECT id, path, added,
         trim(substr(path, length(rtrim(path, replace(path, '/', ''))) + 1)) AS leaf
    FROM root
)
INSERT INTO library (id, name, added)
SELECT id,
       CASE WHEN leaf = ''
              OR (SELECT count(*) FROM named other WHERE other.leaf = named.leaf) > 1
            THEN CASE WHEN (SELECT count(*) FROM named other
                             WHERE trim(other.path) = trim(named.path)) > 1
                      THEN path ELSE trim(path) END
            ELSE leaf END,
       added
  FROM named;

ALTER TABLE root ADD COLUMN library_id INTEGER REFERENCES library(id);
UPDATE root SET library_id = id;

ALTER TABLE series ADD COLUMN library_id INTEGER REFERENCES library(id) ON DELETE SET NULL;

CREATE TABLE work_new (
  id             INTEGER PRIMARY KEY,
  content_key    TEXT NOT NULL,
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
  series_id_hand INTEGER REFERENCES series(id) ON DELETE SET NULL,
  library_id     INTEGER REFERENCES library(id) ON DELETE SET NULL,
  UNIQUE (content_key, library_id)
);
INSERT INTO work_new (id, content_key, slug, series_id, volume, title, reading, hub,
                      sidecar_path, sidecar_mtime, imported, title_hand, title_derived,
                      volume_hand, series_id_hand, library_id)
SELECT w.id, w.content_key, w.slug, w.series_id, w.volume, w.title, w.reading, w.hub,
       w.sidecar_path, w.sidecar_mtime, w.imported, w.title_hand, w.title_derived,
       w.volume_hand, w.series_id_hand,
       COALESCE(
         (SELECT r.library_id
            FROM edition e
            JOIN member m ON m.edition_id = e.id
            JOIN root r ON r.id = m.root_id
           WHERE e.work_id = w.id AND e.key = w.hub
           ORDER BY m.ordinal
           LIMIT 1),
         (SELECT r.library_id
            FROM edition e
            JOIN member m ON m.edition_id = e.id
            JOIN root r ON r.id = m.root_id
           WHERE e.work_id = w.id
           ORDER BY r.id
           LIMIT 1))
  FROM work w;
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

UPDATE series SET library_id = (
  SELECT w.library_id
    FROM work w
   WHERE w.series_id = series.id AND w.library_id IS NOT NULL
   GROUP BY w.library_id
   ORDER BY count(*) DESC, w.library_id
   LIMIT 1
);

CREATE TABLE user_library (
  user_id    INTEGER NOT NULL REFERENCES user(id) ON DELETE CASCADE,
  library_id INTEGER NOT NULL REFERENCES library(id) ON DELETE CASCADE,
  PRIMARY KEY (user_id, library_id)
) WITHOUT ROWID;
INSERT INTO user_library (user_id, library_id)
SELECT ur.user_id, r.library_id FROM user_root ur JOIN root r ON r.id = ur.root_id;
DROP TABLE user_root;

-- `last_insert_rowid()` inside a trigger is the row the trigger inserted, and
-- reverts to the caller's when the trigger ends, so `scan::add_root_in`'s own
-- `last_insert_rowid()` after `INSERT INTO root` is still the root's id.
CREATE TRIGGER root_gets_a_library AFTER INSERT ON root
WHEN new.library_id IS NULL
BEGIN
  INSERT INTO library (name)
  SELECT CASE WHEN leaf = '' OR EXISTS (SELECT 1 FROM library WHERE name = leaf)
              THEN trim(new.path) ELSE leaf END
    FROM (SELECT trim(substr(new.path,
                             length(rtrim(new.path, replace(new.path, '/', ''))) + 1)) AS leaf);
  UPDATE root SET library_id = last_insert_rowid() WHERE id = new.id;
END;

CREATE TRIGGER library_emptied_by_delete AFTER DELETE ON root
WHEN old.library_id IS NOT NULL
 AND NOT EXISTS (SELECT 1 FROM root WHERE library_id = old.library_id)
BEGIN
  DELETE FROM library WHERE id = old.library_id;
END;
CREATE TRIGGER library_emptied_by_move AFTER UPDATE OF library_id ON root
WHEN old.library_id IS NOT NULL
 AND old.library_id IS NOT new.library_id
 AND NOT EXISTS (SELECT 1 FROM root WHERE library_id = old.library_id)
BEGIN
  DELETE FROM library WHERE id = old.library_id;
END;

-- **A library that is an account's only grant keeps its last root.** The two
-- `only_grant_refuses_*` triggers abort the delete of a root, or a change of
-- its library, where it is its library's last root and some account is
-- granted that library and no other. Each runs before its row is deleted or
-- changed, while that row's library and its grants are still there to read.
-- Their message is a literal and names no account; `scan::removal_refusal`
-- asks the same question for a removal and names each one.
CREATE TRIGGER only_grant_refuses_delete BEFORE DELETE ON root
WHEN old.library_id IS NOT NULL
 AND NOT EXISTS (SELECT 1 FROM root WHERE library_id = old.library_id AND id <> old.id)
 AND EXISTS (SELECT 1 FROM user_library ul
              WHERE ul.library_id = old.library_id
                AND NOT EXISTS (SELECT 1 FROM user_library held
                                 WHERE held.user_id = ul.user_id
                                   AND held.library_id <> ul.library_id))
BEGIN
  SELECT RAISE(ABORT, 'this is the last folder of a library that is the only library an account is granted; grant that account another library, or revoke this one, first');
END;
CREATE TRIGGER only_grant_refuses_move BEFORE UPDATE OF library_id ON root
WHEN old.library_id IS NOT NULL
 AND old.library_id IS NOT new.library_id
 AND NOT EXISTS (SELECT 1 FROM root WHERE library_id = old.library_id AND id <> old.id)
 AND EXISTS (SELECT 1 FROM user_library ul
              WHERE ul.library_id = old.library_id
                AND NOT EXISTS (SELECT 1 FROM user_library held
                                 WHERE held.user_id = ul.user_id
                                   AND held.library_id <> ul.library_id))
BEGIN
  SELECT RAISE(ABORT, 'this is the last folder of a library that is the only library an account is granted; grant that account another library, or revoke this one, first');
END;
