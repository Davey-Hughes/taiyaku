-- O206: a library's name is stripped of white space at both ends, not of
-- spaces only, and two names that differ only in ASCII case are one name.
--
-- **The characters stripped are Unicode's White_Space**, the set Rust's
-- `str::trim` strips in `libraries::trim_name`, written below as the code
-- points of each call to `char`. Migration 15 stripped with SQLite's
-- one-argument `trim`, which takes spaces and nothing else, so a tab or a
-- newline at the end of a name stayed in it. `libraries/tests.rs` checks every
-- list here against the set Rust strips.
--
-- **Case is ASCII case**, which is what `COLLATE NOCASE` folds: `Manga` and
-- `MANGA` are one name, and either finds the library called the other. Letters
-- outside ASCII are compared as they are.
--
-- **The names already stored are brought under both rules first**, since the
-- index cannot be made over two names it would refuse. A name that is an older
-- library's once both are stripped and folded keeps its library's id beside
-- it, `Manga (7)`, and the older library keeps its name. Every other name is
-- stripped, in that order, because stripping first would put `' Manga '` on
-- `'Manga'` under `library.name`'s own UNIQUE. A name that is only white space
-- is left as it is: stripped, it would be no name. A store where a name this
-- makes is already taken, `Manga (7)` beside the library `manga` whose id is
-- 7, fails the migration and is left as it was.
UPDATE library
   SET name = trim(name, char(9, 10, 11, 12, 13, 32, 133, 160, 5760, 8192, 8193, 8194, 8195, 8196, 8197, 8198, 8199, 8200, 8201, 8202, 8232, 8233, 8239, 8287, 12288)) || ' (' || id || ')'
 WHERE trim(name, char(9, 10, 11, 12, 13, 32, 133, 160, 5760, 8192, 8193, 8194, 8195, 8196, 8197, 8198, 8199, 8200, 8201, 8202, 8232, 8233, 8239, 8287, 12288)) <> ''
   AND EXISTS (SELECT 1 FROM library older
                WHERE older.id < library.id
                  AND trim(older.name, char(9, 10, 11, 12, 13, 32, 133, 160, 5760, 8192, 8193, 8194, 8195, 8196, 8197, 8198, 8199, 8200, 8201, 8202, 8232, 8233, 8239, 8287, 12288)) = trim(library.name, char(9, 10, 11, 12, 13, 32, 133, 160, 5760, 8192, 8193, 8194, 8195, 8196, 8197, 8198, 8199, 8200, 8201, 8202, 8232, 8233, 8239, 8287, 12288)) COLLATE NOCASE);

UPDATE library
   SET name = trim(name, char(9, 10, 11, 12, 13, 32, 133, 160, 5760, 8192, 8193, 8194, 8195, 8196, 8197, 8198, 8199, 8200, 8201, 8202, 8232, 8233, 8239, 8287, 12288))
 WHERE name <> trim(name, char(9, 10, 11, 12, 13, 32, 133, 160, 5760, 8192, 8193, 8194, 8195, 8196, 8197, 8198, 8199, 8200, 8201, 8202, 8232, 8233, 8239, 8287, 12288))
   AND trim(name, char(9, 10, 11, 12, 13, 32, 133, 160, 5760, 8192, 8193, 8194, 8195, 8196, 8197, 8198, 8199, 8200, 8201, 8202, 8232, 8233, 8239, 8287, 12288)) <> '';

CREATE UNIQUE INDEX library_name_folded ON library (name COLLATE NOCASE);

-- `root_gets_a_library` asks both rules of the name it gives a folder, or its
-- insert would fail the index as a store fault. As migration 15 made it
-- otherwise: the folder's last path component, or its whole path where the
-- component is empty or taken. `last_insert_rowid()` inside a trigger is the
-- row the trigger inserted, and reverts to the caller's when it ends.
DROP TRIGGER root_gets_a_library;
CREATE TRIGGER root_gets_a_library AFTER INSERT ON root
WHEN new.library_id IS NULL
BEGIN
  INSERT INTO library (name)
  SELECT CASE WHEN leaf = '' OR EXISTS (SELECT 1 FROM library WHERE name = leaf COLLATE NOCASE)
              THEN trim(new.path, char(9, 10, 11, 12, 13, 32, 133, 160, 5760, 8192, 8193, 8194, 8195, 8196, 8197, 8198, 8199, 8200, 8201, 8202, 8232, 8233, 8239, 8287, 12288)) ELSE leaf END
    FROM (SELECT trim(substr(new.path,
                             length(rtrim(new.path, replace(new.path, '/', ''))) + 1),
                      char(9, 10, 11, 12, 13, 32, 133, 160, 5760, 8192, 8193, 8194, 8195, 8196, 8197, 8198, 8199, 8200, 8201, 8202, 8232, 8233, 8239, 8287, 12288)) AS leaf);
  UPDATE root SET library_id = last_insert_rowid() WHERE id = new.id;
END;
