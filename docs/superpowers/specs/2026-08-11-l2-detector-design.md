# The detector, redesigned against measured ground truth — design

**Status: settled in brainstorming on 2026-08-11. Not yet built.**

Written in the style of the rest of `docs/`: every decision records the
alternative it beat and what it cost, a figure asserted here was measured on
this machine rather than recalled, and a guess says it is one.

This is **sub-project B of two**. A built the instrument that can judge a
detector — `taiyaku score`, 480 hand-drawn boxes, five outcomes per box — and
shipped on 2026-08-11. B is the detector, and it is the first thing this
project has built that A can measure.

It is expected to close **O31, O32, O33 and O34**, which `status.md` holds
together rather than picking off precisely because they are all the detector's
and all wait on this.

## Where it came from

`taiyaku score` over the 45 dev pages returns **2 one-to-one matches out of 478
scored boxes — 0.4% recall.** `split` and `fragment` are both zero, so the
detector is not finding text and cutting it wrongly; it is not finding it.
`spurious` is 3 across 45 pages, so it barely proposes anything at all.

The measured cause, from `notes.md`: `stroke_scale` is the median longer side
over *every* ink component on the page, and 63% to 96% of those components are
under 5 pixels — screentone dots, JPEG ringing, antialiasing fragments. The
scale collapses to 1–3px, the smear gap to 4–6px, and screentone packs densely
enough that a few pixels of tolerance lets `merge_along`'s transitive passes
weld thousands of specks into one component spanning most of the page, which
`MAX_BLOCK_AREA` then discards.

**Two candidates have already been refused, and they share a blind spot.** L2A
Task 9 measured `above-floor` (median over components above a noise floor) and
`modal` (the mode of the size distribution) against a bar fixed before either
was run. Both were refused. But both changed **which components the median was
taken over**; neither changed **which components were smeared**. Screentone
stayed in the smear input both times, so the mechanism the diagnosis names was
never touched. That is the gap this design goes through.

### What is new, and why it changes the approach

Sub-project A was built to score candidates. It turns out to do something the
design did not claim: **the 480 boxes can measure the constants, not merely
judge the output.** Every constant in `detect.rs` says "a starting value, not a
measurement", and until 2026-08-11 there was no material against which it could
become one. Now there is. The components *inside* a truth box are the glyph
population, measured; the ones outside are the reject population. A size band,
an ink floor and a smear gap can each be read off two real distributions.

This is why the plan opens with a measurement phase rather than a candidate.

## Measurements taken on this machine while settling this design

`python3` over the four `taiyaku.truth.json` documents, 2026-08-11:

| quantity | value |
| --- | --- |
| pages labelled | 45, across 9 editions of 4 works |
| pages with at least one box | 45 of 45 |
| boxes | 480 (478 scored, 2 `ignore`) |
| boxes per page | 10.7 |
| kinds | dialogue 390, sfx 63, other 19, credits 3, logo 3, narration 2 |
| axis | horizontal 266, vertical 211, none 3 |
| box area, page fraction | p10 0.0031, median 0.0113, p90 0.0249, max 0.3503 |
| box height, page fraction | p10 0.043, median 0.095, p90 0.157 |
| box width, page fraction | p10 0.060, median 0.116, p90 0.190 |

`./target/release/taiyaku score --sidecar corpus/<work>/taiyaku.json`, same day,
release build, **all nine editions**:

| edition | 1:1 | merged | missed | spurious |
| --- | --- | --- | --- | --- |
| gotoubun-v01 en-fan | 0 | 0 | 39 | 1 (0.20/page) |
| gotoubun-v01 en-mono | 0 | 12 | 41 | 1 (0.20/page) |
| gotoubun-v01 raw-mono | 0 | 2 | 42 | 1 (0.20/page) |
| kaguya-v01 en-mono | 0 | 5 | 62 | 0 |
| kaguya-v01 raw-mono | 0 | 4 | 53 | 0 |
| kobayashi-v01 en-mono | 0 | 13 | 51 | 0 |
| kobayashi-v01 raw-mono | 0 | 4 | 51 | 0 |
| oshinoko-v01 en-mono | **2** | 0 | 41 | 0 |
| oshinoko-v01 raw-mono | 0 | 7 | 49 | 0 |
| **total** | **2** | **47** | **429** | **3** |

**The totals are `status.md`'s recorded table exactly** — 2, 47, 0 split, 0
fragment, 429, 3 — over 478 scored boxes. That equality is the check this run
was for: **the instrument reproduces its own baseline on this machine**, and
nothing below is designed against a number that exists only in a document.

*One operational note, recorded because the first attempt got it wrong.* The
three loose-page `raw-mono` editions refuse a shell glob over their contents —
`score corpus/kaguya-v01/raw-mono/*` reports "no edition is made of these 213
members" — because **a directory of loose images is one member, not many**, the
rule `corpus::members` already states. `score corpus/kaguya-v01/raw-mono`, the
directory itself, is the correct invocation and is what produced the rows above.
Not a defect and nothing for the plan to fix; a trap for whoever runs it next.

### One hypothesis ruled out cheaply

`MAX_BLOCK_AREA` at 0.15 of the page is **not** costing recall. Exactly **1 of
478** scored truth boxes exceeds it — a full-page sound effect on
`kobayashi-v01/raw-mono` page 65, at 0.35. A ceiling that excludes 0.2% of the
target is not the reason 99.6% is missed. The floor and the population are
wrong; the ceiling is fine.

This is worth stating because "the backstop is eating the answer" is the
natural reading of the diagnosis — `MAX_BLOCK_AREA` *does* discard the welded
blob — and it is the wrong lesson to draw. It discards a page-spanning weld
that was never text. It does not discard text.

## 1. The promotion rule, corrected before any candidate exists

Sub-project A pre-registered:

> A candidate replaces the incumbent only if, over the dev set, it increases the
> count of 1:1 matches and does not increase spurious-per-page. Ties go to the
> incumbent.

**Term 2 as written cannot be satisfied by any real detector, and this was found
by arithmetic rather than by a candidate.** The incumbent scores 0.07
spurious-per-page — 3 boxes across 45 pages — *because it proposes almost
nothing*. A candidate proposing a realistic 10 per page makes roughly 480
predictions, of which at most 3 may miss every truth box. With only 2 `ignore`
boxes on the whole dev set to absorb a near-miss, that is **99.4% precision
demanded of a first candidate**. Term 2 rewards silence, and the silent detector
is the incumbent.

The spec's own stated reason for making term 2 a rate was "a candidate may
legitimately return more predictions in total". A rate **per page** does not
deliver that: it holds the absolute count of junk fixed however many predictions
are made. The correction follows from the spec's own reasoning rather than from
any candidate's numbers.

**Anchoring term 2 to the incumbent at all has the same defect, whatever the
denominator.** Its precision is high for exactly the reason its spurious count
is low. So the corrected term is not relative to the incumbent:

> **A candidate replaces the incumbent only if, over the dev set, it increases
> the count of 1:1 matches, and its spurious-per-page does not exceed the dev
> set's own truth-boxes-per-page (10.7). Ties go to the incumbent.**

Term 1 is unchanged and is sound: from a base of 2 it is honest, cheap and
unambiguous.

Term 2 is now anchored to a **measured property of the labelled material** — the
detector may not emit more junk than there is text — rather than to a guess or
to the incumbent's silence. It blocks the one degenerate strategy that matters,
flooding the page to collect one-to-one matches by accident, and blocks nothing
else.

**The alternative it beat: a precision floor at 50%.** Tighter, and a better
proxy for what a reader will tolerate. It loses because 0.5 is a guess wearing a
round number, and this project has now been burned three times by precisely
that — `COVERAGE` itself is still provisional at 0.5 and still unmeasured. A
precision bar gets written **once the first candidate's distribution exists**,
which is the same promise §8 of the instrument design already made about the
absolute bar, kept the same way.

**Why changing a pre-registered rule is legitimate here, and when it stops
being.** The rule was fixed early so it could not be tuned to flatter a
candidate. No candidate exists. This correction is made from the incumbent's
numbers and the label counts alone, both of which predate it, and it is recorded
in the instrument spec as a dated correction rather than an edit. **After the
first candidate is scored, this rule is frozen.** A change made then would be
the thing the pre-registration exists to prevent, and it does not get made.

## 2. Phase 1 — measure what text is made of

**Deliverable: two distributions and a table in `notes.md`.** No change to
`detect.rs` — phase 1 produces the numbers, and phase 2 is what writes them into
constants.

A new `#[cfg(test)]` module in `taiyaku-cli`, gated on `TAIYAKU_CORPUS`,
following `region_experiment.rs`'s established shape: skips cleanly when the
variable is unset, is not compiled into the binary, calls `detect.rs`'s own
`otsu` and `components` rather than copies of them, and **asserts only that it
measured something** — never a quality figure. A measurement that fails when the
thing it measures improves is a check pretending to be a measurement.

For each of the 45 dev pages: decode, threshold, label components, then
partition the components by whether they fall inside a scored truth box. It
reports, for the inside population and the outside population separately:

1. **Longer side**, in pixels and as a fraction of page height. This is the band.
2. **Components per truth box** — what `MIN_STROKES` should be, measured.
3. **True ink ratio inside a truth box**, counting ink pixels rather than
   summing bounding boxes. This is what `MIN_INK` was meant to be and never was:
   **O34** records 61 of 76 surviving blocks scoring above 1.0 against a floor of
   0.05, because overlapping stroke boxes are summed. Measuring the real
   quantity is what closes it.
4. **Nearest-neighbour gap between components within one truth box**, in units of
   the inside population's median longer side. That quantity *is* `SMEAR_K`.
5. **Gap between adjacent truth boxes**, same units — the ceiling `SMEAR_K` must
   stay under.

**(4) and (5) together are the load-bearing output.** The smear gap has to exceed
the within-box gap and stay under the between-box gap. If those distributions
overlap heavily, then no single gap separates a balloon from its neighbour and
smearing cannot be made to work by tuning — which is a finding, and it is the
one that would send phase 2 to C2 (balloon interiors) instead.

A membership rule has to be picked, and it is picked here rather than left
implicit: a component belongs to a truth box when its **centre** lies inside it.
The alternative, full containment, is wrong for a glyph clipped by the edge of a
hand-drawn box, and a human drawing 480 boxes clips some.

## 3. Phase 2 — the classical candidate

**Deliverable: a promoted-or-not verdict under the rule in §1, plus the per-kind
recall table `taiyaku score` already prints.**

Three changes to `detect_page`, in this order.

### 3.1 Band-pass the component population

One stage inserted **before** the scale: keep only components whose longer side
falls inside the band phase 1 measured. Then `stroke_scale` runs over the
survivors — which is the population it was always meant to describe — and the
gap is a multiple of a scale that is genuinely measuring type.

Screentone falls under the band; panel borders and large art fall over it. The
transitive weld cannot form, because the specks it welds are gone before
`merge_along` sees them.

**Why this is not a third attempt at the same thing.** `above-floor` and `modal`
changed the scale rule and left the smear input alone. This changes the smear
input. The two failures do not predict this one.

**The risk, stated: glyph-sized art detail survives the band.** Eyes, hatching,
speed-line fragments. That is what `MIN_STROKES`, `MIN_INK` and
`MIN_BLOCK_STROKE_SPAN` exist for, and phase 1 is what lets them be set to
values that reject art without rejecting type. They will be doing real work for
the first time: O34 records `MIN_INK` rejecting **0 blocks on 0 of 90 sampled
pages**.

### 3.2 Smear both axes, union, dedupe

Today `axis_of` decides one axis for the whole page, `merge_along` runs once,
and every region on the page is stamped with that answer. The L2 design already
records this as a deviation it shipped without noting — its own §4 correction of
2026-08-08 — and names the consequence: *a page carrying vertical dialogue
beside a horizontal SFX or narration line cannot report both.*

Instead: smear along both axes independently, union the blocks, drop the ones
contained in a block from the other axis, and let each surviving block carry the
axis that produced it.

**This is a recall mechanism, not tidiness.** The dev set is 266 horizontal
against 211 vertical, and the split is not per-edition — it is per box. A single
global axis is wrong on some part of most mixed pages by construction.

It also makes the `axis` field on a `Region` true for the first time.
`taiyaku_core::Axis`'s own doc justifies storing it on the grounds that "the
clustering already knows it", which the L2 correction records as **not yet true
of the code**. After this, it is.

Cost: two smears per page instead of one, plus a containment pass. `components`
— a flood fill over every pixel — already dominates, and it is unchanged.

The scorer ignores `axis` entirely, so none of this moves the promotion figures.
It is done because L2b needs it and because the field is currently a claim the
code does not support.

### 3.3 Constants that cite measurements

Every constant's doc comment either names the phase 1 measurement behind it or
keeps saying it is a starting value. `detect.rs`'s header already states the
standing rule — *a constant in this file asserted as measured, here or anywhere
else, remains a defect* — and that rule is not relaxed. It is finally
satisfiable.

O32 (`MAX_BLOCK_AREA` clamps every region before the median is taken, so
`median_area` cannot fire) and O34 (`MIN_INK` cannot reject anything) both close
here.

### 3.4 What phase 2 does not do

**It does not build C2.** Balloon-interior detection — flood the white, keep
enclosed regions not touching the page edge that hold a cluster of dark
components, return the bounding box of those components — is a real candidate
and it fails in different places from C1: it is blind to the 63 sound effects
and to anything lettered over art, and strong on the 390 dialogue boxes that are
81% of the dev set.

It is not built speculatively because **"where does it miss" stopped being a
guess on 2026-08-11.** `taiyaku score` prints recall per kind. C1's own miss
table is the evidence that says whether the remaining misses are balloon-shaped,
and building C2 before reading it would be choosing a mechanism for the same
reason the last three bars were chosen.

## 4. Phase 3 — the model candidate

**Deliverable: a head-to-head number — incumbent, C1 and a model, on the same 45
pages under the same rule.**

The stated constraint in `detect.rs` is "none of them a model and none of them a
new dependency". Phase 3 tests that constraint against a measurement instead of
holding it on principle, and it is sequenced last so that the cost of breaking
it is weighed against a classical number that already exists.

### 4.1 A spike settles the runtime, because guessing it here would be a guess

`spike/` exists for this, excluded from the workspace so its dependencies and
lint failures cannot reach the real crates. What the spike establishes, before
any design commits to it:

- **Which model.** A manga-specific detector (comic-text-detector, as used by
  manga-image-translator) against a general text detector (CRAFT, DBNet). The
  manga-specific one is trained on this exact material and emits text blocks
  with the shape wanted; the general ones are smaller and better understood.
- **Which runtime.** `tract` is pure Rust with no native library and fits this
  project's offline stance; `ort` binds Microsoft's ONNX Runtime and supports
  more operators. **Unverified here:** whether `tract` runs the operators a
  YOLO-derived detector needs. That is the first thing the spike answers, and
  the reason the runtime is not named in this document.
- **Licence.** Whether the weights are redistributable under GPL-3.0-or-later.
  **Unverified.** manga-image-translator is GPL-3.0, which would be compatible,
  but the weights' own terms are a separate question and must be read before
  anything is fetched into a build.

**Settled by the spike, 2026-08-12.** All three answers, and one the spike added.
The measurements and the quoted licence text are in `notes.md`,
"comic-text-detector through `tract`: four questions, and the head that answers
them". In short:

- **Which runtime — `tract`, and it was never close.** It parses, builds and runs
  `comictextdetector.pt.onnx` at 1×3×1024×1024. Seventeen distinct operators,
  all supported, opset 11. The reason the worry was misplaced is worth keeping:
  a YOLO export's hard operators come *after* the decoded grid, and this export
  has none of them — there is no `NonMaxSuppression` in the file, because
  upstream suppresses in Python. So `ort` is not needed and the offline stance
  holds. **3.8–4.0 s per page on CPU, flat across page sizes**, since everything
  is letterboxed to 1024 first.
- **Which model — comic-text-detector, and the general detectors were never
  reached.** It emits three heads, and the choice between two of them is the
  design decision this paragraph did not know existed: `seg` (a full-resolution
  text mask) recalls the text but at glyph granularity, while `blk` (the YOLO
  grid) is at the **block** granularity the promotion rule counts. Phase 3 scores
  `blk`. `det`, a DBNet-style line head, is below the level L2 pairs on.
- **Licence — redistributable, with an attribution obligation, and the page has
  two licences on it.** The restrictive "solely for academic purposes" clause a
  reader meets first governs **Manga109**; the training data is **Manga109-s**,
  the 87-volume commercially-approved subset, whose conditions explicitly
  contemplate publishing results "including pre-trained models" provided the use
  of Manga109-s "must be indicated clearly within the published work". That
  attribution is **new work this design did not carry**. Two residual risks are
  recorded in `notes.md` and both concern *distributing* the weights, which
  §4.2's fetch-not-vendor choice already avoids.
- **And the question this section did not ask: does a model find what C1
  cannot?** Over the 20 pages of the four Japanese `raw-mono` editions — the
  population where the promoted classical candidate scores **0 of 167** dialogue
  boxes — `blk` proposes **147 blocks against 167 dialogue boxes**, and on the
  four overlays that were cropped and read they sit on the balloons, one box per
  balloon. This is a count of proposals and a look at pixels, **not** a score:
  nothing here has been through `score_page`, which is phase 3's job and the
  reason phase 3 exists.

### 4.2 How it lands

A `#[cfg(test)]` module gated on `TAIYAKU_CORPUS`, the same shape as phase 1 and
`region_experiment.rs`. It is **not compiled into the binary** and `detect_page`
is untouched. Weights are fetched by a script, gitignored, and checksummed —
never committed.

**The alternative it beat: vendor the weights and wire the model in properly.**
That is fully offline and needs no fetch step, which is a real advantage. It
loses because it puts 30–90 MB in every clone forever and adopts a heavyweight
dependency, a licence obligation and a CI story **before any measurement says
the model earned them**. If the model wins, that number is what justifies all
three, and promoting it is a separate decision made with the evidence in hand.

**The alternative it also beat: leave it in `spike/`.** Zero repo cost and the
same number. It loses because a result nobody can reproduce without rebuilding a
throwaway is exactly the failure `region_experiment.rs` was written to undo.

**Landed 2026-08-12.** `crates/taiyaku-cli/src/model_experiment.rs` exists in
the shape this section describes: `#[cfg(test)]`, gated on `TAIYAKU_CORPUS` and
`TAIYAKU_MODEL`, skipping cleanly when either is unset, not compiled into the
binary, `detect_page` untouched, weights fetched by `scripts/fetch-model.sh`,
checksummed and gitignored. The table and the caveats are in `notes.md`, "The
model candidate, scored against the same 45 pages". In short:

- **The number this phase exists for.** Same 45 pages, same nine editions, same
  478 scored boxes, same `score_page` at `COVERAGE = 0.5`: `model/blk` takes
  **130 one-to-one against the classical candidate's 48**, at **0.07 spurious
  per page against 6.73**. Against the rule fixed on 2026-08-11 — 1:1 must
  increase *and* spurious-per-page must stay at or under 10.7, ties to the
  incumbent — **both terms pass**. That row read 118 until `14cb71c`, which made
  suppression class-agnostic; it was class-aware and faithful to YOLOv5, but
  upstream follows NMS with `group_output(...)` and this port does not, so a
  block whose grid cells disagreed about the winning class reached the scorer
  twice. `missed` and `spurious` did not move, which is the check.
- **On the population this phase was scoped around.** The classical detector's
  0 of 167 dialogue boxes over the four Japanese `raw-mono` editions is 47 of
  167 for the model, and the model wins 1:1 in **every** edition, English
  included, at 0.00 spurious per page on all four Japanese raws.
- **What is worse, recorded beside what is better.** `fragment` rose from 81 to
  137 — a different failure shape, not an absent one — and Sfx recall moved the
  wrong way, 5 of 63 to 2 of 63. The model's 0.07 spurious per page is
  numerically the incumbent's figure and reached from the opposite cause; the
  distinction is drawn in `notes.md` and in `status.md`.
- **Promoting the model into the binary is still a separate decision**, as §9
  says, and this section's fetch-not-vendor choice stands. That decision now
  carries the **Manga109-s attribution obligation** §4.1 records — anything
  shipped carrying these weights must indicate clearly that Manga109-s was used.
  It is new work and it did not exist in this design.
- **`COVERAGE` is still provisional at 0.5 and still unmeasured** (§10), and
  this is the result most sensitive to it so far: `blk` hugs the glyphs where a
  hand-drawn box carries margin, so the model's predictions are systematically
  tighter than the truth, and the boundary that costs is `fragment`'s.
  Predictable in direction, unmeasured in size, and not swept here.
- **The held-out set was not opened** (§5). Every figure is over the dev set.
  `BLK_CONF` and `BLK_IOU` are upstream's defaults, taken rather than fitted, so
  no threshold here was chosen against these 45 pages.

## 5. The held-out set

The 30 sealed pages — six editions across regimes deliberately absent from the
dev set — **open once, at the end**, against whichever candidate is promoted.
Reported whatever they say. If they disagree with the dev set, that disagreement
is the finding and it is written up rather than tuned away.

They are not opened between phases. A held-out set consulted twice is a dev set.

## 6. Files, and what each is responsible for

| file | change | responsible for |
| --- | --- | --- |
| `crates/taiyaku-cli/src/truth_population.rs` | create, `#[cfg(test)]` | phase 1: the two distributions and the table |
| `crates/taiyaku-cli/src/detect.rs` | modify | the band, the two-axis smear, the constants |
| `crates/taiyaku-cli/src/main.rs` | modify | one gated `mod` declaration per new module |
| `spike/` | scratch | which model, which runtime, what licence |
| `crates/taiyaku-cli/src/model_experiment.rs` | create, `#[cfg(test)]` | phase 3: scoring the model on the dev set |
| `scripts/fetch-model.sh` | create | weights, checksummed, gitignored |
| `docs/superpowers/specs/2026-08-11-l2-ground-truth-instrument-design.md` | correct in place | the §1 promotion-rule correction, dated |
| `docs/notes.md` | append | the phase 1 table, the spike's findings |
| `docs/status.md` | update | the new figures, O31–O34 |

`detect.rs` is the only production file that changes, and only in phase 2. Both
experiment modules are `#[cfg(test)]` and gated, so neither reaches the binary
and neither can slow the gate — `region_experiment.rs`'s header records what
happened the last time a gated module ran in the ordinary `cargo test`, and the
plan follows the arrangement that fixed it.

## 7. Testing

**The load-bearing tests are the two that pin the mechanism**, and they are
synthetic, so they run in the ordinary suite with no corpus:

- **A field of screentone must be rejected.** A page of dense sub-glyph specks
  produces no region. Without the band it produces one welded blob; with it,
  nothing. This is the failure the whole design exists to end, and a detector
  that silently stopped rejecting it would look like it worked.
- **A column of glyphs beside a row of glyphs must produce two regions with
  different axes.** This is what §3.2 buys and it is not observable from a count.

Then:

- The **existing `detect.rs` tests are the regression net** and must keep
  passing, with one exception treated as a design decision rather than a fix —
  see §8.
- Phase 1 and phase 3 assert only that they measured something: a non-empty
  sample, every page accounted for, every figure finite. Never a quality bar.
- Corpus scoring stays gated on `TAIYAKU_CORPUS` and skips cleanly when unset.

## 8. The page-fraction band, and the fixture it collides with

If phase 1 finds glyph size is a stable fraction of page height, the band is
expressed as a page fraction — which is correct for the material and directly
contradicts an existing test.

`the_gap_does_not_grow_with_the_page` holds stroke size fixed at 10px while the
page grows from 400×600 to 2000×3000. On the large page those strokes are 0.33%
of page height and a page-fraction band would reject them, so the test fails.

**That fixture is load-bearing and cannot be edited away.** Its own doc comment
explains why it exists: `detection_is_scale_invariant` uniformly upsamples, so
page size and stroke size double together and *any* gap formula linear in either
passes it. `the_gap_does_not_grow_with_the_page` is the only test that catches a
gap tied to page width, which is the bug class that would silently return the
whole page on a large scan.

The tension is real rather than a test needing an update. The band is
page-relative; the gap must stay stroke-relative; the fixture varies the two
independently, which is exactly what makes it valuable and exactly what breaks
under a page-relative band.

**Phase 1 decides which way this goes**, and the decision is recorded either way:

- **Stable across the nine editions** → the band is a page fraction, the fixture
  is restated to scale its strokes with the page, and a **new** test takes over
  what it was protecting: the gap, computed on a fixed stroke population, must
  not change when only the canvas grows.
- **Not stable** → the band is derived per page from the page's own component
  distribution, the fixture survives untouched, and phase 2 is harder. The
  warning here is `above-floor`'s recorded failure: excluding a noise floor does
  not isolate a glyph-sized population, because on most pages there is not a
  second population to isolate.

Neither branch is chosen now. Choosing it now, before the measurement that
settles it, is the habit this whole sub-project exists to break.

## 9. What is deliberately not done

- **C2, balloon interiors.** A real candidate, deferred to evidence. See §3.4.
- **Promoting the model into the binary.** A separate decision, made against the
  number phase 3 produces. See §4.2.
- **OCR.** Unchanged from the instrument design: real character selection needs
  a transformer plus Python/PyTorch or a cloud API. The `text` field stays
  reserved so adding it later costs no relabelling.
- **L2b pairing**, and the peek that depends on it.
- **Relabelling, or labelling more pages.** 45 dev pages is what exists; if it
  proves too small to separate candidates, that is a finding for the write-up,
  not a reason to label mid-experiment.
- **Touching the held-out set before the end.** See §5.

## 10. What could not be measured

- **Whether the band separates type from art at all.** The whole of phase 1. It
  is the premise of C1 and it is unverified — if the inside and outside
  distributions overlap heavily, C1 is refused before it is built, which is the
  cheapest possible outcome and a real one.
- **`SMEAR_K`'s ceiling.** Whether within-box and between-box gaps separate.
  Same phase, same status.
- **Whether `tract` can run a YOLO-derived detector**, and **whether the model
  weights are redistributable under GPL-3.0-or-later.** Both are the spike's,
  both are named as unverified in §4.1, and neither is assumed anywhere in this
  document. — *Both measured on 2026-08-12 and answered yes; see the dated block
  in §4.1. The spike also returned something this list did not think to ask for:
  the model has two usable heads at different granularities, and which one phase
  3 scores is now a settled choice rather than an open one.*
- **`COVERAGE`.** Still provisional at 0.5, still unmeasured, and this design
  does not settle it. Every figure above is at that value, and a candidate that
  wins only at one particular coverage is a result this design would not be able
  to detect. Settling it needs a sweep over the labelled set, which is now
  possible and is not scheduled here.
