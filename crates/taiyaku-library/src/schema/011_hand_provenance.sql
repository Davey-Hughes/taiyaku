-- **O80: the four columns a hand edit withdrew from derivation, given the
-- provenance migration 010 gave `title` and `label`.**
--
-- 010 fixed this asymmetry for two fields and stopped: `work.volume`,
-- `work.series_id`, `edition.language` and `edition.kind` stayed plain columns,
-- whatever the last derive wrote. A document that projected them would claim
-- derivation's own answers as though a person had chosen them.
--
-- **One column each, where 010 needed two.** 010 added `title_derived`
-- because nothing recomputes a title after `import::insert_work`'s INSERT, so
-- the derived value was destroyed by the first hand edit and clearing had
-- nothing to restore. These four are recomputed by `derive` on every pass
-- from the group, so the derived value is never lost: the effective column
-- can simply be written `COALESCE(<hand>, <what this pass derived>)`, and
-- clearing a hand value restores derivation's answer on the next scan.
--
-- **The guard goes away with it.** `write_work`'s `absorbed(…, library)` check
-- keeps one job: "this document has been read back from disk, do not read it
-- again". Whether to overwrite these four columns is answered per field, by
-- whether a hand value exists.
--
-- --- The backfill, which is exact where it can be and lossless where it
-- --- cannot ---
--
-- An already-armed marker records that *a* document was absorbed, not what was
-- in it, so this migration cannot tell a genuine hand-set volume from a
-- title-only Save. Two steps together mean it does not have to:
--
--   1. Copy the current values into the hand columns for every armed work.
--      Nothing is lost, including for a work whose document never reached
--      disk (O82's read-only case) or whose address has moved (O81) — those
--      values live only in the database and this is the only chance to keep
--      them.
--   2. Clear the marker, so the next derive re-reads each document through
--      `hand::load` and `librarydoc::apply` writes all four hand columns from
--      it — **NULL for every field the document does not carry**, which
--      unfreezes exactly the columns O80 froze.
--
-- So a work whose document survives ends up with the truth its document
-- records, and a work whose document is gone keeps what the database holds.
-- Neither case loses a correction, and the common case — a title-only Save —
-- is corrected on the first scan after this migration.
-- **The series is two facts, and they needed separating before either could
-- have provenance.** `LibraryDoc.series` carries a work's *attachment* (which
-- series it belongs to) and the *series' own* titles and reading direction —
-- series-level data replicated into every member work's document. An `of` that
-- read every `series_title` row, derivation's included, would make a scanned
-- work's document claim the derived series title as it would the derived
-- volume (O53).
--
-- So `series_title` gains `hand` and `series` gains `reading_hand`, and the
-- attachment gains an explicit fact of its own in the document
-- (`SeriesInfo::chosen`) rather than being implied by the block's presence.
-- Without that last part a document carrying hand-set *titles* would re-assert
-- a *derived* attachment as hand-chosen on the next apply, which is the
-- freeze this migration exists to end, moved rather than fixed.
--
-- `apply_series` still resolves by title text, deliberately: a document
-- arriving at a database that has never seen this library has nothing else to
-- resolve against, which is the import path spec A's round trip depends on.
-- What changes is only whether that resolution is recorded as a person's
-- choice.
ALTER TABLE work    ADD COLUMN volume_hand    REAL;
ALTER TABLE work    ADD COLUMN series_id_hand INTEGER REFERENCES series(id) ON DELETE SET NULL;
ALTER TABLE edition ADD COLUMN language_hand  TEXT;
ALTER TABLE edition ADD COLUMN kind_hand      TEXT;
ALTER TABLE series_title ADD COLUMN hand INTEGER NOT NULL DEFAULT 0;
ALTER TABLE series       ADD COLUMN reading_hand TEXT;

UPDATE work
   SET volume_hand    = volume,
       series_id_hand = series_id
 WHERE id IN (SELECT work_id FROM migrated_document
               WHERE document = 'library' AND absorbed = 1);

UPDATE edition
   SET language_hand = language,
       kind_hand     = kind
 WHERE work_id IN (SELECT work_id FROM migrated_document
                    WHERE document = 'library' AND absorbed = 1);

-- The series half of the same backfill: a series whose works were armed keeps
-- everything it holds, and the next derive replaces it with what the document
-- actually says wherever one survives.
UPDATE series_title
   SET hand = 1
 WHERE series_id IN (SELECT series_id FROM work
                      WHERE series_id IS NOT NULL
                        AND id IN (SELECT work_id FROM migrated_document
                                    WHERE document = 'library' AND absorbed = 1));

UPDATE series
   SET reading_hand = reading
 WHERE id IN (SELECT series_id FROM work
               WHERE series_id IS NOT NULL
                 AND id IN (SELECT work_id FROM migrated_document
                             WHERE document = 'library' AND absorbed = 1));

UPDATE migrated_document SET absorbed = 0 WHERE document = 'library';
