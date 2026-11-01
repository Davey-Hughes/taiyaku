-- Spec C1 §13's tokeniser question.
--
-- **The default tokeniser emits each whole CJK run as a single token.** So a
-- substring of a run, `花嫁` in `五等分の花嫁`, matches nothing, and the only
-- Japanese query that can ever succeed is the entire run typed exactly. That is
-- not a tuning problem; it is the index having no smaller unit to offer.
--
-- `trigram` indexes every three-character window instead, which makes any
-- substring of three characters or more findable in either script and costs
-- the query under three characters, which matches nothing at all.
-- `browse::search` answers those with `LIKE` rather than leaving them silent;
-- see its `TRIGRAM_MIN`.
--
-- **DROP and CREATE rather than ALTER**, because `tokenize=` is fixed at
-- creation. The triggers are untouched by this: they live on `series_title`
-- and `work`, not on the virtual tables, so they survive the drop and keep
-- pointing at the new tables by name.
--
-- **`'rebuild'` rather than a hand-written INSERT SELECT.** These are
-- external-content tables, so `rebuild` re-reads the content table through
-- the same `content_rowid` mapping the triggers use; an INSERT SELECT would
-- be a second, hand-maintained copy of that mapping.
DROP TABLE series_fts;
DROP TABLE work_fts;

CREATE VIRTUAL TABLE series_fts USING fts5 (
  title, content = 'series_title', content_rowid = 'rowid', tokenize = 'trigram'
);
CREATE VIRTUAL TABLE work_fts USING fts5 (
  title, content = 'work', content_rowid = 'id', tokenize = 'trigram'
);

INSERT INTO series_fts (series_fts) VALUES ('rebuild');
INSERT INTO work_fts (work_fts) VALUES ('rebuild');
