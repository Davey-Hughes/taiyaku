-- A note on a book can mark a run of its block's text, a highlight: from
-- `ts` up to `te`, counted as a browser counts a string, in UTF-16 code
-- units. Both NULL for a note on the whole block, and for every note on a
-- page.
ALTER TABLE note ADD COLUMN ts INTEGER;
ALTER TABLE note ADD COLUMN te INTEGER;

-- A link a person pinned between a book's hub and another of its editions:
-- the hub's block `hub_block` and the edition's block `block` start one link,
-- and `taiyaku-serve` aligns the stretches between pins on their own. One pin
-- a hub block and edition; a work's removal or a fold that takes it takes
-- its pins, which name its blocks and nothing else's.
CREATE TABLE book_pin (
  work_id   INTEGER NOT NULL REFERENCES work(id) ON DELETE CASCADE,
  edition   TEXT    NOT NULL,
  hub_block INTEGER NOT NULL,
  block     INTEGER NOT NULL,
  PRIMARY KEY (work_id, edition, hub_block)
) WITHOUT ROWID;
