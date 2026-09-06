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

### The solver is symmetric on the pairing relation, 2026-07-30

Measured because the star's cost depends on it. `topology` composes
`solve(spoke, hub)` with `solve(hub, spoke)` — two independent solves — but a
sidecar stores **one** edge per spoke and has to reverse it to walk the first
leg. If reversing were lossy, the figures above would not describe the path a
reader actually walks, and every spoke would need both directions stored.

Five pairs solved both ways, comparing every paired run against its transpose:

| Pair | Paired runs each way | Result |
| ---- | -------------------- | ------ |
| Go-Toubun raw-mono ↔ en-mono | 3 | identical transposed |
| Go-Toubun raw-mono ↔ en-fan | 8 | identical transposed |
| Go-Toubun en-fan ↔ en-color | 11 | identical transposed |
| Kaguya raw-mono ↔ en-mono | 4 | identical transposed |
| Kaguya en-fan ↔ en-color | 13 | identical transposed |

Only the one-sided gap runs differ, which is by construction: a gap records
pages of the `from` edition with no counterpart, and each direction has its own.
The widest length mismatch available was included deliberately — Go-Toubun
raw-mono at 195 slots against en-fan at 240.

**Worth recording because the code says otherwise on paper.** `calibrate` is
asymmetric: `signal` is the median of the best match *per page of A*, so
swapping the sides changes it whenever the editions differ in length, and with
it the floor. It does not reach the DP's output on any material measured. The
asymmetry is real; its consequence is not.

So `invert` is lossless with respect to the relation, and N−1 stored edges is
enough.

### The shipped path, measured, 2026-07-30

The figures above are page-level and use two independent solves. What a reader
walks is different in two ways — one stored edge per spoke, reversed, and slots
rather than pages — so it is measured separately, by the
`TAIYAKU_CORPUS`-gated test in `crates/taiyaku-cli/src/corpus.rs`. Go-Toubun
v01, five editions built through the merge path, every ordered spoke pair
against a direct solve:

```
2427 page comparisons | loss 0.91% | disagreement 0.00% | 21 blank pages not compared
```

**Loss is lower than the page-level figure, not higher**, and that is the useful
finding: 0.91% against 2.12% on the same volume. `topology` does not split
spreads, so a spread one edition joins and another splits is simply lost to it,
while the sidecar path addresses slots and recovers it. Spread handling pays for
itself once more.

**Blank pages are excluded, and counted rather than dropped in silence.** The
first run disagreed exactly once in 2,448 — `en-color` page 1, direct saying
page 5 and composition saying 43. The page is blank and its dhash is *exactly*
zero, 0 of 256 bits set, because a dhash asks whether the left pixel of each
pair is brighter than the right and on a uniform page none is. Every blank page
in every edition therefore carries the same hash, so where a solve sends one is
arbitrary and comparing two arbitrary answers measures nothing. This is the
hazard already recorded under "Blank pages match each other perfectly and lie
about it"; detecting them in the solver remains open, and the test only declines
to measure them.

### Re-running the baseline, and why it moved

Re-run 2026-07-30 with the composed-verdict column added. **The material
changed, not the solver** — nothing in `solve`, `compose`, `counterpart` or the
fingerprinting was touched by the work that prompted the re-run.

| Series | Comparisons | Loss | Disagreement | Inverse loss |
| ------ | ----------- | ---- | ------------ | ------------ |
| Go-Toubun | 2,448 (was 2,316) | 2.12% (was 1.90%) | **0.00%** | 0.00% |
| Kaguya | 2,583 (was 2,499) | 1.78% (was 1.56%) | 0.39% (was 0.16%) | 0.70% (was 0.32%) |

The comparison count is exactly 3 × (sum of spoke page counts), which pins the
editions:

- Go-Toubun: 2,316 = 3 × (195 + 187 + 199 + **191**); today 3 × (195 + 187 + 199 + **235**).
- Kaguya: 2,499 = 3 × (214 + 211 + 210 + **198**); today 3 × (214 + 211 + 210 + **226**).

Every edition matches its old page count except `en-fan`, which has gained 44
pages in Go-Toubun and 28 in Kaguya. Both are the multi-file fan translations —
seven chapter archives and twelve respectively — so the corpus grew them at some
point between the two runs. **Record the spoke page counts with any future
run**, since the comparison total is the only thing that caught this.

Go-Toubun's disagreement is still exactly 0.00%, which is the number the
topology decision rests on. Kaguya's rise to 0.39% falls under the criterion set
out above: it implicates the solver rather than the topology, on the pairing
already the weakest in the corpus by every other measure.

### `trust` keeps taking the worse leg, and now that is measured

Composition can *merge* runs: two fragmented stretches of one leg can land
contiguously through the other and compress into a single clean run. So judging
the composed runs is not the same operation as taking the worse of the two legs,
and it could in principle report `sound` where the worse leg says `fragmented`.

`topology` now reports both and counts the disagreements. **Zero of twelve pairs
on both series.** `Sidecar::trust` keeps taking the worse leg; the alternative
would have made no difference on any material measured, and the worse leg is the
conservative reading.

One caveat on that column: `topology` does not split spreads, and the
fragmentation metric is unusable without spread handling — Go-Toubun reads
`FRAGMENTED` on all twelve pairs for that reason alone. What the column measures
is the *agreement* between two ways of judging, not the verdict's absolute
value.

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

**The solver got that fix; the sidecar writer did not — found 2026-07-30.**
`cached_edition` sorts naturally before fingerprinting, so the alignment is
correct, but `describe_edition` writes `members` straight from the command line.
A glob that is not already in natural order therefore produces a document whose
member list disagrees with the page sequence its own runs address. Kaguya v01's
fan translation reproduces it: `*.cbz` gives `c10.5, c10, c1, c2, …` and the
solve used `c1, c2, … c10, c10.5`. Nothing in this repository reads member order
— `Edition::slots` sums page counts and nothing else — so the bug is invisible
until an external reader concatenates the members itself, which is precisely what
the reader will do.

**Fixed 2026-07-30, by ordering once rather than per consumer.** `split_editions`
now sorts both sides naturally, so every command downstream of it — `align`,
`verify`, `topology` — sees a member list already in reading order. Putting it
there rather than in `describe_edition` was the point: three consumers had to
agree and only one of them was sorting for itself, and two more were reading the
glob's first member without anybody noticing. `slug` and `label` name an edition
after `members.first()`, so the same Kaguya command labelled the edition
`… v1 c10.5.cbz + 11 more`. Both halves of the bug had one cause.

Measured on the reproduction itself, same command both ways:

| | recorded member order | label |
| --- | --- | --- |
| before | `c10.5, c10, c1, c2, … c5.5, c5, …` | `v1 c10.5.cbz + 11 more` |
| after | `c1, c2, … c5, c5.5, … c10, c10.5` | `v1 c1.cbz + 11 more` |

The solve was already correct and is unchanged — 23 runs either way. What moved
is only what gets written down, which is the half a reader consumes.

`pages::read_edition` and `cached_edition` keep their own sorts. They are
idempotent now, and `corpus.rs` calls `cached_edition` without going through
`split_editions`, so removing them would re-open the bug on the one caller that
does not take the funnel.

**The same hazard one level down, aimed at readers this project will not write.**
Fixing the member order raised the question of what else about the page sequence
lives only in `pages.rs`. The answer was: all of it. `sidecar.md` said an edition
is "assembled by concatenating its `members`" and stopped there, so how a member
itself becomes pages was never written down. That is unfixable by code — it is
the format's contract with Komga, Stump, the PWA, and anything else — and it
fails silently, because a reader deriving a different sequence still matches
every `sha256` and every `pages` count in the file.

Measured over the 57 cbz archives in the corpus, one row per rule a reader has
to get right:

| Rule | Archives where getting it wrong changes the sequence |
| ---- | --------------------------------------------------- |
| Entries sort by name, not by stored order | **32 of 57** |
| Sort on the whole path, not the basename | **2 of the 7** that nest pages in folders |
| Non-image entries are not pages | **12 of 57** (all `ComicInfo.xml`) |
| Natural rather than lexicographic, *within* an archive | **0 of 57** |

The second fails worst and is the least obvious. Go-Toubun's Full Colour edition
nests pages under `Chapter 15 - What They Have Accumulated/` and puts a
`ZBlank (1).jpg` in each chapter folder; sorted on the basename, every chapter's
blank sorts to the front and the chapters interleave. Not an off-by-one, a
shuffle.

The last row is the useful negative. Natural ordering makes no difference at all
*inside* an archive on this corpus — page names there are zero-padded often
enough that lexicographic agrees — so a third-party reader doing the naive thing
will not notice. It is load-bearing one level up, on `members`, where
`c005.5.cbz` sorts before `c005.cbz` and cost 88.7% monotonic against 91.3%. The
spec says natural at both levels anyway, since specifying the rule that is only
sometimes needed is cheaper than specifying two rules.

**What generalises is the rule, not the counts.** CBZ is a zip plus a
convention: the format has no page-order field, so every reader in the ecosystem
recovers order by sorting entry names, and central-directory order is an
artifact of whichever tool last packed the file. Repacking is routine, which is
why a third of these disagree with their own stored order. The frequencies above
describe this library; the failure modes describe the format.

Written into `sidecar.md` as "How a member becomes pages", marked normative, and
pinned by two tests in `pages.rs`. Those tests passed the moment they were
written — the code has always done this — which is the point: the behaviour was
correct and unstated, and only the second half was a bug.

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
alignment to decide — c005.5's first four pages correspond to nothing, while its
last five are volume content. **Those four were called "scanlator title and
credit pages" here until 2026-08-05; they are the volume's own front matter** —
cover, title page, table of contents, decorative half-title — and they pair with
nothing only because `c000` carries a second copy of the same front matter that
the 1:1 rule matched first. See "Scanlation credit pages are already visible in
the alignment", which corrects the same mistake in its own table.

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

This hole was later cited as the reason runs per hundred pages was retired as
the fragmentation measure. That overstates it: the coverage gate described here
already neutralises the hole, and any fragmentation measure needs the same gate
— average run length reports 0.0 for a refusal, which is below any threshold.
See "The margin survey, and where the refusal threshold sits" for what actually
decided the swap, the replacement measure, and the threshold Task 5 settled on
it.

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

## Volume-scale repagination works, and art books do not

### Two tankobon into one deluxe volume

`Monster` exists in the library as 18 Japanese tankobon and as the English **Perfect
Edition**, which is 9 volumes — two tankobon per deluxe volume. That is the
"release granularity" drift the design listed first, at volume scale rather than
chapter scale, and nothing in the corpus had tested it.

Raw volumes 1 and 2 together are 444 slots; Perfect Edition volume 1 is 438.

| Pair | runs | paired | per hundred |
| ---- | ---- | ------ | ----------- |
| raw v01 + v02 → PE v01 | **13** | 427 of 444 (96.2%) | 2.9 — sound |
| raw v01 alone → PE v01 | 17 | 219 of 226 | 7.5 — sound |

It works with no changes. Two things are worth drawing out.

The **seam is found without being told about it**. One gap covers slots 217–230,
which straddles the boundary between the two raw volumes — volume 1's afterword and
ads followed by volume 2's title page and contents. Fourteen slots that exist on
the raw side and nowhere in the deluxe edition, correctly marked one-sided rather
than forced into a pairing.

The **half-covered case also behaves**. Aligning raw volume 1 alone against a
deluxe volume containing two tankobon leaves half of B with no counterpart, and the
result is still sound at 7.5 runs per hundred. Fragmentation rises, which is the
right direction, but not into the refusal band. Coverage, not fragmentation, is
what identifies a partial pairing.

### Multi-member alignment is the general answer to granularity

The chapter-to-volume case and the tankobon-to-deluxe case turn out to be the same
mechanism seen from opposite ends: several members on one side, one on the other.
`align a... -- b...` already concatenates members before solving, so neither needed
a special path. The only thing that varies is which side is plural.

### Art books cannot be aligned, and the solver does not say so

One Piece **Color Walk 10** is 151 pages at 10200×14039, 430 MB each. Reading them
works — the TIFF and large-page handling was built for exactly this, and four pages
fingerprint in 8.6 s at a 3.0 GB peak. Aligning them does not, and the way it fails
is the point.

Four pages sampled through the book, against One Piece raw volume 105:

```
4 runs | 4 whole pairings | 0 involving a spread half | 0 unmatched slots
```

Every page paired. Nothing refused. The margins say what is really going on:

| art book page | matched to | margin |
| ------------- | ---------- | ------ |
| img001 | v105 p117 | 0.000 |
| img040 | v105 p148 | −0.012 |
| img075 | v105 p171 | 0.020 |
| img110 | v105 p176 | −0.047 |

All four sit at zero, and two are **negative** — the page chosen was a worse match
than the runner-up, and was picked only because monotonicity forbade the better
one. There is no discrimination here at all. A real pairing on this corpus runs an
order of magnitude above it.

Looking at what the pages actually contain explains why, and says something about
whether the feature is worth wanting:

- **img040** is rough pencil sketches on a tinted background, with the captions
  blanked. No manga page it could correspond to.
- **img075** is a Jump colour spread — plausibly derived from a manga page, but
  re-cropped from a wider original and printed over a decorative border.
- **img110** is a composite: one illustration plus merchandise art, character
  sheets, a QR code and Japanese captions.

So roughly one page in three has any manga counterpart, and even that one is
re-composed, re-cropped, and framed by decoration that occupies enough of the page
to dominate a 16×16 hash. The artwork-is-identical premise that the whole approach
rests on does not hold for an art book.

**Reading art books is already supported and needs nothing.** They are directories
of images, and taiyaku decodes them.

**Linking art book pages to the manga pages they came from is a different problem**,
and the DP solver is the wrong instrument for it in three separate ways: the
correspondence is sparse rather than near-total, unordered rather than monotonic,
and cross-composition rather than same-artwork. It would want a nearest-neighbour
search with a margin threshold instead — the same similarity and confidence
machinery minus the alignment — and it would want a hash that survives re-cropping,
which dhash does not.

### A small needle in a large haystack always aligns

The art book result exposes a gap in the guard rails that is not specific to art
books. When side A is much smaller than side B, the DP can insert gaps in B freely,
so **A is always fully covered** and the unmatched-slot count is zero no matter how
unrelated the two are. Fragmentation does catch this case — four runs over four
slots is 100 per hundred — but only because A was tiny. The refusal that works for
two unrelated volumes of similar length works by coverage, and coverage is
meaningless in this direction.

Whatever eventually decides "these two do not correspond" needs to look at margins,
not just at counts. The margin table above separates cleanly from every real
pairing measured; the run counts do not.

### New corpus material, surveyed not yet ingested

Everything below has a Japanese raw and at least one English counterpart already in
the library, which roughly triples the number of series available for testing.

| Series | Raw | English editions |
| ------ | --- | ---------------- |
| Monster | 18 tankobon | 18-volume, Perfect Edition (9), Perfect Edition Colored (9) |
| Nausicaä | 7 volumes | Deluxe Edition (2 volumes + extra) |
| Chainsaw Man | 22 volumes | standard, Digital Colored |
| Hunter × Hunter | 36 volumes + カラー版 colour (34) | standard, Official Colored |
| Dungeon Meshi | 12 volumes | standard |
| Bocchi the Rock | v01–v04, v01–02b, v05, anthology v01–05 | standard, anthology, Gaiden |
| Tongari Boushi no Atelier | v01–12, v13, v16 | Tongari Boushi no Atelier |

Three cases in there are worth going after specifically:

1. **Nausicaä, 7 raw volumes into 2 deluxe volumes.** A 3.5:1 repagination, harder
   than the 2:1 Monster case that already works.
2. **Bocchi volume 1 exists three times** — twice inside the eight-volume archive
   (`v01-02b` and `v01-04`, different publishers' digital releases) and once from
   Manga-Zip. Three independent scans of one volume is the material for testing
   base selection, which is the thing that has been wanted and never had data.
3. **Monster in three editions at two paginations**, including a coloured Perfect
   Edition, which puts colour and repagination on the same work.

One archive is empty: `Berserk v01-40 (1990-2018) (Digital SD) (KG Manga)` is a
zero-byte directory, the 7-Zip failure mode from earlier.

## Choosing a base edition, and a mislabelled file caught in the act

Three series turned out to carry more than one raw scan of the same volume, which
is the material base selection needed and never had.

### Chainsaw Man volume 1, three scans and a colour edition

Four independent raws of one volume, each aligned against the same English release:

| Raw scan | pages | runs | paired |
| -------- | ----- | ---- | ------ |
| `v01b` | **199** | **3** | **183** |
| `v01` | 198 | 5 | 182 |
| `v01-02/第01巻` | 198 | 5 | 182 |
| `カラー版 第01巻` (colour) | 198 | 7 | 182 |

`v01b` wins on every axis at once — one page more than the others, the fewest runs,
the most pairings — so there is nothing to trade off here and it is the base. That
will not always be true, and when it is not the ordering has to come from somewhere.
Pages and pairings measure content; runs measure how cleanly the correspondence
holds. Content should win, because a missing page cannot be recovered from a
cleaner alignment of the pages that remain.

Aligning the scans against **each other** is what makes the choice cheap:

| Pair | runs | paired | unmatched **on the `v01` side only** |
| ---- | ---- | ------ | ----------------------------------- |
| `v01` vs `v01-02/第01巻` | 1 | 198 of 198 | 0 |
| `v01` vs `v01b` | 2 | 198 of 198 | 0 |
| `v01` vs colour | 6 | 195 of 198 | 3 |

A single run covering every slot with nothing unmatched is the identity output the
design asked for, arrived at from two files that are not byte-identical. Two of
these three scans are the same pagination; there is no decision to make between
them and only one needs aligning against anything else. The colour edition is the
one that genuinely differs, by three pages.

That is the shape of the base-selection tool: align the candidates against one
another first, collapse the ones that come back as identity, and only then spend
alignments on the distinct ones.

**That last column is one-sided, which matters more than it looks.** `align`
counts unmatched slots for the A side, so a zero there says the left-hand scan
is wholly covered and says nothing about the right-hand one — and `v01b` carries
a slot the other two do not, which is the whole reason it wins the table above.
Run as a duplicate set, the corpus's three raws come out as one identity pair
and one *containment* pair rather than two identity pairs; see "Duplicate
resolution as a command, and half the identity claims above are
containment" below.

### Bocchi volume 1, where one of the three was not the same work

The same experiment on Bocchi looked like it was going the same way and then did not:

| Raw scan | pages | → EN v01 |
| -------- | ----- | -------- |
| `v01-02b/１巻` | 124 | 6 runs, 123 paired |
| `v01-04/第01巻` | 125 | 7 runs, 123 paired |
| `Manga-Zip.info_Bocchi the Rock v01` | 185 | **1 run, 0 paired — refused** |

The first two are **not** the identity case, though the page counts come within
one of saying so. Run as a duplicate set they measure **2 runs, 124 of 125**:
every slot of the 124-slot copy pairs, and the 125-slot copy carries one more
that nothing on the other side reaches. That is containment — the larger copy
holds everything the smaller one does and something besides — which is the
distinction base selection turns on, and it is why the ranking puts the 125-slot
copy first.

**The paragraph above said "the identity case again: 1 run, 124 of 124, nothing
unmatched" until the command was run over the same three directories**, and two
things were wrong with that. `align` reports unmatched slots for the A side
alone, so "nothing unmatched" was a claim about one copy read as a claim about
both. And the Chainsaw Man section above keeps a 198-against-199 pair — the same
shape — distinct and ranks the larger copy first, so this document was using
*identity* two ways at once. The stricter reading is the one that makes base
selection work, and it is the one the command implements.

The third was refused against the English edition and against both other scans.
It was refused because it is a different work. The cover reads ぼっち・ざ・ろっく！
**外伝** 廣井きくりの深酒日記 — the *Gaiden* spinoff, drawn by くみちょう from
はまじあき's original, following a different character. The file is named as if it
were main-series volume 1.

The confirmation is that it aligns against the edition it actually corresponds to,
which the library already had under the right name:

| Pair | result |
| ---- | ------ |
| the mislabelled file → **English Gaiden v01** | **8 runs, 182 of 185 paired (98.4%)** |
| the mislabelled file → main-series raw, all three | refused |
| main-series raw → English Gaiden | refused |

This is the first time the refusal has fired on a file that was not chosen to be
wrong. Every earlier test of it paired two volumes picked *because* they were
unrelated. Here the file arrived labelled as a third scan of Bocchi volume 1, was
about to be added to the corpus as one, and taiyaku both rejected it and identified
what it really was. The identification the docs describe as a side effect of
alignment is not hypothetical.

It also says something about how a library ingest should be shaped. The natural
move on finding several files claiming to be one volume is to pick between them.
The right move is to align them against each other first, because "these are the
same scan", "these are different editions of the same volume" and "one of these is
not this volume at all" are three different answers and only the last one is
urgent.

## Four volumes into one, and working out which four

`Nausicaä` is 7 Japanese volumes against a 2-volume English Deluxe Edition. That is
a deeper repagination than the 2:1 Monster case, and unlike Monster the split is not
even — 7 does not divide by 2 — so which raw volumes land in which deluxe volume is
not something you can work out from the counts.

Aligning raw volumes 1 to 4 against Deluxe volume 1, 600 slots against 576:

```
20 runs | 542 whole pairings | 11 involving a spread half | 47 unmatched slots
```

553 of 600 slots paired, 3.3 runs per hundred, sound. **4:1 repagination works
unchanged**, as 2:1 did.

The boundary can be measured rather than assumed. Adding one raw volume at a time:

| Raw volumes | paired | unmatched |
| ----------- | ------ | --------- |
| v01 | 139 of 144 | 5 |
| v01–v02 | 268 of 288 | 20 |
| v01–v03 | 419 of 452 | 33 |
| v01–v04 | 553 of 600 | 47 |
| **v04 alone** | **145 of 148** | **3** |

Volume 4 alone pairs 145 of its 148 slots into Deluxe volume 1, so it is inside it,
and the edition splits 4 + 3 rather than evenly. **A one-volume probe answers the
containment question directly** — align each raw volume alone against the deluxe
volume and read off coverage — which is cheaper than searching over groupings and
does not need the answer in advance.

The 47 unmatched slots are the same shape as Monster's 17: each raw volume carries
its own cover, title page, afterword and ads, and a deluxe edition that swallows
four of them keeps one set. Unmatched slots scale with the number of members
absorbed, not with how badly the alignment went, which is another reason coverage
has to be read against the pairing structure rather than on its own.

Nausicaä is also the most spread-dense material in the corpus: 28 joined spreads
across 572 raw pages, against 3 in the deluxe edition. It is A4-sized artwork and
Miyazaki uses full spreads heavily, so this is the case where spread handling earns
the most, and it holds up.

## Sorting 109 volumes' worth of duplicates

The One Piece collection is the messiest thing in the library and the reason the
identity question came up. Applying the base-selection method to it end to end.

### Inventory first, and most of it is not volumes

579 directories under the release. Only **141 are volume directories**; the other
438 are weekly Jump issues, calendars, databooks and chapter compilations, at 15 to
28 pages each. Those 141 resolve to **120 distinct volume-editions, 19 of which have
more than one copy** — 17 with two, 2 with three.

### A content signature resolves the free cases without transferring anything

Before aligning anything, take per-copy file count, total bytes, and a hash of the
sorted file-size list. **3 of the 19 came back identical on all three** — the same
scan filed twice under different groupings. Those need no alignment and no
transfer; either copy will do.

That left 16 needing real comparison, about 4 GB rather than 12.

### All pairs, not everything against the first copy

The first pass compared each copy against copy 0 and reported volume 77 as three
mutually unrelated files. It is not. Comparing **all pairs** shows:

| Pair | runs | paired | verdict |
| ---- | ---- | ------ | ------- |
| v077 c0–c1 | 104 | 184 of 236 | fragmented |
| v077 c0–c2 | 100 | 186 of 236 | fragmented |
| **v077 c1–c2** | **1** | **234 of 234, none unmatched** | **identity** |

c1 and c2 are the same edition and c0 is the outlier — a different crop at
812×1320 against 764×1200, a different aspect as well as a different size. Anchoring
on copy 0 hides that, and copy 0 is exactly the copy most likely to be the odd one
out, because the sort that produced it was by page count. Volume 69 has the same
shape: c1–c2 is 3 runs at 99% where either against c0 is 8 or 9.

**Duplicate resolution is a clustering problem, not a ranking problem.** Find which
copies are the same first; only then choose between the ones that differ.

### A completely different pagination is seen through, and only one of these two is identity

The best result in the set is the one that looks least likely on the file listing.

| Volume | copies | slots | measured |
| ------ | ------ | ----- | -------- |
| v092 | 200 images vs **101 images** | 200 vs **202** | 1 run, 200 paired — c0 lies wholly **within** c1 |
| v077 | 234 images vs **214 images** | 234 vs 234 | 1 run, 234 of 234 on both sides — **identity** |

Half the image count and the same work, because the smaller copy is the
all-spread edition — two facing pages per image — and slot expansion puts them back
on the same footing. A file listing says these are different; the alignment says
they are the same content, and it is right.

**v092 is containment rather than identity, and this section claimed identity
until the command measured both sides.** Every one of c1's 101 images detects as
a spread, so expansion gives it 202 slots against c0's 200. The run is
`s0=0-s199=199 -> s1=0L-s200=100R`: it reaches all 200 of c0's slots and never
reaches c1's first or last half-slot. `align`'s "N unmatched slots" is an A-side
figure, so the original "nothing unmatched" was true of c0 and silently assumed
of c1. The root cause is stated once, in full, under "Duplicate resolution as a
command, and half the identity claims above are containment" below.

The conclusion the pair was cited for survives the correction, and v077's c1
against c2 carries it outright: **slots, not pages, are the unit the format
should address.** Page counts are a property of packaging. Slot counts are a
property of the work, and it takes both sides' slot counts to tell "the same
work" from "all of this work and a little more".

### The full classification

Twenty pairs across the sixteen volume-editions:

| Verdict | pairs | meaning |
| ------- | ----- | ------- |
| identity | 2 | same content, drop either |
| variant | 13 | same volume, genuinely different editions |
| fragmented | 3 | inspect |
| refused | 2 | not this volume |

With the 3 signature-identical sets, **5 of 19 duplicated volume-editions are
resolved with no judgement required**.

### The refusals were right, and they are a different product

Volumes 2 and 3 refused outright — 0 pairings against 202 and 198 slots — despite
both copies being named for the volume. The second copy in each case comes from
`ONE PIECE Starter Book v01-03`, and it is a **colour promotional sampler**, not a
tankōbon. Its volume 2 opens on Nami and a lighthouse in full colour, which is
Reverse Mountain, not the Buggy arc that tankōbon volume 2 contains.

Starter Book 1 behaves differently again, and instructively. It pairs 195 of 210
slots against volume 1 — 92% coverage, which taken alone reads as a good match —
but at 45 runs. The run structure says what the coverage hides:

```
s0=0-s102=102 -> s2=1L-s104=52L      <- one clean run, 103 slots
s103=103-s103=103 -> s107=54R        <- then fragments, B skipping around
s104=104-s104=104 -> s109=55R
```

It reproduces the opening of volume 1 faithfully and then diverges. That is exactly
what a sampler is, and **only the run structure distinguishes it from a real
edition**. Coverage cannot: 92% would pass any threshold worth setting.

### Two guards, and they catch different things

Searching Starter Book 2 against all sixteen local volumes, fourteen refuse cleanly
at the floor — 1 run, 0 pairings. Two do not: volume 56 admits 192 slots at 91 runs,
volume 95 admits 188 at 106 runs. Both are noise, and the tell is average run
length: **1.8 to 2.1 slots**, where a real alignment on this corpus runs to tens or
hundreds and volume 1's genuine section was a single run of 103.

So the per-pair floor rejects most unrelated material outright, and fragmentation
catches what slips past it. Neither alone is sufficient, which is worth knowing
before either gets simplified away. **Average run length is the cheaper statement of
what fragmentation measures**, and it may be the better one to expose.

### The protocol this suggests

Ordered by cost, each step only running on what the previous could not settle:

1. **Signature** — file count, total bytes, sorted size list. Settles exact
   duplicates for free.
2. **All-pairs alignment within the set.** Identity collapses copies that differ
   only in packaging, including across different image counts.
3. **Rank what remains** by content first — slots, then pairings — and only then by
   how cleanly it aligns, per the Chainsaw Man result.

On this collection that is 3 settled free, 2 more collapsed by identity, and 13
genuine choices left — down from 19 sets to look at, with the two mislabelled ones
flagged rather than silently ranked.

## Duplicate resolution as a command, and half the identity claims above are containment

The protocol above is now `taiyaku inventory` and `taiyaku duplicates`, and
running the finished thing against the corpus overturned a claim this document
made twice. Everything below is a figure from a run, not an estimate; the six
checks are gated on `TAIYAKU_CORPUS` and print what they measured.

### `align` counts unmatched slots on the A side only, which is how the error was made

This is the root cause, and it is worth stating on its own because nothing about
the output announces it. An alignment of A against B reports `N unmatched slots`
where N counts **A's** slots that paired with nothing. B's unpaired slots are not
in that number and are not printed anywhere else. So a solve that covers all of A
prints "nothing unmatched" whether B is exactly A's length or carries slots at
either end that nothing ever reaches.

Read as "nothing unmatched *at all*", that turns containment into identity — and
containment is the commoner shape. Every pair in the corpus that comes within
two slots of covering both sides, measured from **both** sides this time:

| Pair | slots | paired | what it actually is |
| ---- | ----- | ------ | ------------------- |
| One Piece v077 c1 / c2 | 234 / 234 | 234 | identity — both sides wholly covered |
| Chainsaw Man src-a / src-c | 198 / 198 | 198 | identity |
| One Piece v092 c0 / c1 | 200 / 202 | 200 | **containment** — c0 lies wholly within c1 |
| Bocchi hamaji / krc | 125 / 124 | 124 | **containment** — hamaji contains krc |
| Chainsaw Man src-a / src-b | 198 / 199 | 198 | **containment** — src-a within src-b |

This document called four of these five identity — every one but
`src-a`/`src-b`, which the Chainsaw Man section kept distinct and ranked the
larger copy first, and which is the row that shows the document using the word
two ways at once. **Two of those four are containment**, and containment is the
majority shape overall: three pairs of the five.

**So the command reports five relations, not three** — identity, `within`,
`contains`, distinct, refused — and containment is named rather than swept into
"distinct". The clustering merges on identity alone, because a container holds
content the other copy lacks and collapsing the two would discard the difference
the choice is about; the ranking already prefers the container, on slot count,
so nothing is lost by keeping them apart. Where the difference genuinely is
nothing — v077's c1 and c2, `src-a` and `src-c` — the merge still happens.

### What the six gated checks measured

```
v077, three copies    0 = 236 slots, 1 = 234, 2 = 234
                      0-1  104 runs  184 of 236  FRAGMENTED
                      0-2  100 runs  186 of 236  FRAGMENTED
                      1-2    1 run   234 of 234  sound, identity
                      clusters {0} and {1, 2};  rank c0 then c1;  decided=yes

v092, two copies      0-1    1 run   200 of 200  sound, 0 within 1 (202 slots)
                      clusters {0} and {1};  c1 first on 202 slots

bocchi, three copies  0-1    2 runs  124 of 125  sound, 1 within 0
                      0-2    1 run     0 of 125  REFUSED
                      1-2    1 run     0 of 124  REFUSED
                      the Gaiden file flagged and excluded from the ranking

chainsawman, four     0-1    2 runs  198 of 198  sound, 0 within 1 (199 slots)
raws (a, b, c,        0-2    1 run   198 of 198  sound, identity
colour) against       0-3    6 runs  195 of 198  sound
the English release   1-2    3 runs  198 of 199  sound, 2 within 1
                      1-3    4 runs  197 of 199  sound
                      2-3    6 runs  195 of 198  sound
                      clusters {0, 2}, {1}, {3};  src-b first, 199 slots,
                      191 paired in 3 runs against src-a's 190 in 5 and the
                      colour edition's 190 in 7

inventory over op-duplicates
                      dirs=36 candidates=34 volumes=16 duplicated=16 settled=0
                      excluded=0 excluded-pages=-
```

Two of those were deliberately left unpredicted when the checks were written,
and both came out on the side of the stricter reading. The Bocchi pair the
earlier section called identity is containment. The Chainsaw Man identity
cluster is `{src-a, src-c}` — the two 198-image scans, as the page counts
suggested but nothing had confirmed.

`settled=0` is worth reading: no set here is settled by signature alone, so
every figure above cost a real alignment. That is consistent with what was
extracted — the hand run settled 3 of its 19 sets on signatures before anything
was transferred, and it is the 16 that needed real comparison which live in
`op-duplicates`.

**The sixth check runs the Bocchi set twice, and the two runs disagree.** The
block above is the first of them. The second hands `duplicates` the edition the
mislabelled file actually belongs to — `incoming/bocchi/en-gaiden-v01.cbz`,
which the library already held under the right name, and which the section
"Bocchi volume 1, where one of the three was not the same work" records the file
pairing with at 8 runs and 182 of 185 slots. Same three directories, same
reading direction, one flag different:

```
bocchi, no reference:               set: copies=3 signature=0 clusters=3 flagged=1 refused=2 fragmented=0 slots=125 decided=yes best=…/raw-v01-src-hamaji
bocchi, against en-gaiden-v01.cbz:  set: copies=3 signature=0 clusters=3 flagged=2 refused=2 fragmented=0 slots=185 decided=yes best=…/raw-v01-src-mangazip
```

Nothing about the copies changed between those two lines, and the pairs did not
move: `refused=2` is the same two refusals both ways, `mangazip` against each
real scan. What changed is the question the flagging is allowed to ask. With no
reference the set can only ask which copy disagrees with the others, and
`mangazip` is that copy — it refuses both real scans and they refuse it back, so
the majority settles it: `mangazip` is flagged and excluded, and the two
agreeing copies rank with `hamaji` first on 125 slots. With a reference the set stops being the
authority: every cluster representative is aligned against the reference, the
flagged ones included, and the only copy that pairs anything with
`en-gaiden-v01.cbz` is `mangazip`. `hamaji` and `krc` pair nothing with it — the
main-series raws refuse the Gaiden edition, which the table in that earlier
section already recorded — so they are what `flagged=2` now names, and
`mangazip` is the only copy left to rank, at 185 slots against the 125 that won
the first run.

Both answers are correct about the question they were asked, and that is the
whole of it. "Which of these is not this volume" has no answer inside a set that
has no idea which volume it is; it only has one relative to something outside
the set. The earlier section is about the mislabelled file being caught and
identified. This is the step after: the identification now decides the ranking
rather than merely explaining a refusal. Which line to act on depends on which
volume is being sorted — the first if it is main-series volume 1, the second if
it is the *Gaiden* — and that is a fact about the library, not about the three
files, which is exactly why it has to be supplied from outside the set.

### Two copies and a refusal between them flag both, and that is the designed answer

The One Piece Starter Book copies filed as volumes 2 and 3 come out
`flagged=2 refused=1 decided=no`, with nothing ranked at all:

```
set: copies=2 signature=0 clusters=2 flagged=2 refused=1 fragmented=0 slots=- decided=no best=-
```

Both copies are flagged because **nothing in the set says which one is the
impostor.** With three copies the majority settles it — Bocchi's Gaiden file
refuses against two scans that agree with each other, so it is the odd one out
and the other two rank normally. With two, a refusal is symmetric and the honest
report is that neither can be trusted. `--against` a reference edition is what
resolves it, and the output says so rather than picking one.

What `--against` does here is move the decision outside the set: every cluster
representative is aligned against the reference, the flagged ones included, and
whichever copy pairs with it is this volume however the set treated it. A copy
that pairs nothing with the reference is not, whatever it did within the set.
**On this exact set — a two-copy refusal with no reference to hand — that is
still behaviour rather than a measurement**, because the Starter Book copies
have no reference edition in the corpus. What is now measured is the rule
itself, on the Bocchi set above: handing it the *Gaiden* edition inverts which
copies come back flagged, so the reference overriding the set's own reading is a
gated figure rather than a claim.

### Ranking prefers whichever copy holds more, including what may not belong

The weakness in content-first ranking now has a real example rather than a
caveat. On v077 the command names **copy 0** best, on 236 slots against 234 —
and copy 0 is precisely the different-crop outlier that fragments against both
other copies, at 104 and 100 runs. Content-first is still the right rule, for
the reason the Chainsaw Man section gives: a missing page cannot be recovered
from a cleaner alignment of the pages that remain. But "more slots" and "more of
this volume" are not the same claim, and on this set they disagree.

Two things keep that from being a trap. The pairs table sits above the ranking
in the same output, so the 104 runs are visible next to the copy the rank names
first. And `--against` a reference edition adds pairings and run counts to every
ranked row, which is what turns the ordering from a proxy into a measurement —
the Chainsaw Man set is decided that way and v077 could be.

### `--pattern` matches the deepest directory, not the copy directory

`inventory` takes as one candidate the deepest directory that still holds a
whole edition, which is what keeps a volume whose chapters sit in subdirectories
from counting seven times. The consequence is that the name it groups on is that
directory's, and one candidate in this corpus is nested:

```
corpus/incoming/op-duplicates/v095-colour-c1/DLRAW.TO_ワンピース カラー版 第095巻
```

The default marker scan reads `第095巻` and groups it with `v095-colour-c0`
correctly. `--pattern 'v{}'` does not match that name at all, so the copy falls
into the unparsed group and volume 95 is left with one copy:

| Run | volumes | duplicated |
| --- | ------- | ---------- |
| `taiyaku inventory corpus/incoming/op-duplicates` | 16 | 16 |
| `taiyaku inventory corpus/incoming/op-duplicates --pattern 'v{}'` | 17 | 15 |

The unparsed group is printed whatever else is suppressed, which is why this is
visible rather than a silently missing set. Reach for `--pattern` when the
default parse is wrong, not by default.

### Rebuilding the survey rather than remembering it

`scripts/duplicate-survey.sh` collects one `set:` line per duplicate set over a
release directory, the way `scripts/margin-survey.sh` collects `stats:` lines:

```sh
scripts/duplicate-survey.sh corpus/incoming/op-duplicates
```

It runs `taiyaku inventory` over the same root first and **fails** if its own
shell grouping and inventory's parse disagree about how many duplicate sets
exist, rather than printing a table with the wrong number of rows.

## The margin survey, and where the refusal threshold sits

Run with `scripts/margin-survey.sh` against the whole corpus. Every figure is
`align --stats` output; the script is committed so the table can be rebuilt.

| Group | Pair | \|A\| | \|B\| | noise | signal | sep | margin | avg_run | per_100 | verdict |
| ----- | ---- | --- | --- | ----- | ------ | --- | ------ | ------- | ------- | ------- |
| good | gotoubun-v01 | 195 | 194 | 0.523 | 0.875 | 0.352 | 0.258 | 64.3 | 2.6 | Sound |
| good | gotoubun-v03 | 195 | 194 | 0.523 | 0.848 | 0.324 | 0.230 | 64.3 | 2.6 | Sound |
| good | gotoubun-v08 | 195 | 192 | 0.508 | 0.684 | 0.176 | 0.086 | 64.0 | 2.6 | Sound |
| good | kaguya-v01 | 213 | 214 | 0.516 | 0.730 | 0.215 | 0.109 | 52.2 | 3.8 | Sound |
| good | kaguya-v03 | 210 | 210 | 0.500 | 0.629 | 0.129 | 0.027 | 69.3 | 2.4 | Sound |
| good | kaguya-v08 | 211 | 210 | 0.516 | 0.777 | 0.262 | 0.148 | 69.3 | 2.4 | Sound |
| good | kobayashi-v01 | 146 | 146 | 0.500 | 0.602 | 0.102 | 0.012 | 48.0 | 3.4 | Sound |
| good | kobayashi-v03 | 146 | 146 | 0.500 | 0.602 | 0.102 | 0.012 | 36.0 | 3.4 | Sound |
| good | kobayashi-v08 | 162 | 162 | 0.504 | 0.840 | 0.336 | 0.246 | 39.2 | 4.9 | Sound |
| good | oshinoko-v01 | 223 | 229 | 0.500 | 0.625 | 0.125 | 0.016 | 44.4 | 2.7 | Sound |
| good | oshinoko-v03 | 197 | 195 | 0.500 | 0.637 | 0.137 | 0.027 | 65.0 | 2.5 | Sound |
| good | oshinoko-v08 | 202 | 195 | 0.504 | 0.746 | 0.242 | 0.148 | 64.3 | 2.5 | Sound |
| good | tongari-v01 | 213 | 212 | 0.512 | 0.645 | 0.133 | 0.031 | 52.5 | 2.8 | Sound |
| good | tongari-v09-letterbox | 180 | 177 | 0.504 | 0.656 | 0.152 | 0.047 | 44.0 | 3.9 | Sound |
| good | monster-2to1 | 444 | 438 | 0.504 | 0.609 | 0.105 | 0.008 | 47.4 | 2.9 | Sound |
| good | nausicaa-4to1 | 600 | 576 | 0.504 | 0.777 | 0.273 | 0.172 | 46.1 | 3.3 | Sound |
| good | op-v092-spread-repagination | 200 | 202 | 0.504 | 0.809 | 0.305 | 0.211 | 200.0 | 0.5 | Sound |
| good | bocchi-gaiden-real | 185 | 185 | 0.500 | 0.660 | 0.160 | 0.062 | 36.4 | 4.3 | Sound |
| bad | oshinoko-v01-badcopy | 223 | 229 | 0.504 | 0.609 | 0.105 | 0.012 | 4.8 | 27.4 | Fragmented |
| bad | op-v077-c0-vs-c1 | 236 | 234 | 0.500 | 0.629 | 0.129 | 0.023 | 3.5 | 44.1 | Fragmented |
| needle | colorwalk-to-op-v105 | 9 | 188 | 0.504 | 0.609 | 0.105 | 0.004 | 1.0 | 100.0 | Fragmented |
| subset | one-chapter-to-volume | 33 | 213 | 0.508 | 0.762 | 0.254 | 0.152 | 6.8 | 21.2 | Fragmented |
| unrel | gotoubun-raw-vs-kaguya-en | 195 | 214 | 0.516 | 0.613 | 0.098 | 0.008 | 0.0 | 0.5 | Refused |
| unrel | bocchi-gaiden-vs-main | 185 | 127 | 0.504 | 0.594 | 0.090 | 0.008 | 0.0 | 0.5 | Refused |

**One row that this survey first filed under `needle` disqualifies itself, and
that has to be settled before the rest of this argument rather than after it**
— it is `one-chapter-to-volume`, relabelled `subset` in the table and in the
script once the point below was worked out, so the table above already shows a
single `needle` row.
Kaguya volume 1 genuinely is chapters c1–c10 (recorded above, under "`x.y`
chapters are a prior, not a rule"), so the fan chapter aligned in
one-chapter-to-volume is a real subset of the raw volume rather than
unrelated material offered a lot of candidates. Its margin of 0.152 is
signal, not the noise this guard exists to catch — which is exactly why it
lands inside the good cluster's own range rather than below it. **The only
true needle in this survey is Color Walk, at 0.004**, an art book wholly
unrelated to the One Piece volume it is aligned against; the corpus does not
currently hold a second example of its kind.

**The lowest good margin is 0.008, on Monster's 2:1 repagination (raw
v01+v02 against the English Perfect Edition v01).** Measured against the
correct needle, both rules pass. Rule 1's first clause holds: 0.008 is
greater than 0.004. Its 0.75 clause holds too — 0.75 × 0.008 = 0.006, still
above 0.004. Rule 2 agrees: Monster's ratio is 0.008 / 0.105 = 0.076 against
Color Walk's 0.004 / 0.105 = 0.038, and 0.076 is greater than 0.038.

Neither passes for long. `similarity` is `1.0 - differing / 256.0`
(`solve.rs`), so every similarity, and every margin computed as a
difference of two similarities, is a whole number of 256ths. The median
compounds this rather than smoothing it — `percentile` returns
`values[at]`, a selected sample rather than an average, so the median
margin is a whole number of 256ths too. Color Walk's 0.004 is exactly 1 bit
(1/256 = 0.0039); Monster's 0.008 is exactly 2 bits (2/256 = 0.0078). The
rest of the table checks out the same way: 0.027 is 7/256, 0.047 is 12/256,
0.086 is 22/256, 0.109 is 28/256, 0.152 is 39/256, 0.258 is 66/256, and so
on for every figure in it — this is not a property of these two rows, it is
how every margin in the survey is built. Rule 1's own candidate threshold,
0.006, sits at roughly 1.5 bits: a value no median margin can ever actually
take, in a gap that holds exactly one quantisation step and nothing either
side of it.

**Rule 3 applies — not because the margins fail to separate arithmetically,
they do, barely, but because the entire distance between refusing Color
Walk and accepting a correct alignment that paired 427 of Monster's 444
pages is one bit of a 256-bit hash, and no safety factor fits inside a gap
that size.** Task 4 remains a documentation-only task, recording this
rather than compiling a constant.

Monster is not available to discount as an outlier either. It is one of
only two multi-volume repagination cases in the corpus, alongside Nausicaä,
and no less a genuine good pairing than any single-volume one. Set it aside
regardless and the next-lowest good margins — Kobayashi v01 and v03 at
0.012 (3 bits), Oshi no Ko v01 at 0.016 (4 bits) — sit two and three bits
clear of Color Walk, which would look like room to work with. That is
discounting real evidence because it is inconvenient: the corpus's own
weakest confirmed-good pairing does sit one bit from its own needle, and
one bit is not a distance a safety factor can be asked to cover.

**Kaguya was the canary and did survive**, at a margin of 0.027 against a
separation of 0.129 (v03, its weakest volume of the three surveyed) — the
weakest pairing in the corpus by every earlier measure, so it was expected to
bound the threshold from above. It turns out not to be the binding
constraint this time: Monster's 2:1 repagination posts a lower margin still,
0.008 against a separation of 0.105, indistinguishable from the two
unrelated pairs' own margin of 0.008. (Kaguya v01, historically the volume
called weakest by separation and MEDBIT, is not the weakest of the three
Kaguya volumes by margin either: it posts 0.109 against a separation of 0.215,
four times v03's 0.027, though below v08's 0.148 — a reminder that "weakest by
one measure" does not transfer automatically to another.)

**What the survey says about average run length.** Say plainly what the table
shows: **on this corpus both measures separate, once refusals are gated.**
Every good pairing's average run length sits between 36.0 (Kobayashi v03) and
200.0 (the One Piece v092 pair, one run covering all 200 of c0's slots — the
row is labelled `op-v092-spread-repagination` rather than `identity`, since c0
is *within* c1's 202 rather than identical to it, and the run count is
unaffected);
every bad, needle and subset pairing sits at 6.8 or below, and both unrelated
pairs at 0.0. No overlap: the gap runs from 6.8 to 36.0. But runs per hundred
pages separates too among the pairs that get that far — the good rows run 0.5
to 4.9, and every bad, needle and subset row runs 21.2 to 100.0, a gap wider
in ratio than the other measure's. The one place it fails is the two refused
pairs, which read as 0.5 runs per hundred, better than every good pairing's own
figure (2.4 to 4.9) bar the v092 pair's own 0.5 — the hole this document
has already recorded once, a total refusal reading as excellent fragmentation.
That failure is already neutralised: `verdict` checks coverage first and only
consults fragmentation once something was paired, so a refusal never reaches
the comparison. Average run length needs exactly the same gate, because it
reports 0.0 for a pairing that paired nothing and 0.0 is below any threshold.
Swapping the measure did not retire the coverage check and could not have.

So the earlier claim here — that average run length separates where runs per
hundred does not — was overstated, and the table is what corrects it. **The
defensible case for the swap is narrower.** First, average run length states
the property directly, in slots, and does not vary with the edition's page
count: "runs are two slots long" is a fact about the alignment, where "9.5 runs
per hundred pages" needs the page count supplied before it means anything.
Second, the two measures genuinely disagree on a shape no row of this corpus
happens to take: twenty paired runs of two slots each across 210 pages reads as
9.5 runs per hundred, comfortably under the old `FRAGMENTED_RUNS_PER_HUNDRED`
of 15 and therefore sound, while averaging two slots a run. Twenty runs of two
is confetti whatever the page count says. That case is pinned as a test in
`solve.rs` rather than argued from, because the corpus cannot supply it.

Task 5 put average run length in place of `FRAGMENTED_RUNS_PER_HUNDRED` rather
than merely alongside it. The threshold is `MIN_AVERAGE_RUN = 27.0`, three
quarters of the weakest confirmed-good figure (0.75 × 36.0 = 27.0), which
clears the highest non-good figure, 6.8, by nearly four times (27.0 / 6.8 =
3.97). Rerunning the full survey against the new threshold reclassified
nothing: every `good` row still reads `verdict=Sound`, and every `bad`,
`needle`, `subset` and `unrel` row reads `Fragmented` or `Refused`, exactly as
under the old threshold.

**The threshold is in absolute slots, so it has to be capped by the edition's
own length.** An average measured in slots cannot exceed the number of slots
there are, so a 20-page one-shot that aligns as one perfect run averages 20 and
an uncapped 27.0 calls it `Fragmented` by arithmetic alone. Nothing in this
survey licenses that: the shortest confirmed-good pairing measured here is
Kobayashi v01 at 146 slots, so below 146 the constant is simply unmeasured.
`verdict` therefore compares against `MIN_AVERAGE_RUN.min(pages)`. A pairing
that is one clean run has an average equal to its own length, so it stays
`Sound` at any size, and the cap can only lower the bar — no row can move from
`Sound` to `Fragmented` under it.

Checked against this table, the cap reclassifies nothing. Every good row is at
least 146 slots, so the cap does not bind on any of them and 27.0 still
applies. Color Walk is 9 slots averaging 1.0 against a cap of 9, still
`Fragmented`. The subset row is 33 slots, above the constant, so its cap is
27.0 and its 6.8 is still `Fragmented`.

Making the threshold **proportional** to the edition's length is the obvious
alternative and this table rejects it: Color Walk scores 1.0 / 9 = 0.11 where
Nausicaä's genuine 4:1 repagination scores 46.1 / 600 = 0.077, so the needle
would beat a real alignment. A cap keeps the measured absolute figure wherever
it has been measured and relaxes only below the range the corpus covers, which
is the weaker and therefore the honest claim.

**Honest limits.** The needle side of this table has 1 member against 18 on
the good side, so the threshold is anchored to the good cluster rather than
placed between the two. (The row that would have been the second needle is the
`subset`, disqualified above.) `|A|` is reported for every row because a
median over nine margins (Color Walk) is not the same evidence as a median
over two hundred. And nothing here measures an edition shorter than 146 slots,
which is why the length cap is a cap rather than a second measured constant.

**That limit has since been closed — see the next section, which replaces the
single-needle caveat with twenty-two of them and strengthens the conclusion
considerably.**

## Twenty-two needles, and margin is not merely tight but inverted

The survey above rested on one needle, and said so. The One Piece archive turns
out to hold hundreds of the things: of its 579 directories only 141 are volumes,
and most of the rest are weekly Jump issues, calendars and databooks at 13 to 29
pages each. Short material, and none of it has a counterpart in any tankōbon.
That is the needle condition exactly, occurring naturally rather than
constructed.

Twenty-two pairings measured, of three kinds. Calendars are pure artwork. Jump
issues carry chapters 1005–1021, so against volume 105 — chapters 1055 and after
— no page has a true counterpart, while the series, the artist, the era and the
screentone are all shared. That makes them the hardest needles available rather
than the easiest. Color Walk Compendium 2 is a second art book, sampled five
pages deep at 410 MB a page, to check that the original Color Walk result was
not a property of one book.

Seven of the twenty-two cleared calibration and aligned. Every one of those
seven paired **100% of its pages with nothing unmatched**, which is the
behaviour that opened this whole item.

| Needle | \|A\| | \|B\| | sep | margin | accepted | avg run |
| ------ | --- | --- | --- | ------ | -------- | ------- |
| Color Walk Compendium 2 → Go-Toubun v01 | 5 | 195 | **0.145** | **0.020** | yes | 1.7 |
| XCalendar 2015 → One Piece v105 | 26 | 210 | 0.105 | 0.008 | yes | 1.2 |
| Jump 2021-24 → One Piece v105 | 18 | 210 | 0.102 | 0.016 | yes | 1.1 |
| Jump 2021-19 → One Piece v105 | 18 | 210 | 0.102 | 0.012 | yes | 1.1 |
| Jump 2021-21/22 → One Piece v105 | 23 | 210 | 0.102 | 0.012 | yes | 1.1 |
| Jump 2021-13 → One Piece v105 | 17 | 210 | 0.102 | 0.008 | yes | 1.1 |
| Jump 2021-31 → One Piece v105 | 17 | 210 | 0.102 | 0.008 | yes | 1.1 |
| fifteen others | 5–29 | 195–210 | 0.086–0.098 | 0.004–0.016 | no | — |

### Margin does not merely fail to separate, it points the wrong way

The survey concluded that no margin threshold fits because the needle sat one
bit below the weakest confirmed-good pairing. The truth is worse than that.

**A One Piece art book aligned against Go-Toubun no Hanayome — different series,
different artist, different publisher, different decade — posts a margin of
0.020.** Four confirmed-good pairings score lower: Monster's 2:1 repagination at
0.008, Kobayashi v01 and v03 at 0.012, and [Oshi no Ko] v01 at 0.016. So the
worst possible pairing in this corpus outscores four correct ones on the
statistic that was supposed to reject it.

Margin is therefore not a weak signal at this scale. It is not a signal at all —
the whole population, good and needle alike, sits within five bits of a 256-bit
hash, and the ordering inside that range carries no information about whether
the two editions correspond.

### Separation is not safe either, which was not previously known

The same pairing posts a *separation* of 0.145, and **seven confirmed-good
pairings sit below that**: both Kobayashi volumes at 0.102, Monster at 0.105,
[Oshi no Ko] v01 at 0.125, Kaguya v03 at 0.129, Tongari v01 at 0.133 and
[Oshi no Ko] v03 at 0.137. `MIN_SEPARATION` cannot be raised to catch these
needles without refusing genuine volumes, and this is the first measurement that
shows it. The constant has been carried as "provisional and untuned" since it
was written; it is now known to be untunable in that direction.

The Jump issues also show how fine the existing edge is. Their separations are
0.086, 0.090, 0.094, 0.098, 0.102 — steps of exactly 1/256, straddling
`MIN_SEPARATION` of 0.10. Whether a given issue is refused or aligned is decided
by one bit.

### What this leaves standing

Only run structure. Every needle that got through averages **1.1 to 1.7 slots a
run**, against 36.0 to 200.0 for every confirmed-good pairing in the survey — a
gap of more than twenty times, where both calibration statistics overlap
outright. `verdict` calls all seven `Fragmented`, and since both `align` paths
now print the verdict, all seven say so on screen.

Two things follow. The decision not to ship a margin guard was correct, and by a
wider margin than the evidence available at the time showed. And a reader must
not treat "the solver accepted this pair" as meaning the editions correspond —
acceptance is a statement about calibration, and only the verdict is a statement
about the alignment.

### Where the material is

`corpus/incoming/needles/` holds four of the twenty-two, chosen as the extremes:
`colorwalk-comp2` (highest margin and highest separation of any needle),
`op-xcalendar-2015` (identical to Monster on both calibration statistics),
and `op-jump-2021-24` and `-31`. The rest are reproducible from the NAS —
calendars and Jump issues under `needs_processing/_op/DLRAW.TO_One_Piece_ vol
01-109/`, and Color Walk Compendium 2 under `needs_processing/manga/`, which is
316 pages at 410 MB each and should be sampled rather than copied.

## Provenance: where the measured material came from

Everything measured above was extracted from archives on the server. The extracted
copies are working data and deliberately disposable — nothing in this repository
refers to them by path — but the measurements are only reproducible if the
provenance is written down, so here it is.

Three locations, all on the NAS:

| Path | Holds |
| ---- | ----- |
| `media 1/needs_processing/manga/` | raw archives as downloaded |
| `media 1/Manga/` | English editions, already organised |
| `media 1/Manga (RAW)/` | Japanese editions, already organised |

### Which archive each finding came from

| Finding | Archive, under `needs_processing/manga/` | Members |
| ------- | ---------------------------------------- | ------- |
| 2:1 repagination | `13DL.ME_Monster vol 01-18.rar` | `…/13DL.ME_Monster v01-06/…第01巻`, `…第02巻` |
| 4:1 repagination | `(一般コミック) [宮崎駿] 風の谷のナウシカ 全07巻/…第01巻.zip` … `第04巻.zip` | whole zips |
| Base selection | `DLRAW.AC_Chensoman vol 01-22.rar` | groupings `v01`, `v01b`, `v01-02`, `color v01-11` |
| Mislabelled *Gaiden* | `Manga-Zip.info_Bocchi the Rock v01.rar` | whole archive |
| Bocchi real scans | `DLRAW.APP_Bocchi The Rock  vol 01-08.rar` | groupings `v01-02b`, `v01-04` |
| Duplicate resolution | `DLRAW.TO_One_Piece_ vol 01-109.rar` | extracted whole to `needs_processing/_op` |
| Art books | `Color Walk 10/`, `Color Walk Compendium 2/` | loose TIFFs, not archived |

English counterparts are in `Manga/` under the obvious names, except two worth
noting because the name does not match the raw: the Monster raws pair against
**`Monster - The Perfect Edition`** (9 volumes, not the 18-volume `Monster`), and
the mislabelled Bocchi file pairs against **`Bocchi the Rock! - Gaiden - Hiroi
Kikuri no Fukazake Nikki`**.

### Extracting selectively, and the one thing that goes wrong

`bsdtar` reads all of these, including the RAR5 archives that 7-Zip silently turns
into zero-byte files. Extract single volumes rather than whole archives — the One
Piece archive alone is 61 GB:

```sh
bsdtar -xf "$ARCHIVE" "<root>/<grouping>/<volume>/*"
```

**The pattern must include the archive's own root directory.** Every one of these
archives wraps its contents in a directory named after the archive, so a pattern
starting at the grouping silently matches nothing and exits successfully. Two of
these archives also have a **double space** in that root name — `Bocchi The Rock  vol
01-08` — which is easy to lose when retyping a path.

List the roots before extracting rather than guessing:

```sh
bsdtar -tf "$ARCHIVE" | awk -F/ 'NF>=3 {print $1"|"$2}' | sort -u
```

### Reproducing the headline numbers

```sh
# 2:1 repagination — 13 runs, 427 of 444 slots
taiyaku align monster/raw-v01 monster/raw-v02 -- monster/en-pe-v01.cbz --split-spreads

# 4:1 repagination — 20 runs, 553 of 600 slots
taiyaku align nausicaa/raw-v0{1,2,3,4} -- nausicaa/en-deluxe-v01.cbz --split-spreads

# the mislabelled file against its real counterpart — 8 runs, 182 of 185
taiyaku align bocchi/raw-v01-src-mangazip -- bocchi/en-gaiden-v01.cbz --split-spreads

# one pagination inside another — 1 run, 200 of c0's 200 slots. c0 is *within*
# c1's 202, not identical to it; the unmatched count align prints is c0's alone.
taiyaku align opdup/v092-mono-c0 -- opdup/v092-mono-c1 --split-spreads

# and the same set as a duplicate decision, which reports both sides
taiyaku duplicates opdup/v092-mono-c0 -- opdup/v092-mono-c1
```

The duplicate survey is two steps: build an inventory of every volume directory with
its file count, then take a signature of file count, total bytes and the hash of the
sorted file-size list to settle exact duplicates before transferring anything. Only
what disagrees needs aligning, which on One Piece was 4 GB rather than 12.

One caution learned the expensive way: **compare all pairs within a duplicate set**,
not every copy against the first. Anchoring on the first copy reported volume 77 as
three unrelated files when two of them are an exact identity.

### `kobayashi-v01/en-fan`, ingested 2026-08-13

Ten members, copied with `scripts/ingest-fan-edition.sh` into
`corpus/kobayashi-v01/en-fan` from `/mnt/daveynet/nfs/media 1/Manga (Chapters)/Kobayashi-san cno Meidoragon - Fan Translated`,
filter `' - v1 c'`:

| sha256 | Member |
| ------ | ------ |
| `6dc84b196cddbcf448e0ee9dacc6fcb93af9f9bb21410dceef979ae3e5781cca` | `Kobayashi-san Chi no Meidoragon - v1 c10.cbz` |
| `e0b19c9776e712f723fbe1769d0c910bbe84e4fa32807ab839665a5c0545b8bf` | `Kobayashi-san Chi no Meidoragon - v1 c1.cbz` |
| `a8a7b5638b833ccce7e143543489d9ca9547280f617f344a4ccee98adedefcd0` | `Kobayashi-san Chi no Meidoragon - v1 c2.cbz` |
| `eb84da520cdaf73aea8ba1b400377f5abc9fd7482422252c8c3c2fb6ac9c1be5` | `Kobayashi-san Chi no Meidoragon - v1 c3.cbz` |
| `9e405bd0f2d13402781461c26c32dc6080d6be14e37acd0b98565f2b70367978` | `Kobayashi-san Chi no Meidoragon - v1 c4.cbz` |
| `c975a7cfafea6d040ee5fac43807a3890654fe1e0eae8d1ab8ce59f2577a3ac5` | `Kobayashi-san Chi no Meidoragon - v1 c5.cbz` |
| `2092a14515db0b5c233674958898d1d6fc001c67eb37d1e380a63fbc39d7c188` | `Kobayashi-san Chi no Meidoragon - v1 c6.cbz` |
| `f98d33b422976445e944554c3aac0dad0f15ccbffee58b30b45ee14199296b85` | `Kobayashi-san Chi no Meidoragon - v1 c7.cbz` |
| `5f7f1b0f22d2ccf49f16331e136188a41540be544e0566a16d93f3a979aac8fa` | `Kobayashi-san Chi no Meidoragon - v1 c8.cbz` |
| `4247a4a48798b75372572d926f387dc689dbc4357f0341cef907c69886a6a7c8` | `Kobayashi-san Chi no Meidoragon - v1 c9.cbz` |

### `kobayashi-v03/en-fan`, ingested 2026-08-13

Ten members, copied with `scripts/ingest-fan-edition.sh` into
`corpus/kobayashi-v03/en-fan` from `/mnt/daveynet/nfs/media 1/Manga (Chapters)/Kobayashi-san cno Meidoragon - Fan Translated`,
filter `' - v3 c'`:

| sha256 | Member |
| ------ | ------ |
| `5193a712d832b691987f6fd4878d670f12248bac5265f96c28af9972bfae8047` | `Kobayashi-san Chi no Meidoragon - v3 c21.cbz` |
| `69083d610f96daab41e68b766cc97659ee6e038e001599245331ef30755fc596` | `Kobayashi-san Chi no Meidoragon - v3 c22.cbz` |
| `7cba0093b3e7673b8722f67ff1b044996c970510ffef8d107e1ff0c97be5b0e3` | `Kobayashi-san Chi no Meidoragon - v3 c23.cbz` |
| `768f2330a78017153ddbddc530858b8fcdc99dd1b5c2f6c9c22a00f1c7558ee4` | `Kobayashi-san Chi no Meidoragon - v3 c24.cbz` |
| `b85132e98559b8df1eb8cce2cef02515bb970498223293bfd16e40a60af7285e` | `Kobayashi-san Chi no Meidoragon - v3 c25.cbz` |
| `25f2ce0a32cdd4af9ae3cd4543176b7fad3b4177af5c9a91d26b202b705d492b` | `Kobayashi-san Chi no Meidoragon - v3 c26.cbz` |
| `43334b97af064da79bcfaafa2b1cb8acc36a8340566f236850a737cca5ad37a5` | `Kobayashi-san Chi no Meidoragon - v3 c27.cbz` |
| `f54e15e1993cd3d3b3d6f86889df8d96eea3e66e3e5ca29f3ba12d821a6a4ad2` | `Kobayashi-san Chi no Meidoragon - v3 c28.cbz` |
| `be09e25bcc1f201d317841f3ec00a4b8e8b35d2363bea366811f704c430a92f6` | `Kobayashi-san Chi no Meidoragon - v3 c29.cbz` |
| `2da874103200cd8cb8130ee9eef27802d91b0d5aaf93830f23df9a6a2c3f0e1c` | `Kobayashi-san Chi no Meidoragon - v3 c30.cbz` |

### `kobayashi-v08/en-fan`, ingested 2026-08-13

Ten members, copied with `scripts/ingest-fan-edition.sh` into
`corpus/kobayashi-v08/en-fan` from `/mnt/daveynet/nfs/media 1/Manga (Chapters)/Kobayashi-san cno Meidoragon - Fan Translated`,
filter `' - v8 c'`:

| sha256 | Member |
| ------ | ------ |
| `f11e87842d9dfd5f36440f5ba52be449230c72612bbc9baa088173d4ab205395` | `Kobayashi-san Chi no Meidoragon - v8 c69.cbz` |
| `fd384c9c7ba9db14451fd74ba0979ade993b2ec518179b132b46d37021a8e6d2` | `Kobayashi-san Chi no Meidoragon - v8 c70.cbz` |
| `f2099e22b25850e7e82bf4ec0e0f6b869eae0d831f6526716d6390f19f55d1a6` | `Kobayashi-san Chi no Meidoragon - v8 c71.cbz` |
| `309f0e92795662aaf291e824e8151509c8c86fc59252654a1859c0dd6903d3a4` | `Kobayashi-san Chi no Meidoragon - v8 c72.cbz` |
| `c06c8d568951563142d1c611c0dc1db38885c3d1fb13274e97070b023b5de027` | `Kobayashi-san Chi no Meidoragon - v8 c73.cbz` |
| `7a1445748ccbc62a066c2c72d8a6b13d71e5ef138c3b601af08ccf8ca262101f` | `Kobayashi-san Chi no Meidoragon - v8 c74.cbz` |
| `3c322023e10bbeb30df9b725a895e0f2f48388ec9b7ffa3a600f8ad680462b16` | `Kobayashi-san Chi no Meidoragon - v8 c75.cbz` |
| `dbe952f6ce79a65af0e81fd8821d301ec71216e439cf5c84573e3091cc6a3de1` | `Kobayashi-san Chi no Meidoragon - v8 c76.cbz` |
| `78c35a2bb61b5ad0985e007ad65726fed52f0278fcad785af15f05b71670ba9a` | `Kobayashi-san Chi no Meidoragon - v8 c77.cbz` |
| `811c3fd5d79e9174e02cc5f41e0ad33bb5e280977a4b2c0eb2cfe18f31798890` | `Kobayashi-san Chi no Meidoragon - v8 c78.cbz` |

### `kaguya-v03/en-fan`, ingested 2026-08-13

Twelve members, copied with `scripts/ingest-fan-edition.sh` into
`corpus/kaguya-v03/en-fan` from `/mnt/daveynet/nfs/media 1/Manga (Chapters)/Kaguya-sama wa Kokurasetai - Tensai-tachi no Ren'ai Zunosen - Fan Translated`,
filter `' - v3 c'`:

| sha256 | Member |
| ------ | ------ |
| `e139234e69ade3abfc80fb3726a6172b2fc65a20f32ef6b3d13c212b037bc140` | `Kaguya-sama wa Kokurasetai - Tensai-tachi no Ren'ai Zunosen - v3 c21.cbz` |
| `883e835967c7f2979f8a7099a08f7c93b78ed03d5e2d8475ab001fdb97ac4fa0` | `Kaguya-sama wa Kokurasetai - Tensai-tachi no Ren'ai Zunosen - v3 c22.cbz` |
| `71dd0122b77a78fad420f28717c13d9e1be91d8400bcaf59d06d90019445b01e` | `Kaguya-sama wa Kokurasetai - Tensai-tachi no Ren'ai Zunosen - v3 c23.cbz` |
| `af0a10e85f64703e5f01c20c8aecba6ee7974bba4e7e8dfa64652d542160907d` | `Kaguya-sama wa Kokurasetai - Tensai-tachi no Ren'ai Zunosen - v3 c24.cbz` |
| `d90bd672370544370074248872458a2c78e9d29331e38a1383adae4f1c2a8d9a` | `Kaguya-sama wa Kokurasetai - Tensai-tachi no Ren'ai Zunosen - v3 c25.cbz` |
| `852ecca2db9d28cf0f8826bfd84491bd697a501fe8f8207df10c0e8d33e23011` | `Kaguya-sama wa Kokurasetai - Tensai-tachi no Ren'ai Zunosen - v3 c26.cbz` |
| `2959129bcc1e3bb0dacc77f996324e1163133350d92c1a3962de841cca1cc09b` | `Kaguya-sama wa Kokurasetai - Tensai-tachi no Ren'ai Zunosen - v3 c27.5.cbz` |
| `5fcccded2c4a2a02c31a6934feea05388c846f9e36b3e9d6076439c69e5b1d40` | `Kaguya-sama wa Kokurasetai - Tensai-tachi no Ren'ai Zunosen - v3 c27.cbz` |
| `03f7ff00916e03d650f2db9d5e8a05322904edebfa4857792b587855aa7b6957` | `Kaguya-sama wa Kokurasetai - Tensai-tachi no Ren'ai Zunosen - v3 c28.cbz` |
| `69f886c2794e267861d0301a86349f331083cf47ae86ee60a74f17165d0f5328` | `Kaguya-sama wa Kokurasetai - Tensai-tachi no Ren'ai Zunosen - v3 c29.cbz` |
| `19d7ad6bd6d6c899089c895bc30a91877195ac981719156e6744509f460774bc` | `Kaguya-sama wa Kokurasetai - Tensai-tachi no Ren'ai Zunosen - v3 c30.5.cbz` |
| `2a62c3777a05823c43a304b8160ccaec6bfbc2ab3fab7fe70a08d60f8811ac89` | `Kaguya-sama wa Kokurasetai - Tensai-tachi no Ren'ai Zunosen - v3 c30.cbz` |

### `kaguya-v08/en-fan`, ingested 2026-08-13

Eleven members, copied with `scripts/ingest-fan-edition.sh` into
`corpus/kaguya-v08/en-fan` from `/mnt/daveynet/nfs/media 1/Manga (Chapters)/Kaguya-sama wa Kokurasetai - Tensai-tachi no Ren'ai Zunosen - Fan Translated`,
filter `' - v8 c'`:

| sha256 | Member |
| ------ | ------ |
| `76477194dc606897be36109b8781bdc359efbc70586264c09f7f267b3e586988` | `Kaguya-sama wa Kokurasetai - Tensai-tachi no Ren'ai Zunosen - v8 c71.cbz` |
| `7da4e53ccbab743c2450e072a22c7523b249d885d461014ed12a98ebd291eb57` | `Kaguya-sama wa Kokurasetai - Tensai-tachi no Ren'ai Zunosen - v8 c72.cbz` |
| `5ba96e07037ac0d9e9cf0d1801f5840097372a485fef59f72c1a04202e8087da` | `Kaguya-sama wa Kokurasetai - Tensai-tachi no Ren'ai Zunosen - v8 c73.cbz` |
| `db495317f355793860f09e34f5a41bb46035ead53b3f675364ca8ad4ed9c9e27` | `Kaguya-sama wa Kokurasetai - Tensai-tachi no Ren'ai Zunosen - v8 c74.cbz` |
| `03899b7b6dab5d435b639beda55188c0a9415d51f08708d9ba3920ee9c93a52b` | `Kaguya-sama wa Kokurasetai - Tensai-tachi no Ren'ai Zunosen - v8 c75.cbz` |
| `0b37fd6cce291c99f40ad1775588d598dd9f4cc7f30b9e9adcc458300e663deb` | `Kaguya-sama wa Kokurasetai - Tensai-tachi no Ren'ai Zunosen - v8 c76.cbz` |
| `87632dd9153bf3e1f7327b1d4a3d2322cd07c088b35edae8ceeac7e3bb7958a1` | `Kaguya-sama wa Kokurasetai - Tensai-tachi no Ren'ai Zunosen - v8 c77.cbz` |
| `a1e930a74552b93db1ed06589d6ebb8dfaba3f0b63b3136ece238080dff79d4e` | `Kaguya-sama wa Kokurasetai - Tensai-tachi no Ren'ai Zunosen - v8 c78.cbz` |
| `1677f123a1c618bd9b006e3bc8419dc01414617cdc67be89fa259c632c8aade6` | `Kaguya-sama wa Kokurasetai - Tensai-tachi no Ren'ai Zunosen - v8 c79.cbz` |
| `9b7033df529df82053213af2523173c9acb58e7f68b2b7324166a6423d8dc109` | `Kaguya-sama wa Kokurasetai - Tensai-tachi no Ren'ai Zunosen - v8 c80.5.cbz` |
| `18948b0841e8973cbda10670c032ada92c961bd1ca543ae34043b8ae122e80f8` | `Kaguya-sama wa Kokurasetai - Tensai-tachi no Ren'ai Zunosen - v8 c80.cbz` |

### `oshinoko-v01/en-fan`, ingested 2026-08-13

One member, copied with `scripts/ingest-fan-edition.sh` into
`corpus/oshinoko-v01/en-fan` from `/mnt/daveynet/nfs/media 1/Manga/[Oshi no Ko] - Fan Translated`,
filter `' - v01'`:

| sha256 | Member |
| ------ | ------ |
| `21187541783b0cebe40e550ff359bd117f682615c564db8ad9ccf962788c6ff4` | `[Oshi no Ko] - v01.cbz` |

### `oshinoko-v03/en-fan`, ingested 2026-08-13

One member, copied with `scripts/ingest-fan-edition.sh` into
`corpus/oshinoko-v03/en-fan` from `/mnt/daveynet/nfs/media 1/Manga/[Oshi no Ko] - Fan Translated`,
filter `' - v03'`:

| sha256 | Member |
| ------ | ------ |
| `91322444a6dc02d53e957ce239b2a9cf3d3957a9b806f1c09f4dc3bd10f0d63e` | `[Oshi no Ko] - v03.cbz` |

### `oshinoko-v08/en-fan`, ingested 2026-08-13

One member, copied with `scripts/ingest-fan-edition.sh` into
`corpus/oshinoko-v08/en-fan` from `/mnt/daveynet/nfs/media 1/Manga/[Oshi no Ko] - Fan Translated`,
filter `' - v08'`:

| sha256 | Member |
| ------ | ------ |
| `8d79504339924ee2dfcd5b7a0f7bead640fc1d72f3756e899b09f1d7311bfb42` | `[Oshi no Ko] - v08.cbz` |

## The extension lies about a page's format too, and the server was believing it

`taiyaku serve` has to state a content type for every page it hands out. The
obvious source is the entry's extension, which is also what decides whether an
entry counts as a page at all (`sidecar.md` rule 3). Measured across every page
entry in every corpus archive, that would be wrong on more than one page in a
hundred:

```
8637 page entries across corpus archives
  extension says jpeg  bytes say png      : 114 entries
```

All 114 are in one fan-translated chapter of Go-Toubun v01 —
`en-fan/… - c005.5.cbz`, whose entries are named `002.jpg` and hold PNG data.
Nothing else in the corpus disagrees with its own extension.

This is the same lesson `unsupported_container` already records one level up, at
the container rather than the page: extensions are a naming convention and the
bytes are the fact. A browser `<img>` re-sniffs and would never notice, which is
exactly why it would have gone unnoticed until the OPDS feed handed an iOS reader
a PNG labelled `image/jpeg`. `LocalLibrary` therefore sniffs the leading bytes
and falls back to the extension only when they name nothing.

Reproduce with:

```sh
python3 - <<'PY'
import zipfile
from pathlib import Path
IMG = {"jpg","jpeg","png","webp","gif","bmp","avif","tif","tiff"}
def magic(b):
    if b[:3] == b'\xff\xd8\xff': return 'jpeg'
    if b[:8] == b'\x89PNG\r\n\x1a\n': return 'png'
    if b[:6] in (b'GIF87a', b'GIF89a'): return 'gif'
    if b[:4] == b'RIFF' and b[8:12] == b'WEBP': return 'webp'
    if b[:2] == b'BM': return 'bmp'
    if b[:4] in (b'II*\x00', b'MM\x00*'): return 'tiff'
    if b[4:8] == b'ftyp': return b[8:12].decode('ascii','replace')
    return 'unknown'
norm = {'jpg':'jpeg','tif':'tiff'}
total, bad = 0, {}
for cbz in Path('corpus').rglob('*.cbz'):
    try: z = zipfile.ZipFile(cbz)
    except Exception: continue
    for info in z.infolist():
        name = info.filename
        if info.is_dir() or name.startswith('__MACOSX'): continue
        if name.rsplit('/',1)[-1].startswith('.'): continue
        ext = name.rsplit('.',1)[-1].lower() if '.' in name else ''
        if ext not in IMG: continue
        total += 1
        with z.open(info) as f: head = f.read(16)
        claimed, actual = norm.get(ext, ext), magic(head)
        if claimed != actual: bad.setdefault((claimed, actual), 0)
        if claimed != actual: bad[(claimed, actual)] += 1
print(total, 'page entries;', bad)
PY
```

## Scanlation credit pages are already visible in the alignment

Measured 2026-07-31 on `corpus/gotoubun-v01`, while planning M3. The question was
whether a reader could skip a scanlation group's donation and credits pages
without anyone hand-tagging them. **The alignment already knows where they are.**

**Two of the five rows below were mislabelled, and the labels were corrected on
2026-08-05 by extracting every page and looking at it.** The corrected column is
what the table now carries; what each row used to say is in the "Corrected"
section beneath it, because a table quietly edited teaches nobody what the
mistake was. The arithmetic was never wrong — the same five groups and the same
fifty slots reproduce exactly — only the reading of what the pages *are*.

A credits page exists in the fan edition and in no other, so it pairs with
nothing. Reading the `raw-mono → en-fan` alignment and asking which of the fan
edition's 240 slots no run ever mentions gives 50 slots in five groups. Page
indices are zero-based:

| Fan pages | Count | Member | Local | What it is |
| --------- | ----- | ------ | ----- | ---------- |
| 1–42 | 42 | `c000` | 1–42 | the prototype one-shot; the raw volume has no such chapter |
| 95 | 1 | `c001` | 51 of 52 | **donation plea** |
| 168 | 1 | `c003` | 31 of 32 | **donation plea** |
| 198 | 1 | `c004` | 29 of 30 | **donation plea** |
| 225 | 1 | `c005` | 26 of 27 | **staff credits** — TL / TLC / PR / ED names |
| 226–229 | 4 | `c005.5` | 0–3 | the volume's own front matter — cover, title page, contents, divider |

**Four of the six groups are a single unpaired page sitting at its member's
final index, and all four are credit pages.** Three are the same
`WE NEED DONATIONS! … lhtranslation.com` card, verbatim. The fourth is a staff
credits page — a meme panel captioned `TL: 5TBGamma / TLC/PR: ThunderCloud /
ED: monka`. The 42-page group is genuine content the raw volume does not contain
— `c000` page 20 was extracted as a negative control and is ordinary manga.

**So the separating rule is shape, not content:** an isolated unpaired page at a
member's *final* index is a credits page; an unpaired run spanning most of a
member is material the other edition lacks. Both fall out of `alignments`, which
every sidecar already carries.

### Corrected 2026-08-05: what the five rows actually hold

Re-measured while planning the credit-page feature, by running the complement
arithmetic against the shipped sidecar and then extracting and viewing twelve
pages. Four corrections, three of them to this section's own claims.

**1. `c005` local 26 is a credits page, not a "chapter end".** The row used to
read "chapter end plus a bonus chapter" for a five-slot group spanning two
members. Split at the member boundary it is two groups, and the first is a staff
credits page naming three people. **It is a true positive that this section's own
rule catches and this section's own table threw away.**

**2. `c005.5` locals 0–3 are the volume's front matter, not scanlator matter.**
They were described elsewhere in this file as "scanlator title and credit pages
that do not correspond to anything". Extracted, they are the tankōbon cover, the
volume title page (`Gotoubun no hanayome / Negi Haruba / 1`), the table of
contents listing chapters 1–5 with page numbers, and a decorative half-title.
They are unpaired for a mechanical reason rather than a semantic one: `c000`
also carries a copy of the cover, the 1:1 rule gave raw's front matter to that
copy first, and this one had nothing left to pair with. **A reader wants these
pages**, so a rule that swept them up would be worse than no rule.

**3. The edition has 240 slots, not 235.** 235 is its page count. `en-fan`
carries five joined spreads — pages 45, 91, 133, 164 and 196, recorded in the
edition-level `spreads` list — so expansion adds five slots. The 50-slot figure
survives unchanged because no spread falls inside an unpaired region, which is
luck rather than design and is exactly the kind of agreement that hides a
page-for-slot substitution.

**4. Signal 1's weakness is recall, not precision — and the corpus shows it.**
`c000` local 43, the edition's page 43, is a credits page for a **different
group**: a pink disc reading `PRESENTED BY: underdogscans + BOSEN SCAN` over
`rp / tl / prf / edt / qc` names, where the three pleas are `lhtranslation`. So
one fan edition mixes releases from at least two groups. The solver **paired**
that page, so it is not in the unpaired set at all and no unpaired-based signal
can reach it. Precision on this volume is four of four; recall is at best four of
five, and only twelve of 235 pages have been looked at, so the true denominator
is unknown.

### The dhash signal finds duplicated pages, which is not the same thing

The three cards are visually identical and byte-different — 175,385, 174,117 and
170,889 bytes, three different SHA-256s, because each chapter re-encoded it.
Exact-hash deduplication finds none of them. **dhash finds them, but not the way
this section originally implied, and it is the weaker signal of the two.**

Measured 2026-08-05 over all 235 pages, using the dhash-16 values already in the
CLI's fingerprint cache — the hashes taiyaku itself computed, not a
reimplementation. For each page, the Hamming distance to the nearest page in a
*different member* of the same edition:

| Distance | Pages | What they are |
| -------- | ----: | ------------- |
| 2 | 2 | the volume **cover**, carried by both `c000` and `c005.5` |
| 14 | 2 | two of the three donation pleas |
| 24 | 1 | the third donation plea |
| 33–64 | 2 | ordinary pages |
| >64 | 228 | ordinary pages, and **both staff-credit pages** — 93 and 99 |

The two staff-credit pages sit at 93 (`c000` local 43, `underdogscans + BOSEN
SCAN`) and 99 (`c005` local 26, `TL: 5TBGamma …`), measured 2026-08-06. This
paragraph read "99 and 100" until then, which was wrong twice over: 100 is
`c005.5` local 3, the decorative half-title, which is front matter and not a
credits page at all, and page 43's distance had never been measured. The
correction does not change the conclusion — both are far above anything that
could be called recurrence — but a figure attached to the wrong page is how the
mislabelling this whole section corrects got in.

**Its two tightest matches in the whole edition are a false positive.** The
closest pair by a wide margin is the volume cover appearing in two chapter
files, which is a page a reader wants. The signal is therefore a *duplicate*
detector, and duplicated front matter is as common as duplicated credits.

**It misses both staff-credit pages**, at distances 93 and 99, because each
appears exactly once. So it cannot serve as a confirmer either: gating signal 1
on it would veto the two true positives signal 1 uniquely finds.

**The pleas sit at 14–24 rather than near zero, and that is worth its own
sentence.** They are near-black cards carrying small text, and dhash reads
gradients, so a low-variance image has few strong gradients and re-encoding
noise flips a lot of the weak ones. This is the mirror of "Blank pages match each
other perfectly and lie about it": a blank page's hash is stable and meaningless,
and a nearly-blank page's hash is unstable. Both failures come from the same
property, and neither is visible to a check that compares dhash to dhash.

**Incomplete, marked 2026-08-13 rather than rewritten: a second cause was
never considered, and it acts on every cross-member comparison this section
makes.** The paragraph above is not withdrawn — the three pleas do carry three
different `sha256` and three different byte lengths (175,385 / 174,117 /
170,889), so the re-encoding it names is real and measured. What it misses is
that `fingerprint.rs`'s `describe_pages` crops every page by
`common_border(pages)` before hashing it, and the fingerprint cache is keyed
per member, so **each chapter file derives its own border from its own pages
and a page is hashed as a function of itself and of the archive it arrived
in.** Every distance in this section is a comparison across members, which is
precisely the axis that border varies along. See O41 in `status.md`.

The isolating case came later and from another edition:
`corpus/kobayashi-v01/en-fan` carries one credits card at the end of six
chapters, **byte-identical in all six** — one entry `sha256`, 398,900 bytes,
1028x663 — and it is recorded at four different sizes and reported at
distances 0, 0, 0, 0, 2 and 3. There is one encoding there, so cropping is the
whole of the effect.

Which cause dominates *here* is **unmeasured**, and this note does not guess.
The pleas are 600x600 cards sitting in members whose story pages run about
1465x2162, so the border applied to them is derived from something a different
shape entirely, which is a reason to suspect cropping is the larger term and
not evidence that it is. What can be said without measuring further is that
the error is signed: a crop that varies pushes two copies apart and never
together, so **three of five is a floor on signal 2's precision rather than an
estimate of it**.

**What is still not measured:** the other 223 pages of this edition, the whole of
`corpus/kaguya-v01/en-fan`'s 12 chapter files from a third group, groups that put
credits at a member's *first* index rather than its last, and the false-positive
rate against colour inserts and author afterwords. The Go-Toubun fan edition is
two groups' habits, not the population.

**Two signals, then, and they are not equals.** Signal 1 — unpaired against the
hub, isolated at a member's final index — is four for four here. Signal 2 —
near-duplicate of a page in a sibling member, by dhash — is three for five, and
needs no alignment, so it is the only thing that works on an edition with no
counterpart. Neither is a subset of the other and neither confirms the other;
what signal 2 is really good for is saying *why* a page recurs, which a person
reviewing candidates wants to know. *Signal 2's three for five is a floor
rather than an estimate since 2026-08-13 — see the marked paragraph above and
O41 — because the crop it hashes through varies per member and can only push
two copies of a page apart.*

## A second group's habits, and what the bound is worth

Measured 2026-08-07, to fix `MAX_CREDIT_RUN` in
`crates/taiyaku-cli/src/credits.rs` from something wider than one volume. The
section above ends by listing what was still unmeasured, and the first item on
that list was `corpus/kaguya-v01/en-fan` — twelve chapter files from a group
that had never been looked at. Rebuild with:

```sh
scripts/credit-survey.sh corpus
```

**Two findings, and the second is the one that matters more than the bound.**
The bound stays at 1, for want of any evidence that would raise it. And Kaguya
contributes no true positive at all — its one candidate is a **false positive**,
which takes signal 1's precision across the corpus from four of four down to
four of five.

### The alignment Kaguya was surveyed through is FRAGMENTED

`corpus/kaguya-v01` had no sidecar, so one was written first: `raw-mono` (213
loose JPEGs) against `en-fan`'s twelve archives. The verdict was

```
23 runs | 194 whole pairings | 6 involving a spread half | 13 unmatched slots
FRAGMENTED - paired, but too broken up to trust | average run 13.3 slots
```

**Signal 1 reads an alignment, so every Kaguya figure below inherits that.** The
damage is visible in the rows: `c5.5` has 15 pages, of which the solver paired
exactly two — local 7 and local 14 — leaving unpaired runs of 7 and 6 either
side of a single page in the middle of a chapter. Extracted, all fifteen are
ordinary manga. Those two runs are an artefact of a bad alignment and not a
fact about the release. The Kaguya rows are therefore evidence about
*positioning* — where this group puts its own matter — and much weaker evidence
about run *lengths*, which fragmentation both invents and splits.

Go-Toubun's alignment is unaffected and its result is unchanged: six runs,
candidates at exactly pages 95, 168, 198 and 225.

### The two `credits:` lines

```
credits: edition=en-fan against=raw-mono pages=235 slots=240 runs=6 candidates=4 bound=1 work=Go-Toubun no Hanayome v01
credits: edition=en-fan against=raw-mono pages=226 slots=231 runs=9 candidates=1 bound=1 work=Kaguya-sama v01
```

`work=` is last on the line, and that placement is load-bearing rather than
cosmetic: a work title holds spaces, and `duplicate-survey.sh` and
`margin-survey.sh` established the discipline that a survey reads these fields
positionally. Leading with the title shifted every later field by an amount no
collector can predict. Trailing with it leaves the eight fields before it fixed
and lets the free text run to end of line.

### Every candidate, and what it turned out to be

Fifteen runs across the two editions. Every page either edition marks `credits`
was extracted and looked at, as was every run of length 2 or 3, as were all
twelve of Kaguya's chapter-final pages and all twelve of its chapter-opening
pages — the last two groups are not candidates at all, and were viewed because
"where does *this* group put its credits" is the question the volume was added
to answer, and the fragmented alignment is not trustworthy enough to be asked
it.

| Edition | Pages | Member, local | Run | At end | Marked | What it actually is |
| ------- | ----- | ------------- | --: | ------ | ------ | ------------------- |
| go-toubun | 95 | `c001` 51/52 | 1 | yes | **credits** | donation card, `lhtranslation.com` — true positive |
| go-toubun | 168 | `c003` 31/32 | 1 | yes | **credits** | the same donation card, re-encoded — true positive |
| go-toubun | 198 | `c004` 29/30 | 1 | yes | **credits** | the same donation card again — true positive |
| go-toubun | 225 | `c005` 26/27 | 1 | yes | **credits** | staff credits, `TL: 5TBGamma / TLC/PR: ThunderCloud / ED: monka` — true positive |
| go-toubun | 226–229 | `c005.5` 0–3 | 4 | no | – | volume front matter: cover, title page, contents, half-title |
| go-toubun | 1–42 | `c000` 1–42 | 42 | no | – | the prototype one-shot, absent from the raw volume |
| kaguya | 0 | `c1` 0/31 | 1 | no | – | wraparound dust jacket — front, spine, back, barcode |
| kaguya | 2–4 | `c1` 2–4/31 | **3** | no | – | "THE INSIDE COVER!" omake, "THE INSIDE BACK COVER!" omake, and a blank page carrying only 「この作品は、デジタル配信用に再編集を行ったものです。」 |
| kaguya | 7 | `c1` 7/31 | 1 | no | – | colour chapter opener, 新連載! |
| kaguya | 105–111 | `c5.5` 0–6/15 | 7 | no | – | colour chapter title page, then ordinary manga |
| kaguya | 113–118 | `c5.5` 8–13/15 | 6 | no | – | ordinary manga |
| kaguya | 157 | `c8` 1/18 | 1 | no | – | chapter title page carrying the magazine's own あらすじ blurb and a `seiga.nicovideo.jp` promo |
| kaguya | **212** | `c10` 19/20 | 1 | **yes** | **credits** | **false positive** — the publisher's colour announcement |
| kaguya | 213–220 | `c10.5` 0–7/13 | 8 | no | – | volume front matter and character profiles |
| kaguya | 222 | `c10.5` 9/13 | 1 | no | – | a gag panel between two character profiles |

### The false positive is a colour insert, which was the predicted failure

Kaguya page 212 is `c10`'s last page and unpaired, so the rule takes it. It is a
full-colour magazine announcement: *"THANK YOU SO MUCH FOR THE POPULARITY!! BIG
NEWS!! The War of Love and Brains moves to a new stage!! Volume 1 will be
released on 3/18!!! Kaguya is moving to a new magazine, and will return in issue
17 of Young Jump on 3/24… as a weekly series!!"*

It is the **publisher's** own matter, not the scanlation group's, and the rule's
own definition is "a scanlation group's own matter rather than the work". It is
unpaired for the mechanical reason `c005.5`'s front matter was: `raw-mono` is a
tankōbon scan and this page belongs to the magazine serialisation, so there is
nothing on the raw side for it to pair with. The section above listed "the
false-positive rate against colour inserts and author afterwords" as unmeasured.
It is now measured, at one.

### This group prints no credit page at all

All twelve chapters carry `ScanInformation: Psylocke Scans` in a `ComicInfo.xml`
and nothing on any page. Checked three ways, because a negative is easy to
assert and hard to earn: all twelve chapter-final pages (eleven end on ordinary
manga with an `END` mark; `c1` ends on the tankōbon's grey half-title), all
twelve chapter-opening pages (all ordinary chapter openers), and a sweep of all
226 pages ranked by mean luminance and by standard deviation, since a donation
card is near-black and a plain text card is flat. The darkest pages are the
covers and colour inserts; the flattest are the digital-distribution notice, the
grey half-title, and the volume's table of contents. There is no card.

**So the second volume adds one group whose habit is to print no credits, and
four groups is not four data points about run length.** Go-Toubun's edition is
itself two groups — `lhtranslation` on `c001`/`c003`/`c004` and
`underdogscans + BOSEN SCAN` on `c000` — and `c005` names a third set of people
again.

### Why the bound stays at 1

Two questions decide it, and the corpus answers both against raising it.

**Is there a run of 2 or 3 that is genuinely credits?** No. Every credit page
found anywhere in the corpus is a run of exactly **1** — the three donation
cards, the `c005` staff page, and the `c000` disc. Five for five. There is
nothing longer to raise the bound *for*.

**Is there a run of 2 or 3 that is not credits?** Yes, and it is the corpus's
only run of that length: Kaguya pages 2–4, the volume's two inside-cover omake
and the digital edition's notice page. It is excluded by position as well
(`c1`'s local 2 is not a member end), so it would not flip today — but it is the
one observation available about what a three-page unpaired run tends to hold,
and it holds front matter.

So 1 is not a cautious compromise between two readings. **It is the only length
anyone has measured**, and the single piece of contrary evidence points the
other way. Raising it would buy no true positive and would put a three-page
block of front matter one position-rule away from being swept up.

**What this does not claim.** That no group prints a two-page credits spread.
Four groups is not the population; such a spread would be missed, and missed
silently. That remains roadmap item 1, and it is a recall gap rather than a
precision one — which is the safer of the two to have, since a missed credits
page costs a reader one page-turn and a swept-up front-matter page costs them
part of the book.

### Two things no page-level rule can reach

**Credits typeset onto a story page.** Go-Toubun `c002` local 40 is the
chapter's last story page, and `TL: 5TBGamma / PR: anon` is set in the bottom
margin under the artwork. There is no page to classify — skipping it would skip
the page the chapter ends on.

**A credits page the solver paired.** Go-Toubun `c000` local 43, page 43, is the
pink `underdogscans + BOSEN SCAN` disc naming five people. It is a run of 1 *at
its member's last index* — exactly the shape the rule is built for — and signal
1 never sees it, because the solver paired it and the rule only looks at
unpaired runs. This is the recall ceiling the section above records, confirmed
by extraction rather than inferred.

**Sample size, stated plainly.** Two volumes, two fan editions, 461 pages, 15
unpaired runs, four scanlation groups, one of which prints no credits. Five true
credit pages exist; signal 1 finds four of them and adds one false positive.
Precision four of five, recall four of five. Both denominators are of *known*
credit pages, and the corpus has been swept for unknown ones on Kaguya only.

*Superseded 2026-08-13 by the ten-edition survey below, which is the figure to
quote. This section is kept because it is where the bound and both signals were
first measured, and because three of the four things it argues from turned out
to be properties of two groups rather than of scanlations.*

## Fourteen fan editions, and the four signals that did not survive them

Measured 2026-08-13. The two-edition survey above was judged too small to
support turning page-skipping on by default — one page moves either of its
figures by twenty points — so eight more fan editions were ingested from the
NAS and adjudicated by extraction. **Every candidate below was extracted and
looked at by a person.**

| edition | group | pages | runs | candidates | true | known cards | swept |
| ------- | ----- | ----: | ---: | ---------: | ---: | ----------: | ----- |
| `gotoubun-v01` | lhtranslation, underdogscans+BOSEN | 235 | 6 | 4 | 4 | 5 | yes |
| `kaguya-v01` | Psylocke Scans | 226 | 9 | 1 | 0 | 0 | yes |
| `kaguya-v03` | Psylocke Scans | 215 | 8 | 0 | 0 | 0 | no |
| `kaguya-v08` | /a/nonymous, Jaimini's box & Fans Scans, Psylocke | 195 | 5 | 1 | 0 | 0 | no |
| `kobayashi-v01` | Cirno9baka | 153 | 10 | 9 | 9 | 10 | yes |
| `kobayashi-v03` | Cirno9baka | 153 | 9 | 8 | 8 | 10 | no |
| `kobayashi-v08` | TSHH | 152 | 1 | 0 | 0 | 0 | no |
| `oshinoko-v01` | Oak | 219 | 0 | 0 | 0 | 0 | no |
| `oshinoko-v03` | Oak | 195 | 1 | 0 | 0 | 0 | no |
| `oshinoko-v08` | Oak | 195 | 0 | 0 | 0 | 0 | no |
| `dungeon-meshi-v01` | 1r0n vs Yen Press | 187 | 4 | 2 | 2 | 2 | no |
| `frieren-v01` | vs VIZ Media | 181 | 0 | 0 | 0 | 0 | no |
| `domestic-v01` | danke-Empire vs Kodansha | 190 | 5 | 0 | 0 | **5** | no |
| `solo-leveling-v01` | Flame Scans vs Yen Press | 61 | 3 | 0 | 0 | **9** | no |

**2,557 pages, fourteen editions, twelve groups, 41 known credit pages.** The
last four are series with no raw anywhere on the NAS, so the fan edition is
aligned against **official English** and the work lives under
`corpus/incoming/fan-vs-official/` rather than as a settled directory — `corpus/README.md`
reserves those for works holding every edition that exists with `raw-mono` as
the hub, and a work with no raw cannot serve the zero-drift baseline that makes
one useful. All three aligned **sound**, at 178 to 182 whole pairings, which is
cleaner than any raw-hub edition here: same language, same chapters.

**Rebuilt with two invocations, not one.** `scripts/credit-survey.sh` walks one
level below the root it is given and looks for `<work>/en-fan`, so the settled
works need `scripts/credit-survey.sh corpus` and the four no-raw works need
`scripts/credit-survey.sh corpus/incoming/fan-vs-official` — the first call
reports `incoming` as skipped rather than descending into it, which is the
script working as written and not a bug to chase.

**Signal 1: precision 23 of 25, recall 13 of 15.** The two denominators differ
on purpose. Precision is over every candidate on all ten editions; recall is
over *swept* editions only — the three whose every page was ranked and whose
every extreme was adjudicated. The other seven had only flagged pages viewed,
so their `credit_pages` is a floor and not a proven denominator, and quoting a
recall over them would be quoting a number nobody earned.

### A page's size beats its position, and that was not on anyone's list

Two page-geometry tests, neither of which reads the alignment at all:

- **area** — pixel area below half the volume's median page area
- **aspect** — aspect ratio more than 15% off the volume's median

| rule | precision | finds |
| ---- | --------- | ----- |
| signal 1 | 23/25 | 23/41 |
| area alone | 32/67 | 32/41 |
| aspect alone | 41/87 | **41/41** |
| **area AND aspect, area < 0.5x median** | **32/32** | 32/41 |
| **area AND aspect, area < 0.8x median** | 41/45 | **41/41** |

The second column is "how many of the 32 known cards it finds" rather than
recall, because only three editions were swept exhaustively — everywhere else
`credit_pages` is a floor, so a miss count is a floor too. Signal 1's recall
over the swept three is 13 of 15.

The conjunction is principled rather than fitted, which is the only reason it is
recorded here at all: **a credits card is a canvas the group made, not a scan of
a printed page**, so it is both smaller than a page and a different shape. Each
single test's failure mode is exactly what the other rejects. Oshi no Ko mixes
`[web]` MANGA Plus pages at 1464x2100 with `[raw-d]` pages at 844x1200 in one
volume, and the small ones are still page-*shaped*, so aspect rejects all 35
that area wrongly took. Joined spreads are oddly shaped but full size, so area
rejects the 13 that aspect wrongly took.

**Held out and pre-registered.** After the rule scored 25/25 on seven editions,
its prediction for the remaining three — no credits card in `kaguya-v03`,
`kaguya-v08` or `kobayashi-v08` — was written to
`docs/superpowers/specs/2026-08-13-credit-size-rule-prediction.md` and committed
at `80350bf` *before any page of them was viewed*, together with the failure
mode expected if it was wrong. Every chapter-final page of all three was then
adjudicated and the prediction held.

**What that test did not establish, and the document said so in advance.** All
three volumes are low resolution — 1.00 to 1.07 MPx median against
`kobayashi-v01`'s 3.18 — so a 0.72 MPx card sits *above* the half-median cut and
the area half cannot fire in them whatever the page is. The prediction was right
because those editions carry no cards, not because the rule would have caught
them. **A low-resolution volume that does carry cards is unfound, and is the
sharpest test remaining.**

### Wave 2: a group that prints its card at the chapter's start

`domestic-v01` is the edition where the two rules disagreed, and the
disagreement was pre-registered at `d8b93c8` before a page of it was viewed:
signal 1 proposed nothing, the size rule proposed five. **The size rule was
right on all five.**

All five are 752x700, all five are the same card, and all five sit at a
member's **start** — local index 1 in four of six chapters, local 2 in the
first. Signal 1's rule is "isolated, unpaired, at a member's *last* index", so
it cannot see any of them. That is a **direct counterexample to a claim in the
code**: `UnpairedRun`'s doc says the first-index version of the rule "is zero
for four — what accumulates at a member's start is front matter". That was
gotoubun-v01 alone. Here it is five for five the other way, and the doc is
corrected in place.

So a card's *position* is a per-group habit — start or end — while its *size*
is not. That is the strongest argument yet that the size rule is measuring the
thing itself rather than a correlate: eleven groups, two opposite positional
conventions, one geometric signature.

**A page carrying translator credits is not always a card.** `domestic-v01`
page 189 — `c005.5` local 8 — is an **official** page with the translator
credits typeset onto it, and it must not be skipped: the reader wants that
page. The size rule leaves it alone by construction rather than by luck, since
at 869x1300 it is exactly the volume's median area and page-shaped. It is the
official-edition analogue of the credits-on-a-story-page ceiling this file
already records for Go-Toubun `c002` local 40, and it is the case that shows
why "carries credits" and "is a credits page" are different questions.

**A trap worth writing down, because it caught three separate measurements on
this branch.** Page indices are the *sidecar's* member order, which is not the
shell's. `c005.5` sorts before `c005` in a glob, and `c10.5` before `c10`, so a
glob-ordered scan shifts every page after the reordered member. It put four
wrong member indices into kaguya-v01's fixture, reported gotoubun-v01's staff
credits page as 234 rather than 225, and numbered domestic-v01's last card 166
rather than 157. The adjudicated images were consistent each time; only the
numbers moved. Read member order from `taiyaku.json`, never from a glob.

### Wave 3: the format that defeats a size rule, predicted before it was seen

`solo-leveling-v01` is a Korean webtoon against Yen Press's paginated volume 1,
ingested as a deliberate stress case. It stresses something the design never
anticipated.

**Its alignment is REFUSED** — 1 run, **0 whole pairings**, 321 unmatched slots
— because 61 vertical scroll strips do not correspond to 314 print pages. So
this is the first edition where **signal 1 cannot run at all**: not a rule that
fails, an input that does not exist.

**It carries six credit pages, the densest in the corpus.** Flame Scans prints
two per chapter: a staff banner at each chapter's start, naming translator,
proofreader, cleaner, typesetter, quality checker and raws provider, and a
recruiting card at each chapter's end carrying Discord, Patreon, Ko-fi and
PayPal links. The end cards are byte-identical across all three chapters.

**The size rule misses all six, and the mechanism was named in advance** at
`32a242b` before any page was viewed. Tall strips make the median page 2.39
MPx, so cards at 1.65 to 1.78 MPx are 0.69 to 0.74 of that — above the 0.5 cut
— and the area half cannot fire whatever is printed on them.

**The blind spot is relative, not about resolution.** The wave-1
pre-registration described it as a low-resolution failure: "a 0.72 MPx card
sits above the half-median cut". That was the right mechanism read off the
wrong axis. What matters is the card's size *against the median page*, so a
format with unusually **large** pages defeats the rule exactly as one with
unusually small pages does, and this edition arrived from the opposite
direction to the one predicted.

**And it makes the conjunction's cost visible for the first time.** Aspect
alone catches all six and is now 38 of 38 across the whole corpus, at precision
38 of 87. The conjunction is 32 of 32 precision and 32 of 38. Every earlier
edition had the two agreeing on recall, so the conjunction looked free; it is
not. For skipping-on-by-default the trade still favours it — this file's own
argument is that a swept-up story page costs a reader more than a missed card
costs them — but it is a choice with a documented hole rather than a rule that
works everywhere.

### The blind spot both rules share is real in principle and empty in fact

A credits page typeset at full page size defeats the size rule completely:
neither half fires, because it is neither small nor oddly shaped. `sidecar.md`
and this file both name it, and nothing had ever looked for one.

**Searched 2026-08-13, and nothing turned up.** Every card in this corpus sits
at local index 0, 1 or 2 of its member, or in its last two pages, so the search
took all 421 pages in those positions that neither size test flags and that
nobody has called a card, and looked for near-duplicates across members — a
group reusing a card prints it once per chapter, which is the one property that
survives the card being page-sized. The hash is a plain dhash-16 computed on the
**uncropped** image, so O41's per-member border cannot distort it.

**44 near-duplicate pairs came back and not one is a credits page.** They are:

- Go-Toubun's volume cover, carried by both `c000` and `c005.5` at distance 2 —
  the pair this file already records under signal 2
- Kaguya's inside-cover omake, in both `c1` and `c10.5` at distance 0
- **Kobayashi's decorative half-title divider**, at distance 0 to 9 across seven
  chapters of v01 and distance 0 across seven of v03 — a mostly-white page
  carrying the series logo in grey, reused as a chapter divider

So the blind spot stands as a *principled* limitation and an *unmeasured* one:
fourteen editions and twelve groups, and no group in any of them prints a
full-page credits card. That does not make the rule safe on the next edition. It
does mean the `credit_pages` floors on the eleven unswept editions are tighter
than "unswept" suggests, at the positions where every known card actually sits.

**It also settles why signal 2 was never going to work.** This file says
"duplicated front matter is as common as duplicated credits"; the search
quantifies it. Restricted to exactly the positions where cards live, and
excluding every page a size test already explains, recurrence across chapters
returns **44 pairs, all of them the volume's own matter and none of them a
card.** Recurrence is a property of anything a compiler reuses, and a decorative
divider is reused far more reliably than a credits card is.

### Where this stands: a measured frontier, and no rule that clears it

**IN PROGRESS.** Phase A's survey is recorded; the rule is not settled and
nothing in the binary depends on it. What follows is the state as of
2026-08-13, so a later session starts from the numbers rather than from the
reasoning that produced them.

**No rule is correct on all fourteen editions.** Holding the aspect test at 15%
off the volume's median and sweeping the area threshold gives a frontier:

| area threshold | precision | finds | misses |
| -------------- | --------- | ----- | ------ |
| 0.50x median | **32/32** | 32/41 | 9, all `solo-leveling-v01` |
| 0.60x | **35/35** | 35/41 | 6 |
| 0.70x | 38/42 | 38/41 | 3 |
| **0.80x to 1.00x** | 41/45 | **41/41** | none |

Either end is clean and the middle is not: zero false positives at 0.50 to
0.60, or zero misses at 0.80 to 1.00 for the price of four. The 0.80-to-1.00
plateau is three consecutive values with identical results, so it is not a
knife-edge — unlike the margin threshold this file could not ship, where the
one confirmed needle sat one bit below the weakest confirmed-good pairing.

**The four false positives at 0.80.** One is `oshinoko-v08` page 190, which the
sidecar already marks as a spread — and that exclusion is *not* usable, because
`kobayashi-v01`'s sidecar marks all ten of its cards as spreads too, they being
landscape. The other three are ordinary Solo Leveling panels at 720x2289, tall
strips slightly off their volume's median.

**`solo-leveling-v01` has now been under-counted twice.** Wave 3 predicted three
cards, adjudication found six, and the threshold sweep found nine — the patron
thank-you at local n-2, discovered only because it appeared as a false positive
at 0.80 and turned out not to be one. Three cards per chapter: staff banner,
patron list, recruiting card. **That is the strongest argument for treating
every unswept edition's `credit_pages` as a floor.** Eleven of the fourteen are
unswept, and the one edition looked at hardest kept yielding more.

**What is unbuilt.** None of this is in the binary. `MAX_CREDIT_RUN` is still 1
with its justification withdrawn; the size rule exists only in fixtures and
this note; and Phase B's tier design still names two corroborators that died
here. A later session picking this up should decide the frontier question first
— zero false positives or zero misses — because everything else follows from
it, and the design's own argument that a swept-up story page costs a reader
more than a missed card favours the left-hand end.

### Three signals died here, and one of them was this survey's own

**Colour, refuted.** The tier design argued from Kaguya that colour marks
publisher and magazine matter while a scanlation group's card is
near-universally greyscale. Every one of Cirno9baka's twenty cards is in colour,
and the one non-credits page in `kobayashi-v01`'s candidate region — the
Futabasha colophon — is greyscale. The sign is the opposite of what was argued.

**dhash recurrence, demoted.** See O41: `describe_pages` crops by a per-member
`common_border` before hashing, so a page is hashed as a function of itself and
of the archive it arrived in, while `recurrences` is the one consumer comparing
across members. Its three-of-five is a floor.

**Area alone, killed by the corpus it was found on.** It scored 25/25 on the
three editions it was derived from and 25/60 once Oshi no Ko was added. That is
the same overfitting objection this file made against the colour bet, landing on
a rule this survey proposed itself.

### What the bound is worth now, and it is not what it was

The `MAX_CREDIT_RUN = 1` doc says "every credit page found anywhere is a run of
exactly 1, all five of them, so there is no measured run of 2 or 3 to raise this
for." **That is no longer true.** `kobayashi-v01` page 152 is a Cirno9baka card
sharing a run of two with the publisher's colophon at page 151, so the bound
rejects both. Raising it to 2 would take one true positive and one false
positive together, so the trade is now measurable rather than absent — and it is
still the only run of 2 known to hold a card.

**The paired-credits-page ceiling is a recurring shape, not a one-off.**
`gotoubun-v01` page 43 was recorded as the single case of a credits page the
solver paired, invisible to signal 1 because it reads only unpaired runs.
`kobayashi-v03` pages 91 and 152 are the second and third. No bound reaches
these: they sit in no unpaired run at all.

### Three things this survey retires

**That a series' fan edition is one group's habit.** `kobayashi-v08` is TSHH,
not the Cirno9baka that scanlated v01 and v03 of the same series, and
`kaguya-v08` names three groups on one volume. Group identity is per volume, and
`ComicInfo.xml` is the only thing that says so — 12 of 57 archives carry one.

**That the luminance sweep generalises.** It was calibrated on Go-Toubun's
near-black donation card and Kaguya's flat notice page. On `kobayashi-v01` the
missed page 152 ranks 76th of 153 by mean luminance and 62nd by standard
deviation, and of that volume's nine known cards only one would have made a
top-twenty cut on either measure. Aspect found it immediately. The sweep in the
plan was corrected accordingly.

**That volume-packaged editions align badly.** Oshi no Ko ships one archive per
volume and aligns *better* than the chapter-packaged editions — 3 runs and 216
whole pairings on v01 against `kaguya-v03`'s 30 runs. What a single member costs
is signal 1, which has no member boundaries to fire at: three editions, 219 to
195 pages each, and zero candidates between them.

## M3: what the Stump instance actually offers, measured 2026-07-31

Everything below is what `2026-07-31-m3-stump-opds-design.md` rests on, and every
claim `status.md` makes about Stump support, the composite source and the OPDS
feed traces to a line here.

### The instance

`stump.daveynet.xyz`, **Stump 0.1.4 nightly**, rev `c496517b`, compiled
2026-06-21, behind Cloudflare. Eight libraries, all rooted at `/data`. Two
protocols, split by what Stump actually offers rather than by what the reader
design guessed:

| Surface | Where | Notes |
| ------- | ----- | ----- |
| Whole API | `POST /api/graphql` | 401 JSON unauthenticated |
| Page bytes | `GET /api/v2/media/:id/page/:n` | **1-based**; `image/jpeg`; no `ETag`; 404 past the end |
| OPDS | `/opds/v1.2/*` | Basic auth; **already declares `xmlns:pse`** |

There is no `/api/v1` — those paths fall through to the SPA, which is what makes
a stale REST assumption look like a working server returning HTML rather than an
error.

### `Media.hash` and `koreaderHash` are null, and neither is a SHA-256 anyway

Checked across every book measured, epub and cbz alike: both fields are null on
every one. Neither is the right quantity even when present — `koreaderHash` is
KOReader's partial MD5, not a whole-file digest. Stump resolution therefore
confirms a candidate with a **page** hash — `sha256` of `page/1`, compared
against the local member's first page — never a file hash. Worth stating flatly
because a field called `hash` sitting beside a sidecar field called `sha256` is
exactly the kind of thing a later implementer reaches for without checking.

### `Media.name` is the file stem, not the basename

`Go-Toubun no Hanayome - v01 (2018)`, not `… .cbz`. Matching Stump's `name`
filter against a full basename returns nothing, every time — the extension has
to be stripped before the query goes out.

### Coverage: of one work's three editions, Stump holds one

Measured against every edition directory of the primary development volume,
`corpus/gotoubun-v01`, by exact file size:

| Edition | In the sidecar | Pages | In Stump |
| ------- | -------------- | ----- | -------- |
| `en-mono` | yes | 187 | yes |
| `raw-mono` | yes | 195 | no |
| `en-fan` (7 files) | yes | 235 total | no |
| `en-color` | no — on disk, not a sidecar member | 199 | yes |
| `raw-color` | no — on disk, not a sidecar member | — | no |

Of the three editions the work actually has, Stump serves exactly one. A
whole-work `PageSource` backed only by Stump would 404 on two thirds of every
work in this library — the reason the composite source exists rather than a
standalone Stump-backed `PageSource` implementation.

### Page agreement: five pages checked byte for byte before building the test

Stump book `a8ed51d8-c4c8-4efa-b427-2ba5327f477b` against
`corpus/gotoubun-v01/en-mono/Go-Toubun no Hanayome - v01 (2018).cbz`:

| Stump | taiyaku | Bytes | SHA-256 (truncated) |
| ----- | ------- | ----- | -------------------- |
| `page/1` | index 0 | 1,180,617 | `9183af35f1b47e7fbb9994aca642b8de` |
| `page/2` | index 1 | 1,343,956 | `98d412bdf8cb371d11f611418e7fc4cd` |
| `page/3` | index 2 | 339,050 | `91fd1f798337093345ad04944de9d25c` |
| `page/100` | index 99 | 1,884,398 | `acaf0c241e0826aafe4843b2a9214765` |
| `page/187` | index 186 | 978 | `eb60ee8901f45efecc62db4176fb685d` |

Page 187 is the one `.png` `en-mono` carries among 186 `.jpg` entries — see "The
extension lies about a page's format too" above. Stump serves it as
`image/png`, so it sniffs content the same way `taiyaku-serve` does rather than
trusting the extension, and it derives the same page sequence taiyaku does,
including where that one file sorts. **This is agreement, not a guarantee**:
nothing in Stump's contract binds it to taiyaku's natural-order rule, which is
normative in `sidecar.md` and merely conventional in Stump. The equivalence
check below is what stands guard; this table is what justified writing it.

### The live equivalence check, 2026-07-31

`crates/taiyaku-serve/src/corpus.rs::stump_and_local_serve_the_same_bytes_for_the_same_member`,
gated on `TAIYAKU_CORPUS` and a reachable Stump, run for real against the
instance above:

```
187 pages compared against stump; 8 members stump does not hold
```

Zero disagreements. The 187 are every page of `en-mono`, the one member that
exists in both sources; the 8 absent are `raw-mono` plus all seven `en-fan`
chapter files, matching the coverage table exactly — each recorded as absent
rather than retried per page, and distinguished from a member the circuit
breaker simply never got to (`Some(None)` versus a missing key; see Task 9 in
`progress.md`). This is the third of the four seam tests the reader design
names, and the first end-to-end proof that the composite's Stump-backed path
reads the same bytes `LocalLibrary` does over a live network round trip, not
merely against a recorded fixture.

The companion check in the same run, comparing every page the server serves
against the page the aligner sequenced with no Stump involved:

```
617 pages compared across 3 editions
```

no disagreement — the M2 seam test, re-verified rather than assumed still
green.

### Two series for one work, and what taiyaku's feed actually answers

```
/data/Manga/Go-Toubun no Hanayome                        14 books
/data/Manga/Go-Toubun no Hanayome - Full Color Edition   14 books
```

Neither Stump nor Komga has a concept that relates these, and Stump already
serves OPDS-PSE on its own — Panels can read this library today without
taiyaku. So M3's stated end state, "Panels reads the library", was already true
before this milestone. What taiyaku's feed adds is the grouping above, which is
the one thing neither server can express. See "Stump and Komga cannot express
multiple editions of one work" in the design doc for the fuller argument;
recorded here because it is the finding `status.md`'s account of M3 rests on.

### The Panels performance measurement is still outstanding

The request-log middleware and `scripts/request-summary.sh` exist and were
exercised against a real concurrent burst — in-flight snapshots of 1, 1, 2, 3,
4, 5 recovered the burst's depth exactly, `?width=`/`?height=` were captured,
and every emitted line parsed as JSON — but no iPad has run Panels against this
server yet. The performance targets the design names are not filled in here
because they have not been measured; writing in a plausible-looking number
before the run happened is exactly the kind of claim this document exists to
avoid making.

## Panels, measured: the feed was one-based and should not have been

First real session against `taiyaku serve`, 2026-07-31, Panels on an iPad over
the LAN. 73 requests, one work, three editions. The log is the instrument the
reader design argued for, and it earned itself immediately.

### The defect it found

**OPDS-PSE counts pages from 0, and taiyaku's feed counted from 1.**

Panels opens a book by requesting `pages/0`. Against the 1-based version that
was a 404, so it showed an error screen with a `!`; tapping advanced it to
`pages/1`, which taiyaku answered with index 0 — **the first page, under the
number Panels uses for the second**. Every page after it was shifted, and the
true first page of every edition was unreachable. Panels retried page 0 up to
nine times per edition: **22 of the session's 73 requests, 30%, were that 404.**

**Stump settles the convention**, and is better evidence than re-reading the
spec because Panels already works against it. Its REST API is 1-based:
`/api/v2/media/:id/page/1`. Its OPDS-PSE template is not:

```
href="/opds/v1.2/books/<id>/pages/{pageNumber}?zero_based=true"
     rel="http://vaemendis.net/opds-pse/stream" pse:count="190"
```

It converts *for* PSE. So there is one conversion boundary in taiyaku, not two:
`+1` to Stump's REST API, and nothing at all for PSE.

**Why no test caught it.** Both conversion sites had tests and both asserted
taiyaku's own convention rather than the protocol's, so the 1-based test passed
against 1-based code and proved only self-consistency. A test cannot find a
disagreement with a client it never talks to. This is the same shape as the
member-order bug: a rule held in one place and assumed in another — except the
other place was a third-party application, so only a third-party application
could show it.

### What the log says about how Panels fetches

Two sessions after the fix: a short read (26 requests) and a deliberate
fast-flick run (179 requests, 177 of them pages, 115 seconds, every one a 200).

| Measure | Value |
| ------- | ----- |
| Page requests, flick session | 177, all distinct, none re-requested |
| Transferred | 43.7 MB total, 247 KB mean, 2.0 MB largest |
| Server processing, median | **1.43 ms** |
| Server processing, worst | 15.6 ms |
| Tightest gap between arrivals | **40 ms** |
| Gaps, 10th percentile / median | 44 ms / 114 ms |
| Concurrent page requests | **none observed, in any session** |
| Dimension parameters requested | **none, in any session** |

**The server is not the constraint, by a wide margin.** Median processing is
1.43 ms against a tightest client request rate of 40 ms — roughly twenty-eight
times faster than the client asks. Everything a reader waits for is transfer and
client-side decode of a quarter-megabyte scan, not taiyaku.

**Panels issues no concurrent page requests.** Zero overlaps across 177
requests, and the arrival floor holds at 40 ms even when flicking as fast as the
UI allows. This is a statement about *arrivals*, which is why the server's speed
does not undermine it: a client prefetching a batch would emit those requests
within a millisecond or two of each other whatever the server did, and none ever
arrived bunched.

**What this log cannot say, and should not be read as saying.** It cannot
distinguish "Panels waits for each response before sending the next" from
"Panels is simply paced by its own UI" — the server finishes in 1.43 ms, so no
request was ever still open when the next arrived, and a blocking client and a
timer-driven one are indistinguishable at that speed. Measuring that needs
either a deliberately slowed response or client-side instrumentation, and
neither is worth building for it.

**No reduced size was ever requested**, across every session. Panels takes
whatever resolution the server holds. Komga and Stump both accept a width
parameter on the PSE stream and taiyaku offers none, so this log cannot say
whether Panels would use one if advertised — only that it does not ask
unprompted.

**As a target for a later Swift reader**, the useful figure is not a server
number, since 1.43 ms leaves nothing to beat. It is that a page turn at this
library's page sizes moves about 247 KB, and that a reader can sustain a
40 ms turn rate without the server becoming the limit.

### An ETag would have saved nothing, and the obvious way to check that is wrong

O1 asks whether page responses should carry cache headers. The plan recorded
against it was to run a Panels session and see whether the client sends
`If-None-Match`. **That measurement cannot work, and understanding why is most
of the finding.**

`If-None-Match` carries an entity tag the client obtained from an earlier
response. This server sends no `ETag` and no `Last-Modified`, so Panels has
nothing to send back — the count is zero by construction, on any session, no
matter how the client behaves. `scripts/request-summary.sh` reports it as
`0 of 179`, and that line is a fact about the server, not about Panels. Asking
the question this way would have produced a confident answer to nothing.

**The question that can be asked without a validator** is the one an ETag would
be answering anyway: does Panels ever fetch a page it already has? A 304 saves
bytes only on a second request for the same resource, so if there is no second
request there is nothing to save, and that is visible in a log that carries no
headers at all.

| | Session 1 (2026-07-31, one-based bug) | Session 2 (after the fix) |
| --- | --- | --- |
| Page requests | 65 | 177 |
| Distinct pages | 46 | 177 |
| Paths requested more than once | 3 | **0** |
| Statuses | 51 × 200, 22 × 404 | 179 × 200 |

Session 2 re-requested nothing across 177 pages and 115 seconds, spanning three
editions. Session 1's three repeated paths are `raw-mono/pages/0`,
`en-mono/pages/0` and `en-fan/pages/0`, requested 7, 6 and 9 times — and **every
one of those 22 requests was a 404 of 45 bytes**, the retry loop the off-by-one
produced. They are not a client re-fetching a page it has; they are a client
failing to get one at all.

The two sessions overlap in exactly those same three paths and nothing else. So
across both logs, **the number of successful page fetches for a page already
successfully fetched is zero.** An ETag would have produced no 304s and saved
none of the 43.7 MB.

**What this does not say.** It is a statement about Panels, not about caching.
The browser reader refetches pages by construction — swapping editions is a
refetch of the page you are looking at, in a different edition, and going back
is a refetch of the one you left — so the same measurement against it should be
expected to come out the other way. That is where O1's value is, and it was
already the suspicion; this attaches a number to it rather than leaving it a
guess.

**And the order to do it in, next time.** Ship the validator first, then measure
the 304s. The client-side question is unanswerable while the server offers
nothing to answer it with, so "measure, then decide whether to ship" inverts on
this one item: the cheap thing is the header, and the evidence only exists
downstream of it.

Two smaller things these logs cannot say, because they were written before the
request log recorded any headers: what `Accept` Panels sends — which would say
whether serving anything other than the stored bytes could help — and whether it
ever sends `Range`. Both are captured now, and the summary reports
`(not recorded)` for the old logs rather than pretending.

### The one qualitative note

The owner's assessment after the fix, which no log can capture: performance
"seems similar or equivalent to using Stump/Komga with Panels". That is the bar
the reader design set, met on the first attempt — but it is an impression, and
the numbers above are what a later Swift reader should actually be held to.

## The only concurrent requests Panels ever issued were covers

Three open items — O6, O9 and O11 — were carried out of the M3 review as
things that wanted a decision rather than code. All three turned out to be
answerable from artefacts already on this machine: the two Panels logs, and the
cache directory the server has been writing to since M3. Nothing new was run.
None of the three closes; each now sits on a measurement and names what would
reopen it, which is the shape O1 established.

### `in_flight` peaks at 3, and every one of them is a cover

The table under "What the log says about how Panels fetches" records
**Concurrent page requests: none observed, in any session**. That is exact, and
the qualifier is load-bearing — it has already been read too broadly once, while
scoping O9 in this session, as "Panels issues no concurrent requests". Re-read
on the `in_flight` field the middleware records against every line:

| | Session 1 | Session 2 |
| --- | --- | --- |
| Requests logged | 73 | 179 |
| Of those, page requests | 65 | 177 |
| Max `in_flight` across **page** requests | **1** | **1** |
| Max `in_flight` across **all** requests | **3** | 1 |

Across 242 page requests in two sessions, no page request was ever in flight
alongside another. The claim as written holds exactly.

What it does not say is that nothing overlaps. Session 1's three cover requests
are the whole of the concurrency in either log:

```
in_flight  path                                          status   time      bytes
    2      /opds/v1.2/works/taiyaku/en-mono/cover          200    95.9 ms   60,064
    3      /opds/v1.2/works/taiyaku/raw-mono/cover         200   109.0 ms   54,543
    1      /opds/v1.2/works/taiyaku/en-fan/cover           200   221.4 ms   54,199
```

Panels opens a work by fetching its editions' covers together, and a work with
three editions therefore produces a three-deep burst on a cold cache. Session 2
shows none of this because the covers were already cached by then — which is
also why the burst is a *cold-start* property and not something a second reading
session would ever reveal.

### O9: the one measured burst is one single-flight cannot merge

O9 asks for a single-flight guard on the request path, so that several requests
arriving during one Stump timeout share a single outcome instead of each waiting
it out. The cover burst above is the only concurrency ever observed here, and
covers reach Stump by exactly the same path pages do — `ServeState::page` into
`CompositeSource::page` — so this is not an academic case. It is the case.

**It is still not a case single-flight answers.** A single-flight guard
deduplicates concurrent requests *for the same key*. The burst is one request
per edition — `en-mono`, `raw-mono`, `en-fan` — which are three different
members resolving to three different Stump books. There is nothing to coalesce.
A guard would add a map, a lock and a wait, and merge zero of the three requests
that have ever actually overlapped on this server.

The case single-flight *would* answer is two concurrent requests for the same
page of the same edition. Neither log contains one, and the client that
generates the burst above cannot produce one: Panels never re-requests a page it
already holds (see the ETag section above), so its own two request patterns are
each single-flight-proof for different reasons.

**What already covers the repeat case** is the cooldown latch in
`composite.rs` — after the first transport failure returns, every later request
skips the network entirely for `DOWN_COOLDOWN`. So the unguarded window is not
"forever", it is "requests already in flight when the first failure returns",
which `note_failure`'s own doc comment states and which this log now bounds at
three, all of them distinct keys.

**What would reopen it:** a client that issues concurrent requests for the same
key. M5's browser reader is the candidate — a filmstrip resolving per tile, or
prefetch-ahead colliding with a reader jumping to the page being prefetched. O14
records the same filmstrip as the trigger for its own fix, which is the second
reason to expect that measurement to come out differently from this one.

### O6: a cover costs 96 to 221 ms, and the 430 MB case needs a sidecar nothing writes

O6 records that a cover is generated from page 0, which on an art-book edition
is a 430 MB decode. The three cover generations above are the only ones ever
measured against a real client, and they are the ordinary case: **95.9, 109.0
and 221.4 ms**, producing 54 to 60 KB of JPEG.

The 430 MB figure is real — Color Walk 10 is 151 pages at 10200×14039 — and so
is the slowness it implies. "Art books cannot be aligned, and the solver does
not say so" measures four of those pages fingerprinting in 8.6 s at a 3.0 GB
peak, so a single decode is on the order of 2 s: roughly twenty times the
worst cover measured here, paid once per edition and then cached forever.

**What stops it happening is upstream of this crate.** A cover is generated for
an edition named in a sidecar, and the same section measures that art books
cannot be aligned at all — four pages, four pairings, every margin at or below
zero and two of them negative. An art book has no business in a `taiyaku.json`,
so the material that would make this decode expensive is material nothing
should be writing a sidecar for. The corpus bears that out: the one sidecar
that exists, `corpus/gotoubun-v01/taiyaku.json`, names three ordinary manga
editions, and Color Walk sits in `incoming/` as a negative control.

This is a statement about reachability, not about impossibility. `align` will
happily pair an art book against a volume and `--write` the result; nothing
refuses it. So the premise is one bad invocation away rather than architecturally
excluded, and that is the honest version of the claim.

**What the burst does settle** is the other half of O6, which was never in
doubt but had never been observed: `MAX_CONCURRENT_GENERATIONS = 2` was chosen
against a guess that "a cold OPDS grid view issues one cover request per edition
at once". It does. Three at once, on the only cold grid ever logged. The two
that overlapped took 95.9 and 109.0 ms and so never queued behind the permit
limit; a fourth edition would have. The guess was right and is now a
measurement.

**What would reopen it:** art-book linking shipping, which is the "Later" item
that would give an art book a legitimate reason to appear in a sidecar. Failing
that, any edition whose first page is large enough that one decode is felt on a
cover grid — the number to beat is the 221 ms above.

### O11: the cover cache is 165 KB after every session ever run

O11's second half records that generated covers "are unbounded and never
pruned — an archive that is replaced gets a new cover under its new hash, but
the old file is never reclaimed". True, and the size of the problem is:

```
.taiyaku-cache/covers/   3 files   164.9 KB
```

Three covers, one per edition of the one work that has ever been served, at
54,199 to 60,064 bytes each. Growth needs an *archive* to be replaced, not a
page to be read: the key is the member's `sha256`, so re-reading a work forever
produces no new files at all. A library of a hundred works at three editions
apiece, every one of them replaced once, reaches about 33 MB.

**Two different caches share that directory tree**, which is worth knowing
before anyone clears "the cache": `.taiyaku-cache/covers/` is the server's, and
the 55 `<sha256>.json` files beside it — about 1.2 MB — are the CLI's
fingerprint cache, the one `--cache` names and the one measured at 3.4 s → 189 ms.
Deleting the parent directory throws away both, and only one of them is cheap
to rebuild.

**What would reopen it:** a library where archives are replaced routinely
enough for 55 KB apiece to accumulate, or a second cache consumer — the
filmstrip's thumbnail-per-page route, which `cover.rs` explicitly defers to M5,
would be 195 files per volume rather than one and changes this arithmetic
entirely.

**The first half of O11 is not affected by any of this.** Nothing watches Stump
for a book added after a work first resolved; that remains a deliberate
tradeoff, recorded as Risk 5 in the design, and `--refresh-stump` remains the
way to act on it.

## O8, measured three ways: the third one is why it was worth fixing

O8 — Stump resolution running to completion before the listener binds — had
been deferred twice on the same sentence: "not one to make on a corpus small
enough that nobody has felt the wait." That is a claim about a duration nobody
had timed. Timing it took three runs against `corpus/gotoubun-v01`, which is
one work, three editions, nine members.

The instrument is the gap between process start and the `taiyaku serve
listening on …` line, which is exactly the window in which the server exists
and cannot answer anything.

| Start | Before | After |
| ----- | -----: | ----: |
| Warm cache, Stump reachable | 0.10 s | 0.10 s |
| Cold cache, Stump reachable | 3.15 s | 0.10 s |
| Cold cache, Stump unreachable | **30.09 s** | **0.10 s** |

**Two of the three rows vindicate the deferral.** A warm start makes no queries
at all — `taiyaku.stump.json` answers for all nine members and the transport is
never touched, which the absence of any `resolving …` line confirms rather than
merely implies. A cold start against a reachable instance is 3.15 s for nine
members, about 350 ms each, and that is not only network: `resolve_member`
needs the member's *local* file for its size and for the page-hash confirmation
that makes a match trustworthy, so part of every one of those 350 ms is reading
and hashing an archive on this machine.

**The third row is a different animal, and it is the one the deferral never
weighed.** 30.09 s is the GraphQL POST's 30-second global timeout, paid once —
the circuit breaker introduced in the M3 review spans works, so twenty works
cost one timeout rather than twenty. What the breaker does not do is make the
cost occur once *in time*. It recurs on every start for as long as the instance
is down, and it buys nothing whatsoever: every page of every member in this
corpus is on local disk, and `CompositeSource` would have fallen back to disk
for all of them the moment it was asked. The server spent 30 seconds refusing
to offer what it already had.

**That this is not hypothetical was demonstrated accidentally.** Between the
"before" and "after" runs of this very measurement the instance went down —
Cloudflare returning 502 and then 521 (origin unreachable) to a query that had
answered in 50 ms twenty minutes earlier. The unreachable case is not a
thought experiment about a NAS reboot; it happened twice inside one session of
measuring it.

### What changed

Resolution now runs behind the bound listener, on `spawn_blocking` — a network
round trip and a local page hash per member, neither of which belongs on an
async worker, the same boundary O12 established for `get_page`. `StumpSide`
holds its resolutions behind an `RwLock` rather than owning a finished
`HashMap`; the map starts empty and fills as each work completes.

The safety argument is that **a member not yet resolved and a member Stump does
not have are the same thing to `book_for`** — both are `None`, and both already
meant "read it off disk". So the window during which resolution has not landed
is not a degraded mode; it is the mode this server has had since M2, which is
also why the change needed no new fallback path.

**Reporting is per work rather than one aggregate at the end**, which is the
half that is easy to miss and was worth a test of its own. Handing back every
work's answers together would have moved the wait behind the listener instead
of removing it: a twenty-work library would serve all twenty from disk until
the twentieth had been asked about, which is the original defect wearing a
different hat.

### What the "after" column actually measures

Not just a fast bind — a *useful* one. Against a deliberately unroutable
address, so that resolution hangs on the full 30-second timeout rather than
failing fast:

```
bind at 0.00 s; three pages served by 0.02 s
  page 0: http=200 bytes=1,180,617 time=0.0040s
  page 1: http=200 bytes=1,343,956 time=0.0036s
  page 2: http=200 bytes=339,050  time=0.0015s
```

Three real pages of `en-mono`, 2.8 MB between them, served in about 4 ms each
while the resolution behind them was still waiting on a socket that will never
answer. Before this change the first of those requests could not have been made
for another 30 seconds.

The stderr order is part of the fix rather than incidental to it:

```
taiyaku serve listening on http://127.0.0.1:18099
taiyaku: resolving en-fan/… - c000.cbz against stump (1 of 9)
```

"listening" now precedes the resolution chatter, which is how a person watching
a cold start can see that the wait is no longer theirs. It precedes it on a
*warm* start too, where resolution costs nothing — the ordering is structural,
not a race that happens to come out this way when there is work to do.

**Every cell of that table is measured, including the ones that looked obvious.**
The "after" column for the two reachable rows was initially inferred from the
unreachable one — the bind no longer waits on resolution at all, so it plainly
could not matter whether the instance answered — and inferring it was the wrong
instinct in a document whose whole premise is that plausible things get
measured. Run properly: warm 0.10 s, cold 0.10 s, both after the change.

### The instance moved underneath the measurement, which is itself the argument

The "before" cold-reachable figure is 3.15 s. Re-resolving the same nine members
about half an hour later — after the machine hosting Stump had been restarted —
the background resolution completed in **0.82 s**, roughly a quarter of it, for
identical work producing an identical cache.

Nothing about taiyaku changed between those two numbers. What changed was how
quickly something else answered. That is the case for the fix stated more
sharply than the 30 s row states it: the pre-bind wait was never taiyaku's
duration to predict, it was an unbounded dependency on a remote service's mood,
and the only reliable way to stop paying it at startup is to not pay it at
startup.

### The equivalence check ran against the changed code

`stump_and_local_serve_the_same_bytes_for_the_same_member` skipped during the
branch's own gate, `TAIYAKU_STUMP_URL` being unset. Run once the instance was
back: **187 pages of `en-mono` compared byte-for-byte between `LocalLibrary`
and the composite's Stump-backed path, and the same 8 members recorded absent**
— unchanged from M3's figures, against a `CompositeSource` whose resolutions
now arrive after construction rather than before it. The one check that could
have noticed the map going stale or arriving empty says it did not.

### What is not claimed

**The ordering inside `serve` is not under test.** `serve` binds a socket and
no test in this crate opens one — that is deliberate and predates this change.
What the two tests hold is the mechanism the ordering depends on: that a
`CompositeSource` built with nothing resolved serves from disk and switches to
Stump once resolutions land, and that `resolve_all_works` reports each work as
it finishes. The four lines of ordering in `serve` are verified by reading them
and by the stderr transcript above.

Both tests were checked by sabotage rather than by having once failed to
compile, which is a weaker red than it looks: making `learn` a no-op makes the
first test serve `a1.jpg` where the Stump bytes were required, and accumulating
instead of reporting per work collapses the second's two batches into one.

## M5a Task 2: the filmstrip's tile cache, measured against the real corpus

O11's "what would reopen it" named a second consumer of `.taiyaku-cache`: "the
filmstrip's thumbnail-per-page route, deferred to M5 in `cover.rs`, would
write 195 files per volume rather than one and changes the arithmetic
entirely." That route now exists (`GET
/api/works/{work}/{edition}/pages/{index}/thumb`), so the number it changes
the arithmetic to is measurable rather than projected.

Run against the real corpus, from the repo root so the cache lands beside the
one the O6/O9/O11 measurements already used:

```
cargo build --release -p taiyaku-cli
./target/release/taiyaku serve corpus/gotoubun-v01/taiyaku.json --addr 127.0.0.1:18099 &
for i in $(seq 0 194); do curl -s -o /dev/null "http://127.0.0.1:18099/api/works/taiyaku/raw-mono/pages/$i/thumb"; done
find .taiyaku-cache/thumbs -type f -name "*.jpg" | wc -l
find .taiyaku-cache/thumbs -type f -name "*.jpg" -exec stat -c%s {} \; | \
  awk '{sum+=$1; n++} END {print n, sum}'
```

The work id, `taiyaku`, came from `GET /api/works` rather than being assumed —
the sidecar's filename stem (`taiyaku.json`) slugified, per
`works.rs::work_id`. The edition fetched was `raw-mono`, the sidecar's hub: one
member, 195 pages, no spreads, so `seq 0 194` is every page and each tile is
keyed by a distinct offset (`p0` through `p194`) against that one member's
`sha256`, confirmed by inspecting the resulting filenames.

**195 files, 1,177,982 bytes — about 1.12 MiB, `du` rounds it to 1.6M against
its 4 KiB block size.** Individual tiles ran from 1,117 to 8,808 bytes, mean
6,041 — small enough that a cold filmstrip decoding all 195 at once, bounded by
the same `MAX_CONCURRENT_GENERATIONS = 2` a cold OPDS grid uses, is not a
capacity concern on disk even before caching amortises it to zero on every
later visit.

**Unremarkable, which is the answer O11 asked for.** The cover half of
`.taiyaku-cache` is 165 KB across 3 files; one edition's tiles are roughly
seven times that in bytes for sixty-five times the file count — proportionate
to 195 tiles against 1 cover, not the kind of blowup that would make "the
filmstrip's route changes the arithmetic entirely" true in practice. A hundred
works, one edition's tiles apiece, reaches on the order of 115 MB — well short
of anything that would make a size-based eviction policy urgent. The
`.taiyaku-cache/covers` directory was untouched by this run — still the same
3 files this run started with, 168,806 bytes exactly. `du -sh` reports 176 KB
for that directory, not 165 KB: `du`'s default is block-rounded disk usage,
not a byte sum, so each file rounds up to its filesystem's 4 KiB boundary
(54,199 → 56 KiB, 60,064 → 60 KiB, 54,543 → 56 KiB, summing to 172 KiB) and
the directory's own block adds a fourth 4 KiB, landing on 176 KiB. 165 KB is
the exact byte sum, 168,806 bytes, rounded to the nearest KiB — the number
`docs/status.md`'s O11 entry gives. Same three files, two honest ways of
counting them, no growth. Tiles live in the sibling `.taiyaku-cache/thumbs`
exactly as designed, so this measurement adds a new number rather than
revising the old one.

**What would reopen it, updated:** the same condition O11 already named for
covers — archives replaced routinely enough for the per-file cost to
accumulate — now applies at the tile count's larger constant factor, and a
library holding many editions' worth of warm filmstrips at once is the case
this single run cannot speak to, since only one edition was fetched here.

## M5a, measured directly: the reader's own evidence, not the scratch ledger's

M5a's other measurements were taken during the branch and recorded as they
were made, in a working ledger under `.superpowers/`, which `.gitignore`
excludes from every commit. That ledger does not merge with the branch, so it
does not exist here to point at — the figures below are transcribed instead,
the actual numbers recorded during the work, not new ones and not estimates.
The filmstrip's tile cache, above, is the one M5a measurement that already had
a section of its own; this one carries the rest.

### A swap writes nothing; only navigation does

Swapping between editions produces **zero** `PUT /progress` requests. Only
`navigate` writes a position — a swap has nothing to record, so nothing
changes on disk no matter how many editions a reader cycles through in one
sitting. This is the round-trip-safety property the authority model exists
for, measured rather than assumed.

### The zero-based slot survived contact with the real edition

`en-mono` has 187 pages and 194 slots. Writing progress at its last slot sends
body `{"edition":"en-mono","slot":193}` — zero-based — and the server answers
**204**. A 1-based leak anywhere on this path would have sent `194` and met a
404, the same shape of bug the OPDS feed's own off-by-one cost a whole Panels
session (see "Panels, measured: the feed was one-based and should not have
been," above); it did not recur here.

### One `PUT` for twenty keystrokes, and the page-hidden flush works

A twenty-keypress navigation burst produces exactly **one** `PUT /progress` —
the debounce coalesces the run rather than writing on every keystroke. A
page-hidden event fired inside the debounce window flushes it immediately:
one `PUT` with body `{"edition":"raw-mono","slot":2}`, answered 204, with no
duplicate arriving 600 ms later when the debounce timer would otherwise have
fired on its own.

### Coalescing spreads takes the DOM from 194 to 187

`en-mono` has 7 joined spreads across 194 slots. With every reading mode
drawing a joined spread once rather than as two halves, the DOM holds exactly
**187** `<img>` elements — 194 minus 7, confirmed rather than assumed to be
drawn once.

### The repagination the aligner exists to resolve, seen from the reader

Swapping from `raw-mono` slot 20 (Japanese page 20) lands on `en-fan` page
58 — the same panels, a different page number, which is exactly the
repagination problem the aligner exists to solve rather than a defect in the
reader. One `Space` press returns to page 20.

### A genuine gap names itself, and the toggle back is byte-identical

`raw-mono` slot 1 is genuinely unpaired in `en-mono` — a bare `{"a":"1"}` gap
run in the sidecar, not an artefact of the reader. The reader names the gap
rather than silently landing on the nearest slot, and offers the nearest
paired slots either side as moves the reader takes deliberately. Toggling
back from the gap returns byte-identically to `raw-mono/pages/1`.

### The chrome overlays the page; it does not reflow it

Showing or hiding the chrome (filmstrip and slot readout) leaves the page
image's bounding rectangle identical before and after —
`{x: 684.328125, y: 37.015625, width: 197.328125, height: 294.96875}` in both
states — because the chrome is `position: absolute` and out of the layout
flow. The page does not shift under the reader when `Tab` opens or closes the
strip.

### The compiled module ran, for the first time, under a JS runtime

Every measurement above, and `taiyaku-wasm`'s own test suite, runs the
binding natively. The first actual execution of the compiled WebAssembly
module under any JS runtime happened during M5a, via
`scripts/smoke-wasm.mjs`: six checks passed, and the generated `.d.ts` matched
what the plan had assumed about the binding's shape. See `docs/status.md`'s
M4 entry, updated to record it.

## M5b Task 5: `chapter_at`, walked over every slot of the real corpus

`Sidecar::chapter_at` resolves a slot into whichever edition of a document
is chapter-organised — the one assembled from the most members — and reports
which member the resulting page falls in. `corpus/gotoubun-v01/taiyaku.json`
has exactly one such edition, `en-fan` (seven members, `c000.cbz` through
`c005.5.cbz`), so a throwaway test walked all 195 slots of `raw-mono` (the
hub, no spreads, so slots and pages coincide) and called `chapter_at("raw-mono",
slot)` for each, tallying where it landed. This is the finding, not a
confirmation of anything decided beforehand:

| Member | Path | Recorded pages | `raw-mono` slots landing here |
| -----: | ---- | --------------: | -----------------------------: |
| 0 | `c000.cbz` | 44 | 2 |
| 1 | `c001.cbz` | 52 | 53 |
| 2 | `c002.cbz` | 41 | 42 |
| 3 | `c003.cbz` | 32 | 32 |
| 4 | `c004.cbz` | 30 | 30 |
| 5 | `c005.cbz` | 27 | 26 |
| 6 | `c005.5.cbz` | 9 | 5 |
| — | (no counterpart) | — | 5 |

195 slots in total, 190 landing in a member and 5 landing nowhere — exactly
the `raw-mono → en-fan` quality block's `paired: 190` of `slots: 195`. The
table above shows all seven members reached, not only `c001`–`c005.5`:
`chapter_at` computes membership, not a chapter count, and it does not stop
at whichever file a volume-boundary guess would exclude.

**What was surprising:** `c000.cbz` — the file whose name most directly
invites reading it as chapter zero — is the member `raw-mono` lands in
*least*, by a wide margin (2 slots against `c001.cbz`'s 53). Reading that as
"volume 1 excludes c000" would be exactly the filename-arithmetic move this
function exists to avoid; what it actually shows is only that `raw-mono`'s
own page range mostly does not land there. `c000.cbz` is still reached
(twice), and this function draws no boundary and names no chapter number —
it reports membership for a slot, one call at a time.

**This is the only chapter-organised edition in the corpus, and it is
reached by a `fragmented` alignment.** `raw-mono → en-fan` carries
`"verdict": "fragmented"` in the sidecar, average run 23.75 slots over 190 of
195 paired slots — a different regime from `raw-mono → en-mono`'s `sound`
(avg run 64.33). Every `ChapterAt` this corpus can produce today rides on
that fragmented pairing, so it is no more certain than the pair it came
from. A later readout (Task 7) must carry that verdict alongside the
chapter it reports rather than presenting a chapter as settled fact — the
same lesson the twenty-two needles taught: a number that looks clean on its
own is not proof of anything without the measurement behind it stated too.

## M5b Task 9: the flicker measured, and O1's two behaviours

Both findings below were taken against `corpus/gotoubun-v01`, `raw-mono` and
`en-fan`, via Playwright driving a real Chromium against `taiyaku serve
--release` behind the Vite dev server — not a unit test, since the thing
being measured (does the browser present a half-decoded frame; does the
browser send a conditional request) only exists at that layer.

### The flicker, before and after: a screenshot cannot catch it, `img.complete` can

A blank or half-painted frame is a gap smaller than the time between two
Playwright screenshots, so a screenshot is the wrong instrument regardless of
how many are taken. What can catch it is polling the one on-screen `<img>`'s
own `complete`/`naturalWidth` on every animation frame while the reader
flicks, and counting how many samples land mid-load — a
`requestAnimationFrame` loop pushing `{complete, naturalWidth, src}` once per
frame, run alongside twenty synthetic `ArrowRight` keydowns dispatched 40ms
apart (the module's own toggle, `taiyaku.prefs.prefetch`'s `enabled`, is what
makes disabled/enabled a comparison rather than a memory — see `App.tsx`).

**Module disabled** (`PageView` reverted to a single `<img src>` with no
decode gate): slots 0→20, 79 animation-frame samples, **3 landed mid-load**
— `complete: false, naturalWidth: 0` while the visible `src` had already
become `pages/1`, `pages/3` and `pages/19` respectively, meaning the browser
had already swapped the attribute and painted nothing yet.

**Module enabled** (defaults — `ahead: 2, behind: 1, maxConcurrent: 3`):
slots 20→40, 83 samples, **0 landed mid-load**.

This is a local dev server proxying to `taiyaku serve` on the same machine —
each page fetch settles in 3–5ms (see the resource-timing entries in the O1
section below), far faster than any deployed network would be. That
understates the defect being measured, not overstates it: on a slower
connection the window a naive same-element `<img src>` swap leaves blank is
proportionally larger, so 3 of 79 here is closer to a floor than a ceiling on
what the pre-M5b code would show in the field. The fix does not depend on the
fetch being fast either way — `presentWhenDecoded` (`decode.ts`) never
touches the visible `<img>`'s `src` until `image.decode()` has resolved on an
off-screen probe, so there is no window at all for the browser to have
nothing decoded to paint, at any network speed.

**The decode cap held under a harder stress than the flicker test needed.**
`HTMLImageElement.prototype.decode` was patched to count concurrent calls,
then twenty `ArrowRight`s were dispatched 8ms apart — faster than the ~4ms a
local decode takes to settle, specifically to try to force overlap. 80
`decode()` calls fired in total (one per swap plus up to three per prefetch
window recompute, since both draw from one shared `DecodeQueue` — see
`App.tsx`'s `decodeQueue` doc); **peak concurrency was 3**, exactly
`prefetchModule.defaults.maxConcurrent`, never higher across the run.

### The two-page fix, measured separately: the single-page number above never exercised it

The flicker figures above (3 of 79, 0 of 83) were taken in single-page mode,
which is exactly the mode `7c3b821`'s review finding did not touch — single
mode always renders a bare `PageView` at its one group position, in every
mode and every group shape, so the element *type* React sees there never
changes and the remount defect that commit fixed could not have fired.
Two-page mode used to differ: an ordinary pair rendered a `<div>` wrapping
two `PageView`s, but a joined spread or an unpaired solo slot rendered a bare
`PageView` directly at the same tree position, so a turn crossing into or
out of a spread flipped the element type React saw there. React remounts
rather than updates props across a type change, and a freshly-mounted
`PageView`'s `useState(src)` initialises to the incoming, undecoded source —
the exact defect the decode gate exists to close, reachable in two-page mode
on every turn onto or off a joined spread, regardless of what the single-page
number above says. `pairing.ts`'s `cellsFor` closes it by giving `App.tsx`
two fixed cells, always both present, so the same wrapping `<div>` and the
same two child `PageView`s render on every turn and only their `src`/layout
props change.

**Measured by reverting the fix at its one call site, not by editing
`cellsFor` itself.** `cellsFor` is a pure formatter with its own direct
tests (`pairing.test.ts`); the defect lived entirely in `App.tsx`'s
`renderGroup` choosing what to render from a group's shape, so `renderGroup`'s
two-page branch was replaced with its pre-`7c3b821` form — a bare `PageView`
for `group.spread || group.slots.length === 1`, a `<div>` wrapping two
`PageView`s otherwise, both still wired through the current `swapQueue` (the
queue split is a separate fix from this one and was left alone) — run live,
then replaced back with the committed version and confirmed against `git
diff` that the file matched `HEAD` exactly before continuing. `cellsFor`'s
import went unused for the span of the revert; `vite`'s dev server does not
type-check on save, so this did not block driving the browser, and no
`tsc`/`oxlint` run happened while the revert was in place.

**First attempt undersampled the opportunity, not the defect — corrected
before treating the null as a result.** The same instrument as the
single-page test (a `requestAnimationFrame` loop reading `complete`/
`naturalWidth` off every on-screen, non-collapsed `<img>`) run against a
20-slot flick from `en-mono` slot 0 — the same span the single-page test
used — crosses a group-shape boundary only twice (entering the page-4 spread,
then leaving it), against single mode's twenty opportunities (every turn is
one). Both the reverted and the fixed code read 0 incomplete samples on that
span (129 and 132 total on-screen image samples respectively) — not evidence
the defect was absent, only that two chances is too few to expect a ~3–5ms
window to land inside a ~16ms animation frame. The protocol was widened
instead of stopping at that null: a 60-`ArrowRight` flick from slot 0, still
40ms apart, crossing six group-shape boundaries — into and out of the page-4
spread, then the adjacent page-45/page-46 spreads, then the page-49 spread,
confirmed by watching the on-screen image count change from 2 to 1 and back
at presses 3, 5, 45, 49, 51, 53, identical in both runs.

**Reverted (bug reproduced): 294 total on-screen image samples across the
60-press run, 1 landed mid-load** — `complete: false, naturalWidth: 0` on
`.../en-mono/pages/49`, the page-49 spread itself, one of the seven spread
pages `en-mono` is known to have and immediately downstream of the shape
crossing at press 51 above. **Fixed (`HEAD`, committed): 306 total
on-screen image samples across the identical press sequence and the same six
crossings, 0 landed mid-load.** Both runs otherwise walked the identical
script — same slot path, same timing, same corpus, same fresh page load with
`localStorage` cleared and no stored progress — so the only variable between
them is the one call site under test. One incomplete sample against six real
crossing opportunities (roughly the single-page test's own per-sample rate
of a few percent, applied to a much smaller number of trials) is consistent
with the same fast-local-server floor the single-page section above already
names: 3–5ms is a lower bound on the window a slower connection would widen,
so a real deployment crossing a spread is not expected to show this defect
less often than this measurement did.

### O1, counted: two different real behaviours, and only one is about the ETag

The literal scenario the brief specified — swap editions repeatedly on one
slot — was run first, exactly as asked: 24 `Space` presses, 100ms apart, one
fixed slot, from a fresh page load. A `PerformanceObserver` watching
`resource` entries live (not a query taken after the fact — see below for why
that distinction turned out to matter) recorded **four** `/pages/` requests
across the whole run, all prefetch neighbours, **none for either of the two
centre URLs being toggled between**, despite each being redisplayed twelve
times. That is genuinely zero requests, not zero `304`s — the exact case this
task was warned to watch for: "if you observe no requests at all on a
re-swap, the 304 count is zero for a reason that has nothing to do with O1."
It has nothing to do with O1 here either. A live `MutationObserver` confirmed
the DOM `src` attribute really did change on every single press (ten
presses, ten distinct attribute mutations, correctly alternating) — so this
is not React skipping an unchanged prop. What is happening is one level
below React and below HTTP: a source the browser decoded a moment ago and is
asked to redisplay is repainted from its own short-lived in-memory image
cache, which never reaches the network layer at all, not even to revalidate.
That is a *cheaper* outcome than a `304`, not a failure of the `ETag` — but
counting it as a `304` would have been wrong, and counting it as evidence
against the `ETag` would have been just as wrong; it is a third thing, an
even shorter circuit than the one this task shipped.

**Buffered querying (`performance.getEntriesByType('resource')` called once,
after the run, rather than a live `PerformanceObserver`) gave a materially
different count for the identical sequence** — it is what first surfaced
this finding, before the live observer confirmed it was real rather than an
instrumentation artefact: a single buffered query after 20 rapid presses
reported only 3 entries where a live observer over the same kind of run
reported the true count. Both undercounted the same way relative to a naive
"one request per press" expectation, for the reason above, but the buffered
query is the less trustworthy of the two — a live observer is what every
number in this section is actually built on.

**A session with real separation between visits tells the other half of the
story — and, on review, turned out to be telling two different stories at
once.** Ten slots read forward from `raw-mono` 120 (`Space` to `en-fan` and
back at each one before moving on), then the same ten slots read backward
with the same swap at each, produced 39 `/pages/` requests.
`PerformanceResourceTiming.transferSize` sorts them cleanly: a full page in
this corpus runs 150–850 KB, a headers-only revalidation runs almost exactly
300 — a different thing from the literal scenario above's zero-byte,
zero-request repaint, and `transferSize` is what tells the two apart. **22
were full `200`s, 17 were `304`-sized.**
That forward-then-backward shape is a revisit generator, not reading: nobody
re-reads the ten pages they just read, backwards, immediately after reading
them forward. All twelve requests from the backward pass were 304-sized —
every slot revisited after moving away came back as a revalidation, none as
a silent zero-request repaint — and those twelve are the genuine number:
real revisits, of pages the reader had actually left and come back to.
**The other five 304s are not reader traffic.** The forward pass alone
produced 22 full `200`s and 5 of its own 304s, and those five are
self-inflicted: the prefetch window warms a neighbouring slot by building a
throwaway `Image()` — the same default builder `decode.ts` documents,
`() => new Image()`, garbage once its `decode()` promise settles — and
requesting it before the reader arrives. When the reader's own swap reaches
that slot moments later, `PageView`'s decode gate builds a *second*,
on-screen `Image()` for the same URL and requests it again; the browser
answers that second request with a 304 because the first one already put the
`ETag` in its cache. Neither request would exist without this task's own
prefetch module running, so against a no-prefetch baseline the validator
saved nothing on these five — it neutralised traffic this milestone
introduced, not traffic a reader brought to the server. **The honest split
of the seventeen is twelve genuine reader revisits and five self-inflicted
duplicates.** This is also one scripted session — one browser, one corpus,
one machine, no repetition — against the real recorded session that already
exists for a different client: Panels, 177 requests, answering the
equivalent question at zero. Nothing here is evidence of what a real reading
session does with the browser reader, only of what a script built
specifically to force revisits does.

**The 304 was cross-checked against the wire once, because `transferSize`
and `PerformanceResourceTiming.responseStatus` disagreed and only one of
them can be right — and that check covers one response, not seventeen.**
`responseStatus` reported `200` for a response `transferSize` said carried
no body — Resource Timing's own field normalises a validated cache hit to
the status the *page* effectively got, not the status the *wire* carried,
which is why `responseStatus` was tried first and abandoned as a classifier
here, a genuinely useful trap to have on record. The CDP-level network log
(`taiyaku serve` itself, via Playwright) reported the literal wire text for
that response: `304 Not Modified`. **That check was run as an isolated
repeat — a different request, in a different run, from the 39-request
session above** — not one of the seventeen `304`s counted there. Sixteen of
the seventeen — everything but that one indirect check — rest on
`transferSize` alone, never confirmed against the wire.

**Also verified directly, not assumed:** the brief's expectation was that a
`200` carries neither `Cache-Control` nor `Last-Modified`, so heuristic
freshness is zero and a reuse must revalidate rather than serve silently.
`fetch('/api/works/taiyaku/raw-mono/pages/5', { cache: 'no-store' })` against
a page never requested before returned exactly `{connection, content-length,
content-type, date, etag, vary}` — confirmed, not assumed, and it is the
reason the 300-byte responses above are read as revalidations rather than
some other kind of small reply.
**Neither header has landed since**, so this still holds for all seventeen,
genuine and self-inflicted alike: heuristic freshness is zero, and every
reuse still has to ask the network before it can serve anything — never
silently stale, but never free of the round trip either. The validator turns
a 150–850 KB response into about 300 bytes; it does not turn a round trip
into no round trip. Prefetch exists to hide that round trip before the
reader notices it, and a request that still has to go out and come back,
just smaller, is a request the reader still waits on — on a slow connection
this design saves body bytes and no latency.

**`loading="lazy"` still does not defer, confirmed again on `PageView`
itself rather than only on `Filmstrip`'s tiles.** Entering scroll mode
mounted all 195 of `raw-mono`'s `<img loading="lazy">` elements at once;
500ms later every one of the 195 had `complete: true, naturalWidth > 0`.
`App.tsx` does not lean on the attribute to bound anything it controls —
`buffered` is false in scroll mode specifically so the decode gate does not
force-eager what `loading="lazy"` was meant to (and, per this and the
earlier `Filmstrip` measurement, does not) defer — but the attribute itself
still does nothing observable in this environment.

## M5b Task 11: the settings panel, generated, and all five modules measured switching off

`Settings.tsx` is generated from `settingsPanel.ts`'s pure `fieldsOf`
(runtime type of a `defaults` value → `boolean`/`number`/`string`/`object`)
and `withFieldValue` (coerce a control's raw change back into the right
type, or refuse). Neither function names a module id anywhere — confirmed
by reading, not just by design intent, since that is the property the whole
task turns on. `modules/settings.ts` adds a sixth registry entry —
`settingsModule`, `defaults: {}`, one key (`s`) — so the registry `App.tsx`
builds has six entries, but `fieldsOf({})` returns no fields, and
`Settings.tsx` skips a section with zero fields rather than rendering an
empty one. Opened in a real browser: five sections render — Reading
direction, Motion, Regroup pages, Readouts, Prefetch — not six, confirmed by
counting `<h3>` elements in the panel.

### Every module measured switching off, tuning, and persisting — none left a remnant

Driven with Playwright against `corpus/gotoubun-v01` (`raw-mono`, 195
slots), `localStorage` cleared before each module's sequence, console
checked per navigation rather than cumulatively (the persistent-profile trap
this milestone's brief names twice). Each row is switch off → confirm the
behaviour stops, tune → confirm it takes effect, reload → confirm it
persisted, all three checked live rather than inferred from the stored
JSON alone:

| Module | Switch off | Tune | Persists | Remnant after off? |
| --- | --- | --- | --- | --- |
| `direction` | Reset clears `override` to `{}`; two-page order reverted `[pages/0,pages/1]` → `[pages/1,pages/0]` in the same render, no reload needed | `d` set `raw-mono` to `ltr`; pair order flipped `[pages/1,pages/0]` → `[pages/0,pages/1]` | `taiyaku.prefs.direction` read back `{"override":{"raw-mono":"ltr"}}` after a full reload, order still flipped | None |
| `regroup` | Reset clears `breaks` to `{}`; pairing at slot 3 reverted `(3,4)` → `(2,3)` immediately | `b` at slot 3 (0-based) split `(2,3)`/`(4,5)` into `(2)`/`(3,4)`; stored `{"breaks":{"raw-mono":[3]}}` | Pairing still `(3,4)` after reload | None |
| `readouts` | Unchecking `position` dropped the `slot N / M · p. N · P%` segment from the row on the next render, others unaffected | Same checkbox is the tune — no separate case | Row still missing the position segment after reload | None |
| `motion` | Unchecking `animate` removed `transition` **and** `opacity` from the readout row's inline style entirely — confirmed by reading `style` off the DOM, not just visually — matching `transitionStyle`'s "empty object, not `transition: none`" contract even when reached through the panel | `ms` set to `500` via the panel changed the style to `transition: opacity 500ms` | Style still carried no `transition` after reload with `animate` off | None |
| `prefetch` | Unchecking `enabled`, then navigating to a new slot, produced exactly one `/pages/` request (the slot itself) where the default window produces four; the `<img>`'s `loading` attribute reverted to `"lazy"`, its pre-M5b value | `ahead` set to `4` widened the request set at slot 5 from `{4,6}` to `{4,6,7,8,9}` (`window`'s own nearest-first order, confirmed against the network log) | Both `enabled=false→true` and `ahead=4` reproduced the same widened window, all `304`s, after reload | None |

Two things worth recording beyond the table:

**`maxConcurrent` needed a real code change to be live-tunable, not just a
panel.** Before this task, `App.tsx` built `swapQueue`/`prefetchQueue` once,
via a lazy `useRef`, sized from `prefetchPrefs.maxConcurrent` read at that
one moment — exactly the "load-once until a settings panel exists" every
prefetch field's old comment said. A panel that could change the number
without the number doing anything would be a broken control of the kind the
brief warns against, so the two queues are now `useState` rebuilt in a
`useEffect` keyed on `maxConcurrent`. Measured live: set to `1` mid-session,
then three `ArrowRight` presses in a row — no console error, no stuck frame,
the displayed page tracked the slot on every press. Not measured: the exact
serialisation a cap of `1` should impose on decode timing: the check here is
that the new value took effect and nothing broke, not a timing trace.

**Prefetch's decode cap does not bound the filmstrip's tile burst, and the
`loading="lazy"` finding above still stands unqualified.** `Filmstrip.tsx`
renders its thumbnails as plain `<img loading="lazy">`, imports nothing from
`decode.ts`, and receives no `DecodeQueue` — `swapQueue`/`prefetchQueue` are
only ever passed to `PageView`. So a cold filmstrip's thumbnail burst (one
request per distinct page, up to `groups.length`) is exactly as unbounded
after this task as before it; `maxConcurrent` governs page decodes, not
thumbnail requests, and nothing in this task changed that. This is a
code-reading fact (`Filmstrip.tsx` has no queue prop to plumb one into), not
a new timing measurement, and it means M5a's finding stands: `loading="lazy"`
still does nothing observable in this environment, and nothing in M5b
addresses the tile burst it was meant to bound.

### The nested per-edition preference shapes

`direction.override` and `regroup.breaks` are both `Record<string, …>`,
keyed by an edition string the panel has no independent way to know is
"an edition" rather than any other string key — `settingsPanel.ts`'s
`fieldKind` reports these as `'object'`, a fourth kind alongside
boolean/number/string, and `Settings.tsx` renders a read-only entry count
(`nestedSummary`, e.g. `"1 set"`) rather than a per-key editor. The
section's existing Reset button already clears the whole slice, `override`
and `breaks` included, so a reader who wants the nested value gone has an
affordance; a reader who wants to edit one entry without clearing the rest
still needs the module's own key (`d`, `b`), same as before this task. A
per-edition editor was considered and rejected: it would need the panel to
know what "edition" means for a specific module's `Record`, which is exactly
the per-module knowledge the generated panel exists not to carry — the
brief's own warning against "a branch per module id" applies as much to a
branch keyed on a field's *name* as to one keyed on a module's id.

## Whole-branch review before merge: making "generated" true of the wiring, not only the panel

A review of the whole `reader-m5b` branch, done once every task's own work
was in, found that Task 11's "generated rather than hand-written per module"
framing — repeated in the plan, in `Settings.tsx`'s own doc, and in
`status.md`'s closing paragraph above — was accurate about `Settings.tsx`
and false about the system a module's preferences actually travel through.
`Settings.tsx`'s doc comment ("a seventh module needs no edit to this file")
checked out on inspection: `settingsPanel.ts`'s `fieldsOf`/`withFieldValue`
never branch on a module id or key name, confirmed by reading both files.
`App.tsx`, though, still hand-declared one `useState`+`loadPrefs` pair and
one `useEffect`+`savePrefs` pair per module, and hand-wrote a case per
module in each of `handleSettingChange`'s and `handleSettingReset`'s
switches — five near-identical blocks and two five-armed switches for the
five real modules. A seventh module (`fit`, `filters`, both still deferred)
would have rendered a normal-looking, generated control that changed
nothing until someone also copied all four of those by hand — the exact
"generated but broken input" failure this same milestone already found and
fixed once, for `prefetch.maxConcurrent`, moved one layer up into code
`App.tsx`'s permanent Vitest-unreachability (it value-imports `resolver.ts`)
means only a manual browser pass could ever have caught.

**Fix chosen: make the claim true, not narrower.** `App.tsx`'s preference
state is now one map, `Record<string, object>` keyed by module id, built
once from `registry.all()` via `loadPrefs` and persisted by one effect that
saves every registered module's slice on any change. `module.ts` gained
`sliceFor(map, mod)`, which restores one module's own typed slice out of
that map — the same fallback-to-`defaults` rule `Settings.tsx`'s own
`modulePrefs` lookup already applied at the panel boundary, now applied at
every other read site too. `handleSettingChange`/`handleSettingReset` are
now a `registry.get(id)` lookup plus `withFieldValue`/a defaults-copy,
generic over every module, not a switch. The registry itself — the
`modules` array and the `buildRegistry` call — moved out of `App.tsx` into
a new `web/src/modules/registry.ts`, so registering a module's *existence*
no longer touches `App.tsx` either; the module's own key-bound behaviour
(`direction`'s `d`, `regroup`'s `b`) and any bespoke wiring its behaviour
needs (`prefetch`'s decode-queue rebuild) are the only things that still do,
and those are behaviour, not tunability, so they were left alone.

**Proved with a throwaway seventh module, not just argued.** A
`throwawayModule` (`defaults: { enabled: true, level: 3 }`, no keys) was
added to `modules/registry.ts`'s array and nowhere else. Driven with
Playwright against `corpus/gotoubun-v01`, `localStorage` cleared first,
console checked per navigation: opening the panel (`s`) showed a
"Throwaway (proof)" section with `enabled`/`level` controls the moment the
module was registered — no other file touched. Setting `level` to `9`
through the panel wrote `{"enabled":true,"level":9}` to
`taiyaku.prefs.throwaway` immediately; a full reload (fresh navigation, not
HMR) showed `9` still in the panel's numeric input and still in storage.
Console stayed clean (0 errors, 0 warnings) across both loads. The module
and its registry entry were then deleted — this is not a shipped module,
only the measurement that the claim holds.

**All five real modules re-driven after the refactor, same method as Task
11's own table above, same result.** `localStorage` cleared, console
checked per navigation:

| Module | Switch off / tune | Persisted after reload |
| --- | --- | --- |
| `direction` | `d` on `raw-mono` flipped the two-page order `[pages/1,pages/0]` → `[pages/0,pages/1]`; `taiyaku.prefs.direction` updated to `{"override":{"raw-mono":"ltr"}}` in the same tick | Order still `[pages/0,pages/1]` after reload |
| `regroup` | `b` at slot 3 split `(2,3)` into `(2)`/`(3,4)`, live; `taiyaku.prefs.regroup` updated to `{"breaks":{"raw-mono":[3]}}` in the same tick | Grouping still `(3,4)` after reload |
| `motion` | Unchecking `animate` in the panel wrote `{"animate":false,"ms":120}` immediately | Same value read back after reload |
| `readouts` | Unchecking `position` wrote `{"position":false,...}` immediately; readout row read `1m read · no chapter edition` (no `slot N / M` segment) | Row still missing the segment, and storage still `position:false`, after reload |
| `prefetch` | Unchecking `enabled` wrote `{"enabled":false,...}` immediately | After reload, the mounted `<img>`'s `loading` attribute read `"lazy"` — `prefetchPrefs.enabled` still gates `buffered` correctly through the new map — and storage still read `enabled:false` |

No remnant, no console error, on any of the five or on the throwaway
module, before or after the refactor. Full web suite: 242 → 248 (six new
tests — three for `sliceFor`, one exercising the generic
change/reset shape against a synthetic module, two for the moved
registry), all passing; `oxlint --deny-warnings`, `tsc -b`, and `vite build`
all clean.

**The other stale claim this same review fixed:** `modules/prefetch.ts`'s
doc comment still said, at the top, that no settings panel existed yet and
that `enabled` was toggled "by hand-editing `taiyaku.prefs.prefetch` in
storage" — true before Task 11 shipped the panel, false since, and never
corrected because Task 11's own diff never touched this file's comment.
`motion.ts` and `readouts.ts`, named in the same original sentence, carried
no equivalent stale claim. The sentence was deleted; the paragraph's real
content — why `enabled` is distinct from both `ahead`/`behind` being
zero — is unchanged.

## M5d: the correction arithmetic, measured before it was built

`docs/superpowers/plans/2026-08-04-m5d-correction-ui.md`'s arithmetic was
written into the tree, run against the real fixtures, and reverted before the
plan itself was written — a discipline recorded there as "four things a plan
asserting rather than running would have got wrong". All four were reproduced
against `crates/taiyaku-core/src/sidecar.rs`'s `REAL` fixture (Go-Toubun v01:
`raw-mono` 195 slots, `en-mono` 194 slots) and a three-edition star fixture
built for the purpose, and the shipped tests pin the same invariants at
smaller scale.

### A point correction is not 1:1 unless the target's previous owner gives it up

Correcting `raw` slot 100 from `en` slot 99 to `en` slot 98 left `en` slot 98
claimed by *both* `raw` 99 and `raw` 100 — a run list `invert` would turn into
two runs starting at the same slot, and `counterpart` would answer from
whichever came first, silently. Measured on the `REAL` fixture: **193
targets, 192 distinct.** With eviction — the previous owner of the target
gives it up and becomes a gap — **192 targets, 192 distinct.** `correct_runs`
(`taiyaku-core/src/lib.rs`) implements the eviction; the unit test
`a_corrected_target_is_taken_from_whoever_held_it` pins the invariant at unit
scale (a synthetic 10-slot run), and sabotaging the eviction into a no-op
fails it with "no target slot may be claimed twice" (left: 10 claimed targets,
right: 9 distinct).

### Marking hand runs on any overlapping slot marks 188 of 195 slots — and why `all` is the rule

`Sidecar::correct` has to decide which runs in the freshly-rewritten alignment
count as `by: "hand"` once a correction lands. The candidate rule was "any
slot of the run overlaps the correction"; on the primary corpus volume the
solved run `7-194` is 188 slots wide, and one correction landing inside it
would, under `any`, stamp all 188 `by: "hand"`. That would make O13's
replace-refusal (below) fire on every later re-solve of an edge somebody had
ever touched once, for 187 slots nobody corrected.
The rule is instead **every slot of the run the correction produced**: a
correction that changes the answer always lands as its own single-slot run
once `correct_runs` splits around it, so `all` — every slot of that
now-single-slot run overlaps the correction — marks exactly the slot asserted
and the slot displaced by it. Measured with two corrections against the
`REAL` fixture: the resulting hand runs are `["42", "50"]`, not `["7-41",
"42", "43-49", "50", "51-194"]`.

### A correction that changes nothing must not rewrite anything

`raw` slot 100 already resolves to `en` page 94 in the `REAL` fixture.
Without a no-op check, asserting that same pairing again still stamped `by:
"hand"` and a `note` onto the 188-slot solver run it happened to sit inside —
turning a no-op into damage. `Sidecar::correct` now answers
`Corrected::AlreadyTrue` and changes nothing at all when the assertion
already holds; `a_correction_that_changes_nothing_rewrites_nothing` pins it by
cloning the document before the call and asserting equality after.

## Correcting against the stored direction, without inverting

A sidecar stores one edge per spoke, in one direction, and `invert` reverses
it losslessly for *reading* — the topology work established that
("The solver is symmetric on the pairing relation", above). Writing is
different: correcting `en-mono` slot 10 against a document whose stored edge
is `raw-mono -> en-mono` (the reverse direction) must not go through
`invert`, apply the correction, then `invert` back, because `invert` drops
one-sided gap runs by construction — a round trip through it would silently
discard every gap the stored direction records.
Measured on the `REAL` fixture, which stores 2 gap runs (slots 1 and 6): the
`en-mono -> raw-mono` correction is applied **in the stored direction**,
leaving `raw-mono -> en-mono` in place with its orientation unchanged, and its
gap count goes from 2 to 3 — the original two gaps intact, plus one for the
slot the correction displaced.
`correcting_against_the_stored_direction_keeps_the_stored_gaps` pins the gap
count and that the edge's `from` stays `raw-mono` after a correction issued
against `en-mono`.

## O13 — closes here

`merge_spoke` dropped the stored edge wholesale on a re-solve, so a
hand-corrected run could not survive one. Nothing had noticed because
`Source::Hand` was constructed nowhere in the workspace — the variant was
defined, serde round-tripped it, and no code path wrote one — until the
correction UI became the first thing to construct one.

The fix divides exactly as the design tabled it:

| Merge case | Behaviour |
| ---------- | --------- |
| Spoke's members unchanged | hand runs carry across; the merge reports how many, as `Merged::CarriedHandRuns(n)` |
| Spoke replaced, hand runs present | refused, naming the spoke and how many corrections are at risk |
| Spoke replaced, no hand runs | proceeds exactly as before |

Three things found while building it, each worth a sentence:

1. **The first `carry_hand_runs` dropped a solved run that merely *overlapped*
   a hand run rather than splitting around it.** It used to filter whole
   solved runs by page-range overlap against the hand run's span. Run against
   a real test — two one-slot corrections at slots 5 and 9 inside a single
   30-slot solved run — it discarded 28 of the 30 slots: every slot but the
   two corrected ones, which keep their counterpart through their own
   hand-run entries. `carry_hand_runs`' own doc comment enumerates them as
   slots 0-4, 6-8 and 10-29. `resolve` answered `None` for slots 4, 7 and
   10, none of which any correction touched.
   `sidecar.md` promises only the corrected slots move, so the fix needed the
   slot tables, to split each overlapping solved run around every hand run it
   contains rather than discard the whole run.
2. **The spoke-changed test compared whole `Member` structs, not hashes**, so
   a renamed-but-identical archive read as "replaced" and would have
   discarded its hand runs on a merge that changed nothing about the content.
   Now compares the ordered `sha256` sequence, the idiom the from-side
   guard's own comment already states: "Content, not paths… only the hashes
   can be compared."
3. **The orientation assumption was reachable and silent.** `--direct` plus
   `Sidecar::correct`'s own direct-edge branch lets a pair be re-merged with
   the names swapped relative to the stored edge; without a guard the merge
   *succeeded*, reporting `CarriedHandRuns(2)` while misfiling the correction
   against the wrong edition's slot table.
   `a_direct_edge_stored_the_other_way_round_refuses_rather_than_misapplying_hand_runs`
   pins the refusal.

Tests: `a_hand_run_survives_a_re_solve_of_an_unchanged_spoke`,
`replacing_a_spoke_that_holds_hand_runs_is_refused`, and
`re_solving_a_spoke_whose_archive_moved_carries_hand_runs_rather_than_refusing`
(the sha256-not-struct case, finding 2 above) join the orientation-guard test
(finding 3) in pinning the three merge cases in the table.

What stays open, deliberately: what should become of a correction addressing
bytes that no longer exist. `status.md` already leaves that open and this does
not answer it — refusing is what stops it being answered silently.

## O23 — opens here

**Two hand corrections take Go-Toubun v01 from `sound` to `fragmented`.**

Measured on the primary corpus volume, correcting `raw-mono` against
`en-mono`:

| Corrections | paired slots | paired runs | avg run | verdict |
| ----------- | -----------: | ----------: | ------: | ------- |
| 0 | 193 | 3 | 64.33 | `sound` |
| 1 | 192 | 6 | 32.00 | `sound` |
| 2 | 191 | 9 | 21.22 | **`fragmented`** |

Each correction adds three paired runs and strands one slot, because the 1:1
rule means the asserted slot takes its target from whoever held it and that
slot becomes a gap. One paired run becomes four — the part before the
corrected slot, the corrected slot alone, the part between it and the slot it
displaced, and the part after — which is where the 3 → 6 → 9 in the table
above comes from. `correcting_recomputes_the_quality_beside_the_runs` pins
the two intermediate verdicts (`Sound`, `Sound`) and the final one
(`Fragmented`) against these exact `avg_run` figures.

The threshold is 27.0, and it exists to separate needles (1.1–1.7 slots a run,
measured across twenty-two of them) from confirmed-good pairings (36.0–200.0).
A twice-corrected volume at 21.2 is neither, so the verdict misleads in a way
the threshold was never calibrated for.

**Nothing is changed here.** Altering the fragmentation policy on a guess is
what this project does not do — the same reasoning that kept O1 open on
evidence rather than shipping an `ETag` on plausibility.

**What would decide it:** a work corrected enough times in real reading to say
whether the verdict misleads a reader in practice, or a measurement showing
hand runs and solver runs should be counted differently.

Site: `sidecar.rs::rewrite` (which recomputes the quality block) and
`solve.rs::verdict` (which sets the threshold).

## O24 — a solver-written direct edge outlives the edition it addresses

Found by the M5d whole-branch review, and the half of it that got fixed is
worth stating first, because the two are easy to confuse.

`merge_spoke` refuses a merge that would replace a spoke holding hand-corrected
runs. The original check looked only at the edge *between the two editions being
merged*, which misses a direct edge from that spoke to a **third** edition. The
review reproduced it: a star `a`(hub)/`b`/`c`, a reader corrects `b` against `c`
— creating a direct `b -> c` edge with two hand runs, since the pair had no
stored edge — and then `b` is re-solved after a re-scan. The merge answered
`Ok(Replaced)`, said nothing about the `b -> c` corrections, and left them
addressing the old edition's slots:

```
before: b5 -> c Some(9),  b20 -> c Some(20)
merge : Ok(Replaced)
after : b5 -> c Some(9),  b20 -> c Some(20)
route b->c: Some(Direct)
```

`b20 -> a` is now `Some(22)`, so through the hub `b20` would reach `c18`. The
reader is sent to page 21 instead of 19, and `route` labels the answer `Direct`
— the strongest confidence this format has. With a *shorter* replacement the
failure inverts: `runs_of` errors, `route(b, c)` answers `None`, and by design
it does not fall through to the hub, so a working spoke-to-spoke lookup is
destroyed outright.

That case is fixed: the hand-run check now runs over every alignment mentioning
the spoke, not just the one edge.

**What is left open is the same edge written by the solver.** `align --direct`
records one, nothing marks it `by: "hand"`, and replacing either endpoint leaves
it stale in exactly the way above — with no hand runs to trigger the refusal.

It is not a defect this branch introduced. A direct edge has always needed a
deliberate `--direct`; what M5d changed is that a *hand* one can now be made by
clicking a filmstrip tile. This item is the older half, found by looking at the
new guard and asking what it does not cover — which is the same method that
turned up O19 from O15's scoping, and O20 from O19's fix.

**Why it has never been felt:** no document in the corpus has both a `--direct`
edge and a re-solve. The measurements that argued for direct edges existing at
all — editions furthest from the hub losing 3.5% to each other — were taken
without one ever being written to a file that was later merged into.

## A correction silently stripped every margin on the edge it touched

Found only by driving the reader against the corpus, not by any unit test or
review pass. `entries_from_runs` writes `margin: None` for every entry it
builds — it rebuilds entries from bare runs and has no confidence figure to
hand them — and `Sidecar::rewrite` sent the whole edge through it on every
correction, not only the runs the correction touched.

Measured on Go-Toubun v01: one correction took `raw-mono -> en-fan` from **8
margins to 0**, while the untouched `raw-mono -> en-mono` kept all 3. A margin
is the weakest per-page figure inside its run, recoverable only by re-solving
against the archives — which is the work `sidecar.md` says the document
exists to avoid.

Fixed (`0242c0f`): a margin carries forward only where both spans are
identical. Re-measured after the fix, on a stored edge: `raw-mono -> en-fan`
keeps **6 of 8**, the two dropped being exactly the runs the correction
reshaped (`0 -> 0` became `0 -> 70`; `8-57 -> 46-94` split around the
displaced slot). A run that split, merged or moved has no stored figure that
honestly applies to it any more; a run untouched by the correction keeps the
figure a real solve produced for it, rather than losing it to a rewrite that
never touched its slots.

This was invisible to every unit test and to the whole review chain; it took
driving the reader against real material to find it. Sabotage: reverting the
carry fails the new test with every margin `None`.

## M5d Task 8, step 5: the browser pass against corpus/gotoubun-v01

Driven for real — `taiyaku serve corpus/gotoubun-v01/taiyaku.json --addr
127.0.0.1:8080` behind the Vite dev server on 5173 — because `App.tsx` is
deliberately unreachable from Vitest and nothing else covers the wiring. All
seven items the brief asked for, confirmed:

1. **`c` opens correction mode and the strip shows the counterpart edition.**
   Banner: "Pick the en-fan page that matches raw-mono page 1." The strip
   switched to en-fan's 235 tiles (240 slots minus 5 joined spreads). `to` is
   `en-fan` because the sidecar holds three editions, so `orderedEditions`
   gives `[raw-mono, en-fan, en-mono]` and `initialPrevious` seeds one step
   ahead.
2. **The tile the alignment currently resolves to is marked, and agrees with
   the readout.** Exactly one dashed tile, and zero solid-white "current"
   tiles — `currentIndex: -1` while correcting, as designed.
3. **Picking a different tile names the slot that will be displaced.**
   Picking en-fan page 60 gave "raw-mono page 22 will lose its en-fan
   counterpart" — verified by hand against the stored runs: en-fan slot 59
   sits in `raw 8-57 -> en 46-94`, so it pairs with raw slot 21, page 22
   1-based. Picks at en-fan pages 5 and 20 correctly reported "pairs with
   nothing today", confirmed against `taiyaku lookup`, which agrees both have
   no counterpart.
4. **Confirming writes.** A composed pair (`en-fan -> en-mono`, no stored
   edge) wrote a new direct edge `by: "hand"` with runs `{a: "59", b: "80"}`
   and the displaced `{a: "123"}` — matching the banner's pages 60/81/124
   exactly. `route` then reported `Direct` where it had reported
   `Hub("raw-mono")`. The reader landed on the picked page (readout "slot
   85 / 194 · p. 81").
5. **The correction survives a reload.** After a full page load the direct
   edge and both hand runs were still there.
6. **`Escape` writes nothing.** Banner cleared, `git diff` on the sidecar
   stayed empty.
7. **A conflicting write is reported, not silently won.** A `PATCH` carrying
   a stale `If-Match` validator was refused `412`, naming the current `ETag`
   in the body.

Two defects the pass exposed, both fixed the same session: the margin defect
above (`0242c0f`), and that the confirm step did not restate the assertion —
the banner swapped the "pick a page" prompt for the consequence line alone,
so the screen a reader confirms an irreversible write from never said what
was being asserted, and with 235 tiles the picked tile itself scrolls out of
view. Fixed (`54023fe`): the banner now reads "Pair en-fan page 60 with
en-mono page 81?" above the consequence line.

**A later pass measured a third thing the brief's seven items did not cover:
what happens while the write is still on the wire.** Without an in-flight
guard, a second Confirm click started a second concurrent write against the
same `ETag` and the loser got a `412` for a correction the reader only asked
for once, and `Escape`/Cancel could tell the reader nothing had happened
while the write was still going to land. Fixed (`372f6f7`) with a
`submitting` flag disabling all three exits for the duration. Measured
against the real corpus with the `PATCH` deliberately slowed: Confirm reads
"Writing…" and is disabled, Cancel is disabled, `Escape` leaves the banner
up, and exactly one correction lands.

## M5c: a note written on the raw, read on the English

Two measurements before any code, then the browser pass. All of it against
`corpus/gotoubun-v01`, which holds three editions — `raw-mono` (195 slots),
`en-mono` (194 slots, seven joined spreads) and `en-fan` (240 slots across
seven chapter files).

### A note's slot round-trips exactly, so travel needed no new arithmetic

Written into `crates/taiyaku-core/src/route.rs` as a throwaway test, run, and
reverted before the plan was written — the habit M5d's own plan established
after five of M4's seven defects turned out to have originated in a plan that
asserted rather than ran. Every slot of one edition resolved into another and
straight back, over `sidecar.rs`'s `REAL` (Go-Toubun v01) and `route.rs`'s
`STAR` (three editions, only the two hub edges stored):

| Pair | Slots | Landed | Agreed | Disagreed |
| ---- | ----: | -----: | -----: | --------: |
| `raw-mono` → `en-mono` → back | 195 | 193 | 193 | **0** |
| `en-mono` → `raw-mono` → back | 194 | 193 | 193 | **0** |
| `b` → `c` → back (composed through hub `a`) | 30 | 28 | 28 | **0** |
| `c` → `b` → back (composed) | 30 | 28 | 28 | **0** |
| `a` → `c` → back | 30 | 28 | 28 | **0** |
| `b` → `a` → back | 30 | 30 | 30 | **0** |

The slots that do not land are genuine gaps — `raw-mono` 1 and 6 have no
counterpart at all, and `STAR`'s `b` 0/1 reach a hub gap and stop — which is
the "unreachable from here" case the review list has to show rather than
swallow.

**What this licensed:** `notesHere` resolves once per *edition that holds
notes*, outward from the edition being read, rather than once per note. Both
directions agree on every slot that lands, so the cheaper direction is not a
different answer — and it holds `from` fixed across the scan, which is what
`Binding`'s single-pair route memo is for. Resolving per note would alternate
pairs and thrash it, the case O14's entry names as its reopening condition.
`reviewList` does resolve per note, because "where does *this* note land" is
the question it answers; that the two agree is checked exhaustively in
`notes.test.ts` rather than assumed, and sabotaging `notesHere`'s direction
fails that test and the round-trip and nothing else.

**Nothing about a correction moves a note.** A note is anchored to a slot of
its own edition, and `Sidecar::correct` changes pairings, never an edition's
slot sequence. A correction can change which slot a travelled note appears
*on* — which is correct, and the reason travel is computed at render rather
than stored — and can never orphan the note itself.

### The browser pass

`taiyaku serve corpus/gotoubun-v01/taiyaku.json --addr 127.0.0.1:8080` behind
the Vite dev server on 5173, because `App.tsx` is deliberately unreachable
from Vitest and nothing else covers the wiring.

1. **`n`, type, Save — and it is on disk beside the sidecar.** The editor
   opened naming its anchor, `raw-mono slot 88 · whole page`, on displayed
   page 89. `corpus/gotoubun-v01/taiyaku.notes.json` came back holding one
   note with a ULID id and **no `region` key at all** — absent, not `null`,
   which is the spelling `notes.rs`'s round-trip test pins.
2. **It survives a reload**, because it is on the server rather than in the
   browser profile.
3. **A note written on the raw is readable on the English — the milestone's
   whole end state — and it lands where the CLI says it does.** Verified
   against an independent implementation rather than against itself:

   | Reader | `taiyaku lookup` |
   | ------ | ---------------- |
   | swap to `en-fan` drew the tab on displayed page 127 | `raw-mono 88 -> en-fan 126` (0-based) |
   | swap to `en-mono` drew it on displayed page 84 | `raw-mono 88 -> en-mono 83` (0-based) |

   The review list states the same landing in slots rather than pages —
   `en-mono slot 87` for a note on `raw-mono` slot 88 — which is page 83 plus
   the four joined spreads that precede it. Two units, one answer.
   The travelled tab reads `Note written on raw-mono: …` and is drawn hollow.
4. **A region is stored as fractions of the slot, and the drag that made it
   round-trips.** Dragged from (0.25, 0.30) to (0.60, 0.55) of the painted
   image; stored as `x 0.24999`, `y 0.3`, `w 0.35001`, `h 0.25`.
5. **`r` tints the region on the edition it was written on and nowhere
   else.** On `raw-mono`: one tint, at `24.9995% / 30% / 35.0007% / 25%`,
   matching the stored fractions. On `en-mono`, the same note travelled:
   **zero tints**, tab still present. That is the design's honest half —
   transforming a region between editions is L1 geometry, and drawing the
   untransformed box would put it over the wrong panel.
6. **The review list marks what it cannot reach.** Three notes; the two on
   `raw-mono` 88 and 89 showed their `en-mono` landings, and the one on
   `raw-mono` slot 1 — which `lookup` confirms has no counterpart — was
   listed as `not reachable from en-mono` with its row disabled, rather than
   quietly missing.
7. **Export is the document, not a rendering.** The downloaded
   `taiyaku.notes.json` parsed and compared **equal** to what `GET
   /api/works/taiyaku/notes` returns.
8. **Two devices, and neither note is lost.** A second writer wrote through
   `curl` with the tag the browser was still holding. The browser's next Save
   answered 412, the reader was told "These notes changed elsewhere and have
   been re-read. Save again to keep yours.", **the draft survived untouched**,
   and the second Save landed. Both notes are in the file.
9. **The settings panel generated the module with no hand-wiring**, a seventh
   module proving M5b's claim again: an `Annotations` section with `markers`,
   `travel` and `reveal`. `markers` off took the tabs away (1 → 0) and back
   on returned them. `travel` off hid the note while reading `en-mono` and
   **left it visible on `raw-mono`, where it was written** — the precise
   behaviour, not "hide everything". Both survived a reload.

**One defect found, and it was not in the new code.** Pressing `a` while the
reader was showing a **gap** opened nothing at all: `App.tsx` returns the
`Gap` view instead of its own tree, so no panel below that branch is ever
rendered. A reader standing on a slot the viewed edition has no counterpart
for is exactly who wants to ask where their notes are, and every row in that
list already carries the not-reachable-from-here marking for this case. Fixed
on the branch — the list renders in the gap state too. **The settings panel
and the whole chrome are still unreachable there in the same way**, which is
older than this branch and is now O25 rather than fixed in passing.

The test data written during this pass was removed from `corpus/` afterwards:
that directory is gitignored, so nothing else would have.

## M5c, reported from a real browser: two defects a synthetic pass could not see

Both found by the owner within minutes of opening the reader, on the branch
that had already passed a nine-item browser pass. Both trace to the same
weakness in how that pass was driven: it reached for the shortest way to
produce a state rather than the way a person produces it.

**Typing a note fired every reader key.** `isInteractiveTarget` (`keys.ts`)
stands the window keydown handler down for a `button`, an `a`, an `input` or
anything carrying `tabindex` — and knew nothing about a `textarea`, because
until M5c the reader had none. So typing into the note editor pressed reader
keys at the same time: `s` opened the settings panel, `n` started another
note, `a` opened the review list, `r` toggled the tint, Space swapped edition,
a digit jumped to one. **Why the pass missed it:** every test that "covered"
the editor set its value with Playwright's `fill()`, which dispatches an
`input` event and never a keystroke, so not one of them sent a key through
that function. Fixed, with `textarea` and `contenteditable` both added — the
rule is "is this target taking text", not "is it one of four tag names" — and
pinned by a test in `keys.test.ts`. Verified by typing `sandbars: 3 seals and
a dog` one key at a time, a string chosen to contain every dangerous key:
the textarea holds it exactly, no settings panel opened, and the anchor line
still reads `raw-mono slot 4`, so nothing swapped or jumped.

**Drawing a region worked and was invisible.** The gesture recorded correctly
— a real mouse drag flips the editor's anchor line from `whole page` to
`region`, confirmed before anything was changed — but nothing was ever drawn:
no rectangle during the drag, and none after, because `Annotations` only tints
regions when `reveal` is on *and* only for notes that already exist, which an
unsaved draft does not. **Why the pass missed it:** the region was driven by
dispatching `PointerEvent`s at the image and then reading the stored JSON, so
it measured the arithmetic and never asked what a person sees. Fixed with a
dashed rectangle drawn from the draft — live while the pointer moves, settled
after release, and shown again when the note is reopened — deliberately
independent of `reveal`, which answers a different question ("show me every
stored region here") from the one a drag in progress asks.

A `preventDefault` on `pointerdown` was added at the same time on the reasoning
that a native `<img>` drag was swallowing the gesture. **That reasoning was
wrong and is corrected here rather than quietly:** `PageView` has always set
`draggable={false}`, so no ghost-image drag was ever starting, and the
pre-change measurement above proves it — the drag recorded a region perfectly
well, it simply drew nothing. The call is kept because a `pointerdown` that
begins a region should not also begin a text selection, which is a real effect
and one this reader does want suppressed; the claim about image dragging was
not measured and should not have been written.

Measured after, with a real drag rather than dispatched events:

| Moment | Rectangle |
| ------ | --------- |
| before the drag | none |
| mid-drag | `25% / 30%`, `20% × 12%` |
| later in the drag | `25% / 30%`, `35% × 25%` |
| after release | `25% / 30%`, `35% × 25%` |
| reopened after saving | `25%`, `35%` wide |

and the stored region is `x 0.25, y 0.3, w 0.35, h 0.25`.

**The lesson worth keeping:** a browser pass that produces state through
`fill()`, `dispatchEvent` or direct state manipulation is testing the code's
arithmetic with the browser as a convenient host, not testing what a reader
meets. Where a step exists to be performed by a person — typing, dragging,
clicking — it has to be performed that way, or the pass will confirm
everything except the part that is broken.

## The annotation overlay was placed on the page area, not on the page

Reported straight after the two defects above: "the corners should line up to
where the mouse is but it's not really doing that."

**The cause.** A region is fractions of the *slot*, and the overlay drawing it
was `position: absolute; inset: 0` on the **page area** — which is larger than
the page it draws. A portrait page in a landscape viewport leaves letterbox
down both sides, and that letterbox is background rather than page, so every
rectangle was offset by the left inset and scaled by the ratio between the two
boxes. The drag arithmetic was right the whole time — it measures against the
image, `paintedBox` and all — and the drawing was wrong, which is why the
stored fractions looked correct in every check that read them back.

**Why the earlier pass missed it, again.** The reveal check had confirmed the
tint's *style* — `left: 24.9995%`, `width: 35.0007%` — against the stored
region, which is a comparison between two numbers this code produced. It never
asked the only question that matters: is the rectangle on the page? A check
that had compared the tint's client rect to the image's client rect would have
failed immediately.

**The fix** is `PageAnnotations`, an overlay that finds the page it sits over
and measures it, rather than assuming the box around it is the page. That also
closed a second thing the same shape: continuous-scroll mode had no overlay at
all, because a single measured box cannot describe a column of 195 pages.
Each scroll group now gets its own overlay, and every slot's notes are resolved
once for the whole edition (`notesBySlot` — one resolve per *note* rather than
one per page) instead of scanning the note list once per mounted group.

**A third defect fell out of testing the fix**, and it is the interesting one:
switching modes left the `ResizeObserver` watching an image React had already
removed. It fired once with a zero-sized rect and nothing ever found the
replacement, so the overlay collapsed to a point in the top-left corner —
single mode correct, two-page mode drawing its tint nowhere near a page.
Fixed twice over: the component is keyed on the reading mode so the tree
rebuild remounts it, and a zero-sized measurement is now ignored rather than
taken, since an unlaid-out element is not a page of zero size.

Measured after, with real mouse input and against the pointer's own
coordinates rather than against computed numbers:

| Check | Result |
| ----- | ------ |
| dragged rectangle's corners vs. the pointer | **0.0 px** at both corners |
| tint's fraction of the page, `single` | `x .25  y .30  w .35  h .25` |
| `two` | same, and on a page rather than beside one |
| `scroll` | same |
| back to `single`, `two`, `single` again | same each time |

against a stored region of `x 0.25, y 0.3, w 0.35, h 0.25`.

**The habit worth taking from all four defects:** every one was invisible to a
check that compared this code's output to this code's other output, and
obvious to a check that compared it to the thing a person actually sees — the
pointer's coordinates, the image's rect, a real keystroke. Prefer the outside
measurement even when the inside one is easier to write.

## The drawing rectangle, and moving one

Two requests after the overlay was placed correctly: the selection was hard to
see while drawing, and a region could not be repositioned once drawn.

**Darker while the pointer is down, light once it settles.** The two moments
want opposite things and had been given one style. A manga page is mostly
white, so a pale wash over it barely registers while a reader is aiming — a
dark fill (`rgba(0, 0, 0, 0.42)`) reads at a glance against exactly that
background. But the same fill on a *settled* region would obscure the artwork
the note is about, which is the one thing a marker must not do, so the fill
drops to the light tint the moment the drag ends. The outline is doubled — a
dark line under the bright dashed one — so the edge holds against both a white
page and a black panel border instead of picking one and vanishing into the
other.

**A drag inside the region moves it; a drag anywhere else draws a new one.**
No mode, no handles: a reader who wants a different rectangle sweeps one out,
and a reader who wants this one somewhere else pushes it there. The cursor
says which is on offer — `crosshair` with no region yet, `move` once there is
one — because otherwise that a region can be moved at all is invisible until
someone happens to try it.

`moveRegion` clamps the region **as a whole** rather than clamping `x` and `y`
against `0..1` independently. Per-edge clamping would let a box dragged past
the corner keep its origin while its far edge ran out of room, so it would
quietly shrink as the reader pushed against the boundary — the sort of thing
that reads as the tool fighting you. Moving the origin within `0 .. 1 - w`
instead stops it at the edge at exactly the size it had.

A single click inside a region is exempt from the too-small check that
discards a stray click while drawing. Without that exemption, clicking once on
your own region — a plausible way to check it is selected — would delete it.

Measured with real mouse input:

| Check | Result |
| ----- | ------ |
| fill while drawing | `rgba(0, 0, 0, 0.42)` |
| fill once released | `rgba(255, 214, 102, 0.16)` |
| move vs. the pointer's own shift | `137, 205` px against `137, 205` px |
| size across a move | `206×256` before and after |
| shoved past the bottom-right corner | size kept, flush to the page edge (1px, 0px) |
| stored after that move | `x .7  y .75  w .3  h .25` — `x + w` and `y + h` both exactly `1` |

## The note's words, inside the region it marks

Asked for directly, and it is a **departure from the design**, so it is
recorded rather than slipped in. `2026-08-02-reader-m5-design.md` chose margin
tabs precisely so that "nothing is ever drawn over artwork", and left the body
in the panel; a region drawn around a panel with no hint of what it says turns
out to be a marker you have to go and interrogate, when the whole reason for
anchoring a note to a region is that the two are read together.

The design's concern is answered rather than overridden:

- The backing sits behind **the text only**, not across the region, so the art
  inside the box stays visible.
- It appears only where a region is already being shown — revealed (`r`), or
  being edited — never while simply reading.
- It is clipped to its own box. A region is sized by what a reader drew around,
  not by how much they wrote, so a long note shows its first lines there and
  stays whole in the panel, the tab's tooltip and the review list.
- It is hidden while the pointer is down. A label appearing under a rectangle
  still being swept out would cover the very artwork the reader is drawing
  around.

Measured: revealed saved note draws its body inside its box; a draft draws the
text as it is typed; and a body far longer than the box holds stays inside the
box's bounds rather than spilling across the page.

## Text behind its own switch, and the markers that went missing

**Superseded within the hour — see "One switch, not two" below. What follows
is why the words stopped riding along with `reveal`, which still stands; the
part that did not stand is their having a switch of their own.**

**The note's words became a fourth setting (`labels`, key `t`), off by
default, rather than riding along with the region.** Folding them into
`reveal` made them permanent once a reader had ever pressed `r`, since that
preference persists — which is what "visible all the time" meant. The two draw
different amounts over the artwork and are wanted at different moments: a tint
says *where* a note is, cheap to leave on while reading; its text says *what*
it is, and covers part of the panel it points at. A region has to exist for
text to sit in, so `labels` draws the box too whether or not `reveal` is on,
which is why `t` alone is a complete state rather than a modifier on `r`.

**Fixing the region overlay had moved the margin tabs onto the page.** The
overlay is anchored to the page — it must be, since a region is fractions of
the page — and the tab was positioned `right: 2` within it, which had meant
"two pixels from the right edge of the reader" while the overlay was the page
area and silently became "two pixels inside the artwork" once it was not.
Measured: the tab sat at x 1282–1292 against a page spanning 609–1294, a 10px
translucent sliver on the page's own edge, with 609px of empty background
beside it. Reported as not being able to see the markers, which was fair —
they were hiding on the page. They now sit at `left: 100%` plus a small gap,
in the margin the design puts them in, measured at x 1298 against a page
ending at 1294.

That rests on the design's own claim — fit-whole guarantees side margins on a
portrait page — and the honest failure mode if a page ever fills its area is
that the tab is clipped by the page area's `overflow: hidden`. No page in this
corpus does. The fix then would be to measure the available margin, not to put
the tab back on the artwork.

| State | Tab | Region box | Text |
| ----- | --- | ---------- | ---- |
| reading | in the margin | — | — |
| `r` reveal | in the margin | drawn | — |
| `r` + `t` | in the margin | drawn | drawn |
| `t` alone | in the margin | drawn | drawn |
| back to reading | in the margin | — | — |

## One note per page was all you could have

Reported as "I can't do multiple annotations per page", and it was exactly
that: `n` *reopened* the note already on the slot instead of making another,
and nothing else creates one, so a page could hold one note and no more.

**The reasoning for reopening protected against nothing.** It was written to
stop `n` pressed twice from leaving duplicate empty notes — but a draft is not
a note until Save, so a second press only ever replaced an unsaved draft.
Nothing could have been duplicated. What it cost was the ordinary case: one
note per panel, which is most of why a note has a region at all.

`n` now always starts a new note, however many the slot holds. Reopening an
existing one is what its margin tab is for, which is the affordance that was
already there. The one guard kept is that `n` is ignored while the editor is
already open, since starting fresh there would throw away typing nobody asked
to discard.

**A second defect was hiding behind the first.** A whole-slot note has no
region to derive a tab position from, so every one of them anchors at the
middle of the page — two such notes drew one tab exactly on top of another,
which looks identical to not being able to make a second note. `spreadTabs`
pushes only the colliding ones apart, so a tab whose neighbour is already
clear stays exactly where its region put it and still means something. A run
that reaches the bottom edge is shifted up as a group rather than sliding off,
since a tab past the end of the page is no better than one hidden behind
another.

Measured: three notes written on one page — two whole-slot, one with a region
— all three stored, three tabs drawn at 650, 719 and 963 px (gaps of 69 and
244), and clicking the first reopened *that* note rather than making a fourth.

## Colours, and the focus bug that came out from under them

A note carries a **colour name** — `amber`, `rose`, `violet`, `sky`, `mint` —
never pixels. `notes.rs` validates it as a name on the same alphabet an edition
key is held to and holds no opinion about what any of them look like; which
colours exist and what they are made of is `palette.ts`'s business, so a native
shell or a printout can answer differently without touching a stored note.
Absent means the default, so every note written before colours existed reads
as it always did, and a name this build does not know **falls back rather than
refusing** — the opposite of what this project does nearly everywhere, and
right here, because the alternative is a note that does not draw at all.

Five names rather than a colour picker. The point is telling one note from
another at a glance, which needs colours that are distinguishable rather than
arbitrary, and an arbitrary picker reliably produces a pale yellow on a white
page.

Travelled notes stay distinguishable by **fill** — solid is written here,
hollow is written elsewhere — which is what leaves colour free to mean whatever
the reader decided it means.

### Two defects found while testing it

**The running server was the stale binary.** `color` was added to the Rust
`Note`, the tests passed, and `taiyaku serve` went on rejecting it: `400 these
are not notes: unknown field 'color'`. `cargo test` does not rebuild the
binary, which `docs/status.md` has recorded elsewhere and which cost half an
hour here anyway. Every symptom below was downstream of it.

**A refused save left the reader typing into the page.** With the write
failing, the editor correctly stayed open holding the reader's text — and
focus sat on `<body>`, so the next keystrokes went to the reader rather than
the note: typing `sky note` opened the settings panel on the `s`. This is the
same class as the `textarea` defect fixed earlier, reached through a different
door, and neither `isInteractiveTarget` nor `autoFocus` could have caught it:
one only stands down while focus is *in* a control, and the other applies on
first mount, so a second note in one session was never focused at all.

Fixed at both levels, because either alone leaves the other hole open:

- The editor focuses itself whenever it opens on a note **and whenever a write
  settles**, with the caret at the end — reopening a note to add to it is the
  common case, and landing at the start puts a reader's first keystroke in
  front of what they wrote last time.
- **No key reaches the page at all while the editor is open.** It is a dialog
  that takes text, so a keystroke means the note whatever the browser thinks is
  focused. `Escape` still closes it; everything else belongs to the note until
  it does.

Measured after: three notes written in one session, each given a different
colour, each saved without a dialog opening behind it. Five notes on one slot
draw five boxes — `rgba(255, 138, 160, …)`, `rgba(120, 199, 255, …)`,
`rgba(126, 231, 179, …)` and two amber for the notes that name no colour —
each carrying its own text.

## One switch, not two

`reveal` (`r`) and `labels` (`t`) are one setting again, an hour after being
split. The argument for splitting them was that a tint and a block of text
draw different amounts over the artwork, so a reader might want one without
the other. In use that turned out to be a distinction without a decision: the
**margin tabs already say where the notes are**, without drawing on the page at
all, so the tint alone answers a question nobody has — you can see there is a
note there whether or not it is tinted. What is left is one question, and one
key for it: show me what these say, or give me the page back.

Recorded rather than quietly rewritten, because the reasoning in the split
commit was sound and still wrong: it reasoned about what the two *draw*
without noticing what the tabs already *told* the reader, which is the sort of
thing only using it surfaces.

`t` is free again. Measured: default reading draws a tab and no box; `r` draws
the box with its text; `t` now does nothing; `r` again puts the page back; and
the generated settings panel shows three annotation fields rather than four.

## The editor is an edit mode, not a form for one note

Asked for: pressing `n` should let you drag *any* box on the page around, not
only the one you are typing into. It is the right model — while the editor is
open the page is being edited, and a reader who has drawn three boxes reaches
for whichever one is wrong.

Three things had to change together:

- **Every region on the page is grabbable.** A drag starting inside any box
  moves that note and makes it the one the panel is showing; a drag starting
  anywhere else draws a new one, as before. Boxes are searched innermost-first
  — last drawn is on top — so a box inside another is the one you get.
- **Every region is visible while the editor is open**, whatever `reveal` says.
  A reader cannot grab what they cannot see, and an edit mode that hid the
  things it lets you edit would be a puzzle. `reveal` governs again the moment
  the editor closes.
- **The session holds edits per note, not one draft.** Moving three boxes and
  pressing Save once writes all three; a Save that wrote only the note the
  reader last happened to click would silently drop the other two. The note on
  show is derived from that map rather than stored beside it, so the shown note
  and the saved one cannot disagree.

**A defect the first version of this produced immediately.** Press `n`, then
go and move somebody else's box, then Save — and the note `n` created was
written too: empty body, no region, an invisible marker for a keystroke the
reader had already moved on from. Notes are now dropped on save when they were
invented this session *and* have neither words nor a region. Either half alone
is a real note — a marked area with the words still to come, or a note about
the whole page — and a note that *existed* and has been emptied is kept,
because emptying one is an edit where deleting one is a button.

Measured: two saved notes; `n` then dragging both of them then Save leaves
**two** notes with both regions moved (`x .15 → .40`, `y .45 → .66`); the same
flow with the new note actually written in leaves three.

## A note can be a point, and its words have somewhere else to live

Two additions, and they turn out to be one: a **point** anchor has no area to
hold text, so the surface that shows a note's words away from its own box is
what makes points usable at all.

### Three anchors, one field

`region` absent is the whole slot, zero-sized is a **point**, sized is an area.
The degenerate rectangle rather than a second field, so nothing downstream
grows a third code path and the server validates all three with one rule —
`w >= 0` where it used to be `w > 0`, and the refusal now reads "zero-sized is
a point, negative is nothing".

A **click** on the page plants one, where it used to be discarded as a stray
drag. That was the only thing a click could do before, which meant marking a
panel required drawing a box around it.

A point is drawn as a ring rather than a filled dot — a blob hides the detail
the reader pointed at — and it needs slop to be grabbable, since an exact
containment test on something with no area could only ever be satisfied by
hitting one infinitely small spot. `regionContains` takes that slop from its
caller, so what can be grabbed is what can be seen.

### The list, and the popover

Both, as asked. Every marker now carries a **number**, and the number is the
row in the list — so a note read in the list can be found on the page without
hunting, which is exactly what a point needs.

- **The list** shows the notes on the slot being read, in marker order, in
  their colours. Beside the page or below it, by `notesList`: `auto`, `side` or
  `below`. `auto` answers by window width — beside the page where there is
  margin to hold it, under it where there is not — and re-answers on resize
  rather than at load, since "is there room" stops being true the moment
  somebody drags the window narrower. Measured: at 1600px the list sits to the
  right of the page; narrowing to 900px moves it underneath.
- **The popover** replaces the browser's `title` on a tab, rather than joining
  it. `title` appears after a delay a reader has usually given up on, cannot be
  styled to say which edition a travelled note came from, and never appears at
  all on a touch screen.

### The module contract's first declared options

`notesList` is a string with three legal values, and `settingsPanel.ts` has
carried a note since M5b that a select is "what the design calls for once a
module declares one with options; none does yet, and the contract carries no
field to declare options against, so that path is not built ahead of a case
that exists only in the design". The case arrived. `ReaderModule` gained
`options?: Record<string, string[]>`, `fieldKind` answers `'select'` for a
string that has them, and the generated panel renders a `<select>` — still
without knowing which module it is looking at.

### The same trap, twice in one day

The point note would not save, and the reason was that `taiyaku serve` was
still the binary built before `w >= 0`. `cargo test` does not rebuild it. That
is recorded in `docs/status.md` already and it cost twenty minutes anyway,
both times with a symptom pointing somewhere else entirely — a 400 the client
reported as a stale-validator notice, and a note that simply never appeared.
**After any change to a crate the running server serves, rebuild and restart
it before believing anything the browser says.**

Measured: an area note and a point note saved together, stored as
`w .4 h .18` and `w 0 h 0`; two markers numbered 1 and 2; list rows reading
`1 an area note` and `2 point · a point note`; one ring drawn for the point.

## Three reports on the first real use of points and the list

**A point could not be moved, and the grab test was not the reason.** It has
slop for exactly this. What was missing is that a point *being edited* drew
through the rectangle path — zero by zero, an outline around nothing — so
there was nothing on screen to aim at. A saved point drew as a ring and an
unsaved one drew as nothing at all, which is the whole difference. It now
draws the same ring, dashed to say it is not written yet.

Worth keeping: the fix for "cannot grab it" was in the drawing, not in the
hit-testing, and the hit-testing was where the reasoning had all gone.

**The hover text came out one letter per line.** The popover sat at
`right: calc(100% + 8px)` — outside the layer — and an absolutely positioned
box given only a `right` offset takes its width from the space to the left of
that offset, which there was none of. So it shrank to the narrowest box that
could hold a character and the words ran vertically. Now placed just inside
the page's right edge with a fixed `width`: shrink-to-fit is what failed, so
nothing is left to it.

**The side list stretched the full height of the page whatever it held.**
`top` and `bottom` both pinned, so two rows sat at the top of a column of empty
background — a panel that looked like it had lost its contents. Height comes
from the rows now, with `maxHeight` keeping a long list scrolling inside the
page rather than past it. Measured: one row, 40px tall, in a page area of 1349.

All three were invisible to every check run before them, and all three are
about what the thing *looks* like rather than what it computes — which is the
same lesson this milestone keeps teaching, now with a sharper edge: the
arithmetic being right is not evidence that anything is on screen.

## No Save button, and a way back instead

Four changes, and the third and fourth are one idea.

**A point no longer squeezes its words into itself.** The label rendered inside
the pending marker, which for a point is twenty pixels across — a column one
character wide, sitting on top of the very point being moved. A point's words
live in the list beside the page and in the popover on its tab, which is what
those surfaces were built for.

**A new note takes the next colour nobody on the page is using.** Notes on one
page come out visibly different without a reader choosing, and once every
colour is in use it cycles by least-used rather than giving up and repeating
the first. Counted over *this page* rather than the whole work, since telling
one note from another only matters where they are drawn together.

**Writes are automatic; the Save button is gone.** A reader writing a note has
already decided to write it, and the button's only real job was to be the thing
you did *not* press when you changed your mind. Debounced at 700 ms — longer
than the reading position's 400, because a position is a number and a note is
prose, so pausing mid-sentence should not cost a round trip — and flushed on
`visibilitychange`/`pagehide` alongside the position, since a half-typed note
is the one thing here that cannot be recomputed from anything.

Closing the editor now **flushes** rather than discards, which is the opposite
of what it used to mean. With no Save, closing is "I am done here", not
"abandon this".

**A stale validator no longer stops and asks.** With a button, a `412` could
say "somebody else wrote; press Save again" — the reader was standing there
with a finger on it. There is nobody to ask now, so the write re-reads the
other device's document, lays this reader's own notes back over it *by id*, and
writes that. Notes belonging to other ids survive untouched, which is the part
that matters; two devices on the same note is still last-write-wins, as it
always was. Retried once and once only: a second failure is not a race, and a
loop would hammer the server while a reader watches nothing happen.

**Which is why there is history.** The Save button *was* the undo, and removing
it takes that away, so `history.ts` puts it back properly: whole-document
snapshots, `z` back and `Z` forward, and the same two controls in the review
list. Snapshots rather than an operation log, because a log needs an inverse
for every verb the editor ever gains, where a notes document is a handful of
small records and keeping the whole thing costs nothing measurable.

`z`/`Z` rather than `ctrl+z`: `intentOf` yields any key carrying a modifier so
a reader's own browser shortcuts always win, which means `ctrl+z` never reaches
this reader — and leaves the textarea's own undo working while a note is being
typed, which is the right answer for that key anyway.

A step is *written*, not only shown: what is on the server is the document, so
an undo that changed only the screen would be undone again by the next
keystroke. `Debounced` gained `cancel` for this — undo and delete both replace
the document outright, and a write scheduled before either would land a moment
later carrying exactly what the reader had just removed.

Measured: three notes written without a Save button existing at all (0 found on
screen each time), coloured `amber`, `rose`, `violet` without anybody choosing;
`z` twice took the server from 3 notes to 2 to 1, and `Z` put it back to 2. A
point being edited draws a 24px ring with no label inside it, during the drag
and after.

## No Done button either

Being finished is not an act a reader performs — it is what they have already
done by looking somewhere else. So `Escape` or a press on the background
around the page ends the note, and there is no button for either. What is left
in the editor is `Delete`, which stays a button because it is destructive and
wants aiming at.

**`Escape` flushes rather than drops**, which is the second time this
milestone has had to invert that word. It meant "abandon this" while a Save
button decided what was kept; with writes landing on their own it means "I am
finished here", and the last few hundred milliseconds of typing sitting in the
debounce is a change the reader made. Undo is what abandons now.

**Only the background closes it, not any non-page click.** In `two` mode the
facing page is a page too, and reading "not this one" as "not any" would end
the note because a reader touched the other half of a spread. The test is
whether the press landed on an image at all.

Measured: no `Done` button on screen; typing then clicking beside the page
leaves the editor closed with the text saved (`closed by clicking away`);
typing then `Escape` does the same inside the debounce window (`closed by
escape`); and clicking the page *itself* leaves the editor open, because that
is where drawing happens.

## Closing the gaps before merge

Asked for a list of what was still thin, and then for all of it. Four tests
and one defect.

### The defect: one stale note bricked a work's notes

`validate` checked every note on every write, and the client sends the whole
document back — so a single note left pointing at an edition a re-solve had
renamed, or a slot an edition had shrunk past, refused **every future write on
that work**, including the write that would have removed it. Measured against
the running server before the fix:

```
stored notes:  [raw-mono-OLD]
adding a NEW, valid note beside it:
  404  note `01STALE` names the edition `raw-mono-OLD`, which work `taiyaku` does not have
```

Not exotic for this project: `align --write` merges and re-solves are ordinary,
and O13 and O24 are about this hazard class already.

Fixed by grandfathering: an anchor already stored **and unchanged** is not
re-checked. A stale note survives as something a reader can see — the review
list already marks a note it cannot reach from here — and delete. What is still
refused is a new bad anchor, and moving an existing note onto one, which are
the cases where this write is what makes the anchor wrong. Only the anchor is
grandfathered: a colour and a region cannot be made stale by a re-solve, so
both are checked on every note every time.

That moved validation inside the write guard, since it now needs the stored
document to compare against.

### The tests

**A corpus-gated check for notes**, which every other subsystem had and this
one did not: 629 anchors accepted across three real editions, two of which
join spreads and so have more slots than pages — the case the four-page
fixture cannot tell apart from a page check — plus one past the end of each,
so it cannot be spelled "accept everything".

**The gesture rule, out of `App.tsx` and into `notes.ts`.** Grab-any-box,
innermost-first, background-finishes, and the two guards that keep a press on
the facing page of a spread from being read as either — the most-churned rule
of this milestone, and it lived where nothing could test it. Now
`gestureFor`, with eight tests, and `App.tsx` reads the DOM for it and carries
out the answer.

**`editsToWrite` and `mergeOver`**, likewise: the blank-note rule was
duplicated in two places in `App.tsx` and tested in neither, and the
stale-validator merge — lay this reader's notes over the other device's by id
— was a line inside a `catch`.

**The lost-update guard, raced properly.** The existing test sent one write
then the other, which a build with no guard at all also passes. The new one
races two writes in flight, and does it **twenty times**: measured by removing
the guard and running a single-round version five times, two of those runs
serialised by luck and passed. A test that can miss the defect it exists for
is worse than none — it reports green for a reason it never checked. Twenty
rounds fails the same sabotage 5 times out of 5.

### One blemish left, deliberately

A document over axum's 2 MB default answers `413 Failed to buffer the request
body: length limit exceeded` — axum's wording, not this server's, which is the
thing O5 was about. The limit is right and the message is not ours. Not worth
holding a merge for.

## O25: the chrome in a gap, driven against the corpus

`App.tsx` returned early on `shown.kind === 'gap'`, handing back `Gap` instead
of the reader's own tree, so `Chrome`, `Settings` and the mode toolbar were
never rendered while a gap was showing. Driven in a real browser against
`corpus/gotoubun-v01` (hub `raw-mono`, 195 slots; `en-mono` 187 pages; `en-fan`
235 pages / 240 slots) with `taiyaku serve` behind Vite's proxy.

**The two gaps were chosen out of the sidecar rather than hunted for**, because
the obvious one cannot fail the check that matters. A run with no `b` is a gap:

```
raw-mono -> en-mono  (sound)       gap a=1        prev_b=0        next_b=1-4R
raw-mono -> en-mono  (sound)       gap a=6        prev_b=1-4R     next_b=5-186
raw-mono -> en-fan   (fragmented)  gap a=2-3      prev_b=43       next_b=44-45L
raw-mono -> en-fan   (fragmented)  gap a=7        prev_b=44-45L   next_b=46-94
raw-mono -> en-fan   (fragmented)  gap a=193-194  prev_b=230-234  next_b=None
```

**Gap A — `raw-mono` slot 1 in `en-mono`** is the one M5c already used, and its
value here is its `sound` verdict. `healthReadout` is silent on a sound edge by
design, so it cannot produce this string at all:

```
raw-mono slot 2 / 195 · p. 2 · 1% · sound pair · no counterpart here · 1m read · ~c000 · 1 of 7
```

`sound pair · no counterpart here` on screen therefore has exactly one possible
source, `gapHealth`. The string had been written in `healthLine` since M5a and
was unreachable from the reader for as long as O25 stood.

**Gap A cannot test the filmstrip anchor**, which is why a second was needed:
its neighbours are slots 0 and 2, so the rail sits near tile 0 whether the
anchor works or not. Measured there, `scrollLeft` was 0 — consistent with the
anchor and equally consistent with its absence.

**Gap B — `raw-mono` slot 193 in `en-fan`** is the one that can fail. Its
nearest paired neighbour lands on `en-fan` **page 234**, which is **slot 239**
of 240 — the last one — drawn as the last of 235 tiles. *Both this paragraph
and `status.md` first said "slot 234", naming a page index as a slot: `en-fan`
carries five joined spreads below page 234, so the two spaces differ by five.
`nearestPaired` returns `landed.slot`, and the run before the gap
(`{"a": "188-192", "b": "230-234"}`) lands on page 234. The reader's own
measured row `slot 102 / 240 · p. 100` is the cross-check — slot index 101 for
page index 99, +2 for the spreads below it. Corrected after review; the
measurement below is unaffected, since the rail is clamped at its maximum
either way.*

```
scrollLeft 24260   scrollWidth 25824   clientWidth 1564
```

Scrolled to the far end, not to 0. Without the anchor this reads 0. The rail is
at its maximum — `scrollIntoView({inline: 'center'})` clamps, since the anchor
tile is closer to the end than half a viewport.

The whole screen at Gap B, verbatim — the `Gap` view, its offered move, and the
readout row beneath, read out of the DOM in one go:

```
No en-fan page for raw-mono page 194.
This pairing is fragmented: matches are sparse across the whole volume, so gaps
like this one are expected.
[← en-fan page 235]

raw-mono slot 194 / 195 · p. 194 · 99% · fragmented pair · no counterpart here
  · 3m read · no chapter edition
```

`no chapter edition` rather than a chapter code, because `chapterAt` resolves
through `en-fan` — where this slot is the gap — so there is nothing to name.
Recorded in full because the design doc quotes this row and cited this section
for it; two of its segments were only in the check table below, split apart.

**Every check, and what each produced.**

| check | result |
| --- | --- |
| `Tab` draws the strip, viewed edition's tiles | 187 tiles in `en-mono`, 235 in `en-fan` — both the viewed edition, never the authority |
| no tile marked | `aria-current` absent on every tile; every computed `border-color` `rgba(0, 0, 0, 0)` |
| rail centred on the neighbourhood | Gap B, `scrollLeft` 24260 of 25824 |
| position line names the edition | `raw-mono slot 194 / 195 · p. 194 · 99%` while viewing `en-fan` |
| health says the absence | `sound pair · no counterpart here` (A), `fragmented pair · no counterpart here` (B) |
| row and view agree on the page | `p. 194` and "No en-fan page for raw-mono page 194" |
| `s` draws the settings panel | eight sections, "Credit pages" among them |
| `a` draws the review list | mounts beside `Gap` and `Settings` — three panels, one mount site |
| arrows inert | `ArrowRight` left slot 194 and the gap unchanged |
| a tile click leaves the gap | `slot 102 / 240 · p. 100 · 43% · fragmented pair · …`, one tile `aria-current`, solid white border |

**The `arrows inert` row above is no longer true — corrected in place on
2026-08-13 rather than deleted, since this is a measurement recorded at the
time rather than a claim about today.** `session.ts`'s `step` now takes the
offered move on any non-`at` display, gaps included, on
`borrow-lender-and-arrows`'s "An arrow on a gap takes the offered move, on
both screens" — so `ArrowRight` at Gap B would now move rather than leaving
slot 194 unchanged. What the row measured is unchanged: at the time of this
session, `step` called the unwrapped `display` and refused anything that was
not `at`, and a gap is exactly that, so the arrow did nothing.

**The reading case was checked too, as the negative half.** On `raw-mono`
unswapped the row reads `slot 2 / 195 · p. 2 · 1% · 2m read · ~c000 · 1 of 7`:
no edition prefix, and no health segment at all, since `healthReadout` is
`undefined` while viewing the hub. After leaving Gap B the row reads
`fragmented pair` *without* `no counterpart here`. Both confirm the gap forms
are reached only in a gap.

**Two behaviour changes the design predicted, both confirmed.** The mode
toolbar is now drawn beside the `Gap` view, where the early return had meant it
was never rendered. And the gap branch's second `NotePanel`, whose handlers
were stubbed to `() => undefined`, is gone; the one real panel serves both.

**The tenth check, run separately, under a backup.** The review list's "not
reachable from here" marking needs a note to exist and this work had none. It
was first recorded as not-run — `reviewList` is untouched by this branch, and
`corpus/` is gitignored, so a write there is not revertable by git. It was then
run properly: the three `corpus/gotoubun-v01/*.json` files were copied aside
first, and the reader's own `n` was used to type a note on `raw-mono` slot 193
— the gap's authority slot, so the note is definitionally unreachable in
`en-fan`. Typed a character at a time through the real keydown path rather than
`fill()`, which is the M5c lesson two sections above.

Swapping to `en-fan` and pressing `a` gave:

```
raw-mono slot 193 · not reachable from en-fan
O25 check: this note is anchored to raw-mono slot 193, which en-fan has no page for.
```

**Two things this showed that no other check reached.** The entry's button is
`disabled`, not merely labelled — `onGo`'s `if (entry.here === undefined)
return` is visible in the control rather than being a click that silently does
nothing. And the readout row ended `· no notes here of 1`, which is the note
count's gap form: `here` is empty because a gap has no slot to hold notes, while
the total still counts the whole work. That is the line the design predicted as
"no notes here of 7" and nothing had yet produced.

Afterwards the note file (`taiyaku.notes.json`, stem-prefixed like
`taiyaku.progress.json` — *not* `notes.json` as `notes_path`'s doc reads at a
glance) was removed and `taiyaku.progress.json` restored from the copy; all
three files then diffed identical to the backup.

**A defect in the plan, found by executing it.** The plan's first command was
`cargo build --release -p taiyaku-serve`. `taiyaku-serve` is a library crate and
the `taiyaku` binary belongs to `taiyaku-cli`, so that builds nothing runnable.
Corrected before the pass.

## The credit-pages panel, and where skipping had to be decided

Driven against a **fixture**, not the corpus, because the corpus cannot exercise
this at all: **no sidecar in it has a `credits` field** — all five editions
across `gotoubun-v01` and `kaguya-v01` report `None` — so `Resolver.credits`
answers `[]` everywhere, the panel renders its empty state, and `skip` skips
nothing. A pass against the corpus as it stands could not fail. That is the
same trap the O25 plan fell into when it named a gap whose anchor check could
not distinguish pass from fail, caught this time before the pass rather than
during it.

The fixture is a copy of `corpus/gotoubun-v01/taiyaku.json` with
`taiyaku credits --write` run over all seven `en-fan` members, placed **beside**
the archives it names (member paths are relative to the sidecar) under a
different stem, so its `fixture.progress.json` and `fixture.notes.json` siblings
cannot collide with the real ones. Removed afterwards; the corpus's own three
JSON files diffed identical to a backup taken first.

It wrote `en-fan credits = [95, 168, 198, 225]`, exactly the set
`gotoubun_fan_edition_classifies_four_credit_pages` asserts, and the command
printed its own reasoning:

```
page  member    local  run  at-end  recurs  verdict
  95  c001      51/52    1     yes      14  credits
 168  c003      31/32    1     yes      24  credits
 198  c004      29/30    1     yes      14  credits
 225  c005      26/27    1     yes      99  credits
 226  c005.5     0/9     4      no       2  -
   1  c000       1/44   42      no     103  -
```

**The two rejections are the rule working.** Page 226 sits at a member's
*start* and page 1 inside a 42-page run; `at-end` is what discriminates, which
is §4's "position is the part that survives". Note also that `recurs` would
have ranked page 1 highest of all six — the clearest reason that signal
classifies nothing.

Those are zero-based page indices, so the panel shows **pages 96, 169, 199 and
226**, and the thumbnails requested `/api/works/fixture/en-fan/pages/95…225` —
the conversion checked in both directions at once.

### Skipping had to be decided while pairing, not after it

The design said `skip` would filter finished groups, dropping one only when
every slot in it was skipped, so that `two` mode could not hide an ordinary
page paired with a credit page. Built that way and driven, **`skip` did nothing
at all in `two` mode**:

```
two mode, skip off:  122 tiles   … page 93+94   page 95+96   page 97+98 …
two mode, skip on:   122 tiles   … page 93+94   page 95+96   page 97+98 …
```

Every one of the four credit pages sits at a chapter's end, so every one pairs
with an ordinary page, so no group was ever wholly skipped. The rule was not
wrong about the trade-off it named — hiding a wanted page is worse — it was
wrong that the trade-off was the only choice.

Deciding *while pairing* is the third option: the partner search steps past a
skipped slot, so its neighbour pairs forward instead of being stranded beside
it. Same run, after the change:

```
two mode, skip off:  122 tiles   … page 93+94   page 95+96   page 97+98 …
two mode, skip on:   119 tiles   … page 93+94   page 95+97   page 98+99 …
```

`page 95+97` is the whole of it. `readableGroups`, which had been written,
tested and reviewed clean, was deleted: `groups`' own `skipped` argument
subsumes it and does the part it could not.

### Every check, and what each produced

| check | result |
| --- | --- |
| `C` lists the viewed edition's credit pages | four entries, `page 96/169/199/226`, thumbnails requesting pages 95/168/198/225 |
| viewing an edition with none | `No credit pages found in raw-mono.` |
| `skip` off, `single` | 235 tiles |
| `skip` on, `single` | 231 tiles, no credit tile present |
| `skip` off, `two` | 122 tiles |
| `skip` on, `two` | 119 tiles, no credit tile, neighbour re-paired to `95+97` |
| position readout total | `/ 240` either way — skipping never renumbers |
| taking an entry with `skip` on | `slot 98 / 240 · p. 96`; `→` to p. 97, `←` to p. 95 |
| the panel's own line | "still in the reading flow" → "kept out of the reading flow. Taking one still goes there." |
| `panel` off | `C` inert |

### Five defects the whole-branch review found, and one I had the evidence for

The pass above recorded "taking an entry with `skip` on" as passing on the
strength of its readout — `slot 98 / 240 · p. 96`. It also returned
`pageShown: ["page 1"]` in the same object, which I wrote off as a decode-swap
artefact. **It was the defect.** `App.tsx`'s `currentIndex` clamped
`findIndex`'s `-1` to `0` with a `Math.max` that had been harmless while every
slot belonged to some group, so the page area drew the volume *cover* while the
readout beneath it correctly said page 96. Every surface disagreed with every
other and the check still went green, because the check only read one of them.

Four more, none of which any test could reach:

- **`nearestGroup` could not see a group straddling the skipped slot.** `[2, 4]`
  with 3 set aside is one tile spanning it; testing only a group's first slot
  forward and last slot backward said "not adjacent" both ways, so the arrow
  passed over the one tile next to the reader — `←` gave 0 and `→` gave 5 where
  2 was right. With slots 1 and 3 both set aside the arrow did not move at all.
  Every one of the four new tests for this was `single` mode, where a group is
  one slot and the shape cannot arise.
- **A partially-skipped joined spread was split in `two` mode**, pairing slot 0
  with a spread's second half and dropping the first — while `single` mode, on
  the same input, kept the spread whole. Unreachable through `creditSlotsIn`,
  which filters by page, but the spread branch had deliberately written that
  case out rather than assuming it away, and the two modes then disagreed.
- **A hand-inserted `regroup` break falling inside the skipped run vanished.**
  `groups(…, [1], {1})` was byte-identical to `groups(…, [], {1})`: a reader
  who had fixed parity by hand and later switched `skip` on would get a
  different parity for the rest of the volume and no sign why.
- **Every page skipped drew a blank page area** with none of the words the
  panel's own empty case was given.

The first is fixed by not skipping the page the reader is standing on
(`skippedApartFrom`) — the panel is the only way onto one, so being there is
deliberate, and exempting it means the page is drawn in all three modes, marked
in the filmstrip, and stepped off by the ordinary arrow path. Re-driven with
both preferences asserted rather than blind-toggled:

```
single mode:
slot 98 / 240 · p. 96 · 41%     drawn: page 96
tiles 232 (231 + the exempted one)   credit tiles present: page 96 only
marked: page 96                 → page 97   ← page 95
```

and stepping away puts it back to 231 tiles with none present. **That run is
`single` mode**, which the block did not say. `two` mode behaves the same way
and its numbers differ: the exempted page is drawn as half of a pair rather
than alone, so the strip holds 120 tiles standing on it against 119 away, and
the group is `[96, 97]` — page 95 beside page 96 — where with the reader
elsewhere slot 97 is in no group at all.

Three of the other four are `pairing.ts` fixes, pinned by tests that would have
caught them. **The fourth is not, and its invariant is the one thing here that
nothing else states**: "every page skipped drew a blank page area" is fixed
incidentally by `skippedApartFrom`, in another module, and what makes it true
is that *every position the reader can hold belongs to some group*. Nothing in
`App.tsx` says so and nothing would go red if it stopped being true — the page
area would simply draw nothing. Matching by slot instead of by page would break
it on the first joined spread. `credits.test.ts` now pins it directly.

**What this says about the pass, not the code.** The browser pass was written
to check the readout, and every one of these lives somewhere else — in the
drawn page, in the tile count, in a mode the tests never entered. A check that
reads one surface cannot notice the others disagreeing with it, and this branch
produced four passing tests and an approving review over a feature whose page
area was drawing the wrong page.

### Walking the reader from every position, and the one thing it found

The exemption in `skippedApartFrom` changes the grouping *as the reader moves*
— stand on a credit page and it is drawn, step off and it is set aside again —
which is the kind of feedback loop a fixed example cannot rule out. So the real
per-render pipeline was run as a whole:

```
step(at, by) = stepToGroup(groups(slots, mode, reading, breaks,
                 skippedApartFrom(creditSlotsIn(slots, creditPages), slots, at)),
               at, by)
```

from **every slot** of seven editions — flat, spreads and credits interleaved, a
credit page adjacent to a spread, consecutive credits, credits at both ends, a
hand-inserted break, and the real `en-fan` shape (235 pages, spreads at 45, 91,
133, 164, 196, credits at 95, 168, 198, 225) — in `single` and `two`, asserting
that an arrow always makes progress, never reverses, never revisits a slot, and
that no position is a dead end in both directions.

**Nothing about skipping failed.** No stuck arrow, no oscillation, no content
stepped over. The whole-branch review reached the same conclusion independently
by exhaustive enumeration over every slot table up to nine slots.

**What failed was older than any of it**, in 7 of 42 cases and always the same
shape: a forward arrow on the *last* group's second slot answers that group's
first slot, moving backward one page. It reproduces with no credits and no
skipped set — `stepToGroup`'s clamp meeting its own land-on-the-first-slot
rule, both from `c11c1e9` — and in all three modes wherever the volume ends in
a joined spread:

```
two,    [[0,1],[2,3],[4,5]]      from slot 5 → 4
single, [[0],[1],[2],[3,4]]      from slot 4 → 3
scroll, same                     from slot 4 → 3
```

Tabled as O28 rather than fixed here: the fix is three lines in the reader's
central step function, and this branch is about credit pages. See its entry in
`status.md` for what reaches that state, since an arrow cannot.

*O28 has since closed — see "O28 — closes here" below, which also corrects two
things this passage gets wrong.*

The simulation itself is worth keeping in mind as a shape rather than as a
file — it is what would have caught `nearestGroup`'s straddle bug, which four
unit tests missed because every one of them was `single` mode, where the shape
cannot arise.

## O28 — closes here

The fix is one line where the clamp was, and it is the *shape* of the clamp
that was wrong rather than its bounds:

```ts
const next = here + by
if (next < 0 || next >= all.length) return slot
return all[next].slots[0]
```

`stepToGroup` holds two rules. Landing on a group's first slot makes a swap
predictable from what is visible — both halves of a joined spread draw the same
image and resolve differently, so a reader who cannot see which half they are
on should always be on the one the group names. Clamping stops an arrow running
off the end. **They only meet where there is no next group**, and there the
first has no business running at all: it says where an arrow lands when it
*moves*, and a press with nowhere to go does not move.

**None of the affected presses changed the picture.** A group is one image and
both slots sit inside it, so the arrow appeared to do nothing — correctly, at
the end of a volume — while re-authoring which slot a swap, a note or a written
position would answer from. That is why the manual pass never found it and the
simulation did.

Measured across all three modes, with nothing skipped:

| Mode | Groups | Press | Before | After |
| ---- | ------ | ----- | -----: | ----: |
| `two` | `[[0,1],[2,3],[4,5]]` | forward from 5 | 4 | **5** |
| `single` | `[[0],…,[4],[5,6]]` | forward from 6 | 5 | **6** |
| `scroll` | `[[0],…,[4],[5,6]]` | forward from 6 | 5 | **6** |
| `two` | `[[0,1],[2,3],[4],[5,6]]` | forward from 6 | 5 | **6** |
| `two` | `[[0,1],[2,3],[4,5]]` | backward from 1 | 0 | **1** |
| `single` | `[[0,1],[2],…,[6]]` | backward from 1 | 0 | **1** |

**Two things O28's own entry had wrong, both found by running it rather than
reading it.**

The first: it recorded that backward at the start of the volume "already
returns the slot unchanged". It does — from a group's *first* slot, which is
the only case the existing test covered (`stepToGroup(g, 0, -1)` on a grouping
with no spreads, where every group is one slot). From the second half of a
leading joined spread, backward answered 0: the same invisible move as the
defect, in the same direction the reader had pressed, which is exactly why it
read as correct. Fixing only the forward end would have left it there, and left
the two ends of one function stating two rules — which is how this arose in the
first place.

The second: the entry describes a defect of volumes ending in a joined spread.
In `two` mode it needs no spread at all, because a pair is a group too — an
ordinary six-page volume has it at slot 5. The entry's own first measured line
shows this and the prose around it does not say so.

**Checked by sabotage rather than by having once failed to compile.** With the
old clamp restored, the two new boundary tests fail with the predicted values —
`expected 4 to be 5` and `expected +0 to be 1` — and the third passes either
way. That third one pins that a second slot *away* from a boundary still moves
in both directions (`3 → 4` forward, `3 → 1` backward, across a mid-volume
spread); without it the fix could be spelled "a second slot never moves", which
is a different and worse function.

## O26: align --write, driven against the corpus

The unit test that closes O26 calls `write_sidecar` directly, which is the only
way to reach the write — `align` solves before it writes, so a test that went
through the command would need real archives. Which is exactly what the corpus
is, so the command path was driven there instead of being left unclaimed.

Two runs against `corpus/gotoubun-v01`, writing to a scratch path outside the
corpus so no real sidecar was touched:

```
1. raw-mono -- en-mono          → wrote scratch.json    inode 575378, 1712 bytes
2. raw-mono -- en-fan/*.cbz     → merged scratch.json   inode 575396, 5.1k
                                  added `en-fan` | 3 editions, 2 alignments
```

The second is the case O26 is about: a file that already exists, holding an
edition and an edge, rewritten to add a spoke. **The inode changed**, so the
destination was replaced by a rename rather than opened for truncation — the
same observation the unit test makes, on a real document rather than a fixture.
No `.tmp` file was left in the directory, and `lookup` read the result back
whole: three editions, and `raw-mono 10 -> en-mono 8` still answering off the
edge written by the *first* run, which the second run had to carry through
untouched.

A third run, `en-fan` given as a directory, is worth recording because it is the
failure path: the command refused it (`no page images. It holds .cbz, none of
which taiyaku reads as a page`) and the existing file kept its inode and its
size — the two things `ls -li` shows, which is what was actually looked at, not
a hash of the bytes. That refusal happens before the solve, so nothing is
staged over a document that is about to be kept.

**A refusal *after* the solve names a path the caller never typed**, which is
the one thing this change makes worse. `--write /nonexistent-dir-xyz/out.json`
used to answer `/nonexistent-dir-xyz/out.json: No such file or directory` and
now answers:

```
taiyaku: /nonexistent-dir-xyz/out.json.tmp.4041762.0: No such file or directory
```

`atomic::write` names the *temporary* on a failed stage and the destination on
a failed rename, which is the right distinction inside the module and the wrong
one at a command line, where `.tmp.4041762.0` is a file nobody asked for. The
destination is still legible inside it, so the message misleads about nothing —
but it is noise. `credits --write` has said this since it was fixed, so the two
commands agree; making either one name the destination is a change to
`atomic::write`'s five other callers or a wrapper at both sites, and neither
belongs on a branch whose claim is that `align` writes atomically. Tabled.

*O29 has since closed — see "O29 — closes here, and not where the item said it
would" below. Two things in the paragraph above turned out to be wrong, and
both are why it closed where it did: the fix is a change to no caller at all,
since `atomic::write` is already handed the destination, and this was never
only a command-line problem — three callers in `taiyaku-serve` put the string
straight into a 500 body.*

The verdicts are the ones already on record for this volume — `sound` at average
run 64.3 for `raw-mono -> en-mono`, `FRAGMENTED` at 23.8 for `raw-mono ->
en-fan` — which is the point: the fix moved where the bytes are written, not
what they are.

**The standing checks turned out to drive this harder than the runs above did.**
`corpus::composing_through_the_hub_agrees_with_a_direct_solve` calls
`crate::align` itself, once per spoke, building a **five-edition star with four
alignments** into a scratch path — so every `--write` merge after the first goes
through the changed writer, on a document four times the size of the one the
unit test uses. It passes, and the figure it exists to hold is unmoved:

```
2427 page comparisons | loss 0.91% | disagreement 0.00% | 21 blank pages not compared
```

0.91% against the 1.0% ceiling, which is what `corpus.rs` records as the
measured value. All nine `taiyaku-cli` checks pass in 38.29s; the three in
`taiyaku-serve` pass in 15.80s, one of them printing its own skip for an unset
`TAIYAKU_STUMP_URL`. Twelve for twelve, matching the count `status.md` carries.

**Run them with an absolute path.** The invocation `corpus.rs`'s own module doc
gives —

```sh
TAIYAKU_CORPUS=corpus cargo test -p taiyaku-cli corpus:: -- --nocapture
```

— does not run them. `cargo test` sets each package's test binary CWD to that
package's directory rather than the workspace root, so `corpus` resolves under
`crates/taiyaku-cli/` and finds nothing; the check prints `TAIYAKU_CORPUS unset;
skipping` (the variable *is* set — the message names the wrong cause) and libtest
reports `ok`. A green run that measured nothing, which is the exact failure
`atomic.rs`'s own test comment describes one level down. `scripts/check-all.sh`
already knows: it canonicalises to an absolute path and says why, in a comment
that names this trap. Nothing else does, and the doc comment carrying the
trapping command is not one of them.

## O30: the gate's first job had never run the corpus checks

Closing O30 — a relative `TAIYAKU_CORPUS` skipping every standing check while
libtest reported `ok` — turned the skip into a panic, and the panic immediately
failed `scripts/check-all.sh` on an invocation the gate had always accepted:

```
$ TAIYAKU_CORPUS=corpus ./scripts/check-all.sh
==> cargo test --workspace --all-features
test result: FAILED. 129 passed; 9 failed
```

The gate validated and canonicalised `TAIYAKU_CORPUS` *beside its second run*,
the one that re-runs `corpus::` with `--nocapture`. The `test` job above it
inherits the variable too, and inherited it raw. So the nine `taiyaku-cli`
checks skipped in job one and measured in job two, and nobody could tell,
because job one captures stderr for a passing test — which is the very reason
the second run exists.

Resolving the path at the top of the script instead fixes both. The evidence is
wall-clock, in the *first* run's own line:

```
                       taiyaku-cli        taiyaku-serve
before  (job one)      0.11 s             0.75 s
after   (job one)      38.11 s            15.06 s
```

Nothing was added to job one. That is 38 seconds of corpus work that a green
gate had been reporting as done for as long as anyone has run it with a relative
path — which is the path the module headers documented, so it is the likely way
it was run.

**The order matters more than the check.** The gate's own comment already named
this trap and closed it correctly for the run it sat beside; it was three lines
too low in the file. A guard placed after the thing it guards is worth what it
looks like it is worth right up until something measures it.

## O29 — closes here, and not where the item said it would

The item offered two places and preferred the wrong one. Its wording:

> Either `atomic::write` takes the caller's intended path into its staging
> error — one function, and five callers whose messages all change — or the
> command line wraps it, which is now a *single* place, `write_sidecar`, since
> O26 merged the two into one.

**Reading the callers before building anything settled it the other way**, on
one sentence the item had recorded as a reason not to care:

> the four callers inside `taiyaku-serve` have since `atomic.rs` existed —
> where it matters less, because a 500 does not print a path to anyone.

A 500 here does print a path. `routes.rs`'s `Failed` arm is
`(StatusCode::INTERNAL_SERVER_ERROR, message).into_response()`, so the error
string *is* the response body, and three of the four callers reach it —
`notes::store` (`routes.rs:540`), `progress::store` (`routes.rs:620`) and the
sidecar rewrite (`routes.rs:836`). Only `cover.rs` discards its error with
`let _ =`. So the temporary's name was reaching HTTP clients as well as the
command line, and the cheap command-line wrap would have fixed one surface of
four.

**The other half of the framing was wrong too, in the direction that made the
work smaller.** "Five callers whose messages all change" reads as five edits;
it is none. `atomic::write` is *already handed the destination* — it is the
`path` parameter — and simply named `temporary` instead in one of its two
branches. The fix is that branch.

**What the wrap would have produced, which is why it was never really the
cheaper option.** `write_sidecar` gets back a pre-formatted `String` with a
path already in it, so wrapping means prefixing:

```
staging:  out.json: out.json.tmp.4041762.0: No such file or directory
rename:   out.json: out.json: No such file or directory
```

Both paths in one message, or the same path twice. Producing a clean message
from the command line needs the bare `io::Error`, which means changing what
`atomic::write` returns — touching the same function plus all five call sites,
this time for real.

Both branches now name the destination and carry the distinction in words,
because the two failures genuinely are different events and the temporary is
deleted before the error returns, so naming it points at a file that no longer
exists.

Driven against the corpus, the exact scenario the item recorded:

```
$ taiyaku align "…/raw-mono/Go-Toubun no Hanayome - v01.cbz" \
    -- "…/en-mono/Go-Toubun no Hanayome - v01 (2018).cbz" \
    --name-a raw --name-b en --cache .taiyaku-cache \
    --write /nonexistent-dir-xyz/out.json

5 runs | 180 whole pairings | 13 involving a spread half | 2 unmatched slots
sound | average run 64.3 slots
taiyaku: /nonexistent-dir-xyz/out.json: staging a temporary beside it: No such file or directory (os error 2)
```

against the before, recorded in "O26: align --write, driven against the
corpus":

```
taiyaku: /nonexistent-dir-xyz/out.json.tmp.4041762.0: No such file or directory
```

The alignment line is worth keeping beside it: 64.3 slots a run is the same
figure O23's entry records for this pair, so the run reaching the write is the
ordinary one and not some degenerate case that happens to fail early.

**Two tests, one per branch, and each asserts the absence of `.tmp.` rather
than an exact sentence** — the claim is that no invented filename appears, not
that a particular wording is printed, and `temp_path` builds every temporary
with that infix so it cannot show up by accident. The rename branch is forced
by putting a *directory* at the destination: staging succeeds, and a file
cannot be renamed onto a directory. That test also checks the temporary is not
left behind on the way out.

**Checked by sabotage.** Restoring the old `temporary.display()` in both
branches fails both tests and each names its own branch's message —
`out.json.tmp.1289842.1: No such file or directory` for staging and
`out.json.tmp.1289842.3: Is a directory` for the rename — so neither test is
passing on the strength of the other's branch.

## O1 — closes here, and the fix broke the sidecar on the way

O1 named two exits and had been stuck on the first one since M2. The first was
a real session log showing conditional-request reuse over genuine reading; the
second was one sentence at the end of the entry:

> Landing `Cache-Control`/`Last-Modified` so a genuine repeat costs no round
> trip at all, rather than a cheaper one, would close the item outright,
> independent of any session log.

The second exit is the one taken, and taking it is what makes the first
unnecessary rather than merely optional. The item's whole argument for
reopening was that an `ETag` shrinks a response and leaves the round trip in
place — so the number that mattered was "how many round trips does real
reading make?" Give the client freshness and there are no round trips left to
count, and the question stops gating anything.

**What shipped is one constant and four call sites.**
`PAGE_CACHE_CONTROL = "private, max-age=86400"`, on `get_page`, `get_thumb`,
the OPDS stream and the OPDS cover. Two of those four had no validator of any
kind before this.

**Superseded: the value is now `private, no-cache`.** The four call sites and
the validators all stand; only the freshness lifetime went. A work id is not
unique — it is the sidecar's filename stem, and every sidecar in this corpus
is `taiyaku.json` — so `/api/works/taiyaku/raw-mono/pages/10` served a
different volume's artwork depending on which sidecar the server was started
with, for up to a day, with no symptom but recognising the pictures. A
labeller hit it on the first switch between works.

**`Last-Modified` is named in the item and is deliberately not shipped.** Two
reasons, and the first is the one that would have hurt. It needs the member
file's mtime, which needs a `stat`, on the async worker, on every request —
and `locate_member` exists precisely so a `304` can be answered from the
in-memory sidecar before any I/O happens. Adding a date would have put disk
back on the path the previous task cleared. The second: these bytes are
addressed by content, and mtime is not, so re-downloading a byte-identical
archive would move the date and throw away a cache entry that is still
correct. The `ETag` answers revalidation, `Cache-Control` answers freshness,
and there is no third question left for a date to answer.

**`immutable` is not shipped either, and the reason is the URL, not the
bytes.** A page's bytes really are fixed by `(member sha256, offset)` — that
is what `page_etag` is derived from. But the URL names a work, an edition and
an index, and none of those move when an archive is replaced. `immutable`
tells a client never to revalidate for the entry's whole lifetime, so a
replaced member would leave every reader holding the old bytes with no way to
correct it: O11's failure mode, imported into the browser. One day is chosen
from two ends rather than measured — long enough that a work reopened next
morning costs no round trip, short enough that a replacement corrects itself
with nobody clearing anything. Any value from an hour to a week satisfies
both; this is the round number in the middle, and it is a choice, not a
measurement.

### Measured from the server's side, which is what every earlier attempt lacked

The earlier O1 measurements were all taken in the browser and classified by
`transferSize`, and the entry already records why that was thin — Resource
Timing's `responseStatus` normalises a validated hit to `200`, and only one
indirect CDP check ever confirmed a `304` on the wire. This run does not
argue with any of that; it asks the other end. `taiyaku serve
--log-requests` writes a JSONL line per request it actually receives, so
"did the reader ask?" has an answer that owes nothing to a browser field.

Ten slots read forward in the real browser reader against
`corpus/gotoubun-v01`, `raw-mono`, starting at slot 100 — a populating pass,
then a full reload, then the same ten slots read again:

| | Second read of the same ten slots |
| --- | ---: |
| Distinct pages touched (prefetch included) | 14 |
| Resource Timing entries | 16 |
| …of those, `deliveryType: "cache"` | 16 |
| Bytes on the wire | 0 |
| **Page requests the server logged** | **0** |

The server's log for that whole window holds ten progress writes, two
work-list reads, and one `304` each for the sidecar and the notes. Not one
page request. Before this change every one of those 14 would have been a
conditional request the reader waited on — cheap in bytes, and a full round
trip in latency, which is what the item was about.

`deliveryType` is worth a line of its own. It is a first-class browser field
meaning "served from the HTTP cache", and unlike `transferSize` it does not
have to be interpreted — but it is still a browser field, and this run does
not rest on it. CDP's own request list reports `200` for all sixteen,
including the ones that never left the machine, which is the same trap in a
new place: the wire is the only thing that settles it, and the server is on
the wire.

### The fix broke the sidecar, and the test written for it did not notice

`not_modified` is shared by four handlers — `get_page`, `get_sidecar`,
`get_notes` and the OPDS stream. The first version of this change put
`PAGE_CACHE_CONTROL` inside it, unconditionally, which is the obvious place
for it right up until you list the callers.

The result: **the server began telling clients they could reuse a sidecar,
and a notes document, for a day.** In the browser both stopped reaching the
server at all — `transferSize: 0`, nothing in the request log. That is a lost
correction waiting to happen. `PATCH /alignments` rewrites the sidecar; a
reader who corrects a run, reloads, and is handed their own stale copy has
watched the correction UI silently undo itself. Notes are worse, because the
`ETag` a stale read carries is what the next `If-Match` write is conditional
on.

**Two things about how this was found are worth keeping.** It was not found by
the test written alongside the change — that test asserted the mutable routes
carry no `Cache-Control` on their `200`, which was true the whole time. The
leak was on the `304`, and a client stores what a `304` tells it (RFC 9111
§4.3.4), so the two are worth exactly the same. And it was not found by
reading the diff either; it was found by driving the reader and noticing that
the sidecar was missing from the server's log — a request that *should* have
been there, rather than one that should not. The absence is what showed it.

It was also, briefly, misdiagnosed as pre-existing: two documents being served
from cache with no `Cache-Control` on them looks exactly like heuristic
freshness, which is a thing browsers do. Restarting the server with the leak
fixed and re-asking settled it in one call — both documents revalidated, and
the server logged a `304` for each. Self-inflicted, not inherited.

**The fix makes the caller answer.** `not_modified(tag, freshness:
Option<&str>)`: the page routes pass `Some(PAGE_CACHE_CONTROL)`, the sidecar
and notes pass `None`. A default one caller overrides is what produced the bug;
an argument nobody can omit cannot be inherited by whoever writes the fifth
caller.

**Checked by sabotage, both directions.** Commenting out each of the five
header sites in turn fails
`every_route_serving_a_member_s_bytes_says_how_long_they_may_be_reused` and the
message names the route. Restoring `Some(PAGE_CACHE_CONTROL)` at
`get_sidecar`'s call fails
`no_document_a_client_can_change_is_given_a_freshness_lifetime` on its `304`
assertion — the exact bug, caught by name.

### Two gaps found reviewing the close, and one of them was a real one

**The OPDS stream's `304` was asserted by nothing.** The freshness test
checked all four byte routes on their `200` and then checked exactly one of
them — the `/api/…` page route — on its `304`. `opds.rs`'s
`stream_page` shares `not_modified` and passes `Some(PAGE_CACHE_CONTROL)`,
and no test and no curl in this whole task ever looked at what it sent. That
is the same gap O5 already recorded once and for the same client: Panels
reaches pages through the OPDS stream, not `/api/…`, so a `304` asserted only
on the latter is asserted on the route nothing uses. Both are now in the
loop, and stripping the freshness from `opds.rs:285` fails it by name.

**`status.md` overclaimed in the sentence written to describe this.** It said
the four byte routes "carry an `ETag` and a day's freshness". Two of them do;
`get_thumb` and the OPDS cover carry freshness alone, because both find the
member only inside `spawn_blocking`, which is too late for a validator to
skip the work it would be validating. Corrected there rather than left as a
sentence that reads true and is not.

### The behaviour the bug threatened, driven end to end

Nothing had actually confirmed that a correction survives the browser cache —
only that the `304` no longer carries freshness, which is a statement about
headers rather than about what a reader sees. Driven directly against
`corpus/gotoubun-v01`, with the sidecar backed up first and restored after:

1. The browser reads `/api/works/taiyaku` twice, so the document is stored.
   `ETag` `980d3b80…`, no hand runs.
2. A correction goes in through the real route — `PATCH /alignments`,
   `If-Match` on that tag, `raw-mono` slot 40 to `en-mono` slot 41 — and
   returns 200. The document now holds two hand runs: the correction, and the
   slot it displaced, left unpaired.
3. The browser reads the sidecar again, a plain `fetch` with no cache
   options. It gets `af2168f9…`, both hand runs, `deliveryType` `(network)`,
   5,052 bytes on the wire.

The correction is visible on the next read. With the leak in place this same
read was `deliveryType: "cache"`, `transferSize: 0`, and absent from the
server's log entirely — so the reader would have been handed its own stale
copy and watched the correction undo itself. That is the failure this
verifies against, and it is measured rather than reasoned about, because the
leaking version was run and logged before it was fixed.

## Which derived transform maps a rect between editions

L1 needs one rule for turning a rect on one edition's page into the same
content on another's, derived at read time from the two drawn images' natural
dimensions and nothing else. The design named two candidates and a baseline,
and the promotion rule was fixed in the spec (§4) before any number existed, so
that the result decided the cost rather than the cost deciding how the result
reads. `crates/taiyaku-cli/src/geometry_experiment.rs` is the harness; it
reports and asserts only that it measured something. It ran twice: once to find
the answer, and once more — both directions, 98 s — to try to break it.

**Measured:** `raw-mono` against `en-mono`, the one edition pair all twelve
corpus volumes carry, 24 slots per volume evenly spread through it — **288 of
2,200 eligible slots, out of 2,258 the solver paired**, across four series at
v01, v03 and v08. 52 further paired slots were excluded because one side was a
whole page and the other a spread half, which names no pair of drawn images,
and 6 because a side was blank and every blank page matches every other one
perfectly. For each slot, the central 60% of the raw page was fingerprinted,
and a coarse-to-fine grid search over per-axis scale and offset found the crop
of the English page whose fingerprint scored highest against it. Scored with
`taiyaku_core::similarity` over `fingerprint`, not a metric invented here, so
the number is in the same currency as every figure above.

**Reported over the 284 slots whose search settled**, not all 288. Four ended
the coarse pass on the edge of its own range, which is the search saying it ran
out of room rather than that it found a minimum: its answer there is a bound,
and a bound has no business inside a distribution of measurements. The first
run counted those four and then reported them anyway; every figure below is the
restatement. What the exclusion changed is at the end of this section, and the
short version is that it moved no median by more than 0.0008 and no p90 by more
than 0.0013, and roughly halved every maximum.

Residuals are fractions of the target page, and the figure is the furthest a
candidate puts a point that the best map puts somewhere else. Distribution, not
mean — the tail is the interesting part, for the same reason `margin` is the
weakest per-page value in a run.

| within the window | min | median | p90 | max |
| ----------------- | --- | ------ | --- | --- |
| identity dx | 0.0086 | 0.0296 | 0.0405 | 0.0848 |
| identity dy | 0.0000 | 0.0041 | 0.0116 | 0.0743 |
| letterbox dx | 0.0086 | 0.0296 | 0.0405 | 0.0848 |
| letterbox dy | 0.0025 | 0.0162 | 0.0224 | 0.0899 |
| pillarbox dx | 0.0021 | **0.0195** | **0.0276** | 0.1013 |
| pillarbox dy | 0.0000 | **0.0041** | **0.0116** | 0.0743 |
| CONTROL dx | 0.0011 | 0.0011 | 0.0023 | 0.0394 |
| CONTROL dy | 0.0011 | 0.0011 | 0.0041 | 0.0514 |

### The shipped candidate was the right idea on the wrong axis

The best map's **vertical** scale is 0.996 at the median — the identity map to
within one tick of the search's own final resolution — and its vertical offset
is 0.000. The vertical axis wants nothing done to it. Its **horizontal** scale
is not 1: it tracks `aFrom / aTo` across all twelve volumes, sign included,
going below 1 for exactly the three volumes where that ratio is below 1 and
above it for the other nine. Per volume the two agree to a median 0.5%, against
the 5.5% by which each differs from 1.

That is `letterbox`'s own assumption with its axes swapped. `letterbox` takes
the **max** of the two aspects as the shared content's, which puts every
framing difference on the vertical axis and never rescales the horizontal. The
corpus says the opposite: the shared artwork is as **tall** as the taller page
says, and the wider-aspect page carries its extra as margin at left and right.
Taking the **min** instead — one token in `mapRect` — is the rule called
`pillarbox` in the table above. It was not in the spec because nothing had
looked yet.

So the two candidates the design named finished second and third:

- **`pillarbox` wins.** Median residual **0.0195 of page width and 0.0041 of
  page height**, p90 **0.0276 and 0.0116**. Better than `identity` on the
  horizontal in 10 of 12 volumes, and identical to it on the vertical by
  construction. The two volumes where it does not win — Kobayashi v01 and v03 —
  are the two whose aspect ratio is nearest to matching already (`aTo/aFrom` of
  1.034 and 0.998), so there is almost nothing there for either rule to correct
  and they land within 0.002 of each other.
- **`identity` is second**, median 0.0296 / 0.0041, p90 0.0405 / 0.0116. It is
  exactly right on the vertical and uncorrected on the horizontal.
- **`letterbox` as shipped is third, and is strictly worse than doing
  nothing.** It leaves the horizontal untouched, so it inherits identity's
  entire horizontal residual unchanged, and then adds a 5% vertical rescale
  that the pixels do not want: its vertical residual is **four times**
  identity's at the median (0.0162 against 0.0041) and twice it at p90. A
  candidate that can only add error to one axis and correct nothing on the
  other is not a candidate.
- **`reset` loses to all three.** It is not a residual but a bound: after a
  reset the reader sees the whole page and has to re-find the balloon, so the
  distance is up to half a page. That is a bound and it is not measured; it
  does not need to be, because every candidate above is inside 5% of a page and
  half a page is 50%.

Similarity tells the same story in the solver's own currency. Median over the
window: best map 0.934, **pillarbox 0.727**, identity 0.723, letterbox 0.703.
At p90: 0.969, **0.871**, 0.844, 0.828. The ordering is the same at both
points and the same in the per-volume rows.

### The control, and why the horizontal number is believed

The score is `fingerprint`, which resizes every crop to a 17x16 grid whatever
its shape. Two crops holding the same artwork at different aspects are
therefore *rendered* differently before they are compared, so a search could
buy similarity by picking a scale ratio that squares the two renderings up
rather than by finding where the artwork is — and `sx/sy ≈ aFrom/aTo` is
exactly what that artefact would look like. **The horizontal finding is
indistinguishable from that artefact on its face, so it was tested.**

The identical search was run against a copy of the *source* page resampled to
the target's aspect. A resize moves nothing in fractional coordinates, so the
true answer there is exactly the identity map on both axes, with the aspect
difference fully present and no framing difference at all. It came back
**`sx/sy = 1.0000` for all twelve volumes** while `aFrom/aTo` ranged 0.952 to
1.063, and a residual of 0.0011 — a floor 28 times below identity's horizontal
median. The search does not chase aspect. The horizontal scale it finds on real
pages is a property of the pages.

One thing this control does not establish, said rather than left implied: it
rules out the artefact on a synthetic resample, where the two crops hold the
same pixels and the similarity peak is sharp. It cannot exclude a subtler
coupling on real pages, where content and framing vary together and the peak is
shallow enough for a small preference to tip it.

### The falsification: the same search, the other way round

Run on real pages, that is what the reverse direction settles, so it was run.
`en-mono → raw-mono`, on the identical 288 slots and in the same pass — the
same slots rather than a fresh sample of the reverse pairing, so that a scale
which failed to invert could not be blamed on the draw. The prediction was
fixed before the run: **a real geometric relation between two pages inverts
when the pages swap roles**, so the reverse horizontal scale must come back as
`aTo/aFrom` where the forward one gave `aFrom/aTo`, and the vertical must stay
at 1. A preference belonging to the metric or the search does not know which
page is which; it returns the same side of 1 both ways.

**It inverts, in twelve volumes out of twelve.** Medians per volume, with the
prediction printed beside the measurement rather than asserted about it:

| volume | fwd sx | predicted 1/fwd | rev sx | aTo/aFrom | fwd·rev | rev sy |
| ------ | ------ | --------------- | ------ | --------- | ------- | ------ |
| gotoubun-v01 | 0.9475 | 1.0554 | 1.0450 | 1.0502 | 0.9941 | 1.0000 |
| gotoubun-v03 | 0.9512 | 1.0512 | 1.0488 | 1.0499 | 0.9964 | 0.9963 |
| gotoubun-v08 | 0.9475 | 1.0554 | 1.0375 | 1.0406 | 0.9901 | 1.0037 |
| kaguya-v01 | 1.0525 | 0.9501 | 0.9400 | 0.9479 | 0.9901 | 0.9925 |
| kaguya-v03 | 1.0450 | 0.9569 | 0.9475 | 0.9479 | 0.9944 | 1.0000 |
| kaguya-v08 | 1.0450 | 0.9569 | 0.9475 | 0.9479 | 0.9901 | 1.0075 |
| kobayashi-v01 | 0.9775 | 1.0230 | 1.0038 | 1.0342 | 0.9887 | 0.9925 |
| kobayashi-v03 | 1.0075 | 0.9926 | 0.9888 | 0.9980 | 0.9924 | 0.9850 |
| kobayashi-v08 | 1.0675 | 0.9368 | 0.9325 | 0.9409 | 0.9954 | 0.9850 |
| oshinoko-v01 | 1.0525 | 0.9501 | 0.9475 | 0.9467 | 0.9949 | 0.9963 |
| oshinoko-v03 | 1.0600 | 0.9434 | 0.9475 | 0.9461 | 1.0004 | 1.0000 |
| oshinoko-v08 | 1.0525 | 0.9501 | 0.9400 | 0.9479 | 0.9964 | 0.9962 |

Every row crosses to the other side of 1, including Kobayashi v01 and v03,
whose aspects nearly match and whose crossings are therefore the two smallest.
The reverse scale misses the predicted reciprocal by a median 0.009 and at
worst 0.019; it would have had to miss it by about 0.10 for the bias
hypothesis, which is an order of magnitude away. Read the other way, the
reverse scale tracks `aTo/aFrom` to a median 0.005, the exact mirror of the
forward run tracking `aFrom/aTo` to 0.005.

**The whole test in one number is the product of the two directions' scales**,
which is 1 if the relation is real and the square of the scale if it is not:

| over 277 slots settled both ways | min | median | p90 | max |
| -------------------------------- | --- | ------ | --- | --- |
| sx forward · sx reverse | 0.8920 | **0.9941** | 1.0091 | 1.0840 |
| sx forward squared, if it were bias | 0.7877 | **1.0842** | 1.1236 | 1.2128 |
| sy forward · sy reverse | 0.8929 | 0.9925 | 1.0110 | 1.2502 |

0.994 against 1.084. And the vertical row is the reading of the horizontal
one: the vertical product's *true* value is exactly 1 — every candidate leaves
the vertical alone and the best map agrees — and it measures 0.9925, because
the search's final tick is 0.00375 and it lands one to two ticks low on each
direction. The horizontal product measures 0.9941, above that floor. **The
horizontal scale is reciprocal to within the search's own resolution.**

The reverse direction brought its own control and its own candidates, and both
say what the forward ones said. The reverse control returns `sx/sy` of 1.0000
or 1.0038 in all twelve volumes with a residual of 0.0011, so the search does
not chase aspect in that direction either. And among the reverse direction's
candidates, `pillarbox` inverted still beats `identity` on the horizontal:
median 0.0189 against 0.0307, p90 0.0264 against 0.0416 — the same ordering and
the same magnitudes as the forward run's 0.0195 against 0.0296.

What this does **not** rule out, since it is the reason the control exists: an
artefact that is itself antisymmetric in direction would also invert. That is
precisely the aspect-matching artefact — a search that squares two renderings
up rather than finding artwork would return `aFrom/aTo` one way and `aTo/aFrom`
the other — and it is what the control excludes, by putting the full aspect
difference in front of the same search with the answer known. The two
experiments are complements, and neither alone would be enough: the control
rules out the direction-antisymmetric artefact and the reverse run rules out
the direction-blind one.

### The exposure, and why there is no standing check

The promotion rule's first branch applies: the residual is comfortably inside a
panel, so the number is recorded here and the candidate ships, with no
thirteenth gated check. `docs/status.md` still says twelve.

The threshold was a guess and remains one — what fraction of a page a panel
occupies was never measured, and this experiment did not measure it either. The
spec's working figure was that 5% of a 1200px page is about 60px, "a fraction
of a balloon rather than a panel". `pillarbox`'s p90 of 2.8% of page width sits
at half that, and its vertical p90 of 1.2% at a quarter of it.

Not promoting it to a standing check is a decision with a reason, not an
omission. The transform is `mapRect` in `web/src/viewport.ts`, and a Rust
corpus check cannot see a change to it — it would re-measure the corpus's page
dimensions, which no code in this repository can alter, and go green while the
browser did something else entirely. The unit tests beside `mapRect` are what
guards `mapRect`. What this experiment guards against is choosing the wrong
rule, and that is a one-time question it has now answered.

Three residual exposures, in order of size:

1. **The horizontal residual does not go to zero.** `pillarbox` leaves 2.0% of
   page width at the median and 2.8% at p90 — about 8 times the search's own
   floor, so it is real and not measurement noise. Some of it is a per-page
   horizontal offset rather than a per-edition one: the best map's horizontal
   offset moves by 0.004 to 0.083 *within* a single volume, which is as large
   as its median. Nothing derived from two frames can correct something that
   changes page to page, so that part is a floor for any candidate of this
   shape, including a stored per-page transform's absence.
2. **The vertical residual is small but not zero**: 0.4% of page height at the
   median, 1.2% at p90, against a control floor of 0.1%. Uncorrected by every
   candidate here, all of which leave the vertical alone.
3. **The maxima were the search failing, not the candidate — and that is now a
   measurement rather than a suspicion.** The first run reported identity's
   horizontal max as 0.180 against a control max of 0.133 and read the tail as
   "a handful of low-contrast pages the grid search cannot localise". Both of
   those figures were the boundary slots, and both halve once the searches that
   ran out of room are dropped: identity's max is 0.085 and the control's is
   0.039. The reading was right and the pages have now been looked at. All
   four are near-blank: 0.1% to 0.9% of their pixels are darker than mid-grey
   and their luma standard deviations are 21 to 33, against a per-volume
   median of 65 to 90 for the same edition. Two of them are the same near-blank
   page twice — Kaguya v08's raw pages 65 and 187 are the same size with
   identical statistics and a maximum per-pixel difference of 52. They survived
   the blank-page exclusion because that test is an *exactly* zero dhash and
   these pages carry a mark or two. Every one of the four pinned its horizontal
   scale to an extreme of the coarse range, 0.82 or 1.18, which is ±18% — far
   outside any aspect the corpus pairs. So they are not "the truth is just
   outside the box"; they are pages with no peak to find, where the search
   walked to the wall.

**Two figures above are the search's resolution, not physics.** The best
vertical scale reads 0.9925 rather than 1.0000, and the control reads 0.9963
where the truth is exactly 1; both are one to two ticks of the final refinement
pass (0.00375 each). Read them as 1.0.

### What excluding the unsettled slots changed

Promised above and printed by the harness itself, which reports both the
settled figures and the all-288 figures so that "restated" is a number rather
than a claim. Left column is what the first run reported, right is what 284
settled slots report:

| within the window | all 288 median / p90 / max | 284 settled median / p90 / max |
| ----------------- | -------------------------- | ------------------------------ |
| identity dx | 0.0304 / 0.0405 / 0.1796 | 0.0296 / 0.0405 / 0.0848 |
| identity dy | 0.0041 / 0.0120 / 0.1429 | 0.0041 / 0.0116 / 0.0743 |
| letterbox dy | 0.0163 / 0.0228 / 0.1524 | 0.0162 / 0.0224 / 0.0899 |
| pillarbox dx | 0.0199 / 0.0289 / 0.1631 | 0.0195 / 0.0276 / 0.1013 |
| pillarbox dy | 0.0041 / 0.0120 / 0.1429 | 0.0041 / 0.0116 / 0.0743 |

No median moved by more than 0.0008, no p90 by more than 0.0013, and every
ordering, every margin and every conclusion is unchanged — which is the reason
the correction is worth stating rather than worth hiding. The maxima are where
it shows, and they are the figure the decision was already told not to rest on.
The control's row is filtered by its own boundary rather than the forward
search's, because the four searches of a slot fail independently and filtering
one measurement by another's result is how a distribution acquires a shape
nobody chose; that takes its dx max from 0.1327 to 0.0394.

The reverse direction runs out of room more often than the forward one — 11
slots of 288 against 4 — which is why the reciprocal figures are over the 277
where both settled. It is not explained here, only counted: the two directions
crop different pages and search onto different pages, and which asymmetry does
it has not been measured.

### What ships

`pillarbox`, and the falsification is why it is safe to say so rather than to
hedge. In `web/src/viewport.ts`'s `mapRect`, `const content = Math.max(aFrom,
aTo)` becomes `Math.min`; `cw` and `ch` swap roles, so `mapRect`'s doc claim
that "`x`/`w` come out unchanged" inverts to `y`/`h`, and `viewport.test.ts`'s
`heightens` test becomes a `widens` test. The `MapKind` name and the doc block
that explains the `max` choice at length both change with it: that explanation
is measured wrong.

The alternatives it beat, in order: `letterbox` as currently shipped, which
corrects nothing on the axis that needs it and adds four times the error on the
axis that does not; `identity`, which is right on the vertical and uncorrected
on the horizontal and would still be the recommendation had the reverse run
failed to invert; and `reset`, which is not a residual but a half-page bound.
`zoomModule`'s `onSwap` default and Task 7 Step 8's `kind` must not be `reset`.

The transform is TypeScript, so this note is the record and the unit tests
beside `mapRect` are the guard. A Rust corpus check cannot see a change to
`mapRect`; that is why there is no thirteenth gated check, and it is a decision
rather than an omission.

## Fit, the seam, and the keybind help

Three unrelated reader complaints raised after L1 shipped, grouped onto one
branch because each is small and none touches the alignment. The design is
`docs/superpowers/specs/2026-08-08-fit-seam-and-help-design.md`. What is worth
recording here is not the three features — the design has those — but the two
places the browser disagreed with what had been reasoned, one of which was a
regression this branch introduced.

Everything measured below was measured in a real browser against
`corpus/gotoubun-v01` (`raw-mono` 685×1024, `en-mono` 2147×3056 — the *cover*
alone is 2146 wide, which an earlier draft of `fit.ts`'s comment quoted by
mistake), served by `taiyaku serve` behind Vite's proxy, on a scratch sidecar
with absolute member paths so nothing written landed in the gitignored
`corpus/`.

### The complaint was not the images and not the aspect

"The Japanese raw editions have large gaps and don't fill the page." At a
1400×1263 page area, `en-mono` drew at 887×1263 and `raw-mono` at 685×1024 —
its own natural size, 239px of height unused. `PageView`'s fit rule was
`max-width`/`max-height: 100%` with `width`/`height: auto`, which downscales a
page larger than its box and leaves a smaller one alone.

**Two things it is not**, both checked before anything was built. Not the aspect
difference: raw 0.6689 against `en-mono` 0.7026 is about 5%, nowhere near the
observed gap. Not raw-versus-English: `en-fan` is 1114×1600 and does the same on
a tall enough display. It is low-resolution scans meeting a viewport bigger than
they are. The images themselves are clean — L1's geometry experiment printed a
per-edition flat-border measurement and `gotoubun-v01`'s raw reads `0.000,
0.000`, so there is nothing baked in to trim.

`upscaleToFit` therefore defaults **on**, reversing the M5 design, which is
corrected in place there rather than rewritten. The enlargement is 23% of a
line-art scan and `upscaleFilter` ships in the same commit because of it.

### The seam was closed on the wrong element, and only measurement said so

At the same window in two-page mode: cells 700px wide, images 685px, each
centred in its own cell — 15px of grey at the seam, widening as the window does.
The design's fix was for each cell to stop centring its page and justify it
toward the seam: `display: flex` plus `justify-content: flex-end` on the first
cell and `flex-start` on the second, with a `gap` reopening a measured amount of
grey on purpose.

**Built exactly as designed, it moved nothing.** Measured with the defaults: the
two `<img>` *elements* already touched — 0px between them — while their artwork
sat **359px apart**. Two things had combined. `upscaleToFit` had just made each
element fill its whole cell, so there was no free space left in the cell for
`justify-content` to distribute; and `object-fit: contain` then draws the art
inside that element wherever `object-position` says, which unset is `50% 50%`.
The cell was positioning a box that was already the right size, and the thing
actually off-centre was one layer further in.

The fix is `object-position` on the image — `pageStyle` gained an `ArtworkHug`,
`100% 50%` on the left cell and `0% 50%` on the right — which took the 359px to
**0px**, measured the same way. `seamStyle` keeps only the padding that splits
`gap`.

**The dead declarations were confirmed dead rather than assumed.** On a static
fixture of the cell → `PageView`'s `width: 100%` wrapper → `<img>` nesting, and
then on `gotoubun-v01` itself, swapping `flex-end`↔`flex-start` on both cells
produced byte-identical geometry, and removing `display: flex` altogether did
too. **No test could have caught this.** `justify-content` on a non-flex box, or
on one whose only child fills it, is silently inert, so an assertion can only
see that the declaration is present — which is exactly what the assertion in
`fit.test.ts` did. A rule that is inert and a rule that works are the same
string.

### The regression: every marker measured the element, not the artwork

`upscaleToFit` made the `<img>` element larger than the artwork inside it for
the first time, and two annotation paths were measuring the element.

**`Annotations.tsx` measured the element outright.** In single mode: a
1385×475 element holding **334×475** of artwork, inset 526px each side. A region
tint that should have started at x 842 was drawn at **466** — every marker on
every page out by 376px, on the default settings, from the moment `fit` shipped.

**`notes.ts`'s `paintedBox` was subtler and had been an identity for its whole
life.** With `width`/`height: auto` the element is sized to the fitted image, so
`Math.min` of the two ratios is the fit scale and `x`/`y` come out 0 — the
function described a distinction that had not arrived yet. When it did arrive it
was right about the scale and wrong about the placement: it split the letterbox
evenly, which is what `object-position: 50% 50%` does and is not what a page
hugging the seam does. Measured in two-page mode it answered 179 for both cells
— 179 and 872 across the page area — where the artwork was at **359** and
**693**. Each 692.5px cell holds 334px of art, so 359px of leftover was being
halved instead of pushed to one side.

Both now measure the artwork, through one `hugFor` in `App.tsx` that the two
drawn `PageView`s, the marker overlay and the drag arithmetic all ask, so the
page that is drawn and the geometry that measures it cannot disagree. The
browser arithmetic and the unit tests agree to the pixel: 2147×3056 in a
1385×475 element gives `w` 333.71 and `x` 525.64 against the measured 334 and
526; the same scan in a 692.5×475 cell gives `x` 358.79 and `x` 0 against the
measured 359 and 693.

**One trigger had to be added with it**, `img.addEventListener('load', measure)`.
The overlay now reads `naturalWidth`, and neither existing trigger can see that
change: the element is `100%` by `100%`, so a decode alters no layout size for
the `ResizeObserver` to fire on, and the zoom `version` only moves on a zoom or
a pan. Without it the overlay would sit out the first decode and, on a page
turn, keep the previous page's natural size — invisible while two pages of one
scan share an aspect, and wrong the moment a turn lands on a joined spread.

### The tabs, and the guarantee that stopped holding

A note's tab is drawn at `left: 100%` plus a small gap, in the letterbox the M5
design promised: "fit-whole guarantees the side margins the tabs live in on
every portrait page." Closing the seam is what removed that margin on one side
of each page of a pair. In `two` mode the tab now goes in the margin *away* from
the seam; single and scroll mode are unchanged, and the default is the old
behaviour so no existing call site changed meaning. Measured after: a left-cell
tab at x 552–570 against the facing page's artwork starting at 574.

Two alternatives lost. Holding a minimum seam gap open whenever the left page
carries a note keeps one tab rule everywhere, but the gap then appears and
disappears as pages turn, which reads as a layout bug rather than as considered
space. Letting the tabs overlap is cheapest and is the one thing this codebase
must not do: a tab drawn over the facing page's art asserts a note is on a page
it is not on.

### The help is generated, which is the whole of its design

`keys.ts` is now one ordered `READER_KEYS` table that `intentOf` dispatches from
and the overlay lists from, replacing six bare `if` statements that nothing
could enumerate. A binding that changes what it does cannot keep an old
description, because there is one description. The two properties the `if`
statements carried implicitly are now stated: the reader's own keys are answered
before any module's, and a matched row ends the walk even when it resolves to
nothing — so `4` on a three-edition work is silent rather than offered on to the
modules, which would otherwise let a module hold `4` on a work with three
editions and lose it on a work with four.

`KeyBinding` gained an optional `label` for the same reason, and that is a
widening of the module contract — `module.ts` asks for that to be a decision
rather than a habit, so it was reported rather than done quietly. The M5 design
carries the correction.

**Verified in a browser**, same fixture: `?` and `h` both open the overlay and
`Escape` closes it, and with `zoom.enabled` set false the ZOOM section
disappears from the list — which is the join between the registry and the
reader's preferences doing its job, and the thing that stops the overlay
promising a key that has been switched off.

**One gap in that join is known and is not closed.** `enabled` is a convention
rather than a field of the contract, and `credits` gates its key on a preference
called `panel`. Turning that off leaves `C` listed while the key does nothing.
It is the only module in today's registry with an off switch under another name;
the fix if it grows is a contract field, not a list of per-module exceptions in
the help module, which is the copy of a rule the whole file exists to avoid.

## The text-block detector, measured against four works, and why it does not work yet

`crates/taiyaku-cli/src/detect.rs` proposes text blocks behind `taiyaku
regions`, ahead of L2's per-panel pairing. Its five constants — `SMEAR_K`,
`MIN_STROKES`, `MAX_BLOCK_AREA`, `MIN_INK`, `MIN_BLOCK_STROKE_SPAN` — were
written as starting values, each saying so in its own doc comment, to be
settled by `scripts/region-survey.sh`. This section is that survey, run
read-only (no `--write`, nothing touched under `corpus/`), and it does not
settle them: the detector does not clear the L2 design's own bar, on every
edition tried, and the doc comments below still say "a starting value, not a
measurement" because that remains true. **No constant's value was changed by
this section**, and neither was any of `detect.rs`'s logic — that is a
decision for later, once there is something to decide between.

### The survey ran over four works, not the corpus

`corpus/` holds twelve settled `<series>-v<NN>` directories plus `incoming/`.
Four were surveyed: `gotoubun-v01`, `kaguya-v01`, `oshinoko-v01`,
`kobayashi-v01` — three series beyond the given one, raw and English both
present for every one. **This is four works, not the corpus**; the other
eight settled works and all of `incoming/` were not reached.

Building even that subset ran into two facts about the corpus that bounded
what a survey could reach before the detector's own quality entered into it
at all:

**Only two of the twelve settled works carry a `taiyaku.json`.**
`gotoubun-v01` and `kaguya-v01`. `regions` identifies an edition by matching
member digests against a sidecar; the other ten have no sidecar to match
against, so no script — this one or a working one — can survey them as they
stand. `oshinoko-v01` and `kobayashi-v01` got one for this survey the way this
repository already builds one for unsidecared material: `taiyaku align
--write` against the real corpus material, read-only, written to a scratch
path rather than into `corpus/`.

```
taiyaku align "$PWD/corpus/oshinoko-v01/raw-mono" -- "$PWD/corpus/oshinoko-v01/en-mono"/*.cbz \
  --title "Oshi no Ko v01" --write <scratch>/sidecars/oshinoko-v01.json
# 6 runs | 216 whole pairings | 6 involving a spread half | 1 unmatched slots — sound

taiyaku align "$PWD/corpus/kobayashi-v01/raw-mono" -- "$PWD/corpus/kobayashi-v01/en-mono"/*.cbz \
  --title "Kobayashi-san Chi no Maid Dragon v01" --write <scratch>/sidecars/kobayashi-v01.json
# 5 runs | 144 whole pairings | 0 involving a spread half | 2 unmatched slots — sound
```

**Outside `gotoubun`, every raw edition in this corpus is a directory of
loose images, never a `.cbz`** — checked across all twelve settled works, not
just the four surveyed. (`gotoubun-v01` is not the whole exception: `raw-mono`
is a single `.cbz` for `gotoubun-v03` and `gotoubun-v08` too, the other two
volumes this corpus holds.) `region-survey.sh`'s glob
(`"$work/$edition"/*.cbz`) is written, correctly, to find only archives, so it
cannot see these; it names them skipped rather than dropping them silently,
exactly as designed. Left to run over the whole corpus unmodified, this script
would report almost no raw editions at all outside gotoubun — a fact about
the corpus's packaging, not about `regions`, but one that would have made a
full run look far emptier than the detector's own performance explains. Each
directory-packaged raw edition below was surveyed by a direct
`taiyaku regions <directory> --sidecar <sidecar>` invocation instead — the
same command the script issues, just not discovered by its glob. (One more
instance of the same shape: `gotoubun-v01/en-color` and `raw-color` exist on
disk but are not in `gotoubun-v01/taiyaku.json` either, so they were left out
of the subset entirely — included, `regions` cannot identify them and the
script hard-fails. `kaguya-v01` has the same gap and was not named here
before: its sidecar holds only `en-fan` and `raw-mono`, so `en-mono` and
`en-color` — both `.cbz` — exist on disk but could not be surveyed either. A
work missing from the output is indistinguishable from a work with nothing to
report, which is exactly the silence this section is written against, so it
is named here rather than left implicit.)

### The nine `regions:` lines

```
work=gotoubun-v01  regions: edition=raw-mono   pages=195 blocks=148 v=94  h=54  median_area=0.0713 blocks_per_page=0.76
work=gotoubun-v01  regions: edition=en-mono    pages=187 blocks=348 v=269 h=79  median_area=0.0678 blocks_per_page=1.86
work=gotoubun-v01  regions: edition=en-fan     pages=235 blocks=402 v=272 h=130 median_area=0.0515 blocks_per_page=1.71
work=kaguya-v01    regions: edition=raw-mono   pages=213 blocks=293 v=168 h=125 median_area=0.0921 blocks_per_page=1.38
work=kaguya-v01    regions: edition=en-fan     pages=226 blocks=347 v=169 h=178 median_area=0.0706 blocks_per_page=1.54
work=oshinoko-v01  regions: edition=raw-mono   pages=223 blocks=172 v=87  h=85  median_area=0.0605 blocks_per_page=0.77
work=oshinoko-v01  regions: edition=en-mono*   pages=226 blocks=329 v=74  h=255 median_area=0.0103 blocks_per_page=1.46
work=kobayashi-v01 regions: edition=raw-mono   pages=146 blocks=272 v=79  h=193 median_area=0.0013 blocks_per_page=1.86
work=kobayashi-v01 regions: edition=en-mono*   pages=146 blocks=86  v=57  h=29  median_area=0.0812 blocks_per_page=0.59
```

`*` — the two built sidecars name that edition after the archive's own file
name (e.g. `oshi-no-ko----v01--2023`), not literally `en-mono`; relabelled
here for readability, unchanged otherwise.

**Against the L2 design's three checks:**

- **`median_area` well under 0.15:** passes on all nine — max is kaguya
  raw-mono's 0.0921. **This is not evidence of health.** `MAX_BLOCK_AREA`
  already discards anything above 0.15 per block, before the median is ever
  computed over what is left — so the check is satisfied by the filter that
  is supposed to be the backstop, not by the detector finding text. The
  diagnosis below is what a passing median was hiding.
- **`blocks_per_page` above 2:** **fails on all nine editions**, across all
  four works. The highest reached is 1.86, twice — gotoubun en-mono and
  kobayashi raw-mono — both still well short.
- **v/h follows the edition:** 5 of 9 nominally correct, but three of the
  five are within a few points of even (kaguya raw-mono 57%v, kaguya en-fan
  51%h, oshinoko raw-mono 51%v). Two are wrong with real margin: gotoubun
  en-mono (77%v; English, should read h) and gotoubun en-fan (68%v). One work
  is wrong on **both** sides at once, and inverted rather than merely
  vertical-biased: kobayashi raw-mono comes back 71%h (Japanese, should read
  v) while kobayashi en-mono comes back 66%v (English, should read h) — the
  same volume, backwards both ways simultaneously.

### The diagnosis

The hypothesis under test, from the task that produced this section:
`stroke_scale` takes the median longer side over *every* ink component on a
page, most of a manga page's ink components are artwork rather than type, so
the scale — and the smear gap built from it — tracks art rather than glyphs;
strokes over-merge into a handful of enormous blocks; `MAX_BLOCK_AREA`
discards them, which is exactly a page returning zero blocks; and a merge
driven by artwork has no reading direction for `axis_of` to find.

Tested with a throwaway Rust program (not part of this crate, discarded after
this section was written) that copies `Bounds`, `otsu`, `components`,
`stroke_scale`, `merge_along`, `axis_of` and the five constants **verbatim**
from `detect.rs`, run against 9 real pages pulled from the surveyed
editions — 3 each from gotoubun-v01 raw-mono, gotoubun-v01 en-mono,
kobayashi-v01 raw-mono. It decodes the same way `fingerprint::decode_luma`
does and prints every intermediate stage. Its final block count matched
`taiyaku regions`'s own per-page stderr table on all 8 non-blank pages,
which is the check that the copy is faithful rather than a plausible
rewrite.

| Page | Components | Under 5px | stroke_scale → gap | Final blocks | Largest thing `MAX_BLOCK_AREA` discarded |
|---|---:|---|---|---:|---|
| gotoubun-en p003 | 4,671 | 70% | 2px → 4px | **0** | 2147×2951 — **96.6%** of the page |
| gotoubun-en p006 | 23,502 | 94% | 2px → 4px | 2, axis wrong (v) | 1962×1494 — 44.7% |
| gotoubun-en p012 | 54,286 | 96% | 2px → 4px | 7, axis right (h) | 1856×1268 — 35.9% |
| gotoubun-raw p008 | 749 | 63% | 3px → 6px | 1 | 650×500 — 46.3% |
| gotoubun-raw p013 | 1,931 | 73% | 2px → 4px | 2 | 650×777 — 72.0% |
| kobayashi-raw p000 | 2,574 | 23% (mode 20–40px, 44%) | **19px → 38px** | **25** | 1654×2112 — 33.6%, alongside 25 good blocks |
| kobayashi-raw p001 | 1,336 | 87% | **1px → 2px** | 4, axis wrong (h) | 912×1368 — 39.3% |
| kobayashi-raw p007 | 3,682 | 63% | 3px → 6px | 1 | 1363×1208 — 51.6% |

(gotoubun-raw p001 is a genuine blank interstitial — min=max=255 across the
whole page, 0 components, 0 blocks correctly. Not counted above; a 0-block
page is not automatically a failure, and this one is the control that says
so.)

**The outcome the hypothesis predicts is confirmed on every one of the eight
non-blank pages.** `MAX_BLOCK_AREA` discards at least one block on all eight,
it is always the largest thing on the page, and it always covers a large
fraction of it — 33.6% to 96.6%, the range the table above actually shows.
Where nothing survives it, that is exactly a zero-block page: gotoubun-en p003
merges to one block covering 96.6% of the page and it is the only block, so
the page returns nothing. And the axis
result is exactly as decoupled from real reading direction as predicted —
gotoubun-en p006 and kobayashi-raw p001 each resolve to the wrong axis
individually, which is the mechanism working as described, not noise around
it: a merge dominated by artwork has nothing to do with which way a reader's
eyes are supposed to move.

**The mechanism as stated does not hold.** The hypothesis says the median
gets pulled *up* to an art-sized scale. On 7 of these 8 pages what is
actually driving `stroke_scale` is the opposite: 63% to 96% of the raw
components are **under 5 pixels** — screentone dots, JPEG ringing,
antialiasing fragments — which pulls the median *down*, to 1–3px, nowhere
near either glyph size or "art scale." The resulting gap is 4–6px on pages
685–2147px wide — checked against the pages actually sampled: gotoubun-raw's
two are 685×1024, gotoubun-en's three are 2147×3056, and kobayashi-raw p001
and p007 are close to 1468×2163. (kobayashi-raw p000, the one page that
works, is 4852px wide and is not one of these seven.) The giant blob still
forms at that tiny gap, but not because the gap is large: screentone and
hatching pack ink specks densely enough that a few pixels of tolerance is
already sufficient for `merge_along`'s repeated, transitive passes to weld
thousands of them into one component spanning most of a panel or the whole
page. Gap size is nearly
beside the point once the texture is this dense — avoiding this would need a
gap *smaller* than any measured here, not a bound on how large it gets.

**The one page that works makes the same point from the other side.**
kobayashi-raw p000 is the only sample where sub-5px specks are a minority
(23%), and the largest single bucket — 1,125 of 2,574 components, 44% — sits
at 20–40px, plausibly real glyph or line scale on a 4852px-wide scan.
`stroke_scale` lands at 19px, the gap at 38px, and the detector works: 25 of
27 candidate blocks survive, the axis is right, and the one thing
`MAX_BLOCK_AREA` rejects (33.6% of the page) is discarded *alongside* the 25
good blocks rather than in place of them — the filter doing exactly its
documented job. The difference between this page and the other seven is not
the filter. It is what `stroke_scale` was computed from.

**So: real, but mis-named.** The giant merge-driven blob and the axis noise
are exactly what the hypothesis said they'd be, and they are what is holding
`blocks_per_page` down and corrupting the axis split. But `stroke_scale`
fails to track glyph size because fine print noise outnumbers everything
else on most pages, not because artwork does — "most components are art" is
the wrong noun for what eight of nine probed pages show; "most components are
finer than any glyph" is closer, and the one page without that problem is the
one page that works.

### What this does not claim

Four works of roughly a dozen settled ones, nine editions, nine pages picked
by hand out of several hundred available across those editions. Not a random
sample, and not the corpus. The two built sidecars exist only at a scratch
path outside this repository — reproducing those two rows means rebuilding
them with the `align --write` commands above. `SMEAR_K`, `MIN_STROKES`,
`MAX_BLOCK_AREA`, `MIN_INK` and `MIN_BLOCK_STROKE_SPAN` are exactly as they
were before this section: starting values, not measurements, and their doc
comments still say so because rewriting them to claim otherwise would be the
"a lie" `l2a-task-6-brief.md` itself warns against. What this section adds is
not settled constants — it is a specific, page-level account of why the
values in place today cannot be settled by tuning them: the failure is
upstream, in what `stroke_scale` measures, not in where its multiplier or its
filters happen to sit.

## The scale rule, tried three ways, and neither candidate clears the bar

L2A Task 9 followed directly from the diagnosis above: `stroke_scale` takes
the median longer side over *every* ink component, and on most sampled pages
that population is 51%–99% screentone dots, JPEG ringing and antialiasing
fragments under 5px, so the median lands nowhere near glyph size. The task
fixed its acceptance rule before measuring anything — the discipline §6 of
the design states — and named two candidates to try against today's `all`:

- **`above-floor`** — the median longer side over components above a noise
  floor derived from the page's own distribution, not a fixed pixel count.
- **`modal`** — the mode of the component-size distribution rather than its
  median.

**A candidate is promoted only if it beats `all` on both**, measured over the
same nine editions the original survey covered: `blocks_per_page` clears 2
(the design's own bar), and the v/h split follows the edition — a Japanese
raw majority-vertical and an English edition majority-horizontal — on both
editions of at least one work. `median_area` is deliberately excluded, for
the reason the brief gives: `MAX_BLOCK_AREA` already discards anything above
0.15 before a region is constructed, so a passing median has never been
evidence of anything.

### What each candidate computes

Both live in `crates/taiyaku-cli/src/detect.rs`, `#[cfg(test)]`-gated —
neither is called by `detect_page`, so neither reaches the shipped binary —
and both are exercised by `region_experiment.rs`, extended to run all three
rules over the same strokes on the same 90 sampled pages (10 per edition, the
same nine editions and the same sample the original survey used) so the
comparison is like-for-like:

- **`stroke_scale_above_floor`** buckets component longer-sides by
  `log2(size)` (25 buckets covers any real scan) and runs the same
  variance-maximising split `otsu` uses — restated rather than shared, since
  `otsu` is specialised to a 256-bin luma histogram — over that histogram
  instead of over pixel luma. The floor is the top of the split's lower
  bucket; the scale is the median longer side of whatever clears it, falling
  back to `stroke_scale` itself when nothing does (one population, nothing to
  separate).
- **`stroke_scale_modal`** is the plain mode of the exact pixel sizes — no
  bucketing, no attempt to separate a noise population first, ties broken
  toward the smaller size for a deterministic answer.

### The measurement

```
TAIYAKU_CORPUS="$PWD/corpus" cargo test -p taiyaku-cli region_experiment --release -- --nocapture
```

| edition | all b/p | above-floor b/p | modal b/p | all scale | above-floor scale | modal scale |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| gotoubun-v01/raw-mono | 0.60 | 0.10 | 1.40 | 3px | 9px | 1px |
| gotoubun-v01/en-mono | 2.50 | 2.50 | 2.50 | 2px | 2px | 2px |
| gotoubun-v01/en-fan | 0.60 | 0.20 | 1.20 | 2px | 12px | 1px |
| kaguya-v01/raw-mono | 0.70 | 0.00 | 0.70 | 2px | 8px | 1px |
| kaguya-v01/en-fan | 0.40 | 0.00 | 0.40 | 2px | 7px | 1px |
| oshinoko-v01/raw-mono | 0.70 | 0.00 | 1.30 | 5px | 23px | 2px |
| oshinoko-v01/en-mono | 1.40 | 0.70 | 1.00 | 2px | 12px | 1px |
| kobayashi-v01/raw-mono | 0.20 | 0.00 | 0.20 | 2px | 15px | 1px |
| kobayashi-v01/en-mono | 0.50 | 0.20 | 0.50 | 2px | 7px | 2px |
| **min / mean** | **0.20 / 0.84** | **0.00 / 0.41** | **0.20 / 1.02** | | | |

(`b/p` is `blocks_per_page` over the 10-page sample; `scale` is the median
`stroke_scale` the rule computed over that sample's pages that had one.)

**The `all` column no longer sums to what the module now measures — added
2026-08-12, not edited into the table.** These nine rows total **76** final
blocks over the 90 sampled pages; L2b Task 5 made the staged pipeline smear
both axes and dedupe with `detect::uncontained`, and it now lands on **73**,
with zero-block pages going 55 of 90 to 58 and the v/h split of survivors
moving **55/21 to 65/8**. `region_experiment.rs`'s module header records that
delta and what it does *not* move — `kobayashi-v01/raw-mono` page 7 reproduces
line for line, and every `MAX_BLOCK_AREA` figure stands — and
`detect::uncontained`'s doc comment carries the account of the axis shift,
which is mostly blocks both smears produce identically. The table above is left
as measured, band-free and one-axis, because that is the diagnosis it belongs
to.

**`blocks_per_page` clears 2:** on none of the nine editions, under any of
the three rules. `all` and `modal` both clear it on exactly the one edition
that already cleared it before this task — gotoubun-v01/en-mono, 2.50 both
ways, because that edition's component counts (46,731–170,231 per sampled
page, 93%–99% under 5px) are so overwhelmingly noise-dominated that both
candidates' derived scale coincides with `all`'s (2px in every case). Outside
that one edition, `modal` raises `blocks_per_page` on 3 of 8 remaining
editions and ties on 4, reaching a best of 1.40 — better than `all`'s
comparable best of 0.70, but not within reach of 2. `above-floor` is worse on
7 of 8: four editions (kaguya-v01 both editions, oshinoko-v01/raw-mono,
kobayashi-v01/raw-mono) return **zero blocks across all ten sampled pages**,
something none of the other two rules does anywhere.

**The v/h split follows the edition on both sides of one work:** `all`
manages it on one, oshinoko-v01 (raw-mono 7v/0h, en-mono 2v/12h, in this
sample) — which is itself worth flagging: the L2A Task 9 brief states this
"is what nothing manages today," but by this measurement `all` already does,
on this sample, before either candidate touches anything. (The original
survey's full-population figures, not the sampled ones, showed the same
edition and kaguya-v01 both nominally correct on both sides, just barely —
`notes.md`'s "The text-block detector..." section already named those two as
"within a few points of even," which a 10-page sample is exactly narrow
enough to flip; kaguya-v01/en-fan does flip here, from 51%h over the full
edition to 100%v over this sample.) Neither candidate reaches even one work:
`above-floor` finds zero, `modal` finds zero — a regression from `all`, not
an improvement, on the one work `all` already had.

### Why `above-floor` overshoots

The candidate's own median scale is the tell: 7px to 23px, against `all`'s
2px to 5px on the same pages — not the fixed-fraction-of-page-width kind of
wrong `stroke_scale`'s own doc comment guards against, but still four to
eleven times larger. Excluding the noise floor does not isolate a clean
glyph population, because on all but one of the nine sampled pages in the
original hand-picked diagnosis (`kobayashi-v01/raw-mono` page 0) there never
was a second, glyph-sized mode to isolate — "everything above the noise" is
itself a continuum running from small type up through panel lines, character
outlines and screentone clumps, and the median over that mixed population
lands well above true glyph size. A larger `stroke_scale` means a
proportionally larger smear gap (`SMEAR_K * scale`) and a stricter
`MIN_BLOCK_STROKE_SPAN` floor at the same time, and the net effect on this
corpus is the same failure the original diagnosis described — most of a page
welding into one blob `MAX_BLOCK_AREA` discards — reached from the opposite
direction: not because the scale collapsed toward the noise, but because
excluding the noise pushed it past glyph size into art size. `modal` avoids
this because it does not try to separate a population at all; it just picks
the single most common exact size, which on this corpus is consistently at
or near the noise cluster's own size (1px on 6 of 9 editions) — closer to
`stroke_scale`'s own failure mode than an escape from it, but the resulting
*smaller* gap happens to weld fewer components together per pass, which is
why its `blocks_per_page` improves on most editions without the axis split
improving at all.

### The outcome

**Neither candidate beats `all` on both checks, so neither is promoted.**
`above-floor` is worse than `all` on `blocks_per_page` on 7 of 9 editions
(4 of those to zero) and finds no work with a correct v/h split on both
sides. `modal` raises the mean `blocks_per_page` from 0.84 to 1.02 but gets
no edition newly above 2 and, like `above-floor`, finds no work with a
correct v/h split on both sides — a regression from `all`'s one. Per the
brief's own rule, this is the task working rather than failing: `all` stays
exactly as written, `detect_page`'s behaviour is unchanged, and the five
constants' doc comments still say "a starting value, not a measurement"
because Task 9 measured the scale rule, not them. `stroke_scale_above_floor`
and `stroke_scale_modal` stay in `detect.rs`, `#[cfg(test)]`-gated so neither
reaches the binary, as the record of what was tried and the instrument that
can re-measure a third candidate without a fourth rewrite of the harness.

**Re-run it rather than taking this table's word**, which is the whole reason
the harness is in-tree rather than thrown away like the one before it:

```sh
TAIYAKU_CORPUS="$PWD/corpus" cargo test -p taiyaku-cli region_experiment \
  --release -- --nocapture
```

About eight seconds, and it prints every figure above — per page, per rule:
component count, the fraction under 5px, the scale each rule derives, the gap,
**blocks surviving each of the four filters in order**, the largest block
`MAX_BLOCK_AREA` discarded, and the axis flagged against the edition's true
reading direction. Absolute `TAIYAKU_CORPUS`, per O30.

*One correction to the rule as it was written.* The promotion rule said a
correct v/h split on both editions of one work is "what nothing manages today".
That is wrong: `all` already manages it on `oshinoko-v01`, which this survey's
own earlier section had recorded as nominally correct while noting it sits
within a few points of even. The error did not change the outcome — neither
candidate reaches even one work — but a promotion rule that misstates the
baseline it is measured against is worth correcting rather than quietly
restating, and both candidates *losing* that work is a sharper result than the
rule as written could have expressed.

## The edition that clears the bar is the one finding no text

The two sections above leave a hole, and it is the kind that becomes a second
unmeasured constant if a redesign is started on top of it. The
screentone-welding account says the detector fails because fine noise
outnumbers type, collapsing `stroke_scale` toward 1–3px and letting
`merge_along` weld a page into one blob `MAX_BLOCK_AREA` discards. Read
straight, that predicts the noisiest edition should fail worst. The noisiest
edition sampled is `gotoubun-v01/en-mono`, at 93%–99% of its components under
5px — and it is the **only one of the nine to clear the design's
`blocks_per_page` bar**, at 2.50 against a bar of 2, under all three scale
rules. Every figure either section reports is a *count* of blocks, and a
count cannot say what a block holds.

So, before any redesign: same harness, same nine editions, same 90 pages, ask
what its surviving blocks have that `gotoubun-v01/raw-mono`'s do not. About
seven seconds.

### What was added to measure it

`region_experiment.rs` now reports, alongside the counts it already had:

- **`SurvivorDiag`** — for every block clearing all four filters: its pixel
  size, its area, how many components it contains, what fraction of *those*
  are under `TINY_STROKE`, their median longer side, and its ink ratio. The
  page's own `under5px` is printed beside each, because the comparison is the
  measurement: a block made of the same material as the page around it is a
  patch of screentone that survived, and a block made of markedly coarser
  components is type.
- **the page's pixel dimensions, and the smear gap as a fraction of them**
  (`gap/w‰`). A gap in pixels means nothing without the page it sits on.
- **the largest single component before any smearing** (`biggest`), against
  the largest block `MAX_BLOCK_AREA` discarded after it. This is what
  distinguishes a page the smear welded from a page that arrived welded.
- **what each of the four filters rejected**, summed over all 90 pages.

`detect_page` is unchanged, no constant moved, and the only threshold used to
classify anything is `TINY_STROKE`, which this module already defined and its
existing `under5px` columns already used. A measurement taken to avoid
acquiring a second unmeasured constant must not acquire one.

### The answer, per edition

```
edition                  blocks     page px  gap/w‰  biggest  of page  strokes  under5px  pg und5  med str  coarse
gotoubun-v01/raw-mono         6    685x1024     8.8    40.6%    10.5%     92.0     42.4%    63.1%    6.0px     5/6
gotoubun-v01/en-mono         25   2147x3056     1.9    50.0%     5.2%   1877.0     90.4%    97.6%    2.0px    9/25
gotoubun-v01/en-fan           6   1070x1600     3.7    38.1%     5.5%    343.0     85.8%    88.0%    2.0px     2/6
kaguya-v01/raw-mono           7    844x1200     4.7    39.9%    10.7%    582.0     63.8%    82.9%    3.0px     1/7
kaguya-v01/en-fan             4    863x1250     4.6    73.5%    10.7%    610.0     91.8%    84.8%    4.0px     1/4
oshinoko-v01/raw-mono         7   1521x2160     6.6    46.5%    11.3%    114.0     50.5%    50.4%    4.0px     3/7
oshinoko-v01/en-mono         14    900x1350     4.4    37.4%     7.3%     46.0     10.9%    76.9%   19.0px   12/14
kobayashi-v01/raw-mono        2   1468x2166     2.7    39.2%    13.4%    266.0     77.4%    77.4%   23.0px     1/2
kobayashi-v01/en-mono         5   2145x3056     1.9    28.3%    13.0%   1180.0     85.6%    96.2%    2.0px     2/5
```

Medians over surviving blocks. `coarse` counts the survivors whose own median
component reaches `TINY_STROKE` — blocks made of something other than the noise.

**The median surviving block of `gotoubun-v01/en-mono` holds 1,877 components
whose median longer side is 2 pixels, on a 2147×3056 page.** A balloon of
English type at that resolution is tens of glyphs at tens of pixels. Two
thousand 2px specks is screentone.

Block by block, its 25 divide on the one threshold this module already had —
whether the block's own median component reaches `TINY_STROKE` — and the two
sides of that split do not resemble each other:

- **16 of 25** hold 280 to 14,556 components at 78%–99% under 5px, median
  contained component 1–3px, against pages that are 96%–99% under 5px. Same
  material as the page. The largest is 913×990px holding 14,556 components.
- **9 of 25** are coarse, and **7 of those 9 are on one page**: page 102's
  17×66 to 23×76px marks, 4–5 components each, median 17–22px, each 0.0% of
  the page.
- **The remaining 2 of those 9** are page 158's 583×323 block (18 components,
  0% tiny, median 25px) and page 177's 396×488 (60 components, 0% tiny,
  median 35px).

### What the pixels said, and the noun this section had wrong

The paragraphs above were written from component statistics alone, and a first
draft of this section named the 16 "dense fields of tone" and the 7 "captions
or page numbers". Both nouns were wrong, and it took ten minutes with
`taiyaku regions --write` against a scratch sidecar and a cropping script to
find that out — which is the same shape of error the original diagnosis made
when it said "most components are art" and the measurement answered "most
components are finer than any glyph". The statistic was right both times. What
it was a statistic *of* was not.

Ten blocks were cropped from the source archives at the fractions
`taiyaku regions` itself wrote, and looked at:

| block | the statistic | what it is |
| --- | --- | --- |
| gotoubun p27 b1, 913×990 | 14,556 comps, 98% tiny | a whole **panel** — two characters, speed lines, four text boxes |
| gotoubun p83 b1, 1119×578 | 13,203 comps, 96% tiny | a whole **panel** — a stairwell, screentone, three balloons |
| gotoubun p139 b1, 1253×694 | 13,441 comps, 99% tiny | a whole **panel** — a figure on a bench, tone, one balloon |
| gotoubun p158 b2, 582×323 | 18 comps, 25px | a **panel** — one balloon plus a black silhouette |
| gotoubun p177 b0, 396×488 | 60 comps, 35px | a **caption box**, cleanly bounded — the one clean text region seen |
| gotoubun p102 b3, 16×82 | 4 comps, 17px | a **letter fragment**, a vertical slice of one word |
| oshinoko p214 b0, 669×137 | 15 comps, 109px | the **title logo** |
| oshinoko p214 b1, 604×67 | 25 comps, 21px | a **credits line**, "Aka Akasaka × Mengo Yokoyari" |
| oshinoko p101 b0, 735×120 | 46 comps, 19px | a **caption box**, two lines of English |
| oshinoko p78 b0, 217×460 | 79 comps, 19px | a **panel** — one balloon plus art |

**The detector is not finding text blocks. On this edition it is finding
panels.** That is a sharper statement of the same failure and it explains every
figure at once: a panel on a multi-panel page is comfortably under
`MAX_BLOCK_AREA`'s 0.15, so `blocks_per_page` 2.50 is roughly panels per page
rather than balloons per page; a panel has no reading direction, so the axis is
wrong on 9 of 10 pages; a panel's component population is its screentone, so
the median contained component is 2px; and a panel is 5.2% of a page, which is
the median area reported. The blocks are not patches of tone that happened to
survive — they are panels that swallowed the dialogue along with everything
else.

**The same page shows the opposite failure too.** Page 102's seven "coarse"
blocks are not captions. They are vertical slices of a single piece of display
text — 16×82px of one word — so the same edition at the same gap over-merges
whole panels and under-merges one line of lettering. Both are wrong, in
opposite directions, on one page.

**The counter-example survives the check and needs one qualification.** Of the
four `oshinoko-v01/en-mono` blocks looked at, three are cleanly bounded text —
a logo, a credits line, a caption box — and the fourth is a panel. So that
edition is not uniformly finding text either; it is finding text far more often
than `gotoubun-v01/en-mono`, which finds it once in the six examined.

**And the discriminator needs its own qualification, now that it has been
checked against pixels rather than only against itself.** A coarse in-block
median does separate panel-blocks from everything else — 3 of 3 fine-median
blocks are panels, and no coarse-median block is one of the giant welds. But
"coarse" alone does not mean text: page 102's letter fragments are coarse by
median and are sub-glyph by every other measure. Component *count* is what
separates those — 4 to 5 for a fragment against 18 to 60 for a caption box.
Neither figure is proposed here as a constant; what this establishes is that a
promotion rule needs at least two of them and that `blocks_per_page` is neither.

**And the axis says the same thing independently.** 23 of those 25 blocks are
tagged `Vertical` on an English edition — wrong on 9 of the 10 sampled pages.
The one edition that clears the count bar is the one that gets the direction
most wrong. That holds even where the block *is* text: page 177's caption box
is eight lines of horizontal English and is tagged `Vertical`.

### The counter-example is the edition that fails the bar

`oshinoko-v01/en-mono` scores 1.40 `blocks_per_page`, below the bar, and its
survivors are the healthiest in the corpus: median 46 components at 10.9%
tiny, median contained component 19px, against a page 76.9% tiny — **12 of its
14 blocks coarse**. Their shapes read as English typesetting: 669×138,
605×68, 384×38, 232×38, 736×120, 817×112. 12 of the 14 are tagged
`Horizontal`, correctly. And `oshinoko-v01` is exactly the one work whose v/h
split `all` already gets right on both sides.

So the two editions are ordered *opposite* ways by the count and by the
composition. Nothing about `blocks_per_page` sees that.

### Why the noisiest edition scores best, which is the mechanism correction

Not that it escapes the welding. That it welds at a proportionally finer gap.

`gotoubun-v01/en-mono` derives the same noise-collapsed 2px scale as everything
else, giving the same 4px gap — but on a page 3.1× wider than
`gotoubun-v01/raw-mono`'s. As a fraction of page width that is **1.9‰ against
5.8‰–14.6‰**, up to 7.7× finer. Where raw-mono's proportionally coarse gap
welds a page into a single over-large blob and `MAX_BLOCK_AREA` throws the
whole page away — 6 of its 10 sampled pages return zero blocks — en-mono's
finer relative gap chops the same screentone field into *several mid-sized*
fragments, each under 0.15 of the page.

`MAX_BLOCK_AREA` is the only filter with an opinion about size, and mid-sized
noise is exactly what it is not looking for. The edition scores best because
its noise is cut into pieces the right size to pass, not because more of its
text is found.

**The relative gap explains the composition, and it does not on its own
explain the count.** The two finest-gap editions in the corpus are
`gotoubun-v01/en-mono` and `kobayashi-v01/en-mono`, both at 1.9‰ on the same
2145×3056-class material, and they are also the two whose survivors are most
made of noise — 90.4% and 85.6% of contained components under 5px, medians of
1,877 and 1,180 components a block. That much the gap accounts for. But they
score 2.50 and 0.50 `blocks_per_page`, and nothing measured here says why:
`gotoubun-v01/en-mono` simply arrives at `MAX_BLOCK_AREA` with more mid-sized
fragments per page — 1 to 6 with a median of 5, plus 91 on page 102 — where
`kobayashi-v01/en-mono` arrives with 1 to 4, median 3. A finer relative gap is
what makes surviving-block-shaped noise *possible*; how much of it a given page
produces is not settled by this measurement, and claiming otherwise would be
the one-cause story this section exists to take apart.

(Two of `gotoubun-v01/en-mono`'s ten sampled pages are 4294×3056 — joined
spreads, at 0.9‰ — and both return zero blocks, so they are not what carries
the 2.50. The eight single pages are the 1.9‰ figure quoted above.)

### Half the blobs were never welded by the smear

Comparing `biggest` — the largest single connected component, before
`merge_along` runs — against the largest block `MAX_BLOCK_AREA` discarded:

```
of the 89 pages where MAX_BLOCK_AREA discarded something, the largest thing
discarded was already one connected component before any smearing on 47 of
them (within 1 point of page area)
```

On `gotoubun-v01/en-mono` it is **7 of 10**, and on those seven the two figures
are equal to the printed decimal: 91.9/91.9, 53.4/53.4, 23.0/23.0, 68.6/68.6,
34.4/34.4, 50.0/50.0, 44.9/44.9.

The section above says "screentone packs ink densely enough that a few pixels
of tolerance is already sufficient for `merge_along`'s repeated, transitive
passes to weld thousands of them into one component spanning most of a panel
or the whole page." On about half the corpus that is not what happened. The
page-spanning component came out of `otsu` and `components` already formed,
and no gap — including a gap of zero — would have prevented it. That is a
genuine hole in the stated mechanism and it bears directly on the
denoise-before-labelling route: **denoising the component population that
feeds `stroke_scale` cannot touch a blob that exists before the scale is ever
computed.** Reaching those pages means changing what `components` returns.

The sentence stands for the other half — 42 of 89 pages, where the discarded
block is materially larger than any single component that preceded it, so the
smear did build it.

### One of the four filters is the detector, and one has never fired

```
MIN_STROKES              rejected   121 blocks, on 21 of 90 pages
MIN_BLOCK_STROKE_SPAN    rejected     2 blocks, on  1 of 90 pages
MAX_BLOCK_AREA           rejected   166 blocks, on 89 of 90 pages
MIN_INK                  rejected     0 blocks, on  0 of 90 pages
```

**`MIN_INK` has never rejected anything.** Measured ink ratios across all 76
surviving blocks run 0.38 to 1.89 against a floor of 0.05 — 7.6× to 38× it —
and 61 of the 76 exceed 1.0, which is the constant's own doc comment made
numeric: it sums stroke *bounding boxes*, which overlap heavily, so a block of
dense fine components scores far above 1.0 while covering nothing like that
much paper. As written it cannot fire on this material.

`MIN_BLOCK_STROKE_SPAN` fires twice in 90 pages. `MAX_BLOCK_AREA` fires on 89
of 90 — it is not a backstop, it is the detector. That sharpens O33, which
records that `MAX_BLOCK_AREA` and `MIN_BLOCK_STROKE_SPAN` reject nothing *in
any test*: against the corpus the dead one is `MIN_INK`, which O33 does not
name, and `MAX_BLOCK_AREA` is the opposite of dead. O34 is opened for
`MIN_INK`; O33 is annotated rather than closed, because the gap it names is in
the tests and this measurement is not one.

### What this changes for the redesign, which is the point of running it first

**`blocks_per_page` cannot be the acceptance bar on its own.** The only
edition that has ever cleared it clears it with screentone fragments, and the
edition whose blocks are most plausibly text fails it. A promotion rule stated
in `blocks_per_page` would promote a candidate for cutting noise into
better-sized pieces, and the fixed-before-measuring discipline that saved Task
9 does not help if the thing fixed in advance is the wrong quantity.

What separates the two cases is already in hand and cost no new threshold to
measure: the block's own median component against `TINY_STROKE`, and against
its page's. 12/14 for the edition that looks right, 9/25 — 7 of them on one
page — for the edition that scores best. Whether a detector should *filter* on
that, and at what value, is a redesign decision and is not made here; what is
established is that a rule stated in counts alone cannot see the difference.

Nothing was redesigned here and nothing was tuned. `detect_page` is byte for
byte what it was, all five constants still say "a starting value, not a
measurement", and the denoise-before-labelling route is still the indicated
one — with a mechanism that now accounts for the edition that contradicted it
and for the half of the corpus where the blob predates the smear.

### What this does not claim

The same 90 pages as the section above, from the same nine editions of four
works, sampled the same way — not the corpus, not random.

**Ten blocks were looked at, out of 76.** Six from `gotoubun-v01/en-mono` (of
25) and four from `oshinoko-v01/en-mono` (of 14); nothing from the other seven
editions, and nothing from any raw edition. They were not chosen randomly
either: they were picked to span the statistic — the three largest
component counts, the two coarsest medians, one of the seven small blocks on
page 102 — so they test the reading at its extremes rather than sample it.
"Panel" and "text region" are what those ten are; the other 66 are still read
off component statistics, and the claim that the 16 fine-median blocks are all
panels rests on three of them having been seen.

What does not rest on inference is the ordering, which was the question: the
two editions rank opposite ways under the count and under the composition, the
composition ranking agrees with the axis result and the count ranking does not,
and the edition that clears the bar was seen to be finding panels while the
edition that fails it was seen to be finding caption boxes.

An earlier draft of this section claimed "no page was looked at" and named the
16 blocks "fields of tone" on the strength of that. Looking cost about ten
minutes and changed the noun; it is recorded here rather than quietly fixed
because the measurement was *right* and its description was wrong, which is the
failure mode this whole line of work keeps finding.

Re-run it rather than taking the tables' word:

```sh
TAIYAKU_CORPUS="$PWD/corpus" cargo test -p taiyaku-cli region_experiment \
  --release -- --nocapture
```

About seven seconds, and it prints every figure above.

**The crops are reproducible too, and need nothing added to this repository.**
`taiyaku regions --write` puts a `<stem>.regions.json` beside its *sidecar*, so
copying the sidecar to a scratch path keeps every write out of `corpus/`, which
is the discipline the rest of this file follows for corpus material:

```sh
cp corpus/gotoubun-v01/taiyaku.json /tmp/scratch/gotoubun-v01.json
taiyaku regions "$PWD/corpus/gotoubun-v01/en-mono"/*.cbz \
  --sidecar /tmp/scratch/gotoubun-v01.json --write
```

That run reprints this section's `regions:` line exactly — 348 blocks, v=269,
h=79, `blocks_per_page` 1.86 — and the document it writes carries every block's
`x`/`y`/`w`/`h` in page fractions, which is all a crop needs.
`oshinoko-v01` has no sidecar in the corpus and wants the `align --write` from
"The survey ran over four works" first; that command still reports the 6 runs
and 216 whole pairings recorded there.

## Two pages that arrive together, and a pairing that agrees across editions

Two reader complaints about `two` mode, raised from real reading on
`corpus/gotoubun-v01` and grouped here because both turn out to be the same
kind of defect: a rule that was correct about one page at a time and had never
been asked about two.

- "When seeing 2 pages at a time, especially with larger images, they don't
  show up at the exact same time when scrolling and it's distracting."
- "The editions don't always line up exactly. There's some pages pretty early
  on in particular where the english official and english fan subbed get off by
  1 page."

Everything below was measured against `corpus/gotoubun-v01`, served by
`taiyaku serve` from a scratch sidecar with absolute member paths, behind Vite's
`preview` proxy on the production bundle — so the numbers are minified-build
numbers, not dev-server ones.

### The halves were never gated together, and the old comment said so

`decode.ts`'s gate is per image and always was: `presentWhenDecoded` holds one
`src` until it has decoded. `App.tsx` carried a comment stating plainly that
`two` mode's cells "each run `presentWhenDecoded` independently, so an ordinary
pair's two halves can present at different times", and arguing for leaving it —
gating would make the faster half wait on the slower one "for a mismatch nothing
has measured a reader noticing". It named the thing that should overturn it:
"a measurement showing otherwise is the reason to revisit this, not a guess made
ahead of one."

**The measurement, taken with a `MutationObserver` on both cells' `src`
attributes**, with the cache warm and prefetch on, `gotoubun-v01` `en-mono`:

| Pace | Turns | Median gap | Worst | Turns tearing by >1 frame |
| ---- | ----: | ---------: | ----: | ------------------------: |
| 700 ms between turns | 8 | 89.7 ms | 158.4 ms | 8 of 8 |
| 120 ms between turns | 12 | 213.8 ms | 1276.7 ms | 4 of 4 measured |

Not a rare race: **every turn tore**, and the smallest gap measured at a slow
pace was 51.2 ms, over three frames. The complaint's "worse when going fast" is
in the second row, where one pair took 1.28 s to close.

`cohort.ts` is the fix — a barrier over decode jobs, running the existing queue
*inside* it so the concurrency cap still governs the decodes and the barrier
only governs when their results are shown. **Membership closes at the end of the
current task rather than on a count**, because nothing knows in advance how many
images a turn will change: React flushes a commit's effects in one synchronous
pass, so both cells' `useEffect` bodies join before the deferred close can fire,
and a turn that changes only one cell forms a cohort of one. `defer` is a
parameter so a test can close it by hand.

Re-measured through the same observer, after: **0.0 ms on all 8 slow turns and
all 3 fast ones**, 0 tearing by more than a frame. Both halves land in one
commit, so they land in one paint.

The latency the old comment was protecting is real and is now paid — a pair
appears when its slower half is ready. It was always being paid: the reader was
looking at the slower half's old page for exactly that long either way.

### The "off by 1" is the pairing parity, not the aligner

The obvious reading of the second complaint is an alignment defect, so that was
checked first, and it is not one.

**The aligner is clean.** A shift test over every paired slot of every alignment
— score the pairing the solver chose against the same `from` slot's score
against `to ± 1` — was run over 12 alignments across 9 volumes and 4 series,
built with `align --write` into a `taiyaku.check.json` beside each volume so no
existing sidecar was touched. **Zero pairings are improvable.** The first
version of that scan reported four (`gotoubun` v01/v03/v08 at raw slot 193,
`kaguya-v03` at 208), all with large margins — 0.762 against 0.891 and the
like — and every one dissolved on adding the one-to-one constraint the scan had
been missing: at a volume's tail several near-identical end pages all match one
`to` page strongly, the solver gives it to whichever scores highest, and the
loser then looks improvable against a slot that is already taken. A scan that
does not model the constraint the thing it is auditing is built on reports its
absence as a defect.

**The defect is in `pairing.ts`.** `groups` anchors `two` mode's pairing at slot
0 of whatever edition it is handed, and nothing carried that choice across a
swap. Editions disagree about how many pages precede the story, and every joined
spread present in one and not the other flips the relative parity again from
there on — so the same reading position drew `(n-1, n)` in one edition and
`(n, n+1)` in the next. One page on screen was right, the other was its
neighbour. Simulating `groups` and the sidecar's routes over every ordered pair
of `gotoubun-v01`'s three editions:

| Route | Groups compared | Pair survives the swap |
| ----- | --------------: | ---------------------: |
| `raw-mono` → `en-mono` | 97 | 2 |
| `en-mono` → `raw-mono` | 97 | 1 |
| `raw-mono` → `en-fan` | 96 | 72 |
| `en-fan` → `raw-mono` | 98 | 74 |
| `en-mono` → `en-fan` | 94 | 25 |
| `en-fan` → `en-mono` | 98 | 32 |

206 of 580. The two English editions — the pair a reader is likeliest to
compare, and the one the complaint names — are torn on roughly seven groups in
ten, and *intermittently*, which is why it read as "some pages pretty early on"
rather than as a constant offset: `en-mono` is out of phase with the hub almost
everywhere, `en-fan` in stretches, so the two agree exactly where those
stretches coincide.

Confirmed in the browser before anything was built, by reading the drawn page
indices out of the DOM at one position: `raw-mono` drew pages 24–25, `en-fan`
drew 62–63 (their exact counterparts) and `en-mono` drew 23–24, where 22–23 is
what the alignment says.

### Deriving the parity from the hub — the first rule, and why it was not enough

*Kept as written, because the reader found its limit within the hour and the
limit is the interesting part. The rule that ships is the section after this
one.*

`alignedPairs.ts` computes the hub's own grouping, maps each group through the
alignment, and takes the slot each lands on as a position where that edition
must open a pair. Those go in as more of `groups`' existing `breaks` — the
`regroup` module's parity marks — so the grouping itself needed no new
mechanism.

**The hub and not the authority**: `ReadingState.authority` moves to whichever
edition the reader last navigated in, so deriving from it would re-group every
other edition whenever the reader turned a page somewhere new. The hub is fixed
by the sidecar and every alignment is stored against it.

Same simulation, after: **572 of 598**, and with all five editions of
`gotoubun-v01` in the document, 1918 of 1992. `oshinoko-v03` goes 172/521 to
554/573.

**The cost, stated rather than buried.** Every position where the parity
genuinely has to change costs one group drawn alone, because that is what
restarting a pair means. On `gotoubun-v01` that is 14 solo groups added to
`en-mono`'s 104 and 8 to `en-fan`'s, clustered at the front matter and the
joined spreads, and a reader who never swaps pays it too. It was taken
deliberately over the alternative: a solo page says "there is nothing here to
pair this with", where a silently mismatched pair said nothing at all.

Confirmed in the browser on the built bundle, walking forward and reading all
three editions at each position. At the position measured above, `raw-mono`
8–9 now draws against `en-mono` 6–7 and `en-fan` 46–47 — the alignment's own
answer, in all three.

### The reader found the 4% that was left, at one press

"There's still some mis-alignments, like slot 48, p46." `en-mono` slot 48 is
page 46's right half — the rule above scored 96%, and this is what the other 4%
looks like from a chair.

**The hub cannot follow a boundary only a spoke has.** `en-mono` joins pages 45
and 46 back to back, which the raw carries as four ordinary pages. A joined
spread is one image, so `groups` gives it its own group unconditionally and no
break can halve it — meaning `en-mono` slots 46–47 and 48–49 are each one group
whatever the derivation says. Their counterparts are raw slots 47–48 and 49–50.
But the rule above derives breaks for *spokes only* and leaves the hub on its
natural parity, where raw pairs `(46,47)` and `(48,49)`: slots 47 and 48 sit in
different raw groups, so **no raw group is the counterpart of that spread.**
Standing on it and swapping drew raw 47–48 where 48–49 was right, and again one
group later. Read straight off the reader:

| `en-mono` group | raw drawn | raw correct |
| --------------- | --------- | ----------- |
| page 45 spread, slots 46–47 | 47, 48 | **48, 49** |
| page 46 spread, slots 48–49 | 49, 50 | **50, 51** |

**Pair boundaries are a property of the work, not of an edition** — which is the
sentence the first rule was missing. Wherever *any* edition joins a spread, that
is a boundary the work has, and every other edition has to honour it or it
cannot draw the counterpart pair. So: collect each edition's forced boundaries
(a spread's first half, and the slot after it), map them into the hub's slot
space, union them, group the hub with *those*, and derive every edition —
**the hub included** — from that one grouping.

**Better on both axes, which is not what a fix for a corner case usually
does.** Across eleven volumes in four series:

| Volume | anchored per edition | hub-derived | unioned |
| ------ | -------------------: | ----------: | ------: |
| `gotoubun-v01`, five editions | 42% | 96% | **99%** |
| `gotoubun-v03` | 42% | 97% | **98%** |
| `gotoubun-v08` | 2% | 93% | **100%** |
| `kobayashi-v08` | 1% | 99% | **100%** |
| `kaguya-v08` | 1% | 100% | 100% |
| `oshinoko-v08` | 98% | 100% | 100% |
| `kaguya-v01` | 69% | 99% | 99% |
| `kaguya-v03` | 99% | 100% | 100% |
| `kobayashi-v03` | 1% | 99% | 99% |
| `oshinoko-v03` | 33% | 97% | 97% |

Never worse, strictly better on five. **And it needs fewer solo groups to do
it**, because the hub can now move rather than every spoke bending around it:
`gotoubun-v01`'s `en-mono` goes 14 → 4, `en-fan` 12 → 6, and `gotoubun-v08`'s
`en-mono` 16 → 2. The cost paragraph above is superseded by that: what a reader
who never swaps pays is 4 solo groups in `en-mono`'s 104, not 14.

Confirmed in the browser at the position reported. Both spreads now answer with
the raw pair the alignment names, and `en-fan` 87–88 alongside.

**What the first rule got right, and why it is still most of this.** Deriving a
grouping from the alignment and feeding it in as `groups`' existing `breaks` is
unchanged; so is the choice of the hub over `authority`. What changed is one
step — the hub is grouped from the unioned boundaries rather than from its own
parity — and `alignedBreaks` then treats the hub as an edition like any other
rather than as the fixed point. A rule with a fixed point that cannot move is
what left the 4%.

### Reproducing both

The decode measurement needs a browser; the two simulations do not, and read
the fingerprint cache rather than decoding anything:

```sh
./target/release/taiyaku align corpus/gotoubun-v01/raw-mono/*.cbz \
  -- corpus/gotoubun-v01/en-mono/*.cbz --cache corpus/gotoubun-v01/.taiyaku-cache
```

populates `.taiyaku-cache` for a pair; with the cache warm, the shift scan and
the parity simulation are arithmetic over `taiyaku.json` and those hashes.
`web/src/alignedPairs.test.ts` pins the derivation itself, and
`web/src/cohort.test.ts` the barrier.

## Why page turns were slow, and the two things that were not the cause

Reported as "switching pages sometimes seems slow", on `gotoubun-v01` `en-mono`
in `two` mode. Measured keypress-to-both-halves-on-screen in the browser against
the production bundle: **median 110 ms, p90 199 ms, worst 271 ms**, with the
first few turns of a run at 9–14 ms and everything after that far worse.

### The cause: prefetch counted slots where the reader steps by groups

`modules/prefetch.ts`'s `window` reaches `ahead` slots forward and `behind`
back. An arrow steps by what is *drawn* — `pairing.ts`'s `stepToGroup` — and in
`two` mode a group is two slots. Standing on group `[8, 9]` with the shipped
`ahead: 2`, the window named slots 7, 9 and 10; the next group is `[10, 11]`, so
**slot 11 was never prefetched**. Watched on the wire with a
`PerformanceObserver` over resource entries, every forward turn requested
exactly one page only after the keypress, and it was the second half of the
group being turned to, every time:

| from | to | requested after the keypress |
| ---- | -- | ---------------------------- |
| 14, 15 | 16, 17 | 17 |
| 16, 17 | 18, 19 | 19 |
| 18, 19 | 20, 21 | 21 |
| 20, 21 | 22, 23 | 23 |

**It is the same defect as the tearing complaint.** Before `cohort.ts` the warm
half painted at once and the cold one arrived ~90 ms later, which is exactly the
89.7 ms median tear that was measured and fixed; gating the pair together turned
one missing fetch into a wait instead of a tear. `groupWindow` reaches whole
groups and closes both: median 110 ms → **14 ms**.

### Reaching further then made it worse, which is the interesting half

`groupWindow` with the old depths reaches 6 slots (2 groups ahead, 1 behind)
where `window` reached 3, and a tail appeared: p90 232 ms, worst 335 ms, 29% of
turns over 100 ms. A decoded `en-mono` page is 2147×3056×4 ≈ **26 MB**, and a
joined spread twice that, so a wide window churns the browser's own
decoded-bitmap cache and evicts the pages the reader is about to arrive at.
**Prefetch competing with itself.** Twenty turns per setting:

| window | median | p90 | worst | over 100 ms |
| ------ | -----: | --: | ----: | ----------: |
| `ahead: 2, behind: 1` — 6 slots | 14 ms | 232 ms | 335 ms | 29% |
| `ahead: 1, behind: 0` — 2 slots |  4 ms | 125 ms | 182 ms | 10% |
| `ahead: 1, behind: 1` — 4 slots |  4 ms |  70 ms | 216 ms | 5–20% |

The same effect from the other side: raising `maxConcurrent` from 3 to 6 made it
*worse* (5 slow turns in 16 against 3 in 14), not better. More decode in flight
is more eviction, not more readiness. The defaults are now one group each way.

### Two hypotheses that were wrong, and one dependency that was backed out

Kept because each was plausible, each was tested, and each failed.

**Retaining the prefetched images does not retain their bitmaps.** Prefetch
decodes into a throwaway `Image()` that is garbage once `decode()` settles, so
the obvious fix was an LRU of live references. Measured: decode 26 pages, then
re-decode the first — **72.5 ms with no references held, 160.4 ms holding all
26**. Chrome keeps the encoded bytes and discards the raster for a detached
image regardless, and 26 × 26 MB of retained elements only added pressure.
Mounting the image in the DOM instead (2px, near-transparent, not
`display: none`) measured 82.7 ms against 88.4 ms detached — no retention there
either.

**Progressive JPEG is the real decode cost, and transcoding is still not worth
it.** `en-mono` ships progressive; `en-fan` and `raw-mono` are baseline, which
is why only that edition felt slow. Decode, bytes already cached:

| encoding | pixels | median decode |
| -------- | -----: | ------------: |
| progressive, as released | 6.6 MP | 76.7 ms |
| the same image, baseline | 6.6 MP | 26.4 ms |
| baseline, downscaled to 1024 wide | 1.5 MP | 20.7 ms |

Resolution is the minor factor — 4.4× fewer pixels saves 6 ms where dropping
progressive saves 50. A lossless `jpegtran`-style transform was built against
`turbojpeg` (`mozjpeg`'s safe API is pixel-level only, and this workspace sets
`unsafe_code = "forbid"`), and **backed out**. Three reasons, in order: the
transform costs a **median 86 ms** in process over 24 real pages — more than the
50 ms it saves — so it needs a disk cache plus a cold-path policy or the first
read of every page gets *slower*; it needs `libturbojpeg` in CI; and Komga,
which is the comparison that prompted this, does not do it at all — it "passes
the raw image data without decoding" when the client can decode the format, so
it serves the identical bytes and pays the identical decode.

What is left is a tail: with the narrow window the median turn is 4 ms and a few
turns in twenty still cost 100–300 ms, when the browser has evicted a 26 MB
bitmap. That is a property of large progressive scans, and no client-side
arrangement measured here changes it.

## A fast flick did work it had already thrown away

"Performance switching pages fast still seems kinda bad on the large image
edition — but maybe I'm just trying to switch too fast." Not too fast: a real
defect, and one the reader had already half-diagnosed as "the prefetch just
catches up".

**Two wrong readings on the way to it, both from the measurement rather than
the code.** The first burst test polled for the display to go quiet and gave up
after 500 ms of no change, which under a flick exits *before* the first repaint
— so it read a mid-flick frame as final and reported that sixteen presses had
advanced three pages. That looked like dropped keypresses. Reading the position
the reader had actually committed settled it: sixteen presses at 25 ms advanced
the state **29 slots** and the display **28 pages**. Nothing was dropped, and
the second reading — that the display merely lagged — was right.

**What the lag was.** A sixteen-press flick lands on one group: **two pages**.
It requested **twenty-nine** — every page from 3 to 31 — and twenty-one of those
requests were issued *after* the reader had stopped pressing. `decode.ts` asked
`stale()` only once the decode had finished, to decide whether to *show* the
result; nothing asked before starting one. So every superseded frame was fetched
in full (1.8 MB) and decoded in full (~77 ms) before the queue reached the page
the reader was looking at. Catch-up after the last keypress: **1595 ms** at
25 ms/press, **708 ms** at 60 ms. `raw-mono` measured 0 ms at the same rates,
because pages that decode in 8–12 ms never queue up in the first place — which
is why only the large edition felt broken.

**The fix is where the question is asked, not a new mechanism.** `stale` moves
inside the job, so it is answered when the queue *reaches* the work rather than
when it was enqueued — at enqueue time nothing is ever stale. A superseded job
resolves immediately and never calls `makeImage`, so it costs neither the decode
nor the fetch: `decodeTo` is what assigns `src`, and no assignment means no
request. No signature changed.

The prefetch effect needed the same thing for the same reason and did not have
it at all: it fires once per group crossed, so a flick queues a fresh window per
intermediate position. Each job now compares against the position it was queued
for and drops if the reader has moved — strict equality rather than proximity,
since the position they moved *to* has already queued its own window through the
same effect, so nothing is lost.

| | before | swap queue fixed | both fixed |
| ---------------------------- | -----: | ---------------: | ---------: |
| catch-up, 16 presses @ 25 ms | 1595 ms | 586 ms | **524 ms** |
| catch-up, 12 presses @ 60 ms |  708 ms | 378 ms | **383 ms** |
| pages fetched (2 needed)     |      29 |     29 |     **17** |

Ordinary reading is unregressed and its tail is better: median 5 ms, p90 17 ms,
against a median 4 ms and p90 282 ms before. The seventeen remaining requests
are windows already in flight when the reader moved; `<img>.decode()` cannot be
aborted without discarding the bytes it has, so those are not reclaimable
without a different mechanism.

**What this deliberately does not change.** The screen still holds the outgoing
page through a flick and then jumps once, rather than flipping through every
page crossed. That is `decode.ts`'s own rule — a fast flick's intermediate
targets never flash onto screen after the reader has moved past them — and it is
why the fix shortens the wait rather than filling it.

## Twenty times the pixels, and the sluggishness that survived three fixes

"The large images still seem sluggish to me" — after the pairing, the group
window and the stale-job fixes had each measurably helped. They had, and the
reader was still right, because none of them touched the actual cost.

**The measurement that had been missing all along is one line.** The `<img>`
elements report `naturalWidth`/`naturalHeight` against the box they are drawn
into:

```
natural 2147x3056   drawn 693x475
```

6.56 megapixels fetched and decoded to fill 0.33 — **twenty times the pixels
that reach the screen**, in a 1385×512 window at `devicePixelRatio` 1. There is
no display on which those pixels are visible.

**And it reverses what this file recorded earlier.** The progressive-JPEG
section above concluded that resolution was the minor factor — that dropping
progressive saved 50 ms where downscaling saved 6. That came from a noisy run
which put a 1024-wide decode at 20.7 ms. Measured cleanly, with the bytes cached
so the timing is decode alone:

| fetched at | pixels | decode | decoded bitmap |
| ---------- | -----: | -----: | -------------: |
| 2147, progressive (as released) | 6.56 MP | 74.5 ms | 26.2 MB |
| 2147, baseline | 6.56 MP | 26.5 ms | 26.2 MB |
| 1600 | 3.64 MP | 14.9 ms | 14.6 MB |
| 1024 | 1.49 MP | 7.3 ms | 6.0 MB |
| 700 | 0.70 MP | 4.3 ms | 2.8 MB |

Full → 1024 is 10×; progressive → baseline is 2.8×. **Resolution is the
dominant factor and the earlier entry had the ranking backwards**, which is what
sent that work toward a transcode that was then backed out. The transcode's own
verdict stands — it costs more than it saves — but the reasoning that reached
for it was wrong twice over.

**The bitmap column is the other half, and it closes the tail nobody had
closed.** The residual "a few turns in twenty at 100–300 ms" recorded above is
the browser evicting 26 MB bitmaps under pressure. At 2.8 MB they are not worth
evicting, so those turns stop happening rather than becoming rarer — which is
why this fixes something three previous passes could only mitigate.

### What shipped

`?w=` on the page route, and `pageSize.ts` deciding what to ask for.

**Never upscale**: `fit_at` reads the image *header* only — `into_dimensions`
parses to the SOF marker and stops — and hands back `None` when the page is
already within the requested edge, so the original bytes are served untouched
and nothing is cached. Verified byte-identical on `raw-mono` (685×1024) at
`?w=1024` and `?w=2048`. That is what makes the parameter safe to apply to every
edition rather than only the large one: on this corpus `raw-mono` and `en-fan`
are frequently already smaller than a viewport asks for, and re-encoding them
would cost quality for no pixels gained.

**The long edge is in the `ETag`.** One page now has a representation per size,
and a tag naming only member and offset would let a client holding the 1024px
version be answered `304` when it asked for 2048 — a page that looks soft for a
day with nothing to point at. The full-size request keeps the tag it always had,
so a client that cached a page before this existed still validates.

**A ladder, not the exact size** — 512/768/1024/1536/2048/3072 — because the
server caches one file per (member, page, long edge) and a reader dragging a
window edge would otherwise mint an entry per pixel of width.

**Zoom needs no branch of its own.** Magnification multiplies the pixels wanted;
once that exceeds the top rung, `requestedLongEdge` answers `undefined` and the
original is fetched. One rule covers fit-whole, a nudge above 1×, and L1's 8×,
and a page fetched for fit-whole can never be what a zoom draws from.

### Measured after, in the reader

| | before any of this | after prefetch + stale fixes | after resize |
| ----------------- | ----: | ----: | ----: |
| steady median     | 110 ms | 4 ms | 10 ms |
| steady p90        | 199 ms | 282 ms | **15 ms** |
| steady worst      | 271 ms | 335 ms | **35 ms** |
| turns over 100 ms | most | ~20% | **0 of 18** |
| flick catch-up    | — | 524 ms | **283 ms** |

The median moved 4 → 10 ms and that is the trade, taken deliberately:
consistently 10 ms beats usually-4-and-sometimes-300. Bytes on the wire fall
with it — `en-mono` page 60 is 1,783,784 bytes at full size and 105,139 at
`?w=768`.

**One cost is real and not yet paid down.** The first read of a page at a new
size costs the server a decode and resize — measured at a 157 ms median across a
cold pass, against 10 ms warm — so a volume opened for the first time is slower
until its cache fills. Prefetch reaching one group ahead covers ordinary
reading, since generation is faster than a reader turns pages; a fast flick
through cold pages is not covered. Pre-warming a work's window on load would
close it and is not built.

## The flicker was the layout running ahead of the pixels

Reported alongside the flick complaint: "there's also some flickering, and the
page that we were on when flicking fast seems to linger". Two different things,
and only the first is fixed here.

Sampling the page area every 16 ms through a sixteen-press flick showed the
artwork sitting still on pages 1–2 for the whole flick while the **cell
geometry** changed three times in 114 ms — one wide cell, two half-width cells,
one, two. **Zero blank frames** at any point, so nothing was half-drawn; the
frame around the pictures was flapping.

The cause is that the two follow different clocks. A keypress moves
`ReadingState` at once, so `grouped`, `current` and therefore
`cellsFor(current)`'s `wide` all change on the press — while each cell's picture
waits for `decode.ts`'s gate. Crossing a solo group (the aligned pairing adds
four to `en-mono`) collapses the layout to one wide cell and back, per press,
under artwork that has not moved.

**`wide` is readable off the screen rather than predicted**, which is what makes
the fix small. `cellsFor` hands both cells the same URL exactly when a group is
wide — a joined spread's two slots share one page, and a solo slot is mirrored
into the second cell — so "both cells are drawing the same source" *is* wide.
`PageView` now reports what it is actually drawing (`onShown`), `App.tsx` holds
those two strings, and the geometry compares them. No new gate, no added
latency: the layout simply stops running ahead. The target's own `wide` is still
used for the first render, before either cell has reported anything.

Measured on the same flick, before and after:

| | layout changes | frames drawn |
| - | -------------: | ------------ |
| before | 3 | `[0]/1  [2,1]/2  [2]/1  [2,1]/2  [29,28]/2` |
| after  | 1 | `[0]/1  [2,1]/2  [29,28]/2` |

**The linger is untouched and is a different problem.** The screen still holds
one spread through the flick and then jumps, because `decode.ts` deliberately
never shows a frame the reader has already moved past. Showing them needs a
monotonic ordering `presentWhenDecoded` does not have — it compares opaque `src`
strings and can only tell "current" from "not current", never "newer" from
"older". Two attempts at relaxing it without that ordering are recorded as
failures: skipping only when work is queued behind changed nothing on screen
(presentation is gated separately), and relaxing the presentation gate too broke
four tests outright, because `unboundedDecodeQueue` reports no pending work and
a three-deep queue settles out of order — so a stale page could paint over a
newer one.

**Not covered by a test.** `App.tsx` is Vitest-unreachable, so the evidence is
the browser measurement above.

## Showing the pages go by, and the guarantee it costs

The other half of the flicker report — "the page that we were on when flicking
fast seems to linger and it's confusing". The linger was real: a sixteen-press
flick drew **three** frames, holding one spread for the whole flick and then
jumping twenty-seven pages.

That was `decode.ts` working as designed. `stale()` was asked twice: once at
dequeue, to skip decoding a frame the reader had passed, and once at the end, to
refuse to *show* one. Under a flick every settled frame is superseded — the
reader pressed again while it decoded — so both tests said no to everything
except the last.

**Two failed attempts came first, and both are worth recording.** Relaxing only
the dequeue test changed nothing on screen, because presentation is gated
separately; it merely added back the wasted decodes the skip had removed. Then
relaxing the presentation test as well broke four tests immediately, and
rightly: `presentWhenDecoded` compares opaque `src` strings, so it can tell
"current" from "not current" but never "newer" from "older" — and with a
three-deep queue settling out of order, a frame the reader had passed could
paint *over* one they had not. A code comment asserting it "can only ever draw
something newer" was written and was false.

**What was missing was an ordering, and it cannot live in `decode.ts`.** Only
`PageView` knows the sequence its own `src` took. It now numbers each attempt
and drops any decode that finishes after a newer one has already been drawn;
`presentWhenDecoded` reports whether it actually decoded (a skipped job has no
image to show) and otherwise presents whatever it produced. The dequeue test
became `stale() && queue.pending() > 0`: superseded is not on its own a reason
to skip — newer work already waiting is. That is self-tuning, which is the part
worth keeping. At reading size a page decodes in ~10 ms and the queue drains
about as fast as a reader can press, so most frames get through; zoomed to full
resolution a decode is ~75 ms, the queue stays backed up, and the same test
skips nearly everything.

Measured on the same sixteen-press flick:

| | before | after |
| ------------------------ | -----: | ----: |
| frames drawn             | 3 | **10** |
| linger after last press  | ~250 ms | **221 ms** |
| blank frames             | 0 | 0 |
| tear between the halves  | 0.0 ms | 0.0 ms (max 0.0 over 12 turns) |

Pages travel past — `0, 2/1, 3, 4/3, 7/6, 11/6, 13/12, 17/12, 25/24, 29/28` —
instead of one spread holding and jumping.

**The cost is a real weakening of the pair guarantee, and it is not
hypothetical.** A superseded half is now drawn when nothing newer is queued, so
a pair whose two cells decide differently can briefly show two generations at
once: `11/6` and `17/12` above are exactly that. In an ordinary turn both cells
change together, join one cohort and decode together, so they stay in lockstep —
measured at 0.0 ms apart across twelve turns — but the guarantee is now "they
move together in practice" rather than "they cannot do otherwise". Two of
`decode.test.ts`'s assertions were rewritten to state the weaker contract rather
than deleted, so the change is visible to the next reader.

**And the property that keeps it safe has no test.** The ordering guard lives in
`PageView`, which Vitest cannot reach, so "never draw older over newer" rests on
the browser measurement above. `decode.ts`'s own tests now pin a weaker contract
than they did.

## The flicker that was left, and the two things `wide` had been meaning

With a flick showing pages go by, a slower pass turned the remaining flicker up:
"it's apparent at low speeds". The obvious reading was that showing intermediate
frames had introduced it, and the obvious fix a speed threshold — flip between
the two behaviours depending on how fast the reader is pressing. Both were
wrong, and measuring said so before anything was built.

Sampled every 8 ms at 700 ms per turn: **zero mixed-pair frames**, so the two
cells were not disagreeing. What the timeline showed instead was three
consecutive full-width frames — `2/1`, `3 [wide]`, `4 [wide]`, `5 [wide]`,
`7/6`. Those are real groups. `en-mono` joins page 4 as a spread, and pages 3
and 5 are *unpartnered* — the aligned pairing's parity cost, four such pages in
this volume and clustered near the front matter. Reading across them collapsed
the layout to one wide page and expanded it again, turn after turn. **The same
frames appear whichever presentation rule is in force**, so a fast/slow hybrid
could not have touched it.

**`cellsFor` was answering one question that was really two.**

```ts
const wide = group.spread === true || group.slots.length === 1
```

A joined spread *is* one image twice as wide and has to span the group. A page
that is merely alone is an ordinary page that happens to have no partner, and
drawing it full width is a choice — the one that made the geometry jump. The
two are now separate: `wide` is the spread alone, and `alone` says whether the
second cell holds a real page or a mirror of the first. An unpartnered page
keeps its own cell and leaves the other empty.

They also follow different clocks, which is the part worth keeping. `wide` is
taken from the target group, because it is a fact about the page rather than
about how far a decode has got — reading it off the screen would let two cells
briefly holding one URL mid-flick resize the artwork. `alone` is read off the
drawn sources, because that is what governs whether a cell is *visible*, and a
layout keyed on state flaps ahead of the pictures.

Measured across ten turns at reading pace, by the drawn width of the first cell:

| frame | before | after |
| ----- | -----: | ----: |
| `2/1` | 693 | 693 |
| `3`   | **1385** | 693 |
| `4` (a real spread) | 1385 | 1385 |
| `5`   | **1385** | 693 |
| `7/6` | 693 | 693 |

Two geometry changes over ten turns, both at page 4, both correct. A fast flick
is unchanged: eleven frames going by, two width changes, both at the spread.

**What it costs** is a blank half beside an unpartnered page rather than an
enlarged one — four pages per volume in this edition.

**One process note, because the failure was silent.** Adding the test for this
introduced a stray `})` that closed the `cellsFor` block early. Vitest printed
`591 passed` and no failing assertion while **52 tests had stopped running**;
only the `Test Files 1 failed` line gave it away. A green count is not evidence
that the tests ran — the file count is the thing to read.

## Two review findings, and a guarantee put back

A review of the whole branch, after the reader had signed off on how it felt.
Two real defects, both introduced by this work, and one weakening that turned
out to be avoidable.

### An unpartnered page was claiming the whole group

Splitting `wide` from `alone` left `cellOf` answering `'only'` for both. That
reads as English — an unpartnered page *is* the only page in its group — and it
is the wrong sense of the word for the one consumer: `viewport.ts`'s `cellBox`
takes `'only'` to mean *the page fills the group* and returns `WHOLE`. True of a
joined spread; false of an unpartnered page the moment it stopped spanning the
group and kept its own half-width cell. A zoom on such a page would have been
mapped against twice the width it covers, putting a swap's rect about that far
across. `'only'` is the spread alone now, and the test that asserted otherwise
is renamed rather than edited, so the change is visible.

### The resize filter was a cover's, not a reader's

`fit_at` reused `thumbnail_at`, and `image`'s `thumbnail` is documented as "a
fast integer algorithm where each source pixel contributes to exactly one target
pixel… may give aliasing artifacts". That is the right trade for a 512px cover
and the wrong one for the page a reader is looking at — manga is screentone and
thin ink, the two things that alias worst. Reducing `en-mono`'s 2147px page to
768px through it and through a proper filter differ at **25.3 dB PSNR**, with
the hatching visibly broken up on the fast path.

`CatmullRom` rather than `Lanczos3`: measured at about **5 ms on a 100 ms
generation**, so the choice is nearly free, and Lanczos rings on high-contrast
edges — every panel border and every letter on the page. Reading pages got their
own `PAGE_FORMAT_VERSION` so changing this does not throw away every cover and
tile in a library.

### The pair guarantee did not have to stay weak

Showing a flick's intermediate frames had cost the guarantee that a two-page
turn's halves move together: each cell decided independently whether its decode
was still worth doing, so two cells asking moments apart could answer
differently and the pair would show two generations at once. Measured before:
frames `11/6` and `17/12` in a single flick.

The fix is to ask once. `cohort.ts` hands each job a `CohortMember` whose
`decideOnce` evaluates on the first member to ask and memoises for the rest, so
both halves of a turn skip together or decode together. Measured after: **zero
split-generation frames**, the same ten frames going by, tearing still 0.0 ms
median across twelve turns. The guarantee is back to "cannot do otherwise"
rather than "does not in practice", at no cost to the flick.

Two `decode.test.ts` assertions became unreachable — they constructed exactly
the split the fix prevents — and were replaced by tests that pin the shared
decision directly.

### And the untestable guard was extracted

The ordering that keeps a flick safe — never draw a decode older than what is
already drawn — lived inside `PageView`, which Vitest cannot reach, so the one
property protecting it rested on a browser measurement. It is `ordering.ts` now,
with tests for the case that matters: a queue running three decodes at once
settles them in whatever order the browser finishes, so a flick landing on page
29 must not be overwritten by page 21 arriving afterwards.

## Testing the part that had no tests

The whole two-page mechanism rests on properties no test in this repository
could see. `App.tsx` and `PageView` need a DOM; every existing test is
deliberately DOM-free — `decode.ts`'s own header says "no DOM, no `jsdom`" — so
the rules were covered and the assembly was not. Every defect this work shipped
and then fixed was in the assembly, and every one was caught by a person
looking at the screen.

**`jsdom` would not have closed it**, which is why the obvious answer was the
wrong one. It has no layout engine — `getBoundingClientRect` returns zeroes —
and no real `HTMLImageElement.decode()`. The flicker was cell widths; the tear
was two decodes landing at different times; the sluggishness was a decode. All
three are exactly what a headless DOM cannot answer.

So the `browser` project: Vitest 4's own browser mode over Playwright's
Chromium, holding six tests that cover what nothing else can — a real decode
resolving before a page is drawn, the outgoing page surviving a source that can
never decode, two cells taking equal width in a flex row, one collapsing to zero
while its neighbour spans the group, `onShown` reporting what is drawn, and both
halves of a pair presenting inside one frame.

**Kept small and kept out of CI.** A browser project needs a downloaded
Chromium, and this CI has no cache to keep one in, so `pnpm -C web run test` stays the
`node` project alone — exactly what the forge runs — and `test:browser` is a
separate script `check-all.sh` runs locally, naming itself as unmeasured when
the browser is absent. The browser tests are for the properties nothing else can
see, not for duplicating the 654 tests that already run in milliseconds.

**Two setup traps, both caught by the tools rather than by reasoning.**
`browser.provider` takes a factory in Vitest 4.1, not the string every older
example shows — refused outright, with the error naming the fix. And
`vite.config.ts` must import `defineConfig` from `vitest/config` rather than
`vite`, or the `test` key is a type error; since `tsc -b` typechecks that file,
the wrong import fails the build rather than silently ignoring the block.

A third was caught only by reading the output: React logged "the current testing
environment is not configured to support `act`" while the tests passed anyway.
`act` was not flushing effects, so a test could have passed by observing state
React had not committed. `browser-setup.ts` sets `IS_REACT_ACT_ENVIRONMENT`.

## The mutation baseline, and the minus sign nothing was testing

`env -u TAIYAKU_CORPUS cargo mutants --gitignore=true --test-tool=nextest`,
against the CI-visible suite, measured on this machine on 2026-08-10:
**1,660 mutants in 43 minutes — 1,149 caught, 346 missed, 136 unviable, 29
timed out.** Of 1,524 viable mutants, 75.4% were caught.

| crate | mutants | caught | missed |
| ----- | ------- | ------ | ------ |
| `taiyaku-cli` | 752 | 478 | 217 |
| `taiyaku-core` | 460 | 370 | 47 |
| `taiyaku-serve` | 304 | 217 | 52 |
| `taiyaku-read` | 93 | 56 | 17 |
| `taiyaku-wasm` | 51 | 28 | 13 |

`taiyaku-core`'s 47 are concentrated rather than spread evenly across the
crate:

| file | survivors |
| ---- | --------- |
| `solve.rs` | 26 |
| `sidecar.rs` | 11 |
| `credits.rs` | 7 |
| `slot.rs` | 2 |
| `lib.rs` | 1 |

**The estimate this replaces was wrong twice over, not once.** The plan
recorded after PR #37 guessed "maybe 1,000+ mutants at 10–30s each (hours)"
for the Rust half. The count came in at 1,660 against that open-ended floor,
and the per-mutant cost was an order of magnitude too high. Both errors trace
to the same fact: the whole workspace suite is 526 tests in 1.9 s under
`cargo nextest run --workspace --all-features`, so almost none of a mutant's
≈1.6 s average cost is the test run — it is the incremental rebuild the
estimate did not account for. Forty-three minutes is a coffee break, not the
"hours" that had been budgeted.

**The finding, and it is one character wide.** In `solve::solve`, the line that
initialises the score table's first column — `solve.rs:188` as the tool named it
on the day, and `solve.rs:210` after this branch's own tests moved it, which is
why the symbol is the durable name and the coordinate is only a receipt —
reads `-(i as f32) * GAP_PENALTY`, the cost of reaching that cell having
skipped `i` pages of A. Deleting the unary minus turns that penalty into a
bonus, and all 526 CI-visible tests still pass — nothing in the suite prefers a
short gap to a long one. Applied by hand and run
against the twelve standing corpus checks under `--release`, exactly one
fails:

    11 passed; 1 failed  (15.6s)
    failures:
        corpus::composing_through_the_hub_agrees_with_a_direct_solve

So the arithmetic *is* verified — by one check, running against real material
on a NAS this repository's CI cannot reach, and by nothing CI runs. Eleven of
the twelve corpus checks are indifferent to the mutation, and every test the
forge executes is indifferent to it too.

**`solve.rs` was not picked because it looked unusually bad.** It missed 19%
of its own viable mutants, against 23% for the workspace as a whole, so the
file this branch spends most of its effort on is representative of the
baseline rather than an outlier chosen to make a dramatic example.

The triage of the 47 survivors themselves follows below; `docs/status.md`
tracks what remains of the workspace's other 299, measured but not addressed.

## The 47, killed — and what the fixtures got wrong

**Re-measured after the work: 458 mutants, 411 caught, 3 missed, 38 unviable,
6 timed out.** Against the baseline's 460 / 370 / 47 / 38 / 5, in the same
buckets. The figure is the union of two runs rather than one: a package-scoped
run was interrupted at 412 of 458, having finished `credits.rs`, `lib.rs`,
`sidecar.rs` and `slot.rs`, and the three files it had not — `regions.rs`,
`route.rs` and `solve.rs` — were then run in full as 170 more. The union is
stated here because a number assembled from two runs and a number from one are
not the same claim; the two ledgers are consistent, and `solve.rs`'s share of
it agrees with the two independent file-scoped runs taken while the work was
being done.

| | baseline | after |
| ---- | -------: | ----: |
| mutants | 460 | 458 |
| caught | 370 | **411** |
| missed | **47** | **3** |
| unviable | 38 | 38 |
| timed out | 5 | 6 |
| caught, of viable | 87.7% | **97.9%** |

Two mutants disappeared rather than dying, and that is a real result rather
than an accounting quirk: `percentile`'s clamp turned out to be unreachable —
`at` is already bounded by `len - 1` for any `fraction` in `0.0..=1.0`, so
`values[at.min(values.len() - 1)]` could never bind — and deleting dead code
removes its mutants along with it. The timeout count rose by one because a
test written for the traceback converted `solve.rs`'s `j -= 1` → `/=` in the
`SkipB` arm from `MISSED` to `TIMEOUT`. That is better verification and a
standing cost of about 21 s on every future run of the file — the two timeouts
now cost 20.5 s and 21.5 s against the tool's auto-selected 20 s bound, so the
increment is one of them, not their total. See `docs/status.md`, O39, for why a
hang is not as good as a failure here.

**The headline mutant no longer survives anything.** Deleting the unary minus
that made the gap penalty a bonus now fails a test the forge runs —
`a_weak_opening_pair_beats_gapping_both_sides`, which is its sole killer, and
which also kills the same deletion on the first *row* rather than the first
column. Before this branch that mutation passed all 526 CI-visible tests and
failed exactly one corpus check, on 26 GB of material held on a NAS. That
transfer is the whole point of the branch: the arithmetic is verified in the
same place everything else is.

**And the suite is still fast, which was the binding constraint.** 552 tests in
0.76 s under `cargo nextest run --workspace --all-features`, against 526 in
1.9 s before — 26 tests added and the wall clock went *down*, because the
baseline figure was taken on a colder target. The coverage job runs this suite,
so a fixture that needed real material or a large table would have cost more
than the survivors were worth; none does.

**All three survivors are equivalent mutants, and each carries its argument.**

`sidecar.rs:1052:46`, `||` → `&&` in `Sidecar::trust`. With exactly one of
`from`/`to` unknown, `from != to`, so the identity short-circuit is out and
every remaining path runs through `quality_of(·, unknown)`. `edge_between` only
matches an alignment whose endpoint *pair* is exactly `{from, to}`, so any edge
it returns has the unknown name as an endpoint, and `runs_of` does
`editions.get(&alignment.from)?` / `.get(&alignment.to)?` and returns `Err`,
which `.ok()?` turns into `None`. Both hub legs die the same way. This is
equivalence over the whole input domain of a public function, not over the
inputs its current callers happen to supply — the stronger of the two kinds. It
stops being equivalent the moment `quality_of` can answer without both
endpoints resolving.

`solve.rs:119:18` and `:122:25`, `>` → `>=` in `calibrate`'s best/runner-up
loop. The loop maintains `row_second <= row_best`, so on an exact tie both
forms end at `(s, s)`; line 122's `>=` only ever rewrites `row_second` with the
value it already holds. **The plan's stated reason for expecting these two to
be killable was wrong** — it claimed `>=` "would let the runner-up overwrite
the best or itself and report a margin that is not zero", and the overwrite
writes the same number back. The argument does not even need the float clause
that was first offered with it: `row_second`'s only consumers are
`row_best - row_second` and `is_finite()`, and neither can tell `+0.0` from
`-0.0`. NaN would not separate them either, since NaN fails `>` and `>=` alike.

**Six of the plan's own fixtures passed against unmutated code and could not
tell the mutant from the original.** That is the finding worth keeping, because
every one of them would have shipped as a green test that verified nothing —
which is the exact defect this branch exists to remove. They were caught only
because the plan required each test to be watched failing against a
hand-applied mutation rather than merely passing:

- the all-landscape `spread_flags` fixture — all six aspects sit inside
  `is_spread`'s `1.7..=2.8` band under both the median baseline and the
  narrowest-page one, so `/` and `%` give the same answer;
- the directed-edge `unpaired_runs` fixture — `pages.end <= N` is an upper
  bound and the arithmetic mutants only ever lower it, while `assert_ne!`
  survives a full orientation swap;
- the member-boundary fixture — a member's first page has local index 0 and the
  previous run's `local.end` is never 0, so `&&` and `||` take the same arm;
- the leading-gap fixture — a gap *bonus* rescores the winning path rather than
  changing it, so the headline mutant survives it;
- the below-the-floor fixture — an A-only tail leaves no B page to pair it to,
  so `- floor` → `+ floor` changes nothing;
- the `compress` fixture — gaps split by a paired run never enter the
  `(None, None)` arm the guard lives in.

And a seventh, which is the same failure in a different disguise: the
`parse_slot` test rejects `""`, `"L"`, `"12a"` and `"a12"`, and every one of
those is rejected by `digits.parse()` anyway, so the guard could be inverted and
the test still passed. The string that discriminates is `"+12"`, because
`u32::from_str` accepts a leading `+`.

One more is worth recording for the opposite reason. The oblong-matrix fixture
was written specifically to pin the score table's stride and **does not kill
either stride mutant**; the leading-gap fixture kills both. A test can be aimed
at one thing, hit nothing, and still look like coverage of the thing it was
named for.

## O38: the arm that answered nothing, on every volume in the corpus

`unpaired_runs` answers "which pages of `edition` does the alignment against
`against` never reach". It builds that by collecting every page any run
mentions and complementing the set. The collection was unconditional on one
side:

    let range = match mine_is_a {
        true => Some(run.a),
        false => run.b,
    };

A **gap run** carries an `a` span and no `b` — those pages are precisely the
ones with no counterpart. The `false` arm gets that right for free, because
`run.b` is `None` and the gap contributes nothing. The `true` arm collected
`run.a` regardless, so it marked every gap as *mentioned* and then complemented
it away. The fix is to ask the same question the `false` arm answers by
accident: `run.b.map(|_| run.a)`.

**What the corpus says, and it is the whole corpus.** Twelve volumes across four
series, each aligned raw-mono → en-mono into a scratch sidecar, then asked from
the hub side — the `true` arm — with `credits <raw members> --sidecar F
--against en-mono`. The `before` column is `ddb6149` built from a clean
worktree, not the working tree with the fix reverted, so nothing measured here
depends on an edit having landed:

| work | gap runs stored | gap pages | `runs=` before | `runs=` after |
| ---- | --------------: | --------: | -------------: | ------------: |
| gotoubun-v01 | 2 | 2 | 0 | 2 |
| gotoubun-v03 | 2 | 2 | 0 | 2 |
| gotoubun-v08 | 2 | 3 | 0 | 2 |
| kaguya-v01 | 4 | 4 | 0 | 4 |
| kaguya-v03 | 2 | 2 | 0 | 2 |
| kaguya-v08 | 2 | 3 | 0 | 2 |
| kobayashi-v01 | 2 | 2 | 0 | 2 |
| kobayashi-v03 | 1 | 2 | 0 | 1 |
| kobayashi-v08 | 4 | 5 | 0 | 4 |
| oshinoko-v01 | 1 | 1 | 0 | 1 |
| oshinoko-v03 | 2 | 2 | 0 | 2 |
| oshinoko-v08 | 2 | 8 | 0 | 2 |
| **total** | **26** | **36** | **0** | **26** |

**Zero on every volume.** Not "rarely wrong" or "wrong on an unusual shape" —
the hub side of every stored edge in the corpus answered `[]`, for 26 runs
spanning 36 raw pages, and the failure looks exactly like a clean bill of
health. That is the answer to how long it had been wrong: since the function
was written, on everything anyone could have pointed it at.

**And it was hiding a real credits page.** Eleven of the twelve produce runs but
no candidates — mid-volume gaps are not credit cards, and the heuristic is right
to say so. `kobayashi-v01` is the twelfth:

    page  member      local  run  at-end  recurs  nearest  verdict
     143  raw-mono  143/146    1      no       -  -        -
     145  raw-mono  145/146    1     yes       -  -        credits

Raw page 145 of 146 — the last page of the volume, unpaired against the English
edition, flagged `at-end`, verdict `credits`. Before the fix that same command
printed a bare header and `candidates=0`.

**The test that should have caught it, and why it could not.**
`each_end_of_a_directed_edge_reports_its_own_gaps` executes the `true` arm, is
named for exactly this property, and was *hardened* in `ddb6149` after the
mutation review found its assertions too weak — pinned to exact runs in both
index spaces rather than `assert_ne!`. It still cannot fail on O38, because its
fixture's only run is `{"a":"0-3","b":"2-5"}`: a paired run, so `run.b` is
always `Some` and both arms agree. Its unreached pages are unreached by falling
*outside* every run's span, which is a different shape from falling *inside* a
gap run, and the fixture carries only the first.

This is an eighth instance of the finding recorded above — a fixture that
verifies less than its name claims — and the one with the sharpest edge, because
it had already been reviewed once for that exact weakness and strengthened
against the wrong axis. Strengthening assertions does not help when the fixture
lacks the material. `the_from_side_of_an_edge_reports_its_own_gap_runs` supplies
it, and was watched failing at `left: 0, right: 1` before the fix went in.

**The corpus now watches it, and the check was confirmed failing first.**
`gotoubun_hub_reports_the_pages_its_english_edition_never_reaches` asks
`unpaired_runs(&doc, "raw-mono", "en-mono")` — the `from` side — and asserts the
pages, not the count: two-of-two is reachable by reporting the wrong two.
Applied to a clean `ddb6149` worktree with the defective arm still in place it
fails `left: []`, `right: [1, 6]`, which is O38's signature exactly.

It is worth saying why this did not already exist. Both existing credits checks
ask `("en-fan", "raw-mono")`, because that is the direction a reader asks about
a spoke — so the file had two standing checks *named for* `unpaired_runs` and
neither could ever execute the arm that was broken. The manual drive recorded
above answered the question once; a standing check is what keeps answering it.

**And the tool could not have caught it, before or after.** `cargo mutants
--file crates/taiyaku-core/src/credits.rs` at this branch's head is 43 mutants —
38 caught, 1 unviable, 4 timeouts, **0 missed** — but it generates *no mutant at
all* for `run.b.map(|_| run.a)`: two mutants' diffs contain the line as context
and neither replaces it, because match arms of that shape are not in its
operator set. The four timeouts are the ones §12 of the design already
recorded, at the slot-grouping `while` loop, five lines further down than
before only because this change adds a comment above them.

So the mutation score says nothing about this defect in either direction. It
was found by a reviewer writing a probe rather than by the tool, and the only
evidence the fix works is a test watched failing first and twelve volumes
measured before and after. That is worth stating plainly next to a clean
mutation report, because "0 missed" on a file reads as "this file is verified"
and here it did not mean that.
## The other 30, and the four surfaces they were hiding in

`taiyaku-read` 17 and `taiyaku-wasm` 13, the two smallest of the four crates
left after `ddb6149`. Re-measured on this branch: **30 missed → 0**, at 140
mutants, 110 caught, 22 unviable, 8 timed out.

The interesting part is not the number but the shape. These were not thirty
scattered weak assertions. They were **four surfaces with no test on them at
all**, and each one is a different reason a surface goes unwatched.

**1. The wasm shell's value forwarders (13).** `editions`, `slotCount`,
`slotAt`, `indexOf` and `credits` are one line each — `self.inner.method(…)` —
and every one could be replaced by a constant with the suite still green. The
reason is structural and worth naming: `binding` is thoroughly tested *one layer
below*, and `lib.rs`'s own module doc says the shell "converts types across the
JS boundary and decides nothing". A layer that decides nothing still has to
forward the right argument to the right method, and nothing checked that it did.
Two of the shell's methods — `resolveSlot` and `chapterAt` — already had tests
for exactly this reason; the other five had been passed over because they look
even more trivial than the two that were not.

`slotAt` and `indexOf` are pinned against *each other* rather than separately,
since they are inverses and each adds its own half conversion on the way out and
back.

**And one thing could not be tested at this layer, which is worth recording
because the existing comment understates it.** `indexOf`'s refusal of an unknown
half builds a `JsError`, and *constructing* one on a non-wasm target panics with
`cannot call wasm-bindgen imported functions on non-wasm targets` — not merely
formatting it, which is what the note on
`the_shell_forwards_its_arguments_and_copies_every_field` says. The refusal is
pinned one layer down in `half_from_str`, which is ordinary Rust.

**2. `naming.rs::tokenize` (6, plus all 8 timeouts).** The file carried exactly
one test, and it is a good test — fractional chapters sorting after their whole
numbers is a real defect that cost a volume's page order. It just cannot see
this function's bounds. Every name in it ends in `.cbz`, so every digit run
finishes in the middle of the string and no loop ever approaches its guard.

Two shapes of survivor followed from that:

- **The lookahead.** `chars[i + 1].is_ascii_digit()` decides whether a `.` joins
  the number. Asking `chars[i - 1]` instead always sees the digit the run just
  ended on, so every `.` would swallow — and the ordering test still passes,
  because `c005.cbz` tokenizing as `Number(5.0), Text("cbz")` rather than
  `Number(5.0), Text(".cbz")` compares identically against `c005.5.cbz`. Only
  the token stream shows it, so that is what the new test asserts.
- **The bounds.** `< len` → `<= len` in any of the three loops indexes one past
  the end. Killing those needs names that *end* where a loop ends: `page1` on a
  digit, `c005.` on a trailing dot, `c005.5` on the digits after one. A trailing
  dot is not hypothetical — it is what a truncated rename leaves.

The **8 timeouts are all here**, and all the same shape: `+=` → `*=` never
advances `i`, `<` → `>` never enters, `delete !` never exits. A `tokenize` that
does not terminate cannot silently mis-order a volume, which is the O39
disposition rather than an untested behaviour.

**3. `MAX_DECODE_ALLOC` (6).** `3 * 1024 * 1024 * 1024`, and every arithmetic
mutation of it survived — including ones that lower the ceiling below the
corpus's largest page, which is the entire reason the constant was raised from
`image`'s 512 MB default. A constant with a long doc comment explaining why it
must be what it is, and nothing asserting that it is.

Written as the two facts the doc claims rather than as `assert_eq!(X, 3 * 1024 *
1024 * 1024)`, which is the expression again and would move with any edit to it:
it must admit a 10200×14039 decode at four bytes a pixel, and the default it
replaces must not.

**4. `pages.rs` (5), and one of them was deleted rather than killed.** The
hand-written `Debug` for `Page` — which exists so a failure does not dump
megabytes of JPEG — could return `Ok(Default::default())` unnoticed.
`page_names_from`'s two skip rules could be `&&`-ed together, because the usual
resource fork `__MACOSX/._p2.jpg` satisfies *both* conditions and cannot tell
them apart; the discriminating entry is `__MACOSX/p1.jpg`, with an ordinary
basename. `member_digest` could return a constant, which would report every
member as unchanged forever.

The fifth is the one worth reading. `unsupported_container` guarded its tar
signature with `bytes.len() > 262 && &bytes[257..262] == b"ustar"`, and the
mutant `>` → `>=` survived — correctly, because `>=` is what the slice actually
needs. Reading `bytes[257..262]` requires 262 bytes; the guard demanded 263, so
a 262-byte buffer carrying the signature read as unrecognised. **Unreachable
with a real tar** — the format's first header block alone is 512 bytes — so
this is not a defect anyone could hit. But the choice was between pinning an
arbitrary off-by-one with a test and removing it, and the guard is now
`bytes.get(257..262) == Some(&b"ustar"[..])`, which says what it means and has
no arithmetic to get wrong.

That is why the mutant total moved 144 → 140 rather than staying put: four
mutants at that site stopped existing, one of them the survivor and three
already caught. **29 of the 30 were killed by tests; the thirtieth was killed by
deleting the expression it lived in.** Worth stating plainly, because "30 → 0"
and "30 killed" are not the same claim.
## The 51, and the client that talked to nobody

`taiyaku-serve`, the third crate after `ddb6149`. **51 missed → 0**, re-measured
at 307 mutants: 271 caught, 31 unviable, 4 timed out.

**Over half of them — 27 — were in one struct**, and it is the one the crate's
own design goes furthest out of its way to avoid. `stump/http.rs` opens:

> Every call goes through `Transport`, so the whole of `stump` is testable
> against recorded responses and the suite does not depend on the NAS being up.

That is true, and it is good design, and it is exactly why `UreqTransport` had
no coverage at all. Every test in `stump`, `composite` and `routes` runs against
`FakeTransport`. The real client — the four lines of `ureq` configuration that
decide what actually goes out on the wire — was reachable by nothing. `url`,
`bearer`, `post_json` and `get_bytes` could each return a constant.

**A seam that lets tests skip a component is a seam that hides it.** The fix is
30 lines of `TcpListener` and `write!` in a test module: bind a loopback port,
capture the request, answer with a fixed response. No dependency, and it pins
the four things the configuration exists to do — the path is joined below the
base, the bearer token is attached, the body goes out as typed, and the reply
comes back rather than being invented. Plus the one piece of real judgement in
it: `http_status_as_error(false)`, so a 404 from a page route arrives as the
number 404. Without that, `composite` would read a missing page as *Stump being
down* and trip the cooldown latch.

**Three more were decisions living inside `eprintln!` arms**, which is a
category worth naming because no amount of testing the surrounding component
finds them:

- `reqlog`'s `!self.write_failed.swap(true, …)`. The `!` is the entire
  behaviour — warn once on a failing disk rather than once per request — and
  deleting it inverts that. Every test of the log reads its *contents*; the
  difference is on stderr.
- `resolve_all_works`'s `works.len() - index - 1` and the `> 0` that guards it.
  They decide only the wording of a line printed when a run gives up partway.
  The abort test asserts which requests were made, which is the right thing for
  it to assert and cannot see any of this.

Both are now named functions returning values — `first_write_failure()` and
`skipping_notice(total, index)` — and both tests are about the case that
matters and had no witness: the second failure, and aborting on the *last* work,
where every mutation of the subtraction promises to skip works that do not
exist.

**And `serve` could be replaced by `Ok(())`.** The composition root, binding no
socket, starting no server, with all 168 other tests green — because its doc
comment said "every test drives the router directly and none of them opens a
socket", and that had quietly become a reason not to test it at all rather than
a description of where the seam is. It now has exactly one test, and writing it
turned up a trap worth recording: **`ureq` is blocking**, so polling the server
with it from a default `#[tokio::test]` blocks the very runtime that is meant to
be driving `serve`. The obvious version deadlocked rather than failing. It needs
`flavor = "multi_thread"`.

**One comment was wrong, and the mutant is what asked the question.**
`get_page` filters `?w=` with `*edge > 1`, justified by:

> Zero and one are treated as absent, since `fit_at` would answer `None` for
> them anyway once no real page is that small.

The condition is backwards. `fit_at` returns `None` when the page is *already
within* the requested edge — so for `w=1` it returns `Some` and re-encodes, and
a reader asking for `?w=1` would get a one-pixel JPEG rather than the page. The
filter is load-bearing; only the reasoning for it was wrong. The surviving
mutant `> 1` → `>= 1` is precisely that scenario, and it survived because no
test had ever asked for a width of 1 — the comment having explained why nobody
needed to.

**A note on how the last one was killed.** The two read-limit assertions were
first written as `assert!(MAX_PAGE_BYTES > 430 * 1024 * 1024, …)`, which is
`clippy::assertions_on_constants`, and clippy's suggested fix is
`const { assert!(…) }`. That would be worse here, not better: a compile-time
assertion turns a mutated constant into a mutant that fails to *build*, which
`cargo-mutants` reports as **unviable** rather than caught. The check is bound
through locals instead so it stays a runtime assertion and the mutants stay
killable. Moving a check earlier is not always moving it forward.

## The mutant that was caught twice and missed once

Re-running `cargo mutants -p taiyaku-serve` after rebasing the serve branch onto
a moved `main` produced **1 missed** where two earlier runs had produced 0, on a
mutant that was never in the original 51:

    cover.rs:260: replace -= with /= in Semaphore::acquire

Nothing in the code had changed. `*available -= 1` becoming `*available /= 1` is
a permit count that never decreases, so `acquire` stops blocking and the
two-permit ceiling on concurrent decodes disappears.

**The test that was supposed to catch it is timing dependent, and its own
comment said it was not.** `a_second_acquisition_waits_until_the_held_permit_is_released`
argued:

> Deterministic, not timing-based: the second thread's `acquire()` can only
> return after `Drop` runs on the first thread's guard, which in program order
> happens strictly after `released.store(true, ..)` — so if `acquire()` ever
> returned early, this would be observable as `false`, not as a flaky pass.

The first clause is true **only while `acquire` blocks**, and a broken `acquire`
is precisely one that does not. Once it returns early there is no ordering at
all between thread A's `released.store(true)` and thread B's load of it: B
acquires immediately and races A. Whichever wins decides whether the mutant is
caught. Two runs said caught, the third said missed.

So the reasoning was not merely unlucky — it assumed the property under test in
order to prove the test could observe its absence. A test for "X blocks" cannot
derive its own determinism from X blocking.

The fix is not to make the concurrent test cleverer.
`acquiring_a_permit_decrements_the_available_count` asserts the count directly,
single-threaded, before and after each acquire and each drop. Three consecutive
scoped runs of `cover.rs` then give 39 mutants, 0 missed, every time. The
concurrent test stays, because the blocking property is worth stating and it is
the only test that states it — with its comment corrected to say what it can and
cannot see.

**Worth generalising.** Mutation testing is usually described as finding
untested code; here it found a *flaky* test, and only because the same mutant
was run three times across three bases. A single run reporting a survivor could
have been dismissed as noise, and a single run reporting none — which is what
the two earlier runs did — hid it completely. When a survivor appears that was
not there before and nothing changed, the flakiness is the finding.
## The verdict bands, the gauge, and two pages that hashed alike

`taiyaku-cli` re-measures at **752 mutants, 217 missed** — the recorded figure
exactly — 478 caught, 45 unviable, 12 timed out. Nine files hold them:

| file | survivors |
| ---- | --------: |
| `main.rs` | 70 |
| `fingerprint.rs` | 39 |
| `detect.rs` | 37 |
| `report.rs` | 30 |
| `inventory.rs` | 19 |
| `regions.rs` | 7 |
| `duplicates.rs` | 7 |
| `credits.rs` | 5 |
| `cache.rs` | 3 |

**`detect.rs` and `regions.rs` — 44 between them — are deliberately left.** O31
to O34 record that the text-block detector is measuring the wrong population and
that none of its constants is worth sharpening before it is redesigned. Writing
44 tests that pin its present behaviour would make that redesign harder, not
easier: every one of them would be a claim about output the redesign exists to
change. Mutation coverage is not a goal in itself, and this is the case where
chasing it works against the project.

**`report.rs` is closed: 30 → 0**, verified at 40 mutants, 37 caught, 3
unviable. Its gaps were three shapes.

**The bands, and both their boundaries.** `verdict` sorts a page into `none`,
`weak`, `check` or `firm` on two thresholds, and nothing asserted any of it — it
could return `("xyzzy", "xyzzy")` for every page, and both comparisons could be
inverted. Both tests are strict `<`, so the new test asserts *exactly at* each
constant: a margin of exactly `MARGIN_WEAK` is not weak. That is the distinction
every `<` → `<=` mutant turns on, and an assertion at 0.02 and 0.10 would have
missed all of them.

**The gauge.** `(((MARGIN_FIRM - margin.clamp(0.0, MARGIN_FIRM)) / MARGIN_FIRM)
* 16.0)` is the one thing in the report a reader judges at a glance while
scrolling, and every operator in it survived. Asserted through `render` on the
emitted `--offset` rather than on an extracted helper, because the arithmetic is
written inline there and it is the emitted value a stylesheet reads: 0 at firm,
16 at a collapsed margin, 8 halfway.

**And the header's one rate.** `100.0 * run_count / scored.len()` — where `*` →
`+` gives 102 instead of 50 and reads perfectly plausibly in a table.

**The fixture that hashed alike, which is the part worth keeping.** `prepare`
decodes each page once and takes both a hash and a thumbnail from it; the three
survivors were whole-function replacements returning empty vectors. The obvious
test is two pages and an `assert_ne!` on their hashes — and the first version of
it *failed*, with both pages reporting `PageHash([0, 0, 0, 0])`.

The two fixtures were smooth gradients differing in one colour channel. The
fingerprint is perceptual and runs on `to_luma8`, so a channel that vanishes
into luma leaves two structurally identical images. Had the test asserted only
that two hashes came back — the natural way to kill `Ok((vec![], vec![]))` — it
would have passed while proving nothing about whether the hashes were *of the
pages*. The fixtures are now split on different axes, left/right against
top/bottom, which is structure the hash can see.

This is the same finding as `ddb6149`'s six fixtures, arrived at from the other
direction: there, plausible test data passed against unmutated code; here, a
plausible fixture produced identical inputs to an assertion that was itself
correct. **The assertion was strong enough to expose its own fixture**, which is
the argument for asserting on values that must differ rather than on counts that
must be right.

**Confirmed at crate scale after rebasing.** The `report.rs` figure above is a
scoped run. Re-measuring the whole crate on the rebased base gives **752
mutants, 187 missed, 508 caught, 45 unviable, 12 timed out**, against 217
missed and 478 caught before — thirty moved from missed to caught and nothing
else moved, which is what a change confined to one file should look like. Both
runs' figures close against the 752 total.

That the base had changed underneath — `taiyaku-core` and `taiyaku-read` both
moved while this branch sat open, and `taiyaku-cli` depends on both — is the
reason for re-running rather than carrying the number forward. On the two
branches before this one, doing so found a real defect each time.

## The quarter that never bound, and a constant no test could see

`fingerprint.rs` measures at **127 mutants, 39 missed**, 80 caught, 4 unviable,
4 timed out — the recorded 39 exactly. After six tests it is **1 missed, 118
caught**, the other two figures unchanged: 1 + 118 + 4 + 4 = 127, and 39 - 38 =
1. The single survivor is an equivalent mutant, argued at the end.

The file was not untested. It had four tests, and they were good ones — a
letterbox that comes off, an inked margin that stays, a symmetric trim that
keeps a spread splitting down its middle, a hash unchanged by exposure. What
the survivors marked out was not neglect but four *kinds* of blind spot, none
of which more of the same tests would have found.

**A constant its own test could not see.** `partition_by_size` was covered
exactly — order preserved, every index once, the threshold inclusive, the empty
case — and every assertion in it is written in terms of `LARGE_PAGE_BYTES`, so
it agrees with the constant whatever the constant says. The two ways
`32 * 1024 * 1024` can be mutated give **1,048,608 and 33,792** — 1 MB and
33 KB — and at either one every ordinary manga page takes the decode-alone
path. Nothing fails. The run is slower, on a machine nobody is timing. The test
that kills them had to be written in page sizes rather than in the constant's
name: 3 MB is parallel, 430 MB is not.

**A bound that no fixture ever reached.** Sixteen of the 39 — the largest
single cluster in the file — are the four `MAX_SIDE` caps in `flat_border`,
`w / 4` and `h / 4` and the `w - 1 -` arithmetic around each. Every letterbox
fixture in the module pads *narrower* than a quarter, because that is what a
real letterbox does, so the caps never bound and every operator inside them was
free: `<` could be `<=`, `w - 1` could be `w + 1` or `w / 1`, `h / MAX_SIDE`
could be `h % MAX_SIDE`, and the tests went green on all of it. A 40×40 page
with a 10×10 panel floating in white — fifteen flat lines on every side against
a cap of ten — makes all four bind at once, and one `assert_eq!` on the
resulting `Border` kills all sixteen.

The cap is not incidental. It is what separates this trim from the
ink-bounding-box autocrop that was tried and rejected: without it, that page
would crop to the panel, which is content-following cropping under another
name.

**An axis nothing measured.** `flat_row`'s tolerance, and the `top + bottom`
in `crop`, survived while their left/right twins were caught. Every fixture in
the file pads the sides, because side bars are the shape the corpus has —
Tongari volume 9 is spreads padded into 1920×1080. So the horizontal scan ran
in every test and was asserted in none. The vertical `crop` pair needed
**3 and 2** rather than a matching pair, because an even border cannot tell
`top + bottom` from `top - bottom`: both read as five.

`crop`'s two `||`s went the same way. Each needs a page where exactly one axis
collapses — a border wider than the page but no taller — and a guard requiring
*both* to collapse still hands a zero-width image to a stage that will resize
it to the hash grid.

**Two entry points with no test at all.** `common_border` and `describe_pages`
could each be replaced with a constant. They take encoded pages rather than
decoded ones, which is the whole reason: testing them costs a PNG round-trip
and four tests that took a `GrayImage` directly were cheaper to write.

`common_border`'s three survivors are three different wrong answers, and one
fixture separates all of them. Twenty-four pages: a full-bleed cover at zero,
the other even pages at ten, the odd ones at twenty. Sampling every second page
— what `(len / 12).max(1)` does — sees eleven tens and one zero and answers
**10**. Sampling every page answers **20**. Sampling page zero alone answers
**0**. Taking `v[0]` instead of the median answers **0**. The correct answer is
the only one of the four that requires both the step and the median index to be
right.

That fixture is also the argument for the median stated as a test: the
full-bleed cover is page zero, page zero is always sampled, and under a minimum
it would cancel the trim for the entire volume.

**And the fixture that had to be shown it could fail.** `describe_pages`
splits a landscape page in half, and the natural test — assert two hashes come
back — passes against `width % 2` and against `width * 2` alike. The test
asserts the halves *are* `fingerprint` of the left and right crops, and before
doing so asserts that the two halves hash differently from each other and that
a half hashes differently from the whole. Both guards pass today; they are
there because the same assertion in `report.rs` was written against two pages
that hashed alike, and passed for the wrong reason.

**The four timeouts are unchanged and are not survivors.** `left += 1` → `*=`
and `right -= 1` → `/=` in the four scan loops: 0 times 1 is 0 and 39 over 10
walks down, so the loop stops terminating and `cargo-mutants` reports TIMEOUT
rather than MISSED. That is the same classification O39 records, and for the
same reason — a mutant that hangs cannot produce a silently wrong answer.

**The one survivor is equivalent, and the arm it guards is unreachable.**
`fingerprint.rs:169:25`, `||` → `&&` in `match right <= left || bottom <= top`.
Neither side can ever be true. `left` only increments while `left < w /
MAX_SIDE`, so it ends at most `w / 4`; `right` only decrements while
`right > w - 1 - w / MAX_SIDE`, so it ends at least `w - 1 - w / 4`. The gap is
therefore at least `w - 1 - 2 * (w / 4)`, and the `w < 8` guard above means
`w >= 8`, so the gap is at least **3** — swept over every width from 8 to
100,000, the minimum gap is exactly 3, at the smallest sizes. The vertical pair
is the same statement in `h`. Pixel content cannot change it: content only
makes a scan stop *earlier*, never later.

So the `true => Border::default()` arm never runs, and `&&` is
indistinguishable from `||` because both are always false. The tightest case is
in the tests as an assertion rather than as prose: a uniformly white 8×20 page,
the smallest the guard admits, comes back `Border { 2, 2, 5, 5 }` — the two
scans two and five pixels apart with fourteen columns of flat white between
them, and no way to close it.

This stops being equivalent if `MAX_SIDE` ever drops below 2, which would let
one side claim more than half the page. It is defensive code against a constant
that is not currently allowed to be wrong, and it is left in place.

## The loop that printed, the test that copied it, and a fixture in the order of its own answer

`inventory.rs`, `duplicates.rs`, `credits.rs` and `cache.rs` measure together at
**289 mutants, 34 missed** — 19, 7, 5 and 3, the recorded figures exactly — 223
caught, 22 unviable, 10 timed out. After the work: **308 mutants, 2 missed, 269
caught**, 27 unviable, 10 timed out. Both figures close, and both remaining
survivors are equivalent mutants in one function, argued at the end.

**The total went up by 19, which is the point rather than a side effect.**
Extracting a decision out of a print statement turns an expression nothing could
reach into a function a test can call — and `cargo-mutants` then generates
*more* mutants against it, including the whole-function replacements that make
up five of the six new unviable ones. 32 survivors died and 19 new mutants
appeared, every one of them caught. A file that resists mutation testing by
being unreachable scores well by being small.

**Eleven of `inventory.rs`'s 19 were inside one loop that prints.** The
`duplicated` and `settled` accumulators, the `largest` agreement count, the
`distinct`/`identical`/`n of m` column and the gate deciding which groups appear
at all — every one a decision made inside a `for` loop whose only output is
`eprintln!`. This is the shape `#41` found in `serve` and the fix is the same:
`should_print_group` was already extracted and already tested, and it was the
one thing in that loop nothing could mutate silently. The rest now follows it —
`excluded_span`, `group_by_volume`, `agreement`, `Row::column`, `rows` and
`counts`, each returning a value.

One behaviour changed with it, small and worth naming: signatures for the whole
table are now taken before any row prints, where before each row printed as it
was computed. A signature error part-way through used to leave a partial table
above the error and now leaves none.

**The test that copied the code it was testing.** `inventory.rs`'s
`a_single_unparsed_candidate_is_reported` carried the comment "Now simulate the
grouping logic from `command`" and then reimplemented fourteen lines of it —
the same `find`, the same `sort_by_key`. A copy agrees with itself whatever the
original does, so `command`'s `*volume == number` was free: inverted, every
candidate after the first joins the first group it does not belong to, and the
test still passes. The grouping is now a function both call.

The test's last two lines were the same failure in miniature — they asserted
`copies.len() < 2`, the *old* rule, written out again in the test body to show
it would have skipped the group. That is a statement about a line of code that
no longer exists. It now asserts `!should_print_group(Some(1), 1)`, which is the
same claim made against the code that is actually there.

**A fixture given in the order of its own answer.** `duplicates.rs`'s
`a_reference_edition_breaks_a_tie_on_pairings_then_runs` builds three clusters
and asserts they rank `[1, 0, 2]`. Clusters 0 and 2 tie on slots and on
pairings and separate only on runs — they are the pair the reference tiebreak
exists for — and they were listed in the input as 0 then 2. `sort_by` is
stable. Deleting the entire tiebreak arm leaves them in the order they arrived
in, which is the order the assertion wants, and the test passes with the
tiebreak gone. They are now given as 2 then 0, and the assertion can only pass
if something reordered them.

`decided` had the mirror of the same hole: every test of it either had a winner
on slots or had no reference at all, so the arm that reads the reference never
ran in any of them. It could return `false` for every reference-broken tie —
reporting "no clear winner" on the Chainsaw Man set, which is the case the
reference was passed for.

**A run of one cannot tell subtraction from division.** `credits.rs`'s `Row.run`
is `run.pages.end - run.pages.start`, and the only fixture that reaches
`classify` has a single unpaired page: `4 - 3` is 1 and `4 / 3` is also 1. The
new fixture's run is three pages, where the three operators give 3, 9 and 2.

Beside it, the recurrence lookup — `recurring.iter().find(|r| r.page ==
run.pages.start)` — searches a list holding **one entry per page in the
edition**, so inverting the comparison does not fail to find anything. It finds
page 0's entry instead, and prints another page's nearest neighbour and distance
beside this page's run, in a table whose whole purpose is to put the two
side by side. The fixture sets distances deliberately rather than observing
them: page 3 is one bit from `c2`'s third page, page 0 is five bits from its
first, so the wrong lookup reports `5` and `c2.cbz#0` where the right one
reports `1` and `c2.cbz#2`.

**`cache.rs` had no test module at all**, and only three survivors, which is
worth stating plainly: five of its eight mutants are unviable because the
functions return types with no `Default`. A low survivor count measured nothing
here. All three fall to one test — that a second run over unchanged bytes is
answered from the cache — because `entry_path` returning nothing, a version
check reversed and a digest check reversed all produce the same visible result:
a permanent miss. A cache that never hits is a slow cache, which fails no test
that is not looking for it.

**And one guard reachable only through the seam it was written for.**
`duplicates.rs`'s identity short-circuit builds `PageRange::new(0, slots - 1)`
under `stands_for[a] == stands_for[b] && slots > 0`. Dropping the second
condition underflows on an empty edition — but `Signature::of` refuses a member
with no page images in step 1, several steps before anything is fingerprinted,
so through `resolve` the count can never be zero. The test drives
`resolve_with` with a closure returning no hashes, which is the seam that
function is `pub` for, and it is written as a statement about the seam rather
than about a set of files.

**The two survivors are equivalent, both in `contradictions`.**

`duplicates.rs:448:53`, `>` → `>=` in `clusters.iter().filter(|g| g.len() > 1)`.
The only groups newly admitted are those of length one, and a group of length
one contributes nothing: the outer loop runs once with `at = 0` and the inner
loop is over `group[1..]`, which is empty. Equivalence over the whole input
domain, needing no assumption about what the caller supplies.

`duplicates.rs:450:33`, `+` → `*` in `&group[at + 1..]`, which makes it
`group[at..]`. The extra iterations are exactly `b == a`, and each looks for a
pair with `p.a == p.b`. There is one producer of `Pair` outside the tests —
`resolve_with`'s `for a in 0..copies.len() { for b in a + 1..copies.len() }` —
and it never builds one. This is the **weaker** kind of equivalence, over the
inputs the callers supply rather than over the whole domain: `contradictions` is
`pub`, and a hand-made self-pair distinguishes the two forms immediately. No
test does it, because constructing a `Pair` judging a copy against itself pins
behaviour on an input nothing produces — the same reasoning that keeps
`detect.rs`'s 37 open, applied to a much smaller thing.

## main.rs: 70 to 0, and fourteen that died by deletion

`main.rs` measures at **156 mutants, 70 missed**, 75 caught, 10 unviable, 1
timed out — the recorded 70 exactly. After: **172 mutants, 0 missed, 153
caught**, 18 unviable, 1 timed out. Every one of the seventy is dead, and the
sixteen new mutants are all caught.

The seventy sat in five command handlers, and almost none of them were
reachable from a unit test as the code stood: `topology` runs against real
archives, `align` and `verify` print, `lookup` prints, `serve` starts a server.
The fix is the one `#41` established in `serve` — extract the decision into a
named function returning a value — and it applies here at a larger scale.

**Fourteen died by deletion rather than by test, which is the best outcome
available.** `Sidecar::edge_between` already existed in `taiyaku-core` — "the
alignment relating two editions, in whichever direction it is stored" — tested,
fully caught, and `pub(crate)`. `verify` had its own copy of the two-way
comparison for the `by` it prints, and `lookup` had a third for the edge whose
read error `route` swallowed. Three copies of one predicate, seven mutants each
in the two that were not tested. Making the original `pub` and calling it
removed both copies and all fourteen mutants, and left one comparison where
there had been three.

**`topology`'s 24 were a single loop.** Four per-page accumulators, four
running totals, a verdict comparison, a self-pair skip and the guard on the
match arms — every one of them inside a doubly-nested `for` whose only output
is `println!`. They are now `ordered_pairs`, `Divergence` and `PairReport`.
Two things are worth separating there:

- The four counts are **not** slices of one number, and the fixture that kills
  them says so: five pages where two pair identically, one is lost by
  composition, one is paired only by composition, and one is paired by both to
  different counterparts. Four paired directly, one loss, one inverse, one
  disagreement — four different numbers, so none can be read off another. A
  fixture where any two of them coincided would let a mutant swap the two
  counters and pass.
- The totals became `fold` and `filter(…).count()`, which **removes** the eight
  `+=` mutants rather than killing them. A running counter initialised to zero
  and mutated to `*=` stays zero and prints zero, and the only test that can
  see it is one that asserts the total — which is exactly the test that could
  not be written while the total never left the function.

**`report_summary` prints to two streams, which is why it could not simply
return a string.** The per-run table is stdout and the figure line is stderr,
and `scripts/margin-survey.sh` reads one without the other. It returns both, as
a pair the caller prints to the right stream each — so the paired total and the
runs-per-hundred rate are assertable for the first time. Neither was: `100.0 *
runs.len() / a_len` could become `100.0 + runs.len() / a_len` and print a
number a reader would accept, and `paired += run.a.len()` could become `*=` and
report nothing paired on an alignment that paired everything.

**`main` itself is only killable from outside the process.** `replace main with
()` leaves a binary that does nothing and every unit test in the crate green,
because no unit test invokes it. `crates/taiyaku-cli/tests/cli.rs` spawns the
binary through `CARGO_BIN_EXE_taiyaku` and asserts the two contracts `scripts/`
depends on: a refusal exits 1 with `taiyaku: ` leading stderr and nothing on
stdout, and no arguments prints the usage and exits 0.

**And the re-run caught a test written in this session that could not fail.**
`route_verdict`'s test asserted `route_verdict(&doc, &Via::Hub("raw"), "fan",
"colour") == doc.trust("fan", "colour")` — and the fixture's editions are named
`raw`, `en-fan` and `en-mono`. Both sides were `None`, all three assertions
were `None == None`, and the whole function could be replaced by `None` and
pass. Nothing about the test looked wrong; the edition names were plausible for
this repository and belonged to a different fixture.

That is the fourth instance of this failure in this line of work — six in
`ddb6149`'s plan, one in `report.rs`'s hashing fixture, two found here in tests
written years apart, and now one of mine — and the first where the wrong thing
was a *name* rather than a value. It is also the clearest argument for the rule
the whole exercise runs on: **the re-run is the test of the test.** Reading it
would not have found this. Running the suite would not have found it. Only
re-measuring did, and the same discipline caught two more of my own fixtures
the same afternoon — an `ambiguous_pairings` test where one narrow margin could
not tell `< 0.05` from `== 0.05`, and a `slot_hashes` fixture whose spread was
three times an ordinary page rather than twice and so was not a spread at all.

**Confirmed at crate scale.** The six scoped runs above measure their own files;
re-running the whole crate gives **785 mutants, 47 missed, 669 caught, 57
unviable, 12 timed out**, against **752, 187, 508, 45, 12** before. Every figure
closes against its total, and the arithmetic between them closes too: 33 more
mutants, 140 fewer missed, 161 more caught, 12 more unviable, timeouts
unchanged — and 161 + 12 - 140 = 33.

The 47 are `detect.rs`'s 37, `regions.rs`'s 7 and the three equivalent mutants,
which is the same set the scoped runs left, at the same sizes, with nothing
appearing that a scoped run had not already seen. On the three branches before
this one the crate-scale re-run found a real defect twice; this time it found
none, and that null result is worth something only because the same check has
paid for itself before.

The 33 new mutants are the extracted functions. Every one is caught, which is
the argument for extraction stated as a number: pulling a decision out of a
`println!` does not merely make the old mutants killable, it hands
`cargo-mutants` more to attack — whole-function replacements, the arms of a
match that was an inline `if`, each operator in an arithmetic that had nowhere
to be called from — and all of it dies to the same tests. A file that scores
well by being unreachable is scoring on the wrong thing.

## `serde_json` errors on a map key, not a value — two PR 3 agents hit this and split

PR 3 wrote `# Errors` and `# Panics` sections for 37 functions across
`taiyaku-serve` and `taiyaku-core`, several of which end in
`serde_json::to_string`/`to_string_pretty` and had to say what makes that
call fail. It fails in exactly one way: a non-finite float (`NaN`, `inf`,
`-inf`) serialised as a **map key**. A float sitting in a struct field or a
`Vec` is a different code path and does not fail at all — it writes JSON
`null` and returns `Ok`. Measured directly against this workspace's
`serde_json 1.0.151` with a standalone repro, both halves: a struct with
`x: f64::NAN, y: f64::INFINITY` serialised as ordinary fields returned
`Ok({"x":null,"y":null,...})`; the same `NaN` used as a `BTreeMap` key (via a
wrapper type whose `Serialize` calls `serialize_f64`) returned
`Err("float key must be finite (got NaN or +/-inf)")`. The value case is
`ser.rs`'s ordinary `serialize_f64`; the key case is routed through the
separate `MapKeySerializer`, which is the only place the finite check lives.

Two agents wrote a doc section against this rule in the same PR and landed on
opposite answers. `taiyaku-core::regions::RegionDoc::to_json` and
`taiyaku-core::sidecar::Sidecar::to_json` got it right: both name the map-key
rule and both correctly note that their own float fields — `Region`'s
`x`/`y`/`w`/`h` in the first, `SolverInfo::floor`, `Quality::avg_run` and
`RunEntry::margin` in the second — are struct values, never map keys, so
`to_string_pretty` cannot fail on them; `Sidecar`'s only actual map keys
(`editions`, by `String`) are not floats either. `taiyaku-serve::notes::to_json`
got it backwards: its `# Errors` section claimed failure "requires a
`Region`'s `x`, `y`, `w` or `h` to hold `NaN` or infinity" — exactly the case
the rule says cannot fail, and exactly the case the repro above measured as
`Ok`. Fixed to match the other two.

**Four functions in this PR return `Result` or carry `# Panics` for a
failure that cannot actually happen — none of the four changed behaviour,
the doc sections now just say so instead of overclaiming or, in
`notes::to_json`'s case, misclaiming:**

- `taiyaku-serve::notes::to_json` — the map-key rule above; `Notes`/`Note`
  hold no map keyed by a float.
- `taiyaku-serve::reqlog::Entry::to_json_line` — same rule, and already
  correct when written: every `Entry` field is `String`, `u16`, `u64` or
  `BTreeMap<String, String>`, none of which can fail to encode.
- `taiyaku-core::regions::RegionDoc::to_json` — same rule; `Region`'s floats
  are values, and `pages`' `u32` keys are always finite.
- `taiyaku-core::sidecar::Sidecar::to_json` — same rule; the document's
  floats (`SolverInfo::floor`, `Quality::avg_run`, `RunEntry::margin`) are
  values, and its only map (`editions`) is keyed by `String`.

Two more in `taiyaku-core::sidecar` carry a `# Panics` section for a
different, unrelated kind of unreachable failure — an `.expect` or an index
whose panic depends on an invariant the function itself maintains, nothing to
do with `serde_json` — and both were re-read against the code rather than
trusted from the existing comment:

- `Sidecar::merge_spoke` — the `.expect("just inserted")` follows
  `self.editions.insert(name.to_string(), edition)` on the same key in the
  same statement sequence, so the entry is always there; the two indexing
  operations (`self.editions[&start]`, `self.editions[name]`) are reached
  only inside `if carried > 0`, by which point `start`'s presence was already
  confirmed by the earlier `self.editions.get(&start).ok_or_else(...)?` and
  `name` was just inserted, and the `name == start` case was already refused
  earlier in the function.
- `Sidecar::correct` — the `.expect("orientation was found by the same
  lookup")` re-finds an edge `edge_between` already located; on the only
  branch that reaches this second lookup, nothing between the two mutates
  `self.alignments`, so the second lookup cannot fail to find what the first
  one did.

## A fifth unreachable-panic site, in a different shape, and a fourth `runs_of` cause that is not O24 by another route

`taiyaku-core::credits::unpaired_runs` carries both. Its `# Panics` section is
a fifth site shaped like the four above — an `.expect` reachable only if an
invariant the function itself maintains breaks — but it was not written that
way. Where `RegionDoc::to_json`, `Sidecar::to_json`, `Sidecar::merge_spoke` and
`Sidecar::correct` each read "Panics if X. In practice it cannot: ..." and name
what currently prevents it, `unpaired_runs`'s doc read as a live conditional —
"this can only fire if the two disagreed between the two passes" — even though
the same review had already called the underlying
`.expect("every page has at least one slot")` unreachable by construction
elsewhere in its own notes. The mismatch between what the review already
believed and what the shipped prose actually said is what caught it.

Traced independently rather than taken on the review's word: `slots` (from
`target.slots(...)`) is bound once, immutably, and never mutated between the
loop that builds `unpaired_pages` (reading `slots[at].page`) and the
`slot_start` closure that re-searches that same `slots` array for a page value
read out of it moments earlier. `.position()` cannot fail to find it. The
panic is exactly as unreachable as the other four; only the prose disagreed.
Rewritten to match theirs — no behaviour change.

The function's `# Errors` section had a separate gap: it named three causes of
failure and missed a fourth `runs_of` cause, `alignment.from`/`.to` naming an
edition whose key is absent from `self.editions` altogether. That reads close
enough to O24 — "a stored alignment outlives the edition it names" — to ask
whether it is O24 reached by a new route. It is not, and the two mechanisms
are worth keeping apart:

- **O24** (`sidecar.rs::merge_spoke`, still open): a solver-written direct edge
  survives a spoke replacement because nothing marks it `by: "hand"`. The
  edition *name* persists — nothing in this crate, checked across all five,
  ever removes an edition entry — so the stale edge's far side still resolves
  by name, but its slot numbers now address different content. That can
  surface as a `runs_of` "no such slot" error or, worse, silently succeed with
  a wrong-but-well-formed answer. That silent branch is what keeps O24 open.
- **This gap**: an alignment naming an edition whose key is *absent* from
  `self.editions`. Since nothing in this codebase ever deletes an edition key,
  that state is reachable only by hand-editing `taiyaku.json` — already the
  general class the doc's "hand-edited alignment" language covers — and it is
  handled correctly today: `self.editions.get(...)` returns `None` and the
  call comes back a clean `Err`, not a panic and not a silent wrong answer.

Same family — a stale alignment reference — different code path and different
severity: O24 is a live silent-wrong-answer defect still open in
`route`/`merge_spoke`; this was a doc omission over a path that was already
correct. `unpaired_runs`'s doc now names the fourth cause and points at O24 as
the nearer live concern, with the distinction spelled out rather than left for
a reader to work out from the surface similarity.

## `member_digest`'s five call sites, and whether an empty digest can ever reach a reader

An earlier claim in the same review — that every call site of
`taiyaku_read::member_digest` sits "downstream of a successful page read" —
does not hold for all of them, checked against the five production call sites
(test code excluded) rather than assumed from the shape of one:
`describe_edition`'s write path and `verify`'s `named` closure in `main.rs`,
the fingerprint loop in `credits.rs`, `cache::member_pages` in `cache.rs`, and
`regions`' identify step in `regions.rs`. Three of the five —
`describe_edition`, `cache::member_pages` and the `regions` step — compute the
digest first and unconditionally, before anything checks the member has
pages, the opposite order the "downstream of a page read" claim assumed. So
the real question is narrower and better stated: can any of the five surface
the digest of an empty directory (SHA-256 of zero bytes,
`e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855`) as a
**successful, externally visible result**, without the command that reached
it also failing?

The answer is no, and the reason is not ordering, it is where the one durable
write lives. `Member.sha256` — the only record a digest is ever checked
against — is constructed in exactly one place, `describe_edition`, called only
from `align`'s write path:

```rust
described.push(Member {
    path: relative_to(member, beside)?.to_string_lossy().to_string(),
    sha256: taiyaku_read::member_digest(member)?,
    pages: taiyaku_read::member_pages(member)?,
});
```

The digest is computed before the page count here too, but the `?` on
`member_pages` means a page-less member fails that iteration and discards the
whole `described: Vec<Member>` built so far, digest included. No sidecar can
ever contain a `Member` whose `sha256` is the digest of an empty read — that
invariant lives at this one function, not at each caller. `RegionMember`'s
records are downstream of the same invariant transitively, through the
`regions` step below, not a second independent writer.

`verify`'s `named` closure and the `regions` identify step lean on that
invariant directly: both compute the digest unconditionally for every member,
then match it against sidecar-recorded digests (`edition_of` and
`edition_of_digests` respectively) — a lookup that, per the invariant above,
never includes the empty digest, so a page-less member simply fails to match
rather than succeeding. `named` has a second, independent guarantee besides:
by the time it runs, `read_edition` has already succeeded earlier in the same
function, and `read_edition` errors on any page-less member before `named` is
ever called. The `regions` step has one too: its later
`read_members_in_order` call re-validates non-emptiness before the write,
independent of the digest match.

The fingerprint loop in `credits.rs` calls `cache::member_pages` immediately
after computing its own digest, and that call's fate gates it: if
`cache::member_pages` fails, the `?` propagates before the fingerprint list is
ever handed to `edition_of_digests`.

`cache::member_pages` is the one site where "digest first, unconditional" is
genuinely load-bearing rather than backstopped by a sidecar lookup — a cache
hit under the empty digest's key would return `Ok` without ever calling
`read_edition`. It cannot happen through this codebase's own writes: the only
place this function writes a cache entry is after `read_edition` has already
succeeded, and `read_edition` cannot succeed for a page-less member, so no run
of this program ever populates a cache file keyed by the empty digest. Caveat,
not a defect: a JSON file dropped into the cache directory by hand under that
exact key would be served, same as any other digest would be — a general
property of a cache keyed by untrusted-provenance hashes, not something
specific to page-less members.

No live defect found across any of the five. `member_digest`'s own doc comment
(`taiyaku-read/src/pages.rs`) was left unchanged, per the review's own scope —
this trace confirms it rather than contradicting it, and corrects the
"downstream of a page read" claim to the narrower, actually-true one: the one
place a digest is durably recorded discards the whole write on a page-less
member, and every lookup site either matches against that record or against a
cache only ever populated downstream of a successful read.

## The one cast in 131 that had nothing bounding it — a zip's declared size

`docs/superpowers/specs/2026-08-10-mutation-testing-and-pedantic-design.md`
read ten of the highest-risk `cast_possible_truncation` sites before turning
the lint on and found zero live defects — every one bounded by construction.
The branch-wide pass that followed attached a per-site `#[expect]` to every
`as` cast across all five crates, stating what bounds it, with one rule for
the fourth case: if nothing bounds the value, that is a defect, fixed rather
than annotated. Across every cast this pass reviewed and closed — 131 — that
branch fired exactly once, both sites in `taiyaku-read/src/pages.rs`:

    let mut bytes = Vec::with_capacity(entry.size() as usize);

in `read_archive` and `member_page`. `entry.size()` is the uncompressed-size
field the `zip` crate reads straight from the archive's central directory —
metadata the archive declares about itself, never checked against the real
bytes until they are inflated. A `.cbz` is a user-supplied file, and nothing
between the archive and this line validates the field.

**Measured, not reasoned about.** A valid zip was built, its central
directory's uncompressed-size field patched to `0x7FFFFFF0`, and read with
this workspace's pinned `zip` 7.2.0:

    entry.size() reports: 2147483632 bytes   (2.00 GiB)
    actual inflated bytes: 104
    ATTACKER-CONTROLLED: true

**The first fix tried was `usize::try_from(entry.size()).unwrap_or(0)`, and
it does not work.** `usize::try_from(u64)` is infallible on every host this
crate targets — 64-bit; it does not compile to wasm — so `unwrap_or(0)` never
fires. The conversion always succeeds, at whatever value the archive
declared, and `Vec::with_capacity` reserves that value immediately: 2 GiB in
the probe above, and nothing caps how much higher a crafted or merely corrupt
`.cbz` could push it before the allocator itself refuses and the process
aborts. The truncation lint was satisfied — there is no truncating cast left
to fire it — while the actual exposure, an unbounded up-front reservation
driven by unvalidated file content, was untouched. `taiyaku serve` reads
archives from requests, so this reaches the server, not only the CLI.

**It is a capacity hint only.** `read_to_end` grows the buffer regardless of
what was reserved up front, so a wrong hint was never a correctness issue —
no wrong output, no memory unsafety. The harm is allocation, sized by
whatever the declared field says, and that is the whole defect: describing it
as more than that would be the same kind of inflation the field itself
practises.

**What bounds it now:** `MAX_ENTRY_CAPACITY_HINT`, a new constant in
`pages.rs`, clamps `entry.size()` before the cast —
`entry.size().min(MAX_ENTRY_CAPACITY_HINT) as usize` — so the cast is bounded
by construction (the clamp) rather than by type. No `#[expect]` needed here
at all: clippy's own range analysis follows `.min()` against a constant, so
it verifies the cast is safe rather than having to be told so — the one site
in this branch's 131 where the honest fix also happens to be the one the
lint stops complaining about on its own. Deliberately not
`taiyaku_read::MAX_DECODE_ALLOC`,
the 3 GiB ceiling `taiyaku-serve/src/cover.rs` already applies to image
decoding: that constant bounds a *decoded* pixel buffer, several times the
size of the *encoded* file bytes this hint is for, on the same page. This
project's largest known page files are the Color Walk art books' 430 MB
archival TIFFs (`taiyaku-read/src/lib.rs`), so 1 GiB clears every page this
corpus holds with better than 2x headroom while capping a hostile or corrupt
declared size's immediate reservation at a third of `MAX_DECODE_ALLOC`
instead of all of it. A capacity hint does not need to be generous to be
useful — being tight costs a reallocation on the rare oversized page, never a
wrong result.

A regression test builds a real archive and patches its central directory the
same way the probe did, rather than reimplementing the read path against a
mock: the actual bytes read back correctly despite the lie, and the buffer's
capacity never exceeds `MAX_ENTRY_CAPACITY_HINT` regardless of what the
entry declared.

Ten read, zero defects; 131 reviewed, one — and it survived a first fix that
looked complete because the lint went quiet. The lint measures whether a cast
truncates, not whether the value behind it is bounded to something sane; this
is the branch's one demonstration of the gap between the two.

The range analysis behind `.min()` clearing that cast is narrower than it
looks here, too: it only fires because `entry.size()` is a fixed-width `u64`.
A later pass reviewing `solve.rs`'s own `#[expect]`ed casts tried the same
idiom against a `usize`-sourced page index and got none of it — see "The
clamp that only works for fixed-width sources", below.

## The clamp that only works for fixed-width sources, and the helper that replaced it

Fixing the suppressions above was the design's own stopping point — every
`as` cast in the workspace got a per-site `#[expect]` stating its bound, and
that was treated as done. The pilot asked a further question: could any of
them be fixed outright instead of documented? `solve.rs` went first, as the
file leaning hardest on physical-book invariants rather than explicit guards.

Six of its `cast_possible_truncation` sites (`usize` page/run indices
narrowing to `u32`, in `solve()`'s traceback, `all_unmatched()` and
`confidence()`) looked like exactly the case the zip-size fix above had just
closed: clamp first, and let clippy's range analysis see it. It doesn't.
`.min(u32::MAX as usize) as u32` gets none of the recognition
`entry.size().min(MAX_ENTRY_CAPACITY_HINT) as usize` did — confirmed with a
minimal repro outside this crate before touching the real file. **The clamp
recognition clippy performs only fires when the cast's *source* is
fixed-width** (`u32`, `u64`, …), not `usize`/`isize` — which is exactly why
the zip-size fix worked (`entry.size()` is `u64`) and a direct `usize -> u32`
clamp on a page index does not. The workaround that does satisfy the lint:
widen to `u64` first, clamp there against a shared `MAX_U32_AS_U64` constant,
then narrow — three operations recovering the one pattern clippy recognizes.

That workaround was tried, and then rejected on its own terms rather than
clippy's: `(i as u64).min(MAX_U32_AS_U64) as u32` silences the lint without
changing any failure mode. The clamp cannot fire — `i` indexes a
`&[PageHash]`, and reaching `u32::MAX` needs four billion of them — and if it
somehow did fire, it would produce a wrong index just as silently as the
truncating `as` it replaced, now spread across three operations instead of
one. What replaced it: a single private helper, `page_index(usize) ->
u32`, built on `u32::try_from(..).expect(..)`, with the invariant argued once
in its own doc comment rather than at each of the seven call sites — an
impossible value now panics loudly instead of wrapping silently, and no
suppression is needed either, since there is no cast left for clippy to warn
about. Measured against the pre-pilot baseline on `solve.rs` alone: the clamp
idiom cost +41 lines, the helper +23.

The same helper pattern (`slot_position`, `slot_position_signed`,
`page_index` again for a different collection) then carried across
`sidecar.rs`, `credits.rs`, `route.rs` and `slot.rs` — 19 more
`cast_possible_truncation`/`cast_sign_loss` sites sharing the one invariant
that a position, length or count drawn from `Edition::slots()` cannot
approach `u32::MAX` — and the net across those four files ran the other way
from the per-site `#[expect]`s it replaced: **-72 lines** (90 insertions, 162
deletions across `taiyaku-core`), smaller than the suppressions it deleted,
not merely no bigger. The same technique closed 11 of 13 sites in
`taiyaku-serve` and 16 of 17 in `taiyaku-cli`; what stayed `#[expect]`ed in
each case had a reason the helper couldn't reach — two `taiyaku-serve` sites
trace to request-path data rather than an in-process invariant, and one
`taiyaku-cli` site (`report.rs`'s `f32`-sourced offset) has no `TryFrom` to
route through at all.

## `cast_precision_loss`: all three routes measured, none apply

Twenty-five `cast_precision_loss` sites survived the campaign above with
their `#[expect]` intact — five of them in `solve.rs` alone, the file where
the clamp-only-works-for-fixed-width-sources discovery above was made. This
is the record of why no fix reaches them, checked route by route rather than
assumed.

**Clamping does nothing.** Unlike `cast_possible_truncation`,
`cast_precision_loss` performs no range analysis at all — clamped or not,
literal or const, a `.min()` in front of the cast is invisible to it. The
`solve()` traceback casts (`i as f32`, `j as f32`) tried this first, for the
same reason the `u32`-narrowing sites did, and it does not apply here even in
principle: precision loss is a property of the *target* type's mantissa
width, not a range clippy could bound away.

**`try_from` does not apply.** There is no fallible integer-to-float
conversion to reach for: `as f32`/`as f64` on an integer always succeeds — it
rounds, it never errors — so there is no failure path for a
`try_from`-shaped helper to `.expect()` against. The pattern that closed
every `cast_possible_truncation` site above has nothing to attach to here.

**The third route — Rust's lossless `From` impls** (`f64::from(u32/i32/u16/
i16/u8/i8)`, `f32::from(u16/i16/u8/i8)`) **— was checked site by site and
fails for all 25, in three distinct ways:**

- 11 cast from `usize`: no `From<usize>` for either float type exists,
  since `usize`'s width is platform-dependent, unlike `u32`'s.
- 7 cast `u64` to `f64`: `u64` exceeds `f64`'s 52-bit mantissa.
- 7 cast `u32` to `f32`: `f64::from(u32)` exists, but the target here is
  `f32`, whose 24-bit mantissa is too narrow for `u32`'s full range.

A null result, but a measured one rather than an assumed one: every
`#[expect]` reason at these 25 sites now records that this third route was
tried and why it doesn't apply — `solve.rs`'s `i_f32`/`j_f32` casts are two
of them — closing the question for the next reader instead of leaving it
looking untried.

## O39: what the slowest test actually is, and the two bounds sized from it

Nothing bounded a test's runtime anywhere in this repository until this branch.
No `.config/nextest.toml`, no `timeout-minutes` in `.forgejo/workflows/ci.yml`,
and CI does not run nextest — the `test` job runs `cargo llvm-cov`, which wraps
`cargo test`, which has no timeout. The auto-timeout that made a hang legible
existed only inside `cargo-mutants`, which is how the gap was found: it reports
a mutant that stops a loop terminating as `TIMEOUT` rather than `MISSED`.

**The slowest test was bisected, not estimated.** Running the whole workspace
under a temporary `slow-timeout` and raising the period:

| period | SLOW lines printed | distinct tests marked |
| ------ | ------------------- | ---------------------- |
| 250 ms | 4 | 2 |
| 500 ms | 2 | 2 |
| 750 ms | 0 | 0 |

**"4" at 250 ms is a line count, not a test count — the first reading of this
table conflated the two.** nextest re-prints a `SLOW` line at *every* multiple
of the period a test is still running, not once per test: two tests finishing
between 0.5 s and 0.75 s each print once at 250 ms and again at 500 ms, four
lines from two tests. Re-measured on 2026-08-11 with `--config-file` at a
250 ms period, output captured rather than counted by eye:

```
SLOW lines at 250ms: 4
distinct tests marked: 2
     0.250s] detect::tests::the_gap_does_not_grow_with_the_page
     0.250s] cover::tests::a_cover_is_a_bounded_jpeg_whatever_the_page_was
     0.500s] detect::tests::the_gap_does_not_grow_with_the_page
     0.500s] cover::tests::a_cover_is_a_bounded_jpeg_whatever_the_page_was
Summary [   0.838s] 639 tests run: 639 passed (2 slow), 0 skipped
```

nextest's own summary says `(2 slow)`. The two are
`detect::tests::the_gap_does_not_grow_with_the_page` and
`cover::tests::a_cover_is_a_bounded_jpeg_whatever_the_page_was` — the same pair
the 500 ms row already named, which is why that row was never wrong: at a
500 ms period only one multiple elapses before either test finishes, so line
count and test count coincide there. **The corrected reading is stronger, not
weaker:** the same two tests are marked at 250 ms and again at 500 ms, and none
at 750 ms, so apart from those two, nothing in the workspace exceeds 0.25 s.
30 s is still 40x that figure and 60 s still 80x — the bound was always sized
from two tests, only ever miscounted as four by reading a reprint as a second
test. This is the same error class as the `grep -c 'corpus::'` undercount
below: output that repeats or recolours itself, miscounted by eye instead of
read from the tool's own summary line.

**A caution for anyone re-running this.** An early count of the exempt set said
"3" and was wrong — nextest colours its output, so the literal bytes `corpus::`
do not appear on every line it prints, and `grep -c 'corpus::'` undercounts.
`--color never` is required. With it, `test(corpus::)` selects the same sixteen
tests `cargo test corpus::` does.

**Job durations, from the forge's own `/actions/tasks` API**, over the ten runs
before this branch:

| job | min | max |
| --- | --- | --- |
| `test` | 32 s | 134 s |
| `lint` | 21 s | 85 s |
| `wasm` | 33 s | 84 s |
| `web` | 9 s | 25 s |

`timeout-minutes: 10` is 4.5x the worst of those.

**Switching the runner to nextest was measured, not assumed — and the first
measurement was wrong.** llvm-cov runs each nextest test in its own process
where `cargo test` threads them in one, which can move coverage. A single pair
of runs suggested nextest *improved* coverage by two lines. Repeated three times
each on an unchanged tree, with the same `--ignore-filename-regex`, that turns
out to be noise:

| | `cargo llvm-cov` | `cargo llvm-cov nextest` |
| --- | --- | --- |
| uncovered lines, three runs | 399, 399, 399 | 402, 397, 399 |
| lines | 97.69% every run | 97.68%–97.71% |
| per-file variance | none | `taiyaku-serve/src/lib.rs` 44/42/44, `routes.rs` 55/52/52 |
| `--fail-under-lines 93` | pass | pass |
| `scripts/coverage-floor.mjs` | pass, 38 files | pass, 38 files |
| wall clock, warm | 1501 ms | 915 ms |

**Coverage is unchanged; what nextest adds is nondeterminism.** `cargo test` is
deterministic to the line. nextest is not, and both varying files are in the
async server crate — process-per-test isolation changes tokio scheduling, which
is the same property that makes the per-test timeout possible.

**Neither gate is at risk, and that was checked rather than assumed.** The
aggregate floor is 93 against 97.68–97.71: 4.7 points of margin against a
0.03-point swing. The per-file floor is 57% lines, and the two varying files sit
at 90.74% and 97.62% — a three-line swing moves the worse of them by about 0.6
points, leaving 33 points of headroom.

This is a cost, not a free win: the branch trades a deterministic coverage
number for a bounded test runtime. Cheap at these margins, but anyone later
tightening `--fail-under-lines` toward the measured figure needs to know the
last digit moves on its own.

The workspace has **zero doctests**, which nextest cannot run at all — free
today, and a constraint on whoever adds the first one: they must add a
`cargo test --doc` step beside it or CI will not run it.

**The bound was also triggered on the forge itself, not only reasoned about.**
PR #47 (`o39-test-timeout` against `main`) carried a deliberate hang for one
run: a test named `tmp_hangs_to_trigger_the_ci_bound`, added in commit
`77bcc7d` and reverted in `53b2083`. Run 213 (internal id 717), built from the
hang commit, is the deliberate red run; run 214 (internal id 718), built from
the revert, is the control it is compared against.

- Red: https://forge.daveynet.xyz/davey/taiyaku/actions/runs/213
- Green: https://forge.daveynet.xyz/davey/taiyaku/actions/runs/214

| run | job | result | duration |
| --- | --- | --- | --- |
| 213 (red, hang commit) | `test` | failure | 153 s |
| 213 (red) | `lint` | success | 65 s |
| 213 (red) | `wasm` | success | 34 s |
| 213 (red) | `web` | success | 9 s |
| 214 (green, revert) | `test` | success | 117 s |
| 214 (green) | `lint` | success | 74 s |
| 214 (green) | `wasm` | success | 59 s |
| 214 (green) | `web` | success | 24 s |

Both are far under `timeout-minutes: 10` (600 s) — the job-level bound never
fired here, only the per-test one did.

**Run 215 is the only green CI run of the final tree, branch tip `71bc7cd`.**
This final review's own fixes landed and the branch was pushed again: all four
jobs succeeded — `lint` 28 s, `test` 37 s, `wasm` 36 s, `web` 9 s — comfortably
under both bounds, with neither the temporary hang nor its revert present in
this tree.

**The CI log text could not be obtained when this was first written, and that
was stated rather than implied away.** Against Forgejo `15.0.5~gitea-1.22.0`,
this build registered no job-log endpoint and no job-listing endpoint under
`/repos/{owner}/{repo}/actions/*` — verified against the instance's own
`swagger.v1.json`, and re-probed independently by a reviewer who additionally
tried `list_jobs` and `list_run_jobs`. All 404. The run-detail endpoint exposed
no step data either. So the verbatim `TIMEOUT` line originally below came from
the other end of the gap: the exact `ci.yml` command, run locally with the
hanging test still present, exiting 100.

```
$ cargo llvm-cov nextest --workspace --all-features \
    --ignore-filename-regex '(geometry_experiment|region_experiment|/corpus)\.rs$' \
    --fail-under-lines 93
        SLOW [> 30.000s] (───────) taiyaku-core sidecar::tests::tmp_hangs_to_trigger_the_ci_bound
 TERMINATING [> 60.000s] (───────) taiyaku-core sidecar::tests::tmp_hangs_to_trigger_the_ci_bound
     TIMEOUT [  60.003s] (640/640) taiyaku-core sidecar::tests::tmp_hangs_to_trigger_the_ci_bound
    (test timed out)
     Summary [  60.122s] 640 tests run: 639 passed, 1 timed out, 0 skipped
     TIMEOUT [  60.003s] (640/640) taiyaku-core sidecar::tests::tmp_hangs_to_trigger_the_ci_bound
error: test run failed
error: process didn't exit successfully: `/home/davey/.local/share/rustup/toolchains/stable-x86_64-unknown-linux-gnu/bin/cargo nextest run --manifest-path /home/davey/projects/taiyaku/Cargo.toml --target-dir /home/davey/projects/taiyaku/target/llvm-cov-target --workspace --all-features` (exit status: 100)
```

**The instance was since upgraded, and the inference is now an observation.**
Forgejo went from `15.0.5~gitea-1.22.0` to `16.0.2+gitea-1.22.0`, which
registers `/actions/runs/{id}/jobs` and `/actions/jobs/{id}/logs`. Verified
2026-08-11: `GET /api/v1/repos/davey/taiyaku/actions/jobs/3121/logs` returns
200. Job 3121 is run 213's `test` job — the hang commit, the same red run this
section already reports at 153 s — and its log, retrieved directly from the
forge, is:

```
        SLOW [> 30.000s] (───────) taiyaku-core sidecar::tests::tmp_hangs_to_trigger_the_ci_bound
 TERMINATING [> 60.000s] (───────) taiyaku-core sidecar::tests::tmp_hangs_to_trigger_the_ci_bound
     TIMEOUT [  60.003s] (640/640) taiyaku-core sidecar::tests::tmp_hangs_to_trigger_the_ci_bound
    (test timed out)
  Cancelling due to test failure: 
     Summary [  61.243s] 640 tests run: 639 passed, 1 timed out, 0 skipped
     TIMEOUT [  60.003s] (640/640) taiyaku-core sidecar::tests::tmp_hangs_to_trigger_the_ci_bound
error: test run failed
error: process didn't exit successfully: `/usr/local/rustup/toolchains/stable-x86_64-unknown-linux-gnu/bin/cargo nextest run --manifest-path /workspace/davey/taiyaku/Cargo.toml --target-dir /workspace/davey/taiyaku/target/llvm-cov-target --workspace --all-features` (exit status: 100)
```

`SLOW`, `TERMINATING` and `TIMEOUT` at exactly 30.000 s and 60.003 s, and exit
status 100, are the values `.config/nextest.toml` configures — read on the
runner, not stood in for. Against the local repro quoted above
(`Summary [  60.122s]`), the runner's `Summary [  61.243s]` is 1.121 s slower —
the delta the design guessed a shared docker runner would carry and never
measured. It is small: 1.9% of the 60 s bound, nowhere near the 80x slack that
bound already carries.

**The strongest single piece of evidence is what the red job did not do.** It
stopped at the failing "Tests, with coverage" step and never reached "Install
node" or "Coverage floor, per file", both of which the green run executed —
strictly less work. And yet it took **153 s against the green run's 117 s**,
36 s *longer* to do less. A generic infrastructure flake does not produce that
shape; a roughly-60-s per-test kill inside the test step does.

**What is observed and what is inferred here are different claims, and both
are now measured.** Observed at the time this was first written: a
`test`-job-only failure at 153 s on the hang commit, and success at 117 s on
its exact revert, both far under the 600 s job bound, on the same PR against
the same runner. Inferred at the time: that the extra time was specifically
nextest's `TIMEOUT` reading `.config/nextest.toml`'s 60 s `terminate-after`,
rather than some other kill path producing a similar time signature — the CI
log gap then meant that inference rested on the local repro standing in for
the forge, not on watching the forge do it directly. **That inference is now
an observation**: job 3121's own log, quoted above, shows the `TIMEOUT` line
at the configured period, on the runner itself.

## The cache that followed the working directory, and four rules measured against a real server

`ServeState`'s `cache_dir` was the relative `.taiyaku-cache/covers`, so where
the served cache landed depended on where `taiyaku serve` had been launched
from. That was cheap while the directory held covers. It stopped being cheap
when `?w=` shipped: the sibling `thumbs` directory now holds every
reading-size page as well as every filmstrip tile, and a cold page costs a
decode and a resize at 126 ms median against 10 ms warm. Found by noticing
120 MB of pages under `web/` after a session that had launched `serve` from
there.

It now resolves through four rules — `--cache DIR`, then `$XDG_CACHE_HOME/taiyaku`,
then `$HOME/.cache/taiyaku`, then the old relative path with a line printed to
say so. `docs/superpowers/specs/2026-08-11-serve-cache-root-design.md` is the
design; what follows is the evidence, because none of it existed in this file
before.

### The four rules, driven against the built binary

A scratch sidecar with absolute member paths outside the repository, two
`.cbz` members of two real JPEGs each, the debug binary, and the tile route.
Every figure below was read off a run rather than reasoned about.

| Launch | Rule that fired | Where the tiles landed | Files under the launch directory |
| ------ | --------------- | ---------------------- | -------------------------------- |
| dir A, no flag | `$XDG_CACHE_HOME` | `~/.cache/taiyaku/thumbs` | 0 |
| dir B, no flag | `$XDG_CACHE_HOME` | `~/.cache/taiyaku/thumbs` — **same two files, none added** | 0 |
| dir A, `--cache W/pinned` | flag | `W/pinned/thumbs`, with `covers/` created beside it | 0 |
| `env -u HOME -u XDG_CACHE_HOME` | fallback | `.taiyaku-cache/thumbs` under the launch directory | 1 |
| dir B, `--cache ""` | `$XDG_CACHE_HOME` | `~/.cache/taiyaku/thumbs` | 0 |

The second row is the defect, gone: two different launch directories produced
one cache, and the second launch generated nothing because the first had
already filled it. The fourth row is the only one that still follows the
working directory, which is what the code comments now claim and what a
`Fallback` variant exists to announce:

```
taiyaku: neither XDG_CACHE_HOME nor HOME is set; caching under .taiyaku-cache,
which follows the working directory. Pass --cache DIR to pin it.
```

`--cache` pointed at an existing *file* now complains instead of degrading in
silence, which is the shape O22 was filed for:

```
taiyaku: could not create the cache directory /etc/hosts/covers (Not a
directory (os error 20)); covers, tiles and reading-size pages will be
regenerated on every request rather than kept. Pass --cache DIR to name a
directory that can be created.
```

It serves anyway. A cache is derived data, and the design's §3 argues that a
server refusing to start over one converts a recoverable state into an
unrecoverable one — the same trade `stump::cache::load` refuses two files away.

### Two findings the gate could not see, and one of them was the whole branch

**Nothing observed the thing being changed.** `cache_dir: resolved.covers()`
in `serve` is the entire behavioural change. It could be mutated back to
`PathBuf::from(".taiyaku-cache/covers")` — leaving `resolved` in place so the
notice still compiled — and all 651 tests then in the workspace, plus
`clippy -D warnings`, plus `check-all.sh`, stayed green. The test that looked
like it covered this assigned `state.cache_dir` itself and never called
`serve`. It is now covered by a real server spawned against a decodable
fixture, asserting the generated cover and tile appear under the root `serve`
resolved; the test was confirmed to fail under that mutation before being
accepted. The mutation run also wrote `.taiyaku-cache` into
`crates/taiyaku-serve/`, which is the defect demonstrating itself.

**An empty `--cache` was worse than the defect being fixed.** `--cache ""`
produced `Resolution::Flag(PathBuf::from(""))`, so `covers()` was the relative
`covers` and `thumb_cache_dir()` the relative `thumbs` — with no `Fallback`
variant, so nothing printed. Worse, because `.gitignore` names
`.taiyaku-cache/` and not `covers/` or `thumbs/`: a `git add -A` after a
reading session could stage the generated pages, which the original defect
could not. Both environment variables were already filtered for emptiness by
an argument that applied verbatim to the flag and had not been applied to it.
The filter is now absoluteness rather than emptiness, which subsumes it and
also implements the XDG rule the module cites — a relative `$XDG_CACHE_HOME`
was being accepted while two comments asserted it could not be.

### The count that drifted, four times, in the branch that was documenting count drift

The design named five documentation sites. The plan's own verification grep,
run against the unmodified tree, found a sixth. A reviewer reading the
corrected O11 paragraph end to end found a seventh and an eighth — the
paragraph had come to contradict itself 35 lines down. A ninth was in code
rather than in `docs/`, where the grep could not reach it: `thumb_cache_dir`'s
doc comment justified the sibling arrangement by the very O11 fact this branch
retired, and `cache_root::covers()` deferred to that comment, so the new
module's stated reason for existing pointed at a dead claim.

Then the spec's §7 stated a test breakdown summing to thirteen against a total
of twelve, one section below §6 recording count drift as this repository's
recurring failure. And `status.md`'s own "639 tests" was left standing while
the branch added seventeen.

The lesson is not that counting is hard. It is the one §6 now carries: **a
per-site list is checkable and a count is not**, and every one of these was
found by running something rather than by reading.

## The fixtures that were unique but never removed

Two test helpers in `taiyaku-serve` built a temp directory per call with a
process id and a monotonic counter in the name, and neither ever removed one.
Both carried a doc comment explaining why the *name* had to be unique — Rust
runs tests in parallel threads, and a fixed name means whichever test runs
second deletes the first's fixture mid-test — and neither said anything about
who deletes it afterwards. Uniqueness stops tests colliding. It does not stop
them accumulating.

Measured on 2026-08-11, before the fix:

| Prefix | Directories | Where from |
| ------ | ----------- | ---------- |
| `taiyaku-test-real-pages-*` | 9,147 | `test_support::work_with_real_pages`, reached by 10 tests |
| `taiyaku-thumb-*` | 1,772 | `cover.rs`'s `tempdir_for_test`, reached by 1 |

**106 MB and 32,427 inodes** between them, on a 31 GB `tmpfs`. Growth is one
directory per calling test per run, so the figure is a count of test runs
times call sites rather than anything about the tests themselves.

`test_support::Fixture` replaces both: the same unique name, plus a `Drop` that
removes the directory. **`Drop` rather than a cleanup call at the end of each
test**, because the runs that leak are the ones that fail — a panicking test
never reaches its own last line, so the fixtures a failing run leaves behind
are exactly the ones nobody goes back for. `Drop::drop` ignores the removal's
result in both directions: a test that has already passed must not be failed
by a directory that would not go away, and one that has already failed must
not have its message replaced by this one.

Two tests, and the second is the one that matters. `a_fixture_removes_its_directory_when_it_is_dropped`
asserts both halves — the directory exists first and is gone after, since a
`Drop` that removed a directory never created would pass an
existence-after check and prove nothing. `a_fixture_removes_its_directory_when_the_scope_unwinds`
drives the leaking case directly through `catch_unwind`, rather than
`#[should_panic]`, because the assertion has to run *after* the unwind rather
than being the thing that panics.

Verified by counting rather than by reading: 22 `taiyaku-*` directories under
`/tmp` before, 22 after two consecutive full runs of the crate's 186 tests,
and zero of either prefix. The 22 that remain are fixed-name fixtures from
`taiyaku-cli`, which overwrite themselves each run and so are bounded rather
than leaking.

The 10,919 directories that had already accumulated were removed by hand; no
code reclaims them, and nothing needed to, since the two helpers that made
them no longer do.

## The one count a gate could derive, and the four it cannot

`docs/status.md` states a total test count twice — "Five crates, N tests" and
"Thirteen of the N are standing checks" — and nothing derived either. Both went
stale on two consecutive branches in one day: **639 stated against 656 actual**
when the cache-root branch added seventeen tests, then **656 against 658** when
the fixture branch added two. Both were caught by hand, and only because
someone happened to look.

That is the fourth place this repository has logged the same failure. The
open-items preamble's count word drifted three times, by its own italic
admission. The cache-root design's §6 miscounted its documentation sites, and
its §7 the tests it shipped — the second of those in the section immediately
below the one recording count drift as the recurring problem. The conclusion
all of them reached was that **a per-site list is checkable and a count is
not**. This is the exception, and it is worth being precise about why: this
count is *mechanically derivable*, and none of the others are. So it gets
derived; they stay a reviewer's job.

### The expensive design, and why it was wrong

The first version of this gate shelled out to `cargo nextest list
--message-format json` for a **static enumeration** of the workspace's tests,
rather than reading the run happening two steps above it. The argument was that
a list is more robust than a run: it reads no environment, executes nothing, and
cannot be perturbed by a failing suite. Three objections to using the run were
written down to support it.

**It cost 14.3 seconds a push, and that number was nearly not measured.** The
forge run came back green and the step printed its line, which is where it
would have been easy to stop. The line above it was `Finished \`test\` profile
in 14.01s` — the step had rebuilt the whole workspace, because `cargo llvm-cov
nextest` builds with coverage instrumentation and a plain `nextest list` cannot
reuse those artifacts. Timestamps on run 224 put it at **14.3 s of a 46.3 s
job, 31% of the `test` job's wall clock**, against documentation calling 1.18 s
— the local, already-warm figure — "the whole cost".

Sharing the artifacts was tried and does not work: `cargo llvm-cov nextest
list` is parsed by llvm-cov as `nextest run` with `list` as a test *filter*, not
as the `list` subcommand, and fails on `--message-format json`. Reordering the
two steps does not help either, since whichever runs first pays the
uninstrumented build. So the cost was unavoidable **given the approach** — which
is what forced the approach to change rather than the cost to be accepted.

**The three objections do not survive being looked at**, and that is the part
worth keeping:

- *"A run has two output formats."* In CI there is only ever one. The fallback
  to plain `cargo test` exists only in `check-all.sh`, on a machine missing
  `cargo-nextest`, and that branch skips this check exactly as it already skips
  the per-file coverage floor.
- *"A run's totals move with the environment."* The 271-against-658 measurement
  behind this is real — `TAIYAKU_CORPUS` pointed at nothing makes binaries abort
  and the ones after them never report. But both callers run the suite with that
  variable removed, deliberately and for unrelated reasons, and CI has no corpus
  at all. It cannot reach this.
- *"A run's totals move when something fails."* True, and now **handled** rather
  than avoided: the script refuses to compare when the report shows a failure or
  an error, and names the failures instead. A red suite gets one clear message
  rather than a second confusing one about documentation.

What ships instead is one line in `.config/nextest.toml` — `junit = { path =
"junit.xml" }` — so the run that was happening anyway writes
`target/nextest/default/junit.xml`, and the check is a `node` process over an
XML file. Verified under the exact CI invocation, not just under plain nextest:
`tests="658" skipped="0" failures="0" errors="0"`.

**The honest summary is that the expensive design was chosen first and defended
afterwards.** Three plausible-sounding reasons were assembled for a decision
already made, and none of them was tested against the actual deployment until a
reader asked why checking one number took fourteen seconds. That is the reusable
part; the fix is trivial by comparison.

### Proved by regression, six ways

Not by observing it pass. Each of these was run and its exit code checked:

| Regression | Exit | What it said |
| ---------- | :--: | ------------ |
| A stated figure edited to 657 | 1 | named `docs/status.md:78` and both numbers |
| A test added, docs untouched | 1 | 659 against 658, naming **both** lines |
| A claim reworded out of existence | 2 | named the pattern it could no longer find |
| A report of a run with 3 failures | 2 | named the failures, not the documentation |
| A report with 0 tests | 2 | refused to compare rather than agreeing |
| A report of the wrong shape | 2 | named the element it wanted |

The three 2s are the ones worth having. A gate that can no longer find what it
measures must say so rather than pass — the rule `coverage-floor.mjs` set when
it found that a coverage report with no files in it satisfied every threshold.

The second row nearly did not get proved. The first attempt regenerated the
enumeration with `>`, which this shell refuses under `noclobber`, so the check
read a **stale** JSON file and reported a pass. It was rerun with `>|`, which
is when it produced the 659 above. A regression proof that quietly measures the
previous state is the exact failure the proof exists to prevent.

### What is deliberately not gated

The same paragraph's "Thirteen of the N are standing checks gated on
`TAIYAKU_CORPUS` — ten in `crates/taiyaku-cli/src/corpus.rs` and three in
`crates/taiyaku-serve/src/corpus.rs`" is **correct**, verified by hand against
the same enumeration: 16 tests are named `corpus::…`, of which 3 are
`corpus::tests::…`, unit tests *of* the corpus-path helper rather than standing
checks. 16 − 3 = 13, split 10 and 3 across the two crates exactly as written.

It is not gated because the only rule separating those two populations is the
`corpus::tests::` name prefix, and a gate resting on a naming convention starts
lying the first time someone nests a module differently. A check that is subtly
wrong is worse than no check, because it is believed.

And `status.md`'s third test count — "nothing slower than 0.75 s in 639 tests",
inside O39's record — must never be touched: it is a dated measurement of what
was true when the slowest test was bisected. The two live sites are matched on
their surrounding prose for that reason. A looser "a number near the word
tests" pattern would sweep the historical one in and demand it be falsified on
every branch, which is how a gate teaches people to edit the record.

## What text is made of, measured against 480 hand-drawn boxes

Phase 1 of the detector redesign, 2026-08-12. Reproduce with:

```sh
TAIYAKU_CORPUS="$PWD/corpus" cargo test --release -p taiyaku-cli truth_population -- --nocapture
```

`crates/taiyaku-cli/src/truth_population.rs` partitions every labelled page's
connected components by whether their centre falls inside a scored truth box,
reading its pages through `score::resolve_labelled` so it measures **exactly the
pages `taiyaku score` scores**. All nine editions, all 45 pages, 942,214
components.

| edition | inside boxes | outside |
| --- | --- | --- |
| gotoubun-v01/raw-mono | 1,166 | 4,698 |
| gotoubun-v01/en-mono | 5,139 | 499,309 |
| gotoubun-v01/en-fan | 3,240 | 20,066 |
| kaguya-v01/raw-mono | 2,468 | 17,032 |
| kaguya-v01/en-mono | 12,655 | 182,238 |
| kobayashi-v01/raw-mono | 8,602 | 29,706 |
| kobayashi-v01/en-mono | 3,033 | 135,903 |
| oshinoko-v01/raw-mono | 978 | 4,890 |
| oshinoko-v01/en-mono | 1,486 | 9,605 |
| **total** | **38,767** | **903,447** |

| quantity | p05 | p25 | med | p75 | p95 |
| --- | --- | --- | --- | --- | --- |
| inside, longer side (px) | 1 | 1 | 2 | 7 | 30 |
| outside, longer side (px) | 1 | 1 | 2 | 2 | 4 |
| inside, longer side (‱ page height) | 4 | 4 | 8 | 40 | 141 |
| outside, longer side (‱ page height) | 3 | 4 | 6 | 6 | 17 |
| components per truth box | 4 | 11 | 22 | 43 | 271 |
| within-box merge gap (px) | 0 | 0 | 1 | 6 | 14 |
| between-box merge gap (px) | 0 | 0 | 70 | 424 | 1062 |
| true ink fraction in box | 0.054 | — | 0.127 | — | 0.303 |

**The ink row read `0.050 | — | 0.127 | — | 0.310` until 2026-08-12, and the
change is a percentile convention rather than a re-measurement.** Every other
row here is indexed `round((n − 1) · p)`; the ink line alone indexed
`trunc(n · p)`, one element apart at n = 478, so the table printed two
definitions of "p05" under one heading. `truth_population.rs` now has one,
`percentile_index`, and the row above is that convention. The median does not
move. **`MIN_INK` is unchanged at 0.05** — but its doc comment cited the p05 as
**0.050**, exactly the floor, which read as a constant set to a measured
percentile. Under the one convention the p05 is 0.054, so the floor sits just
below it rather than on it and excludes slightly fewer than 5% of real text
boxes. Recorded here rather than restated in silence, which is the whole
practice this section exists for.

Page height, one figure per page rather than a percentile over components: across
the 45 dev pages, min **1,024px**, median **2,160px**, max **3,056px**. Every
pixel-to-‱ conversion below, and in `detect.rs`, stands on this range.
**Per edition — which is the form that can answer a question this global range
cannot — see "Page height per edition, and what it refuses" below**, beside the
band sweep.

### The components inside a text box are mostly not glyphs

**This is the finding that changes the plan.** The population inside a
hand-drawn box has a median longer side of **2px — identical to the population
outside it**, and in page-height terms a median of 8 ten-thousandths, where a
printed glyph is around 200. Both populations are dominated by the same
sub-glyph material: stroke fragments, JPEG ringing, antialiasing.

A whole glyph is *rare* even inside a box. Only the top few per cent of the
inside population reaches glyph size — p95 is 30px, or 141‱ of page height.
Everything below that is the pieces glyphs shatter into at the Otsu threshold,
plus the ringing around them.

So the premise phase 2 was to be built on — that a size band separates type from
noise — **is not supported as stated**. The two distributions overlap almost
completely at the low end, and no threshold divides them.

### What a band does do, which is a different claim

The tails differ sharply: inside p75=7 and p95=30 against outside p75=2 and
p95=4. Cutting at 4px — the outside p95 — keeps roughly 35% of the inside
population and **at most 5% of the outside**, taking the welding population from
903,447 components to about 45,000.

That 4px cut is a *pixel* threshold; the shipped band cuts in ‱ of page height
instead, and until now nothing had measured the outside population in those
units directly — only converted from the pixel figure at an assumed page
height. Measured directly, the outside population's own ‱ percentiles are
p05=3 p25=4 med=6 p75=6 **p95=17**. Page heights across the 45 dev pages run
1,024px to 3,056px (median 2,160px), and converting the same 4px cut at each of
those gives 39‱ at the small end, 18‱ at the median, and 13‱ at the large end
— it is not one fixed ‱ figure, because the same pixel count is a different
fraction of every page. `BAND`'s `lo = 16` was set from the pixel figure
converted at an assumed page height, not from this population; it lands within
one ‱ of the outside p95 measured directly, which is closer than the
pixel-cut's own spread across the corpus's page heights would guarantee.

**`lo` is no longer 16, and the sentence above is superseded rather than
wrong — corrected in place on 2026-08-12.** Task 7's sweep moved it to **64**,
nearly four times the outside p95 measured here, and says explicitly that 64 is
*not* a boundary between these two populations: see "Where the band goes,
measured at eight bands over 478 hand-drawn boxes" below, which is where the
value now comes from. What the paragraph above measures is unchanged; only its
`lo` is.

That is a ~7× enrichment and a **~20× collapse in the density that does the
welding**, which was always the band's actual job: `merge_along`'s transitive
passes weld thousands of specks into one page-spanning blob because screentone
is *dense*, not because it is small. Fewer components at the same gap is a
weaker weld regardless of what the survivors are.

**Stated plainly because the distinction matters:** the band does not isolate
glyphs, and any doc comment claiming it does would be a defect under
`detect.rs`'s own rule. What it does is thin the field.

### The ceiling on any smear-based detector is about 74%

The sharper limit is not the band, it is geometry. `merge_along` can only
separate two text runs if some gap sits between them, so a run whose neighbour
is closer than the gap needed *within* a run can never come back as its own
region — it reports `merged`, by construction, at every gap.

Within-box merge gap is 1px at the median and **14px at p95**. On a ~2000px page
that is 0.7% of page height. Measured against the labels directly:

| edition | boxes with a neighbour closer than 0.7% of page height |
| --- | --- |
| gotoubun-v01/en-fan | 7 / 39 (17.9%) |
| gotoubun-v01/en-mono | 10 / 53 (18.9%) |
| gotoubun-v01/raw-mono | 10 / 44 (22.7%) |
| kaguya-v01/en-mono | **33 / 67 (49.3%)** |
| kaguya-v01/raw-mono | 14 / 57 (24.6%) |
| kobayashi-v01/en-mono | 12 / 64 (18.8%) |
| kobayashi-v01/raw-mono | 14 / 55 (25.5%) |
| oshinoko-v01/en-mono | 9 / 43 (20.9%) |
| oshinoko-v01/raw-mono | 15 / 56 (26.8%) |
| **total** | **124 / 478 (25.9%)** |

Nearest-neighbour separation over all 478 scored boxes: p05 **−0.011** (they
overlap), p25 0.0063, median 0.031, p95 0.182 of page height.

**So a perfect smear scores at most about 354 of 478 one-to-one, and the rest
are merges it cannot avoid.** The incumbent scores 2. The mechanism is nowhere
near its ceiling; what fails is finding text at all.

`kaguya-v01/en-mono` is the outlier at 49.3%, and it is the same edition
carrying 12,655 inside components — by far the most of the nine. Both figures
say the same thing about it: densely-set English lettering in tightly packed
balloons.

### What this does and does not establish

It establishes the populations, the gaps and the geometric ceiling, over every
labelled page rather than a sample. It does **not** establish that a band-passed
smear finds text — that is a question for the scorer, and the promotion rule
fixed on 2026-08-11 is unchanged and still the arbiter.

It also does not measure any discriminator other than longer side. Screentone is
characterised as much by *regularity and packing* as by size, and nothing here
measures either.

## Where the band goes, measured at eight bands over 478 hand-drawn boxes

Task 7 of the detector redesign, 2026-08-12. `BAND` shipped provisional from
Task 4 and `detect.rs`'s own header calls a constant asserted as measured
without a distribution behind it a defect. This is the measurement that settles
its `lo`. Reproduce with:

```sh
TAIYAKU_CORPUS="$PWD/corpus" cargo test --release -p taiyaku-cli band_sweep -- --nocapture
```

`crates/taiyaku-cli/src/band_sweep.rs` scores `detect::detect_page_with` at each
band over all nine labelled editions — 45 pages, 478 scored boxes — reading its
pages through `score::resolve_labelled` and scoring through `score::score_page`
at `score::COVERAGE`, so it is the same instrument at the same threshold as
`taiyaku score` and `scripts/score-dev-set.sh`. `hi` is held at 2,000 on every
row: one parameter at a time.

### The whole table, losing rows included

| `lo` (‱ page height) | 1:1 | merged | split | fragment | missed | spurious | spurious/page |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 0 | 13 | 47 | 199 | 89 | 130 | 13,248 | 294.40 |
| 8 | 31 | 55 | 200 | 92 | 100 | 8,795 | 195.44 |
| 16 | 50 | 62 | 171 | 102 | 93 | 2,473 | 54.96 |
| 32 | **57** | 70 | 165 | 83 | 103 | 580 | 12.89 |
| **64** | **48** | 73 | 152 | 81 | 124 | 303 | **6.73** |
| 128 | 14 | 62 | 69 | 52 | 281 | 159 | 3.53 |
| 256 | 8 | 39 | 10 | 3 | 418 | 79 | 1.76 |
| 512 | 2 | 19 | 2 | 1 | 454 | 19 | 0.42 |

**`lo = 0` is the control arm, and it is not the incumbent.** The low cut is
removed, but `hi` is still 2,000 where the band-free `NO_BAND`'s is `u32::MAX`,
and the detector around it is Task 5's two-axis union with Task 6's pixel-counting
`MIN_INK` rather than the one-axis detector the 0.4% baseline was measured on. It
answers "does cutting at `lo` help", which is what this sweep asks. It is not a
re-measurement of the incumbent, which scored 2.

### The verdict against the rule

The rule, fixed on 2026-08-11 and unchanged since:

> A candidate replaces the incumbent only if, over the dev set, it increases the
> count of 1:1 matches, **and** its spurious-per-page does not exceed the dev
> set's own truth-boxes-per-page (10.7). Ties go to the incumbent.

Term 1 is 1:1 > 2. Term 2 is spurious/page ≤ 10.7, i.e. total spurious ≤ 481.

| `lo` | term 1 (1:1 > 2) | term 2 (≤ 10.7/page) | passes |
| ---: | --- | --- | --- |
| 0 | pass, 13 | **fail**, 294.40 | no |
| 8 | pass, 31 | **fail**, 195.44 | no |
| 16 | pass, 50 | **fail**, 54.96 | no |
| 32 | pass, 57 | **fail**, 12.89 | no |
| 64 | pass, 48 | pass, 6.73 | **yes** |
| 128 | pass, 14 | pass, 3.53 | yes |
| 256 | pass, 8 | pass, 1.76 | yes |
| 512 | **fail**, 2 — a tie, and ties go to the incumbent | pass, 0.42 | no |

**Three bands pass; the winner is the passing band with the highest 1:1, which
is `lo = 64` at 48 one-to-one and 6.73 spurious per page.** `detect.rs`'s `BAND`
is set to `{ lo: 64, hi: 2000 }`.

**The row with the most 1:1 matches is refused.** `lo = 32` finds 57, nine more
than the winner, at 12.89 spurious per page against a ceiling of 10.7 — it misses
by 20%. It is written down here rather than quietly dropped because it is the
row a rule chosen after the fact would have taken, and the rule was not chosen
after the fact.

### The nine editions at the promoted band

`./scripts/score-dev-set.sh`, at `BAND = { lo: 64, hi: 2000 }`:

| edition | 1:1 | merged | split | fragment | missed | spurious | /page |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| gotoubun-v01/raw-mono | 0 | 6 | 10 | 2 | 26 | 27 | 5.40 |
| gotoubun-v01/en-mono | 13 | 12 | 7 | 12 | 9 | 73 | 14.60 |
| gotoubun-v01/en-fan | 10 | 2 | 5 | 6 | 16 | 30 | 6.00 |
| kaguya-v01/raw-mono | 2 | 11 | 28 | 6 | 10 | 33 | 6.60 |
| kaguya-v01/en-mono | 8 | 12 | 28 | 10 | 9 | 24 | 4.80 |
| kobayashi-v01/raw-mono | 0 | 3 | 17 | 7 | 28 | 8 | 1.60 |
| kobayashi-v01/en-mono | 8 | 13 | 15 | 12 | 16 | 30 | 6.00 |
| oshinoko-v01/raw-mono | 1 | 10 | 29 | 9 | 7 | 46 | 9.20 |
| oshinoko-v01/en-mono | 6 | 4 | 13 | 17 | 3 | 32 | 6.40 |
| **total** | **48** | **73** | **152** | **81** | **124** | **303** | **6.73** |

The nine rows sum to exactly the sweep's `lo = 64` row, which is the cross-check
worth having: the sweep drives `detect_page_with` from a test and the script
drives the shipped `taiyaku score` binary, and a disagreement between them would
mean one of the two was measuring something else.

### Per kind, which is the part a later decision turns on

`taiyaku score`'s own per-kind lines at the promoted band, summed over the nine:

| kind | found 1:1 | drawn | recall |
| --- | ---: | ---: | ---: |
| Dialogue | 43 | 389 | 0.11 |
| Sfx | 5 | 63 | 0.08 |
| Other | 0 | 19 | 0.00 |
| Credits | 0 | 3 | 0.00 |
| Narration | 0 | 2 | 0.00 |
| Logo | 0 | 2 | 0.00 |
| **total** | **48** | **478** | **0.10** |

**Every dialogue hit is on an English edition.** Per edition: gotoubun en-mono
13/49, en-fan 9/30, kaguya en-mono 7/58, kobayashi en-mono 8/47, oshinoko
en-mono 6/38 — and **0 of 38, 0 of 40, 0 of 45 and 0 of 44 on the four raw-mono
editions**, which is every Japanese raw in the dev set. The sound effects run the
other way, thinly: 2 of the 5 are on `kaguya-v01/raw-mono` and 1 on
`oshinoko-v01/raw-mono`. So this candidate finds a little English typeset
dialogue and essentially nothing else; it is not the "finds dialogue and no
sound effects" result or the "a little of each" one, but a third, sharper split
along the raw/English line. Vertical Japanese type in a balloon is not being
found at all.

### Page height per edition, and what it refuses

The row above has two readings and the sweep cannot choose between them.
Either **vertical Japanese type does not smear into one block**, or **the raw
editions are simply scanned smaller** and `BAND`'s `lo` is a fraction of page
height — 64‱ is a 6.5px cut on a 1,024px page and a 19.6px one on a 3,056px
page, three times the absolute size at one end of the corpus than the other.
The dev set's page heights had only ever been reported as one global
1,024–3,056px range over all 45 pages, which cannot separate the two.

`truth_population.rs` now prints page height per edition, inside the loop it
already ran, together with that edition's own inside and outside percentiles in
‱ of page height. No extra decode: the height is per page and the loop existed.

```sh
TAIYAKU_CORPUS="$PWD/corpus" cargo test --release -p taiyaku-cli truth_population -- --nocapture
```

Rows sorted by page height, with the dialogue recall from the section above
beside each:

| edition | page height px | dialogue 1:1 | inside ‱ p05/p25/med/p75/p95 | outside ‱ p05/p25/med/p75/p95 |
| --- | ---: | ---: | --- | --- |
| gotoubun-v01/raw-mono | 1,024 | **0 / 38** | 9 29 58 136 244 | 9 9 19 58 234 |
| kaguya-v01/raw-mono | 1,200 | **0 / 40** | 8 16 25 100 175 | 8 8 16 16 66 |
| oshinoko-v01/en-mono | 1,350 | 6 / 38 | 7 22 96 118 185 | 7 7 14 22 96 |
| gotoubun-v01/en-fan | 1,364–1,600 | 9 / 30 | 6 12 12 75 278 | 6 6 6 12 56 |
| oshinoko-v01/raw-mono | 2,160 | **0 / 44** | 13 46 101 134 185 | 4 9 18 37 199 |
| kobayashi-v01/raw-mono | 2,164–2,168 | **0 / 45** | 4 4 4 9 133 | 4 4 9 9 50 |
| kaguya-v01/en-mono | 2,250 | 7 / 58 | 4 4 4 8 115 | 4 4 4 8 13 |
| gotoubun-v01/en-mono | 3,056 | 13 / 49 | 3 6 9 19 98 | 3 3 6 6 13 |
| kobayashi-v01/en-mono | 3,056 | 8 / 47 | 3 6 19 94 153 | 3 6 6 6 13 |

**The scan-scale explanation is refused. Page height does not track the zero;
the script does, exactly.** Two rows do the refusing on their own:
`kobayashi-v01/raw-mono` at 2,164–2,168px scores **0 of 45** while
`oshinoko-v01/en-mono` at 1,350px scores **6 of 38** — a Japanese raw with 1.6
times the page height of an English edition that finds text — and
`oshinoko-v01/raw-mono` at 2,160px scores **0 of 44** against
`gotoubun-v01/en-fan` at 1,364–1,600px scoring **9 of 30**. The four zeros sit
at 1,024, 1,200, 2,160 and ~2,165px; the five non-zeros at 1,350, ~1,500,
2,250, 3,056 and 3,056. Those interleave. The raw/English split does not: it is
nine for nine.

**No other column separates them either, which is worth stating because one
nearly does.** Sorted by each edition's *own* inside population, the zeros run
4, 25, 58 and 101‱ at the median against the hits' 4, 9, 12, 19 and 96 — a zero
and a hit share the lowest value. So "the band is harsher relative to this
edition's own type" is not the explanation: `oshinoko-v01/raw-mono`'s inside
median is 101‱, meaning `lo = 64` keeps **more than half** of what that
edition's own truth boxes contain, and it still finds no dialogue at all; while
`kaguya-v01/en-mono`'s inside p75 is 8‱, meaning the same cut discards **over
three quarters** of its inside population, and it finds 7. The closest any
column comes is the outside p95 — zeros at 50, 66, 199, 234 against hits at 13,
13, 13, 56, 96 — and it still fails, because `kobayashi-v01/raw-mono` (50) sits
below two English editions that do find text (56 and 96).

**A second thing the per-edition split shows, incidentally.** The global
outside p95 of 17‱ that `BAND` and `glyph_band` are argued against is a
*pooled* figure, and the pool is not balanced: three editions contribute
817,450 of the 903,447 outside components, and all three are English mono
scans with an outside p95 of 13. Per edition the outside p95 runs 13 to 234 —
an eighteenfold spread — and on `gotoubun-v01/raw-mono` (234) and
`oshinoko-v01/raw-mono` (199) the shipped `lo = 64` sits *below* it, where on
the three en-monos it is nearly five times above. "64 is nearly four times the
outside p95" is true of the corpus pooled and true of no single raw edition.

**What is still unmeasured, and it is most of the mechanism.** Nothing has
re-counted the inside and outside populations *at* `lo = 64` — the percentiles
above bound what survives (over half of `oshinoko-v01/raw-mono`'s inside
population, under a quarter of `kobayashi-v01/raw-mono`'s) but do not count it,
and no measurement of what the band keeps per edition exists.

And the four zeros are not one failure. Two of them are **text found and cut
into pieces** rather than text missed: `kaguya-v01/raw-mono` scores 28 splits
against 10 missed of 57 boxes, `oshinoko-v01/raw-mono` 29 splits against 7
missed of 56. On those two the detector is putting predictions on the type and
getting the *shape* wrong — a segmentation failure, not a detection one, and
the same failure the whole candidate's largest column already names. The other
two fail the other way: `gotoubun-v01/raw-mono` misses 26 of 44 and
`kobayashi-v01/raw-mono` misses 28 of 55, so on those the type is not being
reached at all. Nothing here says why the four Japanese raws split into two
kinds of failure, and this branch cannot: it has one detector at one band, and
the question is about what the smear does to a column of glyphs.

### What this establishes, and what it does not

It establishes that over the dev set spurious-per-page falls monotonically with
`lo` across three orders of magnitude — 294.40 to 0.42 — while 1:1 rises to a
peak at `lo = 32` and falls away, and that the rule's ceiling cuts between 32 and
64. Under the rule fixed in advance, 64 wins. That is the whole of what the rule
was asked.

It does **not** establish that 64 generalises. This is a one-parameter fit to a
fixed 45-page dev set, which is a mild overfit and is stated as one; the 30
held-out pages are what would check it and were not opened here.

It does not settle `hi`, which was held fixed at 2,000 throughout and remains a
starting value, nor `COVERAGE`, still provisional at 0.5 — every figure above is
at that threshold and nothing here swept it.

**The failure mode has inverted, and the candidate is not close to working.** The
incumbent missed 429 of 478 boxes and proposed almost nothing (0.07 spurious per
page). At `lo = 64` the largest column is no longer `missed` (124) but `split`
(152), with 81 more fragments beside it: 233 of 478 boxes now have a prediction
sitting on them that is the wrong shape. 48 of 478 is 10%, against phase 1's
geometric ceiling of roughly 354 of 478 for any smear-based detector — so the
mechanism is at about 14% of what its own geometry allows, and what limits it is
no longer that it finds nothing.

**The band's `lo` is not a boundary between two measured populations, and the
sweep is what shows that.** Phase 1 put the outside population's p95 at 17‱ and
the inside population's median at 8‱; 64 is nearly four times the former, so the
winning cut discards most of the components a person's own boxes contain. It wins
anyway, and the reason is the one `glyph_band` already gives: what the cut removes
is what welds. A value read off the inside population would have been 8 or 16, and
both are measured here, and both lose.

## comic-text-detector through `tract`: four questions, and the head that answers them

The L2 detector design routes phase 3 through `spike/` and names three things it
refuses to guess (`2026-08-11-l2-detector-design.md` §4.1): which runtime, which
model, what licence. All three are settled here. A fourth was added when the
spike was scoped — **does a model find the vertical Japanese dialogue the
classical detector scores 0 of 167 on** — because it is the one that decides
whether phase 3 is worth running, and it is answered with pixels rather than
with a number.

Measured on this machine, 2026-08-12, against `comictextdetector.pt.onnx`,
sha256 `1a86ace74961413cbd650002e7bb4dcec4980ffa21b2f19b86933372071d718f`.

### 1. `tract` runs it, and nothing in the graph strains it

`spike/src/bin/probe.rs` reports three stages separately, because they fail for
different reasons — a format problem, a missing operator, an unresolvable shape.
All three pass:

```
stage 1 parse:    ok    producer pytorch 1.10, ir_version 7, opset ai.onnx v11
stage 2 graph:    ok    388 file nodes -> 669 tract nodes
stage 3 runnable: ok    at 1x3x1024x1024
```

Seventeen distinct operators, every one of them supported: `Add`, `AveragePool`,
`BatchNormalization`, `Concat`, `Conv`, `ConvTranspose`, `LeakyRelu`, `MaxPool`,
`Mul`, `Pow`, `Relu`, `Reshape`, `Resize`, `Sigmoid`, `Slice`, `Sub`,
`Transpose`.

**Why it was never close.** The design's worry was a YOLO-derived detector
needing operators `tract` lacks. There is **no `NonMaxSuppression` in the file at
all** — the export stops at the decoded grid and upstream does suppression in
Python (`inference.py::postprocess_yolo`). The operators that usually break a
pure-Rust runtime are the ones a YOLO export puts *after* the grid, and this
export does not contain them. `Resize` appears twice and is the one operator
`tract`'s own documentation calls a dark corner; both instances are in the
segmentation head's upsampling and both resolve.

So the `ort` fallback the spike was authorised to fall back to was never
reached, and the offline stance is intact: no native library, no build-time
binary download.

**Three outputs, not one**, which is the finding that shaped everything after it:

| output | shape | what it is |
| --- | --- | --- |
| `blk` | `1x64512x7` | YOLO grid: 3 anchors over 128², 64² and 32² cells; 7 = cx, cy, w, h, objectness, two class scores |
| `seg` | `1x1x1024x1024` | text mask at input resolution, sigmoid already applied in-graph (`Sigmoid_447`) |
| `det` | `1x2x1024x1024` | DBNet-style line head; not decoded here |

**Cost: 3.8–4.0 s per page, single-threaded CPU, and flat.** Model load is 0.7 s
once. The flatness is the useful part: pages in this sample run from 685×1024 to
4294×3056 and the time does not move, because everything is letterboxed to
1024×1024 first. Inference cost is a function of the input size, not of the
scan — which is the opposite of every classical figure in this document.

### 2. Two transcription hazards, both of which would have faked a negative

Neither is exotic and both are silent, so they are recorded rather than left in
the code:

- **The letterbox is not centred.** `imgproc_utils.py::letterbox` has its
  `dw /= 2` and `dh /= 2` lines commented out and calls
  `cv2.copyMakeBorder(im, 0, dh, 0, dw, ...)`. The page goes at the **top left**
  with black padding on the bottom and right. A centred letterbox — the obvious
  thing to write — displaces every box by half the padding.
- **The two backends feed different channel orders.**
  `inference.py::preprocess_img` converts BGR to RGB, then, *only* when
  `to_tensor` (the torch path), transposes with `[::-1]` and reverses the channel
  axis a second time, feeding BGR. The ONNX path has `to_tensor=False` and hands
  the RGB image to `cv2.dnn.blobFromImage(scalefactor=1/255)`, whose `swapRB`
  defaults to `False`. **The ONNX path is RGB.** Manga is close enough to
  greyscale that the wrong choice would look almost right, which is exactly why
  it is written down.

The control set exists for the same reason. Before running the Japanese pages
the spike ran five **English** pages, where the classical detector does find
dialogue: a blank result there would have indicted the port rather than the
model. It was not blank.

### 3. The licence, which resolves the other way from the first reading

The chain, each link checked rather than assumed:

- **comic-text-detector** — `LICENSE`, GPL-3.0, confirmed via the GitHub API.
- **The weights** are release assets of **manga-image-translator** (`beta-0.2.1`),
  also GPL-3.0 by `LICENSE`. Neither repository states separate terms for the
  weights, so the repository licence is the best available reading, and GPL-3.0
  is compatible with this project's GPL-3.0-or-later.
- **The training data** is the part worth reading, and it is where the trap is.
  One third is **Manga109-s**, one third DCM (Digital Comic Museum), one third
  synthetic.

**The Manga109 page carries two different licences, and the restrictive one is
the one a reader lands on first.** Anyone opening
`manga109.github.io/manga109-project-website/en/index.html#license` reads this at
the top:

> The permission for the use of the works contained in the Manga109 dataset
> (hereby referred to as "the dataset"), including modifications of parts of the
> images, is granted from the authors of each of the works, under the following
> conditions. **The dataset can be used solely for academic purposes.**

That is **Manga109**, and it is not the operative licence here. Further down the
same page:

> Among the works contained in the Manga109 dataset, **87 books are available for
> commercial use**, under the following conditions. This special subset of the
> Manga109 dataset is available as the Manga109-s dataset.

and, in the About section:

> Manga109 has been authorized for research use by the authors under the
> condition that it is limited to academic, non-commercial purposes. In addition,
> 87 of the 109 volumes have been newly approved for commercial use. We provide
> this subset under the name Manga109-s.

**Manga109-s is the commercially-approved 87-volume subset**, and it is the one
comic-text-detector names. Its five conditions, verbatim:

> - Redistribution of the Manga109-s dataset to third parties is forbidden.
> - When publishing results (**including pre-trained models**) obtained from
>   machine learning experiments or image processing experiments, the use of the
>   Manga109-s dataset must be indicated clearly within the published work.
> - Selling manga images within the dataset together with results obtained from
>   machine learning or image processing experiments is forbidden.
> - Direct copies or modifications of the manga images within the Manga109-s
>   dataset must not be treated as products, regardless of the product being
>   either free or being sold for a fee.
> - […] the total number of whole pages (including modifications of whole pages)
>   to be published must not exceed 20% of the entire book (volume) […]

**The first reading of dmMaze's README was wrong, and the correction is the
point.** It says "We would not (don't have the right) share training sets or
fonts publicly", which reads at first like a restriction on the model. Set beside
the Manga109-s conditions it is the opposite: it is about the **data and the
fonts**, whose redistribution is forbidden, and it says nothing against the
weights — which the dataset's own terms name as a publishable result.

So: **redistributing the weights is defensible, redistributing the training data
is not, and attribution is a hard obligation rather than a courtesy** — anything
this project ships carrying these weights must state that Manga109-s was used.
That obligation is new work for phase 3 and did not exist in the design.

**Two residual risks, neither resolved here, and both avoided entirely by the
design's existing choice.**

- **We are trusting the model author's own account of his training data.** That
  it is Manga109-**s** and not full Manga109 rests on dmMaze's README and on the
  Hugging Face model card's `dataset:hal-utokyo/Manga109-s` tag. If the full
  academic-only dataset were used instead, the clause quoted at the top of this
  section would govern and the conclusion above would invert. Nothing available
  from outside the project can check this.
- **GPL-3.0 over weights is an inference, and an awkward one.** Neither
  repository states terms for the weights separately, so the repository licence
  is the best available reading — but GPL-3.0's "corresponding source"
  obligation over a trained network has no settled meaning, and the training
  data that would be its natural referent is the one thing that may not be
  redistributed.

Both are questions about **distributing** the weights. Phase 3 fetches them by
script, checksummed and gitignored, and never commits them — a choice §4.2 made
for repository-size reasons before any of this was read, and which turns out to
avoid every question above. The spike confirms that arrangement rather than
reopening it. If a later decision ever proposes vendoring, these two paragraphs
are the reason it is not a formality.

**One mirror is mislabelled, which matters because it is the first search hit.**
`mayocream/comic-text-detector-onnx` on Hugging Face tags itself `apache-2.0`.
Its file has the same sha256 as the manga-image-translator release asset — the
same bytes, relabelled by a third party. The weights are taken from the GPL-3.0
release, not from the mirror.

### 4. It finds the dialogue the classical detector cannot, and the two heads do different jobs

Twenty pages: the four Japanese `raw-mono` editions, five labelled pages each —
the exact population where the promoted classical candidate scores **0 of 167**
dialogue boxes. Five English pages as the control. Counts are proposals, not
matches:

| edition | `blk` blocks | `seg` components | dialogue boxes | all labelled |
| --- | ---: | ---: | ---: | ---: |
| `gotoubun-v01/raw-mono` | 31 | 286 | 38 | 44 |
| `kaguya-v01/raw-mono` | 39 | 462 | 40 | 57 |
| `kobayashi-v01/raw-mono` | 43 | 368 | 45 | 55 |
| `oshinoko-v01/raw-mono` | 34 | 417 | 44 | 56 |
| **Japanese total** | **147** | **1533** | **167** | **212** |
| `gotoubun-v01/en-mono` (control) | 43 | 307 | 49 | 53 |

**The two heads are not two thresholds on the same thing.**

- `seg` recalls the text and gets the **granularity wrong**: connected components
  of the thresholded mask arrive per glyph or per column, at roughly nine
  components per labelled box. Under `score_page` that reads as `split` and
  `fragment` — the same failure the promoted classical candidate already has,
  arrived at from the opposite direction.
- `blk` is at **block** granularity, which is what the promotion rule counts, and
  its per-edition totals sit just under the dialogue counts rather than an order
  of magnitude above or below them.

**Where the difference shows, on a page that was cropped rather than counted.**
`oshinoko-v01/raw-mono` page 11 carries 20 labelled boxes of which only 9 are
dialogue; the rest is a cluster of phone-UI text labelled `other`, plus a
full-page sound effect. `blk` proposes 8, and they are the eight ordinary
dialogue balloons — it does not propose the UI text or the SFX. `seg` fires on
both correctly. So `blk`'s shortfall against *all* labelled boxes is largely
`blk` doing the narrower job, not missing text.

**What was looked at, and what was not.** Four of the twenty-five overlays were
cropped and read: `gotoubun` p87, `kobayashi` p36, `oshinoko` p11, `kaguya` p137.
On all four the `blk` boxes sit on the dialogue, one box per balloon, with a few
merges across adjacent balloons. One systematic difference is worth carrying into
phase 3: **`blk` hugs the glyphs while a hand-drawn box carries margin**, so the
predictions are consistently *tighter* than the truth. `COVERAGE` is provisional
at 0.5 and still unmeasured, and this is the first result whose scoring depends
on it in a direction anyone can predict.

**What this does not establish.** No 1:1, merged, split, fragment or missed
count. Nothing here has been through `score_page`, deliberately: `taiyaku score`
runs `detect_page` itself and cannot read predictions off disk, so scoring a
model means being inside the workspace, which is phase 3's
`model_experiment.rs`. A second scorer written in `spike/` would be the
duplication `region_experiment.rs` exists to prevent. The claim made here is the
one the pixels support — the model proposes blocks on the dialogue, on the
population where the classical detector proposes nothing — and the number is
phase 3's to produce.

## The model candidate, scored against the same 45 pages

The number the previous section deferred. Measured on this machine, 2026-08-12,
by `crates/taiyaku-cli/src/model_experiment.rs`: comic-text-detector's `blk`
head through `tract`, against `detect_page` as L2b promoted it, over the same 45
hand-labelled pages, the same nine editions, the same 478 scored boxes, through
the same `score_page` at the same provisional `COVERAGE = 0.5`. The module is
`#[cfg(test)]` and gated on `TAIYAKU_CORPUS` and `TAIYAKU_MODEL`; it is not
compiled into the binary and `detect_page` is untouched.

```
candidate      1:1  merged   split    frag  missed  spurious   /page
detect_page     48      73     152      81     124       303    6.73
model/blk      130      97       6     137     108         3    0.07
```

### The model row was measured twice, and the first measurement was wrong

**The figures above are the second ones.** The model row first read
`118 87 32 133 108 3`, and it was wrong because of a defect in this project's
port, fixed at `14cb71c`. The classical row never moved.

`suppress` was **class-aware**, faithfully to YOLOv5, so a text block whose grid
cells disagreed about which of the two class channels won survived **twice**, as
two boxes at an IoU of 0.94 to 0.99. Upstream gets away with class-aware
suppression because `inference.py` follows NMS with
`group_output(blks, lines, im_w, im_h, mask)`, which regroups the survivors; this
port has no such stage, so the duplicates reached the scorer. The fix is
`agnostic=True` on an existing YOLOv5 flag — the other setting of something
already there, not a new mechanism and not a new constant.

**The score could not see it.** It was found by the cross-edition grouping
measurement written up at the end of this document, and confirmed on the pixels
rather than inferred: `gotoubun-v01` raw 165 against `en-mono` 157 is one
balloon with two English boxes on it. Measured before the fix: **40 of 161
English regions carried such a twin, against 0 of 147 Japanese ones.**

**The reasoning above was written down before the re-run, deliberately.** These
numbers move in the direction that flatters the model, and a justification offered
after seeing them would be worth less than one offered before. What moved:

```
1:1        118 → 130      merged   87 → 97
split       32 → 6        frag    133 → 137
dialogue  112/389 → 124/389
```

**`missed` and `spurious` did not move, and that is the check.** They stand at 108
and 3 in both measurements, because removing duplicate boxes cannot change what was
never found. Both rows still sum to 478.

**Every one of the twelve extra dialogue hits is English**, which is the second
check. The Japanese pool is 47 of 167 before the fix and 47 of 167 after it —
unchanged per edition as well as pooled — while the English pool moves 65 of 222 to
77 of 222. That is exactly where the twins were, 40 of 161 English regions against
0 of 147 Japanese, so the arithmetic corroborates the diagnosis rather than merely
failing to contradict it. **The headline claim of this phase — 0 of 167 becoming 47
of 167 — is untouched by the fix.**

**One thing this correction does not reach.** The block counts in "It finds the
dialogue the classical detector cannot" above are the `spike/` crate's, and
`spike/` keeps its own copy of the `blk` decode. It was not changed at
`14cb71c`, so its English figures — including the 43 blocks on the
`gotoubun-v01/en-mono` control — were measured with class-aware suppression and
carry the same twins. They are proposal counts, not scores, on a page set the
promotion rule never read, so nothing in this section depends on them; they are
simply older than the fix, and re-running that crate is not something this
write-up did.

### Per kind, and the population the whole phase exists against

```
detect_page  Dialogue=43/389 Narration=0/2 Sfx=5/63 Logo=0/2 Credits=0/3 Other=0/19
model/blk    Dialogue=124/389 Narration=2/2 Sfx=2/63 Logo=0/2 Credits=0/3 Other=2/19
```

A pooled dialogue figure cannot say *where* the hits are, and the four Japanese
`raw-mono` editions are the whole reason the question was asked. Dialogue found
over dialogue drawn, per edition:

| edition | `detect_page` | `model/blk` |
| --- | ---: | ---: |
| `gotoubun-v01/raw-mono` | 0/38 | 17/38 |
| `kaguya-v01/raw-mono` | 0/40 | 16/40 |
| `kobayashi-v01/raw-mono` | 0/45 | 9/45 |
| `oshinoko-v01/raw-mono` | 0/44 | 5/44 |
| **Japanese total** | **0/167** | **47/167** |
| English, five editions pooled | 43/222 | 77/222 |

**0 of 167 is 47 of 167**, and it is not bought by trading the English side
away: the English pool moves 43 → 77 at the same time. The model takes more 1:1
than the classical detector in **every one of the nine editions**, English
included, and its spurious-per-page is 0.00 on all four Japanese raws and 0.40
at worst anywhere.

### The verdict, stated against the rule rather than against the outcome

The rule fixed on 2026-08-11: a candidate is promoted only if it increases 1:1
**and** keeps spurious-per-page at or under the dev set's own 10.7 truth boxes
per page, ties going to the incumbent. Term 1 is **130 against 48**. Term 2 is
**0.07 against 6.73**, both far under the 10.7 ceiling. **Both terms pass.**

That is the whole of what the rule was asked, and it is deliberately all this
paragraph claims. Promoting the model into the *binary* is a separate decision —
the design says so at §9 — and the sections below are what that decision has to
read before it is made.

### Three things moved the wrong way, and neither is small

**`fragment` rose, 81 → 137.** The model's failure shape is different, not
absent. Where the classical candidate's largest wrong-shape column is `split`
(152 against the model's 6), the model's is `fragment`: 137 hand-drawn boxes
carry a prediction that covers too little of them. Adding `merged`, `split` and
`fragment` together, 306 boxes carry a wrongly-shaped prediction under the
classical candidate and 240 under the model — an improvement, but not the kind
that lets anyone call the shape problem solved.

**Sfx recall moved the wrong way, 5/63 → 2/63.** The classical detector finds
more sound effects than the model does. Sixty-three sound-effect boxes is the
second-largest kind in the dev set after dialogue, and neither candidate is
within sight of them. This is consistent with what the spike saw on
`oshinoko-v01/raw-mono` page 11 — `blk` proposed the eight ordinary balloons and
neither the full-page sound effect nor the phone-UI text — but it is now a
number rather than one page: `blk` does the narrower job, and the narrower job
is a choice made by whoever trained it, not one this project can tune.

**`missed` rose in two English editions, even though the pooled figure fell.**
`kaguya-v01/en-mono` moved 9 → 25 missed and `kobayashi-v01/en-mono` 16 → 24 —
the model missing more of the labelled boxes on both editions than the
classical detector did. Neither shows up in the pooled 124 → 108: unlike
`fragment` and Sfx recall above, this regression is invisible in the summary
row, because the other seven editions fall by a combined 40 to net the pooled
drop of 16. No claim already made in this section is false for it — the
rule's two terms are pooled by design, not per-edition — but it is the one
per-edition regression this write-up had not named, and this project records
losing rows beside winning ones.

### The 0.07 is the incumbent's number and is not the incumbent's fact

`model/blk` proposes 3 spurious boxes over 45 pages, 0.07 per page. So did the
**incumbent** — the pre-L2b detector, in `status.md`'s "8.6% of the page"
paragraph, 3 spurious over the same 45 pages. Identical to two decimal places,
and arrived at from the opposite cause, so writing them as the same number
without the distinction would be a false equivalence:

- The incumbent's 0.07 was **it barely proposes anything at all**. Its `split`
  and `fragment` were both zero and it missed 429 of 478; the number recorded
  silence.
- The model's 0.07 sits beside **137 fragments and 97 merges**, which is proof
  that it proposes plenty. Its predictions land on labelled boxes and get the
  extent wrong; almost nothing it emits lands on bare paper.

The rule's second term is a precision ceiling, and it cannot tell those two
apart on its own. The first term is what does — 2 against 130 — which is the
reason the rule has two terms.

### `COVERAGE` bears on this table harder than on any before it

`COVERAGE` is still provisional at 0.5 and still unmeasured. Every figure above
is at that value, as every figure in this document has been.

What is new is that this result has a **predictable** interaction with it. The
overlays cropped on 2026-08-12 showed that `blk` hugs the glyphs where a
hand-drawn box carries margin, so the model's predictions are systematically
*tighter* than the truth in a way the classical detector's are not. Coverage is
measured from the truth box, so a tighter prediction spends its margin on the
`fragment` boundary rather than on the `spurious` one — which is exactly where
this table's worst column sits. Raising `COVERAGE` should move model boxes from
`1:1` toward `fragment`; lowering it should move them back. **The direction is
predictable and the size is not**, and nothing here swept it. A sweep over the
labelled set is now possible and was not run.

### The licence obligation is now attached to a result

One third of these weights' training data is **Manga109-s**, whose conditions
permit publishing results "including pre-trained models" only on the condition
that "the use of the Manga109-s dataset must be indicated clearly within the
published work". Anything this project ships carrying these weights must say so.

That obligation did not exist in the design, it is new work, and it now travels
with the promotion decision rather than sitting in a licence note: the number
above is what would justify taking the weights, and taking the weights is what
incurs the attribution. The two are one decision.

### What it cost

**44.14 s** for the whole gated module once built — 45 pages through *both*
candidates, plus the eleven synthetic unit tests beside them — in `--release`,
running in parallel over this machine's 32 cores. That is consistent with the
spike's 3.8–4.0 s per page single-threaded: 45 forward passes is about three
minutes of CPU, and the cores are what make it 45 seconds of wall clock.

Building it is the cost that is not small. A cold `--release` build of `tract`
and its dependencies took **3 m 23 s**. And `--release` is not optional: each
page pays a flood fill over every pixel for the classical row and a 1024×1024
forward pass for the model row.

### What this does not establish

**Nothing here opened the 30 sealed held-out pages.** They open once, at the
end, against whichever candidate is promoted, and this is not that. Every figure
above is over the 45-page dev set, which is the set the band was swept on and
the set every classical figure in this document was measured on. The model was
not tuned against it — `BLK_CONF` and `BLK_IOU` are upstream's defaults, taken
rather than fitted — so the overfit risk here is smaller than `band_sweep.rs`'s,
but "smaller" is not "absent" and 45 pages is 45 pages.

It also does not establish that the model is a detector this project can ship.
It establishes that it clears a rule fixed before it was measured.

### `spike/` is not deleted here, and what it still answers

`spike/src/lib.rs`'s header said the directory gets deleted once its findings
land, and the workspace `Cargo.toml` repeated it. **On 2026-08-12 it was
stripped to one file**, and the argument that had kept it alive died the day
before it was settled.

That argument was that `spike/` drew and `model_experiment.rs` did not — the
overlays were what answered the question a count could not, whether the boxes
sit on the balloons, and every correction in the sections above was found by
looking at them. `model_experiment.rs` draws them now, and better: three layers
including `detect_page`'s boxes, which the spike never had, rendered inside the
same closure that produces the numbers rather than from a second decode.
`TAIYAKU_OVERLAY_DIR` turns them on, and the measurement is byte-identical with
them on or off.

**What was left was worse than redundant.** The spike carried its own copy of
the letterbox and the `blk` decode, and that copy did not receive the
class-agnostic suppression fix. So its proposal counts — the 147 blocks against
167 dialogue boxes reported in "It finds the dialogue the classical detector
cannot", and the per-edition counts beside them — **are pre-fix figures and
carry the twins.** They were true when measured and are superseded by
`model_experiment.rs`'s. A second copy of a mechanism is a second place for it
to drift, and this one had already drifted.

**What survives is `probe`**, and it survives because it is the tool you point
at the *next* candidate rather than a record of this one. It answered "does
`tract` run this graph, and what is in it" in about ten minutes, and that answer
decided the phase: seventeen operators, all supported, opset 11, no
`NonMaxSuppression` in the file. Its three stages fail separately on purpose —
a parse failure is a format problem, a build failure names the operator, a
runnable failure is a shape it cannot resolve — and stage 2 naming the operator
is the difference between "pick another runtime" and "pick another export".
With the model question deferred pending a survey of what the platforms already
offer, a tool for evaluating whatever comes next is worth more than a frozen
record of what came before.

## The model against itself across the two editions: does it group the same way

The table above gives the model 97 `merged` beside 130 one-to-one, and the
labelling convention written down at `f165521` — `crates/taiyaku-core/src/truth.rs`
— says why that column cannot be read straight. The 478 boxes were drawn **one per
balloon even where two balloons touch**, so a detector that draws one box over a
touching pair scores `merged` while having found exactly the right text. That is a
disagreement about units, and `score_page`'s five outcomes cannot tell it apart
from genuine over-reach.

**Whether it costs anything is not a labelling question, and no relabelling would
settle it.** L2 pairs region to region. A merge the model makes on the raw page
*and* on the English page is free: both sides agree, and a reader pointing at a
balloon still lands on its counterpart. A merge on one side only breaks the
pairing — and breaks it as *model behaviour* rather than as a convention, so
redrawing the truth boxes would not touch it. The question is therefore about the
model against itself across editions, and it reads no truth boxes at all. The truth
document is consulted only for which pages somebody looked at.

**Compared by centroid containment, not IoU.** A Japanese balloon holds vertical
text and its English counterpart holds horizontal text, so one balloon yields two
boxes of very different shape whose centres agree. IoU would score that pair low
and would be measuring the writing system rather than the grouping.

Measured on this machine, 2026-08-12, by the second gated test in
`crates/taiyaku-cli/src/model_experiment.rs`: the five labelled `raw-mono` pages of
each of the four Japanese works, against each page's English counterpart, resolved
in **slots** rather than file pages so a joined spread cannot shift the whole
comparison by a page.

```
forward: of 147 raw regions, 23 have their centre in no English region, 124 in exactly one, 0 in two or more
reverse: of 141 English regions, 25 have their centre in no raw region, 115 in exactly one, 1 in two or more
twins:   0 of 147 raw and 0 of 141 English
headline: 5 of 147 raw regions hold two or more English centroids; 5 of those are not twins
          0 of 141 English regions hold two or more raw centroids
19 page pairs compared, 1 skipped (0 unpaired, 1 on a joined spread)
```

### This measurement found a defect before it answered its own question

**The headline read 23 of 147, of which only 5 were real.** The other 18 were one
balloon carrying two English boxes — the class-aware suppression corrected at
`14cb71c` and written up under "The model row was measured twice" above. Both
figures print, and after the fix they are **5 and 5**: they **converged**.

**The convergence is the check, and it is a stronger one than the numbers merely
getting smaller.** The two figures measure the same thing by different routes —
what containment can see, and what survives discounting near-identical twins — so
they should never have disagreed. They disagreed only because of the artefact.
Their agreeing is therefore what says the artefact is *gone*, where a fall from 23
to some smaller number would only have said it was *smaller*. Both figures stay
printed, and so does the twin line, precisely so that a regression shows up as them
parting again.

### What it says: the labelling-convention mismatch costs almost nothing

**5 of 147 is about 3% of raw regions, and the mirror case is zero.** When the model
draws one box over two touching balloons it overwhelmingly does the same on both
editions. Pairing maps region to region cleanly, so the units the truth boxes were
drawn in — one per balloon, even where two touch — cost this project almost
nothing.

That **retires relabelling as something this project needs to do for that reason**.
The open question `f165521` left open was whether L2 pairing needs a unit consistent
*across editions*, which is not the same requirement as one-per-balloon and can
disagree with it. It does not disagree with it here. Whether 5 in 147 is tolerable
is still a person's call, and the test asserts nothing about it — its assertions are
only that four works were compared and that regions were found on both sides. A
check that fails when the model improves is not a measurement.

### Two caveats the measurement prints about itself

**Page fractions correspond only approximately.** Regions are in page fractions, so
no explicit transform is needed — but the two editions have different aspect ratios
(685×1024 = 0.669 against 2147×3056 = 0.703, on one real pair), so the same physical
point sits at slightly different fractions on the two scans. Deltas of ±0.02 to
±0.04 printed per pair. It is deliberately not corrected for: **fitting that
transform is L2b's job**, and correcting for it here would put an unmeasured
transform underneath the only number this section produces.

**One page pair was skipped**, because its counterpart slot fell on a joined spread
and splitting that page is L2b's problem too. 19 of the 20 compared, and the skip
count prints beside the compared count so it can never be silent.

## Where the confidence floor goes, and the thing it turns out not to buy

`BLK_CONF = 0.4` came verbatim from upstream's `TextDetector.__init__`, whose
defaults were chosen for a translation pipeline rather than for this project's
promotion rule, and nothing had swept it. The reasoning for sweeping it was
arithmetic and looked strong: the rule caps spurious at **10.7 per page** and
the model sat at **0.07**, roughly 150× of unused budget, so there was an
enormous amount of precision available to trade for recall.

Eight thresholds, NMS held at `BLK_IOU` throughout — one parameter at a time,
for `band_sweep.rs`'s reason. All nine editions, 45 pages, 478 scored boxes,
`score_page` at `COVERAGE = 0.5`. Every row comes from **one forward pass per
page**: `blk` does not depend on the floor, so re-running inference per row
would have cost eight times the wall clock for identical numbers.

| conf | 1:1 | merged | split | frag | missed | spurious | /page | dialogue | ja-dialogue |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 0.02 | **147** | 76 | 36 | 162 | 57 | 25 | 0.56 | 133/389 | 47/167 |
| 0.05 | 144 | 83 | 28 | 156 | 67 | 13 | 0.29 | 133/389 | 47/167 |
| 0.10 | 141 | 84 | 24 | 152 | 77 | 9 | 0.20 | 132/389 | 47/167 |
| 0.15 | 136 | 90 | 18 | 149 | 85 | 7 | 0.16 | 129/389 | 47/167 |
| 0.20 | 133 | 95 | 10 | 147 | 93 | 6 | 0.13 | 127/389 | 47/167 |
| 0.30 | 133 | 98 | 7 | 141 | 99 | 4 | 0.09 | 127/389 | 47/167 |
| **0.40** | 130 | 97 | 6 | 137 | 108 | 3 | **0.07** | 124/389 | 47/167 |
| 0.50 | 123 | 97 | 1 | 137 | 120 | 1 | 0.02 | 118/389 | 46/167 |

`0.40` is the shipped value and the control arm; `0.50` is above it so the table
can report that raising the floor does not help either, which it does — 0.50 is
worse on every term.

**The premise was right and the conclusion is a negative result.** The budget is
real: the loosest row spends 0.56 spurious per page against a ceiling of 10.7,
still **19× under**. Lowering the floor does buy recall — `missed` falls 108 to
57 and 1:1 rises 130 to 147.

**But `ja-dialogue` is flat at 47 of 167 across seven of the eight rows.** Every
one of the nine extra pooled dialogue hits between 0.40 and 0.02 is English. The
population this model is being considered *for* — the four Japanese `raw-mono`
editions, where the classical detector scores 0 of 167 — does not move at all.

That answers the question the sweep was run to answer, in the direction nobody
wanted. The 120 Japanese dialogue boxes the model does not find are **not
low-confidence detections sitting under the floor.** They are not proposed at
any floor this sweep reached. `blk` does not see them, and no threshold makes it
see them. The sweep was framed as testing "the model's blindness or just our
floor"; the answer is blindness.

**The column that carries that finding was nearly not printed.** Pooled
`dialogue` moves 124/389 to 133/389 across the sweep and reads as a real gain.
Split out per script it is 47/167 Japanese, unchanged, and 77/222 to 86/222
English. This is the second time on this branch that a pooled figure has hidden
a per-edition effect — the first cost a plan revision — and the rule it argues
for is now three for three: **print the population the claim is about, beside
the pooled one, always.**

**And the promotion rule cannot choose a row here.** Its second term is slack
across the whole table — no row comes within an order of magnitude of the
ceiling — so it discriminates nothing, and by term 1 alone `0.02` wins with 147
against 130. Applied literally, the rule adopts `0.02`. That would be a
mistake, and the reason is worth stating: **the rule was pre-registered to
compare a candidate against an incumbent, not to pick a parameter inside one
candidate.** Term 2 was designed to block a detector that floods the page; it
was never meant to arbitrate a threshold whose whole range sits far below it. A
rule used outside the question it was written for stops being a safeguard.

**`BLK_CONF` is therefore unchanged at 0.4**, and this is recorded as a refusal
rather than an oversight. The evidence for staying: `0.02` buys nothing on the
population that matters, and it worsens the failure *shape* while doing it —
`split` 6 to 36 and `fragment` 137 to 162, so 198 boxes carry a prediction of
the wrong shape against 143 at the incumbent. Trading 17 English one-to-one
matches for 55 more wrongly-shaped boxes and zero Japanese ones is not a trade
this project's stated aims want.

**This is a one-parameter sweep over a fixed dev set, which is a mild overfit
and is stated as one.** The 30 held-out pages are what would check it and they
are not opened here.

**Where it points instead.** If the missing 120 are not under the floor, the
question moves off `blk` entirely. The spike's overlays showed the `seg` mask
firing on lettering `blk` proposes nothing for, and `seg` comes out of the same
forward pass at no extra cost. That is the next thing to measure, and it is a
different question from tuning — it is whether we are reading the whole model.

## What the platforms already do, and the two halves no public API puts together

The model was measured, it won on both terms of the promotion rule, and it was
deliberately **not** promoted on 2026-08-12 for a reason that was not about the
measurement: *a text detector may not be this project's to own.* Panels, the iOS
comic reader, has text selection that appears to lean on an OS capability rather
than on a bundled model, and if the platforms hand text detection over for
nothing then 95 MB of weights, a fetch step, a licence obligation and 4 s/page of
inference is a large cost to take on. The deferral said, in as many words,
**nobody has looked.** This is the looking.

**What kind of evidence this is, stated first, because it is weaker than every
other entry in this file.** Nothing here was run. There was no iPhone, iPad or
Mac to run it on, so every claim below is a citation to a vendor document, a
specification, or a vendor engineer's answer on a public forum, read on
2026-08-12. That is enough to answer *what an API is documented to return* and
**not** enough to answer *how well it does it on a manga page*, which is the
question this project would otherwise insist on measuring. Where the two differ
this entry says so. What would upgrade it is small and specific: twenty minutes
on any Apple device, one Japanese `raw-mono` page from the corpus, and the two
API calls named below.

The three questions came from `status.md` and are answered in the order that
turned out to matter, which is not the order they were asked.

### The browser, first, because it closes fastest

The reader is a PWA, so the only interesting web API would be one that runs in
Safari and Chrome today. There is one candidate and it fails on its own terms.

**Shape Detection's `TextDetector`.** In Chrome it is behind
`#enable-experimental-web-platform-features` — "only barcode detection is
available by default, starting in Chrome 83". It has never shipped in Safari or
Firefox. Its specification is a WICG Draft Community Group Report which states
outright that it "is not a W3C Standard nor is it on the W3C Standards Track",
and Chrome's own writeup explains the split: text detection "is not considered
stable enough across either computing platforms or character sets to be
standardized at the moment, which is why text detection has been moved to a
separate informative specification."

And then the detail that ends it before any of that matters. The spec defines
the API's scope as detection **"in images of Latin-1 text as defined in
[iso8859-1]"**. Japanese is not poorly supported by `TextDetector`; it is
outside what `TextDetector` is specified to do. A manga raw is the one input the
only web text-detection API declares out of scope.

**WebNN**, the route by which a browser could instead run a model well, reached
Candidate Recommendation in January 2026 and is origin-trial only in Chrome
147–149, with nothing shipped in Safari or Firefox and production estimated at
2027. In-browser ML today means WebGPU with ONNX Runtime Web or transformers.js
— which is not a platform capability, it is shipping the same weights through a
slower runtime.

**Safari's Live Text on an `<img>` is real and is not an API.** A reader can
long-press an image in a web page and pick "Show Text", and on macOS drag to
select over the picture. It is user-invoked, it hands the *user* the string, and
there is no JavaScript entry point and no coordinate that reaches the page. It
is a competing affordance for a reader who only wants to copy a line — worth
knowing it exists, worth not confusing with something this project could build
on.

**Answer to question 1: nothing in the browser detects Japanese text**, and the
one API that exists excludes it by specification rather than by quality.

### What the native APIs return, which is the question that decides it

Question 3 was "does such an API return lines, words, or the *blocks* pairing
needs" and it was expected to be a nuance. It is the finding.

**Apple has two text APIs and they split the two properties this project needs
between them, one each.**

`Vision`'s `RecognizeTextRequest` (`VNRecognizeTextRequest`) returns observations
with bounding boxes and quads — coordinates, which is half of what pairing wants
— and **does not read vertical Japanese.** This is not inferred from a
complaint; it is Apple's own answer. Asked directly whether tategaki can be read
with Vision, a DTS engineer replied that the Vision framework has no native
support for it and that the way through is to pre-process the image yourself:
rotate each line of characters ninety degrees, rotate each character ninety
degrees so it stands upright, read the lines right-to-left, re-render into
conventional horizontal text, and *then* run the request. A second thread
reports the same for Chinese in blunter terms — horizontal text recognises
fine, and "if even two characters are written vertically, then nothing is
recognized."

`VisionKit`'s `ImageAnalyzer`/`ImageAnalysis` — the framework behind Live Text —
**does** read vertical East Asian text, and **exposes no positions at all.** The
capability gap in Apple's favour is documented from the outside: "starting in
Sonoma the text recognition now supports vertical text for CJK languages
(Japanese, Chinese, Korean) which is not yet supported in Vision." The missing
half is documented from the inside, by a developer who wanted exactly what this
project wants: `ImageAnalysis` "does not appear to have functionality to get the
position of extracted text (words, etc.). This functionality appears to be in
place in a private unexposed API, since the `ImageAnalysisOverlayView` is able to
leverage it to show the live text interface." The public surface is the
transcript and Apple's own selection UI. The rectangles exist; they are Apple's,
and they stay there.

**So the API that can read the writing system returns no coordinates, and the API
that returns coordinates cannot read the writing system.** Taiyaku needs both at
once, on both editions, or there is nothing to pair. No public Apple API puts
them together, and the workaround Apple itself suggests — de-rotate the page
before recognising it — presumes you already know where the vertical text is,
which is the detection problem this whole sub-project is about. It is circular
for this use.

Two more, for completeness:

- **`RecognizeDocumentsRequest`** (Vision, WWDC 2025) groups lines into
  paragraphs and reads tables and lists across 26 languages, which is the first
  Apple API to return anything at *block* granularity — the level L2 pairs on.
  It is Vision, so it inherits Vision's vertical limitation, and a manga page is
  not a document: there are no paragraphs, no reading order across balloons, and
  the structure it exists to recover is not the structure a page has.
- **Android's ML Kit Text Recognition v2** is the one that returns the right
  shape: `TextBlock` → `Line` → `Element`, each with `boundingBox`,
  `cornerPoints` and a confidence, with Japanese among the supported scripts. It
  is a Google library rather than an OS capability, its documentation says
  nothing about writing direction for any language, and general-purpose OCR on
  manga is reported at roughly 40% without vertical-text handling. Android is
  also not a platform this reader targets. It is the closest thing to a free
  answer that exists and it is on the wrong device, undocumented on the one
  property that matters.

### Whether a native shell changes it — and why the Panels comparison does not transfer

Question 2 asked whether a native shell is close enough to change the answer.
It is moot in the direction it was asked: a full native iOS app, not a shell,
gets the same two halves and the same gap between them.

What the survey found instead is that **Panels is not doing what taiyaku needs,
and the difference is exactly the missing half.** Panels offers *selection*: a
reader drags over a balloon and gets a string, a translate button, a share
sheet. Apple's overlay draws that interaction and never hands the app a
rectangle — which is why `ImageAnalysis` can ship without exposing one. Taiyaku
needs a rectangle on the raw and a rectangle on the English page, in page
coordinates, to map one to the other. The capability Panels leans on is
precisely the capability that returns nothing pairable. Panels' newer
on-device Live Translations overlays translated text back into the balloons,
which does imply *someone* has the geometry; if it is built on `ImageAnalysis`
it is built on the part of it this project cannot reach.

That is the whole answer to the deferral's premise: the platforms do not hand
this over for nothing, and the reader whose feature prompted the comparison is
getting a different thing.

### The premise underneath the question, which was the wrong shape

"What can the browser PWA use, since a native iOS capability does not reach it
and the reader is a PWA first" contains an assumption worth naming, because it
is what made the OS-API question feel binding: **that detection would have to
happen where the reading happens.**

It does not. The PWA is served by `taiyaku serve`, a native Rust process running
on the reader's own machine, which already decodes pages, resizes them, caches
them under `$XDG_CACHE_HOME/taiyaku`, and already serves per-work JSON on
`/api/works/{work}/notes` and `/api/works/{work}/truth`. A `regions` route
alongside those is the same shape as what is there, and detection at 4 s/page is
a server cost paid once per page and cached, next to the ~126 ms/page decode
that is already paid and cached. The browser would receive boxes, which is all
it ever needed.

So the OS-API question binds only in a future that does not exist: a static PWA
reading from Stump or an OPDS feed with no taiyaku process behind it, or a
native app. Both are real possibilities and neither is built or planned. Asking
what iOS offers was asking what to do in an architecture this project has not
chosen.

### What it does to the decision, which is less than it looks

The deferral had two parts. The first — *the platforms may already do this* — is
now answered no, on every platform, for this writing system, and doubly no for a
browser. That part does not survive.

The second is untouched: 95 MB of weights, a fetch step and its checksum, a
Manga109-s attribution obligation, a heavyweight dependency and 4 s/page. Every
one of those costs is exactly what it was. What has changed is that there is no
longer a cheap alternative waiting to be found, so the comparison the deferral
was holding out for cannot come.

And the model does not redeem itself on the strength of that. It finds **47 of
167** Japanese dialogue boxes — better than the classical detector's 0, and not
a solution. The confidence sweep showed the other 120 are not under any
threshold. So the position after this survey is not "promote the model"; it is
that **no route is free**, and the cheapest unexplored one is still the `seg`
head, which comes out of the same forward pass at no extra cost and fires on
lettering `blk` proposes nothing for. That measurement was already the one this
branch pointed at, and nothing here displaces it.

### What is not established

- **No API was run.** Every Apple and Google claim above is documentation and
  vendor forum answers. None of it has met a manga page on this project's
  corpus.
- **The private-API observation about `ImageAnalysis` positions dates to
  December 2022**, re-checked here only against current documentation, which
  still exposes no position API. If Apple has since added one, this section is
  wrong in the direction that matters most, and one call in a playground would
  say so.
- **Vertical support is a moving target with a cheap check.** Apple could add
  tategaki to Vision in any release. The check is `supportedRecognitionLanguages`
  plus one rotated fixture, and it should be re-run before anyone cites this
  entry as a reason.
- **ML Kit on manga is unmeasured**, by this project and by its own
  documentation, on both accuracy and writing direction.
- **What Panels' Live Translations is actually built on is unknown.** The
  inference above is from what the public APIs can and cannot do, not from
  anything Panels has published.

## A borrowed page is not a reading position, and might one day want to be

Recorded on 2026-08-12 while settling
`docs/superpowers/specs/2026-08-12-gap-fallback-design.md`. **Nothing here is
measured** — it is a note about a limitation the design chose deliberately, so
that choosing differently later starts from what it would cost rather than from
scratch.

The gap fallback draws another edition's page in place of a gap. It does so as
a *view*, in the slot `Gap` occupies in `App.tsx`'s tree, leaving `at`
undefined exactly as a gap does. The consequence is that a borrowed page is not
a page as far as the rest of the reader is concerned: **no annotations overlay,
no zoom, and not a prefetch target.** Every rule that guards on `at` is
therefore unchanged, which is the entire reason the feature does not touch the
correction UI, note anchoring, `pairing.ts`'s grouping or the filmstrip.

That is a limitation rather than an ideal. A borrowed page is a real page and a
reader may well want to zoom it, or read the lender's notes on it. **What
making it a full position would have to answer, and why neither is a small
edit:**

- **Which edition's slot does a note on a borrowed page belong to?** Notes are
  anchored to an `(edition, slot)` pair. The lender's slot is the honest
  anchor — the note is on the lender's artwork — but then a note made while
  reading `en-mono` lands in `raw-mono`'s note list and is invisible from where
  it was written. Anchoring to the viewed edition is worse: that edition has no
  slot here, which is what made this a gap.
- **`mapRect` gains a case where the drawn page is neither the viewed nor the
  authority edition.** L1's transform was measured, and the measurement went
  against the design's own two candidates (`status.md` §5). A third edition in
  the middle of it is not a parameter change.
- **Prefetch would have to warm a page from an edition the reader is not
  reading**, against `cover.rs`'s two-permit `GENERATION_LIMIT` — the same
  contention that keeps page-size pre-warming unbuilt under `status.md`'s
  "Later".

**Deliberately not filed as an open item or a "Later" entry.** No reading has
yet produced the complaint, and the count that would say whether it matters —
how often a gap arises at all in real reading — is itself unmeasured (§10 of
the design). *That count is no longer unmeasured — corrected in place on
2026-08-13 rather than deleted, since this was accurate when written. It is
"How often a reader meets a gap, and who actually lends", below; the three
questions above are unaffected by it and still stand.* This note exists so
that if the complaint does arrive, the three questions above are already
written down.

## The lender rule in a browser: two directions, and the authority lending in neither

Recorded on 2026-08-13, verifying `gap-fallback` against the corpus before it
shipped. `status.md`'s "Fall back to another edition…" entry cites the two
lender results below; this is the session they come from.

**What kind of evidence this is, stated first.** One session, on one volume,
driven by an agent through Chrome DevTools, with no second observer and nothing
re-run. It is weaker than a test — a test runs on every commit and this ran
once — and its value is that it is the only thing that exercised the real
resolver, the real sidecar and real page bytes end to end, which no test in this
repository does for `App.tsx`. Every figure below was read out of the DOM rather
than off a screenshot, because an `<img>`'s `src` names the edition and its `alt`
names the page, and that is what settles which edition lent. Read the "What is
not established" section at the end before citing any of it.

### The material, and why it had to be this volume

`gotoubun-v01` is the only corpus volume with three editions — `raw-mono` (the
hub), `en-fan`, `en-mono` — and therefore the only material where `lenderFor`'s
rule does anything the rejected "the authority always lends" rule would not.
Everything else in the corpus has two, where the two rules agree by construction.

Ground truth, read out of the sidecar's own `alignments` block before touching
the reader, so the expected answers were fixed in advance rather than rationalised
after:

| pairing | verdict | paired | unpaired `raw-mono` slots |
| --- | --- | --- | --- |
| `raw-mono -> en-mono` | `sound` | 193 / 195 | **1, 6** |
| `raw-mono -> en-fan` | `fragmented` | 190 / 195 | **2-3, 7, 193-194** |

No slot is unpaired in *both*, which matters twice below: it is why every gap
here has a third edition available to lend, and it is why the swap-between-two-
borrowed-positions case that `Borrowed`'s `key` exists for is not reachable on
this volume.

### Corpus isolation, because a stray write here is invisible

`corpus/` is gitignored, so a write into it would not show in `git status` and
would not be recoverable. The sidecar was **copied** to a scratch path with every
member path rewritten absolute, and the copy served; progress, notes and
corrections then land beside the copy. `taiyaku serve <scratch> --addr
127.0.0.1:8123 --cache <scratch>/cache`, with vite proxying to it via
`TAIYAKU_SERVE` on `127.0.0.1:5199`. Afterwards **`find corpus -newermt today`
returned nothing**, which is the check that says the isolation held. The work id
the reader sees is `gotoubun-v01-taiyaku`, derived from the scratch filename
rather than from the volume — worth knowing before re-running this.

The browser console carried **no errors and no warnings** for the whole session.

### The measurement the rule rests on: two directions, neither lent by the authority

This is the finding. Both cases were chosen from the ground-truth table above,
one at each end of the volume, and in each the authority (`raw-mono`) was a
legal lender that the rule was supposed to pass over in favour of a third
edition:

| authority | slot | viewing | drew | `alt` |
| --- | --- | --- | --- | --- |
| `raw-mono` | 1 | `en-mono` | `…/en-fan/pages/43` | `page 44, borrowed from en-fan` |
| `raw-mono` | 193 | `en-fan` | `…/en-mono/pages/185` | `page 186, borrowed from en-mono` |

**The authority lent in neither direction.** The first case cross-checks against
the sidecar's run `{a: "1", b: "43"}`, which is where `raw-mono` slot 1 lands in
`en-fan`; the second is its mirror, with the roles of the two English editions
swapped, and it is the one that rules out the result being an artefact of
`en-fan` simply sorting earlier. Had either drawn `raw-mono`, the rule would be
wrong. The mark read `borrowed from en-fan page 44` and `borrowed from en-mono
page 186` respectively — 1-based, matching the `alt`.

### The other six checks

- **The mark reveals the `Gap`, offers intact.** Pressing it replaced the page
  with `No en-mono page for raw-mono page 2.`, the verdict line `This pairing is
  sound overall — this is one isolated gap, not a sign of trouble.` (matching
  `sound` from the table above), and two buttons: `→ en-mono page 1` and
  `en-mono page 2 ←`, running the way an RTL work does.
- **An offer lands on a real page and moves authority.** Clicking `en-mono page
  2 ←` drew `…/en-mono/pages/1` unmarked, and `GET …/progress` answered
  `{"edition":"en-mono","slot":1}` — authority genuinely moved, not just the
  view, since progress is written from the authority.
- **The switch.** `taiyaku.prefs.borrow` → `{"enabled":false}` put the `Gap` back
  at the same position with zero page images; `{"enabled":true}` brought the
  borrowed page and its mark back. The generated settings section is one
  `enabled` toggle, thirteenth and last, from registration alone.
- **Solo, and filling the page area, in all three modes.** The borrowed arm is
  tested before `mode` is consulted, and this is what that looks like from
  outside:

  | mode | page images in the page area | borrowed box | page area |
  | --- | --- | --- | --- |
  | `single` | 1 — `…/en-mono/pages/185` | 1646×534 | 1646×534 |
  | `two` | 1 — same | 1646×534 | 1646×534 |
  | `scroll` | 1 — same | 1646×534 | 1646×534 |

  Exactly one image in each, and the box identical to the page area rather than
  merely inside it. Repeating the switch in `two` and in `scroll` gave zero
  images and `No en-fan page for raw-mono page 194.` with the module off, and the
  marked page back with it on.

- **The arrows are inert on a borrowed page**, which was not anticipated.
  **No longer true — corrected in place on 2026-08-13 rather than deleted, since
  this is a measurement recorded at the time rather than a claim about today.**
  `session.ts`'s `step` now takes `nearestPaired`'s before/after offer on the
  pressed side when `display` is not `at`, on `borrow-lender-and-arrows`'s "An
  arrow on a gap takes the offered move, on both screens" — so on this same
  volume `ArrowLeft`/`ArrowRight` from a borrowed page now move, the same
  destinations the revealed `Gap`'s own buttons offer. What this bullet measured
  is unchanged: at the time of this session, `session.ts`'s `step` called the
  *unwrapped* `display` and refused anything that
  is not `at`; a borrowed state is a gap there, so `ArrowLeft` left the page
  exactly where it was. The consequence is practical: "turn a page away and back"
  cannot be done from the borrowed page itself. Doing it via the viewed edition —
  reveal the offers, swap to `raw-mono`, step away, step back, swap to `en-mono` —
  showed the **mark** again rather than the revealed `Gap`, so the toggle does
  reset on a page turn. It resets by *unmount*, though, not by the `key`: every
  move reachable on this volume unmounts `Borrowed` entirely.

### The filmstrip anchor, and how it was established

The method is the valuable half here, because the obvious measurement is wrong
twice. `Filmstrip`'s anchor is **centring only — deliberately no border and no
`aria-current`** (its own doc says why: in a gap neither "where you are" nor
"where this lands" is true). So `aria-current` was absent and that is correct,
not a failure. And the rail is `flex-direction: row-reverse`, so a naive
`scrollLeft` read misleads — the useful measurement is which tiles actually
intersect the rail's box.

The discriminator is that **`Chrome` unmounts the strip while hidden**, so
revealing it is always a fresh mount. A fresh mount with no anchor sits at the
rail's natural origin, showing tiles 0–14. Measured at `raw-mono` slot 193
viewing `en-fan` (235 tiles):

| state | visible tile window |
| --- | --- |
| borrowed, module on | **220 – 234** |
| gap, module off, same position | **220 – 234** |

220–234 rather than 0–14 is what says the anchor is set and honoured in a
borrowed state, and the two states agreeing is what says the borrowed case did
not change the gap's behaviour. The anchor itself is the nearest paired slot
behind — `raw-mono` 192, which is `en-fan` slot 234.

### What is not established

- **One session, one volume, one observer.** Nothing here was repeated, and no
  second person watched it. A test would be stronger; two of these behaviours now
  have jsdom tests (`App.test.tsx`, "a borrowed page where the viewed edition has
  none") and the rest do not.
- **Only the three-edition case was exercised for the rule.** The two-edition
  degeneration — where the authority lends because there is no third — was
  observed only in the jsdom fixture, not in a browser. That is most of the
  corpus. **No longer true — corrected in place on 2026-08-13 rather than
  deleted, since this is a measurement recorded at the time rather than a
  claim about today.** The degeneration itself is gone:
  `borrow-lender-and-arrows` removed the authority fallback, so `lenderFor`
  now answers nothing on a two-edition work rather than the authority's own
  page (`status.md`, "The authority stops lending"). The jsdom fixture this
  bullet names no longer demonstrates it either — it now carries a third
  edition, `fan`, for the same reason (`App.test.tsx`, "a borrowed page where
  the viewed edition has none").
- **The mirror case the rule gets wrong was not reached.** Authority `en-fan`,
  viewed `en-official`, third `raw-mono` draws the raw over the other English
  edition; no corpus volume has two English editions plus a raw in that
  arrangement, so this session could not have found it and did not.
- **`Borrowed`'s `key` was never exercised.** It exists for a swap straight from
  one borrowed position to another, which needs a slot two editions both lack,
  and `gotoubun-v01` has none. That path is unexercised rather than dead.
- **No geometry claim beyond the box comparison.** 1646×534 is one viewport at
  one window size; it says the page area was filled, not that any fit rule is
  right.
- **How often a gap arises in real reading is still unmeasured**, which is the
  number that would say whether any of this is felt or theoretical. The design's
  §10 says the same, and this session did not change it.

## How often a reader meets a gap, and who actually lends

Recorded on 2026-08-13, running
`crates/taiyaku-cli/src/borrow_frequency.rs`'s
`how_often_a_reader_meets_a_gap` against the corpus — the harness
`docs/superpowers/specs/2026-08-12-gap-fallback-design.md`'s §10 said did not
exist, and the session above closed by naming still unmeasured —

```
TAIYAKU_CORPUS="$PWD/corpus" cargo test -p taiyaku-cli borrow_frequency -- --nocapture --test-threads=1
```

— which answers `docs/superpowers/specs/2026-08-12-gap-fallback-design.md`'s
§10 directly. This is the session behind
`docs/superpowers/specs/2026-08-13-borrow-lender-and-arrows-design.md`'s §1.
The harness reads every corpus volume's `taiyaku.json` where `align --write`
already left one, rather than re-solving, so what is counted is the reader's
own `display`/`withBorrow` logic — ported to Rust against a `Binding` rather
than driven through the browser's WASM `Resolver` — over material the solver
was not being re-measured on.

**What kind of evidence this is.** Twelve corpus volumes, every ordered pair of
editions, every slot of the authority — 6,340 reader positions. It answers
what a reader would be shown at each one; it does not say whether they would
want it, which is unmeasured before and after this session.

### What a reader saw, before this branch's fix

| | count | share |
| --- | ---: | ---: |
| real page (`at`) | 6,064 | 95.65% |
| borrowed | 276 | 4.35% |
| gap surviving `borrow` | 0 | 0.00% |

Zero gaps survived — confirming on real material what `borrow.ts` had only
argued by reasoning: the authority's own slot always exists for its own
authority, so the only gap the module could leave standing was one where that
slot itself was invalid, and `restore`/`navigate` already prevent that.

Borrowing was concentrated, not evenly spread:

| reading | borrowed |
| --- | ---: |
| a fan translation (`en-fan` as authority) | 13.42%–21.67% |
| an official edition against another | 0.45%–4.46% |

The worst pair is `gotoubun-v01`, `en-fan → en-mono`: 52 of 240 positions,
21.67% — roughly one in five. The same volume read the other way,
`raw-mono → en-fan`, is 2.56%: the authority defines the slot space, and the
fan release carries 240 slots against the official volume's 194.

### Who actually lent

| lender | count | share |
| --- | ---: | ---: |
| the authority (the fallback) | 228 | 82.6% |
| a third edition | 48 | 17.4% |

**The candidate list never had more than one entry anywhere in the corpus.**
`lenderFor`'s candidates were `orderedEditions` minus the viewed edition minus
the authority, so a two-edition work left none and a three-edition work left
exactly one:

| volume shape | candidates | volumes |
| --- | ---: | ---: |
| 2 editions | 0 | 10 |
| 3 editions | 1 | 2 |

So the priority ordering the previous design reasoned about — prefer a third
edition, and the mirror case it gets wrong — never had two candidates to choose
between on real material; it needs four editions to do anything, and no corpus
work has four. Of the 228 fallbacks, 54 are in the ten two-edition volumes,
where the authority is by construction the edition the reader just left; the
other 174 are in the two three-edition volumes.

### The runs an arrow would have had to cross

104 unpaired runs across the corpus:

| run length | count |
| --- | ---: |
| 1 position | 64 |
| 2–3 | 28 |
| 4 or more | 12 |
| longest | 43 |

The 43-slot run is `gotoubun-v01`, `en-fan → en-mono` again — the fan release
ships a 44-page `c000` member the official volume has no counterpart for.

### What the 228 figure decided

82.6% of what the mark labelled "borrowed" was the page the reader had just
swapped away from — `applyIntent`'s toggle already lands on `state.authority`
whenever `viewing !== authority`, true of every gap — drawn without arrows,
annotations, zoom or prefetch, in place of the `Gap` view that named the
missing edition and offered both escapes. That is what turned "the two-edition
degeneration" from a caveat in `status.md`'s "Later" entry into a defect: the
fallback never showed a reader anything they could not already reach with one
`Space`, and showed it worse. `borrow-lender-and-arrows`'s Task 2 removed it —
`lenderFor`'s final arm answers `undefined` rather than the authority — and
re-running the same harness afterward reproduces the same total against the
new split: `positions 6340 | at 6064 | borrowed 48 | gap 228`, all 48 a third
edition, none the authority. `at` unchanged at 6,064, which is the check that
the fix moved borrows into gaps without touching whether a real page was
found. `status.md`'s "Fall back to another edition…" entry carries the
after-figures in place and points back here.

## Fifty test modules moved into their own files, and the coverage figure that fell without a test changing

Every `#[cfg(test)] mod tests { … }` in the workspace was inline in the file it
tested. Fifty of them — 48 named `tests`, plus `model_experiment`'s
`block_tests` and `letterbox_tests`, `stump/http`'s `ureq_tests`, and the two
`pub(crate) mod test_support` blocks — now sit in their own files, declared
`#[cfg(test)] mod tests;` where the block used to open. A child module in a file
has the same module path and the same access to its parent's private items as an
inline one, so this is a move and nothing else. `crates/*/tests/` was never the
alternative: an integration test is a separate crate and sees only the public
API.

### That it was only a move

Three checks, because "it still compiles" is not one of them.

| check | result |
| --- | --- |
| `cargo nextest` test names, before against after | 764 both sides, **identical to the character** |
| added lines across the 47 edited files | 50, every one of them a `mod …;` declaration |
| `cargo fmt --all -- --check`, `cargo clippy … -D warnings` | clean |

The name check is the load-bearing one. `atomic::tests::two_temp_paths_for_one_destination_differ`
is spelled the same whether the module is a block or a file, so a filter, a
`#[ignore]`, or a CI invocation naming a test by path keeps working — and had
the path changed, this comparison is what would have said so rather than a
green run.

The mechanical pass tokenised Rust rather than matching text, because two things
would have been silently wrong otherwise: a `}` inside a string literal moves a
brace counter that does not know about strings, and dedenting a raw string's
interior by four spaces **edits the string**. Lines whose first character sits
inside a string or comment were left at their original indentation; 45,919 lines
of Rust produced no such line needing manual attention, which is luck rather
than a property, but the pass would have named them.

### The figure that moved on its own

`cargo llvm-cov` 0.8.7 drops a source file from its report when the file's
**basename contains `tests`**, with no flag involved and nothing said about it.
Demonstrated inside one directory rather than inferred: `src/stump/http/`
now holds `test_support.rs`, which is in the report, and `ureq_tests.rs`, which
is not. So moving the modules out took every one of them out of coverage.

| | files | lines | covered | lines % |
| --- | ---: | ---: | ---: | ---: |
| inline, 174619c | 44 | 19486 | 18979 | 97.40% |
| in their own files | 45 | 7421 | 6986 | **94.14%** |

**Roughly 12,000 of the 19,486 lines were test code measuring itself.** Test
code is executed by definition, so it reported near 100% and lifted every file
that carried it. What is left is implementation.

The count went up by one while the lines fell by 62%, and both halves are worth
naming: the two `test_support` modules became rows of their own (`taiyaku-serve`
17 → 19), and `taiyaku-read/src/lib.rs` **left the report entirely** — once its
test module was elsewhere the file held only `mod` declarations, a `pub use` and
a `const`, and llvm-cov records nothing for a file with no executable lines.

Per-file, the largest drops, none of which are a test that stopped running:

| file | before | after |
| --- | ---: | ---: |
| `taiyaku-serve/src/lib.rs` | 90.91% | 77.85% |
| `taiyaku-cli/src/credits.rs` | 93.13% | 82.51% |
| `taiyaku-core/src/regions.rs` | 95.65% | 85.71% |
| `taiyaku-serve/src/stump/resolve.rs` | 95.07% | 87.61% |
| `taiyaku-serve/src/routes.rs` | 97.22% | 90.79% |

The lowest file in the workspace is now `taiyaku-serve/src/truth.rs` at 73.68%,
against a per-file floor of 57. The floor was left where it is: it is a collapse
detector, not a regression detector, and it still has sixteen points of slack.

### The gate got stricter without its numbers changing

`--fail-under-lines 93` is workspace-aggregate, so its blind spot is a function
of how many lines the report holds. Shrinking the report shrank the blind spot:

| | a module can go dark unnoticed if it has | files inside that |
| --- | ---: | ---: |
| inline | ≤ 857 covered lines | 39 of 44 |
| own files | ≤ **84** covered lines | 25 of 45 |

Concretely, `taiyaku-read/src/pages.rs` losing every test used to land the
workspace at 93.05% — green. It now lands at **90.18%** and the step is red. The
largest file that still gets away with it is `taiyaku-wasm/src/binding.rs` at 84
covered lines (93.01%); the smallest that does not is
`taiyaku-cli/src/credit_fixture.rs` at 87 (92.97%).

The threshold was **not** re-derived. This repository's rule for coverage
thresholds is "what was achieved, less two points", which against 94.14% gives
92. Leaving it at 93 keeps the gate no weaker than it was, on a number that now
moves about three times as far for the same untested code, and costs a margin of
1.14 points where there used to be 4.40. That is a deliberate tightening and the
first branch to add a large untested path will feel it — which is the gate
working, not a reason to move the number.

### What is still test code in the report

`taiyaku-serve/src/test_support.rs` (168 lines, 99.40%) and
`stump/http/test_support.rs` (84 lines, 100%) are `#[cfg(test)]` and counted, for
want of an `s` in the filename. They were counted before this change too, folded
into `lib.rs` and `http.rs`, so the split made them visible rather than
introducing them. Together they hold the aggregate about 0.19 points above where
an implementation-only figure would sit. Excluding them is a one-line change to
the `--ignore-filename-regex` in `check-all.sh` and `ci.yml` — both, or the two
copies disagree — and was not made here.

### The documented figures were already stale before any of this

`15041 lines, 14402 covered, 95.75%`, `38 files` and a `413 covered lines` blind
spot appear in `scripts/coverage-floor.mjs`, `.forgejo/workflows/ci.yml`,
`docs/ci.md` and `docs/status.md`. Measured at 174619c, before this branch
touched anything, the tree was already at 19486 lines, 18979 covered, 97.40%
over 44 files with an 857-line blind spot. Nothing derives those numbers, and
the test count beside them — the one figure a gate does derive — has been correct
throughout. Every one of them has been re-measured and rewritten here; the
`#[ignore]` regression proof in `docs/ci.md` is marked as measured on the inline
tree rather than restated, because replacing its four observed numbers takes
re-running it.

### The two loose ends, closed the same day

**`test_support` joined the exclusion list.** Two `#[cfg(test)] pub(crate) mod
test_support` modules had survived cargo-llvm-cov's own rule by spelling — its
default drops a basename containing `tests`, and `test_support` has no `s` — so
they were the last `#[cfg(test)]` code in the report. Naming `test_support` in
the `--ignore-filename-regex`, in `check-all.sh` and `ci.yml` both, costs 252
lines and 0.19 points:

| | files | lines | covered | lines % |
| --- | ---: | ---: | ---: | ---: |
| test modules split out | 45 | 7421 | 6986 | 94.14% |
| `test_support` excluded too | 43 | 7169 | 6735 | **93.95%** |

The value is not the 0.19: it is that one sentence is now true of the whole
report rather than nearly all of it. The exclusion list holds eleven files in
three groups — gated one-off measurement modules, corpus-gated modules, and
these two — and `docs/ci.md` names each with a reason.

**The margin under `--fail-under-lines 93` is now 68 lines.** It was 857 while
the tests were inline. Stated in lines because 0.95% sounds like room and 68
lines is not: a branch adding a 200-line module at 60% coverage turns the step
red. The threshold was kept rather than re-derived to 92, because the two-point
rule's stated reason is v8 counting what a run loaded — an instability llvm-cov
does not have, its line counts being deterministic on a fixed tree. So a tight
margin here fails honestly rather than flapping. 92 remains the number the rule
would give, and would still leave a gate six times tighter than the one standing
before this branch.

**The `#[ignore]` regression proof was re-run rather than annotated.** All 100
tests in `taiyaku-cli/src/tests.rs` ignored:

| | tests | `main.rs` lines | workspace lines | `--fail-under-lines 93` |
| --- | ---: | ---: | ---: | :--: |
| inline, as first measured | 84 | 97.23% → 2.12% | 95.75% → 82.19% | **1** |
| split, 2026-08-14 | 100 | 95.83% → **11.84%** | 93.95% → **81.32%** | **1** |

`main.rs` bottomed out near zero the first time because the ignored tests were
*in* it and took their own covered lines with them. They are in another file now,
so the 11.84% left is `main.rs` genuinely exercised by `tests/cli.rs` and by other
modules' tests — a truer reading of what one module's suite is worth. The
workspace figures barely differ (82.19% against 81.32%) because the same work
vanished from a report a third the size.

Two things were confirmed on that run rather than carried forward. `cargo
llvm-cov` still says **nothing**: 9,165 bytes to stdout, all of it the ordinary
table, and **zero to stderr**. And the per-file floor beside it does say
something —

    coverage-floor: 1 file/metric pair(s) below the per-file floor:
      crates/taiyaku-cli/src/main.rs  lines 11.84% < 57%

— exiting 1 and naming what to open. Both halves of the Rust gate were observed
firing on one regression, which is the first time that pair has been recorded
together.

### The one file that did not reach CI, and why nothing local could see it

`crates/taiyaku-cli/src/corpus/tests.rs` was written, compiled, tested and
covered on this machine, and was never committed. `.gitignore` line 8 read
`corpus/` — unanchored, and a bare pattern matches a directory of that name at
**any** depth — so the directory the splitter created for `corpus.rs`'s test
module was ignored along with the top-level corpus of manga archives it was
written for. `git add -A` said nothing. `git status` said nothing.

The local gate could not see it either, and the reason is structural rather than
an oversight: `cargo fmt`, `cargo clippy`, `cargo llvm-cov nextest`, the coverage
floors and the test-count check all read the **working tree**, where the file is
present and correct. Only something reading the **index** can tell that git is
hiding it. So the first thing to notice was CI, three minutes after the push:

    error[E0583]: file not found for module `tests`
     --> crates/taiyaku-cli/src/corpus.rs:833:1

Two fixes, and the second is the one that generalises. The pattern is now
`/corpus/`, anchored to the repository root, which is all it ever meant —
`fixtures/` and `testdata/` beside it stay unanchored deliberately, because those
names carry the copyright risk wherever they appear and the one legitimate
`fixtures/` is re-admitted by name. And `scripts/check-all.sh` now fails, before
`cargo fmt` and ahead of everything else, on any `crates/**/*.rs` that
`git ls-files --others --ignored --exclude-standard` reports. Proven both ways:
it names the file on the pre-fix tree and is silent on the fixed one.

It is a local-only check, like the corpus and browser passes, and for the
opposite reason to theirs: CI *does* catch this, and catches it late and
expensively. The point is to fail in one second on the machine that made the
mistake rather than in three minutes on the forge.
## The web file count had gone stale into a hole, and the hole was open

`scripts/coverage-floor.mjs` requires a minimum *number* of files in each
coverage report, because a file that has left the report is one no per-file check
visits and no aggregate notices. The web side's number was `minFiles: 50`,
derived when `web/src` reported 57 files: seven of margin, chosen so that the
near miss it was written for — one deleted `*` in `coverage.include`, dropping
all of `src/modules` — would trip it at 44.

`web/src` has since grown to 66 files. Re-running the same three-row experiment
on 2026-08-14:

| `coverage.include` | files | vitest | floor at 50 | floor at 59 |
| --- | ---: | --- | :--: | :--: |
| `['nope/**/*.ts']` | 0 | `Unknown%` on all four, exit 0 | 1 | 1 |
| `['src/*.ts']` | **52** | 97.20 / 93.52 / 99.00 / 99.09, exit 0 | **0** | **1** |
| `['src/**/*.ts']` | 66 | 97.49 / 93.99 / 99.10 / 99.17 | 0 | 0 |

**The middle row passed every check.** Fourteen files — the whole of
`src/modules` — leave the gate, vitest reports four green thresholds, the
per-file floor visits nothing that is missing, and the count that exists for
exactly this case reads 52 against a floor of 50. The gate was measuring 52 of 66
files and saying so nowhere.

Nothing broke it. The tree grew, both numbers grew with it, and the *gap* between
the near miss and the floor closed on its own — 44 against 50 is caught, 52
against 50 is not. The original derivation said "growth only ever moves the count
the safe way", which is true of the count and false of the gap. That sentence is
the defect, not the 50.

Re-derived by re-running rather than by scaling: 66 measured, seven of margin,
**59**. Proven at both values, which is why the table above has two verdict
columns — 50 lets the near miss through, 59 stops it and names
`web/vite.config.ts` as the file to open. The property to re-check when web/src
grows another ten files is "the near miss still trips it", not "the count is
still above the floor".

### Two things measured on the way past

**The four web thresholds have narrowed.** They were set at 98.03 / 94.38 /
99.29 / 99.53 less two points; the clean tree now reads **97.49 / 93.99 / 99.10 /
99.17**, so the statements margin is 1.49 rather than 2. Every threshold still
passes and none was lowered — the number to move when this fires is the coverage,
not the bar — but the two-point rule is no longer quite true of the tree it was
derived from.

**Three of the four per-file floors have narrowed the same way**, and the lowest
files have changed hands: statements is now `Labeller.tsx` at 91.85% against a
floor of 91 (0.85 of margin, where it was `fixtures.ts` at 93.10), branches
`api.ts` at 79.41 against 78, functions `decode.ts` at 93.33 against 91, lines
`api.ts` at 93.02 against 92. `statements` is the one to watch; the fix when it
fires is a test for `Labeller.tsx` rather than an 89 in the script.

## Three measurements Task 10 owed to this file: chapter markers, `notify`'s build cost, and what a scan itself costs

`docs/status.md`'s own preamble promises every claim there traces to a
measurement recorded here. Three landed over the scanner branch without a
corresponding entry — this closes that.

### The kanji counter survey, run against the development corpus on 2026-08-17

`taiyaku-read/src/naming.rs`'s `CHAPTER_MARKERS` and `VOLUME_MARKERS` decide
which kanji counters `chapter_number`/`volume_number_marked` recognise after
`第`. Counted across the development corpus (`corpus/`, this checkout), on the
form the parsers actually fire on — `第`, digits, the counter — in the
basename of every entry the walk would meet:

| counter | files | directories | total |
| --- | ---: | ---: | ---: |
| `第N話` | 577 | 0 | **577** |
| `第N巻` | 497 | 37 | **534** |
| `第N章` | 0 | 0 | **0** |
| `第N回` | 0 | 0 | **0** |
| `第N卷` | 0 | 0 | **0** |

Re-measured 2026-08-18, with the method written down, because the figure this
replaces was not. It said `巻` **498**, and 498 reproduces under no reading:
not 497 (files), not 534 (files and directories), not 729 (files carrying `巻`
anywhere in the name) and not 766 (all entries carrying it anywhere). `話` 577
is exact under every one of them, since no directory in this corpus carries
`話` at all — which is why one of the two figures survived checking and the
other did not. The commands are
`find corpus -type f -printf '%f\n' | grep -cE '第[0-9]+巻'` and its
`-type d` sibling; `corpus/` is gitignored and local, so a figure quoted
without its method cannot be checked by anyone who has not got this tree.

`話` and `巻` ship on that evidence. `回` and `卷` stay out on the same
evidence — also measured at zero, and adding either is a two-line change the
day a corpus shows one. `卷` also mis-reads as a chapter counter rather than
a volume one if it were ever added here — `第7卷` is volume 7, and the
chapter table is the wrong table for it regardless of what any corpus shows.

`章` ships anyway, on knowledge of the language rather than on measurement —
it scores the identical zero `回` and `卷` were excluded for, and `naming.rs`
says so itself rather than leaving the inconsistency for someone to notice.
Its own stated reason is that `章` can only ever fire on an explicit
`第N章`, so it cannot manufacture a false positive the way a wrong marker
guessed from nothing could — a property `回` shares exactly (`第N回` is
equally explicit) and this note does not claim otherwise. What actually
separates the two is narrower than a technical property: `章` is a
counter someone with knowledge of the language judged worth including
regardless of this corpus, and `回` has not been given that judgment here.
"Measured, and safe to keep despite reading zero" is the wrong test for
either — `章` is a guess and says so; `回` is neither shipped nor
justified, just absent.

### `notify`'s build cost: 262 against 248 CPU-seconds, this workstation only

Already stated in `docs/status.md`'s dependency list, and recorded properly
here per this file's own purpose: a clean `cargo build --workspace
--all-targets --all-features`, 32 cores, cost about 6% more CPU time with
`notify` in the dependency graph than without — 262 CPU-seconds against 248.

**Stopping here is deliberate.** This was not run through CI, and a 32-core
workstation parallelises a build differently from a cacheless runner with a
different core count — the branch already corrected one attempt upstream of
this to read a workstation figure as a statement about the ten-minute CI
budget. The 6% is what was measured; nothing about the budget is inferred
from it.

### What a scan costs here, with the deployment host named as unmeasured

**Re-measured 2026-08-18, against the fixed binary — see the correction
below the table.** Run against `corpus/` (28 GB, 22,902 files on disk) from
this workstation, release build, `taiyaku library scan --add corpus --db
<scratch>`:

| Pass | What changed on disk | Wall clock | CPU |
| --- | --- | ---: | ---: |
| 1st — `--no-derive` | fresh add, walk only | 0.81 s | 0.11 s (14%) |
| 2nd — derive | facts already walked, nothing hashed yet | 21.63 s | 16.31 s (75%) |
| 3rd — derive again | nothing on disk changed, every hash already recorded | 0.06 s | 0.05 s (96%) |

**The percentages come from the unrounded figures, not from the two columns
beside them**, which is why the third row reads 96% where 0.05 ÷ 0.06 is 83%.
Said here because it is otherwise an arithmetic error on the page.

**And the wall clock is the least stable thing in this table.** Re-run on the
same day, the same release binary and the same corpus, with a different page
cache: 0.97 s / 31.13 s / 0.04 s against 0.81 / 21.63 / 0.06 — the second pass
half again as long. What did *not* move by one across the two runs is every
count the pass reports (23 series, 68 works, 68 editions, 110 members, 5
unattached chapter directories, 59 unreadable, 174 suppressed, 42 merged).
So the ratio between the second and third pass is the finding here and the
absolute seconds are a property of one machine on one afternoon; §13's own
sentence about the deployment host applies to this table as much as to the
watcher.

The walk itself — `measure`/`flatten`/`record`, §3's fact layer — found 203
directories and 375 entries in under a second: stat and `read_dir`, no
archive opened except to sniff a format for a new-or-moved entry. Deriving
that same walk into structure — `rules.rs` resolving 375 entries, and
critically, hashing the 110 that became members for the first time, per §3's
"lazy until a member is about to become part of a work" — is what the 22
seconds is. The third row is the same derive a second time with nothing
changed: every member's `sha256` is already sitting on `scan_entry`, so
`derive.rs` opens no file at all, and the roughly 380-fold difference between
the second and third row is the entire argument for computing a hash once and
storing it rather than reopening every archive on every scan. 23 series, 68
works, 68 editions, 110 members, 5 unattached chapter directories, 59
unreadable (sidecar/notes/truth JSON files under `corpus/`, plus every volume
directory holding archives rather than loose images, per
`DeriveReport::unreadable`'s own four-situation doc) and 174 suppressed
(entries an ancestor spoke for, or that deferred to whatever is inside them,
per `DeriveReport::suppressed`) came out of it — and **4 proposals**, not 0:
two `edition-of` (Go-Toubun no Hanayome's "Full Color" and "Full Color
Edition" volumes) and two `attach-chapters` (`gotoubun-v01`, one
chapter-organised directory; `incoming`, four).

**The correction.** This section used to read "0 proposals — this corpus's
naming does not happen to trip `edition-of`/`same-series`/`attach-chapters`",
which was not a fact about this corpus. `library scan` called
`proposals::raise` a second time, over a database `derive` had already
raised every open proposal into — every one of `INSERT OR IGNORE`'s rows
conflicted, so the second call's own count was unconditionally zero
regardless of what the first call had actually opened (whole-branch review,
I1). The reading was an artefact of the instrument, never evidence that this
corpus's naming was clean; correcting the counter and rerunning the same
command over the same corpus is what the four proposals above are. The row
counts moved too — 59 works became 68, 226 suppressed became 174 — which is
Task 11's four Criticals landing between the old run and this one, not a
second artefact; this section is a fresh run against the fixed binary in
full, not a patch over the old numbers.

**What this is not a measurement of, per §13 of the scanner design.** This
workstation's `corpus/` sits on local disk, not the deployment host's
filesystem, and nobody has timed the walk, the sniff or the first hash there.
A NAS is not this machine, and "22 seconds for 110 members on a 32-core
workstation with local NVMe" says nothing defensible about how long the same
derive takes over whatever the deployment host's storage actually is.

## Spec A2's measurements: a cascade the migration could have hidden, argon2's real cost, and the dev proxy that refused every login

Taken on the `library-auth` branch, 2026-08-18, on this workstation unless a
line says otherwise. Spec A2 §14 named three things it could not measure; two of
them are below, the third is still owed and says so.

### The rebuild that would have deleted every note, and why no migration could say so

`user.password_hash` goes NULL → NOT NULL in migration 4, which SQLite cannot do
in place, so it is the documented twelve-step rebuild: create, copy, drop,
rename. `progress` and `note` both `REFERENCES user(id) ON DELETE CASCADE`, and
`DROP TABLE` on a parent performs an implicit `DELETE` that fires those cascades.

The pragma that would prevent it **cannot live in the migration**. `PRAGMA
foreign_keys` is a no-op inside a transaction — it does not error and does not
warn, it simply does not apply — and `schema.rs::apply_pending` runs every
migration inside one. Reproduced with `sqlite3` against a two-table copy of the
real `user`/`note` pair:

| | notes surviving | user rows |
| --- | --- | --- |
| pragma inside the transaction, as a migration would write it | **0** | copied correctly |
| pragma before `BEGIN`, `pragma_foreign_key_check` before `COMMIT` | 1 | copied correctly, id preserved |

The failure is worse than the one §3 anticipated ("getting it wrong silently
drops the rows it copies"): it copies the rows it was asked to copy, perfectly,
and deletes the children nobody mentioned. So `migrate_with` disables foreign
keys around the whole run and restores whatever was set before, and
`apply_pending` runs `pragma_foreign_key_check` inside each transaction before
writing the version row — the enforcement that would have happened continuously,
happening once.

`schema::tests::migration_4_rebuilds_user_without_taking_the_notes_with_it`
fails with `migration 4 cascaded the notes away, left: 0, right: 1` the moment
that wrapper is removed. Verified by removing it, not by reasoning about it.

### argon2id's real cost, and why the test does not assert it

Release profile, mean of ten, at OWASP's `m=19456 KiB, t=2, p=1`:

- **hash 11.92 ms, verify 10.93 ms**

§10 asks for the cost to be "asserted to be within a band, not merely correct —
a parameter set that verifies correctly and takes 2 ms is the defect". A timing
floor is the obvious reading of that sentence and is the wrong test: a floor set
anywhere near 23 ms fails on a *faster* machine, reporting a hardware upgrade as
a security defect. The parameters are what would be lowered by mistake and they
are constants, so they are three `const` assertions beside their own definitions
— a compile error rather than a test failure. A 2 ms clock floor stays for the
failure those cannot see: a `hash_password` that stopped calling argon2 at all.

**Still owed, and §14's first item: the cost on the NAS that will run this.**
This machine is the workstation. A parameter set that is fast here and slow
there is exactly the failure that discipline exists to catch, and nothing below
this line has measured it.

### The one new crate, and what it costs to build

`cargo clean`, then:

| | wall | CPU-seconds |
| --- | --- | --- |
| whole workspace | 12.48 s | **138.85** (125.79 user + 13.06 sys) |
| `argon2` and its five new transitive deps alone | 0.97 s | **1.70** |

`base64ct`, `blake2`, `password-hash` and `rand_core` 0.6.4 are the five.
`base64` moved 0.23.0 → 0.23.1 as a side effect; `rand` 0.10.2 and `getrandom`
were already in the lockfile transitively. So the branch's one genuinely new
crate is about 1.2% of a clean workspace build. **No inference is drawn about
CI's budget**, which is `notify`'s precedent two sections above.

### The dev proxy refused every login, which §14 predicted and said how to fix

§14 recorded the session cookie's scoping through `vite.config.ts`'s
`changeOrigin: true` as unverified. Measured through a real `pnpm dev` on
`localhost:5173` in front of `serve` on `127.0.0.1:8080`:

```
POST /api/session  ->  403 Forbidden
```

`changeOrigin: true` rewrites `Host` to the target while the browser's `Origin`
stays `localhost:5173`, so spec A2 §4's origin check compares `localhost:5173`
against `127.0.0.1:8080` and refuses. Every login through `pnpm dev` failed, and
a reader would have seen the reader's own "the server refused this request's
origin" message with no way to act on it.

§14 said which way to fix that before it happened, and it is worth quoting
because the tempting fix is the other one:

> If it misbehaves, the fix is in the dev proxy and not in the cookie policy — a
> production cookie must not be weakened for a development convenience.

So `changeOrigin` is now `false` and the proxy forwards the `Host` the browser
sent. Nothing needs the rewrite: the target is this project's own `serve`, which
routes on the path and reads `Host` only to compare it against `Origin`.

Re-measured through the proxy afterwards, all five:

| | |
| --- | --- |
| `POST /api/session` | 204, `HttpOnly; SameSite=Lax; Path=/; Max-Age=2592000` |
| `GET /api/works` with the cookie | 200 |
| `GET /api/me` | `{"admin":true,…,"user_id":1}` |
| `PUT /api/works/t/progress`, matching `Origin` | 204 |
| the same write, `Origin: http://evil.test` | 401 |

`Secure` is absent, correctly: no `--trusted-proxy` is configured, so the cookie
must survive a plain-HTTP hop.

### What an OPDS client sees, and the half of it that is still unmeasured

Against `serve` directly, with a key from `taiyaku library key create`:

| | |
| --- | --- |
| `GET /opds/v1.2/catalog`, anonymous | 401 with `WWW-Authenticate: Basic realm="taiyaku"` |
| the same, `-u davey:tyk_…` | 200, the catalog feed |
| `GET /opds/v1.2/works/t/raw/pages/0` | 200, `image/jpeg` |

**The server half works and the client half is still unverified.** §14's second
item is whether *Panels* accepts a `tyk_`-prefixed key in the Basic password
field, and that cannot be answered with `curl`: what is checked above is that a
client sending the key as the password is served, which is what the
specification implies every client does. Whether Panels does it is one attempt
against the real reader on a real iPad, and §14 says it should happen before the
account screen promises it. The account screen currently promises it.

If it turns out not to work, the fallback is stated rather than invented then:
the account screen offers a password-shaped credential per device instead, and
§2's table is amended rather than the prefix removed.

### What the whole-branch review found, 2026-08-19

Fifteen findings against a branch whose gate was green, whose 1280 tests passed,
and which had been exercised against a running server. Ten are fixed here; the
list below is the three worth reading as a class, because each is a different way
for a check to be present and not working.

**A requirement that never shipped.** §7 says the limiter is "on the login route
*and on failed `Authorization: Basic` verification only*" and only the first half
was written — so a password was guessable at any rate through any guarded route
while ten tries at the login route locked the pair out. Reproduced: eleven wrong
passwords over Basic, all 401. Fixed and re-measured: `401×10` then 429, with the
right password still 429 while locked out. It was also an unauthenticated denial
of service, since each guess holds a `spawn_blocking` thread for 11.9 ms and
allocates argon2's 19 MiB buffer.

**A safety net that read the wrong list.** `every_route_refuses_an_anonymous_request`
carried `patterns.len() >= 13` and never moved when six account routes were added
twenty lines below it, so 19 routes shipped against a floor that 13 would satisfy
— and nothing read `open_routes`, the by-name exemption from the guard. Moving
`/api/password` there was verified to pass green before the fix and to fail
after. The floor is derived from both lists now, and the exemption has a test of
its own that proves each entry is outside the guard by a success no guarded route
could give: "not 401" would not do it, because a login refusing wrong credentials
answers 401 with the guard's own body.

**A promise kept by the row and broken by the cookie.** `Max-Age` is absolute
from receipt, so a session cookie written once at login was discarded thirty days
later however often it was used, while `session.expires` slid forward on every
request. Three doc comments — `004_auth.sql`, `SESSION_DAYS`, and §9's cost
argument for refusing a `--no-auth` bypass — were all true of the row and false
of the cookie.

The pattern across all three: each had a check, a comment, or a spec line
asserting the property, and none of them was wrong about what it *wanted*. What
was missing was anything that would fail if the property stopped holding.

### Migration 5, and the second rebuild

`note.id` was a global `TEXT PRIMARY KEY` while notes became per-user, so two
readers could not hold the same id — and since `put_notes` writes whole
documents, the second reader's *entire* notes document was rejected rather than
one note. Reachable by importing a `taiyaku.notes.json` somebody else exported.
The key is `(user_id, work_id, id)` now — `(user_id, id)` was the first answer
and was still one column too wide, since `put_notes` replaces one reader's notes
on one *work*, so the same reader holding that id on another volume hit the same
whole-document 500. Corrected after the second review.

**Nothing REFERENCES `note`**, so unlike migration 4 there was no cascade to
fire — checked with `grep -rn 'REFERENCES note' schema/` rather than assumed.
What a `DROP TABLE` does take regardless is the table's own indexes, silently:
`note_by_work` had to be recreated, and a rebuild that forgot it would leave
every notes read scanning, correct and slower, with nothing able to tell. The
test asserts the index back; removing that one line from the migration fails it.

### The fourth review's two security findings, 2026-08-19

Both reproduced before being fixed, and both are the same shape: a predicate
that was correct about its own question being asked a different one.

**A library its owner had already set up was still claimable by a stranger.**
`account::admin_exists` counts `role = 'admin' AND password_hash <> '!'` and is
right to — a fresh library has an admin row and no admin, which is why the claim
route cannot read the role alone. But `account::unclaimed` asks a different
question, "is the seeded row still locked", and the two answers come apart the
moment the first account is made with `taiyaku library user add alice`, which
inserts with role `user` and leaves row 1 locked. `admin_exists` then answers
false on a library being read from daily, while `unclaimed` still finds row 1 —
so `POST /api/claim` promoted it for whoever asked. §5 requires promotion to
*update* row 1 rather than insert beside it, precisely so pre-A2 notes and
positions follow the account, which means the stranger inherited them along with
administrator.

Reproduced at **204** in
`session::tests::a_library_someone_has_already_set_up_cannot_be_claimed_by_a_stranger`
— fresh library, one `add`, one anonymous claim.

The fix is a third predicate rather than a change to either existing one:
`account::is_claimed`, "can anybody log in at all, of any role". The claim route
and the "nobody can log in" startup warning read it; `admin_exists` keeps the
`--allow-web-setup` notice, which really is asking about administrators. Both
remain because both questions are asked, and reading one for the other is what
made the hole.

The test that covered this asserted the defect as the requirement. Its doc
comment read "so the claim route does not close because an ordinary reader was
added first", which is a description of the behaviour written as though somebody
had wanted it. Corrected in place, with a note saying so.

Closing the hole opened a second, so it was closed in the same change:
`user promote alice --role user` sets the only promotable row to a role that
cannot administer, and then `unclaimed` finds nothing to promote a second time
and `is_claimed` shuts the claim route — a library readable and permanently
unadministrable, with nothing having said so. `promote` now refuses a non-admin
role when no admin exists, naming `user add --role admin` as the order that
works. Adding an admin first and promoting to `user` afterwards is still allowed,
because the rule is the invariant rather than the order.

**Inventing a new name every request bought unlimited argon2.** The lockout key
is `(client address, username)` per §7, deliberately: keyed on the account alone,
one misconfigured OPDS client retrying hard would lock its owner out of every
device. The name half comes from the caller, so a fresh invented name is a fresh
counter — which
`session::tests::guessing_at_names_is_rate_limited_as_well` already *stated*, in
a comment calling it "the trade §7 makes deliberately and worth stating rather
than pretending otherwise".

It was a defensible trade when it was written, because an unknown name returned
in under a millisecond. **Round two then added `credential::DECOY_HASH`** so an
unknown name costs what a real one costs, closing a timing oracle that
enumerated the account roster. That is the right fix for the oracle and it
converted the trade into a denial of service: every request carrying a name
nobody has ever used now takes a `spawn_blocking` thread and 19 MiB of argon2
buffer for ~12 ms, against a counter it need never approach, and leaves a
`login_attempt` row behind — swept only at startup.

One round's fix produced another round's finding, which is the clearest single
instance of why the rate did not fall. It was not visible from either change
alone.

The fix is a second, wider key: `account::address_key`, the address by itself,
at `ADDRESS_LOCKOUT_AFTER = 50` in the same fifteen-minute window — five
accounts' full allowance, and a `const _: () = assert!` keeps it above
`LOCKOUT_AFTER` so the ceiling cannot be lowered into the floor. It is checked
before the decoy verify runs, on both credential paths, and both record failures
against both keys. A success clears the **pair** only: clearing the address too
would let one correct login from a shared NAT reset a guesser's budget, and the
wide counter is a cost ceiling rather than a lockout, so it lapses with its
window instead.

Reproduced through the login route in
`session::tests::inventing_a_new_name_every_time_does_not_buy_unlimited_hashing`,
which drives three real requests and winds the counter forward between them
rather than paying fifty argon2 verifies — the cost being bounded is the reason
not to spend it in a test. Verified against a mutant: with
`is_address_locked_out` stubbed to `Ok(false)` the test fails at 401 against 429.

**What this does not fix is O57.** Both paths check their counter and then do the
work, so requests arriving together all read the same under-ceiling count and all
proceed. `spawn_blocking`'s pool is 512 by default at 19 MiB a verify — roughly
9.5 GiB of transient buffer in one burst, on a host chosen for being small. The
sustained flood stops once the first failures land; the burst does not.
Increment-then-check is not the fix, because an OPDS client presents Basic on
every page request and metering successes would lock a reader out of their own
library. A semaphore capping concurrent verifies is, and the cap is a number
nobody has measured on the host that will run this — which is §14's outstanding
argon2 measurement. Recorded rather than guessed.

**And one adjacent finding fixed while in the same lines:** `change_password`
keyed its limiter on the constant `"  change-password"`, so every reader behind
one address shared one counter — somebody mistyping their old password ten times
locked everyone at that address out of changing theirs, and a guesser working on
one account spent a budget the others needed. It carries the `user_id` now.

### The mutation harness, and the thirty survivors it found on its first run, 2026-08-19

The sweep that closed all five crates in August was a one-off. Nothing re-ran
it: no `mutants.toml`, no script, no CI job, and `check-all.sh` did not know the
word. **The permanent artefact was the fixtures written to kill survivors, not a
harness** — so the four modules spec A2 added were never swept at all, and
nothing would have swept them.

What landed: `mutants.toml`, `scripts/mutants.sh`, `scripts/mutants-baseline.mjs`,
`docs/mutants-baseline.md` and `.forgejo/workflows/mutants.yml`, weekly and
gating nothing.

**The baseline is the part the roadmap entry did not ask for, and it is the part
that makes the rest usable.** A survivor list is only actionable if a new one is
distinguishable from an old one, and 45 survivors are deferred behind L2's
detector redesign. A weekly job reporting those 44 every week is a job nobody
reads by the third week. The baseline records the accepted survivors with their
reasons; the script reports the difference.

It is also an answer to the question the four review rounds raised. Fifteen
findings a round, four rounds, no decay — because nothing remembered what had
already been checked, so every reviewer re-sampled the whole branch from
scratch. Coverage records which lines *ran*. The baseline records which lines
have an assertion behind them and which are known not to, and it is committed,
so it outlives the session.

#### Thirty of 255, in 23 minutes

**Ten were the session lifecycle, and they are one shape.** `close_session` →
`Ok(())`, `sweep_expired_sessions` → `Ok(0)` or `Ok(1)`, `sessions_of` →
`Ok(vec![])`, `revoke_session` → `Ok(true)` *or* `Ok(false)`, plus its `rows > 0`
surviving `==`, `<` and `>=`. Every test in the workspace passed against all ten.

One reason: `account/tests.rs` contained no test naming any of the four
functions, and `serve`'s route tests asserted through `session_owner` — which
answers `None` for a session that was never opened just as readily as for one
properly closed. **A revoke that revoked nothing would have shipped green**, on
the route whose entire purpose is ending a session somebody believes is
compromised. Four direct tests now assert against the functions, with fixtures
built so that the count, the survivors and the casualties are three different
numbers — `Ok(0)` and `Ok(1)` were both survivors, so a fixture where the right
answer is 1 would have killed only one of them.

**Three were fail-closed claims that comments argue for at length and nothing
checked.** The `Ok(Attempt::Failed)` arm could be *deleted*, falling through to
the 401 beside it — and the 401 is the one answer that cannot be true when a
correct password's session write loses to a busy writer, because it makes every
client say "that name and password did not match" and leaves a reader retyping a
password that was right the first time. That was a third-review fix. The other
two are the 429s for a failure the limiter could not record, on both credential
paths. All three are now reachable deterministically by dropping the table the
write needs, which is the same error class the comments reason about rather than
a simulation of it.

**One was `failed()` replaced by `Response::default()` — a 200 with an empty
body** for a password change that did not happen, leaving a reader believing a
password they cannot log in with.

**One was the match guard letting a header credential speak past a cross-site
cookie.** `resolve`'s comment states it; nothing in the suite paired the two
credentials, so replacing the guard with `true` passed. Under that mutant an
OPDS client carrying a stale browser cookie is refused for an origin it never
sent, with its valid `Authorization` header never looked at.

**Four were decisions inside print statements**, which is the shape this
codebase's sweeps keep finding. `ended > 0` inside `println!` carried three
mutants — `>=` prints "ended 0 open session(s)" — and the `X-Forwarded-For`
warning's `&&` carried one. Both extracted into functions returning values, which
is what the August sweep found kills this shape: 11 mutants in `inventory`, 24 in
`topology`.

#### Three of them were written that morning

`note_failure(&conn, &key).is_err() || note_failure(&conn, &wide).is_err()`, in
three places, from the address-key fix earlier the same day. Reading them for
the mutant found a **defect rather than a missing test**: `||` short-circuits, so
a failure recording against the pair key skipped the wide key entirely — the one
counter that bounds the denial of service, dropped on the path that most needs
it. Both calls run unconditionally now and the decision is taken after.

That is the argument for a harness stated as an event rather than as a
principle. The defect was introduced in the morning and found by a tool two
hours later, on a branch where the alternative was a fifth review round that had
already been argued against.

#### What the harness itself got wrong, twice

**`--output DIR` means "put a `mutants.out` inside DIR".** Written as
`--output target/mutants.out`, the results landed in
`target/mutants.out/mutants.out/` and the diff read an absent `missed.txt` as an
empty one — reporting a clean sweep because it could not find the answer. The
script created that file when it was missing; it now exits 1 and says where the
log is. **A harness that says green because it looked in the wrong place is
worse than no harness**, and this one did exactly that before anybody used it.

**`/tmp` is a 31 GB tmpfs on this machine, and cargo-mutants builds there.**
Every mutant's build directory carries its own `target/`; the second five-file
run filled it and died at mutant 140 of 255 with `No space left on device` — the
same error the 28 GB corpus produces, from a different cause, which is why the
script now names both. `TMPDIR` is set to `target/mutants-scratch`, on the disk
with 2.7 TB free, overridable with `TAIYAKU_MUTANTS_TMP`.

#### Two fixtures that could not fail, caught while writing the tests

Both the recurring shape, and both mine that afternoon.

A login test used `fixture`/`fixture-password` where the constants are
`local`/`fixture`, so it got its 401 for the ordinary reason rather than the one
it named. It failed loudly, which is luck: the two tests beside it named an
account that does not exist and passed — through the unknown-name branch rather
than the failed-verify branch they claimed to test. All three now use
`test_support`'s constants.

An assertion written as `contains(a) || contains(b)` passes on either half. It
was checking the `Cidr` refusal's mapped-address sentence, and **the first
version did not kill the mutant it was written for**: `&&` and `||` agree on both
inputs it used — a mapped address satisfies both operands, a plain IPv6 range
neither. The discriminating input is an *ordinary IPv4* range, which contains no
`::ffff:` and has a v4 base, so the two disagree. Verified by hand-mutation
before and after: survives, then dies.

**The re-run is the test of the test.** Reading would not have found either.

### The first sharded run found a mutant the local sweep had recorded as caught, 2026-08-20

`mutants.yml`, sweeping `taiyaku-library` in three shards, reported three
survivors outside the baseline:

```
crates/taiyaku-library/src/scan.rs:793:5: replace mtime_secs -> i64 with 0
crates/taiyaku-library/src/scan.rs:793:5: replace mtime_secs -> i64 with 1
crates/taiyaku-library/src/scan.rs:793:5: replace mtime_secs -> i64 with -1
```

**All three were in the local sweep's `caught` list.** That is the interesting
direction: not a survivor the runner introduced, but a *catch* the workstation
had invented.

Applying the mutation by hand — body replaced with a constant, exactly as
cargo-mutants writes it, with the edit asserted to have landed — every one of
the 347 library tests passes. Nothing catches it. Re-running cargo-mutants
locally reports 3 caught, twice.

The per-mutant log names the difference. Two tests fail under the mutant when
cargo-mutants runs them, and only then:

- `derive::tests::a_volume_whose_files_change_stays_the_same_work_and_keeps_its_progress`
- `derive::tests::a_volume_replaced_by_a_copy_of_another_does_not_fail_the_derive`

cargo-mutants builds in a **copy** of the tree under `TMPDIR`; the hand run is
in place. Same mutation, same tests, different answer. One of the two failures
is not even a detection — it is a *precondition* assertion ("the copy must
actually collide with Alpha's content") giving way.

**This is the third time this pair has caused trouble and the first time it did
so silently.** They are the two O56 is written about, and the two that raced a
one-second clock in CI two days before and were stamped forward with
`File::set_times` to fix it. That fix stopped them failing; it did not make them
independent of the tree they run in. Until now the consequence was a red CI job,
which is loud. This consequence is a mutation score that reads better than the
tests deserve, which is silent — and it is the failure a baseline is least able
to catch, because a mutant recorded as caught never appears in any list to
review.

**A green suite is not evidence a survivor is dead, and a caught mutant is not
evidence a test caught it deliberately.**

The property nothing was testing is small and now has its own test.
`scan::tests::a_files_modification_time_reaches_its_scan_entry` writes a member
directory, **sets** its mtime to a fixed instant rather than waiting for a clock
to move, and asserts the value reaches the scan entry. 2001-09-09T01:46:40Z, far
from 0, 1 and -1 so no constant can coincide with it. Verified to kill all three
mutants under cargo-mutants **and** under hand-application in place, which is
the pair of environments the old catch could not manage.

What is left is O59: any other mutant those two tests happen to fail under is
credited the same way, and a caught count cannot distinguish that from a real
catch.

## The trigram tokeniser against a real shelf: six rows confirmed, the index size that finally moves, and what still is not measured

Task 1 built migration 8 on a measurement §13 of `2026-08-14-library-browse-design.md`
had left open — whether SQLite's default FTS5 tokeniser can find a Japanese
title by a substring of it — and this closes the two things that measurement
itself flagged as unfinished: a `docs/notes.md` entry for it, and a size figure
taken over more than one title. Machine throughout: this workstation, 32 cores,
SQLite 3.53.4 (the system library `rusqlite` links against here, no `bundled`
feature).

### The `fts5vocab` finding, six queries against the shipped tokeniser and its replacement — measured 2026-08-21

`unicode61`, FTS5's default and what every table here shipped with until
migration 8, tokenises on Unicode word/script boundaries and treats a run of
CJK characters as **one token**. Built from `001_initial.sql` and given the
real titles `五等分の花嫁` and `かぐや様は告らせたい～天才たちの恋愛頭脳戦～`,
`fts5vocab` shows the index holds exactly `五等分の花嫁`,
`かぐや様は告らせたい` and `天才たちの恋愛頭脳戦` — whole runs, split only
where the `～` interrupts them. Nothing mid-string can ever match, at any query
length, because the index has no smaller unit to offer.

| query | `unicode61` (shipped) | `unicode61` + `*` | `trigram` |
| --- | --- | --- | --- |
| `五等分の花嫁` whole | ✅ | ✅ | ✅ |
| `五` / `五等` (1–2 chars) | ❌ | ✅ | ❌ |
| `五等分` (prefix, 3) | ❌ | ✅ | ✅ |
| `花嫁` (mid-string, 2) | ❌ | ❌ | ❌ |
| `の花嫁` / `告らせたい` (mid-string, 3+) | ❌ | ❌ | ✅ |
| `Hanay` (Latin prefix) | ❌ | ✅ | ✅ |

`trigram` indexes every three-character window rather than whole tokens, so
any substring of three characters or more is findable in either script — the
only queries it answers with silence are the ones under three characters,
including the first two keystrokes of every search. `unicode61` with a
trailing `*` (a prefix query) recovers the *first* few characters of a run but
never a mid-string substring, which is why `花嫁` stays ❌ in that column.
Trigram plus `browse::search`'s `LIKE` fallback for queries under three
characters (Task 2) is the only column pair in this table with no ❌ anywhere,
which is why migration 8 ships both rather than trigram alone.

### Index size, before and after migration 8: a one-title sample, and a three-work one

**Task 1's own figure, one imported title (`corpus/gotoubun-v01/taiyaku.json`,
no series row).** `dbstat`'s `SUM(pgsize)` over every `*fts*` shadow table read
an identical **32768 bytes** on a version-7 (`unicode61`) database and a
version-8 (`trigram`) database built from the same single work row — SQLite's
per-table minimum allocation (four shadow tables per FTS5 virtual table, one
4096-byte page each) swamping whatever the tokeniser actually did to the
content. `SELECT SUM(length(block)) FROM work_fts_data` showed the real
difference: **69 bytes (`unicode61`) → 216 bytes (`trigram`), a 3.1× growth**
in indexed content for one ~25-character title. `series_fts_data` was
uninformative either way (7 → 9 bytes) because that import writes no series
row. **`dbstat` was the wrong instrument at this scale, not a defect in the
migration.**

**This task's figure, a real multi-work scan.** Built a scratch library by
hardlink from three corpus volumes (`gotoubun-v01`, `gotoubun-v03`,
`kaguya-v01`, 2.1 GB, hardlinked in seconds — nothing in `corpus/` touched,
confirmed with `find corpus -newermt` after) and scanned it with
`library scan --add`: 5 series, 7 works, 7 editions, 9 members derived. On the
resulting (trigram-only; every table here has been at schema 8 since migration
8, so there is no "before" to rebuild without hand-applying migrations 1–7
again as Task 1 did) database:

| table | `dbstat` `SUM(pgsize)` | `SUM(length(block))` |
| --- | ---: | ---: |
| `work_fts_data` | 12288 bytes (3 pages) | 4782 bytes |
| `work_fts_idx` / `_docsize` / `_config` | 4096 bytes each | — |
| `series_fts_data` | 4096 bytes (1 page) | 944 bytes |
| `series_fts_idx` / `_docsize` / `_config` | 4096 bytes each | — |

**`dbstat` finally shows something at this size, but `length(block)` is still
the figure to quote.** Unlike Task 1's one-title sample — where both content
tables sat at the identical one-page floor — `work_fts_data` here has enough
trigram-indexed content (4782 bytes, seven titles including two long romanised
ones) to spill into a third page, while `series_fts_data` (944 bytes, five
short series titles) does not yet cross even the second. So `dbstat` now
*differentiates* the two tables in the right direction — more content, more
pages — but it does so in 4096-byte jumps, telling you nothing about where
inside that range the real number sits; `length(block)` is exact at any scale
and is what this file should keep quoting. The migration-8 ratio itself is
still best represented by Task 1's controlled one-title before/after (3.1×,
same content indexed twice under two tokenisers) rather than this section's
figures, which have no "before" to compare against without redoing that same
hand-applied-migration exercise on three works instead of one — not done here,
since the question this task actually owed `docs/notes.md` was which
*instrument* is informative at library scale, and the answer is `length(block)`
at every scale measured so far, `dbstat` only sometimes.

### The `の花嫁` query time on the scratch library — measured 2026-08-21

```
sqlite3 lib.db ".timer on" "SELECT count(*) FROM series_fts WHERE series_fts MATCH '\"の花嫁\"';"
sqlite3 lib.db ".timer on" "SELECT count(*) FROM work_fts WHERE work_fts MATCH '\"の花嫁\"';"
```

`series_fts`: 0 rows, 0.000556 s real. `work_fts`: 0 rows, 0.000441 s real.
Both count zero — **honestly, not as a defect.** These three corpus volumes'
derived titles (`Go-Toubun no Hanayome - v01 (2018).cbz` and siblings,
`kaguya-v01`) are romanised throughout; nothing in `work.title`,
`series.sort_title` or `series_title.title` in this scratch library carries
any CJK text at all, checked directly by pattern-matching every title against
the Hiragana/Katakana/CJK Unicode blocks. So `の花嫁` legitimately cannot
match anything here — this is a timing over a full negative trigram scan, not
a hit-path timing, and this scratch library answers no positive case for it.
Sanity-checked that `MATCH` itself works correctly on this database first:
`work_fts MATCH '"Hana"'` returns 6, `series_fts MATCH '"Hana"'` returns 3, so
the zero above is the corpus subset's content, not a broken index or a broken
query. Half a millisecond either way, over 7 titles and 5 series titles, is not
a library-scale timing claim — it says the mechanism is cheap at this size and
nothing about whether it stays cheap at thousands of series.

### What was not measured

- **The cold-cover timing §13 of the browse design spec still leaves open.**
  "Whether a 127-cover grid is actually fast enough on the deployment host over
  a real network, with covers generated on demand the first time... the cold
  case is the one nobody has timed" — still true after this task. Nothing here
  touches cover generation, and the scratch library built for Step 1 was never
  served, so no request against it was timed either.
- **Whether anyone has ever held a root grant on a real deployment.** Also
  named open by §13, in the same terms: `user_root` has no rows in any library
  that exists outside a test fixture, here or elsewhere, because nothing wrote
  one before spec C1. The scratch library this task built was scanned and
  queried directly against `lib.db`; no account was created against it and no
  grant was issued, so this remains a prediction rather than an observation.

## O64 and O65: two pre-existing defects a browser pass found, 2026-08-21

Run against `library-browse-series` @ `5ce1122`, in Chrome via the
chrome-devtools MCP, 1440x900, both colour schemes. The full session record —
setup, screenshots, contrast sweep, three other (branch-diff) defects fixed
the same session — is `.superpowers/sdd/srs-browser-pass.md` (its D4 and D5),
session-local like `panels.jsonl` and not part of the committed record; what
follows is what that session measured for these two.

Fixture: `corpus`'s `gotoubun-v01`, `gotoubun-v03`, `gotoubun-v08` and
`kaguya-v01` hardlinked under `target/browser-check/shelf` (`cp -al`,
confirmed afterward with `find corpus -newermt` that no corpus inode's mtime
moved), scanned, given rules by `library rules --import` so one volume
(`v08`) is covered by only one of the two editions the others have, scanned
again, then round-tripped through `library export` / `library import` to
carry over the corpus's own real alignment blocks. Servers were killed by PID
after, ports confirmed free, working tree left clean.

### O64 — `library import` clears `member.root_id`

A/B on one database:

```
after `library scan`                      -> root_id 1 : 33 members
after `library import` (all four works)   -> root_id NULL : 33 members
after `library scan` again                -> root_id 1 : 33 members
after `library import` of work 4 only     -> work 1: root 1 (16)   work 2: root 1 (11)
                                             work 3: root 1 (4)    work 4: NULL (2)
```

Exactly the imported work's members lose their root. `import.rs`'s member
insert (`INSERT INTO member (edition_id, ordinal, path, sha256, pages) …`)
never mentions `root_id`, so a fresh row takes the column's NULL default.

The user-visible consequence, measured over HTTP as a `reader` account
holding `library user grant-root reader $W/shelf` (the admin account still
saw everything):

```
GET /api/library/series          -> 五等分の花嫁  works=2  editions=9   (should be 3 / 11)
GET /api/library/series/2        -> volumes: [v01, v03]                 (v08 gone)
GET /api/library/works/…v08…     -> 404
```

`grant-root` already detects and reports the condition when granting —
"Note: 2 member(s) carry no root yet and are invisible to any restricted
account. `taiyaku library scan` re-derives them." — but that warning fires
only on the grant path. Grant-then-import, the more ordinary order, prints
nothing and the work vanishes in silence.

### O65 — the reader chrome's `Library` button covers two layout buttons

Reproduced by opening any volume in the reader and pressing `Tab` to show the
chrome. `Chrome.tsx`'s `leaveButtonStyle` (`position: absolute; z-index: 3`,
`top`/`left` at the module's `inset` constant, 16) sits over the layout-mode
strip (8,8, no `z-index`, normal flow). `document.elementFromPoint` at each
mode button's own centre:

| button | rect | element actually on top at its centre | reachable |
| --- | --- | --- | --- |
| `single` | x 8-58.8, y 8-29 | `BUTTON:Library` | no |
| `two` | x 66.8-103.6, y 8-29 | `BUTTON:Library` | no |
| `scroll` | x 111.6-158.7, y 8-29 | `BUTTON:scroll` | yes |

With the chrome open — the state a reader is in exactly when reaching for the
layout switch — clicking where `single` or `two` appear to be leaves the
reader instead.

## Four measurements for spec C2's download, 2026-08-23

Taken while designing `docs/superpowers/specs/2026-08-23-library-download-design.md`,
against the same `corpus/` this repository already measures against elsewhere.
`cbz.rs`'s module doc names the first of these; §1 of the spec corrects §13.1
of the older browse design on the strength of it.

### The entry compression split: producing a `STORE` download inflates 97.9% of what it draws from

§13.1 called generation "a copy". It is not: a generated CBZ writes every
page as `STORE`, so any page whose source archive used `DEFLATE` comes out
larger than it went in. Measured over every `.cbz` under `corpus/` (140
archives, `incoming/` included):

```
python3 - <<'PY'
import zipfile
from pathlib import Path
total = deflate = store = dominant_stored = archives = 0
for cbz in sorted(Path('corpus').rglob('*.cbz')):
    try:
        z = zipfile.ZipFile(cbz)
    except Exception:
        continue
    archives += 1
    a_deflate = a_store = 0
    for info in z.infolist():
        if info.is_dir():
            continue
        total += 1
        if info.compress_type == zipfile.ZIP_DEFLATED:
            deflate += 1; a_deflate += 1
        elif info.compress_type == zipfile.ZIP_STORED:
            store += 1; a_store += 1
    if a_store > a_deflate:
        dominant_stored += 1
print(archives, "archives,", total, "entries")
print("DEFLATE:", deflate, "STORE:", store)
print("archives dominantly stored:", dominant_stored)
PY
```

```
140 archives, 11681 entries
DEFLATE: 11431
STORE:   250
archives dominantly stored (STORE entries > DEFLATE entries): 4
```

11,431 of 11,681 entries (97.9%) are `DEFLATE`; 250 are `STORE`; every entry
sniffed as one or the other, none reported anything else. 4 of the 140
archives are dominantly stored themselves, so a generated CBZ is occasionally
close to a straight copy — just not because generation *is* one.

### How often passthrough fires: 31 of 47 directories hold exactly one page-bearing archive

Spec C2 §3's single-file passthrough only has somewhere to apply when a
directory holds exactly one archive with pages in it; a directory holding
more (a chaptered fan release, a set of alternates) always generates. Same
corpus walk, grouped by parent directory and counting only archives with at
least one image entry:

```
python3 - <<'PY'
import zipfile
from pathlib import Path
from collections import defaultdict
IMG = {"jpg","jpeg","png","webp","gif","bmp","avif","tif","tiff"}
def is_page(name):
    if name.endswith('/'):
        return False
    base = name.rsplit('/', 1)[-1]
    if base.startswith('.') or name.startswith('__MACOSX'):
        return False
    ext = base.rsplit('.', 1)[-1].lower() if '.' in base else ''
    return ext in IMG
by_dir = defaultdict(list)
for cbz in sorted(Path('corpus').rglob('*.cbz')):
    try:
        z = zipfile.ZipFile(cbz)
    except Exception:
        continue
    pages = sum(1 for info in z.infolist() if is_page(info.filename))
    if pages > 0:
        by_dir[cbz.parent].append(cbz.name)
dirs = list(by_dir.items())
one = sum(1 for _, a in dirs if len(a) == 1)
print(len(dirs), "directories;", one, "hold exactly one",
      f"({one/len(dirs)*100:.0f}%)")
PY
```

```
47 directories; 31 hold exactly one (66%)
```

### What zip64 is not exercised by: no group in the corpus reaches 4 GiB

A download's `Plan` takes the zip64 branch once a *group's* total — every
member a generated edition would draw from, summed — exceeds `U32_MAX`
bytes. Same walk, this time by total bytes rather than page count:

```
python3 - <<'PY'
import zipfile
from pathlib import Path
from collections import defaultdict
IMG = {"jpg","jpeg","png","webp","gif","bmp","avif","tif","tiff"}
def is_page(name):
    if name.endswith('/'):
        return False
    base = name.rsplit('/', 1)[-1]
    if base.startswith('.') or name.startswith('__MACOSX'):
        return False
    ext = base.rsplit('.', 1)[-1].lower() if '.' in base else ''
    return ext in IMG
by_dir = defaultdict(list)
largest = (0, None)
for cbz in sorted(Path('corpus').rglob('*.cbz')):
    try:
        z = zipfile.ZipFile(cbz)
    except Exception:
        continue
    if sum(1 for info in z.infolist() if is_page(info.filename)) == 0:
        continue
    size = cbz.stat().st_size
    by_dir[cbz.parent].append(size)
    if size > largest[0]:
        largest = (size, cbz)
GIB = 1024**3
print("largest single archive:", largest[1], largest[0], "bytes")
group_sizes = {d: sum(s) for d, s in by_dir.items()}
biggest_group = max(group_sizes.items(), key=lambda kv: kv[1])
print("largest summed group:", biggest_group[0], biggest_group[1], "bytes")
over = sum(1 for s in group_sizes.values() if s > 4 * GIB)
print(f"{over} of {len(group_sizes)} groups exceed 4 GiB")
PY
```

```
largest single archive: corpus/incoming/nausicaa/en-deluxe-v01.cbz 1491678844 bytes
largest summed group: corpus/incoming/nausicaa 1491678844 bytes
0 of 47 groups exceed 4 GiB
```

1.49 GB (1,491,678,844 bytes), and the largest *group* is the same number,
because that directory holds only the one archive. **This is also the corpus's
one outlier with no `taiyaku.json`** — Task 10 found `load_works` cannot reach
it, so no download route can currently serve it either; the figure describes
the corpus, not what any check exercises. Even counting it, 0 of the 47
page-bearing groups pass 4 GiB, so zip64 is arithmetic this branch's tests
exercise (`cbz/tests.rs`'s `oversized()` fixture, declared rather than real)
and real material does not.

### Directory-backed members: 121 candidates, 14 survive, ~9% of what a download could ever pass through

A member need not be an archive: `taiyaku-read::format::sniff` calls any
directory `Format::Dir` outright, with no distinction drawn by what is
inside it. What decides whether one *particular* directory is the member,
rather than double-counting a subdirectory's pages under both it and its
parent, is nesting — `derive.rs::nested_in_claim`'s rule, "a directory is a
member only if nothing above it is."

**Tried first, and worth recording so it is not re-tried:** running the real
`scan::scan_root` + `derive::derive` over `corpus/`, as
`scanning_and_deriving_the_corpus_produces_real_structure` already does for
the settled `<series>-vNN/` directories, admits zero directory members
anywhere in this corpus — not because none exist, but because `admit()`
requires the rule engine to resolve a series from the path, and the loose
material under `corpus/incoming/` (`berserk-cleaned/Berserk.T01/`,
`colorwalk-pages/`, …) matches none of the seeded naming rules a real library
scan needs. The count below is therefore computed directly against the
filesystem, applying `nested_in_claim`'s same ancestor-prefix rule by hand
rather than through the rule engine that would refuse to admit this material
at all:

```
python3 - <<'PY'
from pathlib import Path
IMG = {"jpg","jpeg","png","webp","gif","bmp","avif","tif","tiff"}
def has_direct_images(d):
    for child in d.iterdir():
        if child.is_file() and not child.name.startswith('.'):
            ext = child.name.rsplit('.', 1)[-1].lower() if '.' in child.name else ''
            if ext in IMG:
                return True
    return False
page_bearing = [d for d in sorted(Path('corpus').rglob('*'))
                if d.is_dir() and '.taiyaku-cache' not in d.parts
                and has_direct_images(d)]
pb = set(page_bearing)
survivors = [d for d in page_bearing if not any(a in pb for a in d.parents)]
print(len(page_bearing), "hold pages directly;", len(survivors), "survive;",
      len(page_bearing) - len(survivors), "suppressed as nested")
PY
```

```
121 hold pages directly; 14 survive; 107 suppressed as nested
```

121 directories hold page images as direct children. 107 of those sit inside
another page-bearing directory's own tree — a chapter folder inside a volume
folder, mostly, the same shape §7 of the scanner's design names — and are
correctly excluded rather than double-counted; 14 are the deepest page-bearing
directory in their own chain and are what a library would actually treat as a
member.

Divided against the same corpus's page-bearing archives (140, all of them —
the entry-compression measurement above already counted every `.cbz` and
found none with zero page entries), a real download's set of candidate members
is 154: 140 archives plus these 14 directories. **14 of 154 is 9.1%, the ~9%
this branch's design cites** — and the reason it matters here rather than
being trivia: `download.rs::prepare`'s single-file passthrough branch requires
`std::path::Path::is_file()`, so every one of these 14 always generates a
`STORE` zip from its loose pages rather than passing anything through,
whatever else is true of the directory that holds it.

### Task 10's two corpus download checks, and the one that now exceeds `.config/nextest.toml`'s stated ceiling

`taiyaku-serve/src/corpus.rs` gained two standing checks for spec C2:
`a_real_multi_member_edition_downloads_as_a_readable_cbz` and
`a_real_single_member_edition_is_the_file_on_disk`. Both carry their own
wall-clock measurement as a doc comment, dated 2026-08-24, from three runs of:

```sh
TAIYAKU_CORPUS="$PWD/corpus" cargo test -p taiyaku-serve --lib \
  corpus::a_real_multi_member_edition_downloads_as_a_readable_cbz \
  -- --test-threads=1
TAIYAKU_CORPUS="$PWD/corpus" cargo test -p taiyaku-serve --lib \
  corpus::a_real_single_member_edition_is_the_file_on_disk \
  -- --test-threads=1
```

Against `gotoubun-v01/en-fan` (7 members, 235 pages, `DEFLATE`), the
multi-member check ran **1.02–1.03 s** each of three runs — reproduced here at
1.06 s. Against `gotoubun-v01/en-mono` (one 334,752,036-byte archive), the
single-member check ran **0.19 s** each — reproduced here at 0.30 s, the
difference plausibly this session's colder cache, since the doc comment
itself calls 0.19 s "faster than expected for a third-of-a-gigabyte file read
twice" and credits repeated corpus runs' filesystem cache.

**The multi-member figure is the one fact this branch's own tests add that
`.config/nextest.toml`'s justification for its 30 s slow mark no longer
states correctly.** That file's comment reads "nothing in 639 tests exceeds
0.75 s"; the workspace is now 1,513 tests and this one check runs about a
third of a second past that ceiling, on real material a corpus-less CI never
sees. See this task's edit to that file for how the sentence was corrected —
qualified to name corpus checks as a separate, un-timed-here category, rather
than re-measured line by line, since the corpus job is a distinct job from
the nextest suite the 0.75 s claim was ever about.

## The doc pass's own re-measurements, 2026-08-24

Findings 1 and 6 of the branch's documentation review asked for figures to be
recounted rather than trusted, since this branch had already drifted the same
`docs/status.md` paragraph twice. Both recounts below were run with
`TAIYAKU_CORPUS` unset (confirmed with `echo "${TAIYAKU_CORPUS:-<unset>}"`
first), matching what `check-all.sh`'s `test` job and CI both run.

### `routes::grant_postures`: the table itself, counted, not grepped

`grep -c` over the enum definition and its doc comments would double-count —
both name every variant. Counted instead from the `vec![` literal in
`routes::grant_postures` (`crates/taiyaku-serve/src/routes.rs`), by extracting
the function body with `awk` and excluding comment lines before counting each
posture's closing token:

```sh
awk '/pub\(crate\) fn grant_postures/,/^}/' crates/taiyaku-serve/src/routes.rs \
  | grep -vE '^\s*//' \
  | grep -c 'Refuses,\?$\|Refuses)'      # 7
  | grep -c 'Filters)'                    # 11
  | grep -c 'GrantIndependent {'          # 9
  | grep -c 'KnownGap {'                  # 1
```

**7 `Refuses`, 11 `Filters`, 9 `GrantIndependent`, 1 `KnownGap` — 28 total,
matching `docs/status.md`'s "28 guarded routes".** Cross-checked against
`auth::tests::every_route_declares_its_grant_posture` (`crates/taiyaku-serve/src/auth/tests.rs`):
every `Refuses` entry is driven by the generic GET loop except
`/api/works/{work}/alignments` (`PATCH`-only, driven in its own block with a
body and `If-Match`), every `Filters` entry is inserted into `proven_filters`
by the end of the test (the `/opds/v1.2/works` split-fixture edition-count is
proven separately in `opds::tests`, but the route's whole-work case is still
driven in this test's first loop), and `GrantIndependent`/`KnownGap` entries
are declared only. So **18 driven (7 + 11), 10 declared (9 + 1)** — which is
exactly what `docs/status.md`'s paragraph already stated before this pass
touched it.

**This figure was already correct on the tree.** `git show cf937f8 --
docs/status.md` shows the parenthetical was corrected from "(6 and 12)" to
"(7 and 11)" in the same commit that moved `patch_alignments` from `Filters`
to `Refuses` — so the branch had already caught its own third drift before
this doc pass started. What changed here instead: the sentence was reworded
to drop the exact `Refuses`/`Filters` and `GrantIndependent`/`KnownGap`
sub-splits, keeping only "18 of the 28" and "the remaining 10" — both stable
across a route moving *within* the driven or declared set (exactly the kind
of move that broke the parenthetical twice), and both still meaningful
because they only change if a route crosses from driven to declared or back,
which is the substantive event worth a human re-reading the sentence for.

### Six crates, 1,524 tests — not 1,513

```sh
env -u TAIYAKU_CORPUS cargo llvm-cov nextest --workspace --all-features
node scripts/status-test-count.mjs target/nextest/default/junit.xml docs/status.md
```

Run twice, because this doc pass's own Finding 4 added a test after the
first measurement — the second run is the one the committed figures match.

**First run**, before any Rust edit: `target/nextest/default/junit.xml`'s
`<testsuites>` root reported `tests="1523" failures="0" errors="0"`.
`scripts/status-test-count.mjs` (the mechanical gate `check-all.sh` runs)
confirmed both live claims — "Six crates, N tests" at `docs/status.md:120`
and "N of the N are standing checks" at `:178` — were both still saying
1513, and both moved to 1523. Both occurrences are the *same* number by
construction (the standing-check sentence's `N` in "Nineteen of the N" is
the workspace total, not a second derived figure), so a single fix moved
both.

**Second run**, after Finding 4 added
`comicinfo::tests::noncharacters_u_fffe_and_u_ffff_are_dropped_too`:
`tests="1524"`. Both `docs/status.md` occurrences and `.config/nextest.toml`'s
two "1,523 tests" mentions (the "as of the library-download branch" sentence
and the one beside it) moved again, to 1524, so the gate and this file agree
with the tree Finding 3 and Finding 4 actually left.

`env -u TAIYAKU_CORPUS cargo test -p taiyaku-serve` separately confirms:
**371 passed; 0 failed** — the given 370-test baseline plus the one test
Finding 4 added; Finding 3 changed no test count, only where two `#[expect]`
attributes live.

The "Nineteen" standing-check figure itself needed no correction. Verified by
hand against the same methodology this file already used for the
"thirteen" era of this count (grep `#[test]`/`#[tokio::test]` per file, then
exclude anything that does not actually gate on `TAIYAKU_CORPUS`):
`crates/taiyaku-cli/src/corpus.rs` — 10 (excludes its `mod tests;` submodule,
3 helpers that only assert path validation); `crates/taiyaku-serve/src/corpus.rs`
— 5 (grew from 3 with Task 10's two download checks); `crates/taiyaku-cli/src/score_corpus.rs`
— 2 of its 4 `#[test]`s (the other two are marked "these need no
`TAIYAKU_CORPUS`, deliberately"); `crates/taiyaku-library/src/corpus.rs` — 2.
10 + 5 + 2 + 2 = 19, matching `docs/status.md`'s existing text exactly.

### `.config/nextest.toml`'s own count went stale the same way, and was not named in the review brief

While re-measuring the above, `.config/nextest.toml`'s comment above
`[profile.default]` claimed `test(corpus::)` selects "twenty-two" tests
(measured 2026-08-18) with "fifteen standing checks ... three in
taiyaku-serve" — both stale by the same Task 10 change that took
`taiyaku-serve/src/corpus.rs` from 3 standing checks to 5. Recounted:

```sh
env -u TAIYAKU_CORPUS cargo nextest list -E 'test(corpus::)' \
  | grep -v "Compiling\|Finished" | wc -l
```

**24**, not 22 (17 standing checks — ten `taiyaku-cli`, five `taiyaku-serve`,
two `taiyaku-library` — plus 3 `corpus::tests::` helpers plus 4
`score_corpus::` tests caught by substring; 17 + 3 + 4 = 24). Fixed in the
same file, alongside its two "1,513" occurrences (one inside the "workspace is
1,513 tests as of the library-download branch" sentence, which is a live
claim about "the library-download branch" rather than a dated snapshot like
the "639" figure two paragraphs above it, so it was corrected rather than
left as historical narrative) — and its own running tally of how many times
this exact comment has been wrong was extended from three to four, following
the pattern the three previous corrections already established there.

Not named in the review brief; fixed anyway because it is the same class of
drift, in the same file family, caught while the corpus test population was
already being counted by hand for the paragraph above.

## The first real OPDS client, 2026-08-25 — spec C2's second half

Everything here comes from one session against a real iPad, driven because
spec C2's stated purpose is offline reading in Panels and nothing had ever
checked that a device could reach a download at all.

### O55 closes: Panels accepts a `tyk_` key in the Basic password field

The library is a scratch one — `target/o64-check/library.db`, three works and
eleven editions hardlinked from `corpus/`, built for O64's browser pass and
reused rather than rescanned.

```sh
./target/release/taiyaku library key create local --name ipad-o55 --db target/o64-check/library.db
./target/release/taiyaku serve --db target/o64-check/library.db --addr 0.0.0.0:8080 \
  --log-requests "$PWD/panels-o55.jsonl"
```

What the log recorded, filtered to the client rather than the shell:

| status | path | user-agent |
| --- | --- | --- |
| 401 | `/opds/v1.2/catalog` | `Panels/950 CFNetwork/3860.700.1 Darwin/25.6.0` |
| 200 | `/opds/v1.2/catalog` | same |
| 200 | `/opds/v1.2/works` | same |
| 200 | `/opds/v1.2/works/{work}` | same |
| 200 | five `…/cover` fetches, then `…/pages/{n}` | same |

The 401-then-200 pair is the whole finding: Panels was handed
`WWW-Authenticate`, prompted, sent the `tyk_` key as the Basic password, and
was served. The `tyk_` design needs no fallback.

**Read the `user-agent` before believing any of this.** An earlier attempt at
the same URL produced an identical-looking 401/200 pair from
`Mozilla/5.0 (X11; Linux x86_64) … Firefox/155.0` — a desktop browser, which
proves only the server half that `curl` had already proved. The two runs are
indistinguishable by status code alone.

### Two things the attempt needed that nothing had recorded

**iOS App Transport Security refuses plain HTTP.** Panels answered
`http://192.168.1.21:8080/…` with an ATS policy error, not a credential
prompt — so no key, correct or otherwise, could have worked. The session
reached the server through `tailscale serve --bg --https=443
http://127.0.0.1:8080`, tailnet-only, which supplies a real certificate and
therefore needs nothing trusted by hand on the device. The account screen's
instructions do not mention any of this.

**The LAN address was unreachable in the first place**, independently of ATS:
`nft` and `iptables` rulesets are both empty on this workstation and the
server binds `0.0.0.0`, so the iPad simply was not on `192.168.1.0/24` at the
time. Worth separating from the ATS finding, because the two produce
different messages and only one of them is about the server.

### What the client rendered, which is O67

The feed is faithful and the data is wrong. `opds::work_feed` renders
`{title} — {label}`, and for the five editions of volume 1 that is:

```
Go-Toubun no Hanayome - Full Color Edition - v01 (2021).cbz — en-color
Go-Toubun no Hanayome - Full Color Edition - v01 (2021).cbz — en-fan
Go-Toubun no Hanayome - Full Color Edition - v01 (2021).cbz — en-mono
Go-Toubun no Hanayome - Full Color Edition - v01 (2021).cbz — raw-color
Go-Toubun no Hanayome - Full Color Edition - v01 (2021).cbz — raw-mono
```

The rows behind it:

```
work.id  work.slug                                                     work.title                                                   volume  series_id  hub
1        go-toubun-no-hanayome---full-color-edition---v01--2021--cbz   Go-Toubun no Hanayome - Full Color Edition - v01 (2021).cbz   1.0     1          en-color
```

One work, five editions, which is `derive.rs`'s `Group` behaving exactly as
designed. The title is the stem of whichever entry minted the row. The labels
are all correct; only the title is not, and the slug derives from it.

### The acquisition link, against real material

Verified after Task 3, on the same library, over the `/opds` namespace:

```sh
curl -s -u 'local:…' "http://127.0.0.1:8080/opds/v1.2/works/$W" | grep -A1 acquisition
curl -s -u 'local:…' -o /dev/null -w '%{http_code} %{content_type} %{size_download}\n' \
  "http://127.0.0.1:8080/opds/v1.2/works/$W/en-mono/download"
```

Five acquisition links, each `type="application/vnd.comicbook+zip"` and each
ending `/download`; the one fetched answered `200`,
`application/vnd.comicbook+zip`, **334,752,036 bytes**. That is the largest
download this project has served end to end, and it is a **passthrough** one:
the edition has a single `.cbz` member, so the bytes are the member's own
rather than a rebuilt archive. *(An earlier draft of this paragraph called it
generated, which contradicts the clause after it — the two branches are
exclusive and a lone `.cbz` member takes the second.)*

### Test counts, and a stale pair corrected

Workspace **1529 → 1536** over this branch's five code commits: two for the
OPDS route, three for `advertised_type`, and a net two for the acquisition
link (one existing test replaced by three). Counted with
`cargo nextest list --workspace --all-features`.

`web/`'s figures in `status.md` were **already stale before this branch**, and
by more than it changed. Measured on `main` at `3595b02`: 1,030 in the `node`
project and 497 in `jsdom`, against the 1,024 and 488 recorded — 1,527 rather
than 1,512. This branch adds seven `jsdom` tests, so the corrected pair is
1,030 and 504. Both projects were run on `main` and again on the branch rather
than one figure being derived from the other.

### O66's trigger, checked once and not fired — 2026-08-25

O66 names its own reopening condition: "a fresh `taiyaku serve --log-requests`
session against Panels, bound to the LAN, showing `Range` on an acquisition
fetch". This branch is the first build where an acquisition fetch exists at
all, so this is the first time that trigger *could* fire.

Panels took five downloads, one per edition of volume 1:

| status | path | `range` header |
| --- | --- | --- |
| 200 | `…/en-color/download` | (none) |
| 200 | `…/en-fan/download` | (none) |
| 200 | `…/en-mono/download` | (none) |
| 200 | `…/raw-color/download` | (none) |
| 200 | `…/raw-mono/download` | (none) |

All `200`, none `206`, and no `Range` on any. **O66 stays open exactly as
written**, and nothing about the generated seek is built on this branch — which
also removes the reason this branch might have had to carry a behaviour change
into a squash commit alongside a feature.

**Three qualifications, because a negative result is only as good as its
instrument.**

*The session ran over a TLS reverse proxy, not the LAN the item asks for.* iOS
ATS refuses plain HTTP (above), so the fetches reached the server through
`tailscale serve` and arrive from `127.0.0.1`. A proxy cannot invent the
absence of a header, so the result stands — but it is one step removed from
the direct connection O66 names, and a proxy that buffered a ranged request
into an unranged origin fetch would look exactly like this. A direct-LAN repeat
would settle it outright and has not been run.

*The log cannot confirm the downloads completed.* Every one recorded
`bytes: 0`, because `routes.rs`'s middleware takes its count from
`response.body().size_hint().exact()` and a streamed body has no exact hint —
the same field reads 815 for the catalog, which is buffered. So the log shows
five downloads *started*. That they finished is the client's report, not the
server's record. **This is a blind spot in the instrument this project uses to
settle open items**, and it is the same shape as the one O66's own entry
complains about: the older `panels*.jsonl` logs report `(not recorded)` for
headers, and their silence was an instrument's limit rather than evidence.
**Tabled as O68**, because a gap recorded only here is exactly what the
open-items table exists to prevent — and because closing it is a decision
rather than a patch: counting a streamed body means writing the log line when
the body completes rather than when the response is produced, which changes
when every line is emitted and what `micros` measures for every route.

*A resumed download is the case O66 is about, and nothing here interrupted
one.* Panels was never made to drop a connection mid-fetch and retry, which is
the circumstance a `Range` would most plausibly appear in. The five fetches
above are five clean first attempts.

## What the proposals inbox costs, and what it looks like against real pages, 2026-08-26

Spec C3's first PR — the proposals inbox. Three measurements the build design
listed as not taken, and one look at the screen with real manga in it.

### The `/resolution` dry run's cost

Build design §11 listed "what the `resolution` dry run costs on a real
library" under what could not be measured. It has been.

Release build, synthetic `scan_entry` rows, one machine:

| rows | `POST …/{id}/resolution` | `GET …/{id}` | `GET …/proposals` |
| --- | --- | --- | --- |
| 8 | 0.28 ms | 0.32 ms | 0.10 ms |
| 5,008 | 5.9 ms | 6.6 ms | 0.15 ms |
| 25,008 | 27.4 ms | 30.5 ms | 0.23 ms |

The two sweeping routes cost about 1.1–1.2 µs per row and the list route is
flat, which is the shape the design predicted: the list reads `proposal` and
the other two walk every entry under the root.

**Two things this is not.** The rows are synthetic rather than a real
library's, and a library holding many `regex` rules compiles more per request
than this one did — so the figures are a floor rather than a range. Neither
qualification is a reason to distrust the shape; both are reasons not to quote
the milliseconds as what any particular library will cost.

> **Corrected 2026-08-26, by re-measurement — see "What a swept row actually
> costs" below.** *"About 1.1–1.2 µs per row"* is not a property of the route.
> It is a property of the rows this harness happened to generate, whose
> `rel_path`s were the length a real library's are. **The cost tracks the
> bytes of `rel_path` swept, not the row count**, and at a fixed 250,005 rows
> it moves by 2.4× with path length alone. This table's per-row figure is
> therefore quotable only alongside the paths it swept, which the paragraph
> above does not state — and that omission is what put it two-fold at odds
> with the `spawn_blocking` figures immediately below, which measure the same
> quantity over shorter synthetic names. The three columns and the shape of
> the curve are unaffected; the µs-per-row constant is the part that was
> being read as more general than it is.

### The `spawn_blocking` result

Measured with a canary task recording how long the runtime could poll nothing
else — the same instrument O8's failure class wants, since a handler's own
duration is unchanged by moving it off the reactor and cannot show this.

At 250,000 rows: `/resolution` stalled the runtime **138.45 ms** before and
**0.02 ms** after; `GET …/{id}` **167.86 ms** → **0.01 ms**. Handler duration
was unchanged in both, which is the point — what moved is what else could run.
An independent reproduction on a second harness got roughly 20% higher
absolute numbers and the same shape.

> **Qualified 2026-08-26, by the same re-measurement.** These figures are
> *this* harness's synthetic rows, whose `rel_path`s were short tokens rather
> than library-shaped paths — which is why they sit at roughly half the
> per-row cost of the table above, and why the pair quoted from them into
> `curation/tests.rs` read as a contradiction of it. **The equality the
> paragraph asserts — stall ≈ handler duration — is corroborated** and is the
> part that mattered: at 250,005 short-path rows the handlers themselves take
> 115.31 ms and 143.70 ms on this tree, which brackets 138.45 and 167.86 at
> the ±20% this section already allows itself. What is not corroborated is
> "at 250,000 rows" as a statement about row count.

### What a swept row actually costs, and why two records of it disagreed

**The complaint.** Two figures for one quantity were shipped on this branch,
about 2× apart, and nothing said they measured different things:

- `crates/taiyaku-serve/src/curation/tests.rs` — *"the runtime was stalled for
  exactly as long as the handler ran — 14.4 ms and 17.5 ms at 25,000 rows,
  138 ms and 168 ms at 250,000"*.
- the table two sections above — 27.4 ms and 30.5 ms at 25,008 rows.

The 250,000 pair agrees exactly with the `spawn_blocking` section's own
138.45/167.86, which rules out a units slip and says both came from one
instrument family. Settled below by re-measuring rather than by reasoning
about which looked more plausible.

**The instrument.** An `#[ignore]`d harness in `curation/tests.rs`, run as
`cargo test --release -p taiyaku-serve -- --ignored --nocapture`, built on the
existing `test_support::router_over_two_suffixed_series()` fixture and calling
the handlers directly rather than through the router — the same reason
`no_curation_route_stalls_the_runtime` gives for doing so. It is recorded here
in full rather than kept in the tree, because *what it generated* turns out to
be the whole finding and a harness whose row shape is not written down is how
this contradiction happened in the first place:

```rust
// N rows under the fixture's one root, in one transaction.
let mut stmt = conn.prepare(
    "INSERT INTO scan_entry (root_id, rel_path, kind, format, size, mtime, pages)
     VALUES (?1, ?2, 'file', 'zip', 1024, 0, 12)")?;
for i in 0..n {
    // (a) library-shaped, ~55 characters, two separators:
    //     format!("{under}/Synthetic {:06}/Synthetic {:06} - v{:02}.cbz", i/20, i/20, i%20)
    // (b) short, one separator:  format!("{under}/{i}")
    // (c) fixed width w, one separator:  format!("{under}/{i:0w$}")
    stmt.execute(params![root_id, rel])?;   // asserts 1, so a duplicate cannot
}                                           // quietly make the row count a lie
// then, best of five per row count:
let t = Instant::now();
curation::resolution(State(state.clone()), Extension(admin), AxPath(id),
                     Json(curation::rule_json(&rule))).await;
```

`under` decides whether the rows *match* the candidate rule the fixture's own
proposal offers (`prefix`, `Go-Toubun no Hanayome - Full Color Edition/`), and
the harness prints `entries_matched` back out of the response so that is a
measurement rather than an assumption.

**Result, this tree, 2026-08-26, release, best of five.** Row counts include
the fixture's own five entries.

| rows | path shape | matched | `POST …/resolution` | `GET …/{id}` | `GET …/proposals` |
| --- | --- | --- | --- | --- | --- |
| 25,005 | library-shaped | 1 | 31.45 ms | 33.59 ms | 0.06 ms |
| 250,005 | library-shaped | 1 | 302.68 ms | 325.38 ms | 0.07 ms |
| 25,005 | short | 1 | 10.36 ms | 13.77 ms | 0.05 ms |
| 250,005 | short | 1 | 115.31 ms | 143.70 ms | 0.06 ms |
| 25,005 | library-shaped | 25,001 | 77.62 ms | 34.43 ms | 0.06 ms |
| 250,005 | library-shaped | 250,001 | 861.64 ms | 337.86 ms | 0.07 ms |

**Both records reproduce, on different rows.** The table above this section
lands on the library-shaped, unmatched rows (27.4/30.5 against 31.45/33.59);
`curation/tests.rs`'s pair lands on the short ones (138/168 against
115.31/143.70, and 14.4/17.5 against 10.36/13.77) — each inside the ±20%
spread the `spawn_blocking` section already records between two harnesses. The
*ordering* corroborates it independently: both records have `GET …/{id}`
slower than `/resolution`, which only holds when the swept entries do not
match the candidate. Where they do match, `/resolution` resolves each entry a
second time and overtakes it by 2.5×, which neither record shows.

**The controlled variable, because "different rows" is not yet a cause.** Row
count held at 250,005, separator depth held at one, `entries_matched` held at
one, and nothing varied but the number of characters in each `rel_path`:

| `rel_path` width | `POST …/resolution` | `GET …/{id}` |
| --- | --- | --- |
| 8 | 118.96 ms | 145.01 ms |
| 24 | 163.96 ms | 190.26 ms |
| 48 | 206.98 ms | 222.92 ms |
| 96 | 289.13 ms | 325.88 ms |

**So the cost is per byte of path swept, not per row** — 2.4× across that
range, from path length alone, with every other variable pinned. Roughly
0.42 µs of fixed per-row cost plus about 7.6 ns per character, on this
machine. A "µs per row" constant is therefore not a property of the route at
all, and both records quoted one as though it were.

**What this changes.**

- `curation/tests.rs`'s doc keeps the claim its test is about — no curation
  route does its SQLite work on the async worker, and the stall equalled the
  handler's own duration — and **stops quoting milliseconds against a row
  count**, pointing here instead. Its numbers were the ones being read as a
  general statement about 25,000 and 250,000 rows, and that reading is what
  did not survive.
- Both records above are marked in place rather than rewritten, per the
  open-items ritual: the reasoning is why they disagreed.
- **The equality the `spawn_blocking` section rests on survives intact.** Its
  138.45/167.86 sit inside this tree's 115.31/143.70 for handlers doing the
  same sweep over the same shape of row, so "stalled for exactly as long as
  the handler ran" is corroborated by an independent instrument. Only the
  framing "at 250,000 rows" was doing more work than it could carry.

**What is still not measured.** A real library's `scan_entry` table rather
than a synthetic one — every figure on this branch is synthetic, and the two
qualifications the first table already carries (synthetic rows, one `regex`
rule) both still apply. What has changed is that the sensitivity nobody had
measured is now measured, and it is larger than either qualification.

### O62's cost claim, and the plan that disproves it

Spec C1 §5.5 deferred *"a work whose members are missing from disk"* to C3 on
the reasoning that the fact is *"not derivable cheaply — `member.path` is
absolute and `scan_entry.rel_path` is relative to its root, so the join is over
a computed string rather than an index"*. The build design's §8 disputes that
and quotes a plan; recorded here because `status.md`'s preamble promises every
claim there traces to a measurement in this file, and O62's row is being
marked against it.

Reproduced 2026-08-26 on a database built by applying
`crates/taiyaku-library/src/schema/001`–`008` to an empty file, with no rows in
it — a plan is chosen from the schema, and an empty table is the case least
favourable to a claim that an index is used:

```
sqlite> EXPLAIN QUERY PLAN
   ...> SELECT m.id, se.id IS NULL AS missing
   ...>   FROM member m
   ...>   LEFT JOIN root r ON r.id = m.root_id
   ...>   LEFT JOIN scan_entry se
   ...>          ON se.root_id  = m.root_id
   ...>         AND se.rel_path = ltrim(substr(m.path, length(r.path) + 1), '/')
   ...>  WHERE m.root_id IS NOT NULL;
QUERY PLAN
|--SCAN m
|--SEARCH r USING INTEGER PRIMARY KEY (rowid=?) LEFT-JOIN
`--SEARCH se USING COVERING INDEX sqlite_autoindex_scan_entry_1
                                  (root_id=? AND rel_path=?) LEFT-JOIN
```

One pass over `member`, which is unavoidable because the question is asked
about every member, and one covering-index probe per row. **The computed
string is on the probing side**; the side being probed is `scan_entry`, whose
`UNIQUE (root_id, rel_path)` is exactly the index the probe wants.

**With a negative control, because a plan naming an index proves nothing on its
own.** `SEARCH … USING COVERING INDEX` and `SCAN … USING COVERING INDEX` differ
by one word and mean opposite things, so the same session was asked a join the
index genuinely cannot serve:

```
sqlite> EXPLAIN QUERY PLAN
   ...> SELECT m.id FROM member m
   ...>   LEFT JOIN scan_entry se ON se.rel_path LIKE '%' || m.path;
QUERY PLAN
|--SCAN m
`--SCAN se USING COVERING INDEX sqlite_autoindex_scan_entry_1 LEFT-JOIN
```

`SCAN`, and the index named anyway. So the word above is load-bearing and the
check can tell the two apart.

Shipped code already performs the same join for a different reason:
`proposals::edition_formats` reads an edition's formats back out of
`scan_entry` on `(root_id, rel_path)`, with `rtrim` in place of `ltrim` for a
root that is a bare `/`. **O62 is therefore a defect with a known fix rather
than a decision**, and it stays open only because the fix is not built.

### The panel against real pages

Nothing on this branch had met real material until here: every screenshot
archived through Task 9 used placeholder images or failed loads, so C3 §2's
first decision — a cover **and two interior pages** per side — had never been
exercised on the comparison it exists to serve.

Ten archives from `corpus/` were hardlinked into a scratch tree under
`target/` and scanned into a scratch database, giving five series, ten works
and three `edition-of` proposals, each scoring 1.0. Nothing wrote into
`corpus/`: the scratch tree held hardlinks rather than copies, and after it was
deleted `find corpus -newermt` over the whole session's span returned nothing
and every source file was still in place.

**The decision holds, and the interiors are what carry it.** The two covers
are two entirely different illustrations — the Full Color edition's Latin-titled
cover against the raw Japanese one — and *both are in colour*, because a manga
cover always is. A reader comparing covers alone sees two unrelated pictures
and learns nothing about whether one edition is the other colourised. The
interior pages answer it immediately: one side's are colour, the other's are
monochrome, in the same panel layout.

`interior_indices` picks `page_count / 4` and `page_count * 3 / 5`, computed
per side, so **how well the two interiors line up depends on the two editions
paginating alike**:

| pair | pages | interiors shown | what the two sides show |
| --- | --- | --- | --- |
| `Full Color` (raw) against the base | 195 and 195 | 49 and 118 both sides | the same two pages, one colourised |
| `Full Color Edition` (en) against the base | 199 and 195 | 50/120 against 49/118 | neighbouring pages, not the same page |

The first row is the design's claim exactly as written. The second is the
qualification it does not state: the interiors are chosen by proportion, not by
alignment, so where the page counts differ the two sides land near each other
rather than on one page. Colour against monochrome still reads at a glance;
"the same panel layout" does not.

`.superpowers/sdd/shots/t10-corpus-*.png` are the shots, and the two `-crop-`
files are the strips at 2×, which is the magnification the judgement actually
needs — at the panel's own thumbnail size the colour/mono difference is
obvious and the panel *layout* is not.

**At 700px the two sides stack**, subject above proposed target. Nothing is
cut off and the thumbnails stay legible, but a side-by-side judgement becomes a
scroll-and-remember one. The alternative is thumbnails too small to read, so
this is the better trade rather than a defect — recorded because the panel's
whole purpose is a comparison, and at that width the comparison is no longer
simultaneous.

**A root whose archives sit directly in it offers `/` as the pattern**, and the
form answers "This rule matches nothing." with Accept disabled — correct, and
worth writing down because it is what a flat scratch tree produces and reads
like a defect until you notice the refusal is right. With the archives in
per-series directories the offered pattern is
`Go-Toubun no Hanayome - Full Color/` and the form is live.

Timings against that library — **debug build**, so these are not comparable
with the release figures above and are recorded only for the shape: warm,
over 16 `scan_entry` rows, `/resolution` about 3.0 ms, `GET …/{id}` about
2.5–2.8 ms, `GET …/proposals` about 0.6 ms and flat.

### Accepting one, and the rows the next scan leaves behind

The accept path had never run outside a fixture either. Through the API, on
the rule the panel offered: it writes the rule, marks the proposal `accepted`,
and **changes no `edition` row at all** — which is what the panel's own
sentence promises ("Nothing on disk is touched"), the rule taking effect on the
next scan.

The next scan applies it, and the library ends up holding two of something.
From a fresh database, one accept and one `library scan`:

| | series | works | editions | members | distinct files |
| --- | --- | --- | --- | --- | --- |
| before the scan | 5 | 10 | 10 | 10 | 10 |
| what the scan printed | 4 | 8 | 10 | 10 | — |
| after the scan | 5 | 10 | **12** | **12** | 10 |

The derivation is right and the store does not follow it: the two colour
volumes each end up present **twice** — once as the `full-color` edition of the
base series' work, which is what was asked for, and once still standing as
their own work under the series the rule was supposed to fold away. Twelve
member rows over ten files, and the command's own summary line disagrees with
its database on every count it prints.

Also measured on those new rows: `edition.kind` is NULL, while the rule that
produced them stores `set_kind = 'official'`. Whether that is the same fault or
a second one is not established here.

They were recorded here undiagnosed, because they are reproducible from a
clean database in two commands and because they were found the only way they
could be — by accepting a real proposal over real archives rather than by
reading the code that does it.

**They turned out to be two faults rather than one, and they were separated
before either was answered.** The NULL `edition.kind` is fixed: nothing wrote
`edition.kind` or `edition.language` from the rules at all, and
`derive::write_edition_metadata` now does. The duplicated volumes are **O73**,
open, because a fix means designing what happens to a work that has been
emptied — including the `progress`, `note` and truth rows that cascade off it.
Re-run after the first fix landed, at four archives rather than ten: the
duplication is unchanged, so the two really were independent.

### What the accept panel promised, and what the database held, 2026-08-26

The whole-branch review's first blocker: the sentence over the Accept button
read *"Accepting will: … to series X, **move N works**, and keep every volume
number as-is."* — and nothing moves. Re-run from a clean database to check the
corrected wording against what the store actually does, rather than editing
the sentence from the code.

**The reproduction.** Four archives hardlinked out of `corpus/` into
`target/c3fix/tree`, two per-series directories:

```
Go-Toubun no Hanayome/               v01, v03  (raw-mono)
Go-Toubun no Hanayome - Full Color/  v01, v03  (raw-color)
```

`library scan --add` → 2 series, 4 works, 4 editions, 4 members, one
`edition-of` proposal at score 2.0. The dry run for the rule the panel offers,
read through `proposals::preview` + `proposals::resolves` on that database:

```
pattern "Go-Toubun no Hanayome - Full Color/", root-scoped, set_series
"Go-Toubun no Hanayome", set_edition_key "full-color", set_kind "official"
Resolution { entries_matched: 2, subject_reached: true, works_moved: 2, … }
```

So the panel printed **move 2 works**. `library proposals --accept 1` writes
rule 5 and changes no `edition` row; `library scan` then prints
`derived 1 series, 2 works, 4 editions, 4 members` over a database holding:

| work | series | vol | editions after the rescan |
| --- | --- | --- | --- |
| 1 | Go-Toubun no Hanayome | 1.0 | `full-color`/`official`, `raw` |
| 2 | Go-Toubun no Hanayome | 3.0 | `full-color`/`official`, `raw` |
| 3 | Go-Toubun no Hanayome - Full Color | 1.0 | `raw` |
| 4 | Go-Toubun no Hanayome - Full Color | 3.0 | `raw` |

**Six member rows over four files.** Works 1 and 2 gained the edition the rule
asked for, with the kind the rule set — that half is right, and is what the
edition-kind fix beside O73 shipped. Works 3 and 4 are untouched: same series,
same files, nothing emptied. `works_moved: 2` is right about the *members*;
"move" is wrong about all of it. This is O73 unchanged, reproduced a third
time, and the sentence is what was fixed here — `curation.ts`'s `consequence`
now says "from 2 works … nothing in the library changes until the next scan",
with `LEFT_STANDING` drawn beneath it naming the duplication and the item.

### Un-accepting, driven the whole way, 2026-08-26

Same database, immediately after the above. The review noted
`curation.ts`'s `unacceptProposal` has no caller and asked whether the gap is
a missing screen. It is not, and this is why:

| step | rule 5 | proposal 1 | editions on works 1–2 |
| --- | --- | --- | --- |
| after accept + scan | present | `accepted` | `full-color`/`official`, `raw` |
| after `--unaccept 1` | **gone** | **`open`** | *unchanged* |
| after the next scan | gone | `open` | *unchanged* |

That last scan prints
`derived 2 series, 4 works, 4 editions, 4 members, **2 editions kept**` — the
two kept editions being exactly those `full-color` rows, left standing by
`import::replace_editions`' deliberate rule that an edition the incoming
document no longer names is kept rather than deleted
(`DeriveReport::kept_editions`). Six member rows over four files before the
un-accept and six after it.

**So un-accept reopens the question and returns the library to nothing.** A
control labelled Undo would be a false statement about what it does, which is
why one is not built here. **O74.**

Nothing wrote into `corpus/`: hardlinks rather than copies, and
`find corpus -newer target/c3fix/started-at` returned 0 entries afterwards.
The scratch tree and both databases are deleted.

## O73's cascade, measured — 2026-08-27

O73 named the measurement it was waiting on and left it untaken: *"whether a
stranded work also strands the `progress`, `note` and `truth_*` rows hanging on
its slug. The corpus session reproduced the duplication and not the cascade …
Reproduce it before choosing."* This is that measurement. **The rows survive on
the work the fold empties, and a plain delete takes every one of them** — so by
the item's own criterion the first option is data loss, and the third is the
only honest one. Whether the third is *reachable* is the second half of what
was measured, and it is not free either.

### The reproduction, with a reader on it

The four-archive tree of 2026-08-26, rebuilt at `target/o73/tree` — hardlinks,
not copies:

```
Go-Toubun no Hanayome/               v01, v03  (raw-mono)
Go-Toubun no Hanayome - Full Color/  v01, v03  (raw-color)
```

`library scan --add` prints `7 added, 0 updated, 0 gone` and
`derived 2 series, 4 works, 4 editions, 4 members; 1 proposal(s) raised` — the
same `edition-of` proposal at score 2.0 as before. 5.8 s, debug build.

**What the three earlier reproductions did not have is a reader.** Before the
accept, each of the four works was given a position, a note, one looked-at
page, one box and one labelled member digest — **through the API rather than by
`INSERT`**, so every row is one the product's own write path produced:
`PUT /api/works/{work}/progress`, then `PUT …/notes` and `PUT …/truth` each
behind the `If-Match` its handler demands. Slots 11, 22, 33 and 44, one per
work, so a row can be told from its neighbour by eye.

| work | | progress | note | truth_page + box | truth_member |
| --- | --- | --- | --- | --- | --- |
| 1 | `Go-Toubun no Hanayome` v01 (mono) | `raw` slot 11 | slot 11 | `raw` p11 | `86c82cea…` |
| 2 | `Go-Toubun no Hanayome` v03 (mono) | `raw` slot 22 | slot 22 | `raw` p22 | `641a19a8…` |
| 3 | `… - Full Color` v01 (colour) | `raw` slot 33 | slot 33 | `raw` p33 | `920d44d8…` |
| 4 | `… - Full Color` v03 (colour) | `raw` slot 44 | slot 44 | `raw` p44 | `6b302592…` |

Then `library proposals --accept 1` (`wrote rule 5`) and `library scan`, which
prints `0 added, 1 updated, 0 gone` and
`derived 1 series, 2 works, 4 editions, 4 members; 0 proposal(s) raised` over a
database holding **four works, six editions and six members over four files**.
**O73 unchanged, reproduced a fourth time**, now with a reader's data standing
on the works the fold was supposed to empty.

**One thing the rescan does that is worth recording on its own.** Works 1 and 2
lost their `edition` rows and gained new ones — ids 1 and 2 are gone, replaced
by 6 and 8, beside the new `full-color` rows 5 and 7 —
`import::replace_editions` doing exactly what its name says. Every
`progress`, `note` and `truth_*` row on those works survived that, because all
five tables key on the edition **string** and not on `edition.id`. A schema
that had joined them to the row id would have lost the base works' reader data
on an ordinary rescan, with no accept involved at all.

### The cascade

`DELETE FROM work WHERE id IN (3,4)` — the two works a fix would be emptying —
on a copy, `PRAGMA foreign_keys=ON`:

| | work | edition | member | progress | note | truth_page | truth_box | truth_member |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| before | 4 | 6 | 6 | 4 | 4 | 4 | 4 | 4 |
| after | 2 | 4 | 4 | **2** | **2** | **2** | **2** | **2** |

Every per-reader and per-labeller row on those two works goes with them: a
position, a note, a page somebody had looked at, the box they drew on it and
the digest of the archive they drew it against. This is the answer O73 asked
for, and it is the one that closes off the cheap option.

**A copying trap worth recording, because the first attempt fell into it.** The
library runs in WAL mode, so `cp DB COPY` copies the main file and not the
`-wal` beside it. The first copy read as four editions and **zero** progress
rows — a snapshot from before the session's writes — and would have answered
the question backwards, reporting a cascade that costs nothing. `sqlite3 DB
".backup COPY"` is the copy to take, and it works while `serve` holds the
database.

### "Stranded" is not true of the library today

The duplicates are not orphans. After the rescan **and** after a `serve`
restart, works 3 and 4 answer everything:

| request against work 3 | answer |
| --- | --- |
| `GET /api/works` | lists all four works; 1 and 2 now carry `["full-color","raw"]` |
| `GET …/progress` | `{"edition":"raw","slot":33}` |
| `GET …/notes` | the planted note, unchanged |
| `GET …/raw/pages/33` | `200`, 725,192 bytes |
| `GET /api/library/series` | **both** series — 1 with 2 works/4 editions, 2 with 2 works/2 editions |

So today's defect is duplication, not loss: the reader's position is intact and
reachable, and the library simply holds each colour volume twice. The word
*stranded* describes the state a fix would create, not the state that exists —
worth saying plainly, because it is the difference between a data-loss item and
a correctness one, and O73 was opened without knowing which it was.

A **third** scan, with nothing changed on disk, leaves it at four works, six
editions and six members. Nothing tidies this up later.

### What a move would have to get past

The third option — move what hangs off the emptied work onto the work its
material went to — was tried as SQL, each statement on its own `.backup` copy:

| move | result |
| --- | --- |
| `progress` → work 1, edition kept `raw` | **REFUSED** — UNIQUE `(user_id, work_id)` |
| `progress` → work 1, edition → `full-color` | **REFUSED** — same; the key holds no edition |
| `truth_page` → work 1, edition kept `raw` | **REFUSED** — FOREIGN KEY |
| `truth_page` → work 1, edition → `full-color` | **REFUSED** — same |
| `truth_member` → work 1, edition kept `raw` | **REFUSED** — UNIQUE `(work_id, edition, ordinal)` |
| `truth_member` → work 1, edition → `full-color` | accepted |
| `note` → work 1, edition kept `raw` | **accepted, and wrong** |
| `note` → work 1, edition → `full-color` | accepted |

`truth_page` refuses because `truth_box`'s composite foreign key names
`(work_id, edition, page)` with no `ON UPDATE` — the schema is built for
`put_truth`'s order, page row then boxes, and does not support relocation by
`UPDATE` at all. A move has to delete and re-insert in that order.

**The `note` row is the sharp one, because nothing stops it.** Moved with its
`edition` left as `raw`, a note the reader wrote about **colour** page 33 lands
on work 1, whose `raw` edition is the **monochrome** archive: same slot number,
different book, and no constraint, validator or screen says so. It is the same
failure `truth_member`'s own schema comment calls the sharpest form of this
problem — a box on the wrong art on the right page — one table over, and
reached by the obvious one-line fix.

**What makes the move possible at all** is that the two rows describe the same
bytes. Work 3's `raw` edition and work 1's new `full-color` edition are the
same archive: sha256 `920d44d8…`, 195 pages, one member each. So slots, page
indices and box coordinates transfer one for one, and only the `edition` string
has to change. The arithmetic is exact; there is nothing to re-derive.

| work | edition | sha256 | pages |
| --- | --- | --- | --- |
| 1 | `full-color` | `920d44d8…` | 195 |
| 1 | `raw` | `86c82cea…` | 195 |
| 3 | `raw` | `920d44d8…` | 195 |

**What makes it a decision** is `progress`'s primary key, `(user_id, work_id)`:
one position per work, not one per edition — deliberate, and its own schema
comment says why. The reader here holds a position on work 1 (`raw`, slot 11)
and a position on work 3 (colour, slot 33). After the fold those are two
positions on one work, and the schema stores one. Keep the most recently
updated, keep the target's, keep the incoming one, or widen the key: that is a
product call about what a reading position *means* across editions of one
volume, and it is the thing to settle before any of this is written. `note`,
`truth_page`, `truth_box` and `truth_member` have no such problem — they are
per-edition all the way down and merge cleanly once the edition string is
rewritten.

### The emptied series, and what the screen does with it

`series` has no dependency on `work` — `work.series_id` is `ON DELETE SET NULL`
in the other direction — so deleting works 3 and 4 leaves the
`Go-Toubun no Hanayome - Full Color` series row standing with nothing in it.
Served: `GET /api/works` answers two works and `GET /api/library/series` answers
**one** series, so `browse` filters an empty series out and the orphan row is
invisible on screen. It is still in the database, and nothing deletes it.

### Reproducing this

Six commands from a clean tree; the whole run is about a minute in a debug
build.

```sh
mkdir -p target/o73/tree/"Go-Toubun no Hanayome" target/o73/tree/"Go-Toubun no Hanayome - Full Color"
ln corpus/gotoubun-v01/raw-mono/*.cbz corpus/gotoubun-v03/raw-mono/*.cbz  target/o73/tree/"Go-Toubun no Hanayome"/
ln corpus/gotoubun-v01/raw-color/*.cbz corpus/gotoubun-v03/raw-color/*.cbz target/o73/tree/"Go-Toubun no Hanayome - Full Color"/
taiyaku library scan --add "$PWD/target/o73/tree" --db target/o73/lib.db
printf 'pw\npw\n' | taiyaku library user promote reader --db target/o73/lib.db   # then plant over the API
taiyaku library proposals --accept 1 --db target/o73/lib.db && taiyaku library scan --db target/o73/lib.db
```

Nothing wrote into `corpus/`: hardlinks rather than copies, and
`find corpus -newermt '2026-08-27 09:00'` returned nothing afterwards. The
scratch tree and its databases are deleted.

## O73's fold, proved against real archives, 2026-08-27

Nine tasks of the progress-per-edition plan shipped the widened `progress`
key, the fold and its refusals against fixtures. None of it had met a real
library. This is that run: plant a reader through the API, accept, rescan,
and read back what the database actually holds — the same four-archive
reproduction O73 itself came from, taken all the way through this time.

### The reproduction, rebuilt

Four archives hardlinked (never copied) out of `corpus/` into
`target/o73/tree`, the same two per-series directories as O73's own
reproduction:

```
Go-Toubun no Hanayome/               v01, v03  (raw-mono)
Go-Toubun no Hanayome - Full Color/  v01, v03  (raw-color)
```

`library scan --add "$PWD/target/o73/tree" --db target/o73/lib.db` printed
`derived 2 series, 4 works, 4 editions, 4 members; 1 proposal(s) raised` —
matching the plan's stated expectation exactly. Before the fold: 4 works, 4
editions, 4 members over 4 distinct files.

### The reader, planted through the API, not by `INSERT`

`library user promote reader` created the account, and
`taiyaku serve --db target/o73/lib.db --addr 127.0.0.1:8099` served it. A
position, a note and a truth document — one box, one member digest — were PUT
onto all four works, at slots 11, 22, 33 and 44 on works 1 through 4
respectively (`raw` is every edition's only key before the fold, so every PUT
named it). Every write is a real route response, not a row this session
constructed by hand:

- `PUT .../progress` (unconditional, no `If-Match`): `204` on all four.
- `PUT .../notes`, `If-Match` read off the prior `GET`'s `ETag`: `200` on all
  four.
- `PUT .../truth`, same `If-Match` pattern, each work's own member `sha256` as
  provenance: `200` on all four.

Twelve reader rows stood before the fold: 4 `progress`, 4 `note`, 4
`truth_page` (one `truth_box` and one `truth_member` under each).

### The fold, and what it actually printed

`library proposals --accept 1` wrote rule 5 and changed no `edition` row, as
every earlier accept in this branch's history has. The following
`library scan` printed:

```
derived 1 series, 2 works, 4 editions, 4 members, 2 work(s) folded away
(2 position(s) written, 0 position(s) lost, 2 note(s) and 2 truth page(s)
moved, 0 truth page(s) already held, 0 row(s) discarded), 1 series left
empty; 0 proposal(s) raised
```

**It fired.** Six narrowings landed on this detection rule over the course of
this branch, and this is the first time it has run against material this
project's own test fixtures did not write. The two colour works
(`Full Color - v01`, `Full Color - v03`) are gone, and their series
(`Go-Toubun no Hanayome - Full Color`) stands empty — measured to show the series
row stands with no work referencing it, matching the design's §6 claim that an
emptied series is left standing rather than cascaded. The design document also states
it is invisible on screen for exactly that reason, a visibility claim not re-verified
in this session.

### The end state, queried rather than trusted

```
sqlite3 target/o73/lib.db "SELECT (SELECT count(*) FROM work), (SELECT count(*) FROM edition),
  (SELECT count(*) FROM member), (SELECT count(DISTINCT path) FROM member);"
2|4|4|4
```

Two works, four editions, four members over four files — the duplication this
whole plan exists to close is gone, and both remaining works carry a `raw`
edition and a `full-color` edition, the latter's `kind` correctly `official`
off the accepted rule.

Every reader row survived, remapped rather than lost:

| work | edition | slot | truth page |
| --- | --- | --- | --- |
| 1 (`v01`) | `raw` | 11 | 11 |
| 1 (`v01`) | `full-color` | 33 | 33 |
| 2 (`v03`) | `raw` | 22 | 22 |
| 2 (`v03`) | `full-color` | 44 | 44 |

Twelve reader rows before the fold and twelve after, on the four `(work,
edition)` pairs above — nothing lost, which is exactly what the scan's own
`0 position(s) lost` and `0 row(s) discarded` said before the database was
ever queried.

### Reachable, not just present

`serve` was restarted against the post-fold database — its own startup line
now reads `2 editions` for both works, where it read `1` before the fold.
`GET /api/works/go-toubun-no-hanayome---v01-cbz/progress` answered both
positions, newest first:

```json
{"positions":[{"edition":"full-color","slot":33,"updated":"2026-08-27 16:08:33"},
              {"edition":"raw","slot":11,"updated":"2026-08-27 16:08:32"}]}
```

`GET .../full-color/pages/33` answered `200 image/jpeg`, 1605×2400 — and it is
actually colour, not merely on the colour edition's route: a 50×50 downsample
put 720 of the 1,650 sampled pixels outside the ±8 per-channel band a
monochrome scan would produce.

### The corpus, untouched

Hardlinks throughout, and `find corpus -newermt '2026-08-27T16:07:40Z'`
printed nothing once the whole run — plant, fold, assert, restart, clean up —
was done. The scratch tree and database were removed with
`rm -rf "${W:?}"`, `W=target/o73`, the colon form rather than a bare `"$W"`.

## Spec C3 PR 2: metadata editing, its on-disk home, and O62 (2026-08-28)

### O62's query plan, re-measured after migration 010

`docs/status.md`'s O62 row rests on one line of a query plan, and that plan was
measured on 2026-08-26 against a database built from `schema/001`–`008`. This
branch adds **migration 010**, which puts two columns on `work` and two on
`edition` — so the measurement was retaken rather than assumed to survive, by
the task's implementer and independently by its reviewer, against a database
built by applying all ten migrations to an empty file.

Unrestricted — the form the design argument quotes:

```
QUERY PLAN
|--SCAN m
|--SEARCH r USING INTEGER PRIMARY KEY (rowid=?)
`--SEARCH se USING COVERING INDEX sqlite_autoindex_scan_entry_1
                                  (root_id=? AND rel_path=?) LEFT-JOIN
```

Unchanged. One pass over `member`, which is unavoidable because the question is
asked about every member, and a covering-index probe per row. **The design
claim survives the migration.**

**The roots-restricted form was measured too, and neither report had covered
it** — which matters, because that is the form the shipped code runs: Task 7b
gave `missing_members` the caller's grant, so `AND m.root_id IN (?,?)` is
appended in every restricted call. The `se` covering-index line is identical.
What moves is `m`'s own access path: `sqlite_autoindex_member_1 (edition_id=?)`
becomes `member_by_root (root_id=?)`, and `r` is scanned first by rowid. No
correctness consequence, and no cost claim in `status.md` depends on it — but a
plan nobody had looked at is now on the record rather than assumed to match the
one that was.

### What the branch's own numbers are

`env -u TAIYAKU_CORPUS scripts/check-all.sh` at `d0050a7`: 1705 Rust tests
(1671 at the branch point), and 1,694 in `web/` — 1,080 `node`, 614 `jsdom` —
against 1,669 before. Both figures are the ones `docs/status.md` now states,
and `scripts/status-test-count.mjs` checks the Rust one against the JUnit report
at both of its two sites.

### O85, measured: what a browse listing cost in SQL statements, and what it costs now

The item was recorded as *"not measured against the corpus"*. It is now
measured, twice against the same corpus and with the same instrument, so the
before and after are comparable rather than one figure beside an argument.

**The instrument.** `browse.rs` routes every query it issues through two
wrappers, `prepare` and `query_row`, which increment a `#[cfg(test)]`
thread-local counter. That is the same choice `proposals::tests`' one-sweep
check made and for the same reason — no `rusqlite` Cargo feature has to be
turned on for a test to see it, and thread-local rather than a shared `static`
so a test running concurrently on another thread cannot land inside the
reset-then-read window. Counting *statements* rather than calls to a named
function matters: a call counter on the per-work loop would read zero the
moment that loop was deleted, which is passing for the wrong reason.

**The before figure was taken with the same counter, not inferred.**
`crates/taiyaku-library/src/browse.rs` and its `tests.rs` were restored to
`c85c357`, the instrumentation re-applied to that version alone, and the
measurement re-run — so the only difference between the two columns is the
query shape.

**Corpus, 2026-08-29** — 13 settled roots (`incoming/` excluded, as
`scanning_and_deriving_the_corpus_produces_real_structure` excludes it), 11
series, largest series `Kobayashi-san Chi no Meidoragon` at 3 works and 3
editions:

| call | before | after |
| --- | --- | --- |
| `one_series`, largest series | 8 | 4 |
| `home` | 43 | 5 |

**`home`'s 43 is the *capped* case, not the worst one.** Its three lists are
bound at `LIMIT 20` each and its `added` list was full — 20 works — so 43 is
what a bound already in place produced. `unfiled`, reached by both
`/api/library/series` and `/api/library/search`, carries no such bound, and it
is the same 4 after.

**The fixture is where the law is legible**, because the corpus's largest
series is only three works and a real hurt needs a bigger set than this corpus
holds. `library_with_volumes` builds two roots, so every work carries two
editions:

| works in the series | before | after |
| --- | --- | --- |
| 2 | 8 | 4 |
| 6 | 20 | 4 |

That is **2 + 3n** — one statement for the series, one for the works, then per
work one for its editions and one per edition for its `missing` count —
against a flat **4**. The standing check asserts the two counts are *equal*
rather than asserting 4, because the number is an implementation detail a later
join may legitimately move while "the cost does not grow with the set" is the
property the item is about.

**A rename travels with this**: `browse::missing_members`, which answered one
edition, is now `browse::missing_by_edition`, which answers a set and returns a
map. **An edition with nothing missing has no row in that map** — `COUNT(*)`
over an empty group produces none — so a caller reads absence as zero. O84's
row named the old symbol as a site and now names the new one. The passage above
in "O62's query plan, re-measured after migration 010" names the old one too
and is left as written: it records what was measured on the day, and the plan
it reports was taken against that code.

### The mutation sweep that proved nothing, and the one `use` line that fixed it

O85's branch swept clean on its first run — `--since main`, 48 mutants, **18
caught, 30 unviable, 0 survivors**. Read at face value that is the result the
harness exists to produce. It was worth nothing about the code the branch had
just written.

**Fourteen of the thirty unviable were the whole-body mutants of the two new
functions**, `editions_by_work` and `missing_by_edition` — every one of them.
`cargo-mutants` replaces a body with a literal it generates and writes the
**short** type name:

```
replace editions_by_work -> Result<HashMap<i64, Vec<EditionCard>>, String> with Ok(HashMap::new())
```

`browse.rs` spelled `std::collections::HashMap` out in full at every use site,
so `HashMap` was not in scope, so none of those mutants compiled, so all of them
were counted `unviable` and none was ever run. **An unviable mutant is not a
survivor**, so this does not appear in the survivor list at all — it is
invisible in exactly the output the harness is read for.

Adding `use std::collections::{HashMap, HashSet};` to the module took the same
file from **48 mutants to 95**, and both new functions are now covered with
every one of their mutants caught — including
`delete field missing from struct EditionCard expression in editions_by_work`,
which is the one that asserts the batched `missing` count is actually wired
through the map. That comment on the import is load-bearing and says so.

**The seven new survivors that run reported are all false, and checking cost
ten minutes.** All seven are `visible_editions`. Hand-applying
`Ok(HashMap::new())` in place and running both packages:

| package | result with the mutation applied |
| --- | --- |
| `taiyaku-library` (the package the sweep tested) | 557 passed, 0 failed |
| `taiyaku-serve` | **5 failed**, incl. `the_works_feed_does_not_name_a_work_outside_the_grant` and `the_work_list_does_not_name_a_work_outside_the_grant` |

That is **O87 reproduced on a second function**, independently of the
`librarydoc::apply_with_series_id` case that opened it — and with the same
count of five cross-crate failures. `visible_editions` exists *for*
`taiyaku-serve`; its own doc says so. The seven are recorded in
`docs/mutants-baseline.md` as deferred behind O87 rather than as a gap in the
tests, with this verification as their reason.

**Two different failures, one shared consequence.** O87 makes a *caught* mutant
read as MISSED. O88 makes an *untested* function read as a compile artefact.
The first inflates the survivor list, the second empties it — and the second is
the more dangerous, because a clean run reads as a result.

*This section first said "37 files in this workspace still spell those paths
out in full", naming `proposals.rs` at 41 and 42 unviable as the worst case.
**That figure was wrong and is corrected below** — it counted files
*mentioning* `std::collections::` rather than functions *returning* one, and
only a return type can produce the uncompilable mutant. Measured the same day:
two functions, and `proposals.rs` has none of them. Kept as written because the
wrong measurement is the point — see "O88's scope, measured" below.*

### The grant test that could not tell right from wrong, and the id collision that hid it

O85 replaced a per-work edition query with one that binds its roots twice and
then a generated `IN (?,?,…)` of work ids, so both the placeholder count and
the binding order now scale with the set. Every grant test in `browse` asked
about a **single** work — `two_root_library` holds one volume — and an `IN`
list of one can show neither a mis-sized list nor an off-by-one in the order
the parameters are pushed. So a multi-work grant test was added.

**The first version of it passed against a deliberately broken build**, which
is the only reason it is worth writing down. Swapping the last two parameter
groups — pushing the work ids before the second root group — left it green.
The fixture is why: `library_with_volumes` adds `en` first, so the granted root
is id **1**, and derivation gives the three works ids **1, 2, 3**. Under the
swap the `IN` list reads `{w2, w3, r}` = `{2, 3, 1}`, which is *the same set*
as `{1, 2, 3}`, and the root id that lands in the `EXISTS` clause is `w1` = 1,
which is the granted root. Every parameter is in the wrong place and every one
of them is coincidentally right.

**Granting `raw` — the root added second, id 2 — breaks the coincidence**, and
the same substitution then fails on the first work. That is the version kept.
Both failure modes are now checked by hand: the binding-order swap fails it,
and `holes(1)` in place of `holes(work_ids.len())` fails it.

The general shape is one this repository has met before: a test written against
a fixture whose values collide cannot distinguish the code it was written for
from the bug it was written against. The cheap defence is what was done here —
make the mutation by hand and watch the new test fail — and it costs about two
minutes.

**The parameter ceiling this trades into.** Replacing N queries with one
N-parameter query moves an unbounded count from one place to another, and
`unfiled` carries no `LIMIT` at all. Measured against this machine's system
libsqlite3, 3.53.4, 2026-08-29 by binary search on a bound `IN` list: **at
least 200,000** parameters bind without complaint. A library would need that
many works in one listing to reach it, so this is recorded on
`editions_by_work` rather than guarded — a guard would be code no library can
execute.

### O84: which edition the reader actually opens, and why the item's own answer was the wrong one

O84's entry proposed carrying **the hub's identity** in the browse response.
Reading the client first says that is not the question the guard needs answered.

`startup.ts` builds its state with `reading::restore(progress, hub, resolver)`,
and `restore` has three branches: no stored position at all opens the **hub**;
a position whose edition the resolver knows opens **that edition**; a position
naming an edition the resolver does *not* know falls back to the **hub**.
`api.ts::getProgress` supplies that position as the head of a newest-first
list — "the server answers every edition's position, newest first… the reader
resumes into the head", which has been true since a position became per-edition
in migration 9.

**So the edition that opens is caller-dependent, and a hub-only field cannot
express it.** The case that decides it is `Library.tsx`'s Continue strip: a
reader part-way through the intact sibling of a work whose hub has lost its
files is exactly who the strip exists to bring back, and a hub-only guard would
withhold that link. That is O62's over-withholding — the failure the `every`
form was written to avoid — reintroduced in a narrower place. The item's own
proposal would have traded one wrong answer for another.

`WorkCard.opens` is therefore `restore`'s rule computed server-side: the newest
position's edition **if the grant still shows it**, and `work.hub` otherwise.
The grant check is not decoration — a progress row naming a hidden edition is
precisely one `resolver.slotCount` answers `undefined` for, so agreeing with
`restore` requires it.

**The cost, taken deliberately: `WorkCard` is per-caller from here.** It was
user-independent, and `works_of` took no `user_id`; it now does, threaded
through `work`, `unfiled` and `search` and four `taiyaku-serve` call sites.
No user-independent field can answer "is this readable", because the same work
is readable for one reader and not for another.

**Four component tests had to change their answer, and that is the fix
working.** `Library.test.tsx` and `Series.test.tsx` each built a volume with
one broken edition and one intact and asserted the link was kept — correct
under `every`, and under-specified afterwards, because the answer now depends on
which of the two opens. Each names `opens` explicitly, and the mirror case is
asserted beside it rather than left implied. The download test in
`Series.test.tsx` needed the same treatment for a different reason: it is about
the *download* being withheld for a broken tile while the volume stays
readable, so its `opens` has to name the intact edition or the reader link
disappears too and the test can no longer tell its two guards apart.

### The defect that was not there, and the one that was

**This branch first reported that a grant hiding the *hub* breaks the reader
outright** — reasoning that `session.ts::orderedEditions` prepends the hub
whether or not the caller can see it, `restore` returns it, and the resolver
holds no slots for an edition it was never sent. **That is wrong**, and it is
written down rather than quietly dropped because of how it was got wrong: the
client was read without the route that feeds it.
`routes::filter_sidecar_editions` already substitutes a hidden hub with the
first surviving edition in key order, and its own doc comment says so in as
many words — *"`hub` is checked too, and substituted rather than left
dangling"* — citing `browse::cover_target` for the same trap on a series cover.

**The substitution being real is what made the fix wrong.** `opens` returned
`work.hub` whenever no visible position named an edition, hidden hub included.
So for a caller whose grant hides the hub, `opens` named an edition that is not
in `editions`, `unreadable`'s `find` answered `undefined`, the guard answered
"readable" — and whatever the edition the reader *actually* opens had lost went
unasked. That is O84's own defect, reintroduced by O84's fix, in the one case
the fix did not think about.

`opens` now falls through to the first visible edition in key order when the
named one is not visible. `editions_by_work` returns each work's editions in
key order, which is the same order `filter_sidecar_editions` takes with
`BTreeMap::keys().next()`, so the two agree by construction rather than by
coincidence. `opens_substitutes_the_first_visible_edition_when_the_grant_hides_the_hub`
fails against the version without it.

**The lesson is the cheap one and it was nearly skipped.** The claim was going
to be filed as an open item on the strength of reading two client functions.
Reading the third — the server route between them — took five minutes and
turned a defect that did not exist into one that did.


### O88's scope, measured — two functions, not thirty-seven files

The claim above was sized by `grep -rl 'std::collections::'` and that is the
wrong instrument. `cargo-mutants` replaces a function **body** with a generated
literal, so the only way the short-name problem can bite is a function whose
**return type** is a fully-qualified collection. A local or a parameter written
out in full costs nothing at all.

**`proposals.rs` is the counter-example, and it was the claim's own headline.**
42 unviable, `std::collections::BTreeSet` written sixteen times, and **zero** of
the 42 from this cause — all sixteen are locals or parameters. Swept 2026-08-29,
206 mutants, 156 caught:

| cause of the 42 unviable | count |
| --- | --- |
| `Ok(Default::default())` and variants, against types deriving no `Default` | 18 |
| `cargo-mutants` guessing a constructor for a custom type (`Offer::new`, `Offer::from`) | 24 |
| the fully-qualified path | **0** |

Workspace-wide the right grep — a return type, not a mention — finds **two**
functions: `opds::visible_editions_by_work` and
`routes::visible_editions_or_refuse`, against **41** files that mention the
path. Two `use` lines: their twelve whole-body mutants went from `unviable` to
viable and **all twelve are caught**, 0 missed, 0 unviable, and no new survivor
surfaced. Nothing in the workspace returns a fully-qualified collection now.

**So the cause that looked like a branch of its own was two lines, and the one
filed as a footnote is the real one.** 18 of `proposals.rs`' 42 are the missing
`Default` derive, and 24 are cargo-mutants guessing at a constructor, which
nothing here can fix. How many functions the derive cause hides workspace-wide
has not been counted; that is the measurement that says whether adding
`derive(Default)` to public types for a test tool's benefit is worth it.
### O86: fixing the permission model rather than the handler, and the eleven fixtures that had been proving nothing

O86 was a stale cache: `patch_series` writes `series_title` for the whole
series — that table is keyed `(series_id, title)` and has no per-work column a
grant could filter on — while its per-work loop refreshes `.library.json` only
for the works `works_in_series` returns, which *is* grant-filtered. A
restricted administrator therefore renamed a series library-wide and left every
work outside their grant holding an old cached copy.

**The entry's two options were not two.** "Refuse the edit when the series
straddles the grant" or "leave `series_title` partially updated" — and the
second cannot be done at all, there being no per-work dimension in that table
to leave partial. So the real choice was refusal, or removing the state that
makes the question askable.

**Measured before choosing.** This library holds two accounts, `davey` (admin)
and `alex` (user), and **both hold zero grants** — an empty grant is
unrestricted. No account was in the O86 shape, and nothing but the CLI can
create one. So the change was free of live consequence, which is what made
changing the model the cheaper option rather than the braver one.

**The rule: role dominates.** `Principal::from_account` drops an
administrator's roots and reads `can_download`/`can_stream` as true. Spec A2
lists the four permission columns side by side and never says whether they
compose; they do not, and one function now decides it. It is done where the
principal is built rather than by refusing the CLI grant, because all three
resolvers — API key, HTTP Basic, session cookie — pass through it, and a CLI
refusal would leave an existing row live, including on an account promoted to
admin after being granted. The row is kept rather than deleted, so a demotion
restores the grant it always held, and `grant-root` says so.

### The eleven fixtures, and why this is the O62 shape again

Making the rule true broke eleven test fixtures, and **that is the finding
worth recording**: every one of them granted a root — or withheld a
capability — from the *seeded admin*, then asserted the restriction held.

Under the new rule those assertions cannot fail. But the point is that they
were already weak: they were measuring a filter through an account the model
had never decided should be filtered, and a reviewer reading them would have
believed the filter proven. This is the same shape O62's whole-branch review
found in `unreadable` — *"mutating `.every` to `.some` left the entire
1694-test web suite green"* — and the same shape as O84's grant test, which
passed against a deliberately broken build because the fixture's root ids and
work ids collided. **Three instances now, all of them a fixture that cannot
distinguish the code from its negation.**

They route through one helper, `test_support::grant_root`, which demotes before
granting. One place holds the rule instead of eleven.

### Three driven proofs, reclassified rather than repaired

The four admin-only metadata PATCH routes were classified `Refuses` and driven,
asserting 404 for a hidden work. **That proof is void, not weakened**: it
required an account holding both a live grant and the admin role those routes
demand, and no such account can exist now. They move back to
`GrantIndependent` — where I8 found them — and their blocks keep driving all
four, asserting the guard that is actually load-bearing: the role gate refuses
first. Deleting the blocks would have left four routes with no driven proof at
all, which is how a reclassification quietly becomes a hole.

**The positive control needed the most care.** I8's pair was "the withheld half
404s, the granted half still 200s", proving the check was narrower than "deny
everything". Asserting 403 in both halves would have collapsed it into one
assertion made twice — which looks like coverage and is not. The account is
promoted for that half only, so the pair now reads **non-admin → 403, admin →
200 against identical material**, and the difference is attributable to the
role alone. The promotion is scoped and reversed immediately after, because the
OPDS proofs further down the same test need a caller whose grant is live.

### The guard the rule quietly stopped testing

`work_by_slug`, `edition_owner`, `member_owner` and `works_in_series` still
take `roots` and still filter on it, kept deliberately: correct for any caller
with a non-empty grant, and the difference between a leak and none if the role
rule is ever revisited.

**This was first written as "unreachable through those four routes", and that
is wrong.** They are reached on every single edit — a sweep catches all
nineteen whole-body mutants of the four. What no longer reaches them is a
*non-empty* `roots`, so the clause is always a no-op there. The distinction
looked like pedantry until the sweep produced the consequence:

```
metadata.rs:379:5: replace root_grant_clause -> String with String::new()   MISSED
```

**Deleting the entire grant filter passed every test in the workspace.** The
I8 proofs that used to exercise it drove a restricted *administrator*, which
is exactly the account O86 made unrepresentable — so the rule that closed O86
took that guard's only coverage with it, silently, and the sweep is the only
thing that said so.

`metadata::tests::the_grant_clause_still_filters_even_though_no_route_can_supply_a_grant`
asserts the contract at the function instead of through a route, with a slice
no route can currently supply, and fails against that mutation. Verified by
hand-applying it.

**The general lesson is the one this session keeps meeting from a new angle.**
Three times today a test could not distinguish the code from its negation —
O62's `unreadable`, O84's colliding fixture ids, O86's eleven admin fixtures.
This is the fourth and the nastiest, because nothing was *wrong* with the test
suite before or after: a rule change moved a guard out of every test's reach
without touching a line of the guard or a line of any test. Only a sweep sees
that, and only if the unviable count is read alongside the survivor list.

### O80: one asymmetry, six fields, and the fact a series block could not express

O80 read as "a hand edit withdraws four columns permanently". The cause was
narrower and worse than that: **`librarydoc::of` projected the *hand* value for
two fields and the *effective* value for the rest.**

Migration 010 gave `work.title` and `edition.label` a `_hand` column precisely
so a document would carry only what a person set. It stopped there. `volume`,
`series_id`, `edition.language`, `edition.kind` and every `series_title` row —
including the `is_primary` one `derive` mints for each series it discovers —
were projected at their effective value. So the document every save wrote
claimed derivation's own answers as a person's, `hand::save` armed the marker
on any non-empty document, and `write_work` read that marker as *never touch
these four columns again*.

**The marker was doing two jobs**, and that is the part worth keeping: *"this
document has been read back from disk, do not read it again"* and *"do not
overwrite these columns"*. A title-only edit legitimately satisfies the first
and has nothing to do with the second. Migration 011 answers the second per
field, and the guard keeps only the first.

**One column each, where 010 needed two.** 010 added `title_derived` because
nothing recomputes a title after `import::insert_work`'s INSERT, so the first
hand edit destroyed the derived value and clearing had nothing to restore.
These six are recomputed by `derive` every pass, so `COALESCE(<hand>, <what
this pass derived>)` needs no second copy and clearing restores derivation's
answer on the next scan.

### The fact a series block could not express

`LibraryDoc.series` carries two different things: the **series'** titles and
reading direction — series-level data replicated into every member work's
document — and **this work's** attachment to it.

Nothing distinguished them, so a document that corrected only a series' titles
was indistinguishable from one that said "a person put this work here". Under
per-field provenance that ambiguity is fatal: `apply_series` would record the
derived attachment as hand-chosen on the next apply, which is O80 moved one
table across rather than fixed. `SeriesInfo::chosen` is that missing fact, and
it is absent from the JSON when false, so a titles-only correction says nothing
about attachment at all.

`apply_series` still resolves by title text. That is deliberate and load-bearing:
a document arriving at a database that has never seen this library has nothing
else to resolve against, which is the import path spec A's round trip depends
on. What changed is only whether the resolution is recorded as a choice.

### What the existing tests caught that a reading would not have

**An empty series block used to mean "take this work off the shelf."** After
this change it means "nobody has corrected this series" — true of nearly every
work in a scanned library — so clearing `series_id` unconditionally detached
all of them on the first apply. `the_last_work_leaving_a_series_takes_the_shelf_with_it`
is what said so. Withdrawal now removes the effective attachment only where it
was hand-chosen, and `derive` re-attaches on the next pass.

**Clear-versus-keep differs per field, and each answer has a reason.**
`language` and `kind` clear immediately, because `write_edition_metadata`
re-derives both on the next pass. `label` falls back to `label_derived`,
because nothing recomputes a label. `volume` keeps its value until the next
scan, because blanking it would empty a working row for no gain. Three
different answers in one function, each one a test.

### The thing this does not fix, and the three items it explains

The series' own metadata is **series-level data stored per work**. That is why
editing a series loops over every work (O85's N+1 inside an N+1), why the loop
covering only granted works left hidden siblings stale (O86), and why the block
needed `chosen` at all.

A series-level document — exported once, with its own provenance — would
dissolve all three rather than handling each. It is a change to the document
set, `export`, `import` and `apply_series`, which is a design round rather than
a defect fix, and nothing here attempts it.

## O81 and O82, measured against real archives — 2026-08-31

Both were tabled as decisions on 2026-08-28 with no number against either.
This is the number: **three of five works lost or reverted a hand-set title
when the database was rebuilt from the same root.** That is the store spec
§4.4 claim — *losing the database costs a rescan, not the organisational
work* — failing on the two most routine events a library sees.

### The library

Five `.cbz` files out of `corpus/` into one root under `target/`, **copied
rather than hardlinked**: three of the events below rewrite an archive, and a
hardlink would have rewritten the corpus's own copy. Three series, five works,
five editions, five members, every work carrying a volume number — so
`import::scanned`'s `(series_id, volume)` address probe applies to all of
them, and the work rows are expected to survive events the `content_key` does
not.

Each work was then given a hand-set title through `PATCH
/api/library/works/{slug}` — the real route, driven with a session cookie, not
an `INSERT` — and all five answered `persisted: true` with a document under
`<root>/.taiyaku/`.

### The events, and what each did to the address

| work | event | `content_key` | document |
| ---- | ----- | ------------- | -------- |
| 1 | repacked the archive, same pages | `a39071e5` → `a1d0626c` | orphaned |
| 2 | one page added | `8e600729` → `ece7d7ec` | orphaned |
| 3 | file renamed | unchanged | reachable |
| 4 | untouched | unchanged | reachable |
| 5 | edited while the root was read-only | unchanged | **stale** |

One rescan reported `1 added, 3 updated, 1 gone` and `2 rekeyed`, with all
five work rows standing. The two moved keys left two files under `.taiyaku`
that no work addresses any more, and two works whose address holds no file —
O81's orphan and O82's gap, one of each per event.

**The controls hold.** A rename moved nothing: work 3 kept `909e7fc7` and its
document stayed reachable, which is what the entry claims and what
`content_key` hashing member digests rather than paths predicts.

### The rebuild is the measurement

A rescan alone loses nothing. `title_hand` survived every event above, because
migration 011's `_hand` columns hold it and `derive` writes `COALESCE(<hand>,
<derived>)`. What the stranding costs is only visible by doing what the
durability claim is about — deleting the database and scanning the same root
again:

| work | live database | rebuilt from the same root |
| ---- | ------------- | -------------------------- |
| 1 | `HAND …v01-cbz` | `Go-Toubun no Hanayome - v01.cbz` |
| 2 | `HAND …v03-cbz` | `Go-Toubun no Hanayome - v03.cbz` |
| 3 | `HAND …v08-cbz` | `HAND …v08-cbz` |
| 4 | `HAND …kaguya…` | `HAND …kaguya…` |
| 5 | `READONLY EDIT kobayashi` | `HAND …kobayashi…` |

Works 1 and 2 come back as their derived filenames with `title_hand` NULL, the
orphaned documents sitting in `.taiyaku` holding the corrections nothing reads.
Work 5 comes back **at its previous correction**, not at the one the live
database holds.

### What O81's entry gets wrong

The entry reads *"derivation simply re-derives over the work"*. It does not,
and has not since O80 closed on 2026-08-30: the four columns migration 011
gave `_hand` copies to are re-applied by `COALESCE` on every pass whether or
not the document is reachable, and `title_hand` came through all three
key-moving events intact. **The passage is left in place and marked**, because
the wrong reasoning is why the item was filed as the narrow remainder.

What is actually lost is the durability half alone — which is smaller than the
entry claims and larger than "silent": 2 of 5 works here, from repacking an
archive and adding a page to one.

### The state neither entry names: a document that disagrees

O82 describes a work whose marker is set and whose document is **absent**.
Work 5 produced a third state instead. The edit was made while the root was
read-only, so `hand::save` answered honestly —

```
persisted: false
why: […]/3086fd50….library.json: staging a temporary beside it:
     Permission denied (os error 13)
```

— and the file at that address **still existed**, holding the correction from
before. So there is no gap to find and no orphan to find: a reconciliation
pass counting only presence calls this work reachable and moves on, and the
rebuild above reads the older title back as though nothing had happened. A
silent revert to a superseded correction is a worse outcome than the absence
the entry describes, because it looks like it worked.

**Nothing retries.** Write permission was restored and a full scan and derive
run over the root; the document still held the superseded title afterwards.
That is the entry's own claim, driven end to end rather than read off the code.

### One query answers both directions

The marker table is the register of works that owe a document:

```sql
SELECT work_id FROM migrated_document
 WHERE document = 'library' AND absorbed = 1
```

Resolved against `hand::path_for`'s address and the files actually under each
root's `.taiyaku`, one listing gives all three states — a marked work with no
file (O82's gap), a file matching no work's `content_key` (O81's orphan), and,
with the document's own `updated` compared against the row, the stale case
above. There is no second pass to write: the two items are one reconciliation.

### The corpus, untouched

`find corpus -newermt` against a timestamp taken before any of this returned
nothing. The archives were copies, so the isolation held by construction
rather than by care.

### Reconciliation's own cost, measured

The whole-branch reviewer measured what design §6 had only asserted: that
"one `read_dir` per root is nothing beside hashing archives." It is not —
`reconcile` also pays `hand::path_for` for every marked work, run once per
root (the `owed` query is global rather than root-scoped, so this is roots ×
works point lookups, not works alone), plus a file read, a JSON parse and a
full `librarydoc::of` projection for every present, addressed document — the
stale check. All of it runs unconditionally on every `derive`, unlike the
read-back beside it, which `!absorbed` gates.

**2000 works across 4 roots cost 105 ms; 5000 works across 10 roots cost
573 ms** — release build, in-memory database, tmpfs. Neither figure
includes archive hashing, which `derive` pays separately and which this
measurement did not isolate.

It is not an N+1 inside an N+1 the way O85's own was: this is roots × works
point lookups against SQLite, flat rather than compounding, and on a NAS the
file reads it pays per present document — not the SQL — are expected to
dominate instead.

Design §6 is corrected against this measurement rather than left to say
"nothing beside hashing archives."

### After the fix, driven again on real archives

`hand::rename_for_rekey` is this branch's prevention half. It was driven on
the same two events as the measurement above, to confirm the files-changing
cause is closed rather than argued closed.

Three `.cbz` files out of `corpus/` into one root under `target/` —
**copied rather than hardlinked**, since two of the three get rewritten:
`gotoubun-v01/raw-mono`, `gotoubun-v03/raw-mono`, `kobayashi-v03/en-mono`.
The scan derived 2 series, 3 works, 3 editions, 3 members.

Each work was given a hand-set title through `PATCH
/api/library/works/{slug}` with a session cookie — the real route, not an
`INSERT`. All three answered `persisted: true`.

`Go-Toubun no Hanayome - v01.cbz` was repacked, same pages, `ZIP_STORED`;
`- v03.cbz` gained one page. Both moved their member digest: `86c82cea…` →
`43d0d892…`, and `641a19a8…` → `a6ccb859…`.

The rescan reported `0 added, 3 updated, 0 gone` and `2 rekeyed`, **with no
orphan clause in the summary at all**, and exactly three documents under
`.taiyaku/`, each sitting at its work's new key — the old addresses hold
nothing. The documents were moved, not copied.

Rebuilding the database from the same root returned **all three**
corrections. The identical shape, run the morning before this fix, cost two
of five (above). That is the whole difference this branch makes, measured
rather than argued.

`find corpus -newermt` against a timestamp taken before any of this returned
nothing.

**What this does and does not prove.** It exercises the rekey path end to
end, on real archives, with a repack and an added page both landing. It does
not exercise the fold path — `fold::absorb`'s stranding is untouched by this
branch — and it does not exercise the read-only-root retry that O82's stale
case still lacks.

## The scanner against a real library — 2026-09-01

Everything before this ran against `corpus/`, which was assembled to test
things. This is the first time derivation met a library somebody actually
keeps: 128 series, 1915 archives, 490 GB, copied local off an NFS share.
Two defects came out of it that no corpus produced, and one cost measurement
that closes a question the reconciliation design left open.

### The library, and how it was run

`/mnt/daveynet/nfs/media 1/Manga/Volumes` copied to local disk with `rsync
-rlt` — deliberately not `-a`, because the source is `root:root 755` and the
copy has to be writable to exercise anything. 1915 archives in, 1915 archives
out. One root, no `edition_key`, which is what a person configuring this for
the first time would do.

Scanned and derived to **130 series, 1858 works, 1858 editions, 1883
members**, with `1 unattached chapter directories, 13 unreadable, 25 merged, 1
series holding no work` and 29 proposals raised.

### Two releases of one volume, merged into one edition

**25 works across three series hold two members each, and the two members are
different releases of the same volume.** `[Oshi no Ko]` 12, `Death Note` 9,
`Yu Yu Hakusho` 4. A reader opening `[Oshi no Ko]` volume 1 gets the fan
translation followed by the official 2023 release as one page sequence.

The mechanism is exact, and it is about where the distinguishing text sits:

```
separate series:
  Monster - The Perfect Edition/Monster - The Perfect Edition - v01.cbz
merged into one edition:
  [Oshi no Ko] - Fan Translated/[Oshi no Ko] - v01.cbz
  [Oshi no Ko]/[Oshi no Ko] - v01 (2023).cbz
  Death Note (Colored Council)/Death Note - v01 (Colored Council).cbz
  Death Note/Death Note - v01 (2005).cbz
```

Series resolves from the text **before the volume marker**. `Monster - The
Perfect Edition` carries its distinction there and survives as its own series,
which is why `proposals::raise` then offers it as an `edition-of` — the
machinery works, on the cases that reach it. A distinction living only in the
directory name, or in a parenthetical *after* the marker, never reaches it:
both files resolve to one series, `(series, volume)` groups them, and
`import::scanned` writes them as two members of one edition.

**The count exists and the attribution does not.** `DeriveReport::merged` is
the field built for exactly this — "entries that joined an edition another
entry had already opened, which is right for the chapters of a volume and
wrong for anything else" — and it reported 25. What it cannot say is *which*
25, or that their members came from different directories, which is the half a
person would need to act. That is O89.

### Twelve volumes that cannot be opened

Twelve files named `.cbz` are RAR archives. `taiyaku_read::sniff` gets this
right from the first 512 bytes and records `scan_entry.format = 'rar'` — the
half of `docs/sidecar.md`'s "a zip named `.cbr` and a RAR named `.cbz` are both
routine" that works. `derive::is_readable` then admits only `Dir` and `Zip`, so
all twelve are counted in `DeriveReport::unreadable` and never become members.
One series, invisible. That is O90.

The thirteenth `unreadable` is the counter's other site: a member whose bytes
could not be hashed, not a format that cannot be read. One field, two causes.

### What a derive costs when there is nothing left to hash

**8.4 seconds**, wall clock, for 1858 works and 1883 members over 130 series —
walk, derive, proposals and `hand::reconcile` together, on local NVMe.

That closes the question §6 of the reconciliation design left open. The
concern was that `reconcile` is paid unconditionally on every derive where the
read-back beside it is gated on `!absorbed`. At real scale the whole derive is
8.4 s and `reconcile` is not visible in it. The synthetic figure taken on
2026-08-31 — 105 ms at 2000 works and 4 roots, in-memory SQLite on tmpfs — was
the right order and pessimistic about the surrounding cost, not optimistic.

Two measurement hazards found on the way, both worth not repeating:

- **The debug binary hashes about fourteen times slower.** 0.21 archives/sec
  against 2.9 in release, measured on the same files: 141 archives in eleven
  minutes, then 130 in forty-five seconds after `cargo build --release`.
  sha256 is what derivation spends its time on, and it is exactly the code an
  unoptimised build ruins. Any timing this project has taken with
  `target/debug/taiyaku` is an order of magnitude pessimistic on hashing.
- **An interrupted derive resumes.** The first run was killed mid-hash; the
  141 digests it had written to `scan_entry` were still there and the restart
  picked up from 141 rather than from zero. That is `derive` opening no file
  except to hash a member nothing has hashed yet, working as designed — and it
  means a first scan of a large library is safely interruptible, which nothing
  had checked.

### The walk over NFS, which is why the library was copied

Scanning the share in place, `--no-derive`, no hashing at all: **2 hours 50
minutes** for 6003 entries, at **0% CPU**. About 1.7 seconds per entry, all of
it round-trip latency. The same walk over the local copy is seconds, and
`rsync` pulled the 490 GB off the same mount at ~105 MB/s — so the link is
fine and it is per-file metadata that is not.

Not tracked as an item: the deployment target is a server with the library
local to it, so this measures the mount rather than the product. Recorded
because the number is surprising and somebody will otherwise re-derive it.

### One thing that worked, and one that will not on a read-only share

`proposals::raise` offered **29** suggestions on first contact, and the
scoring separated them correctly: `Full Color Edition`, `The Perfect Edition`
and `Digital Colored Comics` scored 2.0 against the edition vocabulary, while
`Gaiden`, `Side Story` and `Anthology Comic` scored 0.0 — the second group
being different works rather than editions, which is the judgement the score
exists to make.

The share itself is mounted `rw` but its directories are `root:root 755`
against uid 1000, so `watch::probe` was refused, named the refusal, and fell
back from 24-hour to 15-minute polling rather than failing the scan —
`watch.rs`'s "a write that fails is not an error return" under a condition no
test stages. On a library mounted that way `<root>/.taiyaku/` can never be
created and every correction answers `persisted: false`, which is O82's
read-only case as the ordinary case rather than the corner one. Not tracked
here either: the server that will run this owns its storage.

## What a *save* costs on a real library — 2026-09-05

The section below priced the preview and nothing priced the save, because
until C4 PR B there was no save: every rules route wrote the `rule` table and
returned. `preview::apply` is that missing half — the same derivation, without
the rollback and under `Documents::Reconcile` rather than `Skip`.

Measured through `apply_cost_on_a_real_library`, an `#[ignore]`d test beside
the preview's own and pointed at the same `TAIYAKU_PREVIEW_DB`, on a `--release`
build against a **copy** of the same library the section below used — 130
series, 1858 works, 2044 scan entries, 4 rules, and a root with
`keeps_documents` **off**.

| | run 1 | run 2 |
| --- | --- | --- |
| `preview`, unchanged list | 287.1 ms | 308.4 ms |
| `apply`, unchanged list | 280.7 ms | 330.6 ms |
| `apply` again, unchanged list | 277.2 ms | 303.3 ms |

**A save costs what a preview costs.** The two straddle each other across the
runs, which is the finding: committing instead of rolling back is not where
the time goes, and neither is the reconcile pass. The derivation is the whole
cost, exactly as the preview's own measurement concluded — so the screen's
Save button needs no different treatment from its preview, and the ~281 ms the
editor was designed around is the right figure for both.

**Single-shot per run, not a median of ten** like the preview measurement
beside it, and the second run sat under a concurrently building workspace.
That is why two runs are tabled rather than one number: the honest claim is
"the same, within noise", not a figure to three digits.

**Two things this library could not measure.** It holds **0 accepted
proposals** (29 open), so the reopen path — the property
`a_save_of_the_unchanged_list_reopens_nothing` exists for, and the one an
import breaks — is asserted here against a library that cannot exercise it;
the fixtures are what cover it. And its root has `keeps_documents` off, so the
`Documents::Reconcile` pass had no document to write: what a save costs on a
root that *keeps* documents is unmeasured, and it is the only case where a
save should cost more than a preview.

**The impact agreed exactly**, both runs, and both saves reopened nothing:
`Impact { series_before: 129, series_after: 129, series_merged: 0,
works_rehomed: 0, works_orphaned: 0, proposals_reopened: 0,
proposals_raised: 0 }`. That the preview and the save agree is pinned on
fixtures by `a_preview_and_a_real_save_produce_the_same_structure`, where a
fixture has three works; this is the same claim where 1858 works give a
disagreement somewhere to hide.

*(129 series against the 130 the table above counts: `Impact` counts distinct
series **names** reached by a member, and one series in this library holds no
member the grouping query returns.)*

## What a rule preview costs on a real library — 2026-09-03

Spec C4 §11 marks its mockup's "34 ms" as **an illustration and not a
measurement**, and says so honestly: when that spec was written there was no
store, no scanner and no library to re-derive. This is the measurement it
asked for, and it changes one thing about the screen.

Measured on the local copy of the real library — **130 series, 1858 works,
1858 editions, 1883 members, 2044 scan entries, 4 rules** — with a `--release`
build, ten runs each, through `preview::preview`. The harness is
`preview_cost_on_a_real_library`, an `#[ignore]`d test pointed at a database
by `TAIYAKU_PREVIEW_DB`, the same shape the corpus tests use for
`TAIYAKU_CORPUS`. It was run against a **copy**: the preview writes nothing,
but that is not a reason to point a new code path at the only real library on
this machine.

| what | median | best | worst |
| --- | --- | --- | --- |
| preview of the library's own unchanged rule list | **281 ms** | 279 ms | 286 ms |
| preview of that list with one rule appended | **495 ms** | 489 ms | 510 ms |
| `grouping` alone, 1883 members | **1.3 ms** | — | — |

**The diff is free and the derivation is the whole cost.** `grouping` is 1.3
ms of the 281, so the remaining ~280 ms is `replace_all`, `derive_with` and
the rollback — that is, re-deriving 1858 works. Nothing here is worth
optimising by making the diff cheaper.

**So the preview is not affordable on every keystroke, and C4 §6's
"debounced" is not enough.** §6 says the route "writes nothing and is safe to
call on every keystroke, debounced". Safe, yes. Affordable, no: at 281 ms a
keystroke-driven preview is one derivation per typed character on a 130-series
library, and this library is *small* against the 156 series C4 §1's mockup
assumed and far smaller than the thousands §11 names as the unknown case.

**The screen previews on demand.** C4 §11 predicted this exact outcome and
pre-approved the remedy — *"if it is not, the fix is to debounce harder or
preview on demand, and neither changes this design"* — so this is a settled
question rather than a new one. The rule editor gets an explicit control
rather than a keystroke hook. Reordering by drag stays a preview trigger,
since a drag is one gesture rather than a stream of them.

**The second row is also the first evidence the mechanism works on real
data.** The appended rule is a bare `(?<series>.+)` capture over the whole
path, which is the degenerate case: it gives all but one work its own series,
and the preview reports `series_after: 1881, works_rehomed: 1880,
proposals_raised: 343` — 343 new proposals a person would face after saving.
That is the Impact tab earning its place. A screen that let someone save that
rule and *then* discover 343 proposals is the screen this one replaces.

**The write lock is held for the duration.** `preview` opens `BEGIN
IMMEDIATE` and holds it across the derivation, so a preview blocks any writer
for 0.3–0.5 s on this library — including a scan on the second connection
`Library::working` hands out. Not a defect and not tracked as an item: the
alternative is deriving outside a transaction, which is the thing the whole
design exists to avoid. Recorded because a person previewing rules while a
scan runs is an ordinary thing to do, and this is the number that says what
happens.

**Not measured:** a library of thousands of series, which §11 names and
nothing here answers; and the cost with override detection running alongside,
which is quadratic in rules and was measured on four. *(It was called conflict
detection until O95 closed on 2026-09-05; nothing about the cost changed.)*

## What a `VACUUM INTO` of the real library costs — 2026-09-04

The C4 PR 2 build design §4 left this unmeasured and said so, and §10 named it
as the one figure that PR could produce and had not. It decides whether
`POST /api/library/backups` can stay synchronous. It can, by two orders of
magnitude.

Measured on a **copy** of the real library — **130 series, 1858 works, 1883
members, 2044 scan entries**, 3,534,848 bytes — through the shipped command,
`taiyaku library backup --db <copy>`, `--release` build, seven runs:

```
run 1: 7 ms      run 5: 6 ms
run 2: 6 ms      run 6: 6 ms
run 3: 6 ms      run 7: 6 ms
run 4: 6 ms
```

**Median 6 ms**, and that figure is an **upper bound on the `VACUUM INTO`
itself**: it is whole-process wall clock, so it includes `exec`, opening the
database, the migration check `Library::open` runs, and the `strftime` that
mints the name. The statement is some fraction of six milliseconds and this
measurement cannot say which fraction — it does not need to, because the
question it was taken to answer is whether a route can afford to answer when
the copy is done, and at 6 ms the answer is not close.

For scale, the same library's **rule preview costs a median of 281 ms**
(2026-09-03, above). A backup is roughly **one fiftieth** of a preview the
screen already runs on demand.

**The backup is smaller than the database**: 3,338,240 bytes against
3,534,848, because `VACUUM INTO` writes a defragmented copy. That is a
property of the statement rather than a loss — the copy restores whole:
`sqlite3 <backup> "select count(*) from work, series, member"` answers
**1858 / 130 / 1883**, the same as the source.

**Isolation, proven rather than asserted** — the discipline the preview
measurement above records for the same reason. The command was pointed at a
copy in the scratch directory, and afterwards `cmp` reported the copy
**byte-identical to the source**, so nothing was written to either. The source
directory listing is unchanged. `VACUUM INTO` is a read of the source and a
write of a new file, and this is the evidence for that rather than the
assumption.

**Not measured:** what this costs on a library of thousands of series, which
is the same gap the preview measurement leaves; and whether a second
connection's `VACUUM INTO` is as invisible to the server's readers as WAL
promises, which the build design §10 names and which wants the process under
load rather than a command run seven times. Both are still open.

## What the roots screen's own scan costs, driven end to end — 2026-09-04

Spec C4's roots screen (PR A) is the first thing that can start a scan from a
browser, and the build design's §10 named the cost of one as a gap both spec B
and C4's server half carry. This is the first figure.

**Setup.** A copy of the real library's `library.db` (1858 works, 130 series,
one root at `/home/davey/temp/manga-test/Volumes`, 490 GB, never scanned by any
serving process — `last_scan_at` and `last_scan_status` both `NULL`), served by
a debug build on `127.0.0.1:8137`. The scanned root is **not** that one: it is a
scratch root of three four-page CBZs written into the session scratchpad, so the
walk is trivial and what the number measures is the derivation behind it. The
real tree was read and never written, and the proof is below.

**The figure: `elapsed_secs` 1.028**, from `POST /api/library/roots/2/scan` to
the poll that first read `running: false`. Wall clock across the screen's own
1 s polling loop was 1.05 s, so the poll interval is the resolution here rather
than the measurement. The walk itself is 2 directories and 3 files; the other
second is `derive::derive`, which is library-wide rather than per-root and so
re-derived all 1861 works. **That is the shape worth carrying: a scan's cost is
its root's walk plus a whole-library derivation, so scanning the smallest root
in a large library is not cheap.** A 1 s poll is therefore not obviously wrong,
and is still a guess: nothing has scanned the 490 GB root.

**What the report actually carried.** 8 fields from `ScanReport` and 29 from
`DeriveReport` — **37**, which is the number `ScanReport.test.tsx` asserts, now
confirmed against the wire rather than against arithmetic over the two structs.
Non-zero among the fields the screen draws *first*, the ones naming what the
scan did not do: `unreadable: 13`, `unattached_chapter_dirs: 1`,
`series_holding_no_work: 1`. The 13 are the real library's — O90's RAR-named
`.cbz` volumes and one member whose bytes could not be hashed — so the
confession-first ordering had something real to confess on its first run.

`last_scan_at` and `last_scan_status` were written for the first time by a
serving process: `2026-09-04 20:38:42` / `ok`.

**The three validations, each refusing with its own sentence:**

```
POST /roots {"path":"…/Volumes/Akame ga KILL!"}
  400  is inside existing root /home/davey/temp/manga-test/Volumes (id 1)
POST /roots {"path":"/home/davey/temp"}
  400  contains existing root /home/davey/temp/manga-test/Volumes (id 1)
POST /roots {"path":"/nope/not/here"}
  400  No such file or directory (os error 2)
POST /roots {"path":"…/scratch-root"}
  200  watching: true, watch_reason: null, interval_secs: 86400
```

Both nesting directions, and the probe answered on the *success* — which is the
only one of the three that produces anything when nothing is wrong, and is what
lets the screen state the regime before the first scan.

**The regime line for the real, never-scanned root** renders as
`watch state unknown — probed when a root is added · no scan interval set ·
never scanned` — no `null`, no `undefined`, which is the case the row that had
never been scanned would otherwise have shown three of.

**Single-flight held.** A second `POST` while the first was running answered
**409**, naming the root and its path, rather than queuing a second walk.

**`?dryrun=true` is still refused.** 400, `unknown field 'dryrun', expected
'dry_run'` — the `deny_unknown_fields` fix from PR 1's whole-branch review,
re-confirmed from the client side, since a client that misspells that key runs a
committed removal. `config.ts`'s own test pins the exact string for this reason.

**Removal quoted the same counts it then charged.** Dry run
`{works: 1, positions: 0, notes: 0}` with the work count unchanged at 1859
afterwards; the real `DELETE` reported the identical three and left 1858 — the
library's original count exactly.

**Proof the real tree was never written.** `find … -printf '%p %s\n' | sort |
sha256sum` over `/home/davey/temp/manga-test`, before and after the whole
session: `75c20e25f913ec55ecde79173dbb6f8e5a89a6baf1fc5258337e23563430fb13`
both times, and `find … -name .taiyaku` finds none. The root's
`keeps_documents` is `0`, so nothing should have been written under it, and
this is the measurement rather than the assumption.
