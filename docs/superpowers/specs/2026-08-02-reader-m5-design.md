# The reader, M5 — design

**Status: settled in brainstorming on 2026-08-02. Nothing built.** This document
supersedes the **M5** section of
[`2026-07-30-reader-design.md`](2026-07-30-reader-design.md) and folds that
document's **M6** into the same plan. Everything else it settles stands
unchanged and is not restated here.

Written in the style of the rest of `docs/`: every decision records the
alternative it beat and what it cost, a figure asserted here is a measurement
already recorded in `notes.md`, and a guess says it is one.

## What carries over unchanged

Named so the boundary between the two documents is deliberate rather than left
to whoever reads them next.

- React 19 + Vite + TypeScript, chosen on the adoption argument.
- Resolution runs in the browser, in WASM, via `taiyaku-wasm`.
- `PageSource`, and the composite that routes per member.
- The authority model — `{ authority, slot, viewing }`, a swap never writes
  position, so round-tripping is lossless by construction.
- Swap is `Space` to toggle and `1`..`9` to jump. **This was re-settled on
  2026-08-01 and `docs/status.md` line 1001 is stale in saying it is
  undecided**; that line should be corrected when this lands.
- Parity breaks are client-side layout, seeded from `spreads`.
- The filmstrip does three jobs: navigate, regroup, correct.
- A swap onto a gap succeeds and says so. The verdict travels with the swap.
- Progress is server-side and recorded in slots.

## Scope

**In, and built in this plan:**

- The page-view core: three reading modes, swap, the gap state, the chrome
  shell.
- The module system.
- Six modules: `regroup`, `direction`, `motion`, `prefetch`, `readouts`,
  `annotations`.
- The correction UI, which the earlier document scheduled as M6.
- O13, fixed where it is tractable and refused where it is not.
- `Edition::chapter_at`, a new `taiyaku-core` function, as the `readouts`
  module's third line.

**Designed here, built later:**

- `fit` and `filters` — geometry overrides and pixel filters. The *defaults*
  are core and need no toggle, so only the overrides wait.

**Out, and its own brainstorm → spec → plan cycle:**

- **The text layer** — highlightable text, word search, dictionary lookup.
  This is OCR, and it is a subsystem rather than a feature. See "The text
  layer" below for what M5 does to keep the door open.

**Out, unchanged from the earlier document:** replacing Stump, library
management, EPUB/PDF/CBR, L1 geometry, L2 text regions, a native iOS reader,
AniList sync.

## Decisions

### The text layer is a separate subsystem, and M5 pays one cheap premium for it

Highlight-to-copy, word search and dictionary lookup were requested together.
All three need a text layer, and **taiyaku has none**: pages are images, the
sidecar carries no text, and neither the core, the server nor the binding has
any notion of one. The nearest thing already on the roadmap is L2 text regions,
which is about *pairing* bubbles between editions rather than reading them.

Cross-language makes it larger rather than smaller — vertical Japanese and
English are different recognizers, and a dictionary lookup for Japanese needs
tokenisation and deinflection that an English one does not.

Rejected: a first cut inside M5, on the grounds that it would delay a reader
that can display a page at all, and that this project's habit is to measure
before designing around a result.

**The premium M5 pays is one design decision, not one line of code.** An
annotation is anchored to a *region of a slot* rather than to a slot, so the
same anchor carries an OCR'd text box when that subsystem lands. Deciding this
now is free; retrofitting it would mean migrating stored notes.

### Modules are declarative, not self-registering

The request was that features be individually switchable and tunable. Three
mechanisms were compared.

| Mechanism | Verdict |
| --------- | ------- |
| Build-time feature flags | Rejected — turning a feature off means a rebuild, which is not what "I can turn them off" means |
| Lazy-loaded chunks | Rejected *for now* — buys bundle size against a problem nobody has measured, and this project measures first |
| **Runtime toggles, one bundle** | **Chosen** |

Code-splitting later is a Vite configuration change rather than an architecture
change, so the cheap choice does not foreclose the expensive one.

**Declarative rather than self-registering is the part that matters.** A module
that registers itself on load cannot be enumerated while it is disabled, so the
settings panel would need hand-written knowledge of every module — which is
precisely the coupling "modular" exists to remove. Declaring also turns a
keybinding collision into a startup error instead of two modules silently
fighting over one key.

### Notes travel through the alignment; their regions do not

A note written on slot 88 of the raw appears, by default, while reading the
English — resolved through `Route::resolve_slot`, the same call the swap makes,
so no new arithmetic exists to disagree with the old.

This is the one feature on the list that no other reader can offer, and it is
the sidecar earning its keep a second time.

**What does not travel is the region, and saying so is the honest half.**
Transforming a region between editions correctly is L1 geometry — an affine
transform per pair — which is a *Later* item. So a travelled note renders as a
margin tab at the resolved slot and **never as a tinted box**. Drawing the box
at an untransformed position would put it over the wrong panel, which is the
reader lying about what it knows; this project's whole discipline is against
that.

Rejected: edition-local notes that never travel. Simplest model, and it makes
your notes on the raw invisible while reading the English, which for a
bilingual reader is most of the reason to annotate.

**A note whose slot has no counterpart in the viewing edition does not
disappear.** It surfaces in the review list marked unreachable from here,
reusing the no-counterpart treatment already specified for swap. Silence is the
failure this project is written against.

### Notes live beside the sidecar, not inside it

`<stem>.notes.json`, in the sidecar's directory, the same placement `progress`
already uses.

Rejected: inside `taiyaku.json`. The earlier document's own rule is that the
sidecar describes correspondence and must diff meaningfully in git; human prose
would be noise in it, and it would put notes into a file that re-solving
rewrites — the same hazard O13 describes for hand corrections.

Rejected: client-side, in IndexedDB. Notes must survive a different machine and
must be exportable, and a browser profile is neither.

### The chrome hides entirely, and the page fits whole

Reading shows the page and nothing else. The filmstrip **floats** — lifted off
the bottom edge rather than docked to it — so it overlays the page and
summoning it never reflows what you are reading. The status readouts sit on the
bottom edge beneath it, and **strip and readouts appear and disappear
together**.

Considered and rejected: a permanently visible status line (costs page height
on the one axis a portrait page cannot spare); a side rail in the margin a
portrait page already wastes (best use of space, but two-page mode wants that
width back, and a layout that changes with reading mode is more to build and
more to explain).

**The cost is stated because it was raised twice during brainstorming and
chosen anyway:** four status readouts that hide by default are readouts you
check rather than readouts you know.

**The page fits whole.** Aspect preserved, downscaled to fit, **never cropped,
never distorted, never force-filled, and not upscaled past 100%**. The
letterbox around it is background.

Every geometry override lives in the `fit` module and is off by default:

| Setting | Default | Does |
| ------- | ------- | ---- |
| `trim` | off | autocrop to the inked area |
| `fillWidth` / `fillHeight` | off | fills one axis, cropping the other |
| `upscaleToFit` | off | lets a page smaller than the viewport grow past 100% |
| `upscaleFilter` | `smooth` | interpolation used when upscaling |

`upscaleToFit` earns its own setting rather than falling out of "fit": a
low-resolution scan on a 4K monitor is a real case in this corpus, and the
alternative is a small page marooned in a large letterbox. **It carries
`upscaleFilter` because manga is line art**, and bilinear upscaling of inked
line work reads as mush where nearest-neighbour keeps the line crisp at the cost
of visible stair-stepping. Which is better is a per-scan judgement rather than a
global one, so it is exposed rather than chosen here. Both settings are inert
while `upscaleToFit` is off.

**`notes.md` rejects autocrop and this does not overturn it.** That rejection is
about *hashing*: autocropping to the inked area destabilised the aligner's
fingerprints. Trimming for *display* is a different operation with a different
consumer, and the corpus argues for offering it — Tongari v09 is the letterboxed
case, 176 of 180 slots once trimmed symmetrically. Both facts belong next to
each other so the next reader does not think one contradicts the other.

`fit` is geometry and `filters` is pixels; they are separate modules despite
arriving as one request, which also means `fit` can move into this plan on its
own if trimming turns out to be wanted early.

### Margin tabs while reading, regions on demand

Because no chrome persists, a note must announce itself on the page or be
invisible.

Chosen: **small tabs in the margin** at each note's vertical position, so
nothing is ever drawn over artwork, plus **one key that tints every annotated
region at once**. The reveal is a `transform`, so it is a module toggle like
everything else.

Rejected: always-tinted regions (intrusive on a busy page). Rejected: a corner
badge counting notes (quietest and cheapest, but it discards the anchor
visually — storing regions that are never shown).

Fit-whole guarantees the side margins the tabs live in on every portrait page,
and because regions are stored as fractions of the slot, tabs and tints track
correctly at any fit scale without extra arithmetic.

### O13 is fixed where it is tractable and refused where it is not

The correction UI is what produces a `Source::Hand`, so shipping it makes O13
live: `merge_spoke` currently drops the stored edge wholesale on a re-solve.

`status.md` records that the fix divides, and this design takes that division
literally:

| Merge case | Behaviour |
| ---------- | --------- |
| Spoke `sha256` unchanged | Hand runs carry across; the merge reports how many it carried |
| Spoke replaced, hand runs present | **Refused**, naming the corrections at risk and how many |
| Spoke replaced, no hand runs | Proceeds exactly as today |

The third row is the one that keeps this a narrowing rather than a breaking
change: re-solving a spoke after fixing its archive stays ordinary, and only the
case that would destroy a correction silently is refused.

Rejected: deciding now what becomes of a correction addressing bytes that no
longer exist. `status.md` deliberately leaves that open, and answering it
without a real case in hand is the kind of guess this project refuses
elsewhere.

Rejected: shipping the correction UI with O13 open. The earlier document's own
words about the adjacent lost-update hole apply — "you would have to notice a
fix had vanished to know anything happened."

## Architecture

### The module contract

```ts
interface ReaderModule<P> {
  id: string
  label: string
  defaults: P                 // preference slice; the settings panel is generated from it
  keys?:      KeyBinding[]    // declared, so collisions are a startup error
  surface?:   Surface         // one of a small fixed set of mount points
  transform?: PageTransform   // applied to the render pipeline in declared order
  status?:    StatusReadout[] // lines contributed to the readout row
}
```

A module uses the extension points it needs and no others. Generating the
settings panel from `defaults` is what makes "tunable" free per module rather
than hand-built UI seven times.

| Module | `defaults` | `keys` | `surface` | `transform` | `status` |
| ------ | ---------- | ------ | --------- | ----------- | -------- |
| `regroup` | breaks per edition | insert / remove | filmstrip layout | — | — |
| `direction` | override per edition | toggle | — | page order | — |
| `motion` | animations on/off, duration, page-change speed | — | — | transition | — |
| `prefetch` | depth ahead / behind, decode cap | — | — | — | — |
| `readouts` | which of the four show | — | readout row | — | all four |
| `annotations` | markers, travel, reveal | new / reveal / review | margin + overlay | tabs, region tint | note count |
| `fit` *(later)* | trim, fill, upscale-to-fit, upscale filter | — | settings only | geometry | — |
| `filters` *(later)* | per-filter parameters | — | settings only | canvas / WebGL | — |

Eight rows against one interface is the evidence the interface is the right
shape. If a ninth needs a new extension point, that is a signal to revisit
rather than to widen quietly.

### Where state lives

The rule is the earlier document's, applied rather than reinvented: **position
and content are server-side** because they must survive a different machine;
**layout and preference are client-side**, the precedent parity breaks set.

| State | Where |
| ----- | ----- |
| Progress `(work, edition, slot)` | Server — as built |
| **Annotations** | **Server**, `<stem>.notes.json` |
| Corrections | Server, in `taiyaku.json` as `by: "hand"` |
| Parity breaks | Client |
| Module toggles, animation speed, filter and fit parameters, marker style | Client |
| Reading-direction override | Client, per edition |

A direction override never writes back to the sidecar. `reading` is a fact about
the edition; an override is a preference about the person.

### The notes file

```json
{
  "work": "go-toubun-v01",
  "notes": [
    {
      "id": "01J8…",
      "edition": "raw",
      "slot": 88,
      "region": { "x": 0.12, "y": 0.34, "w": 0.28, "h": 0.09 },
      "body": "…",
      "created": "2026-08-02T14:03:11Z",
      "updated": "2026-08-02T14:03:11Z"
    }
  ]
}
```

- **Regions are fractions of the slot, never pixels.** A re-scan, or the same
  edition served at another resolution, would break a pixel anchor; fractions
  survive both, and they make the marker arithmetic scale-free.
- **Anchored to a slot, not a page** — the M4 lesson. A note on the left half of
  a joined spread must not silently become a note on the whole spread.
- **`region` is optional.** Absent means the whole slot: a degenerate case
  rather than a second kind of note.
- **ULID ids**, so ordering is free and two devices writing independently do not
  collide.
- **Written atomically**, temp file plus rename, with the temp name **unique per
  attempt** — annotations are exactly as multi-device as progress, which is the
  reasoning O2 already recorded for `progress::store`.
- **`If-Match` on write**, the same validator the correction path needs, for the
  same lost-update reason.

Two routes, shaped like the ones that exist:

```
GET   /api/works/:work/notes
PUT   /api/works/:work/notes        If-Match required
```

### The render pipeline

One ordered pipeline, so "which module wins" is answerable by reading a list
rather than by tracing React renders:

```
page bytes → fit (geometry) → filters (pixels) → decode → double-buffered swap
                                                        ↑
                                    annotation overlay (tabs, tints) composited above
```

The overlay composites above rather than inside, so no module can move a note's
marker away from the region it names.

## The eight requests, mapped

| # | Request | Where it lands |
| --- | ------- | -------------- |
| 1, 1b, 1c | Highlightable text, word search, dictionary lookup | Own spec cycle. M5 pays the region-anchor premium |
| 2 | Status info | `readouts`, four lines, this plan |
| 3 | Image filters, tunable | `filters`, designed here, built later |
| 4 | Regroup when viewing two at a time | `regroup` — already designed as parity breaks, this plan |
| 5 | Change reading direction | `direction`, this plan |
| 6 | Tune or remove animations, tune page-change speed | `motion`, this plan |
| 7 | No flicker when changing pages rapidly | `prefetch` plus the double buffer, this plan |
| 8 | Annotations with export and review-all | `annotations`, this plan |

Trim margins arrived under (3) and is implemented in `fit` rather than
`filters`, because geometry and pixels are different concerns with different
consumers.

## Status readouts

Four lines, appearing with the filmstrip.

1. **Position** — `slot 88 / 195 · p. 84 · 45%`. Every figure comes from the
   sidecar. The percentage *changes on swap*, because slot counts differ per
   edition; that is shown rather than hidden, because hiding it would be the
   reader implying the two editions are the same length.
2. **Reading time** — session elapsed, plus an estimate of what is left from
   recent pages per minute, labelled as an estimate rather than shown to the
   minute. Client-side only.
3. **Chapter** — a new `taiyaku-core` function, derived from the alignment
   rather than from filenames. `notes.md` already specifies this for AniList
   sync and records that it beats declared metadata on real material: Go-Toubun
   volume 1 is c001–c005 *plus* c005.5, and Kaguya's fan release labels two
   files `v1` and is wrong both times. The readouts are its first consumer.
   **Where the work holds no chapter-organised edition, it says so rather than
   guessing** — which is most works, so the empty case is the common one and
   must not read as a bug.
4. **Alignment health** — the current pair's verdict, and whether this slot has
   a counterpart. The earlier document already requires the verdict to travel
   with the swap; this is where it lives without nagging. The twenty-two needles
   are why: acceptance is a statement about calibration, and only the verdict is
   a statement about the alignment.

## No flicker, and the two open items it makes measurable

"Do not flicker" is a symptom, so the design is the mechanism.

**Two `<img>` elements, double-buffered.** The incoming page is decoded
off-screen with `img.decode()` and swapped into view only once decoding
resolves, so no blank or half-painted frame is ever visible. Prefetch runs *N*
ahead and *M* behind along the reading direction, both tunable, with a cap on
concurrent decodes so a fast flick cannot queue forty.

**O1 stops being hypothetical.** Swapping editions refetches the same slot in
another edition — the refetch-by-construction case `status.md` predicts, against
Panels' measured zero. The order inverts here, exactly as that document says:
ship the `ETag` validator first, then count the 304s, because the client-side
question cannot be answered while the server offers nothing to answer it with.
The correction path needs `If-Match` anyway, so the validator lands for two
reasons at once.

**O9 becomes reachable.** Filmstrip tiles resolving per tile, and prefetch
colliding with a reader jumping onto the page being prefetched, is precisely the
"concurrent requests for one key" that O9 names as its reopening condition — the
thing Panels could never produce.

**O14 becomes worth fixing.** `Binding::resolve` rebuilds the route per call; a
filmstrip resolving per tile pays it once per tile. Memoising the last pair is
the cheap half and the swap already wants it.

## Correction UI

Unchanged from the earlier document: pick the counterpart in the filmstrip, the
client computes a run split, `PATCH /api/works/:work/alignments`, serve rewrites
`taiyaku.json` with `by: "hand"`. Correcting a pair that resolved composed
through the hub writes a direct edge, seeded from `compose()` with the fix
applied. Run-splitting arithmetic goes in `taiyaku-core`, written test-first,
because it is format logic that any other reader would need.

New here is only O13, above, and `If-Match`, which the earlier document added in
a late revision and this one carries.

## Testing

The four seam tests from the earlier document stand. Four more, each guarding a
seam where two things must agree and nothing else would notice:

| Test | Property | Catches |
| ---- | -------- | ------- |
| **Note round-trip** | a note written in A, resolved into B and back, lands on its original slot, over every note-bearing slot of the corpus sidecar | drift in a second consumer of `resolve_slot`; the same class the swap round-trip guards |
| **Hand-run preservation** | all three merge cases behave as tabled above | O13 regressing silently, which is how it arose |
| **Keybinding collision** | no two enabled modules claim one key | the failure mode declared keybindings exist to prevent |
| **Decode-before-swap** | the visible `<img>` never sources an image that has not resolved `decode()` | pins the mechanism rather than perceived smoothness, the way O2's test pins rename-not-truncate |

Per layer, as before: Vitest for the pure state machine — authority, swap, gap,
parity, module resolution, note resolution — and Playwright for the reading
flow. The note round-trip is arithmetic over the authority model and is
therefore exhaustive rather than sampled.

## Staging

Roughly four to five weeks on the earlier document's own estimates, which it
also warns are estimates. Each stage has an end state that can be shown.

| | What | End state |
| --- | ---- | --------- |
| **M5a** | Page-view core, three modes, swap, gap state, chrome shell | **the swap works** |
| **M5b** | Module system + `regroup`, `direction`, `motion`, `prefetch`, `readouts`, and `chapter_at` in the core | every requested reading behaviour, individually switchable |
| **M5c** | `annotations` — store, travel, markers, review, export | a note written on the raw is readable on the English |
| **M5d** | Correction UI, O13, `If-Match`, `ETag` | **fix a wrong pairing and keep it** |

M5a before the module system is deliberate: a plugin interface with no plugins
is an untested guess, and M5b arriving promptly is what stops the contract being
shaped around its only caller. This is the same reasoning M2 and M3 were split
on.

## Claims this document makes that the plan must verify against the code

Five of M4's seven defects originated in its plan, from asserting test data and
code behaviour nobody had run. The following are asserted here from
`docs/status.md` and `docs/notes.md` rather than from reading the code, and the
implementation plan must check each before depending on it:

1. `Source::Hand` is constructed nowhere in the workspace.
2. `re_aligning_a_spoke_replaces_it_rather_than_duplicating` pins a
   sha256-changed spoke merge as succeeding — the test this design narrows.
3. `merge_spoke`'s existing guard compares the *from* side only.
4. `progress::store` writes temp-then-rename with a per-attempt unique name, and
   `cover::unique_temp_path` is the shape to reuse.
5. `Route` holds runs in slot indices, making `resolve_slot` the direct
   operation.

Verified by reading during this brainstorm, and safe to rely on: the four
`/api/…` routes and the OPDS routes in `routes.rs`; the binding's surface
(`fromJson`, `editions`, `slotCount`, `resolveSlot`) in `taiyaku-wasm`; the
five-crate layout; the absence of any text layer, OCR dependency or frontend
scaffolding anywhere in the workspace.

## Open questions

Recorded rather than resolved.

- **Filmstrip thumbnail caching.** The earlier document left this open until M5
  knew its access pattern. It now does — roughly 195 tiles per volume against
  one cover per edition — and O11 says that changes the cache arithmetic
  entirely: covers measured 165 KB total, thumbnails would write 195 files per
  volume. Needs deciding in M5a, before the filmstrip exists.
- **Whether corrections need an undo.** Carried unchanged. `by: "hand"` is
  recorded and nothing reverts a correction short of editing JSON.
- **Whose progress wins under Stump.** Carried unchanged, and still not blocking
  because nothing writes to Stump.
- **Whether a travelled note should transform its region once L1 exists.** L1
  would make it arithmetic. Whether a note *should* follow the panel it was
  attached to, or stay where the reader put it, is a preference question and not
  obviously the same answer for everyone.
- **What the text layer does to the region anchor.** M5 stores a region so OCR
  can reuse it. Whether an OCR'd text box is a *note without a body* or a
  different record sharing the anchor is for that spec, not this one.
