# Shorter book URLs (O282) and titles without the file extension (O284)

2026-11-14. A short design: a small change, one PR, Rust only.

## Why

The owner's idea, a nice-to-have rather than a pain: shorter book URLs, if
there is a clean way. The test library's are long because a slug is minted
once and every one there predates afb6e4ba (`berserk---v01--2003--cbz`;
1,896 works, average 42 characters, longest 108). While trying the O283
prototype the owner also asked that a title not show its file extension by
default (`Berserk - v01 (2003).cbz` → `Berserk - v01 (2003)`).

## Approaches considered

- **Series + volume** (picked by the owner): the slug is the series' name and
  the volume, `berserk-v01`. Short, and it still says which book it is.
- **Today's rule, re-made**: new books already get `berserk-v01-2003`; only
  re-make the old ones. Smallest, but the year and subtitles stay
  (`jojo-s-bizarre-adventure-colored-v092-qualifications-for-the-corpse-…`).
- **Leave it**: a store made fresh already gets today's shorter rule.
- Dropped before the question: `/read/<series>/v01` (needs series slugs, a
  second route shape, and the API and OPDS still name a work by one word),
  and `/read/1271` (short but says nothing).

## Decided by the owner (2026-11-14)

1. **A slug is the series and the volume**: `slug_token(<series name>)` +
   `-v` + the volume with at least two digits — `berserk-v01`,
   `one-piece-v85`, `one-piece-v105`, `death-note-colored-council-v01`. A
   fractional volume goes through the same character rule (`v10-5`). A book
   with no series or no volume keeps today's rule from its title. The
   character rule and the `-2`/`-3` suffixes are O262 decision 1's, unchanged.
2. **Existing books' slugs are re-made once** to the new rule.
3. **No redirect.** An old slug that no work now holds answers exactly as a
   slug no work holds, as the overlapping-libraries ruling already does for a
   deleted row's slug; one that has become another book's new slug opens
   that book. Bookmarks and any OPDS ids Panels stored for re-made books stop
   resolving, or name that other book.

## My calls (check these)

- **Still minted once.** After the re-make, a later edit to a book's series,
  volume or title does not change its slug, as now (import.rs:470-473): a
  book re-numbered by hand to v02 keeps `berserk-v01`.
- **The re-make runs by itself, once**, when the upgraded build first opens
  a store (a migration step), so there is nothing to remember at deploy. It
  goes in work-id order, so a `-2` lands on the later-added book, and it must
  cope with a new slug equal to another book's current one.
- **O284, the title:** a title *made from a file name* (derive's `title_of`
  fallback) drops one trailing archive extension, using import.rs's
  `ARCHIVE_EXTENSIONS`. A title from a work's `taiyaku.json` drops it too,
  where the import reads it, so a reimport keeps it off. A rule's captured
  title on a book a scan newly adds, or one typed by hand, shows as written;
  migration 23 cannot tell a stored capture from a stored file name and
  strips both. It is the server's derived title that
  changes, so the Series tiles, the reader, the continue strip, search and
  OPDS all agree; the web trims nothing. Existing books' stored derived
  titles are brought up in the same one-time pass, not left until a scan.
- "The series' name" is the name the Series page shows; the builder confirms
  which column that is.

## Must not happen

- A reading position or note lost or moved by the re-make. They key on
  `work_id`, so the pass writes only `work`'s slug and titles, and the
  counter and search index that follow `work`'s updates; the check proves it
  on a copy of the test library.
- A slug that changes on every later edit.

## Non-goals

Series slugs or `/series/<name>` URLs; a route of any other shape; redirects
or an alias table; the argv bridge's own old rule (`works::work_id`, which a
live registry never mints from).

## What proves it

On a copy of the test library after the pass: every slug unique and of the
new form (or the title fallback where there is no volume); progress and note
rows identical before and after (counts and work ids); `/read/berserk-v01`
opens Berserk v01; an old slug no work now holds answers like an unknown
one; OPDS hrefs and
ids carry the new slugs; no derived title ends in an archive extension, and
a hand-typed one that does is untouched.

## Left to the builder

Where the pass runs (an SQL migration cannot call `slugify`); whether the
mint site already holds the series name and volume (the series id and volume
are written after the import, by `derive::write_work`); the tests whose literal slugs and titles move;
the doc rows that state the old form (O262, O67, the specs' slug examples).
