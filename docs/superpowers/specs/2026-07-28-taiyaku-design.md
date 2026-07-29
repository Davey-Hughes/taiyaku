# taiyaku — design

**Date:** 2026-07-28
**Status:** design agreed, nothing built

## Summary

A manga reader that shows the same page in two editions and swaps between them on a
keypress. The hard part is not the reader — it is establishing which page in edition A
corresponds to which page in edition B, given that the two releases were assembled
differently.

The decision that shapes everything else: **alignment is computed offline and stored in
a portable sidecar file.** The reader consumes the sidecar. The sidecar, not the
reader, is the thing worth building.

## Problem

Two editions of one volume rarely share a page numbering. Sources of drift, in rough
order of how often they bite:

1. **Release granularity.** Japanese raws are typically volume scans. English releases
   are frequently per-chapter, assembled from magazine serialisation. One volume file
   may correspond to seven chapter files.
2. **Inserted pages.** Credit pages, scanlator notes, translator's notes, ads. Present
   on one side only, and usually not at the start.
3. **Spread handling.** A double-page spread joined into one wide image in one edition
   and split into two portrait pages in the other.
4. **Colour inserts.** Present in the raw, dropped or greyscaled in the release.

The correspondence is monotonic — page order is preserved — but admits insertions and
deletions on both sides. That is exactly the shape of a sequence alignment problem.

## Key observation

The artwork is identical across editions. Same panels, same linework, same
composition; only the glyphs inside the speech bubbles differ, plus scan-level
variation in crop, resolution, gamma, and slight rotation.

This means page similarity is cheap and reliable to measure. A structural hash taken
after downscaling — which discards the high-frequency detail that text lives in —
gives a strong signal for "these are the same page" while being largely blind to which
language is printed on it.

> **Corrected 2026-07-29.** The conclusion holds and has been confirmed across five
> series, but the reasoning in the parenthetical is wrong. Downscaling does not
> discard text; at a 16×16 grid a speech bubble is about one cell, and whether that
> cell holds glyphs changes its brightness against its neighbour. What makes the
> hash work is that Japanese and English glyphs fill a bubble at comparable
> density, so translation preserves the comparison. The hash is blind to *which*
> text, not to *whether there is* text — and a cleaned edition with the text
> removed loses about 0.24 of similarity, more than colourisation costs. See
> "The hash is blind to which text" in `notes.md`.

It also means the problem is *sequencing*, not recognition. Getting a good similarity
metric is the easy half.

## Decision: sidecar-first

**Chosen.** A standalone command-line aligner reads two archives and writes
`taiyaku.json` beside them. A thin PWA reader consumes the sidecar. Neither depends on
any existing comic server.

Rationale:

- It answers the risky question first. Within a weekend you learn whether the alignment
  survives contact with a real, messy library — before any reader code exists.
- The sidecar is the upstreamable artifact. Offering a maintainer a documented format
  plus a tool that produces it is a far easier proposition than a patch threaded
  through their reader internals.
- It keeps the library immutable. Archives are read, never rewritten.

### Alternatives considered

**Fork or extend Komga or Stump now.** Would supply a reader, library management, and
OPDS for free. Rejected for now: it front-loads learning an unfamiliar codebase before
the core idea is validated, and neither reader UI is built around edition-swapping, so
the novel part is precisely where you would be fighting the framework. Revisit once the
format has proven itself — at that point integration is the natural next step rather
than a bet.

**Repack archives with embedded metadata.** Self-contained; no sidecar to misplace.
Rejected as the primary store: it mutates the library, doubles storage during
processing, and is invalidated whenever an edition is re-downloaded. Reasonable later
as an *export* option for portability.

## Data model: three layers

Each layer is a strict superset of the one before. Readers consume whatever depth is
present and ignore the rest, so layers can ship independently.

### L0 — page correspondence

Per-page structural hashes on both sides, a similarity matrix, and a monotonic
alignment with gaps recovered by dynamic programming (Needleman-Wunsch, or DTW with
skip penalties). Output is a list of pairings, each carrying a confidence score, plus
explicit gap markers where a page exists on one side only.

Cost is trivial once hashes exist: a 200 × 200 comparison is 40,000 operations.
Decoding the images to hash them dominates, and happens once.

Unlocks whole-page swap and synchronised side-by-side.

### L1 — geometry

For each matched pair, an affine transform mapping one page's coordinate space onto the
other's. Recoverable by phase correlation for translation and scale, refined with ECC,
or by feature matching with RANSAC. Both are reliable here because the underlying
artwork is identical rather than merely similar.

Unlocks preserving zoom and pan position across a swap — you stay on the same panel
rather than being thrown back to a whole-page view.

### L2 — text regions

Bubble and text-block detection run independently on each edition, then paired by
overlap of transformed centroids using the L1 transform.

This is where L1 pays off a second time: with a transform in hand, cross-edition bubble
correspondence is arithmetic, not a matching problem. No cross-language reasoning and
no OCR is required to establish the pairing — OCR only becomes relevant if you later
want the text itself rather than the image region.

Unlocks tapping a bubble in the translation to see the original line.

## Sidecar format

Design goals, in priority order: human-readable, diffable in git, hand-correctable,
and forward-compatible so that an L0-only reader safely ignores L1 and L2 fields.

It should record the edition identities and content hashes it was derived from, so a
reader can detect when an archive has been replaced and the alignment is stale.

Concrete schema is deliberately left open until the aligner exists and the real shape of
the data is known.

## Correction UI

Required, not optional.

Automatic alignment will get some pages wrong. An alignment the user cannot inspect or
repair is worse than no alignment at all: it silently presents the wrong page, the user
has no way to notice, and confidence in the entire feature quietly collapses.

Minimum viable form: a two-column thumbnail view of both editions with the current
pairing drawn between them, a confidence heat strip along the edge to draw the eye to
weak matches, and drag-to-repair with the correction persisted back to the sidecar.

## Implementation language

**Rust, edition 2024**, for the aligner and the portable core.

This fits the architecture better than it might first appear:

- The core compiles to WebAssembly, so the PWA can run alignment in the browser rather
  than requiring a server round-trip, and a future native shell reuses the same code
  rather than reimplementing it. "Portable core" stops being an aspiration and becomes
  the default.
- L0 is hashing and dynamic programming over a similarity matrix — exactly the kind of
  CPU-bound, easily parallelised work Rust is good at. Hashing a volume pair is
  embarrassingly parallel across pages.
- Stump is written in Rust. If upstreaming ever becomes the goal, sharing a language
  removes the largest practical obstacle.

Likely crates: `zip` for archive reading, `image` for decode, `rayon` for parallel
hashing, `realfft` or `rustfft` for phase correlation at L1, `ort` for ONNX inference at
L2.

**One honest caveat.** Rust's computer-vision ecosystem is thinner than Python's. L0 is
unaffected — it needs no CV library at all. L1 is fine: phase correlation over FFTs is
a few hundred lines and does not need OpenCV. L2 is where friction appears, because
bubble detection means running a trained model, which requires exporting something like
`comic-text-detector` to ONNX and driving it through `ort`. That is a known path, not a
research problem, but budget for it being slower going than the equivalent Python. If
L2 stalls, prototyping the detection in Python and porting once the approach is settled
is a reasonable escape hatch — it does not compromise the L0/L1 core.

## Clients

**PWA now, native later.** One codebase to start, installable and offline-capable via
service worker and the Cache API. The Rust core compiles to WASM and is shared, so a
native shell can reuse the reading and alignment logic if and when iOS offline storage
limits become the binding constraint.

## Ecosystem

Worth evaluating rather than rebuilding. All predate this project and their current
state should be verified before taking a dependency:

- **Manga109** — annotated manga dataset, including frame and text regions.
- **comic-text-detector** — text-region detection, from BallonsTranslator.
- **manga-ocr** — Japanese OCR tuned for manga typography.
- **mokuro** — generates selectable-text overlays for single-edition Japanese manga.
  Effectively prior art for L2 on one edition; the novelty here is the cross-edition
  link.

## Test corpus

A local library already covers every case this design needs; no additional acquisition
is required to build and tune L0 and L1. Archives live on a NAS share under
`Manga/`, `Manga (Chapters)/`, and `Manga (RAW)/`, located at build time via a
`TAIYAKU_CORPUS` environment variable and never committed.

Four series carry both a raw and a translated edition: *Go-Toubun no Hanayome*,
*Kaguya-sama wa Kokurasetai*, *Kobayashi-san Chi no Meidoragon*, and *[Oshi no Ko]*.

*Go-Toubun no Hanayome* alone provides five editions of the same work, which makes it
the primary development corpus. Measured page counts for volume 1:

| Edition | Pages |
| ------- | ----- |
| Raw, monochrome | 195 |
| Raw, full colour | 195 |
| English official, monochrome | 187 |
| English official, Full Colour Edition | 199 |
| English fan translation, per chapter | c000–c004 sum to 199 |

This yields a test matrix that isolates one variable at a time:

1. **Raw mono ↔ raw colour.** Same language, same publisher, identical page count.
   Colour is the only variable, and the correct answer is a clean 1:1 diagonal. This is
   the sanity baseline — if the aligner cannot produce a perfect diagonal here, it is
   broken, and it makes an excellent regression test.
2. **Raw mono ↔ English official.** Eight pages of drift across a single volume.
   The core case.
3. **English mono ↔ English Full Colour Edition.** Twelve pages of drift with language
   held constant, isolating re-release differences from translation differences.
4. **Raw volume ↔ English chapters.** The hard case. Roughly five chapters map to one
   volume, and chapter `c005.5` is a nine-page side story that may not appear in the
   tankōbon at all — a natural test of one-sided insertion handling.

The presence of a zero-drift pair and a maximum-drift pair in the same series is
unusually convenient, and worth exploiting: tune against case 1 first, then walk up.

## Effort

Part-time, evenings and weekends:

| Piece | Estimate |
| ----- | -------- |
| L0 aligner, first working version | 1 weekend |
| Tuning against a real, messy corpus | 1–2 weeks |
| Sidecar format and correction UI | ~1 week |
| PWA reader worth using | 3–4 weeks |
| L1 geometric transform | 2–4 days |
| L2 bubble peek | 2–3 weeks |

Roughly **2–3 months to something genuinely good for personal use.** Releasable to
strangers is a different multiple, dominated by ingestion of arbitrary archives rather
than by anything discussed here.

The estimate's centre of gravity is worth stating plainly: the algorithm is a weekend.
The tuning and the reader are the project.

## Open questions

- Sidecar schema specifics, deferred until real alignment output exists.
- Which structural hash. Candidates: pHash, dHash, or a small custom descriptor over
  downscaled gradients. Decide empirically against the corpus.
- Whether chapter-to-volume alignment wants a distinct pre-pass to establish coarse
  chapter boundaries before per-page alignment, or whether gap-tolerant DP handles it
  in one shot.
- How editions are paired at the volume level. Assumed manual to begin with — the user
  picks two files. Automatic series matching is explicitly out of scope.
