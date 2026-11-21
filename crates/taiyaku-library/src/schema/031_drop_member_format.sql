-- `member.format` goes (O69). No writer sets it: `import::replace_editions`,
-- the one insert into `member`, names no format, so the column is NULL in a
-- store taiyaku wrote.
--
-- No trigger, view or index names the column, so DROP COLUMN takes it in
-- place: every other column, row, index and trigger of `member` stands.
ALTER TABLE member DROP COLUMN format;
