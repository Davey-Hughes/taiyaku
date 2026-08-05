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

A credits page exists in the fan edition and in no other, so it pairs with
nothing. Reading the `raw-mono → en-fan` alignment and asking which of the fan
edition's 235 slots no run ever mentions gives 50 slots in five groups:

| Fan pages | Count | Member | Local | What it is |
| --------- | ----- | ------ | ----- | ---------- |
| 1–42 | 42 | `c000` | 1–42 | the prototype one-shot; the raw volume has no such chapter |
| 95 | 1 | `c001` | 51 of 52 | **donation plea** |
| 168 | 1 | `c003` | 31 of 32 | **donation plea** |
| 198 | 1 | `c004` | 29 of 30 | **donation plea** |
| 225–229 | 5 | `c005`, `c005.5` | 26, 0–3 | chapter end plus a bonus chapter |

Three of the five groups are a **single unpaired page sitting at its member's
final index**, and all three were extracted and looked at: the same
`WE NEED DONATIONS! … lhtranslation.com` card, verbatim. The two long groups are
genuine content the raw volume does not contain — `c000` page 20 was extracted as
a negative control and is ordinary manga.

**So the separating rule is shape, not content:** an isolated unpaired page at a
member boundary is a credits page; an unpaired run spanning most of a member is
material the other edition lacks. Both fall out of `alignments`, which every
sidecar already carries.

**The three cards are visually identical and byte-different** — 175,385, 174,117
and 170,889 bytes, three different SHA-256s, because each chapter re-encoded it.
Exact-hash deduplication finds none of them. **dhash does**, and taiyaku already
has dhash-16 with a corpus behind it — the same property that makes it work
across editions makes it work across a re-encode. `notes.md` already records
distance-zero lookup finding duplicate pages inside one edition; this is that,
put to a use.

**Two independent signals, then**, which is what makes this worth building rather
than guessing at:

1. unpaired against the hub, and isolated at a member boundary
2. near-duplicate of a page in a *sibling member* of the same edition, by dhash

Signal 2 needs no alignment at all, so it also works on an edition with no
counterpart — which matters, because a fan edition is often the only one a
library has.

**Not yet measured, and stated as unknown:** whether the rule holds on other
scanlation groups in the corpus, what happens to groups that put credits on the
*first* page rather than the last, and the false-positive rate against colour
inserts and author afterwords, which are also single unpaired pages near a
boundary. The Go-Toubun fan edition is one group's habits, not the population.

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
