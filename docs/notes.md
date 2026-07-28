# Implementation notes

Running notes for decisions and findings that aren't worth a design doc but
shouldn't be re-derived later. Append freely; this is not a spec.

## Alignment can and should be dynamic

Cost of aligning two 195-page volumes is heavily lopsided:

| Step | Cost |
| ---- | ---- |
| Read and decode 390 images | seconds — dominates |
| Hash decoded pages | milliseconds |
| DP over the 195×195 matrix | microseconds |

Only decode matters. Two levers on it:

- **Never decode at full resolution.** Hashing works on a heavily downscaled
  image, and JPEG decodes directly at 1/2, 1/4, or 1/8 scale via DCT scaling —
  much cheaper than full decode followed by a resize. Check whether the `image`
  crate exposes this or whether it needs `jpeg-decoder` directly.
- **Parallelise across pages** with rayon. Embarrassingly parallel.

Expect low single-digit seconds for a first-time volume pair off the NAS.

So the model is **lazy and cached**, not batch-precompute-the-library:

1. User opens a work with two or more editions.
2. Reader is usable immediately — reading edition A needs no alignment.
3. Alignment runs in the background; the swap control activates when it lands.
4. Cached thereafter, so it is instant forever after.

**Fingerprints are per-archive, not per-pair.** Five editions of one volume means
five hashing runs, after which all ten pairwise alignments cost microseconds.
Cache is O(archives), not O(pairs). This is the reason `taiyaku hash` is a
separate command rather than a private step inside `align` — it also makes
re-running the solver with different tuning instant, which matters during the
tuning phase that dominates the schedule.

Escape hatch if this ever feels slow: windowed alignment, hashing only a band
around the current page rather than the whole volume. Almost certainly premature.

## What runs where

- **taiyaku-core (WASM-safe): lookup, not solving.** Resolving a page against an
  existing alignment is arithmetic over a handful of runs. That belongs on the
  client.
- **Solving stays server-side or desktop.** It needs decode, and decoding 390
  images in a phone browser is slow and burns data. Server solves, ships the
  sidecar, client resolves against it.

This is a refinement of "the core compiles to WASM" — the *format* is shared, the
*solver* is not necessarily.

## Sidecar shape

**The unit of alignment is an edition, not a file.** One raw volume corresponds
to roughly five chapter files, so archive-to-archive pairing cannot express the
corpus. An edition is a named, ordered page sequence assembled by concatenating
one or more archives; both sides flatten to a linear sequence, align in that flat
space, and indices resolve back to `(archive, entry)`. Volume-vs-chapter then
stops being a special case.

Readium's `readingOrder` (via OPDS 2 / Web Publication Manifest) is the same
concept and worth borrowing as a model regardless of whether OPDS is ever spoken.

**Runs, not per-page entries.** A clean diagonal should be three lines, not 195
objects. Run-length encoding is what makes the "hand-correctable" principle
actually true — a dozen runs can be reasoned about at a glance — and it diffs
meaningfully in git, so a bad tuning change shows up as a readable diff instead of
195 churned lines. `taiyaku-core` already encodes this shape.

Record edition identity and archive content hashes so a reader can detect that an
archive was replaced and the alignment is stale.

## Alignment topology: hub-and-spoke, raw as hub

**Working default, not yet validated.** The target is N editions simultaneously,
not two — the corpus already has five of Go-Toubun volume 1 — and N editions is
not merely "more pairs".

Rejected: **all-pairs**, storing N(N-1)/2 alignments. Ten alignments for five
editions, five new runs to add a sixth, and nothing enforcing agreement between
them. A→C solved directly can contradict A→B→C, so a reader could land on
different pages depending on which edition the user happened to swap through.

Chosen: **hub-and-spoke.** One edition is the reference; every other edition
aligns only to it, and any A→B resolves by composing A→hub→B.

- N-1 alignments rather than N(N-1)/2 — four rather than ten for the corpus.
- Adding an edition is one alignment run regardless of how many already exist.
- Transitivity holds by construction, so contradictions cannot arise.

**The raw is the hub.** Every other edition derives from it, so it is the most
complete sequence and carries the fewest gaps relative to the others.

**Cost.** Composition cannot see pages that exist in two spokes but not the hub —
a translator's-note page present in two English releases but absent from the raw,
say. Gaps compose the same way: a page mapping onto a hub gap leaves the chain
undefined even when the far edition does have a counterpart. Both are uncommon
with the raw as hub, since it is usually the superset, but neither is impossible.

**Mitigation.** Allow optional direct edges layered over the star. Default to
composition; add a direct alignment only where composition is shown to fail. Keeps
the common case cheap without making the rare case unrepresentable.

**Follow-ups this creates:**

- `taiyaku-core::counterpart` is still the pairwise `(runs, page)` signature. It
  needs to become something closer to `(graph, from_edition, to_edition, page)`.
- Swap UX needs an answer for five editions. Cycling works for two or three; past
  that it wants a picker. Undecided.

### Validate it against all-pairs, and keep checking

The argument above is theoretical. Whether composition *actually* loses anything
worth caring about is an empirical question, and the corpus can answer it. Do not
treat hub-and-spoke as settled until it has.

The test is cheap because all-pairs is not a second system — it is the same solver
run more times. Build the solver once, then:

1. Solve all ten direct pairwise alignments for the five Go-Toubun editions.
2. Solve the four star alignments with the raw as hub.
3. For each of the six spoke-to-spoke pairs, compare composed `A→hub→B` against
   the directly solved `A→B`, page by page.

Metrics worth recording per pair:

- **Loss rate** — pages where direct finds a counterpart and composed returns
  nothing. This is the specific failure the star topology predicts, so it is the
  number that decides the question.
- **Disagreement rate** — pages where both resolve but to different targets.
  Should be near zero; if it is not, suspect the solver rather than the topology.
- **Inverse loss** — pages where composed resolves and direct does not. Indicates
  a bad direct solve, which is useful signal in its own right.

Decision rule: near-zero loss across the corpus confirms the star. Material loss
concentrated on one pair does not condemn the topology — it earns that pair a
direct edge, which is exactly the mitigation above.

**Run it intermittently, not once.** Tuning changes to the hasher or the solver can
alter composition behaviour without touching anything topology-related, so this
wants to be a standing check rather than a one-time gate. Reasonable shape: a
command or test gated behind `TAIYAKU_CORPUS`, skipping when unset so CI stays
green without needing the archives, and run locally whenever the solver changes.

**The decision is reversible**, which is why defaulting to the star now is safe:
all-pairs data is a strict superset of star data, and the format already permits
direct edges. Being wrong costs a re-solve, not a redesign.

## CLI artifacts

Three outputs:

1. **Fingerprint cache** — content-addressed by archive SHA-256, so it invalidates
   correctly on re-download. `taiyaku hash <archive>`.
2. **Sidecar** — `taiyaku.json`, the work/editions/alignments document.
3. **Visual verification report** — `taiyaku verify --html`, a contact sheet of
   paired thumbnails with confidence shading. Read-only ancestor of the correction
   UI; roughly a day of work and satisfies "wrong alignments must be visible" long
   before drag-to-repair exists. Worth having in week one.

Explicitly not produced: modified archives, re-encoded images, a database.

## Reader integration, in order of ambition

| Tier | Reader change | Cost |
| ---- | ------------- | ---- |
| 1. External client | none | write a PWA against their existing REST API |
| 2. Alignment endpoint | serve the sidecar, resolve lookups | small server-side patch |
| 3. Native edition groups | schema linking series as editions | the invasive one |

**Tier 1 needs nobody's permission**, which is the point. Komga and Stump both
serve page images over REST, so the PWA can be a client of an existing server and
inherit library management, metadata, and auth for free. The version of this
project that requires a maintainer to say yes is the version that dies.

Tier 3 is where pushback is reasonable: both currently model two editions as two
unrelated series, and "these series are the same work" is a genuine schema change.
Don't lead with that ask.

**Adoption lever: the WASM core.** A Komga (Vue/TS) or Stump (React/TS) frontend
can import the module and get alignment lookup without reimplementing the format.
"Add a dependency and a keybinding" is a far easier pitch than "here is a spec, go
implement it."

## Gotcha: swapping must not round-trip through the alignment

Raw page 44 is a colour insert with no English counterpart. Swap, reader picks the
nearest match, you land on EN 43. Swap back, alignment maps EN 43 → raw 43. **You
have silently moved one page**, and it compounds on every swap.

Fix: keep a per-edition *anchor* that advances only on real navigation, never on a
swap. Cheap, but only discoverable by getting it wrong, so it belongs in the
format's implementation notes for anyone else implementing a reader.

Related open question: what should a swap onto a gap actually do? Refuse with a
toast, jump to nearest, or show an explicit "no counterpart" state. Probably the
third, since it is the only one that doesn't lie.

## Spike findings, 2026-07-28

Measured against Go-Toubun volume 1, all five editions copied locally to
`corpus/gotoubun-v01/` (gitignored). Throwaway code in `spike/`, excluded from
the workspace. Everything below is a measurement, not an estimate.

### The premise holds

Structural hashing separates same-page from different-page comfortably, and it
survives both colourisation and translation.

| Case | Pair | Result |
| ---- | ---- | ------ |
| 1 | raw mono ↔ raw colour (195/195) | 99.5% argmax on the identity diagonal |
| 2 | raw mono ↔ EN official (195/187) | 92.8% of pages on a monotonic staircase |
| 3 | EN mono ↔ EN Full Colour (187/199) | 96.8% |
| 4 | raw mono ↔ EN fan chapters (195/…) | 91.3% |

Mean similarity on correct pairs is 0.85 against 0.52 for incorrect ones —
**separation 0.33**. These are raw argmax numbers with no sequencing at all; the
DP pass has plenty to work with. Case 2 recovers offsets stepping
−1 → −2 → −5 → −6 → −7 → −8, and that final −8 is exactly the eight pages of
drift the design doc predicted.

### Which hash: dhash at 16×16, on plain luma

| Variant | Case 1 argmax | Separation |
| ------- | ------------- | ---------- |
| dhash 8×8 (64-bit) | 97.9% | 0.320 |
| **dhash 16×16 (256-bit)** | **99.5%** | **0.328** |
| dhash 24×24 | 99.5% | 0.340 |
| phash 8/16/24 | 97.4–99.5% | 0.320–0.324 |

Two hypotheses died here:

- **pHash is not worth its DCT.** It never beat dhash at equal grid size.
- **Edge preprocessing buys nothing.** Sobel-magnitude hashing was meant to
  survive colourisation, on the theory that repainting flat regions wrecks luma
  while leaving linework alone. It moved separation from 0.320 to 0.334 — noise.
  Luma handles colourisation fine, so the extra pass is unjustified.

Going past 16×16 is not free of cost and not clearly better. Take 256-bit dhash.

### Confidence: use margin, not similarity

Margin over the runner-up is the discriminating signal, and it is sharp:
`margin < 0.05` isolates the one-sided pages exactly, with no DP involved.

The same raw pages get flagged against the EN official release *and* against the
independent fan translation — {5,6,7}, {53,54}, {95,96}, {127,128}, {159,160} —
which is two independent editions agreeing on which raw pages have no
counterpart. That is about as good as unlabelled cross-validation gets, and it
means the confidence heat strip the correction UI needs comes for free.

### Decode cost, and the DCT-scaling question

12 cores, release build, warm page cache:

| Edition | Resolution | Per page | Whole volume |
| ------- | ---------- | -------- | ------------ |
| EN official mono | 2147×3056 | 12.1 ms | 2.27 s |
| raw mono | 685×1024 | 0.78 ms | 0.15 s |

A volume pair lands in low single-digit seconds, as predicted. **Decode is not
the bottleneck, so the scaled-decode lever is not needed.**

Answering the open question directly: `image` 0.25 cannot do it. It decodes JPEG
via `zune-jpeg`, which has no scale option, and `image`'s own `JpegDecoder`
exposes nothing but `new`. DCT-scaled decode would mean taking `jpeg-decoder` as
a direct dependency for the JPEG path. Worth roughly 5–8× on the largest
archives, and worth nothing today.

### What the archives actually contain

- **JPEG throughout**, one stray PNG in the EN official release. No CBR, no
  ComicInfo.xml, no non-image entries at all in these five.
- **Resolutions differ by 3×** between editions: 685×1024 for the raw mono up to
  2147×3056 for the EN official. Any fixed working resolution has to be chosen
  independently of the source.
- **Landscape pages exist only in the English editions** — 7 in the official, 5
  in the Full Colour, none in either raw, each exactly double width. So the
  English releases *join* spreads that the raws *split*. Spread handling is not
  hypothetical; it is present in volume 1 and it is the main reason the page
  counts move.
- **Entry names carry structure worth reading.** The EN official encodes chapter
  and role in every filename (`c001 (v01) - p000 [Cover]`, `[ToC]`), and the EN
  Full Colour nests pages in per-chapter folders. Free labels for validating the
  solver, and a plausible source of coarse chapter boundaries.

### Hazards found by getting them wrong

**Lexicographic filename sort is incorrect for this corpus.** `c005.5.cbz` sorts
*before* `c005.cbz`, so a naive glob orders the fan translation wrongly. Measured
cost of the bug: 88.7% monotonic against 91.3% once fixed. Natural-order sorting
is required, for entries within an archive and for archives within an edition.

**Blank pages match each other perfectly and lie about it.** Raw page 1 resolves
to EN page 186 at similarity 1.000 — both are blank, so their hashes are
identical, and it beats the true match outright. A near-blank page carries no
structural information but scores maximally against every other near-blank page.
The solver needs to detect low-variance pages and refuse to anchor on them;
similarity alone will not catch this, because the score is not merely high, it is
perfect.

### Chapter-to-volume grouping cannot be assumed

The design doc records that fan chapters c000–c004 sum to 199 pages and treats
that as volume 1. **It is the wrong grouping.** Aligned against c000–c004, the
raw volume's last 33 pages have no counterpart at all. The volume is
**c001–c005, plus c005.5**.

Two separate errors, and they point in opposite directions:

**c000 is not in the volume.** It is a 44-page one-shot pilot. Page-count
arithmetic matched by coincidence and pointed at the wrong five chapters.

**c005.5 *is* in the volume**, despite the fractional numbering that suggests
side material. Its pages 4–8 map to raw pages 188–192 contiguously at similarity
0.82–0.97 with margins 0.20–0.36 — as strong as any match in the corpus. Only
raw 193–194 are genuinely unmatched back matter. An earlier pass here called all
of 188–194 back matter, which was wrong: it had simply never been offered the
chapter that covers it.

That generalises, and it is the operative rule for ingestion: **`x.y` chapter
numbering usually means bonus content that belongs inside the tankōbon**, not
content outside it. Extras, side stories, and omake are routinely released as
fractional chapters and then collected in the volume. So a fractional chapter is
a *candidate member* by default. Which pages of it land, and where, is for the
alignment to decide — c005.5's first four pages are scanlator title and credit
pages that do not correspond to anything, while its last five are volume content.

More broadly: editions vary in what extras they carry, so the members of an
edition cannot be declared from filenames at all. This settles the open question
about whether chapter-to-volume wants a distinct pre-pass — **which chapters
constitute a volume has to be discovered.** Gap-tolerant DP over the flattened
sequence does find it, tolerating both a wrongly-ordered chapter and a 44-page
non-member and still recovering ~90% of the staircase, so the pre-pass can be a
validation step rather than a prerequisite.

### Hazard: redrawn content scores between noise and truth

The c000 pilot is the dangerous case, because it is not simply absent. Its pages
3–23 produce a *monotonic* run against raw pages 8–28 at mean similarity **0.687**
— well above the 0.52 noise floor, well below the 0.85 of a genuine match. The
one-shot shares scenes with chapter 1 in redrawn form, so the artwork is related
but not identical.

A solver that maximises score subject to monotonicity has every reason to accept
that run. **Monotonicity is not sufficient evidence of membership; there has to
be an absolute similarity floor as well.** The three regimes separate cleanly
within this pair — noise ~0.52, redrawn ~0.69, true ~0.85.

### The floor cannot be a constant: colourisation moves it

Checking that floor against the colour editions killed the idea of hardcoding it.
Distribution of each page's best match, same hash and settings throughout:

| Pair | Transform | Shape |
| ---- | --------- | ----- |
| raw colour ↔ EN colour | none (colour both sides) | **bimodal**: 138/195 pages at 0.95–1.00, 44 more at 0.90–0.95, then nothing at all until 0.65 |
| raw mono ↔ raw colour | colourisation | unimodal, spread 0.60–0.95, peak 0.80–0.90 |
| EN mono ↔ EN colour | colourisation | unimodal, spread 0.60–0.95, peak 0.75–0.85 |

Colour-to-colour is the strongest pair in the corpus by a wide margin, with an
*empty* valley two full buckets wide between matched and unmatched pages. That is
consistent with the EN Full Colour edition being derived from the JP colour
edition — they share a resolution exactly (1605×2400), which is unlikely to be
coincidence.

**Colourisation costs roughly 0.10–0.15 of similarity and erases the valley.**
On both mono↔colour pairs the distribution is unimodal: there is no gap to put a
threshold in. A 0.75 floor, which is comfortably correct for colour↔colour, would
reject 28 of 187 pages on EN mono ↔ EN colour, most of them true matches.

So the floor has to be **derived per pair from the observed distribution** — find
the valley, or take a percentile of the best-match scores — rather than compiled
in as a constant. Any fixed number is implicitly a claim about which transform
separates the two editions, and the corpus contains at least three different ones.

## Cross-series validation, 2026-07-28

Volume 1 of all four series, same hash and settings throughout (dhash, 16×16,
luma, no crop). Corpus copied to `corpus/` — note that three of the four raws are
**directories of loose images rather than cbz archives**, so archive reading
cannot assume zip.

### The core case generalises

| Series | JP raw ↔ EN official | Peak of best-match distribution |
| ------ | -------------------- | ------------------------------- |
| Go-Toubun | 92.8% | 0.80–0.90 |
| Kaguya-sama | 88.3% | 0.70–0.75 |
| Kobayashi | 91.1% | 0.85–0.90 |
| [Oshi no Ko] | 96.0% | 0.75–0.80 |

Four series, four different publishers and scan sources, all recovering a
monotonic staircase from raw argmax with no sequencing. The approach is not
tuned to Go-Toubun.

### The floor is definitively not a constant

The peak of the true-match distribution ranges from **0.70 to 0.90 across series**
on the *same* transform, on top of the 0.10–0.15 that colourisation moves it. Any
compiled-in threshold would be wrong for most of the corpus. Derive it per pair.

### `x.y` chapters are a prior, not a rule

Testing the convention that fractional chapters hold volume content:

| Chapter | Declared | Actually in the volume? |
| ------- | -------- | ----------------------- |
| Go-Toubun c005.5 | (undeclared) | **yes** — pages 4–8 → raw 188–192 at 0.82–0.97 |
| Kaguya v1 c5.5 | filename says `v1` | **no** — all 15 pages at 0.57–0.61, margins ~0.01 |
| Kaguya v1 c10.5 | filename says `v1` | **no** — matches only repeated template pages |

So a fractional chapter is a strong *candidate* and must be offered to the
solver, but membership is decided by alignment, not by the number. Note that
Kaguya's fan release declares `v1` in the filename for both, and is wrong both
times: page counts also lie, since c1–c10 plus c5.5 sums to exactly the raw's 213
pages, and adding c5.5 changes the result not at all (175/213 either way).

**Declared metadata is a hint. The alignment is the authority.**

### Hazard: repeated template pages

Kaguya c10.5's pages 4–12 match raw pages 25, 47, 67, 107, 127, 147, 167, 189,
209 at similarity 0.82–0.99. Those targets are spaced ~20 pages apart: they are
the chapter divider pages, which share a template and are therefore
near-identical to each other. High similarity, scattered targets, not monotonic.

This is the blank-page hazard generalised — **any page that is one of many
near-identical pages will score highly against all of them.** Margin catches it
where raw similarity does not: these matches carry margins of 0.09–0.15 against
0.20–0.35 for genuinely unique pages. Another reason to treat margin, not
similarity, as the confidence signal.

### Alignment quality detects a bad copy

The library holds two copies of [Oshi no Ko] volume 1. They disagree:

| Copy | Against EN official v01 |
| ---- | ----------------------- |
| `Oshinoko v01` (850×1200) | **31.4%** |
| `Oshinoko v01b` (1521×2160) | **96.0%** |

The two copies also fail against *each other* (27.8%), while sharing the volume's
eight colour inserts at 0.93–0.98 — so they are nominally the same volume, and
one of them is not what it claims to be. Whatever `Oshinoko v01` contains, it is
not the volume the English release corresponds to.

Worth stating as a capability rather than an annoyance: **a failed alignment is
useful signal about the library, not only about the aligner.** A pair that scores
30% where every comparable pair scores 90% is evidence of a bad file, and the
verification report should say so rather than silently emitting a bad sidecar.

Same lesson for **Kobayashi's "in COLOR! Chromatic Edition"**: 175 pages, scoring
15.1% against both the raw and the English volume 1, with a handful of pages at
0.90+. It is not a volume-1 counterpart — most likely a colour compilation
spanning several volumes. Edition pairing cannot assume that "the colour edition"
corresponds volume-for-volume with the mono one. (Unconfirmed against other
volumes; worth checking before relying on it.)

### Hazard: mixed naming schemes inside one edition

`Oshinoko v01` contains `0001.jpg`–`0215.jpg` alongside `_01.jpg`–`_08.jpg`, the
latter being the volume's colour inserts, which belong at the **front**. Every
sort order puts them at one end or the other by their prefix, and natural sorting
puts them last. The inserts were confirmed misplaced by matching them against the
other copy, where they occupy pages 0–7 at 0.93–0.98.

Filename-derived page order is therefore not reliable even within a single
directory. Since alignment against a known-good edition locates such pages
correctly, ordering is another thing the solver can repair rather than assume.

### Two more hypotheses tested and rejected

**Autocrop to the inked area: rejected.** Aspect ratios do vary between editions
and correlate with alignment quality — Kaguya's raw is 0.703 against the English
0.667, and its pairs score worse than Go-Toubun's aspect-matched ones. But
cropping to the content bounding box made *every* pair worse (Go-Toubun raw mono
↔ raw colour fell 99.5% → 91.3%), because the crop box depends on each page's own
content and so varies page to page. It adds more variance than the margin
difference it removes. Aspect normalisation may still be worth attempting, but
not this way.

**Mirrored pages: rejected.** dhash compares horizontal neighbours, so a flipped
release would fail while looking content-identical, and flipped Western releases
are a real historical practice. Hashing one side mirrored made the [Oshi no Ko]
copy-vs-copy comparison worse (27.8% → 14.3%), so that is not what is wrong there.

## Twelve volumes: volume 1 is not the outlier I expected

Volumes 1, 3 and 8 of all four series, core raw ↔ EN official pair, same settings
throughout. `MEDBIT` is the median number of bits differing at each page's best
match, out of 256.

| Series | v01 | v03 | v08 | spread | MEDBIT |
| ------ | --- | --- | --- | ------ | ------ |
| Go-Toubun | 92.8% | 93.8% | 91.3% | 2.5 | 40 / 40 / 38 |
| Kaguya-sama | 88.3% | 92.4% | 87.7% | 4.7 | 69 / 57 / 57 |
| Kobayashi | 91.1% | 92.5% | 94.4% | 3.3 | 40 / 33 / 46 |
| [Oshi no Ko] | 96.0% | 96.4% | 95.0% | 1.4 | 55 / 55 / 52 |

**The expectation was wrong.** Volume 1 was supposed to be the worst
representative — it carries the pilot chapter, the heaviest front matter and the
introductory colour inserts. Those features are real, but they cost nothing
measurable: mean across series is 92.1% for v01, 93.8% for v03, 92.1% for v08.
Volume 1 is mid-range in two series and slightly low in two others.

What the spread actually shows is that **series identity matters far more than
volume number.** Within a series, volumes vary by 1.4–4.7 points; across series
the range is 87.7–96.4. Tuning against a single volume of one series was a
reasonable thing to have done; tuning against a single *series* would not be.

### The floor is per edition-pair, and it transfers across volumes

More useful than the headline: `MEDBIT` is nearly constant within a series and
clearly distinct between them — Go-Toubun sits at 38–40, Kobayashi 33–46,
[Oshi no Ko] 52–55, Kaguya 57–69. The scale of the difference between two
editions is a property of *those two editions*, not of the volume.

That sharpens the earlier conclusion. The floor cannot be a global constant, but
it does not have to be recomputed per volume either: **calibrate once per edition
pair, then reuse it across every volume of that pairing.** Which fits the sidecar
model, since an alignment already records which editions it relates.

### Two v01 findings replicate

**Colour↔colour stays the strongest pairing.** Go-Toubun v03 raw colour ↔ EN
colour: 134 of 195 pages at 0.95–1.00, 52 more at 0.90–0.95, median 8 bits
differing, and the same empty valley between 0.70 and 0.85. Identical in shape to
v01.

**Bad copies remain detectable.** [Oshi no Ko] v03 exists in two variants, as v01
did. Against the English volume: `v03` scores **96.4%**, `v03s` scores **55.4%**,
and they score 24.9% against each other with a median of 93 bits differing.
`v03s` also holds 186 pages against `v03`'s 197, so it is not the same content.

Note that 55.4% is less obviously broken than v01's bad copy at 31.4%. A
"is this file what it claims" check should compare a pair against its siblings
rather than against a fixed cutoff — every genuine pairing in the corpus scores
87.7% or better, and every mismatched one 55.4% or worse, but the gap between
those two numbers is where the threshold has to live and nothing yet pins it down.

## Naming: normalise the library, but do not depend on it

Every naming scheme below is one the corpus actually contains, for four series:

| Where | Scheme |
| ----- | ------ |
| Go-Toubun raw | `001.jpg` … `195.jpg` — 3-digit, 1-based |
| Kaguya raw | `0001.jpg` — 4-digit, 1-based |
| Kaguya raw, other copy | `000.jpg` — 3-digit, **0-based** |
| Kobayashi raw | `DLRAW.TO_001.jpg` — source-site prefix |
| [Oshi no Ko] raw | `0001.jpg`…`0215.jpg` **plus** `_01.jpg`…`_08.jpg` |
| Go-Toubun EN official | `… - c001 (v01) - p000 [Cover] [dig] [Kodansha Comics] {HQ}.jpg` |
| Go-Toubun EN colour | `…(KojoZero Scans)/Chapter 1 - The Five-part Bride/001.jpg` |
| Fan chapters | `- c000.cbz`, `- v1 c5.5.cbz`, `- v12 c114.5.cbz`, `- c131 (2023).cbz` |

Eight schemes, four series. Two of them are actively wrong before anything is
decoded: the 0-based copy produces a constant one-page offset against its 1-based
twin, and [Oshi no Ko]'s underscore-prefixed colour inserts sort to the *end* of
the volume when they belong at the front.

### What normalising fixes, and what it does not

Worth doing — it removes a whole class of ingestion bugs for the cost of a rename
pass:

- Zero-padding width, and 0-based versus 1-based numbering.
- Source prefixes (`DLRAW.TO_`) and mixed schemes inside one directory.
- Nested per-chapter folders, which sort correctly only until `Chapter 10`.
- Padding entries like Go-Toubun's `ZBlank (1).jpg`.

Not fixed by any rename, because the information was never in the filename:

- **Membership.** Kaguya's fan release declares `v1` for c5.5 and c10.5; neither
  is in the volume. Page counts agree with the wrong grouping by coincidence.
- **Insert position.** That the colour inserts belong at the front is a fact
  about the volume, not about their names.
- **Which copy is correct.** `Oshinoko v01` and `v01b` are named as equals; one
  of them does not match the English release at all.

So normalisation makes the easy cases easier and leaves every hard case exactly
as hard. **Filename order stays a prior; the alignment stays the authority.**

### Consequences for taiyaku

**Do not require it.** "Your library is not modified" is a design principle, and
a tool that only works on a tidied library is a tool that does not work. The
reader must cope with all eight schemes above, which means decimal-aware natural
sort, recursing into nested folders, and treating archives and loose directories
alike.

**Do not normalise in place either** — same principle. What fits the sidecar
model is *reporting*: since alignment against a known-good edition locates
misplaced pages correctly, taiyaku can emit a suggested renaming and let the user
apply it. `taiyaku verify` is the natural home for that.

**Do not normalise away the metadata that helps.** Go-Toubun's English filenames
carry chapter, volume, page index and role tags — `[Cover]`, `[ToC]` — and that
is free ground truth for validating the solver and seeding chapter boundaries.
Flattening those to `p001.jpg` would destroy the most useful naming in the
corpus. If normalising, preserve the tags or move them into ComicInfo.xml first.

A reasonable canonical form, if one is wanted: one archive or directory per
edition member; pages `p001.jpg`, fixed width, 1-based, in reading order with
inserts in place; chapters `c005`, `c005.5` with the integer part zero-padded.
Sorting still has to be decimal-aware — `c005.5` sorts before `c005` under plain
lexicographic comparison no matter how tidy the names are.

## Why pairing needs the matrix, not a lookup

Hashing is already per-page and independent, cached per archive — that part is
settled. The open question was whether *pairing* could then be a lookup over
those hashes rather than an all-pairs comparison. Measured answer: **no, and it
does not matter.**

**Exact lookup finds nothing.** Bits differing at each page's best match, out of
256:

| Pair | Median | p90 | Pages found by exact-hash lookup |
| ---- | ------ | --- | -------------------------------- |
| raw mono ↔ EN official | 40 | 58 | **1 of 195** |
| raw colour ↔ EN colour (strongest) | 9 | 20 | 3 of 195 |
| two copies of the same scan | 0 | 0 | 213 of 213 |

A true match differs in ~16% of its bits. Distance zero happens only when the
files are byte-identical. Bucketing schemes — LSH, multi-index hashing, BK-trees
— are built for radii of a few bits; at radius 40 they probe so much of the space
that they cost more than the scan they replace.

**And the scan is free.** A full 195×187 matrix takes **~100 µs**, against 2.2 s
to decode and hash the same two volumes. The comparison is ~0.005% of the work.
There is no performance problem here to solve, and hub-and-spoke keeps it at N−1
matrices rather than N(N−1)/2.

**The deeper reason is not performance.** "Hash every page, pair the similar
ones" *is* the argmax path, and it is measurably not sufficient: it lands at
88–96% monotonic across the corpus, so 4–12% of pages need the sequence to
resolve. Worse, the residue is not random noise — it is exactly the cases where a
per-page nearest neighbour is *confidently wrong*:

- **Blank pages** match every other blank page at similarity 1.000.
- **Template pages** (chapter dividers) match all nine of their siblings at
  0.97–0.99.
- **Confidence itself is comparative.** Margin over the runner-up is the signal
  that works, and it cannot be computed from one page's hash in isolation.

Independent per-page pairing has no way to reject any of those. Monotonicity
does. So: hash once per page and cache it, then take the cheap full matrix, then
let the DP use the sequence — the ordering constraint is doing real work, not
just tidying up.

Where distance-zero lookup *is* useful: spotting that two archives are the same
scan (Kaguya's two "copies" match at distance 0 on every page), and finding
duplicate pages inside one edition.

## OPDS 1.2 vs 2.0

| | 1.2 | 2.0 |
| --- | --- | --- |
| Encoding | Atom / XML | JSON |
| Model | Atom feed with acquisition links | Readium Web Publication Manifest |
| Extending | XML namespaces | add JSON properties and `rel` values |
| Status | stable, universal | long-running draft, patchier adoption |

Neither has any concept of "these two publications are alternate editions with a
page correspondence," so an extension is needed either way. OPDS 2 just makes it
pleasanter: a `links` entry with a custom `rel` pointing at the sidecar, versus
XML namespace wrangling. Its `readingOrder` is also a natural fit for the edition
model above.

Page-by-page streaming is not core OPDS at all — that's **OPDS-PSE**, the Page
Streaming Extension, which is what manga clients use to fetch page N without
pulling the whole archive.

**Position:** OPDS 2 is a better technical fit but is not on the critical path.
Tier 1 needs neither version, and the native REST APIs are richer than either.
Treat an OPDS 2 contribution as a *political* investment — a well-scoped,
uncontroversial PR that builds standing with a maintainer before asking for
something stranger — rather than a technical prerequisite.

Verify what Komga and Stump support today before planning around this; the
picture here may be stale.
