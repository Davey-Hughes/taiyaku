# Reading JPEG XL pages

Date: 2026-11-17. Status: approved by the owner on 2026-11-17.
Branch: `jxl-pages`. Closes O130.

## Why

O130: taiyaku counts no `.jxl` file as a page, so an archive of JPEG XL
pages is never a member and the scan reports it unreadable. On 2026-09-17
the owner deferred reading JPEG XL as a future feature, and the one such
archive in the library (Bisque Doll de Chu ♡, 185 pages) was converted to
PNG outside taiyaku. The owner has now chosen to build it.

The owner's aim (2026-11-17) is for taiyaku to read more formats as they are,
without anyone converting files first. JPEG XL stays the odd file: the library is not being
converted to it. So decoding is kept off the path most pages take, and a
client that cannot show JPEG XL is served a JPEG made once and cached.

## Goals

- An archive or folder whose pages are `.jxl`, all or some of them, reads
  like any other: counted at scan, served, and used for covers, thumbnails,
  resized pages, OPDS and downloads.
- Every place that decodes an image decodes JPEG XL: covers, thumbnails,
  resized pages and tiles in the server, content fingerprints (content
  matching PR 2, being built on branch `content-fingerprints`), and the
  CLI's image work.
- A browser that shows JPEG XL gets the page as stored. Any other client
  gets a JPEG.
- Archives scanned before the upgrade get their pages counted without the
  owner doing anything.

## Non-goals

- Encoding JPEG XL, or converting a library to it.
- JPEG XL images inside EPUB books.
- Animation: a page is its first frame.
- Decoding in the browser (WASM).

## Anti-goals

- A page served for reading that shows as broken in a client that cannot
  decode JPEG XL. (Downloads are exempt: see Serving.)
- A decoder that ignores the decode limits the server sets
  (`image::Limits` in `crates/taiyaku-serve/src/cover.rs`, `MAX_DECODE_ALLOC`
  in `crates/taiyaku-read/src/lib.rs`).
- One response cached for both variants of a page, so a client that cannot
  decode JPEG XL gets the stored bytes from a cache.

## What exists on main (9e50c44f)

- `IMAGE_EXTENSIONS` (`crates/taiyaku-read/src/pages.rs:12`) decides what is
  a page, for archives and folders alike. It has no `jxl`.
- A served page's type comes from its bytes, then its name
  (`sniffed_type` and `type_from_extension` in
  `crates/taiyaku-serve/src/source.rs`). Neither knows JPEG XL.
- The server decodes with `image` 0.25.9 to resize a page to a long edge and
  to make thumbnails (`cover.rs`), and caches the JPEGs it makes on disk by
  sha256. The reader asks for resized pages with `?w=`
  (`web/src/api.ts`, `pageUrl`). `image` has no JPEG XL codec, but its
  `hooks` module can register a decoder for a format it lacks
  (`register_decoding_hook`, `register_format_detection_hook`).
- Page responses carry an ETag and answer 304s (`crates/taiyaku-serve/src/routes.rs`).
- OPDS declares page streams as `image/jpeg` (`crates/taiyaku-serve/src/opds.rs`).
- A scan reopens a file only when its size or mtime moved
  (`unchanged`, `crates/taiyaku-library/src/scan.rs:3295`). An archive whose
  pages are all `.jxl` was stored with no page count and stays that way.
- Browsers: Safari has decoded JPEG XL since version 17. Firefox 158 and
  Chrome 155 ship decoding, per their 2026 announcements.
- The owner's library holds no `.jxl` file today (census of `Manga/` and
  `Comics/` on the NAS, 2026-11-17). Bisque Doll's original is kept on the
  NAS under `needs_processing/taiyaku-originals/Manga/Volumes/`.
- `cjxl` and `djxl` are installed on the dev machine.
- `taiyaku-read` carries no image codec into a shipped binary (its
  `Cargo.toml`), so the decoder does not go there.
- The content fingerprints design (branch `content-fingerprints`) moves the
  server's and CLI's decode code into a crate both use (its Design 5), and
  retries a file it could not read whole when the server's build changes
  (its Design 9).

## Approaches

1. **Decode on the server with a pure-Rust decoder registered as an `image`
   hook; send JPEG XL as stored to clients that say they accept it and a
   JPEG to the rest. Chosen.** Every decode path in the server and CLI goes
   through `image` already, so one registration covers them all, and the
   hook is the single place to swap decoders.
2. **Always send a JPEG.** No negotiation and no per-client difference, but
   the NAS decodes and re-encodes every JPEG XL page for every reader, and
   readers lose JPEG XL's smaller files.
3. **Always send the stored bytes and decode only for covers and resized
   pages.** The least work on the server, but a client without JPEG XL shows
   broken pages, which is an anti-goal.

Two decoders fit the licence (GPL-3.0-or-later):

- `jxl-oxide` (MIT OR Apache-2.0, 0.12.6), pure Rust, with a longer record as
  a crate;
- `jxl`, the jxl-rs decoder from the libjxl authors (BSD-3-Clause, 0.8.0),
  which Chrome now ships.

## Design

### Pages

- `jxl` joins `IMAGE_EXTENSIONS`.
- `type_from_extension` maps `jxl` to `image/jxl`. `sniffed_type` recognises
  both JPEG XL signatures: the bare codestream (`FF 0A`) and the container
  (`00 00 00 0C 4A 58 4C 20 0D 0A 87 0A`).

### Decoding

- One function registers the JPEG XL decoder with `image`'s hooks, once per
  process. Every binary that decodes (the server and the CLI) calls it at
  start. A test proves `image::ImageReader` decodes a JPEG XL fixture after
  the call, and another that the decoder refuses an image over the limits
  the caller set.
- The registration lives with the shared decode code, in the crate content
  fingerprints creates if that has merged. Content fingerprints decode
  through it; their build-keyed retry picks up the JPEG XL pages they could
  not read before, with nothing more from this PR. Whichever of the two PRs
  merges second adds a JPEG XL fixture to the fingerprint tests.
- The decoder is chosen by the first task: decode every page of the Bisque
  Doll original with both crates, compare each against `djxl`'s output,
  check whether each can rebuild the original JPEG from a recompressed one,
  and time both on the NAS. Correctness decides first, then speed. The
  builder reports the comparison and goes on with the winner; this is not a
  stop for the owner.

### Serving

- A request for a JPEG XL page whose `Accept` header names `image/jxl` gets
  the stored bytes as `image/jxl`. Any other request gets a JPEG, made once
  and cached on disk by sha256 next to the resized pages.
- That JPEG is the original JPEG rebuilt from the file where the page is a
  recompressed JPEG and the chosen decoder can rebuild it: no quality lost
  and no decode. Otherwise it is a decode encoded at a high JPEG quality set
  for full-size pages, separate from the cover settings.
- The rule covers every route that can hand out stored bytes: the full-size
  page, a `?w=` request for a page that already fits (`Fitted::Original` in
  `cover.rs` serves the stored bytes), and OPDS's `stream_page`, whose ETag
  comes from `page_etag`. Every page and 304 carries `Vary: Accept`, and
  the two variants have different ETags, so neither a 304 nor a cache can
  hand one variant to a client that asked for the other.
- Covers and thumbnails are always JPEG. The library cover route is cached
  as immutable, so it cannot vary.
- OPDS clients get pages by the same rule, so one that does not name
  `image/jxl` gets the `image/jpeg` the feed declares.
- Downloads send pages as stored: a download is the owner's file, and
  rewriting a lossless page as JPEG would lose quality.
- Before relying on the rule, the builder reads the `Accept` header that
  each of the owner's clients actually sends for a page image (iPhone
  Safari, desktop Chrome and Firefox, and Panels over OPDS), from the
  server's log, and records it. A client that does not name `image/jxl` gets
  JPEGs: correct, with more work for the NAS.

### Re-reading when the reader changes

This PR builds the mechanism; the RAR design
(`2026-11-17-rar-libarchive-design.md`, on branch `rar-libarchive`) uses it.

- `taiyaku-read` exposes a signature: a short string naming what it counts
  as a page and which containers it opens. The RAR work later adds its
  backend and version to it.
- The store keeps the signature its last scan used. When a scan starts with
  a different one, it re-counts every entry whose format the change could
  affect, even though its size and mtime did not move, and does not hash it
  again.
- For this PR that means every zip and folder entry is counted once more on
  the first scan after the upgrade. Only an entry holding `.jxl` files gets a
  different count.

*Note, 2026-11-18: "does not hash it again" holds for a file. A folder that
holds a digest, was listed whole and is walked at another page count than it
stored has its digest taken again, because no read of it could hash to the
old digest (`Held::digest_outdated_by` in `scan.rs`). A folder holding both
JPEG and JPEG XL pages is therefore hashed once at the first scan after the
upgrade.*

An entry holding both JPEG and JPEG XL pages changes its page numbering when
the JPEG XL pages start counting, which would move a saved place. No such
entry exists in the owner's library, so this is accepted.

## Checks

- Fixtures made with `cjxl`: lossy, lossless, a recompressed JPEG, one with
  alpha, and one past the decode limits. Each is decoded and compared with
  `djxl`'s output.
- Real material: the Bisque Doll original, copied read-only into a scratch
  library. A store scanned by main counts it unreadable (main never hashes
  it, since an entry with no page count is set aside before hashing); a
  scan by this branch's build counts 185 pages, and entries that main had
  already hashed are not hashed again. A test server
  shows its first and last pages to a browser that accepts JPEG XL (as JPEG
  XL) and to a request that does not (as JPEG), and makes its cover.

## Decisions (my calls; the owner can overturn any)

- Negotiate on `Accept` rather than always transcoding or always passing
  through (see Approaches).
- Downloads stay as stored, and covers are always JPEG.
- The decoder comparison is a report, not a stop.
- The scan re-counts on the reader signature, not on the server's build:
  keyed on the build, every deploy would reopen every archive in the
  library. Content fingerprints keep their own build-keyed retry.
- JPEG XL support is always built, not a feature: both decoders are pure Rust
  and need nothing from the system.
- One PR. It closes O130; any follow-up rows take numbers from the next free
  range when the plan is written.

## Left to the builder

- Where the registration function lives, so both the server and the CLI
  reach it.
- Where the store keeps the reader signature, and which formats a change
  marks for re-counting.
- The tolerance for comparing lossy decodes against `djxl`.
