-- **Provenance for the two fields that are always present.** C3 §5 makes
-- `work.title` and `edition.label` editable, and O67 is the defect that asks
-- for it: a work spanning five editions is named after whichever file minted
-- its row, `.cbz` and `(2021)` included.
--
-- Both columns are `NOT NULL` and both are always populated, so a
-- `<stem>.library.json` that projected them directly would be non-empty for
-- every work in the library. `librarydoc::of(...).is_empty()` decides two
-- things that would then both be wrong: whether `migrate` thinks this work
-- already holds hand-fixed metadata, and whether `export::work` writes a
-- library document or removes a stale one. The second would put a
-- `taiyaku.library.json` beside every work in the corpus.
--
-- So the hand value gets a column of its own and the effective value keeps
-- its. `librarydoc::apply` writes both; everything that reads a title or a
-- label goes on reading `work.title` / `edition.label` unchanged, which is
-- why this is two `ALTER TABLE`s and no rewrite.
--
-- **Four columns, not two, and the reason is what `NOT NULL` costs.**
-- `work.title` and `edition.label` are `NOT NULL`, so the effective column
-- cannot be emptied — and the value derivation minted is destroyed by the
-- first hand edit that overwrites it, since nothing recomputes a title after
-- `import::insert_work`'s INSERT. Without somewhere to keep it, *clearing* a
-- hand-set title cannot restore anything: the row would keep displaying the
-- withdrawn hand value while `title_hand` read NULL, so `librarydoc::of`
-- would report no hand edit, `export::work` would delete the document
-- recording it, and a rescan of the same library would mint a title the
-- database disagrees with. Found by Task 2's review, 2026-08-28.
--
-- So the derived value gets a column of its own, and `apply` writes
-- `title = COALESCE(?, title_derived)`. Clearing then restores exactly what
-- derivation minted, and every reader goes on reading `work.title` unchanged.
--
-- **Additive, per the store spec §10's rule.** The backfill is exact rather
-- than approximate: O44 records that there has never been a route to apply a
-- hand edit after a work's first import, so every `title` in every existing
-- database *is* the derived one, and copying it across states a truth rather
-- than guessing one.
ALTER TABLE work    ADD COLUMN title_hand    TEXT;
ALTER TABLE work    ADD COLUMN title_derived TEXT;
ALTER TABLE edition ADD COLUMN label_hand    TEXT;
ALTER TABLE edition ADD COLUMN label_derived TEXT;

UPDATE work    SET title_derived = title;
UPDATE edition SET label_derived = label;
