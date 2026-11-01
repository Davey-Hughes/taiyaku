-- Migration 6 added `member.root_id` and left every existing row NULL, which is
-- wrong for a member a *scan* produced: it genuinely sits under a root, and is
-- NULL only because nothing has re-derived since the column appeared.
--
-- **The consequence is that the first grant blanks the library.** With every
-- member NULL, `member.root_id IN (…)` excludes all of them, so a grant gives
-- that reader a library of nothing.
--
-- **A migration rather than an edit to 006**, which the append-only rule
-- requires here and which 005's own exception does not cover: 006 has run on a
-- real database, so editing it would leave that one permanently un-backfilled
-- while new databases got the fix. This way a database that already ran 006 is
-- repaired the next time it is opened.
--
-- **Matched by path prefix, longest first.** `derive` writes a scanned
-- member's `path` absolute, from the root down, precisely so it resolves for
-- every reader of these rows — so the root is recoverable from the path with
-- no `scan_entry` join. `ORDER BY LENGTH(r.path) DESC` makes a nested root win
-- over the one above it, which is the same "no ancestor of it is one" rule
-- derivation applies to members.
--
-- An imported work's members keep NULL, which is correct and is what the
-- filter's "NULL excludes itself" rule is for: `taiyaku.json` member paths are
-- relative to the document and match no root, so the `LIKE` finds nothing.
UPDATE member
   SET root_id = (
     SELECT r.id FROM root r
      WHERE member.path = r.path
         OR member.path LIKE r.path || '/%'
      ORDER BY LENGTH(r.path) DESC
      LIMIT 1
   )
 WHERE root_id IS NULL;
