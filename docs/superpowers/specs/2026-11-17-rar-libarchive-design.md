# Reading RAR archives through libarchive, behind a build feature

Date: 2026-11-17. Status: approved by the owner, 2026-11-17.
Branch: `rar-libarchive`. Reopens the decision recorded in O90 and in §1 of
`2026-09-15-o90-unreadable-attribution-design.md`.

## Why

taiyaku refuses RAR. O90 closed that way because taiyaku is
GPL-3.0-or-later and RARLAB's UnRAR source carries a restriction the GPL
forbids adding. Every RAR in a library is counted unreadable, and the owner
has to repack it to zip by hand before it can be read. RAR archives are
routinely misnamed `.cbz`, so the owner usually finds them only when a scan
says so.

The owner's aim (2026-11-17) is for taiyaku to read more formats as they are,
without anyone converting files first. For RAR that means a decoder whose licence fits.
It should be easy to swap that decoder for another later, and to leave RAR
out of a build entirely.

O90 §1 weighed three Rust crates (`unrar`, `unrar-rs`, `rar`) and no C
library. libarchive was not considered, though `docs/notes.md` ("Practical:
7-Zip cannot extract these RARs, libarchive can") already records it as the
implementation that reads these files. The condition O90 named for
reopening, a library where repacking is not available, is not what reopens
it now: the owner chose to.

## Goals

- A build with the feature reads RAR4 and RAR5 archives everywhere a zip is
  read today: scan page counts, served pages, covers and resized pages,
  OPDS, downloads, fingerprints.
- A build without the feature behaves exactly as main does now: RAR is
  refused and counted under `rar`.
- The RAR decoder sits behind one internal interface. Replacing libarchive
  with another backend (the `rars` crate, say) means adding one file and
  changing one feature name.
- Moving a library between builds, with and without the feature or from one
  backend to another, never moves a reader's saved place.

## Non-goals

- Writing into RAR archives. ComicInfo stamping stays zip-only.
- 7z and tar, though libarchive could read both.
- Encrypted RAR entries: they are skipped, as zip skips them today.
  *Ruled otherwise on 2026-11-17: encrypted archives are not a concern for
  this project, nothing detects an encrypted entry, and one is listed as a
  page (O381).* *PR 2, on the owner's ruling P2-R4 of 2026-11-19: taiyaku's
  own walk of the block headers finds an encrypted entry, and the archive
  is refused at the count (O381, closed).*
- Multi-volume sets (`.part1.rar` and its siblings). *PR 2 refuses a volume
  of a set, RAR4 or RAR5, at the count (ruling P2-R6).*
- A cache for solid archives (see Decisions).
- Making a plain `cargo build` need libarchive. (CI and the local gate do
  need it: see CI and the local gate.)

## Anti-goals

These would be failures even if RAR files opened:

- A reader's saved page changing because the build or backend changed.
- A RAR counted with fewer pages than it holds, for instance because names
  outside ASCII were dropped under the container's C locale.
- RAR code that no CI job compiles or tests.
- Any code derived from UnRAR in the build.

## What exists on main (9e50c44f)

- `taiyaku_read::sniff` recognises RAR from its bytes. `Format::is_readable`
  refuses it, and its doc comment is the O90 record
  (`crates/taiyaku-read/src/format.rs`).
- Every page read goes through `open_archive`
  (`crates/taiyaku-read/src/pages.rs:346`). It refuses RAR in
  `unsupported_container` and otherwise opens a `zip::ZipArchive`. The public
  functions built on it are typed on zip: `member_page_names`,
  `member_page`, `archive_page`, `member_pages`, `member_page_sizes`,
  `member_page_entries`, `for_each_member_page`, `member_entry`,
  `read_edition`, `read_members_in_order`. `member_digest` hashes the whole
  file and needs no change.
- The rules every container follows live in `pages.rs`: an encrypted or
  undecodable entry is skipped so page numbers do not shift
  (`zip_can_read`); `__MACOSX` and dotfiles are dropped and names sorted
  naturally (`page_names_from`); capacity hints are clamped
  (`MAX_ENTRY_CAPACITY_HINT`); `for_each_member_page` reports the bytes it
  actually read; `member_page_entries` gives each page's crc32 up front,
  which a generated CBZ download needs (`crates/taiyaku-serve/src/cbz.rs`).
- Nothing keeps an archive open between requests. Each page request reopens
  the file and lists it again (`crates/taiyaku-serve/src/source.rs`,
  `LocalLibrary::page`).
- A scan reopens a file only when its size or mtime moved
  (`unchanged`, `crates/taiyaku-library/src/scan.rs:3295`), and counts pages
  only for zip and reflowable EPUB (`pages_of`). The derive admits formats
  by a token list in `derive.rs`, which
  `the_token_side_readability_test_agrees_with_the_enums_own` keeps in step
  with the enum.
- The workspace forbids `unsafe` (`Cargo.toml`, `unsafe_code = "forbid"`).
- The runtime image installs only `libsqlite3-0` (`Dockerfile`). CI's Rust
  jobs run in `rust:1-bookworm` and install no `-dev` packages
  (`.forgejo/workflows/ci.yml`, `mutants.yml`). `scripts/check-all.sh`
  builds on the host.
- The licence page bundles its texts by hand (`web/src/licenceTexts.ts`).

## What the owner's library holds

A read-only census of every archive under `Manga/` and `Comics/` on the NAS
on 2026-11-17 found 21 RAR archives, 17 of them named `.cbz`:

- 10 in the RAR4 format, all in `Comics/Issues`;
- 11 in RAR5: five Zelda Legendary Edition volumes in `Manga/Volumes`, four
  RuriDragon volumes in `Manga/Raw`, two files in `Manga/Extras`.

None is solid (`unrar lt` on each). The list is in
`.superpowers/sdd/rar-libarchive-evidence/census.txt` in the main checkout.
Taking the first or the last page out of the nine in Volumes and Raw with
`bsdtar` cost about the same, so libarchive skips the entries before the one
it is asked for rather than decoding them.

## Approaches

1. **libarchive through the `compress-tools` crate. Chosen.** libarchive is
   BSD-2-Clause, and its RAR4 and RAR5 readers are its own code (copyright
   Andres Mejia, 2011, and Grzegorz Antoniak, 2018). Debian ships it in main;
   bookworm has 3.6.2. `compress-tools` (MIT OR Apache-2.0) wraps it, so no
   `unsafe` enters this workspace. The cost: a C library on the runtime
   image, in CI and on every dev machine, and C code parsing untrusted bytes.
   *Since the spike of 2026-11-17, `compress-tools` comes from the owner's
   fork, pinned by commit, with four fixes its 0.16.1 release lacks (O382;
   `docs/notes.md`, "The patched compress-tools (owner's fork, 2026-11-17)").*
2. **libarchive through a binding of our own.** Full control over seeking,
   the encrypted flag and name encoding, but it needs `unsafe_code` relaxed
   for one module. A fallback only, and the owner decides before it is used
   (see Decisions).
3. **The `rars` crate.** Pure Rust and Apache-2.0, so no C library and no
   build changes. Not now: its first release was 2026-05-13, its README calls
   it "an agentic development experiment", and nothing says how its knowledge
   of the format was obtained. That matters for a crate that also writes RAR,
   the one use UnRAR's licence forbids. The interface below is what lets it
   replace libarchive later.

UnRAR stays ruled out, for the reason O90 §1 gives.

## Design

### The feature

- `taiyaku-read` gains the feature `rar-libarchive`, with `compress-tools`
  as an optional dependency. `taiyaku-library`, `taiyaku-serve` and
  `taiyaku-cli` each pass through a feature of the same name. All default
  off.
- Inside `taiyaku-read`, one cfg alias is true when any RAR backend feature
  is on. When several backends are compiled, which `--all-features` does, a
  fixed order picks the one the library uses, and the shared suite below
  runs over every compiled backend.
- The Dockerfile builds the server with `--features rar-libarchive` and
  installs `libarchive13t64` in the runtime stage, and `libarchive-dev` in the
  build stage if the image lacks it.

### The interface

A private module, `taiyaku_read::rar`, gives `pages.rs` three things and
nothing format-specific:

- list an archive's entries in archive order, each with its name as UTF-8,
  its uncompressed size, and whether it can be read (not encrypted, not a
  directory);
- read one entry, by name, to bytes;
- visit every entry once, in archive order, with its bytes.

Opening, seeking and skipping stay inside the backend file
(`rar/libarchive.rs`). `open_archive` dispatches on the sniffed format: zip
as today, RAR to this module when the feature is on.

`pages.rs` applies the page rules above the interface, once, for every
backend: the filter, the natural sort, skipping unreadable entries, the
capacity clamp. It also computes each RAR page's crc32 from the bytes rather
than trusting a backend to report it, so planning a download of a RAR
edition decodes it once.

*Changed in PR 2: the walk of the block headers runs at listing too, so a
listing carries each entry's CRC32 where the header declares a CRC32 and, for
a reference or hard link, its target. A download is planned from those (the
notes' D7). A fourth call, `read_listed`, lists and reads through one open
(O384). A fifth, `signature`, gives the backend's part of the reader
signature: its name and the libarchive version `pkg-config` gave the build.*

*Added on 2026-11-18: libarchive does not compare a stored entry's data with
its header's checksum, so the backend does. Every read walks the archive's
block headers and holds each entry to the CRC32 its header declares. A RAR5
entry that declares only a BLAKE2 hash is not checked (O383). The plan's
amendment A9 says what is checked and what is not.*

### Readability

- `Format::is_readable` admits `Rar` exactly when the feature is on. The
  derive asks it of each stored token, so the derive follows, and
  `scan::pages_of` counts a RAR as it counts a zip.
- Without the feature, `unsupported_container` refuses RAR as it does today.
- `is_readable`'s doc keeps the O90 record and adds that the owner reopened
  it, pointing here.

### Re-reading when the reader changes

A scan skips files whose size and mtime have not moved. A RAR scanned by a
build without the feature would therefore stay uncounted after an upgrade,
because the file itself never changed.

The JPEG XL design (`2026-11-17-jxl-pages-design.md`, branch `jxl-pages`)
has the same problem and designs the fix: `taiyaku-read` exposes a signature
naming what it can read, the store keeps the signature its last scan used,
and when the two differ the next scan re-counts the affected entries without
hashing them again. Whichever of the two PRs switches its format on first
builds it; the other uses it. This design adds the RAR backend to the
signature, with libarchive's version read at run time where the binding
exposes it (a change re-counts only RAR entries, so a libarchive update
costs a header walk of each RAR), and the crate's version otherwise.

*PR 2, on the owner's ruling P2-R5 of 2026-11-19: the signature names the
libarchive version `pkg-config` gives `taiyaku-read`'s build script, not one
read at run time, and the fork is unchanged. It reads
`rar:libarchive-<version>:<the page extensions>`.*

Content fingerprints (branch `content-fingerprints`, its Design 9) asks
`taiyaku-read` which formats it opens, so RAR files become due for
fingerprinting the day this switches on, with nothing more from this work.

### Swapping safely

- **Shared fixtures.** Small RAR archives, made once with RARLAB's `rar` on
  the dev machine and committed with `taiyaku-read`'s test data: RAR5 stored,
  RAR5 compressed, RAR5 solid, RAR4, one with an encrypted entry, one with a
  directory entry plus `__MACOSX` and dotfiles, one with Japanese names. Each
  has a manifest of the expected page names, order and sha256. Their pages
  are generated images: no published page goes into the repository.
- **One suite every backend passes.** It runs each fixture through the public
  `pages.rs` functions and compares the result to its manifest, once under
  the dev machine's locale and once under `LC_ALL=C`, as the container runs.
- **Rolling back.** Under the O91 ruling ("Path counts too",
  `browse::held_by`), a file whose pages the walk did not count still holds
  its work. So a store scanned with the feature and then without it keeps its
  RAR works visible, and their pages are refused with a reason; nothing is
  deleted and saved places are kept. Scanning with the feature again makes
  them readable. The builder proves this on a copy of a real store rather
  than assuming it.
  *Proved in PR 2 on a copy of a store built from the owner's RARs: works,
  members, counted rows and saved places held. Two costs were found: a row
  refused at the count loses its signature to a scan without the feature,
  so the next scan with it counts it again; and a server without the
  feature drops every RAR fingerprint row, so the next server with it reads
  each RAR once more.*

### CI and the local gate

- CI's clippy and coverage runs and `scripts/check-all.sh` (clippy, tests,
  coverage, corpus) all use `--all-features`, so they turn the feature on.
  They keep doing so: CI's Rust jobs and the mutation workflow install
  `libarchive-dev`, and coverage measures the RAR code. Every dev machine
  needs the libarchive headers; `check-all.sh` fails with a clear message
  when they are missing.
- One new CI job runs the tests with default features, to cover the build
  without RAR. It is to become a required check in branch protection.
- The mutation runs build with the feature on, so RAR code gets mutants.

### Licence page

libarchive's BSD-2-Clause notice is added to the licence page, marked as
part of builds with RAR support.

## Decisions (my calls; the owner can overturn any)

- **Solid archives are read, not refused, with no cache.** Reaching a page in
  a solid archive means decoding every entry before it. None of the owner's
  21 is solid, so the cost is accepted until one appears, and a row records
  the follow-up (unpack a solid archive once, on first open).
  *Found otherwise for RAR4 on 2026-11-17: libarchive refuses a solid RAR4 at
  its second entry ("RAR solid archive support unavailable"), so only a solid
  RAR5 is read, and taiyaku refuses a solid RAR4 whole (O380).*
- **compress-tools is checked before anything is built on it.** PR 1 starts
  with a spike against the Zelda v01 volume and the fixtures. It passes only
  if all of these hold:
  - reading one page of a non-solid archive reads the entry headers and that
    page's data, not the data of the entries before it, measured by counting
    the bytes libarchive pulls through our reader (the last page of Zelda v01
    costs about the same bytes as the first). Every page request lists the
    archive again, so a binding that can only read forwards would read most
    of a 270 MB file per page;
  - it reports an entry as encrypted;
  - it gives correct UTF-8 names under `LC_ALL=C`;
  - it works with bookworm's 3.6.2.

  If any fails, the builder stops and reports, and the owner chooses between
  a binding of our own (relaxing `unsafe_code` for one module) and something
  else.
- **Two PRs.** PR 1: the interface, the libarchive backend, the fixtures and
  suite, and all build plumbing (feature, CI jobs, gate, Docker, mutants);
  `taiyaku-read` reads RAR under the feature, but the library and server
  still refuse it. PR 2: switch it on in `is_readable`, the derive and the
  scan, add RAR to the reader signature (building the signature if the JPEG
  XL PR has not merged), the licence page, and the checks on real material
  below.
  *Found otherwise on 2026-11-17, by reading the code while PR 1 was built:
  in a build with the feature, the image among them, a sidecar that names a
  RAR member, as that build's `align --write` can write, is imported and its
  pages served, since neither the import nor the server checks a member's
  format. The derive and the scan still refuse RAR. PR 1 adds no refusal for
  the sidecar, since PR 2 makes serving it the intended behaviour.*
  *PR 2 did this on 2026-11-19, and departed from the design. The fixes of
  O381 come from taiyaku's own walk of the block headers, run at listing as
  well as at reads, and not from the owner's fork, which cannot supply them:
  libarchive gives no target for a `rar -oi` file reference, and marks a RAR5
  entry encrypted only from 3.7.7 (ruling P2-R4). A reference or hard link
  whose size is not its target's, and a RAR5 entry listed as a file that no
  read gives whole (a junction, a redirection of a type libarchive does not
  read, or a file whose header marks it a directory, the last of which
  libarchive 3.8.11 refuses itself, with its own cause, where it holds data),
  are refused at the count (`RESIZED`, `UNREAD_REDIRECTION` and
  `MARKED_DIRECTORY` in `rar::libarchive::headers`). And a download passed
  through takes its type and extension from the format the scan stored, where
  the file is as that scan saw it, and otherwise from a sniff of the file,
  not from the file's name, in every build and not only with the feature (the
  plan's D6).
  `docs/notes.md`, "RAR PR 2: libraries read RAR (2026-11-19)", has the
  rest.*
- **O-numbers.** RAR reading gets a new row, opened and closed by PR 2; the
  solid-archive cache gets its own open row. Numbers come from the next free
  range when the plan is written, after the claims of the branches in flight.
  *RAR reading is O391, opened and closed by PR 2; the solid-archive cache
  is O380, still open.*

## Checks on real material (PR 2)

Copies of the owner's 21 RARs, read from the NAS read-only, go into a scratch
library:

- a scan with the feature counts all 21, each with the page count
  `bsdtar -tf` lists after the same filter;
- a test server serves the first and last page of each, a cover, a resized
  page, and a generated download of an edition with several RAR members;
- the same scratch store, scanned by a build without the feature, keeps the
  works visible with their pages refused and a reason given, and scanned
  again with it reads them, saved places unchanged.

*Found on 2026-11-18, reading the 21 through PR 1: libarchive cannot decode
Fantastic Four v1 360 on either version, nor one page each of Zelda v01 and
v03 on bookworm's 3.6.2, the image's version, though `unrar t` passes all
three. These checks cannot pass for all 21 until PR 2 settles those three
(O386).*

*On the owner's ruling of 2026-11-19 the image and CI moved to Debian
trixie's libarchive 3.7.4, which reads every page of both Zelda volumes;
Fantastic Four v1 360 is refused on 3.6.2, 3.7.4 and 3.8.11 alike, since
no libarchive release decodes RAR 2.0. On the owner's word it was converted
to a stored zip on 2026-11-19, so the checks above run over the other 20 of
the 21; O386 stays open for a user's RAR 2.0 archive.*

*Run in PR 2 over copies of 21 of the owner's RARs, the census's other 20
and Pokemon Ep.14, a RAR4 under `Magazines/` outside the census's scope,
with Fantastic Four v1 360's RAR 2.0 original beside them: each count
equalled `bsdtar`'s; the first and last page of each, a cover, resized pages
and a generated download of an edition of four RAR members came back right;
and the rollback kept works and saved places. The original was refused at
the count, its cause written. `docs/notes.md`, "RAR PR 2: libraries read RAR
(2026-11-19)", "The owner's RARs".*

## Left to the builder

- The interface's exact types and names, and how `open_archive` dispatches.
- The CI job's layout and where the feature pass sits in `check-all.sh`.
- How to make the RAR4 fixture if the installed `rar` no longer writes RAR4.
