-- The title a file declares inside itself: a reflowable EPUB's `dc:title`,
-- which `derive.rs` names a work by before its file name. The file name of a
-- book is a person's or a shop's, often with the punctuation a filesystem
-- refuses replaced (`Sorcerer_s Stone`); the package says what the book is
-- called. NULL for every other format, and for a book that declares none.
ALTER TABLE scan_entry ADD COLUMN title TEXT;

-- A book already scanned was read for its blocks and not for its title, and a
-- scan opens a file again only once its size or mtime moves. So each is set to
-- have moved: the next scan reads it whole, and finds the same bytes.
UPDATE scan_entry SET mtime = -1 WHERE format = 'epub-reflow';
