# Status

Where the project is, what is settled, and what comes next. Updated 2026-08-01.

The detailed evidence for everything asserted here is in [`notes.md`](notes.md),
which is a working log rather than a summary — every claim below traces to a
measurement recorded there. The format is specified in
[`sidecar.md`](sidecar.md).

## Where it stands

**L0 works.** The aligner reads two editions, pairs their pages, and writes a
sidecar. It has been measured against eleven series and holds up on every case the
design anticipated, plus several it did not.

**The reader has a back end and no front end.** Milestones 1 through 3 shipped:
page reading is a crate of its own, `taiyaku serve` hands an HTTP client the
sidecar, any page by index, and where you were, and it can now read those pages
from a Stump instance as well as local disk and publish an OPDS feed grouping a
work's editions. Nothing renders any of it yet — the browser reader is the next
piece, and none of *that* has started.

## What is built

Four crates, 189 tests, no dependencies beyond `image`, `zip`, `sha2`, `rayon`,
`base64` and `serde`, plus `axum`, `tokio`, `tower` and `ureq` in the server.
Three of the 189
are standing checks gated on `TAIYAKU_CORPUS`, of which the third also needs
`TAIYAKU_STUMP_URL` and `TAIYAKU_STUMP_KEY`; each skips cleanly when what it needs
is absent, so CI stays green without the corpus or a reachable Stump. One measures
composition against a direct solve — 2,427 page comparisons, 0.91% loss, 0.00%
disagreement. One measures that the server and the aligner agree about what page N
is — 617 pages across 3 editions, no disagreement. One measures that the server
and Stump serve the same bytes for a member both hold — 187 pages compared
byte-for-byte, no disagreement, and 8 members this Stump instance does not hold
recorded as absent rather than silently skipped. `check-all.sh` runs all three
with `--nocapture` against an absolute, canonicalised `TAIYAKU_CORPUS`, so a green
gate shows what each one measured, or says plainly that it did not run them at
all.

`taiyaku-core` is the portable half — no I/O, so it compiles to WebAssembly:

| Module | Holds |
| ------ | ----- |
| `solve` | similarity, per-pair floor calibration, Needleman-Wunsch, run compression, confidence, verdict |
| `slot` | spread detection, slot expansion, reading direction |
| `sidecar` | the `taiyaku.json` format — parse, emit, judge, merge an edition in |
| `route` | which path a lookup walks between two editions, and the page it lands on |

The other three touch the disk or the network:

| Crate | Holds |
| ----- | ----- |
| `taiyaku-read` | how a member becomes pages — archives, directories, natural order, page counts, content hashes, one page by index |
| `taiyaku-serve` | the HTTP API: works, sidecar, page by index, progress, an OPDS 1.2 + PSE feed and generated covers, all behind a `PageSource` trait with **two** implementations — local disk, and a composite that resolves each member to disk or, inline within the same type, to Stump (see below) |
| `taiyaku-cli` | the `taiyaku` binary: image decoding, fingerprinting, the content-addressed cache, the HTML report, argument parsing |

`taiyaku-read` exists because the page sequence is normative — `sidecar.md`, "How
a member becomes pages" — so two consumers deriving it separately is a defect the
format cannot detect. The aligner and the server now derive it in one place, and
a gated corpus check compares all 617 pages of the primary volume across the two
paths.

Five commands:

```
taiyaku align  <a-member>... -- <b-member>...   [--write FILE] [--split-spreads] [--ltr] [--stats] [--direct]
taiyaku verify <a-member>... -- <b-member>...   [--out FILE] [--sidecar FILE]
taiyaku topology <hub>... -- <spoke>... -- <spoke>...
taiyaku lookup <taiyaku.json> <from> <to> <page>...
taiyaku serve  <taiyaku.json>...                [--addr HOST:PORT] [--stump-url URL] [--stump-key KEY] [--log-requests FILE] [--refresh-stump]
```

`align` takes several members on either side, which is how one volume aligns
against seven chapter files, or two tankōbon against one deluxe volume. `--write`
**merges** into an existing sidecar rather than overwriting it, which is how a work
gets a third edition; on a merge the a-side is the document's hub, so `--name-a` is
optional and an error if it names anything else. `--direct` overrides that for a
pair that has earned a direct edge. `verify` writes a contact sheet for looking at
what the aligner decided. `lookup` resolves a page through a written sidecar, which
is the operation a reader performs — through the hub where no direct edge exists,
saying on every call which path it walked. `serve` is handed sidecar paths the
same way `align` is handed members, because deciding which files constitute a
library is a different job; it serves `/api/works`, the sidecar verbatim, a page
by index, and a reading position, and 404s with a message naming what was missing
rather than returning a blank page. `--stump-url`/`--stump-key` (or their
`TAIYAKU_STUMP_URL`/`TAIYAKU_STUMP_KEY` env fallbacks) point it at a Stump
instance; pages come from whichever source — Stump or local disk — actually holds
the member, resolved once at startup and cached beside the sidecar. An OPDS 1.2 +
PSE feed at `/opds/v1.2/catalog` groups a work's editions under one entry, which
is what Stump and Komga cannot express on their own.
`--stats` prints what calibration measured and what shape came out — noise,
signal, separation, margin, run count, average run length and the verdict — one
line each, in a fixed field order so a survey can collect them into a table.
`scripts/margin-survey.sh` is that survey.

## What is settled

Decided by measurement, not preference. Each of these was tried against an
alternative that lost.

- **dhash-16 over pHash.** 256 bits on luma, chosen on measured separation.
- **Margin over similarity as the confidence signal.** How far the best match beats
  the runner-up, not how good it looks in isolation.
- **A per-pair similarity floor**, calibrated from the pair itself rather than fixed
  globally. This is what refuses unrelated volumes.
- **Fragmentation as the quality signal**, read together with coverage — neither is
  sufficient alone.
- **Fragmentation is judged on average run length**, not runs per hundred pages.
  Measured across the corpus, every confirmed-good pairing runs 36.0 to 200.0
  slots a run and everything that must not pass 6.8 or below, with no overlap;
  the threshold sits at 27.0, three quarters of the weakest good figure, capped
  by the edition's own length because nothing shorter than 146 slots has been
  measured. Both measures separate on this corpus once refusals are gated on
  coverage; average run length was chosen because it states the property in
  slots rather than per hundred pages, so it does not shift with the edition's
  length.
- **Slots, not pages, as the addressed unit.** A slot is a whole page or one half of
  a joined spread. Spreads expand before solving, so the alignment stays strictly
  1:1 and the solver never learns they exist.
- **Reading direction is an input.** Monotonicity forbids reversing a pair, so it
  cannot be discovered.
- **Hub-and-spoke topology**, raw as hub, validated against all-pairs at ~0%
  disagreement, and **resolvable** — a lookup with no direct edge composes
  through the hub. Measured on the path a reader actually walks: 0.91% loss and
  0.00% disagreement over 2,427 comparisons on Go-Toubun v01.
- **One stored edge per spoke, reversed when needed.** The solver is symmetric on
  the pairing relation, measured across five pairs in two series, so N−1 edges
  carry every ordered pair rather than 2(N−1).
- **JSON for the sidecar**, with span notation (`7`, `4R`, `2-5`, `45R-46L`).
- **How a member becomes pages is part of the format, not an implementation
  detail.** The page sequence is derived, so a reader deriving it differently
  disagrees with every run while still matching every `sha256` and `pages` count
  in the file. Entries sort by whole path in natural order rather than as
  stored; non-images are not pages. Measured over 57 archives: getting the first
  wrong changes the sequence for 32 of them, the second shuffles chapters
  together on 2 of the 7 that nest, the third shifts everything after a
  `ComicInfo.xml` on 12.
- **A content-addressed fingerprint cache** keyed on member SHA-256. 3.4 s → 189 ms.

## What has been proven against real material

Everything here is a measurement against the library, not a synthetic test.

| Case | Result |
| ---- | ------ |
| Same volume, raw ↔ English | sound across eleven series |
| Colour ↔ monochrome | costs little; less than removing text does |
| Joined spread ↔ split pages | recovered symmetrically via slots |
| Chapters → volume | handled by multi-member alignment |
| **2 tankōbon → 1 deluxe volume** (Monster) | 427 of 444 slots, 13 runs |
| **4 volumes → 1 deluxe volume** (Nausicaä) | 553 of 600 slots, 20 runs |
| **Letterboxed spreads** (Tongari v09) | 176 of 180 slots once trimmed symmetrically |
| **All-spread edition ↔ ordinary** (One Piece v092) | 200 of 200 slots, 1 run |
| Unrelated volumes | refused |
| **A mislabelled file** (Bocchi *Gaiden* filed as v01) | refused, and its real counterpart found |
| Art book ↔ manga | pairs everything and should not — see below |
| **Twenty-two needles** (calendars, Jump issues, two art books) | 7 clear calibration and pair 100%; all 7 flagged `FRAGMENTED` |

## What is known to be wrong or missing

Stated plainly because each is a thing a reader would hit.

**A small needle in a large haystack always aligns — but `verdict()` already
catches it, and the tool was not saying so.** When side A is much shorter than
side B, the DP inserts gaps in B freely, so A comes back fully covered no
matter how unrelated the two are. Measured across twenty-two needles — One
Piece calendars, weekly Jump issues carrying chapters no volume in the pair
contains, and two art books — seven cleared calibration, and every one of the
seven paired 100% of its pages with nothing unmatched.

**Neither calibration statistic can refuse them, which is now measured rather
than suspected.** A One Piece art book against Go-Toubun no Hanayome — a
different series by a different artist — posts a margin of 0.020, beating four
confirmed-good pairings, and a separation of 0.145, beating seven. So margin is
not a weak signal but an inverted one, and `MIN_SEPARATION` cannot be raised to
catch needles without refusing genuine volumes. No margin guard ships.

What does separate is run structure: every needle that got through averages 1.1
to 1.7 slots a run against 36.0 to 200.0 for every confirmed-good pairing.
`verdict()` calls all seven `Fragmented`, and both `align` paths now print that
verdict rather than one of them discarding it. **The consequence for a reader is
that acceptance is a statement about calibration, not about the alignment —
only the verdict is the latter.** See "Twenty-two needles" in `notes.md`.

**Art books cannot be aligned and the solver does not say so.** Reading them works.
Linking their pages to the manga pages they came from is a different problem —
sparse, unordered, cross-composition — and wants nearest-neighbour search with a
margin threshold plus a crop-tolerant hash. Not the DP.

**No correction UI.** The design calls this part of the core, and it does not exist.
`verify` shows what the aligner decided; nothing lets you fix it.

**No reader UI.** `taiyaku serve` will hand a client the sidecar, its pages and a
reading position, and nothing draws them. There is no page view, no swap, and no
correction UI, so the thing a person can actually do with a sidecar today is
still `lookup`.

### Open items

Twelve, each found by a whole-branch review or carried as a deferral, and each
judged not worth blocking on at the time. They are tracked here rather than left
in a scratch file so that closing one is a visible act rather than a thing
someone remembers. **Seven are now closed — O2, O3, O4, O5, O7, O10 and O12 —
leaving five.**
Closed rows stay in the table rather than being deleted, because a list that
only ever grows says nothing about whether anything gets fixed. The evidence for
each is in the detail below; the table is what a reader scans.

| ID | Open | What | Where | Opened by |
| --- | :---: | ---- | ----- | --------- |
| O1 | [ ] | Page responses carry no cache headers — *measured: nothing to cache yet* | `routes.rs::get_page` | M2 review |
| O2 | [x] | Progress is written non-atomically | `progress.rs::store` | M2 review |
| O3 | [x] | `PUT /progress` does not range-check the slot | `routes.rs::put_progress` | M2 review |
| O4 | [x] | Two sidecars claiming one work id is untested | `works.rs::load_works` | M2 review |
| O5 | [x] | A non-numeric page index answers axum's default 400 | `routes.rs` | M2 review |
| O6 | [ ] | A cover from page 0 can be a 430 MB decode | `cover.rs` | M3 review |
| O7 | [x] | The request log grows in memory for nothing | `reqlog.rs::Log` | M3 review |
| O8 | [ ] | Stump resolution blocks the listener — *no longer silently* | `lib.rs::resolve_all_works` | M3 review |
| O9 | [ ] | No single-flight guard on the request path | `composite.rs::StumpSide` | M3 review |
| O10 | [x] | `MAX_DECODE_ALLOC` is duplicated across crates | `cover.rs`, `taiyaku-cli/src/fingerprint.rs` | M3 review |
| O11 | [ ] | Neither on-disk cache notices a change on the far side — *escape hatch added* | `stump/cache.rs`, `cover.rs` | M3 review |
| O12 | [x] | `spawn_blocking` covers three of five blocking handlers | `routes.rs::get_progress`, `::put_progress` | M1/M2 deferral |

Sites are named by symbol rather than line, because a line number in a document
is wrong the first time someone edits the file above it.

**`serve` was a first cut, and five things about it were known thin** — O1 to
O5, each found by the whole-branch review. Four of the five are closed. Only O1
remains, and it remains on a measurement rather than a plan: an ETag would have
saved nothing against the sessions actually recorded. O5 was nearly deferred
alongside it on the strength of a wrong estimate — "a custom rejection handler
touches routing for every route" — which was checked rather than repeated and
turned out to be two handlers and a local parse:

- **O1 — Page responses carry no cache headers. Open, but no longer
  unmeasurable.** The bytes for a given work, edition and index never change,
  and the sidecar already records a `sha256` per member, so an `ETag` is a line
  of code. The design's "one fetch, one cache entry" argument for serving whole
  spreads assumes the client caches; nothing yet helps it.
  **What blocked this was not the header but the evidence.** The request log
  recorded no request headers at all, so "does a client ever send
  `If-None-Match`?" — the question that decides whether an `ETag` buys anything
  — could not be answered from a session that had already been run. It now
  records five, on an allowlist: `if-none-match`, `if-modified-since`, `range`,
  `accept` and `user-agent`. An allowlist rather than everything, because
  everything grows each line without bound and puts a `Cookie` or an
  `Authorization` into a file that gets read by scripts and pasted into issues.
  `scripts/request-summary.sh` reports the conditional-request count, the range
  count and which clients were seen; on a log written before this it reports
  zero and "(not recorded)" rather than failing.
  **And then the measurement came out against shipping it — see `notes.md`,
  "An ETag would have saved nothing, and the obvious way to check that is
  wrong".** The planned check was itself unsound: `If-None-Match` carries a
  validator the client got earlier, this server sends none, so that count is
  zero by construction and says nothing about any client. What can be measured
  without a validator is whether Panels ever fetches a page it already has, and
  across both recorded sessions it does so **zero** times — 177 page requests
  and 177 distinct pages in the flick session; the only repeats anywhere are
  session 1's three `pages/0` paths, all 404s from the off-by-one. An `ETag`
  would have produced no 304s and saved none of the 43.7 MB.
  So O1 stays open and stays deferred, now on evidence rather than suspicion.
  The browser reader refetches by construction — swapping editions refetches
  the page you are on — so the same measurement against it should come out the
  other way. **The order inverts for that one:** ship the validator first and
  measure the 304s, because the client-side question cannot be answered while
  the server offers nothing to answer it with.
- **O2 — Progress is written non-atomically. Closed.** `fs::write` truncated
  first, so a crash mid-write left a short file and the next read answered 500
  rather than "nothing recorded" — the worse of the two wrong answers, since it
  tells the reader the server is broken rather than that it has no position to
  restore. `store` now writes a temp file and renames, the pattern the Stump
  resolution cache (`taiyaku.stump.json`) already used beside the same sidecar.
  The temp name is **unique per attempt** rather than fixed, which the cache's
  is not: progress is the one file here a person writes from two devices at
  once, and two writers sharing one temp name can interleave into publishing
  the empty file the change exists to prevent. Same reasoning and same shape as
  `cover::unique_temp_path`; the two are not shared yet. A crash cannot be
  staged in a test, so the test pins the mechanism instead — the destination is
  replaced by a rename, never opened for truncation, made observable by taking
  write permission off the existing file, and skipping with a printed line
  where that precondition does not hold (root, or a filesystem ignoring the
  mode bits) rather than reporting a green that measured nothing.
- **O3 — `PUT /progress` does not check the slot is in range. Closed.** It
  validated the edition, so a position could name a real edition and a page
  past its end. It now range-checks too, and **in slots rather than pages**:
  a position is a slot — that is why `progress.rs` records one — and an edition
  that joins spreads has more slots than pages, so checking against
  `total_pages()` would have refused the last slot of every such edition, One
  Piece v092 being spreads throughout. A test holds that distinction: 4 pages
  with page 0 a spread is 5 slots, and slot 4 must be accepted. The refusal is
  404 with the same wording shape as the edition check above it, naming the
  edition and how many slots it has.
- **O4 — Three error paths were named here; all three are now covered.**
  `PageSource` narrowed to one member during M3, so the edition
  walk that used to live in `LocalLibrary` now lives in `ServeState`. An
  unknown edition is tested there, both directly and end to end over HTTP (a
  404 for an edition a work does not have). An out-of-range page index is
  tested at the `ServeState` level and dispatched through the router on both
  the `/api/…` page route and the OPDS stream route. **Two sidecars claiming
  one work id is now tested too** — the code refused it from M2 and nothing
  exercised it, so it was correct by inspection only. The test asserts the
  message names *both* paths, since the point of refusing is that the person
  running the server cannot otherwise tell which two files collided; a second
  test loads two differently-named sidecars side by side, so the refusal
  cannot be spelled "refuse everything".
- **O5 — A non-numeric page index answered axum's default 400. Closed.** It was
  the only message this server sent that this server had not written:
  `Invalid URL: Cannot parse value at index 2 with value `abc` to a `u32``,
  which names a Rust type and a position in a path tuple, neither of which
  means anything to a client. Both routes addressed by page index — the
  `/api/…` one and the OPDS stream — now take the segment as a `String` and
  run it through one shared `page_index`, which answers ``\`abc\` is not a page
  number; pages of `raw` are numbered from 0``.
  **The status stays 400 rather than becoming 404**, deliberately: `pages/4`
  names a page an edition does not have and `pages/abc` does not name a page at
  all, and a client is entitled to tell those apart. What they now share is
  that both come from here.
  Shared rather than written twice, and pinned from both ends — Panels reaches
  pages through the OPDS route, not the `/api/…` one, so fixing only one would
  have fixed the one nothing uses. The refusal is carried by a named
  `NotAPageIndex` rather than a bare `Response`, the pattern `NoSuchWork`
  already established in the same file for the same reason: a `Response` is 128
  bytes and `Result<u32, Response>` trips `clippy::result_large_err`.

**M3 found six more, the same way** — O6 to O11, its own review, not blocking.
Two are closed; the four that remain each want a decision before they want
code, which is why they are not simply "next":

- **O6 — A cover is generated from page 0, which on an art-book edition is
  a 430 MB decode.** The corpus's Color Walk material is TIFF at that size, so
  the first request for such a cover is slow and nothing warns before or during
  it. The decode limit is raised to 3 GB, mirroring
  `taiyaku-cli/src/fingerprint.rs`, so the request succeeds — but succeeding is
  not the same as being fast.
- **O7 — The request log accumulated in memory as well as on disk, unbounded.
  Closed.** Every entry logged to disk was also kept in a `Vec` behind a mutex
  for the life of the process, and nothing in production ever read it back:
  `Log::entries()` was called only from tests, while
  `scripts/request-summary.sh` — the actual consumer — reads the JSONL on disk.
  So the growth was real and bought nothing, on a log that exists precisely to
  be left running through a reading session. The `Vec` and `entries()` are
  gone, and `Log::new` no longer takes an `Option<PathBuf>`: a log with nowhere
  to write would now record nothing anywhere, and `serve` already expresses
  "not logging" as no `Log` at all.
  The four tests that read `entries()` read the file instead, **converted
  first and run green before the in-memory copy was removed**, so the disk
  artefact was shown to carry every fact the `Vec` did rather than assumed to.
  That also closes a weakness the old suite had and named in a doc comment:
  every other test used `Log::new(None)`, so deleting the whole file-writing
  branch would have left the suite green on the strength of one test. Every
  test here now goes through the file a person actually measures from.
- **O8 — Stump resolution happens at startup, for every member of every work,
  before the listener binds. Still true; the silence is not.** A summary line
  printed once a work *finished* resolving, so a work with many unresolved
  members was silent for its whole duration and a slow resolution was
  indistinguishable from a hung one. `resolve_work` now reports each member as
  it asks about it — its path, its position and how many there are — and
  `resolve_all_works` prints that. A member answered from the cache reports
  nothing, so a warm start is exactly as quiet as before: a line per member on
  every start would be noise that trains a reader to ignore it, and a test
  pins both halves. **What remains is the blocking itself**, which wants the
  listener to bind first and resolution to run behind it, serving from disk
  until it lands — a real change to how `StumpSide` holds its resolutions, and
  not one to make on a corpus small enough that nobody has felt the wait.
- **O9 — There is no single-flight guard on the request path.** Several requests
  arriving during one Stump timeout all wait it out independently rather than
  sharing the one outcome.
- **O10 — `MAX_DECODE_ALLOC` was duplicated. Closed.** Between
  `taiyaku-cli/src/fingerprint.rs` and `taiyaku-serve/src/cover.rs`, across a
  crate boundary with no dependency edge between them. Each copy said so in a
  comment, and a comment asking whoever changes one to keep the other in step
  is the arrangement that drifts. It now lives in `taiyaku-read`, which both
  already depend on — it is the crate they share — and whose pages are the
  bytes both of them decode, so the ceiling on that decode sits alongside them.
  Not `taiyaku-core`, which compiles to WebAssembly and describes the format; a
  native decoder's memory ceiling is neither. No new test: the duplication is
  removed rather than checked, so the compiler holds what a test would have
  had to assert, and the existing decode tests on both sides cover the move.
- **O11 — Two on-disk caches never notice a change on the far side, and this
  was documented nowhere until now.** `taiyaku.stump.json`, written beside each
  work's sidecar (same directory, same stem), records the Stump book each
  member resolved to — and, deliberately, records the members Stump does not
  have too, so a miss costs one query once rather than one per page turn. That
  is also its weakness, named as Risk 5 in the design and left unaddressed:
  nothing watches for the library changing on Stump's side, so a book added to
  Stump after a work's first resolution stays invisible to taiyaku until
  someone knows to delete `taiyaku.stump.json`, which forces every member of
  that work to be re-resolved on the next start. **That last sentence is now a
  flag rather than a piece of folklore:** `taiyaku serve --refresh-stump`
  forgets every handed work's resolutions before resolving them again, which is
  precisely what deleting the file by hand did, and it says so rather than
  doing nothing when no `--stump-url` was given. The tradeoff itself stands —
  a remembered miss still costs one query once instead of one per page turn,
  and nothing watches Stump for changes — but acting on it no longer requires
  having read this paragraph. The generated-cover cache has
  the same shape without the excuse of a design tradeoff behind it: covers
  live under `.taiyaku-cache/covers`, relative to the server's working
  directory, and are unbounded and never pruned — an archive that is replaced
  gets a new cover under its new hash, but the old file is never reclaimed.

**One deferral was carried rather than found** — O12, named while planning M1
and M2, and now closed:

- **O12 — `spawn_blocking` covered three of the five handlers that do blocking
  I/O. Closed.** `get_page`, and the OPDS `stream_page` and `cover`, handed
  their work to a blocking pool; `get_progress` and `put_progress` read and
  wrote files directly on the async worker. All five do now, so the only
  handler left on the worker is `get_sidecar`, which serves the parsed
  document from memory and touches no disk at all.
  `put_progress`'s validation moved inside the boundary with its write rather
  than staying outside it: it blocks on nothing, but splitting the handler
  across the boundary would mean looking the work up twice, since a borrow
  cannot cross into the closure. A small type, `NoProgress`, carries the
  distinction `spawn_blocking` would otherwise flatten — a work, edition or
  slot that does not exist is a 404 and the client's mistake; a position file
  that will not read or write is a 500 and this server's. `get_page` needs no
  such type because both of its failures are 404s.
  **Measured, not asserted.** A FIFO at the progress path blocks a real read on
  the real code path, and on a deliberately `current_thread` runtime a read
  that runs on the worker stops the runtime dead — so a canary task never gets
  scheduled and a watchdog thread has to rescue the request, which it records.
  Before the change the test fails in 0.56 s; after it, it passes in about
  60 ms. The write path gets the same treatment and is **not** measured that
  way: `store` writes through a temp file whose name is unique per attempt, so
  there is no path a test can pre-create as a FIFO.
  The other deferral recorded alongside it was already closed: the index →
  (member, offset) walk now lives in `taiyaku_core::Edition` rather than in
  `LocalLibrary`.

**Series-level identity is manual.** Which two files are the same volume is decided
by the person running the tool.

**Blank pages are still anchored on.** A blank page's dhash is exactly zero, so
every blank in every edition is identical to every other and the solver places
them arbitrarily. The corpus check declines to measure them rather than pretending
they agree; refusing to anchor on low-variance pages is unbuilt. See
`notes.md`, "Blank pages match each other perfectly and lie about it".

**Hub choice is declared, not measured.** `notes.md` argues the best hub is the one
minimising total loss across spokes, and that every input to that decision is
something an alignment already reports. A merge keeps whatever hub the file names.

## What comes next

Roughly in order. The first two are small and unblock things; the third is the large
piece.

### 1. Refuse on margins, not just counts — measured, and margin does not fit

The candidate fix was measured against the corpus rather than shipped. Margins
are quantised to whole 256ths, and the corpus's one confirmed needle (Color
Walk, margin 0.004) sits exactly one bit below its weakest confirmed-good
pairing (Monster's 2:1 repagination, margin 0.008). No safety factor fits in a
one-bit gap, so no margin threshold ships. Average run length carries the case
instead — every confirmed-good pairing at 36.0 to 200.0 slots, everything else
at 6.8 or below — and on this corpus runs per hundred pages separates as well,
once refusals are gated on coverage; the swap was made because run length is
stated in slots rather than per hundred pages, not because the old measure
could not tell these rows apart. `verdict()` already called the needle case
`Fragmented`; both `align` paths now print that verdict, so the case that
opened this item is no longer silent.
**Closed.** Twenty-two needles have since been measured, and they settle it
harder than the survey could: a needle outscores four confirmed-good pairings
on margin and seven on separation, so neither calibration statistic can be made
to refuse this case. Only run structure separates, by a factor of twenty. What
remains open is narrower and belongs to the reader rather than the solver —
`Fragmented` is reported and nothing refuses on it, so a reader consuming a
sidecar has to look at the verdict rather than assume acceptance means
correspondence.

### 2. Duplicate resolution as a command

The protocol is worked out and was run by hand across 109 volumes. It wants to be a
command:

1. **Signature** — file count, total bytes, sorted size list. Settles exact
   duplicates without decoding anything.
2. **All-pairs alignment within the set** — not everything against the first copy,
   which hides clusters. Identity collapses copies differing only in packaging,
   including across different image counts.
3. **Rank what remains** by content first (slots, then pairings), then by how
   cleanly it aligns.

On the One Piece collection that took 19 duplicate sets down to 13 real choices,
with 2 mislabelled files flagged rather than silently ranked.

### 3. The reader

A PWA consuming the sidecar, with the correction UI as part of it rather than
after it. This is the multi-week piece and the point of everything above.

**Its blocker is cleared.** Until 2026-07-30 there was no way to produce a
sidecar with three editions and no way to resolve through one, so on any work
with more than two editions — which the primary development corpus is, at five —
the reader could not have answered "show me this page in edition C". `align
--write` now merges and `resolve` now composes. What the reader still needs from
this side is a swap UX for five editions, which `notes.md` leaves undecided:
cycling works for two or three and past that wants a picker.

**M1, M2 and M3 have landed.** M1 moved page reading into `taiyaku-read`, so the
aligner and anything serving pages derive the normative sequence from one place,
and added addressing a single page by index — serving page 47 of a 195-page
volume costs one page rather than the volume. M2 added `taiyaku-serve` and the
`serve` command: the work list, the sidecar verbatim, a page by index with its
content type, and a reading position stored beside the sidecar, all behind a
`PageSource` trait so running beside Stump and replacing it are the same code
above that line. A gated corpus check compares every page the server serves
against the page the aligner sequenced — 617 of them on Go-Toubun v01, no
disagreement.

M3 added Stump support and an OPDS 1.2 + PSE feed. **Say what shipped, not what
the design sketched:** the design named a third `PageSource` implementation,
`StumpLibrary`, alongside `LocalLibrary` and a composite. It was never built,
because it could not have answered anything on its own — resolving a member to
a Stump book needs the resolution map, which only the composite holds, so a
standalone Stump-only source would have been unusable by construction. What
shipped is `CompositeSource`, which resolves each member to whichever source
holds it, with the Stump-backed branch (GraphQL to resolve, REST to fetch
bytes) living inline inside it. `PageSource` has **two** implementations, not
three.

**What would make the third one worth building**, recorded so the decision can
be revisited on evidence rather than re-argued: a work whose members live only
in Stump and never locally. That is not reachable today for a reason deeper
than the trait — `resolve_member` needs the member's *local* file, for its size
and for the page-hash confirmation that makes a match trustworthy, so a member
taiyaku does not hold can never resolve at all. M3 therefore offloads page
reads for files you already have; it does not let taiyaku serve a work it does
not have. Serving one would need a different way to match a member to a book,
which is its own design question and wants its own pass — at which point a
standalone Stump source stops being nomenclature and starts being load-bearing. **Say what the feed actually earns, not what the milestone originally
claimed:** M3's stated end state was "Panels reads the library", but that was
already true without taiyaku — this Stump instance serves OPDS-PSE itself, and
a third-party reader could point at it today. What taiyaku's feed adds is
grouping several editions under one work, which neither Stump nor Komga can
express: on this instance `Go-Toubun no Hanayome` and its Full Color Edition
are two unrelated 14-book series, with no link between them and no way to make
one inside either server. The gated `PageSource` equivalence check measured
this for real against the live instance: 187 pages of `en-mono` agree
byte-for-byte between `LocalLibrary` and the composite's Stump-backed path,
and 8 members — the raw and all seven fan chapters — are absent from Stump and
correctly recorded absent rather than retried per page. See `notes.md` for the
coverage table and what the instance exposes.

**The Panels measurement has started, and found a defect before it found a
number.** Panels read from this server over the LAN on 2026-07-31 — see
`notes.md`, "Panels, measured: the feed was one-based and should not have been".

The first session's finding was not a performance figure but a correctness bug:
**OPDS-PSE counts pages from 0 and taiyaku's feed counted from 1**, so Panels
met a 404 on opening every book, and every page it then displayed was the one
before the page it believed it was on. Thirty per cent of the session's requests
were that 404. Fixed, and the test that used to assert the wrong convention now
asserts the right one at both ends of the range.

**The targets are now measured**, over a deliberate fast-flick session of 177
page requests. The server is not the constraint by a wide margin: median
processing 1.43 ms against a tightest client request rate of 40 ms. Panels
issues no concurrent page requests — zero overlaps, and the arrival floor holds
at 40 ms even flicking as fast as the UI allows — and never asks for a reduced
size, so it takes whatever resolution the server holds, 247 KB a page here.

One thing is deliberately **not** claimed: whether Panels *blocks* on each
response. At 1.43 ms no request was ever still open when the next arrived, so a
blocking client and a UI-paced one are indistinguishable in this log. Measuring
it would need a deliberately slowed response, which is not worth building.

What remains: the browser reader itself, the correction UI, and the five open
items tabled above — all of them found by a whole-branch review, now that O12,
the one M3 carried rather than found, is closed.

### 4. Skipping scanlation credit pages

A reader setting that keeps donation pleas, credits cards and recruitment ads out
of the reading flow, and puts them in a section of their own that can be opened
deliberately. Not deleted and not hidden — a scanlation group's credit is the
thing it asked for in exchange for the work, and a reader that silently discarded
it would be taking a side. It is simply not part of reading the chapter.

**The evidence says taiyaku can already find them**, which is why this is on the
roadmap rather than in the "if it is worth it" pile. Measured on the primary
volume — see `notes.md`, "Scanlation credit pages are already visible in the
alignment" — three of the fan edition's seven chapter files each contribute
exactly one page that pairs with nothing and sits at the member's final index,
and all three are the same donation card. Long unpaired runs, by contrast, are
genuine content the other edition lacks. **Shape separates them, and every
sidecar already carries the alignment that shows it.**

A second signal needs no alignment at all: the cards recur across sibling members
of one edition, so a near-duplicate by dhash finds them even in an edition with no
counterpart, which is the common case for a fan release. They are byte-different
per chapter, so exact hashing does not.

**Unsettled, and wanting a brainstorm before any code.** Whether the classification
belongs in `taiyaku.json` as a per-edition list — the way `spreads` is, since both
are derived facts about a page sequence — or stays client-side the way parity
breaks do. Whether it is `taiyaku align`'s job to record or a separate command's to
propose. What the toggle does to page numbering, which is the same authority
question swapping already raised. And the false-positive rate against colour
inserts and author afterwords, which have the same shape and must not be swept up.
None of that is decided, and the measurement above is one group's habits rather
than the population.

**Sequencing:** after the reader can display pages at all, since the feature is a
reading-flow setting and there is nothing to toggle before M5.

### Later

- **L1 geometry** — affine transform per pair, so a swap preserves zoom and pan.
- **L2 text regions** — bubble pairing by transformed centroid, which L1 makes
  arithmetic rather than a matching problem.
- **Art book linking**, if it is worth it — needs a different hash, and roughly one
  art book page in three has any manga counterpart at all.

## Corpus

Eleven series with both a raw and at least one English edition. Held on a NAS, never
committed, located at build time. Beyond the four the design started with, the
material now covers repagination at 2:1 and 4:1, editions published entirely as
joined spreads, letterboxed scans, archival TIFFs at 430 MB per page, three
independent scans of one volume, and at least two files that are not what their
names claim.

It now also holds **negative** material, which it lacked: `incoming/needles/`
keeps four short unrelated pairings that the solver accepts and should not. A
corpus of things that ought to work can only measure one half of a guard rail.

The awkward material is the valuable material. Every guard rail in the solver exists
because something in this corpus broke without it.
