# Content matching PR 2: fingerprints in the server

Date: 2026-11-17. Status: draft for the owner.

Parent design: `2026-11-15-edition-matching-by-content-design.md`, §1
("Fingerprints at scan time"), §7 (cost) and Delivery item 2. This document
settles what that design left open for PR 2 and nothing beyond it.

## Why

From PR 3 on, editions are matched by page content, and every page in a
library needs a fingerprint before any match can be looked up. On the NAS
that first pass is long: PR 1 measured it at 22.18 h at 4 threads under
`nice 19` (`docs/notes.md`, "Edition matching by content: PR 1's
measurement (2026-11-16)"). PR 2 builds the pass and its storage, so the
fingerprints exist by the time matching ships. It makes no match and changes
nothing a reader sees.

## Goals

- Every page of every readable file in a library with matching on is
  fingerprinted once, and stored under the file's content digest, so a renamed
  or copied file costs nothing.
- The fingerprints are bit for bit the ones PR 1 measured (the CLI's
  `describe_pages`: one border crop per file, a 16×16 dhash of each page, and
  the two halves of a page wider than it is tall), because PR 1's bars were set
  on them.
- The pass runs in the background beside scans at low priority, and never
  makes a scan, rule save, stamp, Move or Remove wait or refuse.
- An admin sees it in the scan feed, with a step and a count, and can cancel
  it.
- Interrupted by Cancel, a restart or a crash, it resumes without reading a
  stored file again.
- Matching is a per-library switch, on by default (parent §7).

## Non-goals

- No lookup, vote, alignment, match, proposal or "matched by content" mark:
  those are PR 3 onward.
- No divider detection. A template divider page shows only against other
  files, which is PR 3's lookup; PR 1's low-information test is a bit count
  that PR 3 applies to the stored hashes.
- No new formats. Reading RAR archives and JXL pages is another session's
  work, so the pass handles them gracefully instead: it covers what the
  server reads today and picks the rest up when the reader learns them
  (Design 9). 7z, tar, PDF and fixed-layout EPUB stay unread; reflowable
  EPUBs are books, not pages.
- The CLI's own fingerprint cache and its experiments do not change.

## Anti-goals

- The library gets slower to use or to scan while the pass runs.
- A fingerprint stored under a digest it does not belong to, because the file
  changed while it was read.
- A pass that starts again from nothing after an interruption.
- A RAR file or JXL page left unfingerprinted for good once the reader learns
  to read it.

## Constraints

- The NAS runs the server in a container with `/mnt/user` mounted read-only;
  nothing is written inside a library directory.
- SQLite 3.40.1 in CI and the image.
- `taiyaku-read` carries no image codec into a shipped binary (its
  `Cargo.toml`), so the decode does not move there.
- The `paused-frozen` branch adds migration 027 and edits the scan job
  runner; whichever of the two merges second renumbers its migration.

## Approaches

- **A phase at the end of the scan job.** Rejected: a scan job holds the
  store-wide library job lock and reserves its directories until it ends, so
  a pass of about a day would refuse rule saves, stamps, Move, Remove and Scan
  for that long.
- **Fingerprinting during the walk.** Rejected for the same lock, and it would
  make every first scan as slow as the pass.
- **A separate background pass. Chosen.** It has its own entry in the scan
  feed and takes no job lock.

## Owner ruling (2026-11-17)

Cancel stops the current pass only. The next scan of a library with matching
on, scheduled or pressed, starts a pass that carries on from where the last one
stopped. Turning a library's switch off stops the pass for that library. A
directory paused by a scan's Cancel gets no fingerprinting until Scan is
pressed for it.

## Design (my calls, unless under the ruling above)

1. **When a pass runs.** One starts when a scan job ends without being
   cancelled, whether or not it found anything and whichever library it
   scanned, once at startup if anything is due, and when an admin turns a
   library's switch on. The scheduler's startup
   scan covers only directories whose schedule is not Off, and a restart (a
   deploy, say) is not a choice to stop, so the pass starts at startup by
   itself. A Cancel is therefore not remembered across a restart. Under the
   ruling above, a directory on Automatic is scanned again within the
   unwatched interval if nothing watches it, or a day if something does
   (`watch.rs`), and that scan resumes a cancelled pass. One pass at a time across the store. A scan that ends while a pass
   runs marks it to look again for new files before it ends.
2. **What a pass covers.** The unit is a member as the derive would claim one
   (`derive.rs`'s claim rule: a zip-family file, or a folder of pages with no
   member above it), whether or not it makes a work, so PR 3 finds a
   fingerprint under every member's digest and a series folder or the root is
   never read as one file. Units that make no work today, chapter-only
   directories, are covered: PR 5 matches them. A unit is due when it is in a
   directory that belongs to at least one library with matching on and is not
   paused, and its digest has no stored fingerprint. A rule edit that moves a
   claim makes the new unit due, and the old row goes at the end of a pass.
   Each digest is read once however many entries share it. Newest entries
   first, so a volume added today is done before the backlog. An entry with no
   digest yet is hashed by the pass and the digest written back onto
   `scan_entry` only where it is still empty, as the derive does. A folder
   with no page the reader reads is not due: its digest is the digest of no
   pages, which every such folder shares. Directories in no library are not
   covered, so the NAS's held-out `Chapters`, `Issues` and `Extras` are
   fingerprinted when they are added to one.
3. **Storage.** A new table keyed by digest: the algorithm's version, the
   outcome (fingerprinted, or unreadable with a reason), the page count, and
   one blob of per-page records (width, height, the 256-bit hash, and the two
   halves' hashes where the page is wider than tall). One row per file, written
   in its own short transaction once the file is read, so a pass resumes at a
   file. A file stored as unreadable is not tried again until the version or
   what the reader can read changes (Design 9). At the end of a pass, rows
   whose digest no claimed unit holds (Design 2) are dropped. A new version makes every row stale and due again.
4. **A file that changes while it is read.** Its size and modification time
   are compared with its `scan_entry` before and after the read; on a
   mismatch the result is dropped and the next scan records the change.
5. **Where the code goes.** The decode, crop, dhash and halves move out of
   `taiyaku-cli` into a crate the server and the CLI both use, and the CLI
   calls the moved code. A test pins the moved code's output to the hashes the
   CLI produced before the move, on fixture pages. The table's SQL lives in
   `taiyaku-library` beside the schema; the pass lives in `taiyaku-serve`.
6. **Priority.** Four worker threads, the setting PR 1 timed, each lowered to
   `nice 19` and to the idle I/O class. Between files the pass waits while a
   scan job holds the library job lock, and while a page or book has been
   served to a reader in the last 30 seconds, so neither a scan nor a reader
   shares the disk with it. A worker samples a file's pages for its border,
   then reads and hashes one page at a time, so it holds about one page in
   memory beside the sample; the hashes stay PR 1's because the border comes
   from the same sample.
7. **In the feed.** The pass is its own kind of job in
   `GET /api/library/scans`, with the step "Fingerprinting pages", one
   store-wide count of files done out of files due, and a Cancel. Admins see it
   on the Libraries list and on the page of each library with matching on, and
   a Cancel from any of them stops the pass. Readers see nothing, since nothing
   they see changes. The pass holds no directory, so nothing the feed holds
   back while a scan runs (Move, Remove, a removal's re-read) waits on it.
8. **The switch.** A library gains a "match by content" flag, on by default
   for existing and new libraries, accepted by `PATCH
   /api/library/libraries/{id}` and shown as a checkbox on the library's edit
   page. A directory in two libraries is covered if either has it on. Turning
   it off stops a running pass from starting new files that only that library
   reaches, and keeps the fingerprints already stored.

9. **Formats and pages the server cannot read yet.** Another session is
   teaching the reader RAR archives and JXL pages, so nothing here may treat
   them as unreadable for good. A file in a format the reader does not open is
   not due at all (the pass asks `taiyaku-read` which formats it opens), so a
   RAR file becomes due the day the reader opens it, with no row to clear. A
   file the reader opens but cannot read whole is stored with what it could
   read and marked partial, or marked unreadable, together with the server's
   build: a page that fails to decode (an AVIF page today), or an entry that
   is neither a page nor a known companion file such as `ComicInfo.xml` (a
   JXL page today). When the server's build changes, only partial and
   unreadable rows are due again, never the whole library. Keying the retry on
   the build rather than on a list of formats catches a decoder that learns a
   format as well as a reader that learns a page type, and errs towards a
   cheap extra retry: a file with an odd companion file is read again once per
   new build. Neither a partial nor an unreadable file stops the pass, and the
   feed's count includes them.

## Left to the builder

- The table's and columns' names, the blob layout and the migration number.
- How the job kind is spelt in the feed's JSON and the web types.
- Which crate the moved fingerprint code lands in.
- What a page that fails to decode inside an otherwise readable file stores
  (PR 1's `describe_pages` behaviour is the starting point).
- The wording of the step and the checkbox. The owner tries both on the test
  server before the PR.

## How it is checked

- The pinned-output test above, and the CLI's existing fingerprint tests,
  unchanged.
- Resume: a pass cancelled part way, then started again, reads no file it
  stored.
- The lock: a rule save, stamp, Move and Remove succeed while a pass runs.
- The guard: a file rewritten during its read stores nothing.
- Formats: a file stored partial or unreadable is due again after the build
  changes, and a file stored whole is not; a RAR file is not due while the
  reader does not open RAR.
- Startup: with every directory's interval Off, a restart resumes a pass that
  has files due.
- A hand-check on a scratch copy of the test library (`manga-test`): the
  feed shows the pass, Cancel stops it, and a scan resumes it.
