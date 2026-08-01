# M3 — `StumpLibrary`, the composite source, and OPDS-PSE — design

**Status: built, with one named deviation.** Settled in brainstorming on
2026-07-31 and shipped the same day. This refines M3 of
[`2026-07-30-reader-design.md`](2026-07-30-reader-design.md), which sized it at
~1 week and described it as "`StumpLibrary` + OPDS-PSE".

Every figure below was measured against the live Stump instance on 2026-07-31, not
assumed. Where the reader design guessed and the measurement disagreed, this
document says so and the reader design is corrected rather than left standing.

> ## Deviation: `StumpLibrary` was not built, and should not be
>
> **This document names `StumpLibrary` throughout as a third `PageSource`
> implementation. It does not exist.** The mentions below are left as the record
> of what was designed; this note is what actually shipped.
>
> `PageSource` has **two** implementations — `LocalLibrary` and
> `CompositeSource` — and the Stump-backed branch lives inline inside the
> composite, holding the transport, the resolution map and the circuit-breaker
> latch in a `StumpSide` value.
>
> **Why.** A standalone Stump source cannot answer anything. Turning a member
> into a Stump book id requires the resolution map, and the map is the
> composite's. A `StumpLibrary` implementing the trait would have been a type
> that could only ever be called by the one caller that already had to own its
> state — nomenclature matching this document rather than an abstraction earning
> its place.
>
> **The seam this document actually cared about is intact**, which is the test
> that matters: nothing above `PageSource` knows where a page came from. The
> routes and the OPDS feed call only `ServeState::page`; the sole Stump-aware
> code above the trait is the composition root in `serve`. The whole-branch
> review verified this specifically.
>
> **What would change the answer.** A work whose members live *only* in Stump.
> That is blocked today by something deeper than the trait: `resolve_member`
> needs the member's local file, both for its size and for the page-hash
> comparison that makes a match evidence rather than a guess. So M3 offloads
> page reads for files already held; it does not serve a work taiyaku does not
> have. Serving one needs a different way to match a member to a book — its own
> design question — and at that point a standalone Stump source becomes
> load-bearing rather than cosmetic. Revisit it then, not before.

## Why this document exists

The reader design named one thing in M3 that could fail outright:

> A sidecar member is a `path` and a `sha256`; Stump addresses books by ID.
> `StumpLibrary` must reconcile them […] **If Stump exposes neither usably,
> `StumpLibrary` does not ship** — and that is the one thing in this design that
> could fail outright rather than merely cost more than expected. **It is checked
> first in M3, not last.**

It was checked first. **`StumpLibrary` ships.** Checking it also invalidated three
other premises, one of which reshapes the milestone.

## What the instance is

`stump.daveynet.xyz`, **Stump 0.1.4 nightly**, rev `c496517b`, compiled
2026-06-21. Behind Cloudflare. Eight libraries, all rooted at `/data`: `Comics`,
`Ecchi Manga`, `Game Art & Companions`, `Graphic Novels`, `Light Novels`,
`Literature`, `Manga`, `Science & Nonfiction`.

| Surface | Where | Notes |
| ------- | ----- | ----- |
| Whole API | `POST /api/graphql` | 401 JSON unauthenticated; `/api/graphql/ws` for subscriptions |
| Page bytes | `GET /api/v2/media/:id/page/:n` | **1-based**; `image/jpeg`; `cache-control: private,max-age=31536000`; no `ETag`; 404 past the end |
| Thumbnail | `GET /api/v2/media/:id/thumbnail` | `image/jpeg` |
| Whole archive | `GET /api/v2/media/:id/file` | `application/x-cbr` |
| Liveness | `GET /api/v2/ping` → `pong`, `POST /api/v2/version` | unauthenticated |
| OPDS | `/opds/v1.2/*` | Basic auth; **declares `xmlns:pse`** |

There is no `/api/v1`; those paths fall through to the SPA, which is what makes a
stale REST assumption look like a working server returning HTML.

## Four premises of the reader design, corrected

### 1. Stump is GraphQL, not REST

The reader design says `StumpLibrary` "proxies Stump's REST API, reusing its auth".
The SPA bundle contains exactly three API paths — `/api/graphql`, `/api/graphql/ws`,
and a versioned base — and every data operation goes through the first.

REST survives only for bytes: page images, thumbnails and whole files under
`/api/v2/media/:id/…`. So `StumpLibrary` speaks **two protocols, split by what
Stump offers**: GraphQL to resolve a member to a book id, REST to fetch the page.

### 2. There is no `sha256` fallback, because there is no usable hash

The reader design's mitigation was *"if paths do not reconcile, `sha256` is the
fallback."* `Media` does expose `hash` and `koreaderHash`. **Both are null on
every book measured**, across epub and cbz alike, and neither is a SHA-256 of the
file in the first place — `koreaderHash` is KOReader's partial MD5.

This is stated flatly because a later implementer will see a field called `hash`
beside a sidecar field called `sha256` and reach for it. They are not the same
quantity and the Stump one is, on this instance, absent.

### 3. Stump already serves OPDS-PSE

M3's stated end state is *"Panels reads the library; taiyaku runs beside Stump"*.
The first clause is already true without taiyaku: the catalog at
`/opds/v1.2/catalog` declares `xmlns:pse="http://vaemendis.net/opds-pse/ns"` and
Panels can read this library today.

So taiyaku's feed has to earn its place on something Stump cannot do, which is the
next item.

### 4. The reconciliation problem is not the one that was anticipated

Path reconciliation is **easier** than the design expected — `Query.mediaByPath` is
a direct lookup, so the "index Stump's book paths once and match" plan is
unnecessary. But coverage is **worse**, and that is what reshapes the milestone.

## The finding that reshapes M3: Stump holds half of each work

Measured across every edition directory of the primary development volume,
`corpus/gotoubun-v01`, by exact file size. Three of the five are recorded as
editions in that volume's `taiyaku.json`; the other two exist on disk but are not
currently members of the work:

| Edition | In the sidecar | Local bytes | Pages | In Stump |
| ------- | -------------- | ----------- | ----- | -------- |
| `en-mono` | ✅ | 334,752,036 | 187 | ✅ `/data/Manga/Go-Toubun no Hanayome/Go-Toubun no Hanayome - v01 (2018).cbz` |
| `raw-mono` | ✅ | 34,527,438 | 195 | ❌ |
| `en-fan` (7 files) | ✅ | 21,955,438 (c000) | 235 total | ❌ |
| `en-color` | ❌ | 139,053,825 | 199 | ✅ `/data/Manga/Go-Toubun no Hanayome - Full Color Edition/… v01 (2021).cbz` |
| `raw-color` | ❌ | 137,206,998 | — | ❌ |

Stump indexes the English releases. **The raws and fan translations — the material
taiyaku aligns English editions *against* — are not in it**, and a library of
English releases is what a Stump instance normally is.

Read against the sidecar rather than the directory, the result is sharper still:
**of the three editions this work actually has, Stump can serve exactly one.** A
whole-work source would leave two thirds of it unreadable.

The consequence is decisive: **`StumpLibrary` as a drop-in replacement for
`LocalLibrary` cannot serve a single taiyaku work**, because every work would 404
on its raw side. A trait implementation that serves nothing real is the
untested-guess problem M2 and M3 were split apart to avoid, merely relocated.

**Rejected: adding the raws to Stump as a library**, which would make
`StumpLibrary` a true replacement. Rejected on two grounds. It is not needed — the
composite below covers the case, and the equivalence test only needs a member that
exists in *both* sources, which `en-mono` already is. And it makes an existing
problem worse, which is the next section.

## Stump and Komga cannot express multiple editions of one work

Recorded because it is the owner's observation about running the library, and
because it is the argument that earns taiyaku's OPDS feed. Measured on this
instance:

```
/data/Manga/Go-Toubun no Hanayome                        14 books
/data/Manga/Go-Toubun no Hanayome - Full Color Edition   14 books
```

Two unrelated series, adjacent in the library, for one work. Add the raws and the
fan chapters and one work occupies four series with nothing relating them. Neither
Stump nor Komga has a concept that groups them, and **the shortcoming extends to
ebooks** — translations and revised editions of one novel have the same problem.

**The honest caveat:** taiyaku's L0 correspondence is *page*-based, so it does not
transfer to reflowable EPUB. The **grouping** problem generalises; the alignment
does not. Saying otherwise would overclaim.

## Decisions

### Pages come from a composite source, resolved per member

Chosen against a whole-work choice (Stump *or* local per work) and against adding
the raws to Stump.

```
routes.rs
  edition.locate(index) ──▶ (member, within)
  source.page(root, member, within)
     └── CompositeSource
           ├── StumpLibrary   resolved? ─▶ GET /api/v2/media/<id>/page/<within+1>
           └── LocalLibrary   ──────────▶ taiyaku_read::member_page(root/member.path, within)
```

A work stays readable when Stump knows only half of it, which is every work in
this library. `PageSource` gets its genuine second implementation, and the
composite is a third over the same trait — which is a stronger test of the seam
than two implementations would be.

### `PageSource` narrows to one member, and the arithmetic moves to `taiyaku-core`

The reader design carried this forward as a deferral to settle while planning M3:

> **Rule 1 is implemented where only one caller can reach it.** The index →
> (member, offset) walk lives inside `LocalLibrary` […] `Edition::total_pages` and
> `Edition::locate` in `taiyaku-core` would fix it […] at which point `page_count`
> leaves the trait entirely.

**It is not optional. It is a prerequisite.** The composite routes per member, and
the current trait takes a flat edition index and walks members inside the
implementation. The composite cannot be built against that shape. The deferral was
judged correctly and lands exactly as predicted — the second `PageSource` reshapes
the first.

```rust
// taiyaku-core — normative, tested, and where M4's filmstrip needs it anyway
impl Edition {
    fn total_pages(&self) -> u32;
    fn locate(&self, index: u32) -> Option<(usize, u32)>;   // (member, within)
}

// taiyaku-serve — one method, no arithmetic
trait PageSource {
    fn page(&self, root: &Path, member: &Member, within: u32) -> Result<ServedPage, String>;
}
```

This also removes the second derivation of a normative quantity:
`LocalLibrary::page_count` summed member pages a crate away from
`taiyaku_core::Edition::slots`.

### A member is matched by name and size, and confirmed by a page hash

Chosen against a configured path-prefix map, against name-and-size alone, and
against hashing Stump's whole copy.

```
1. candidates   media(filter: { _and: [ { name:  { eq: "<file stem>" } },
                                        { size:  { eq: <bytes> } },
                                        { pages: { eq: <member.pages> } } ] })
2. CONFIRM      sha256(stump page/1) == sha256(local page index 0)
3. cache        member path → { stump_id, confirmed_by, at }
```

**`Media.name` is the file stem with its extension stripped** — measured:
`Go-Toubun no Hanayome - v01 (2018)`, not `… .cbz`. Matching on the full basename
returns nothing, every time. `MediaFilterInput` exposes `name` and `extension` as
`FieldFilterString`, `size` as `NumericFilterI64` and `pages` as
`NumericFilterI32`, with `_and`/`_or`/`_not`, so all three predicates go in one
query rather than being filtered client-side. Verified against the live schema:
the query above returns exactly `a8ed51d8-c4c8-4efa-b427-2ba5327f477b`.

**Step 3 is the one that matters.** The corpus contains deliberate duplicate sets
and at least two files that are not what their names claim, which is exactly the
material name-and-size matching gets wrong — silently, by showing the wrong page.
Confirming with content is the discipline this project already applies everywhere
else, and confirming with *one page* costs about 1 MB rather than the 335 MB a
whole-file hash would cost on this volume, on a corpus that holds 430 MB pages.

A page hash also confirms the property that actually matters — that page *N*
agrees — rather than that two files resemble each other.

**Rejected: a configured path-prefix map** (`/mnt/nas/Manga` = `/data/Manga`),
which would let `mediaByPath` resolve exactly in one query with no heuristics. It
is exact where a sidecar sits beside the real library, and it cannot match the
corpus at all: `corpus/gotoubun-v01/en-mono/…` is a reorganised local copy sharing
no prefix with `/data/Manga/…`. A reconciliation that cannot run against the
development material is one that never gets tested. `mediaByPath` remains the
primitive used *after* a candidate is known.

### The mapping is cached beside the sidecar

Following the convention M2 established for progress:

```
taiyaku.json  ·  taiyaku.progress.json  ·  taiyaku.stump.json   ← new
```

`taiyaku.stump.json` records, per member, the resolved Stump id and the evidence
that confirmed it. **A member Stump does not have is recorded as absent too**, so
a miss costs one query once rather than one per page turn. `taiyaku.json` stays a
pure alignment document that diffs meaningfully in git, which is why neither
progress nor this lives inside it.

### taiyaku authenticates to Stump with a service API key

This answers an open question the reader design recorded — *"How does the client
authenticate to Stump […] pass through the user's session, or hold a service
credential."*

Stump exposes `createApiKey(input: ApikeyInput) → CreatedAPIKey { apiKey, secret }`
with per-key `permissions` and an optional `expiresAt`. taiyaku holds one
read-scoped key, sent as `Authorization: Bearer <secret>`, supplied by
`TAIYAKU_STUMP_KEY` or `--stump-key`.

**Session passthrough is rejected**, on two grounds rather than preference:
`taiyaku serve` resolves members when it loads a sidecar, before any reader has
authenticated; and the raws never involve Stump at all, so a reader's Stump
session cannot be what authorises reading a work.

### The OPDS tree groups editions under one work

```
/opds/v1.2/catalog                    navigation — root
  └─ /opds/v1.2/works                 navigation — one entry per work
      └─ /opds/v1.2/works/:work       acquisition — one entry per EDITION
           entry "raw-mono"  pse:count=195
           entry "en-mono"   pse:count=187
           entry "en-fan"    pse:count=235
           link rel="https://taiyaku.dev/rel/sidecar" ─▶ /api/works/:work
```

The work → edition grouping is the entire justification, given Stump already
serves PSE. It is the structure neither Stump nor Komga can represent.

**An edition streams as one publication even when it is seven files.** `en-fan` is
seven chapter archives and PSE sees `pse:count="235"` over a flat page space. That
works only because the reader design chose to address pages by flat index, and
this is where that decision earns its keep:

```xml
<link rel="http://vaemendis.net/opds-pse/stream"
      href="/opds/v1.2/works/gotoubun-v01/en-fan/pages/{pageNumber}"
      type="image/jpeg" pse:count="235"/>
```

**OPDS 1.2 rather than 2.0**, per `notes.md`: 1.2 is stable and universal, 2.0 is a
long-running draft, and neither expresses alternate editions, so an extension is
needed either way. The custom `rel` pointing at the sidecar is that extension.

### One off-by-one boundary, in one place, tested

**This section originally claimed two, and the second one was the bug.** It is
corrected here rather than rewritten, because how it was wrong is the useful
part: the reasoning was that a page-addressing scheme is either 0-based or
1-based per *protocol*, and PSE was assumed 1-based by analogy with Stump's REST
API. It is not. Only Stump's REST API converts.

| Boundary | Conversion |
| -------- | ---------- |
| taiyaku → Stump REST | `+1` on the page fetch |
| OPDS-PSE → taiyaku | **none — both count from 0** |

**Measured against Panels on 2026-07-31, in the first real session.** Panels
opens a book by requesting `pages/0`, retried it up to nine times per edition
against the 404 the 1-based version returned, showed an error screen, and only
advanced when tapped — at which point every page it displayed was the one
before the page it believed it was on, and the first page of every edition was
unreachable. Twenty of the session's thirty-nine requests were that 404.

**Stump settles it without re-reading the spec**, and is the better evidence
because Panels already works against it: its REST API is 1-based, and its
OPDS-PSE template is
`…/pages/{pageNumber}?zero_based=true` — it converts *for* PSE. A server whose
PSE stream is 1-based therefore disagrees with both the client and the other
implementation of the same feed.

**What let it ship.** Both conversion sites had tests, and both tests asserted
taiyaku's own convention rather than the protocol's — so the 1-based test passed
against 1-based code and proved only self-consistency. No test in the suite
could have caught this, and none did; the request log built as M3's second
deliverable found it on its first session, which is what that instrument is for.

Each lives in one function with a test pinning it. This is the class of defect
that shows a reader the wrong page while every `sha256` and page count in the
sidecar still matches — the same shape as the member-order bug, and nothing else
would notice.

### Covers now, filmstrip thumbnails in M5

Each OPDS entry needs a cover. M3 generates it from page 0 on demand and caches it
on disk.

The reader design's open question — *"Thumbnail caching […] Generating them per
request is likely too slow for a 195-page volume; where they are cached is
unspecified"* — is answered **for covers only**. The per-page filmstrip route
waits for M5, when its access pattern is known rather than guessed. One cover per
edition and 195 thumbnails per volume are different problems and should not be
solved together on a guess about the second.

### `spawn_blocking` at the router

The reader design's second carried-forward deferral, and M3 is where it stops
being theoretical:

> The handlers do blocking filesystem I/O on tokio workers […] It gets worse in
> M3, where `StumpLibrary` does blocking HTTP behind the same synchronous trait.

`spawn_blocking` in the route, as already decided. Keeping `PageSource`
synchronous stays right — the boundary belongs at the router, so neither
implementation nor the composite has to be async to be correct.

## Testing

### The third seam test lands here

The reader design names four tests that exist because they guard seams where two
things must agree and nothing else would notice. M2 landed one.

| Test | Status after M3 |
| ---- | --------------- |
| Serve/aligner agreement | landed in M2, 617 pages |
| **`PageSource` equivalence** | **M3** |
| Round-trip swap | M5 |
| WASM/native agreement | M4 |

**`PageSource` equivalence** compares `LocalLibrary` and `StumpLibrary` byte for
byte over every page of `en-mono` — 187 of them — since that member exists in both
sources. It is gated on `TAIYAKU_CORPUS` **and** a reachable Stump, skipping
cleanly when either is absent, matching the convention the two existing standing
checks use.

The reader design allowed for comparing `dhash` instead if Stump transcodes. It
does not: five pages already measured byte-identical, so the test compares bytes
and the `dhash` fallback is not built. If Stump ever starts transcoding, this test
is what says so.

**The measurement that justifies building it** — Stump book
`a8ed51d8-c4c8-4efa-b427-2ba5327f477b` against
`corpus/gotoubun-v01/en-mono/Go-Toubun no Hanayome - v01 (2018).cbz`, SHA-256
truncated to 32 hex characters:

| Stump | taiyaku | Bytes | SHA-256 |
| ----- | ------- | ----- | ------- |
| `page/1` | index 0 | 1,180,617 | `9183af35f1b47e7fbb9994aca642b8de` |
| `page/2` | index 1 | 1,343,956 | `98d412bdf8cb371d11f611418e7fc4cd` |
| `page/3` | index 2 | 339,050 | `91fd1f798337093345ad04944de9d25c` |
| `page/100` | index 99 | 1,884,398 | `acaf0c241e0826aafe4843b2a9214765` |
| `page/187` | index 186 | 978 | `eb60ee8901f45efecc62db4176fb685d` |

Stump derives the same page sequence taiyaku does, including sorting a lone `.png`
among `.jpg` entries at index 186 — and it served that one as `image/png`, so it
sniffs content rather than trusting the extension, as `taiyaku-serve` does.

**This is agreement, not guarantee.** Nothing binds Stump to taiyaku's natural-order
rule, which is normative in `sidecar.md` and merely conventional in Stump. The
test is the guard, and a mismatch must fail loudly rather than serve a page.

### The corpus gate gets fixed here

`status.md` records that a skipped standing check and a real one look identical,
because both print their skip reason to stderr and `check-all.sh` does not pass
`--nocapture`. M3 adds a third gated test and so makes the defect worse; the fix
belongs in the gate, in this milestone.

### Everything else

`taiyaku-core`'s two additions are new behaviour and are written test-first.
`taiyaku-serve` continues to drive routes through tower's `oneshot`, so no test
opens a socket. `StumpLibrary`'s GraphQL and REST calls are tested against
recorded responses rather than a live server, so the suite does not depend on the
NAS being up; the equivalence test is the one that talks to the real instance, and
it is gated.

## The second deliverable: performance targets from the request log

The reader design's argument stands — automated UI testing of Panels is a dead end,
so the request log is the instrument. M3 adds a tower middleware recording method,
path, status, bytes, duration and in-flight concurrency, and a script that
summarises prefetch depth, page-turn concurrency, requested image dimensions, and
re-request patterns that reveal cache policy. The numbers go into `notes.md`.

**Measured from an iPad running Panels over Tailscale**, not from a Mac: prefetch
depth and requested image size are tuned to the device, and the targets are meant
to describe the case a future iOS reader has to beat. Panels also runs on Apple
Silicon as a Designed-for-iPad app, and that run is recorded as a second column —
the delta between them separates Panels' behaviour from the device's.

**The log measures the performance half only, and that is a real limitation.** What
makes Panels good is also its reading interaction — gestures, page transitions,
zoom behaviour, how it handles a spread, what it does at a chapter boundary — and
none of that is visible in a request log. Capturing it needs someone using Panels
and writing down what it gets right. **That is a later task**, named here so it is
not mistaken for something M3 delivers, and it feeds M5's PWA as much as any future
Swift reader.

## Risks, named

1. **`Media.hash` is null everywhere and is not SHA-256.** Mitigated by confirming
   with a page hash. The risk is a future implementer seeing `hash` beside the
   sidecar's `sha256` and reaching for it.
2. **Stump's page ordering is agreement, not guarantee.** Five pages matched;
   nothing binds Stump to the rule. The equivalence test is the guard.
3. **Blocking HTTP behind a synchronous trait.** Addressed by `spawn_blocking`,
   but it is the deferral that got worse, and a page fetch over the network stalls
   a worker for longer than a local read does.
4. **Cloudflare sits in front of the dev instance.** taiyaku should reach Stump
   over the tailnet or LAN. Per-page fetches and 335 MB archives through a public
   proxy is the wrong path, and `ImageRef.url` already emits an internal
   `http://…:10801` URL that does not match the public one — so the base URL is
   configured, never taken from a response.
5. **Coverage is a moving target.** Which members Stump knows changes when the
   owner adds or reorganises a library. The cache records absence as well as
   presence, so a stale "absent" outlives a newly added book until the cache is
   invalidated. Cheap to clear; worth stating that nothing watches for it.

## Shape of the work

Sized properly in the implementation plan, not here.

1. `taiyaku-core` — `Edition::total_pages` and `Edition::locate`, test-first
2. `taiyaku-serve` — narrow `PageSource` to one member; `LocalLibrary` follows
3. `taiyaku-serve` — `StumpLibrary`: GraphQL resolve, REST bytes, API key
4. `taiyaku-serve` — reconciliation and the `taiyaku.stump.json` cache
5. `taiyaku-serve` — `CompositeSource`, and `spawn_blocking` at the router
6. `taiyaku-serve` — OPDS 1.2 + PSE tree, and covers
7. `taiyaku-serve` — request-log middleware
8. gated test — `PageSource` equivalence, and the `--nocapture` gate fix
9. measure — Panels on iPad, targets into `notes.md`

## What this leaves open

- **Whose progress is it under `StumpLibrary`?** Still open, but now concrete
  rather than abstract. Stump records progress per user as
  `ResumeReadingCursor { page, percentageCompleted, epubcfi, … }` and mutates it
  with `updateMediaProgress`; taiyaku records `(work, edition, slot)`. Reading
  through taiyaku would move taiyaku's and not Stump's. Two systems recording
  where you were is a conflict, and which one wins is unspecified. It is not
  blocking: nothing in M3 writes to Stump.
- **Thumbnail caching for the filmstrip.** Answered for covers, deferred for the
  195-per-volume case until M5 knows the access pattern.
- **Whether corrections need an undo.** Unchanged, M6.
- **What Panels gets right about reading**, as opposed to fetching. A later task,
  above.
