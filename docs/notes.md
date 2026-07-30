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

**Validated against all-pairs on 2026-07-28; see the results below.** The target
is N editions simultaneously, not two — the corpus already has five of Go-Toubun
volume 1 — and N editions is not merely "more pairs".

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

### The hub should be chosen, not assumed

"The raw is the hub" is a convention resting on the argument that the raw is
usually the most complete sequence. The topology measurements say hub choice has
real consequences — **distance from the hub predicts loss**, with editions sharing
the hub's page count losing under 0.5% against each other and the two furthest
losing 3.5% — so it is worth choosing on evidence rather than convention.

The preference to encode is: **one edition is the base, chosen for quality and
coverage, and everything else layers onto it.** Official releases are usually the
cleaner scan, but not always; some fan releases beat the official one, and the
raw is not automatically the most complete either.

All three criteria are things an alignment already measures:

- **Coverage** — the edition with the fewest gaps when others align onto it. This
  is exactly what the topology command reports as loss, so the best hub is the one
  minimising total loss across all spokes.
- **Quality** — resolution is already read during fingerprinting, and page-count
  completeness falls out of the alignment.
- **Content** — an edition carrying pages the others lack shows up as those
  editions gapping against it.

So hub selection can be a measurement rather than a declaration: solve every
candidate as hub, keep the one with the lowest total loss. For five editions that
is eight alignments per candidate, and the matrix costs microseconds, so the whole
comparison is cheap once fingerprints are cached.

That also answers the question of what a base is *for*. Choosing a base and
flipping between the others is the reading model, and hub-and-spoke is already
exactly that shape — the base is the hub, and every swap resolves through it.

### Result: the star holds, at a measured cost of about 1.7% of pages

Run with `taiyaku topology <hub>... -- <spoke>... -- <spoke>...` on volume 1 of
both series that carry five editions. Composed `A→hub→B` against a directly
solved `A→B`, page by page, over every ordered spoke pair.

| Series | Comparisons | Loss | Disagreement | Inverse loss |
| ------ | ----------- | ---- | ------------ | ------------ |
| Go-Toubun | 2,316 | 1.90% | **0.00%** | 0.00% |
| Kaguya-sama | 2,499 | 1.56% | 0.16% | 0.32% |

**Disagreement is the number that mattered, and it is essentially zero.** The
theoretical worry was contradiction — a reader landing on a different page
depending on which edition it swapped through. Across 4,815 page comparisons
that happened four times, all of them in Kaguya, which is the weakest pairing in
the corpus by every other measure too. Per the criterion set out above, that
implicates the solver rather than the topology.

**Loss is real but small and safe.** Around 1.7% of pages — roughly three in a
195-page volume — resolve through a direct alignment but not through the hub,
exactly the predicted case of a page landing on a hub gap. The failure direction
is the harmless one: the reader shows "no counterpart" rather than the wrong
page.

**Distance from the hub predicts loss**, which is the useful finding for where to
spend a direct edge. In Go-Toubun the raw colour edition, which shares the raw
mono hub's page count exactly, loses 0.0–0.5% against the other spokes. The two
editions furthest from the hub, the English Full Colour release and the fan
translation, lose 3.5–3.7% against each other. If any pair earns a direct edge it
is that one, and the mitigation already designed above covers it.

**Inverse loss is not noise.** Kaguya's 0.32% is composition succeeding where the
direct solve failed, which per the criterion above marks bad direct solves. It is
a small vote in the star's favour rather than against it.

**Cost, concretely.** Five editions: eight alignments for the star against twenty
for all-pairs, and every one of the twenty lookups is still answerable. Adding a
sixth edition costs two more alignments rather than ten.

**Verdict: keep hub-and-spoke, raw as hub.** Re-run the command whenever the
hasher or solver changes; the numbers above are the baseline to compare against.

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

## The hash is blind to which text, not to whether there is text

**This qualifies the design doc's central claim**, which reads:

> A structural hash taken after downscaling — which discards the high-frequency
> detail that text lives in — gives a strong signal for "these are the same page"
> while being largely blind to which language is printed on it.

The first half is right and the corpus has confirmed it five times over. The
parenthetical is wrong, and a *cleaned* Berserk edition — a tankōbon scan with
the speech-bubble text digitally removed, the base scanlators typeset onto — is
what showed it.

| Pair | Similarity peak | Monotonic |
| ---- | --------------- | --------- |
| raw → English, text on both sides | **0.85–0.90** | 90.3% |
| cleaned → raw, text removed on one side | **0.60–0.65** | 60.8% |
| cleaned → English | — | 26.6 runs per 100 pages |

Removing text costs roughly **0.24 of similarity**. That is larger than the
0.10–0.15 colourisation costs and nearly the whole 0.33 that separates signal
from noise. Aligning a cleaned edition against anything is the hardest case in
the corpus by a wide margin, where the premise predicts it should be the easiest.

**Why, mechanically.** At a 16×16 grid a speech bubble is roughly one cell.
Japanese glyphs and English glyphs both fill that cell with dark marks at
comparable density, so the cell's brightness relative to its neighbour survives
translation — which is exactly the invariance the method needs and gets. Delete
the text and the cell becomes uniform white, and the comparison against its
neighbour flips. The hash is invariant to text **substitution**, not to text
**presence**.

**One confound, stated rather than hidden.** The cleaned edition also varies in
page width from 4846 to 5343 pixels, about 10%, where the raw is uniform. That is
far more crop variation than anything else measured — Kobayashi's physical scan
varies 0.4% — so it plausibly contributes. Text removal is the likely dominant
cause because the effect is so much larger than any resolution or crop effect
measured elsewhere, including a 4.8× resolution gap that cost nothing, but the
two are not separated here and a controlled test would need a cleaned edition
with uniform page widths.

**Consequences.** Cleaned and redrawn editions are a distinct hard case, not a
variation on the easy one. Anything wanting to align them would need to work on
panel geometry rather than page luma — masking bubble interiors before hashing,
say. Nothing in the corpus needs that today, and it is worth knowing that the
method has a boundary and where it sits.

## An alignment is an edition-identity oracle

A side effect worth naming, because it is the only thing taiyaku can do that
nothing else in this ecosystem can: **an alignment answers "are these the same
work" from content rather than from names.**

Measured, across twelve volumes and four series:

| Relationship | What the aligner does |
| ------------ | --------------------- |
| Same work, any two editions | 87.7–98.6% of pages on a monotonic staircase, ≤8 runs per hundred pages |
| Different works | **refuses outright** — Go-Toubun's raw against Kaguya's English release returns a single gap run, nothing paired |
| Wrong copy of the right work | pairs 31–55% and fragments to 22–45 runs per hundred pages |

The three regimes do not overlap, so the question is decidable without any string
matching, publisher metadata, or human naming convention being correct.

### Why that matters outside this project

Every existing tool that links a local library to a tracking service matches by
title. `komf` searches AniList, MAL and MangaUpdates by name and writes the match
into the server's metadata; `komga-anilist-sync`, `komga-sync` and MAL-Sync then
read that link and push progress. All three depend entirely on the name match
being right.

Name matching breaks on exactly the case this library is full of. `Go-Toubun no
Hanayome - Full Color Edition`, `One Piece - Digital Colored Comics`,
`Monster - The Perfect Edition`, `Uzumaki (Deluxe Edition 3-in-1)` are all
name-distinct and work-identical. Content matching does not care.

### It also derives the chapter-to-volume map

The second thing that breaks tracker sync is units: AniList counts chapters, a
volume-organised library counts volumes. Aligning a volume edition against a
chapter edition *is* the mapping, and the corpus already shows it beating
declared metadata:

- Go-Toubun volume 1 is fan chapters c001–c005 **plus c005.5**, not the c000–c004
  the page-count arithmetic suggested.
- Kaguya volume 1 is c1–c10. Its fan release names two further files `v1 c5.5`
  and `v1 c10.5`; both filenames say volume 1 and both are wrong.

A metadata-derived chapter-to-volume map would be wrong in both cases. The
content-derived one is right in both.

### The shape this should take, and what it should not

**Not a sync tool.** OAuth, rate limits, progress-conflict rules and tracking a
third-party GraphQL schema are ongoing maintenance with no novelty in them, and
the existing tools already do that part adequately. Building another one trades
the interesting half for the tedious half.

**A small command that emits identity**, for those tools to consume: which
members of a library are one work, and which chapters constitute which volume.
Then a person binds one tracker ID per work by hand, once, and the commodity
tools do the commodity work.

**Cost is the honest objection.** This means fingerprinting archives, which is
O(archives) and cached but still real decode time — hours for a 122-series
library on a first run, worse over a network share. It is a background job, not
an interactive query.

**Sequencing: after the reader exists, not before.** The estimate in the design
doc predates spreads, mislabelled copies, and eight incompatible naming schemes,
and every session so far has found something. A second product before the first
one is usable is how projects die.

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
which is two independent editions agreeing on which raw pages the margin cannot
place. That is about as good as unlabelled cross-validation gets, and it means
the confidence heat strip the correction UI needs comes for free.

**Corrected 2026-07-28:** those pages were originally read here as one-sided
inserts, chapter title pages present only in the raw. They are not. They are the
two halves of joined double-page spreads, and the reason both English editions
agree is that both join the same spreads. See "Spreads are not gaps" below.

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

**Where fan editions sit.** The preference is official and clean where possible,
but that is a default rather than a rule — some fan releases are better than the
official one, and the chapter releases in this corpus are the messiest input by
some distance. Nothing needs deciding in the abstract: an edition earns the base
position by measuring better, and any edition that is not the base is a spoke
whether it is official or not. Chapter releases are first-class as *members* of
an edition; whether a given one deserves to be the base is a measurement.

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

## The DP solver works, 2026-07-28

Needleman-Wunsch over the page-similarity matrix, in `taiyaku-core`. Note this
refines "core is lookup, not solving": the solver needs no image decoding, only
hashes, so it is portable and belongs beside the lookup. Only fingerprinting is
native-only.

Go-Toubun volume 1, raw against the English release, in full:

```
   0-0    ->    0-0     offset  +0
   1-1    ->     gap
   2-4    ->    1-3     offset  -1
   5-6    ->     gap
   7-46   ->    5-44    offset  -2
  47-50   ->     gap
  51-52   ->   47-48    offset  -4
  53-54   ->     gap
  55-94   ->   50-89    offset  -5
  95-96   ->     gap
  97-126  ->   91-120   offset  -6
 127-128  ->     gap
 129-158  ->  122-151   offset  -7
 159-160  ->     gap
 161-194  ->  153-186   offset  -8
```

**Fifteen runs for 195 pages**, recovering exactly the staircase that was mapped
by hand from the argmax paths, and landing on the same final offset of −8 that
the design doc predicted. This is the "hand-correctable" property made real: a
person can read that in one glance and a bad tuning change shows up as a
readable diff rather than 195 churned lines.

**Page 1 is the vindication.** It is blank, and it matched English page 186 at
similarity 1.000 — a perfect score, and wrong. The DP gaps it, because pairing it
would break monotonicity. That is the failure mode independent per-page matching
cannot see and sequencing fixes for free.

Across all twelve volumes: 3–17 runs each, 91.3–98.6% of pages paired.

### Coverage is the wrong quality metric; fragmentation is the right one

| Pairing | Runs | Paired | Runs per 100 pages |
| ------- | ---- | ------ | ------------------ |
| Kaguya v03 (good) | 6 | 98.6% | **2.9** |
| Kobayashi v01 (good) | 5 | 95.9% | **3.4** |
| [Oshi no Ko] v01 (good) | 12 | 97.3% | **5.4** |
| Go-Toubun v01 (good) | 15 | 92.3% | **7.7** |
| [Oshi no Ko] v03s (bad) | 41 | 90.9% | **22** |
| [Oshi no Ko] v01 bad copy | 76 | 85.2% | **34** |
| Kobayashi Chromatic (bad) | 79 | 72.6% | **45** |

Coverage does not separate them at all — the bad `v03s` copy pairs 90.9% of its
pages, beating correct Go-Toubun at 92.3% closely enough to be useless. **A
wrong alignment is not sparse, it is fragmented.** It finds a counterpart for
most pages and stitches them into confetti. Every correct pairing measured sits
at or below 8 runs per hundred pages, every known-bad one at or above 22.

That is now reported, though not enforced: the gap is drawn from four series and
wants more evidence before a file gets rejected on it.

**Important qualification, added later.** These numbers were measured without
spread handling, and spreads fragment a correct alignment badly enough to reach
the same range — One Piece volume 105 scores 21.9 runs per hundred unhandled and
3.8 handled. Fragmentation separates good from bad **only once spreads are
expressed**. See "The fragmentation metric is unusable without spread handling".

**And a second qualification.** A refused pair produces one run, scoring about
0.9 per hundred, which beats every correct alignment. Coverage has to gate the
measure. See "The fragmentation metric has a hole at the bottom".

### The separation guard catches a different failure

`MIN_SEPARATION` — requiring the median best-match to stand clear of the median
of the whole matrix — refuses to align **different works** outright. Go-Toubun's
raw against Kaguya's English release returns a single gap run covering all 195
pages, which is the correct answer.

It does **not** catch a bad copy of the *right* work, which still paired 85.2%.
That makes sense: a different scan of the same series shares art style, screentone
density and panel rhythm, so its noise floor rises with its signal. The two
checks are complementary rather than redundant — separation rejects the wrong
work, fragmentation flags the wrong copy.

### Built test-first

Ten tests in the core, driven one at a time, each watched failing first. The two
that mattered both encode corpus measurements rather than intuitions:

- *a uniformly weak pairing still aligns* — a fixed floor of 0.75 returned one
  195-page gap on hashes modelling Kaguya's weak pairing. This is what forced
  per-pair calibration into the design rather than leaving it a note.
- *unrelated editions produce no pairings* — calibrating from the data is
  circular without a guard, and the failing run emitted four runs of confident
  noise. Refusing is the correct answer.

One test, *a constant offset stays a single run*, passed on first run: it covers
the skip-B path the earlier tests had already forced into existence, so it is
regression coverage rather than a driven behaviour.

## The verification report

`taiyaku verify <a>... -- <b>... [--out FILE]` writes a single self-contained
HTML contact sheet: paired thumbnails as `data:` URIs, no loose files beside it.
A whole volume pair is about 3.5 MB, small enough to keep next to the archives.

Design is borrowed from print production, because a wrong alignment *is*
misregistration — two plates that do not line up. Each pair's gutter carries a
gauge of two ticks that close into one rule when the pairing is firm and slip
apart as the margin collapses, so degree is visible where the colour buckets
only show category.

**Margin drives everything shown.** Similarity is reported but never shaded on,
for the reason the corpus already established: a chapter divider matching all
nine of its siblings scores 0.98 and means nothing.

On Go-Toubun volume 1 the report asks for attention on **10 pages out of 195**:

| Verdict | Rule | Pages |
| ------- | ---- | ----- |
| firm | margin ≥ 0.15 | 170 |
| worth checking | 0.05 ≤ margin < 0.15 | 8 |
| ambiguous | margin < 0.05 | 2 |
| no counterpart | gap | 15 |

Margins on that volume run from −0.13 to +0.29, median +0.22, so the thresholds
sit in real gaps in the distribution rather than at round numbers. Ten pages is
a review a person will actually do; flagging fifty would not be.

Three details that turned out to matter, all found by looking at the rendered
page rather than by reasoning about it:

- **A gap needs its rejected score, labelled as rejected.** The first draft
  printed `sim 1.00` beside "no counterpart" — true, and it reads as a
  contradiction. It now says *best rejected*, which turns the number into the
  explanation for the gap.
- **A firm pairing should say nothing.** Verdict text on all 195 rows was noise
  that made the eight that mattered harder to find.
- **Density is the whole point.** One pair per row meant 41,000 pixels of
  scrolling; tiling them means a dozen visible at once, which is what lets the
  eye snag on the wrong one.

The negative margins are worth keeping visible rather than clamping: they mark
pages where the solver passed over a better-scoring candidate to hold
monotonicity. Usually that is the sequencing doing its job, and occasionally it
will be the alignment being wrong — either way it is exactly what a reviewer
should be looking at.

## Spreads are not gaps, and the format cannot currently say so

**This is a schema gap, found before writing the sidecar rather than after.**

Go-Toubun volume 1, English official release: seven pages are landscape at
4294×3056, exactly double the 2147×3056 of every other page. The raws have none.
The English filenames say so outright — `p004-005`, `p046-047`, `p048-049`,
`p052-053`, `p094-095`, `p126-127`, `p158-159`. The English edition **joins**
double-page spreads that the raw **splits**.

Checking the solver's output against those seven indices: all seven are
unreached, no raw page maps to any of them. And the gaps account for it exactly:

```
gapped raw pages: 15 in 7 runs
  (1,1) (5,6) (47,50) (53,54) (95,96) (127,128) (159,160)
7 spreads x 2 raw halves = 14, plus 1 blank page = 15
EN 187 + 7 absorbed spread halves + 1 blank = 195 = raw pages
```

The whole eight-page drift of the core test case is now accounted for: seven
spread joins and one blank page. Nothing else.

**Why this matters more than it looks.** The alignment currently records those
fourteen pages as having *no counterpart*, and that is simply wrong. The
counterpart exists — it is right there, merged into a wide page. A reader on raw
page 47 that says "no counterpart" is giving a worse answer than the one it has,
on 7% of the volume.

`Run` cannot express this: `Run::paired` asserts both sides carry the same page
count, so a two-to-one correspondence has no representation. The options, none
yet chosen:

1. **Allow a run to be many-to-one**, with a flag distinguishing a spread join
   from a genuine gap. Smallest change to the format; the reader has to know
   which half of the target page it wants.
2. **Give a page a fractional counterpart** — raw 47 → EN 45 left half, raw 48 →
   EN 45 right half. Expresses the truth exactly and is what L1's affine
   transform will want anyway, since "which half" is a crop.
3. **Leave it a gap and let the reader special-case it.** Cheapest, and it
   pushes the problem onto every reader that consumes the format, which is
   against the point of the format.

Option 2 looks right and is worth deciding before the schema is written, because
it is the one that changes the shape of a correspondence rather than adding a
field. Detection is not the hard part — a target page at roughly double the
aspect ratio of its neighbours is a strong signal, available before any hashing.

### Prototyped, and it works: `align --split-spreads`

The premise was checked first: cut each landscape page down the middle, hash both
halves, and see what they match. On Go-Toubun volume 1 both halves of six of the
seven spreads land on exactly the two consecutive raw pages that were gapped, at
similarities of 0.72 to 0.87 — indistinguishable from ordinary matches.

The implementation needs no change to the solver at all. A landscape page becomes
**two slots** in the hashed sequence, the alignment stays strictly one-to-one, and
the halves are folded back afterwards. The solver never learns spreads exist.

| | before | after |
| --- | ------ | ----- |
| Go-Toubun v01 | 15 runs, 15 unmatched | **5 runs, 2 unmatched** |
| Across twelve volumes | 110 runs, 92 unmatched | **62 runs, 41 unmatched** |

Fifty-one of ninety-two unmatched pages recovered, and the run count down 44%.
Volumes with no spreads are untouched, which is the right null result.

### Reading order is not discoverable, it has to be declared

The first attempt recovered only **seven** halves rather than thirteen — exactly
one per spread, never both. The cause is worth recording because it is not
obvious.

Manga reads right to left, so the **right** half of a joined spread is the
*earlier* page. Emitting the two halves in image order, left then right, asks the
alignment to run backwards across the pair, and the solver enforces monotonicity
and correctly refuses. It cannot try the other order either: monotonicity is
global, so there is no way to reverse a single pair within an otherwise forward
alignment.

Emitting halves in *reading* order fixes it and recovers all thirteen. The
practical consequence for the schema: **reading direction is an input, not
something the alignment discovers.** It could be inferred by solving twice and
keeping the better result, which is cheap given the matrix costs microseconds,
but it cannot be left implicit.

### Both sides carry spreads, so the model has to be symmetric

The first version of this cut spreads on the target side only, which quietly
assumed the raw always splits and the translation always joins. That holds for
every raw-against-English pair in the corpus and is still wrong, because the
colour editions join spreads too — so the *same pair* read the other way round
puts the spreads on the source side.

Measured on Go-Toubun volume 1, all four configurations:

| Spreads on | plain | symmetric |
| ---------- | ----- | --------- |
| B only (raw → EN mono) | 15 runs, 15 unmatched | **5 runs, 2 unmatched** |
| A only (EN colour → raw) | 21 runs, 15 unmatched | **11 runs, 10 unmatched** |
| both (EN mono → EN colour) | 10 runs | **8 runs, 1 unmatched** |
| neither (raw mono → raw colour) | 2 runs | 2 runs, unchanged |

The A-only row looks worse than the others and is not. Reading the same pair in
both directions gives **194 pairings either way** — 184 whole plus 10 halves,
identical. The only difference is unmatched count, 1 against 10, and that is
arithmetic: the colour edition is 199 pages plus 5 spreads, so 204 slots against
the raw's 195, and nine surplus slots on the A side *must* become gaps. Runs are
A-indexed, so surplus pages are visible in one direction and silently absent in
the other. The correspondence itself is direction-symmetric.

Note also that without symmetric handling the A-only direction scores 10.6 runs
per hundred pages, which is above the threshold that separates good pairings from
bad ones. A perfectly good alignment was being made to look like a broken one
purely by which edition was named first.

### What this means for the schema

1. **Both sides are sequences of slots**, and a slot is `(page, Option<Half>)`.
   The alignment is one-to-one between slots. This handles all four
   configurations uniformly, needs no change to the solver, and collapses what
   looked like a many-to-one problem into the one-to-one one already solved.
2. `Half` is in image coordinates — left or right — because that is what a reader
   has to crop. Which half is the earlier page follows from reading direction and
   must not be baked into the field's meaning.
3. **Reading direction belongs on the work**, alongside edition identity. It is
   an input; the alignment cannot derive it.
4. Run-length encoding survives intact. Across twelve volumes, runs drop from 110
   to 62; Go-Toubun volume 1 goes from fifteen runs to five.

**Known limit, accepted deliberately.** Every one of the 28 spreads in the corpus
is exactly twice the width of its neighbours, so halves are enough. A three-page
gatefold would not fit, and generalising `Half` to "part *i* of *n*" would cost
nothing now. It is not worth doing on zero evidence: a gatefold needs geometry
rather than a fraction, which is L1's problem, and inventing a shape for a case
nobody has seen is how formats acquire fields that never get used.

### Aside: the fragmentation baseline was measured on fragmented data

The earlier finding that correct alignments sit at or below 8 runs per hundred
pages was measured with spreads fragmenting every affected volume. With spreads
handled, Go-Toubun volume 1 drops to 2.6 runs per hundred. The separation from
the known-bad pairings at 22 and above therefore gets wider, not narrower, which
strengthens fragmentation as a quality signal.

## Container formats: content, not extension

Repackaged the same 195 raw pages six ways and aligned each against the same
English edition. Every one produced a **byte-identical run listing**:

| Container | Result |
| --------- | ------ |
| `.cbz` (plain zip) | identical |
| `.cbr` holding zip content | identical |
| nested folders two deep | identical |
| `__MACOSX`, `.DS_Store`, `ComicInfo.xml` present | identical |
| mixed `.jpg` / `.png` / `.PNG` | identical |
| loose directory, no archive | identical |
| `.cb7` (real 7-zip) | refused, clear error |
| `.cbt` (real tar) | refused, clear error |

Two things worth keeping:

- **Extension is a hint, content decides.** A zip named `.cbr` reads fine, which
  is right: misnamed archives are common, and the `zip` crate is being asked to
  parse the bytes rather than trust the suffix.
- **Real RAR and 7-zip are not supported and fail cleanly**, which is the correct
  behaviour for now. The library holds 3,708 `.cbz` and **zero `.cbr`** across
  `Manga/`, `Manga (Chapters)/` and `Manga (RAW)/`, so this is not worth a
  dependency yet. It becomes worth it the moment the tool is pointed at someone
  else's library, where CBR is common. `unrar` is not redistributable under
  GPLv3-compatible terms, so the practical route is the `sevenz-rust2` and
  `unrar` crates or shelling out — a decision to make deliberately, not by
  accident.

Re-tested with a real 7-Zip binary (`brew install sevenzip`, giving `7zz`):
genuine LZMA2 and stored-mode `.cb7` archives are both refused, and 7-Zip's own
zip writer — a different implementation from `zip(1)` — produces a `.cbz` whose
run listing is byte-identical to the original. That last one matters: it shows
the reader is parsing zip rather than one writer's quirks.

**A genuine RAR fixture could not be obtained, and the attempt is worth
recording so nobody repeats it.** RAR compression is proprietary, so only
RARLAB's own binary can create one. The Homebrew cask is deprecated for failing
the macOS Gatekeeper check and disappears on 2026-09-01; installing it lays down
`unrar` and the documentation but **not the `rar` binary itself**, leaving a
dangling symlink in `/opt/homebrew/bin`. Extracting `rar` by hand from the cached
tarball and clearing its quarantine attribute still produces a binary that will
not execute.

A synthetic RAR header stands in instead, which is sufficient for the only thing
under test: taiyaku recognises the magic bytes and refuses, never reading past
them. If real CBR fixtures are ever needed, they will have to come from a
non-macOS machine or from a real-world archive.

### Errors name the container

Extensions are wrong in both directions in the wild, so the leading bytes decide
what the error says. Previously a perfectly good 7-Zip file produced
`invalid Zip archive: Could not find EOCD`, which tells the holder nothing.

```
this is a 7-Zip archive. taiyaku reads zip-based archives and directories
of page images; convert it or unpack it first
```

Recognising 7-Zip, RAR and tar is deliberately **not** a step towards reading
them. No dependency, no decoder, and the bytes are never read past the header.

### Bug found by this: an empty edition panicked

A cbz containing only a `ComicInfo.xml` decodes to zero pages, and the floor
calibration then took the median of an empty slice. Fixed, with a test: every
page of A is simply unmatched, which is an answer, and panicking is not.

### Limitation, and a fix that was measured and rejected: a partial edition is refused

Aligning a *single chapter* against a whole volume returns no pairings at all.
The floor calibration takes the median of each page's best match as the signal
level, which assumes most pages have a counterpart. Align 52 chapter pages
against a 195-page volume and three quarters of the volume has nothing to match,
so the median sits at the noise level and the separation guard refuses.

That is defensible for the intended use — an edition is the whole set of chapters
— but it means `align` cannot be pointed at one chapter to see where it lands.

**The obvious fix was tried and reverted.** Estimating the signal from the 90th
percentile of best matches rather than the median does make the case align, and
the distributions looked cleanly separable beforehand: an unrelated pair puts its
90th percentile at about 0.65 against a 0.62 noise floor, a volume against one of
its own chapters at 0.93.

Measuring the consequences killed it. With the percentile in place:

- Go-Toubun's raw against Kaguya's English release **pairs 146 of 195 pages**
  instead of being refused. Fragmentation still flags it at 49 runs per hundred,
  but refusing was the correct answer and is what the identity-oracle claim rests
  on.
- Kaguya's own alignment **degrades from 8 runs and 204 pages paired to 12 and
  201**, because the raised signal raises the floor and the floor starts
  rejecting genuine matches.

Trading a correctness property for a diagnostic convenience is a bad trade, so
the refusal stays by design and there is now a test pinning it so the same trade
is not made again by accident.

## New raws, 2026-07-29: both schema questions closed

Berserk, Monster and Tongari Boushi no Atelier raws obtained specifically to test
the two things the sidecar draft left open. Neither changed the format, and three
other things fell out.

### No gatefolds exist in anything measured

Berserk was the strongest candidate available — a series known for elaborate
spreads, and its volume 1 carries **nine**, more than any volume in the corpus.
Every one is 3620×2560 against a 1810×2560 page: exactly double, to the pixel.
Tongari volume 13 and every Monster volume carry none at all.

**37 spreads measured across six series, not one anything but a clean double.**
`Half` stays as it is.

### Berserk: the densest spread test, and the metric trap again

The downloaded Berserk raw splits every spread the English release joins — 227
pages, all 844×1200, no landscape at all, against 219 English pages of which nine
are joined. The arithmetic closes: 219 + 9 = 228 against the raw's 227.

| | result |
| --- | ------ |
| plain | 90.7% paired, **11.0 runs per 100 pages** |
| spreads split | **7 runs**, 206 whole + 18 halves, 3 unmatched |

All eighteen halves of all nine spreads recovered. And 11.0 runs per hundred is
*above* the threshold separating good pairings from bad — the third time now that
unhandled spreads have made a perfectly good alignment look broken.

### Omnibus assembly works, and the seam is visible

Monster's English Perfect Edition collects two Japanese volumes per book. The two
Japanese volumes as one edition of two members, against the single English
member: **416 of 444 pages paired, 6.5 runs per hundred**, inside the band every
correct pairing occupies.

What makes it convincing is the run structure rather than the totals. The offset
holds at +1 through page 216, gaps for the ten pages of back matter the omnibus
drops, then settles at −14 for the whole second volume. **The member seam appears
exactly where Japanese volume 1 ends**, as an offset change rather than a
failure. Nothing in the format needed to change, because both sides flatten
before anything else happens.

### Physically scanned raws work

Monster's Japanese original is a genuine physical scan from 2002 — pages
alternate between 736×1072 and 744×1072, which is what per-page crop variation
looks like. It still aligns at 93.7%. This was the untested axis behind the
autocrop experiment, and the answer is that the hash tolerates it without help.

### Tongari confirms the pattern on a fifth series

Raw volume 1 is 213 pages at 720×1024; the English release is 208 pages at
3450×4913 with four joined spreads. Spread handling takes it from **14 runs to
6**, recovering all eight halves, at 94.4% paired.

Two things worth noting. The four spreads measure 6900×4913 against a 3450 page
width — **exactly double again**, which is the fifth series to say so. And the
resolution gap between the two editions is **4.8× linear**, the widest in the
corpus by some way, which it aligns across without difficulty. Whatever the hash
is sensitive to, absolute resolution is not it.

### One Piece across three eras, and a wide page that is not a spread

Volumes 1, 60 and 105 of a series running since 1997, against the English
release. Raw resolution climbs with the era — 760×1200, 1005×1600, 1304×2048 —
which is the digital transition visible inside one series.

| Pair | plain | spreads split |
| ---- | ----- | ------------- |
| raw v01 → en v001 | 16 runs, 94.8% | 10 runs |
| raw v60 → en v060 | 16 runs, 95.2% | 6 runs |
| raw v105 → en v105 | **46 runs, 71.4%** | **8 runs** |

Era costs nothing: the oldest and newest volumes both align. What looked like a
failure on volume 105 was entirely spreads — the English release carries
**twenty-two** of them against three in volume 1 and five in volume 60.

**Spread density varies by an order of magnitude within one series**, which is
worth knowing before treating any per-volume number as characteristic.

### The fragmentation metric is unusable without spread handling

Volume 105 unhandled scores 46 runs across 210 pages, or **21.9 runs per hundred**
— sitting exactly on the boundary of the band that was supposed to identify a
mislabelled file. A perfectly good pairing is indistinguishable from a bad copy
on that measure alone.

This is the fourth case where unhandled spreads made a correct alignment look
broken, after Berserk at 11.0, the Go-Toubun colour edition at 10.6, and Kaguya.
The rule is not "fragmentation identifies bad copies" but **"fragmentation
identifies bad copies once spreads are expressed"**. Measured with spreads
handled, the same volume is 8 runs, or 3.8 per hundred.

### A letterbox has to come off both sides equally

Tongari volume 9 is not scanned like the rest of the series. Every page is a
joined spread letterboxed into 1920×1080, with flat white bars taking twenty per
cent of the width. Unhandled it scores **88 runs** across 180 slots — FRAGMENTED,
indistinguishable from a mislabelled file — because splitting a padded page down
the middle leaves each half a fifth blank, and that shifts the content within the
hash grid enough to wreck the comparison against a clean page.

Cropping the bars off with a fixed `sips` crop, 197 pixels each side, takes it to
**3 runs** at 176 of 180 slots paired. So the bars are the whole problem, and
removing them is the whole fix. Getting a detector to reproduce that took three
attempts, and the two failures are the interesting part.

**Per-page trimming made things worse across the corpus.** Kaguya went from 8 runs
to 15, Go-Toubun 5 to 7, Oshi no Ko 6 to 11, and Tongari only reached 81. Pages
whose art reaches the edge trim less than their neighbours, so the content lands
differently in each hash. This is the ink-bounding-box autocrop failing again from
a different direction: any crop that moves with what a page happens to contain
adds more variance than it removes. The trim has to be **per member** — a
letterbox is a property of how the volume was packaged, identical on every page.

**A per-member minimum detected nothing.** Sampling ten pages and taking the
element-wise minimum gave no regressions but left Tongari at 75 runs. The cause is
that the first page sampled is the cover, which is full-bleed and has no border at
all, so the minimum is zero and the trim is cancelled for the entire member. A
**median** across samples survives a handful of such pages.

**The bars were then found, and the alignment got worse still** — 98 runs. Printing
the measured border per page shows why:

```
Dl-Raw.Net-03.jpg   1920x1080   left=8   right=200   top=0   bottom=0
Dl-Raw.Net-04.jpg   1920x1080   left=8   right=200   top=0   bottom=8
Dl-Raw.Net-05.jpg   1920x1080   left=8   right=200   top=3   bottom=0
```

`right=200` is the true padding. `left=8` is not: a single-column scan seam near
the left edge stops that scan dead. Cropping those unequal amounts leaves the
content off-centre — the midpoint of `[8, 1719]` is 863 where the true centre is
960 — so **every spread gets split in the wrong place**, which costs more than the
bars did.

The fix is not a better detector. It is that **a letterbox is centred by
construction**: the padding exists to fit an image of one aspect into a frame of
another, so an honest measurement of it is symmetric, and an asymmetric result
means one side was mismeasured. Evening each opposing pair up to the larger of the
two — the smaller being the mismeasurement — gives:

| Pair | before | after |
| ---- | ------ | ----- |
| **Tongari v09 (padded)** | **98 runs, 139/180 paired** | **7 runs, 176/180 paired** |
| Go-Toubun v01 | 5 runs | 5 runs |
| Kaguya v01 | 8 runs | 8 runs |
| Oshi no Ko v01 | 6 runs, 215 paired | 6 runs, 216 paired |
| unrelated works | refused | refused |

The exposure is a page with a genuine one-sided margin, where taking the larger
over-crops the other side. Bounded by the existing quarter-per-side cap, and
measured across five series without regression.

Two general lessons. Detection working is not the same as alignment improving —
the first attempt found 90 spreads correctly and still produced a worse result
than finding none. And **for a joined spread the centre of the page is load-bearing
geometry**, not just a framing detail: anything that moves it moves the line the
two pages are cut along.

### Not every wide page is a spread

One Piece volume 60 opens with its **wraparound dust jacket** scanned as a single
image — front cover, spine, back cover and both inside flaps, carrying the ISBN
barcode and the price. It is 3450×1600 against a 1005-wide page: **3.43 times**.
Detection was `width > height`, so it was being split down the middle into two
halves corresponding to nothing, and two more pages were being invented.

Detection is now a ratio bound rather than a landscape test: a page is a spread
when its aspect is 1.7 to 2.3 times the median aspect of the edition's portrait
pages. Volume 60 drops from three spreads to one, which is correct.

The bound also records something the earlier measurements got slightly wrong.
**Spreads are not exactly double.** They were in the clean digital releases
measured first, but across the wider corpus they run 1.95× (One Piece English,
trimmed) to 2.13× (One Piece raw). Anything demanding an exact doubling would
have missed both.

### Bug: an edition that is entirely spreads had no spreads detected

Hunting for labelled bad copies in the One Piece collection turned up something
better than the thing being looked for. Ninety-six volumes have a second copy at
roughly half the page count, which looked like truncation. It is not.

Volume 1's "short" copy is 107 images of **1524×1200 — landscape, every page** —
against the good copy's 210 of 760×1200. Looking at one confirms it: two facing
pages side by side, filenames ending `-000a` and `-000b`. It is the same volume
**published two pages per image**, which is a common scan convention and not a
defect at all.

The solver refused it outright, and the reason was a bug. Spread detection
measures each page against the median aspect of the edition's **portrait** pages,
and this edition has none, so the baseline fell to zero and nothing was detected
as a spread. A 107-page landscape sequence then had nothing in common with a
210-page portrait one.

Fixed by falling back: with no portrait page anywhere, the typical page *is* a
spread, so an ordinary single page is half of one. The effect on that pair:

| | before | after |
| --- | ------ | ----- |
| spreads detected | 0 | **107** |
| slots paired | **0 of 107** | **202 of 214** |

The rest of the corpus is unchanged, dust jacket still correctly excluded.

### The threshold-tuning plan failed, and was worth doing anyway

The One Piece collection looked like a gift for calibrating the bad-copy
thresholds: ninety-six volumes with a second, much shorter copy, apparently
ninety-six labelled good/bad pairs against a shared English release, where the
existing thresholds rest on three.

**They are not bad copies.** Every one is a spread-per-image edition of the same
volume, and once detection was fixed all eight sampled pairs align well:

| Volume | good copy | spread-per-image copy |
| ------ | --------- | --------------------- |
| 1 | 205 of 210 paired, 4.8 runs/100 | 202 of 214, 7.9 |
| 5 | 189 of 194, 1.5 | 187 of 198, 4.0 |
| 9 | 204 of 210, 2.4 | 203 of 214, 2.8 |
| 20 | 205 of 218, 3.2 | 204 of 225, 4.4 |

So the premise was wrong and the plan produced no bad examples at all. What it
produced instead was **eight more confirmed-good examples**, all between 1.5 and
7.9 runs per hundred, plus the spread-detection bug above and the hole in the
metric below. A worthwhile trade, but the honest accounting is:

- **Good side: around twenty examples, all at or under 8 runs per hundred.**
- **Bad side: still the same three** — [Oshi no Ko] v01's mismatched copy at 34,
  its v03s variant at 22, Kobayashi's Chromatic Edition at 45.
- The range between 8 and 22 remains unpopulated, so where exactly the line sits
  is still unmeasured. `MIN_SEPARATION` is likewise untuned.

Finding genuine bad copies is harder than expected precisely because most
odd-looking files turn out to be legitimate editions in an unexpected packaging.
That is itself worth knowing: **"looks wrong" has meant "packaged differently"
every time so far**, and every investigation of one has found a bug in taiyaku
rather than a defect in the file.

### The fragmentation metric has a hole at the bottom

The same investigation exposed a flaw in a measure this document has been
treating as reliable. A refused pair produces exactly **one run** — the single
gap covering everything — which on a runs-per-hundred-pages scale reads as
**0.9, better than any correct alignment in the corpus.**

So fragmentation is not monotonic in quality:

| Outcome | runs per 100 pages |
| ------- | ------------------ |
| refused entirely | **~1** |
| correct alignment | 3 to 8 |
| aligned but wrong | 22 to 45 |

A threshold of "at most 8 runs per hundred is good" classifies a total refusal as
excellent. **Coverage has to gate it**: check that something was paired at all,
then use fragmentation to judge what was. Earlier notes here said coverage was
the wrong metric and fragmentation the right one — that holds only among pairs
that produced an alignment, and the qualification was missing.

### Bug: unreadable page formats were skipped in silence

Pointing the aligner at a directory of archival TIFFs reported `A: 0 pages` and
carried on cheerfully. The files were present and readable; `.tif` simply was not
in the recognised extension list, so every one of them was ignored without a
word. An edition that reads as empty is nearly always a mistake, and saying
nothing is the worst available response.

Fixed twice over. TIFF is now recognised, and a member that yields no pages is an
error naming what it found instead:

```
no page images. It holds .psd, .txt, none of which taiyaku reads as a page
```

The second half is the part that generalises — it catches any unsupported format,
not just the one that happened to turn up.

### Aside: `image` refuses oversized files rather than dying

Those Color Walk scans are 430 MB per page. Once TIFF was recognised, decoding
one returned `Memory limit exceeded` from the `image` crate's own guard, peaking
at 768 MB resident and exiting cleanly. That guard matters more than it looks:
fingerprinting runs across twelve cores, so without it a directory of files this
size would try to decode twelve simultaneously.

Worth remembering that Color Walk is an artbook rather than a colour edition of
the manga — it is not a candidate for alignment at all, and its only real use to
this project would be as a negative control.

### Practical: 7-Zip cannot extract these RARs, libarchive can

`7zz` fails on all three RAR5 archives with `ERROR: Unsupported Method`,
extracting only the handful of stored entries and writing **zero-byte files for
the other 194** — a silent-looking failure that reports success at the shell
until the sizes are checked. `bsdtar`, which is libarchive and ships with macOS,
extracts them completely.

Worth remembering as an ingestion note: if taiyaku ever reads RAR, libarchive is
the implementation that actually works on real-world files, and it is already
present on macOS.

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

### A canonical form, and what it buys

Ranked by what each actually prevents, measured against this library:

1. **Zero-pad page numbers to a fixed width, 1-based, one scheme per member.**
   Prevents the two errors that are wrong before any decoding: a 0-based copy
   offsetting by one against its 1-based twin, and `_01.jpg` inserts sorting to
   the end of a volume that numbers its other pages `0001.jpg`.
2. **Put the volume number in every member's filename, and the chapter number
   too where it applies** — `Series - v01 c005.5.cbz`. Kaguya's fan release
   already does this and it is the only membership hint in the library, even
   though it is wrong twice. A wrong hint that can be checked beats no hint.
3. **Keep role tags rather than stripping them.** `[Cover]`, `[ToC]`, `p046-047`
   are the most valuable naming in the corpus: the last of those identifies a
   joined spread without decoding anything, and the volume's spreads are what
   the schema now has to model.
4. **One directory per edition, one archive or directory per member.** Do not
   mix `Series v01`, `Series_v01` and `DLRAW.TO_Series v01` in one folder, which
   this library does.
5. **Suffix alternate copies distinctly** — `v01`, `v01b` is already the
   convention here, but nothing says which is good. Since alignment quality
   identifies the bad one, record the verdict rather than guessing at the name.

What this deliberately does not ask for: a single container format, a flat page
layout, or stripped metadata. The tool handles all of those already, and the
metadata is load-bearing.

Sorting still has to be decimal-aware regardless — `c005.5` sorts before `c005`
under plain lexicographic comparison no matter how tidy the names are.

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
