-- Migration 6 added `member.root_id` and left every existing row NULL. That
-- comment claimed NULL "is exactly true of an imported work … and of every
-- member written before this migration", and the second half of that is wrong:
-- a member a *scan* produced genuinely sits under a root, and is NULL only
-- because nothing has re-derived since the column appeared.
--
-- **The consequence is that the first grant blanks the library.** With every
-- member NULL, `member.root_id IN (…)` excludes all of them, so
-- `taiyaku library user grant-root davey /shelf` gives that reader a library
-- of nothing — while the command prints "restricted to that one root". Found
-- by the whole-branch review, and reproduced against a real library on
-- 2026-08-20: 21 members, all NULL, 0 works visible to a granted account.
--
-- **A migration rather than an edit to 006**, which the append-only rule
-- requires here and which 005's own exception does not cover: 006 has already
-- been applied to a real database, so editing it would leave that one
-- permanently un-backfilled while new databases got the fix. This way the
-- library that hit the bug is repaired the next time it is opened.
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
