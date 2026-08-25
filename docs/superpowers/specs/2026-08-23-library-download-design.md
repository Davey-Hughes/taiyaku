# The library's download — design

**Status: built, with two named deviations.** Settled in brainstorming on
2026-08-23 and built the same branch (`library-download`): both routes under
`/api/works`, the plan and stream passes, and the generated `ComicInfo.xml`
all shipped as designed. §2's third route — the OPDS-namespace mirror at
`/opds/v1.2/works/{work}/{edition}/download` — and §8's web UI tile control
did not; both are deferred to a second plan (§9's corrected comment and
`docs/status.md`'s `taiyaku-serve` entry record why).

Written in the style of the rest of `docs/`: every decision records the
alternative it beat and what it cost, a figure asserted here was measured on
this machine rather than recalled, and a guess says it is one.

This is **spec C2** of the roadmap in `2026-08-14-library-store-design.md` §10,
and it fills in that document's §13.1. It depends on **A2** alone — the guard,
for the per-user permission — but it ships after **C1** and takes C1's browse UI
as the place a person clicks, so in practice it depends on both.

**What it is for is stated in §13.1 and worth repeating, because it is the whole
reason this spec is not "nice to have": there is no download route today, the
OPDS acquisition link points at a cover image, and the consequence — which the
documented deviation at `opds.rs::work_feed`'s doc comment did not state — is that offline
reading in Panels does not work.** Panels is the client that made this
project's grouping visible to a real reader in the first place. A catalogue it
cannot take on a train is a catalogue with a hole in it.

## 1. Four corrections to §13.1 before anything is built

§13.1 was written as an audit note, not a design, and three of its sentences do
not survive contact with the code. They are corrected here rather than quietly
worked around, because two of them were load-bearing.

**§13.1 says `Content-Length` is exact "because a `STORE` zip's size is a
deterministic function of its entry names and sizes, both of which the sidecar
already records". The sidecar records neither.** `Member` carries `path`,
`sha256` and `pages` and nothing else (`core/sidecar.rs:202`); the `member`
table carries the same three plus `format` (`schema/001_initial.sql:52`). No
per-page entry name and no per-page byte size is stored anywhere in this
project. The conclusion is still true and the reason is different: the sizes are
**recoverable at request time from each member archive's central directory**,
without reading a byte of page data, and `taiyaku_read::member_page_sizes`
(`read/pages.rs:577`) already does exactly that. §4 below is what replaces the
false premise.

**§13.1 says the pages "are already JPEGs", so a CBZ of them uses `STORE`. The
first half is right and the second does not follow the way it reads.** Measured
on `corpus/` on 2026-08-23 across all 140 archives: **11,431 of 11,681 entries
are `DEFLATE`** and only 250 are `STORE`; only **4 of 140** archives are
dominantly stored. So a `STORE` output is not a copy of what is on disk — it is
a decompress of 97.9% of entries followed by a store. `STORE` is still the right
output, for the two reasons §13.1 gives (an exact length before the first byte,
and therefore working ranges), but "generation is a copy" is not one of them,
and a rebuilt CBZ is *slightly larger* than the source it came from because
`DEFLATE` on a JPEG does buy a percent or two.

**§13.1 says a member downloads as "its original bytes — unchanged, proxied
from Stump where Stump holds it", while §10 of the same document says the Stump
page source is a migration path and "nothing should build on it either".** The
owner settled this on 2026-08-23: **C2 does not touch Stump.** The download
path reads the local filesystem. A member that lives only on Stump has no
local bytes and fails the way any absent file fails. The project is departing
from Stump; a download route built on it would be work with a known expiry
date.

**§13.1 says "both paths need a per-user download permission, which is A2's" and
stops there.** A2 resolved *two* flags, `can_download` and `can_stream`, and
both are the open half of **O54** — reported by `/api/me`, governing no route
(`serve/auth.rs:257`). This spec closes O54 by reading both. §2 says where the
line between them falls, which A2 never had to decide.

## 2. Routes, and why there are two namespaces

```
GET /api/works/{slug}/{edition}/download                    the edition, one CBZ
GET /api/works/{slug}/{edition}/members/{ordinal}/download   one member, verbatim
GET /opds/v1.2/works/{work}/{edition}/download              the edition, one CBZ
```

**Under `/api/works` rather than `/api/library`**, beside the `/pages/{index}`
and `/thumb` routes that already address an edition (`routes.rs:1480`). C1 put
*browse* under `/api/library` because browse is a different question — what
exists, and for whom. A download is the same question `/pages` answers at a
different granularity, asked of the same `(slug, edition)` pair, and answered
after the same visibility check.

**`{ordinal}` is `member.ordinal`, counting from 0** — the position in the
document, which is what `import.rs:774` writes and what `Edition::locate`
returns as its first element. A member is not addressed by its path: a path is
long, contains separators, and is the field `import` reconciles on, so a URL
built from one would change under a rescan that moved a file the identity rule
deliberately followed.

**The OPDS mirror exists because of `auth.rs::refuse`, not for symmetry.** A 401
on `/opds` carries `WWW-Authenticate: Basic realm="taiyaku"` and a 401 on `/api`
deliberately does not — because that header makes a browser raise its own native
auth dialog over the reader, which the web app cannot style, pre-fill or recover
from. An OPDS client following an acquisition link into `/api/…` cold would
receive a bare 401 and never prompt for credentials. `stream_page`
(`opds.rs:238`) already duplicates the page route into the OPDS namespace for
exactly this reason and the download follows it. **One handler, two
registrations** — the OPDS route is not a second implementation.

There is no member download in the OPDS namespace. OPDS asks for a publication;
a chapter file inside one is not something a feed has an entry for.

### 2.1 Two refusals that are two different facts

The guards run in this order, and the order is the design:

1. **`refuse_if_hidden`** (`routes.rs:391`), C1's root grant, answers **404**
   `no work {slug}`. An edition outside the grant does not admit that it
   exists. This is C1's decision, unchanged, and reusing the same helper is
   what stops the download becoming the third place that spells the rule out.
2. **`can_download`** answers **403**. This is a different fact and gets a
   different code: the caller may see this edition, is not being told it does
   not exist, and may not have a copy of it. A 404 here would be a lie that
   also breaks the UI's ability to explain itself.

### 2.2 Where `can_stream` falls, which A2 left open

`can_stream` gates **`/api/works/{slug}/{edition}/pages/{index}`** and the OPDS
PSE stream **`opds.rs::stream_page`**. It does **not** gate `/thumb`, the
library covers, or any browse route.

The line is drawn there because a download-only account must still be able to
*see* its shelf. An account that cannot render a cover cannot choose what to
download, and "you may take volumes away but not read them here" is a coherent
policy while "you may take volumes away but not see which ones exist" is not.
Thumbnails are how the shelf renders, so they follow browse.

Both columns default to `1` (`schema/004_auth.sql:18`), so **no existing account
changes behaviour on the day this ships.** Nothing can set either flag to 0
yet — that is C4's user administration — which is worth stating plainly: this
spec makes two flags load-bearing before anything can exercise them outside a
fixture. That is the same trade C1 made deliberately with
`library user grant-root`, and it is why §10's guard tests set the columns
directly rather than waiting for a UI.

## 3. Two body shapes behind one route

**Passthrough** when the edition has exactly one member *and* that member is a
file. Its original bytes are served, byte for byte, with `Content-Length` from
`stat` and real ranges from a seek.

**Generated** otherwise — a multi-member edition, or a single member that is a
directory.

**Passthrough was chosen by the owner on 2026-08-23 over always rebuilding**,
and it is the common case rather than a corner: measured on `corpus/` the same
day, **31 of the 47** directories that hold page-bearing archives hold exactly
one (~66%). Two-thirds of archive-backed editions therefore cost a `sendfile`
and no CPU at all, against a decompress-and-restore of every page.

**What passthrough gives up, measured rather than hand-waved.** A rebuilt CBZ
flattens nested page paths into zero-padded flat names; a passthrough one keeps
whatever the source had. `docs/sidecar.md:326` measures **2 of the 7** archives
that nest pages in folders as sorting differently by basename than by whole
path — Go-Toubun's Full Colour edition, which carries a `ZBlank (1).jpg` in
every chapter folder, so a client sorting basenames interleaves the chapters
into a shuffle rather than an off-by-one. Rebuilding fixes that for such a
client; passthrough cannot. It is a *client* defect either way — CBZ has no
page-order field, so every reader recovers order by sorting entry names, and the
whole-path rule is the one taiyaku states — but it is a defect this route could
have papered over and now does not. Two of seven nesting archives, and seven
nesting archives out of 57, is the size of it.

The second thing passthrough gives up is `ComicInfo.xml`: §6's generated
document is not injected into a file being served verbatim. A source archive's
own `ComicInfo.xml` travels unchanged — 12 of 57 carry one
(`docs/sidecar.md:327`) — and the other 45 arrive without metadata.

Both costs are accepted. The bytes on a passthrough download are provably the
bytes on disk, which is a property worth more than a metadata document a client
may not read.

## 4. The plan pass, which is what makes the length exact

Before a byte is sent, the generated path computes the complete output layout.
Nothing here reads page data for an archive member.

**Per member, in `docs/sidecar.md`'s normative page order** — members in
document order, pages in natural order within each member, non-images excluded,
whole path not basename:

- **Archive member.** The central directory yields, per entry, its name, its
  **uncompressed size** and its **crc32**. All three are what a `STORE` local
  header needs. This is one file open and one seek to the end of the file per
  member; no entry is decompressed and no page byte is read.
- **Directory member.** `walk` + `metadata` gives the size; the crc32 does not
  exist anywhere and the file is read once to compute it.

**A new `taiyaku_read::member_page_entries(path) -> Result<Vec<PageEntry>,
String>`** returns `{ name, size, crc32 }`. It carries the error contract of the
functions beside it unchanged, including the total-not-fallible name decode: an
entry name that is not valid UTF-8 arrives garbled rather than refused.

**It is an addition, not a refactor of the two functions next to it, and that
correction was found while planning the code.** Folding `member_page_names`
(`read/pages.rs:412`) and `member_page_sizes` (`:577`) onto it looks like
removing a duplicate traversal and is really a performance regression on two
paths that deliberately do not read bytes. `member_pages` — which `align` and the
scanner both reach — is `member_page_names().len()`, so a directory member would
start hashing every page to be counted; and `member_page_sizes`'s own doc comment
states the property that would be destroyed: *"A listing, not a read — the
property that lets an inventory run over a NAS and decide what is worth
copying."* `taiyaku-cli`'s `signature.rs:51` is the caller relying on it. What
the three genuinely share is `directory_page_paths`, `page_names_from` and
`open_archive`, and they already do; three thin functions over those helpers is
the right shape, and the second entry in `docs/notes.md` for this branch is why.

**A second addition, for the stream pass:
`for_each_member_page(path, f) -> Result<(), String>`**, calling `f(&PageEntry,
&[u8])` per page in reading order. One archive open per member and one page in
memory at a time. Without it the stream pass would reach for `member_page`,
which opens the archive afresh for every index — 200 opens for a 200-page
member, against the one that `/pages` pays per request because a request is one
page. Handing the entry back alongside the bytes also lets the stream check the
member against the plan as it goes, so an archive replaced mid-download is
detected rather than silently producing a zip whose central directory lies.

**CRC from the central directory was chosen over data descriptors.** Setting the
streaming bit and writing crc and size *after* each payload would need no
planning read at all and would keep the layout exact (a data descriptor is a
fixed 16 bytes, 24 under zip64). It was rejected because some older CBZ readers
mishandle descriptor-flagged entries, and this is the one artefact taiyaku
produces that leaves the server: it fails on a device, offline, where nothing
reports back. The cost of the conservative choice lands on directory-backed
members — the only ones read twice — and they are the minority: `status.md`'s
on-disk measurement of 2026-08-18 found 121 directories holding page images
directly with 107 of them suppressed as nested, leaving ~14 directory members
against 140 archives, about 9%. A mixed output — descriptors for directory
entries only — was rejected in turn because it makes the shape of the file
depend on how a member happens to be stored, which is not a distinction a
downloaded volume should carry.

**Output entry names are zero-padded and sequential across the whole edition**,
width taken from the total page count, extension taken from the source entry:
`0001.jpg`, `0002.png`, … So **name order equals reading order**, a client's own
name sort agrees with what the app shows, and the "stored order differs from
name order in 32 of 57 cbz files" trap (`docs/sidecar.md:325`) cannot be
entered — the stored order *is* the name order.

**Every page is included**, `page_flag` spreads and credits alike. A credit page
is kept out of the *reading flow* by a heuristic a reader can overrule; an
archive that silently omitted pages would be a different volume from the one on
disk.

**The layout, and therefore the length.** With names and sizes known, the output
is arithmetic:

```
per entry:   local file header (30 + name length) + payload
after all:   central directory record (46 + name length) per entry
             end of central directory (22)
```

`ComicInfo.xml` is generated first, so it is an entry like any other with a
known length and crc. `Content-Length` is the sum, exact, before the first
byte — which is what makes §13.1's `Range` claim reachable at all.

**zip64.** Classic records when every entry and the total fit in 32 bits, zip64
extra fields otherwise. Both layouts are deterministic, so the length stays
exact either way. **Nothing in this corpus reaches the branch**: measured
2026-08-23, the largest single archive is 1.49 GB
(`corpus/incoming/nausicaa/en-deluxe-v01.cbz`), the largest summed group is the
same file alone, and **0 of 47** groups exceed 4 GiB. Always emitting zip64 was
rejected — it would put the compatibility risk of the whole feature into the
case that always happens — and refusing over 4 GiB was rejected because it
fails on exactly the volumes someone is most likely to want offline. §10 says
what the zip64 branch is and is not tested against.

## 5. The stream pass

**Corrected after the code was written — the shipped reasoning is better than
this section's first draft, so the draft takes the correction.** A dedicated
`std::thread`, not a `spawn_blocking` closure, writes into a **bounded**
`tokio::sync::mpsc` of `Bytes`, and the response body is built from the
receiver. Backpressure is the channel's; a 1.49 GB volume is never in memory.
The zip-decompression-stalls-a-worker reasoning `get_page` records at
`routes.rs:736` (drifted from `:529` since this section was first written —
re-anchored here rather than left to rot again) still motivates keeping this
off an `.await`-able task, but
does not by itself justify `spawn_blocking`: tokio's blocking pool is sized
for a worker that returns in milliseconds, and a producer thread here holds
its slot for as long as the download runs — minutes, for a slow connection on
a large volume — which would starve the pool for every other blocking call in
the process. `download.rs::produce`'s own doc comment states this; §9's
correction for the OPDS comment is the same kind of fix, prose brought back in
line with code that had already moved past it.

**Two more corrections, both additions the whole-branch review made after this
section settled, neither contradicting anything above.** The generated path's
payload is chunked at a fixed 64 KiB (`download.rs::CHUNK_SIZE`) the same way
passthrough's file reads already were, rather than queued as whole pages —
this corpus's largest page is ~430 MB, so a channel bounded in *pages* bounded
memory by page count while its actual size varied by two orders of magnitude
across pages, which chunking fixes by bounding it in bytes instead. And
`download.rs::DOWNLOAD_LIMIT`, a `tokio::sync::Semaphore` capping concurrent
downloads at four (`MAX_CONCURRENT_DOWNLOADS`), bounds how many producer
threads — and the memory each one's in-flight page can hold — may run at
once; the module's own doc comment states this is a reasoned starting point,
not a measured one, for the same reason true-offset seeking below is
deferred: nothing in this repository measures concurrent-download behaviour
yet.

**A third correction, later still: an idle send timeout, first declined and
then built.** `DOWNLOAD_LIMIT` bounds how many producer threads a stalled
client can occupy; it does not reclaim one. A client that stops reading — not
merely a slow one — leaves hyper polling nothing under TCP backpressure, and
the producer thread used to park in `blocking_send` forever, holding both its
buffers and the permit `DOWNLOAD_LIMIT` gave it. Before that semaphore
existed, such a client cost only its own thread and memory; after it, four
such clients hold every permit and every other reader's download blocks
behind them — the cap turned a per-connection leak into a whole-server
outage at exactly the threshold meant to bound memory. A timeout for this was
considered beside the cap and declined once, for a reason that was sound:
nothing in this repository measures how long a legitimate client — Panels,
on mobile data, mid-tower-handoff — can go without accepting a chunk, and a
timeout tuned from a guess risked severing a real download mid-volume, a
worse failure than the leak it would fix. **What changed is not that
judgement — a guessed number is still dangerous — but the means of arriving
at one.** `download.rs::IDLE_SEND_TIMEOUT` (ten minutes) is derived from the
module's own 64 KiB chunk size rather than from any measured stall: a client
that accepts one chunk every `T` seconds is reading at `65536 / T` bytes/sec,
and at `T` = 600 s that is under 110 bytes/sec sustained — not a slow reader
on any link this feed serves, a dead connection. Ten minutes clears any
plausible legitimate stall by a wide margin without needing to know its real
duration, which is what makes arithmetic sufficient where a measurement (the
same Panels session log with timing this section's true-offset-seeking
decision below is still waiting on) remains absent. The timeout is idle,
reset by every accepted send (`download.rs::send_with_timeout`), so it cannot
cap how long a whole download takes — a legitimate multi-hour transfer over a
slow link is unaffected — and it applies to both body shapes and both
routes: `passthrough` and `generated`, `edition` and `member`, through the
one helper, so a client that goes silent is abandoned the same way and logged
the same way — "the one thing a stream cannot do", below — regardless of
which produced the bytes it stopped reading.

**`Accept-Ranges: bytes` on both paths.** Passthrough seeks the file.
**The generated stream honours a range by producing from zero and discarding the
prefix** — correct, one code path, and wasteful only on an actual resume, which
is the rare event rather than the common one. A 206 carries `Content-Range` and
the exact partial length, both of which §4's arithmetic already knows.

True offset→(entry, offset-within-entry) seeking is the obvious better version
and is **deliberately deferred**, because the contract does not change when it
arrives: `Accept-Ranges` already says `bytes` and the response is already
correct, so it is a pure optimisation. What would trigger it is a measurement,
not a preference — a fresh Panels session log showing Panels actually issues
`Range`. **The existing logs cannot answer that**: `panels.jsonl` and
`panels2.jsonl` predate request-header capture, so they report `(not recorded)`
for headers, and reading their silence as "no ranges" would be reading an
instrument's blind spot as data. §12 opens the item.

**The one thing a stream cannot do, stated rather than discovered.** Once the
body has begun, a failure — a member deleted mid-download, an unreadable entry,
a disk error — cannot become a status code. The only honest behaviour is to
close the stream; the client sees fewer bytes than the `Content-Length`
promised, and that mismatch is what tells it the download is bad. The route
therefore does everything it can fail at in the plan pass, which is also where
a missing member is cheapest to notice, so the common failures are 404s before
the first byte rather than truncations after it.

## 6. `ComicInfo.xml`, generated path only

Built from the library's own rows, one element per fact that exists:

| Element | Source |
| --- | --- |
| `Series` | the primary `series_title` row, falling back to `series.sort_title` |
| `Number` | `work.volume`, integral values without the `.0` the REAL column carries |
| `Title` | `work.title` |
| `PageCount` | the edition's total pages |
| `LanguageISO` | `edition.language` |
| `Writer` / `Penciller` / `Translator` / `Publisher` | `credit` joined to `person`, by `role` |
| `Notes` | the edition label |

**`LanguageISO` comes from a column, not from the edition key.** `edition.language`
is a real field (`schema/001_initial.sql:47`) that the rule engine's
`set_language` writes (`schema/003_scanner.sql:76`) and that `import` preserves
across a reconcile (`import.rs:761`). Deriving a language from a key like `raw`
or `en` would be a guess presented as metadata, which is the mistake `work.root_id`
was rejected for in C1.

**An absent fact is an absent element, never an empty one.** A `<Writer/>` with
nothing in it is a claim that the writer is unknown; leaving it out is a claim
about nothing. And most credit fields *will* be absent: `credit` has no writer
until C3's metadata editing ships (O44 is that there is no route to set it), so
in practice today's downloads carry series, number, title, page count and label.
That is stated here rather than found out later.

## 7. The filename

`Go-Toubun no Hanayome - v01 (raw).cbz` — series, volume, edition label.

- No series (a hand-imported work has none: `import` never writes `series_id`)
  falls back to the work title.
- No volume drops the ` - vNN` segment entirely.
- An integral `work.volume` prints as `v01`; a half-volume prints as `v01.5`.
- `Content-Disposition: attachment` with an ASCII-transliterated `filename=`
  **and** an RFC 6266 `filename*=UTF-8''…`, because this library's titles are
  routinely non-ASCII and a client that only understands one of the two forms
  must still get something usable.
- Path separators, control characters and leading dots are stripped. A title
  that arrived from a filename can contain anything a filename can.

`Content-Type: application/vnd.comicbook+zip` on the **generated** path, which
is the registered type and what an OPDS client matches its acquisition link
against.

**A passthrough download's type and extension follow the bytes, not the route.**
`member.format` admits `epub-fixed`, `epub-reflow` and `pdf` beside `zip` and
`dir`, so a single-member edition is not necessarily a comic archive at all —
and serving a PDF as `application/vnd.comicbook+zip` under a `.cbz` name would
be a lie told twice, in the header and in the filename. The passthrough type
comes from the member's own extension by the same table `source.rs::type_from_extension`
already uses for pages, and the constructed name in §7 keeps the member's real
extension rather than `.cbz`. The name is still `Series - v01 (raw)`; only the
suffix is the file's own.

## 8. The UI, and what the UI is not

**There is no work page.** `/api/library/works/{slug}` powers `AlignmentPanel`
inside the series view (`web/src/Series.tsx:143`), and the panel is itself a
`browseModule` preference that can be `off`, `button` or `on-select`. A download
placed inside it would be a download some readers have switched off.

So: **one control per volume tile, downloading the edition the tile is already
showing** — `shown ?? w.editions[0]` (`Series.tsx:359`), the same expression the
tile's cover uses. A reader asking "which edition am I getting" is answered by
the edition selector the page already has, and a volume with three editions gets
one control rather than three. The `table` layout gets the same control as a
column.

**A plain `<a download href=…>`, never a `fetch`.** An anchor navigation carries
the session cookie, so it is authenticated the same way every other browser
request is; and the browser's own download manager takes the file, where a
`fetch`-to-blob would buffer 1.49 GB in the tab. This is also why the route
must work as a top-level navigation rather than only as an XHR.

**Absent entirely when `can_download` is false.** `/api/me` already reports the
flag and `web/src/api.ts:105` already types it, so the control is conditional on
data the shell has held since A2. Absent rather than disabled: a disabled button
invites a reader to find out why, and there is nothing on this screen that can
tell them.

The panel gains nothing. Per-member download has a route and no UI — a reader
wants the volume, not chapter 3 of it — and that route exists for direct callers
and for the case where a member is the unit somebody actually needs.

## 9. OPDS, and a documented deviation that stops being true

`opds.rs::work_feed`'s doc comment records, as a deliberate decision by the owner, that the
acquisition link points at the edition's first page because "taiyaku does not
serve a whole publication a client could download". **That sentence becomes
false on this branch**, so the comment is rewritten rather than left standing —
a stale justification is worse than none, because it argues against the code
beside it.

- `rel="http://opds-spec.org/acquisition"` → the new
  `/opds/v1.2/works/{work}/{edition}/download`, with
  `type="application/vnd.comicbook+zip"`.
- The PSE link (`rel="http://vaemendis.net/opds-pse/stream"`) is **untouched**.
  Streaming stays the in-app read path; acquisition becomes the offline one.
  They are two mechanisms for two different intentions and always were — the
  deviation existed only because one of them had no implementation.

## 10. Testing

**The layout arithmetic is checked against a real reader, not against itself.**
`zip = "7.2.0"` is already a dev-dependency of `taiyaku-serve`, so the generated
bytes are parsed back: entry names, entry order, uncompressed sizes, crc32s, and
that the total byte count equals the `Content-Length` the route promised. A
test that only re-derived the arithmetic would pass on a wrong constant.

**Range by whole-versus-parts equivalence.** Fetch an edition whole; fetch it in
N randomly-chosen ranges; assert the concatenation is byte-identical and that
each 206 carries the right `Content-Range`. This is the test that catches an
off-by-one in the discard path, which is the likeliest defect in §5.

**zip64 is tested as arithmetic and not as a stream, and the spec says so.**
The layout function is unit-tested against entry sizes above 4 GiB; no test
streams 4 GiB, because generating that much material to prove a header field is
a test that would be deleted the first time it made the suite slow. The branch
is therefore *reachable in theory and unexercised against real material* — the
corpus measurement in §4 is the evidence for how unlikely it is, not for its
correctness.

**Passthrough is tested as identity.** A single-member edition's download must
be `stat`-length bytes equal to the file on disk, and must *not* acquire a
`ComicInfo.xml`. The second half is the assertion that stops someone
"improving" passthrough into a rebuild.

**Corpus standing checks** (`TAIYAKU_CORPUS`, gaining two): download one real
multi-member edition and one real single-member one, re-read both with the `zip`
crate, and assert the page count matches the edition's.

**The largest archive in the corpus is not reachable from these checks, and
that is worth stating rather than leaving as an implied claim.**
`corpus/incoming/nausicaa/en-deluxe-v01.cbz` is the 1.49 GB outlier §4 measures,
and it has no `taiyaku.json` beside it — so `works::load_works` cannot build a
`Work` for it and no route can serve it. The largest archive these checks
actually exercise is `gotoubun-v01/en-mono` at 334.75 MB. The passthrough path
is therefore proven against a third of a gigabyte of real DEFLATE material and
not against the corpus's true worst case; a figure quoted from §4 is a figure
about the corpus, not about what the suite covers. This is the check that
fails when a real archive does something a fixture does not — and a fixture in
which the right answer and the wrong one render identically is the one shape of
test that cannot catch any of this.

**Guards.** `auth::tests::every_route_refuses_an_anonymous_request` derives its
count from `guarded_routes` and `open_routes`, so the three new routes arrive
covered without anyone remembering to add them (`routes.rs::guarded_routes`, line 1475). What needs
writing is the pair the enumeration cannot express: `can_download = 0` gets 403
from both download routes while still getting 200 from `/thumb` and the browse
routes; `can_stream = 0` gets 403 from `/pages` and from the PSE stream while
still getting 200 from the download. Both set the columns directly, because
nothing else can until C4.

**Web.** The control is present, points at the right `(slug, edition)` for both
`chosen === 'all'` and a chosen key, is absent under `can_download: false`, and
is absent on a tile whose edition the grant hides (`className="without"`).

## 11. What is deliberately not done

- **No cache and no single-flight.** §13.1's reasoning holds and is the good
  half of that section: generation streams, so there is no first-request block,
  no eviction policy and no disk cost. `cover.rs`'s `Condvar` is not needed
  here.
- **No whole-work or whole-series download.** A work is several editions of the
  same volume; a zip of all of them is a thing nobody asked for. A series is
  tens of gigabytes.
- **No Stump proxy** (§1).
- **No per-member UI** (§8).
- **No true offset seeking for generated ranges** (§5), pending a measurement.
- **No writing metadata into the archives.** §13.2's opt-in `ComicInfo.xml`
  write-back is the one feature that modifies a reader's library, and it belongs
  to C3's metadata editing, not to a read-only download route.

## 12. Open items this moves

- **O54 closes.** Both `can_download` and `can_stream` are read by a route.
  The `#[expect(dead_code)]` pattern A2 used for `roots` is the model: an
  attribute that fails the build when the lint stops firing, so it cannot
  linger.
- **One item opens**: generated-range resume reads and discards the prefix
  rather than seeking to it. Its trigger is named and is a measurement — a
  fresh `--log-requests` session against Panels showing `Range` on an
  acquisition fetch. Not a defect on evidence; a known cost with a known fix.
- `serve/opds.rs`'s acquisition-link comment is rewritten (§9).
- `docs/status.md` takes its five-part edit for O54's closure, plus a row and bullet for
  the new item, plus the count-word sweep on the way out.
- `docs/notes.md` takes the four measurements this document asserts: the
  DEFLATE/STORE entry split, the single-archive-directory proportion, the
  largest archive and group, and the directory-member proportion.

## 13. Branch shape

One branch, one squashed PR. Ordered so the risky claim is tested first:

1. `taiyaku_read::member_page_entries` and `for_each_member_page`, beside
   `member_page_names` and `member_page_sizes` rather than replacing them (§4).
   Unit tests, including a garbled name and a directory member.
2. The layout function — names and sizes in, byte offsets and total out —
   classic and zip64, unit-tested against a real `zip` parse.
3. The generated stream: blocking producer, bounded channel, body. Whole-file
   equivalence.
4. Ranges on both paths, whole-versus-parts.
5. `ComicInfo.xml` and the filename.
6. The three routes, the two guards, and the refusal tests.
7. OPDS acquisition link and the comment rewrite.
8. The tile control and its tests.
9. The two corpus standing checks; `docs/notes.md`; `docs/status.md`.

## 14. What could not be measured

- **Whether Panels issues `Range` on an acquisition link.** §5. The instrument
  exists (`taiyaku serve --log-requests`, bound to the LAN); the session has
  not been taken. Everything about ranges in this spec is built to be correct
  without knowing the answer, and the optimisation is held back until it is
  known.
- **Whether a real CBZ reader on a phone accepts the generated container —
  measured 2026-08-25, and it does.** §10's `zip`-crate round trip proves only
  that the output is a well-formed zip, which is a weaker claim, so this was
  taken on a device. `gotoubun-v01`'s `en-fan` edition — seven chapter
  archives, 235 pages, DEFLATE sources — was downloaded from a live `serve`
  and opened in **Panels on an iPad**. It reads. The same file opens in a
  desktop CBZ reader, `unzip -t` reports no errors, and a page-by-page
  comparison against the seven source archives found all 235 pages
  byte-identical and in the right order across every chapter join, `c005.5`
  after `c005` included. **The reading order was verified programmatically,
  not by eye in Panels**, which is a weaker check than a person turning the
  pages and is recorded as such.

  **Two things that measurement cost more to learn than the answer did.**
  Panels' *loose-file* import refused the generated CBZ — and refused an
  untouched original corpus archive in exactly the same way, which is what a
  control is for: the container was never implicated. What works is adding a
  **directory as a library**. And the route a device would really use is OPDS
  acquisition, which this spec defers (§9), so the path Panels is actually
  built for still cannot be exercised. Until that link is repointed, a
  download reaches a device only by hand.

  **A browser cannot reach these routes at all, by design, and that is worth
  stating where someone will look for it.** `/api` answers 401 with no
  `WWW-Authenticate` — spec A2's deliberate choice, so the browser never raises
  its own dialog over the reader — so a plain navigation to a download URL
  looks like a connection failure, and inline `user:pass@host` credentials do
  not rescue it because Safari and Chrome strip them from top-level
  navigations.
- **What a rebuilt edition costs in wall-clock on the NAS.** The corpus
  measurement says two-thirds of archive-backed editions never rebuild at all,
  and a rebuild is bounded by a decompress of the volume — but this workstation
  mounts the library over nfs4 and the server will run on the NAS, so a figure
  taken here would be a figure about the wrong host. The same correction §10 of
  the store design records about the watcher applies.
