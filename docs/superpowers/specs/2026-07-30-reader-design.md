# The reader — design

**Status: design, nothing built.** Settled in brainstorming on 2026-07-30. Every
decision below records why it was taken and what it cost, in the same style as
`notes.md`; where a figure is asserted it is a measurement already recorded
there, and where something is a guess it says so.

The reader is the multi-week piece the rest of the project has been unblocking.
`docs/status.md` has called it "next" since the aligner worked; its two stated
blockers — no way to write a sidecar with three editions, no way to resolve
through one — were cleared on 2026-07-30.

## What this is

A desktop-first browser reader that shows one manga page and swaps it for the
same page in another edition on a keypress, plus the server that feeds it and
the correction UI that fixes what the aligner got wrong.

It exists to prove the sidecar is useful. The README's principle stands: *the
bespoke reader here exists to prove the format is useful, not to become the only
way to use it.*

## Scope

**In:** `taiyaku serve` (own API and OPDS-PSE), a React PWA that reads and swaps,
and a correction UI. The correction UI is in scope because the design calls it
part of the core rather than a follow-on — *"an alignment you cannot inspect or
correct is worse than none, because it silently shows you the wrong page and you
never find out."*

**Out, and named so the boundary is deliberate:**

- **Replacing Stump.** taiyaku runs *beside* Stump first. The architecture keeps
  replacement reachable without committing to it — see `PageSource` below.
- **Library management** — scanning, filesystem watching, users, permissions,
  metadata providers. Stump does this and it is the tedious 90%.
- **EPUB, PDF, CBR.** taiyaku reads zip-based archives and directories today.
- **L1 geometry and L2 text regions.** The format is forward-compatible for
  both; the reader consumes L0 and ignores what it does not understand.
- **A native iOS reader.** Designed for, not built — see the constraints below.

## Decisions

Each was chosen against alternatives during brainstorming. The rejected option
is recorded because the reason it lost is the useful part.

### Page images come from `taiyaku serve`, not from the browser

Rejected: a PWA reading local files through the File System Access API, which is
Chromium-only and would mean reimplementing archive reading in the browser.

Rejected as the *first* target: a client of an existing Komga or Stump instance.
`notes.md` calls this Tier 1 and says it "needs nobody's permission" — true, but
it assumes an instance already exists, and there is none here. It returns as
`StumpLibrary` below rather than being lost.

### Desktop browser first

Keyboard-driven. The README's promise is *"press a key and the page in front of
you becomes the original"*, which is a desk. A phone-first reader would want a
sheet rather than a keypress, and the correction UI wants screen space and a
pointer.

Phone remains reachable — the layout is responsive — but is not tuned for.

### Resolution runs in the browser, in WASM

Three architectures were compared: the WASM core, a server-side `/resolve`
endpoint, and a TypeScript reimplementation of the run arithmetic.

The WASM core wins on `notes.md`'s own reasoning — *"resolving a page against an
existing alignment is arithmetic over a handful of runs. That belongs on the
client"* — and on the adoption argument: *"a Komga (Vue/TS) or Stump (React/TS)
frontend can import the module and get alignment lookup without reimplementing
the format."*

A server endpoint costs a round-trip on the one interaction that must feel
instant, and makes offline impossible. A TypeScript reimplementation means two
implementations of `compose`, `invert`, slot expansion and `trust`, of which only
one has the corpus behind it. That is the same class of defect as the member-order
bug fixed on 2026-07-30: a rule held in one place and assumed in another.

**Cost, stated plainly:** `taiyaku-core` takes `wasm-bindgen`, its first
dependency beyond `serde`, and the crate's manifest carries a standing warning
about dependencies that must keep the wasm target building. `wasm-bindgen` is
wasm-native, so it is the safe kind, but the warning is there for a reason.

### React 19 + Vite + TypeScript, not Leptos

The owner's other web projects are `cargo leptos`, and full-Rust would be faster
for them to write. React was chosen anyway, on one argument: **Stump is Axum +
React/TS, and the WASM binding is the adoption lever.**

A Leptos reader takes `taiyaku-core` as a plain Rust dependency and therefore
never builds the TS binding at all — it would be written later, with no consumer,
against no real use. Building the reader in React makes the binding load-bearing
from day one, and the same module serves Komga's Vue frontend.

**The risk, named:** if contributing to Stump never happens, React bought nothing
and cost several weeks outside the preferred stack.

### Swap is a toggle key plus number keys

```
Space   flip between the current edition and the previous one
1..9    jump directly to edition N
```

Rejected: cycling through editions in order, which `notes.md` already doubted —
with five editions it takes up to four presses and you lose track of where you
are. Rejected: a hold-to-open picker, which shows you what you are switching to
but costs the one-key promise on every swap.

The toggle is the alt-tab model: the common back-and-forth stays one key, and the
gesture does not change as editions are added.

### All three reading modes, with regroup in two-page mode

Single page, two-page spread, and continuous scroll. Two-page is right-page-first
under `rtl`.

The `spreads` list earns its keep here: a joined spread is one wide image and the
reader shows it as one tile rather than splitting it wrongly.

**Regroup** handles the case two-page mode always hits — one unpaired page throws
the parity off for everything after it. See "Parity breaks" below.

## Architecture

### Repo layout

```
crates/
  taiyaku-core     unchanged — format, resolve, solve. no I/O
  taiyaku-read     NEW  — archives, directories, natural order, page bytes
  taiyaku-cli      align / verify / topology / lookup / serve
  taiyaku-serve    NEW  — axum: sidecar, pages, thumbnails, OPDS-PSE, corrections
  taiyaku-wasm     NEW  — wasm-bindgen over core, emits a TS package
web/               NEW  — React 19 + Vite + TS
```

**`taiyaku-read` is an extraction, not new code.** `pages.rs` and `naming.rs` are
library code living in a binary crate, and `serve` is a second consumer. Moving
them keeps the page-sequence rules — made normative in `sidecar.md` on
2026-07-30 — in exactly one place, which is the property that whole change was
about. The alternative is `serve` duplicating them, which is the bug that change
existed to prevent.

`taiyaku serve` stays the command and calls into `taiyaku-serve`. The cost is
that the CLI binary links axum and tokio; a second binary was judged worse UX.

### `PageSource`: work with Stump now, replace it later if ever

```rust
trait PageSource {
    fn page(&self, edition: &Edition, index: u32) -> Result<Vec<u8>, Error>;
    fn page_count(&self, edition: &Edition) -> Result<u32, Error>;
}
```

| Implementation | Pages from | What it buys |
| -------------- | ---------- | ------------ |
| `LocalLibrary` | filesystem, via `taiyaku-read` | standalone; the replacement path |
| `StumpLibrary` | Stump's REST API, reusing its auth | Stump keeps scanning, metadata and users; taiyaku adds only alignment |

Nothing above the trait knows the difference. This is `notes.md`'s Tier 2 —
*"alignment endpoint: serve the sidecar, resolve lookups"* — reached without
patching Stump, and it makes the replace-or-not decision permanently reversible.

**The named risk.** A sidecar member is a `path` and a `sha256`; Stump addresses
books by ID. `StumpLibrary` must reconcile them, most plausibly by indexing
Stump's book paths once and matching, since both point at the same NAS. If paths
do not reconcile, `sha256` is the fallback. **If Stump exposes neither usably,
`StumpLibrary` does not ship** — and that is the one thing in this design that
could fail outright rather than merely cost more than expected. It is checked
first in M3, not last.

### The serve API

```
GET   /api/works                                works this server knows
GET   /api/works/:work                          the sidecar, verbatim
GET   /api/works/:work/:edition/pages/:n        page image
GET   /api/works/:work/:edition/pages/:n/thumb  thumbnail, for the filmstrip
PATCH /api/works/:work/alignments               write a corrected run
GET   /api/works/:work/progress                 where you were
PUT   /api/works/:work/progress                 { edition, slot }
```

Plus an OPDS 1.2 + OPDS-PSE tree, so standard clients read the library.

Two decisions inside:

- **Pages are addressed by flat index**, not `member` and `entry`. Serve derives
  index → (member, entry) using the rules in `sidecar.md`. The spec earns its keep
  immediately, and any third-party server implementing these routes agrees with
  this one by construction rather than by care.
- **Serve never sends half-pages.** A joined spread goes out whole and the client
  crops it, since the client has `spreads`. One fetch, one cache entry, both
  halves.

### The WASM binding

Shaped by what `taiyaku-core` already returns rather than invented:

```ts
const sc = Sidecar.parse(json)
sc.editions()             // [{ key, label, pages, spreads }]
sc.route(from, to)        // Route | null   — fetched once per pair, cached
route.resolveSlot(slot)   // { page, half } | null
route.resolve(page)       // { page, half } | null   — first slot of that page
route.via                 // "direct" | "reversed" | "composed"
sc.trust(from, to)        // "sound" | "fragmented" | "refused"
```

`Route` is the unit rather than a per-page `resolve(from, to, page)` call because
`core::route()` already returns one and it carries the runs. A swap is then local
arithmetic over a handful of runs — never a call, never a fetch. That is what
makes the toggle key feel instant, and it is exactly the module Stump or Komga
would import.

**`resolveSlot` does not exist yet and `taiyaku-core` must gain it.** The current
`Route::resolve` takes a *page* and returns the first slot of its counterpart,
which cannot express "I am on the left half of a joined spread" — and the reading
model below tracks position in slots precisely so that it can. The addition is
small rather than awkward: `Route` already holds its runs in slot indices, so
`resolve_slot` is the direct operation and the existing `resolve` is a
page-convenience wrapper over it. `via` is a public field on `Route`, exposed as a
getter by the binding.

### What `taiyaku-core` gains

Two additions, both written test-first:

| Addition | For |
| -------- | --- |
| `Route::resolve_slot` | slot-accurate position, so a spread half survives a swap |
| run splitting for corrections | the correction UI, and any other reader that writes back |

## The reading model

### Position is one authority, not a per-edition cursor

`notes.md` records the trap under "swapping must not round-trip through the
alignment": swap onto a page with no counterpart, land on the nearest, swap back,
and you have silently moved one page — compounding on every swap.

The note proposes a per-edition anchor. A smaller model has the same property:

```
state = { authority: edition, slot: number, viewing: edition }

display  =  viewing === authority
              ? slot
              : route(authority, viewing).resolveSlot(slot)
swap     =  set viewing.     authority untouched.
navigate =  authority := viewing, slot := new position.
```

`slot` is a slot index throughout, never a page number, which is why
`resolve_slot` has to exist: a reader stopped on the left half of a joined spread
must swap to the counterpart of *that half*, not to the start of the page.

Round-tripping is lossless **by construction** — a swap never writes position, so
there is nothing to drift. This is the note's "an anchor that advances only on
real navigation" reduced to a single authoritative pair.

### A swap onto a gap succeeds and says so

`notes.md` leaves this open and leans one way: *"probably the third, since it is
the only one that doesn't lie."* Taken.

The reader shows a no-counterpart state naming the edition and page, offering the
nearest paired slots either side as explicit moves. Not a toast, not a silent
jump to the nearest page.

This falls out of the authority rule: because a swap never moves authority, **one
press of the toggle returns you exactly where you were**. The gap state is a
view, not a position. Taking an offered move is real navigation, so authority
moves — visibly, and only because it was asked for.

**The verdict travels with the swap.** The twenty-two needles established that
acceptance is a statement about calibration and only the verdict is a statement
about the alignment. So swapping into a `FRAGMENTED` pair says so, rather than
presenting it as a `sound` one.

### Parity breaks are a reading preference, not alignment data

Two-page mode pairs slots `(0,1), (2,3), …`. A break at slot *k* restarts pairing
there. Per edition, since editions differ in which pages are missing.

- **Seeded from the format.** A joined spread occupies two slots and displays as
  one wide tile, so breaks around it are derived rather than typed.
- **One key inserts or removes a break at the current page**, and the filmstrip
  re-lays-out live so the effect is visible immediately.
- **Stored client-side, never in `taiyaku.json`.** The sidecar describes
  correspondence; pairing parity is layout, varies by reading mode, and would be
  noise in a file meant to diff meaningfully in git.

### Progress is server-side, and recorded in slots

Where you were is stored by `serve`, not in the browser: it has to survive a
different machine, and a progress tracker needs to read it. Recorded as
`(work, edition, slot)` — the edition you were last reading in and the slot you
reached, which is exactly the authority pair above.

**Slots rather than pages, deliberately.** A slot is the addressed unit
everywhere else in the format, and recording a page would lose which half of a
joined spread was reached. Deriving a chapter number from a slot later is a pure
function over the alignment and needs no schema change — see "Progress sync"
below.

Parity breaks stay client-side. They are layout, not position.

### The filmstrip does three jobs

A thumbnail strip along the bottom, laid out **as pages will be paired** — pairs
read as pairs, joined spreads as one wide tile.

| Job | Interaction |
| --- | --- |
| Navigate | click a tile to jump |
| Regroup | insert or remove a parity break; layout updates live |
| Correct | switches to the counterpart edition, current resolution highlighted |

The third job is the one Stump's preview does not do, and it is why this is worth
building rather than reusing a scrubber.

### Correcting, and the composed-pair problem

Pick the right counterpart in the strip, the client computes a run split, `PATCH`
to serve, serve rewrites `taiyaku.json` with `by: "hand"`.

**The run-splitting arithmetic goes in `taiyaku-core`**, not the client. It is
format logic, it needs tests, and any other reader would need the same function.
New API, written test-first.

The hard case is correcting a pair that resolved **composed through the hub**:
there is no stored edge to edit. **A correction on a composed pair writes a direct
edge**, seeded from `compose()` with the fix applied. This is principled rather
than expedient — `notes.md` permits direct edges "for pairs where composition
demonstrably loses", and a hand correction is exactly that demonstration. Both
`compose` and `--direct` already exist.

The cost: that pair leaves the star and no longer inherits transitivity by
construction. The format already sanctions this trade.

## Testing

Ordinary unit testing throughout, plus four tests that exist because they guard
seams where two things must agree and nothing else would notice.

| Test | Property | Catches |
| ---- | -------- | ------- |
| **Round-trip** | swap A→B→A returns to the starting slot, every slot, every pair in the corpus sidecar | the `notes.md` gotcha — silently moving one page per swap |
| **Serve/aligner agreement** | page *N* from `serve` is byte-identical to page *N* from `read_edition` | a server deriving the page sequence differently from the aligner |
| **WASM/native agreement** | `resolve` through the binding equals `resolve` in Rust, over the corpus sidecar | drift in the binding, which would make "one implementation" false |
| **`PageSource` equivalence** | `LocalLibrary` and `StumpLibrary` return the same page for the same edition and index | a swap showing different pages depending on where the server got them |

The round-trip test is pure arithmetic over the authority model, so it is
exhaustive rather than sampled. It is the bug a reader finds by losing their
place three chapters later, which is the worst way to find it.

`PageSource` equivalence compares bytes where Stump serves originals. If Stump
transcodes, it compares `dhash` instead — taiyaku already has a hasher whose
whole job is content identity that survives re-encoding, so the existing
machinery answers the new question.

Per layer:

- **`taiyaku-core`** — as now. Run-splitting is new behaviour, so test-driven.
- **`taiyaku-read`** — the existing `pages.rs` tests move with the code, which is
  what makes the extraction behaviour-preserving rather than hoped-to-be.
- **`taiyaku-serve`** — axum's `oneshot` harness, no network.
- **`web/`** — Vitest for the state machine, which is all pure functions:
  authority, swap, gap, parity breaks. Playwright for the reading flow.

## Milestones

| | What | Size | End state |
| --- | --- | --- | --- |
| **M1** | `taiyaku-read` extraction | half a day | nothing new; tests prove nothing broke |
| **M2** | `taiyaku-serve` + `PageSource` + `LocalLibrary` + progress | ~3 days | pages over HTTP from disk |
| **M3** | `StumpLibrary` + OPDS-PSE | ~1 week | **Panels reads the library; taiyaku runs beside Stump** |
| **M4** | `Route::resolve_slot`, `taiyaku-wasm` + agreement test | ~2 days | resolve a slot from JS |
| **M5** | Reading PWA: modes, swap, gaps, parity, filmstrip | ~2 weeks | **the swap works** |
| **M6** | Correction UI | ~1–2 weeks | fix a wrong pairing and keep it |

M2 and M3 are separate deliberately: a trait with one implementation is an
untested guess, and the second implementation arriving promptly is what stops
`PageSource` being shaped around its only caller.

M3 earning something real before any React exists is also deliberate. If the PWA
stalls, the work still produced a server the owner's preferred iOS app can read.

**Sizes are estimates, not measurements.** Every session on this project so far
has found something the previous estimate did not anticipate.

## Measuring against Panels, without automating it

The bar for any future iOS reader is Panels. Automated UI testing of Panels is
mostly a dead end: App Store apps are device builds and FairPlay-encrypted, so
the iOS Simulator cannot install them, and driving a third-party app on a device
through XCUITest is fragile and Apple-discouraged.

**The request log is the instrument instead.** Once M3 serves OPDS-PSE, Panels
talks to this server, and everything that makes it feel fast is visible in what
it asks for and when: prefetch depth, request concurrency on a page turn, whether
it requests reduced-size images and at what dimensions, re-request patterns that
reveal its cache policy, and how it walks a feed while browsing.

**M3 therefore has a second deliverable: performance targets derived from that
log**, written into `notes.md`. That turns "as good as Panels" from an impression
into numbers, in the same style as the rest of the project, and gives a later
Swift reader something to beat.

## Designed for a native iOS reader, not building one

`taiyaku-core` is portable Rust with no I/O, so the same crate reaches Swift via
UniFFI or swift-bridge — the path Firefox iOS and the Matrix Rust SDK both take.
`taiyaku-read` ports the same way for local archive reading.

Three constraints keep that reachable. They are discipline, not effort:

1. `taiyaku-core`'s public API takes no `wasm-bindgen` types; the binding lives
   entirely in `taiyaku-wasm`.
2. `serve` stays REST and JSON, with no browser-specific session assumptions.
3. No browser-only types in `taiyaku-read`.

This is also the retroactive justification for putting resolution in the core
rather than in TypeScript: a Swift client would otherwise need a *third*
implementation of the format's trickiest logic. A native app additionally escapes
the iOS offline-storage limits the original design doc named as the eventual
reason to go native.

## Progress sync to AniList: designed for, deliberately not built here

`notes.md` already ruled on this under "An alignment is an edition-identity
oracle", and the ruling stands:

> **Not a sync tool.** OAuth, rate limits, progress-conflict rules and tracking a
> third-party GraphQL schema are ongoing maintenance with no novelty in them […]
> Building another one trades the interesting half for the tedious half.
>
> **Sequencing: after the reader exists, not before.**

**One premise of that ruling has changed.** It assumed taiyaku had no reader, so
progress would come from Komga or Stump and taiyaku would only emit identity. The
reader means taiyaku now *has* progress of its own, and something has to carry it.
That makes sync reachable, not urgent — the sequencing rule is unaffected, and it
is a milestone after M6, not inside it.

**Split the interesting half from the tedious half**, the same way `PageSource`
splits them:

| Half | Where | Why |
| ---- | ----- | --- |
| slot → chapter number | `taiyaku-core` | novel, pure, testable against the corpus |
| AniList GraphQL, OAuth, rate limits, conflict rules | its own crate, behind a `ProgressTarget` trait | tedious, and "maybe others later" becomes an impl rather than a rewrite |

**The first half is the part nothing else can do.** `notes.md` records why:

> The second thing that breaks tracker sync is units: AniList counts chapters, a
> volume-organised library counts volumes. Aligning a volume edition against a
> chapter edition *is* the mapping.

And that it beats declared metadata on the corpus — Go-Toubun volume 1 is fan
chapters c001–c005 **plus c005.5**, not the c000–c004 the page arithmetic
suggested; Kaguya's fan release labels two files `v1` and is wrong both times. A
metadata-derived chapter map is wrong in both cases and the content-derived one
is right in both.

So a reader that knows you are on slot 88 of a volume edition can say which
chapter that is, correctly, where every name-matching tool guesses. That is worth
building. The GraphQL client is not novel and should be quarantined behind a trait
so it can be replaced or dropped without touching anything else.

**What this design commits to now:** nothing beyond recording progress in slots,
which costs nothing and keeps the chapter derivation a later pure function.

## Open questions

Recorded rather than resolved, because none blocks M1.

- **Which work does a URL name?** `/api/works/:work` needs an identifier. The
  sidecar has a title but no id, and titles are not unique or stable.
- **How does the client authenticate to Stump** under `StumpLibrary` — pass
  through the user's session, or hold a service credential.
- **Thumbnail caching.** The filmstrip wants a thumbnail per page of the current
  edition. Generating them per request is likely too slow for a 195-page volume;
  where they are cached is unspecified.
- **Whether corrections need an undo.** `by: "hand"` is recorded but nothing
  reverts a correction short of editing the JSON.
- **Whose progress is it under `StumpLibrary`?** Stump tracks progress per user
  already. Two systems recording where you were is a conflict waiting to happen,
  and which one wins is unspecified.
