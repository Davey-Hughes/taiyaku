# Margin-based refusal — design

**Date:** 2026-07-30
**Status:** design agreed, nothing built

## Summary

The refusal is not trustworthy when side A is much shorter than side B. A handful
of art book pages against a whole volume comes back fully paired, at margins of
zero and below, and nothing in the solver objects.

The fix is a second calibration guard, on margin rather than on counts, placed
beside the separation guard it complements. Both it and the provisional
fragmentation threshold are calibrated from one survey pass over the corpus,
because run lengths fall out of the same alignments that produce the margins.

This is the correctness issue everything else rests on. `status.md` lists it
first for that reason: the identity oracle, duplicate resolution and base
selection are all statements about what taiyaku refuses.

## The defect, precisely

`calibrate_floor` (`crates/taiyaku-core/src/solve.rs:61`) estimates the signal
level as the median of each A page's **best** match. That best is a maximum over
`|B|` candidates, and the maximum of many noise samples stands further above the
noise median the more samples are drawn. So the signal estimate inflates with the
length of B, for reasons that have nothing to do with whether the two editions
correspond.

Four Color Walk 10 pages against One Piece raw volume 105 therefore clear
`MIN_SEPARATION`, and the pair is certified. Coverage cannot rescue it: the DP
inserts gaps in B for free, so A comes back fully covered no matter what.
Measured (`notes.md:1666`):

| Art book page | Matched to | Margin |
| ------------- | ---------- | ------ |
| img001 | v105 p117 | 0.000 |
| img040 | v105 p148 | −0.012 |
| img075 | v105 p171 | 0.020 |
| img110 | v105 p176 | −0.047 |

Two are negative: the page chosen scored *worse* than the runner-up and was taken
only because monotonicity forbade the better one. There is no discrimination here
at all, and the solver reports `0 unmatched slots` as though there were.

**Why margin is the signal that survives this.** A margin is the difference
between two cells in the same row, so it does not inflate with `|B|` the way a row
maximum does. Offering unrelated material more candidates raises its best score
but gives it no reason to beat its own runner-up. That asymmetry is the whole
basis of the guard.

## What changes

### 1. `calibrate_floor` becomes `calibrate`, and returns its evidence

The present signature discards the numbers it computes:

```rust
pub fn calibrate_floor(a: &[PageHash], b: &[PageHash]) -> Option<f32>
```

On a refusal the noise, signal and separation are all known and then thrown away,
which is why there is no way to ask the tool why it refused. Replace it with:

```rust
pub struct Calibration {
    pub noise: f32,   // median of every matrix cell
    pub signal: f32,  // median of each A row's best
    pub margin: f32,  // median of each A row's (best − runner-up)
    pub floor: f32,   // (noise + signal) / 2
}

impl Calibration {
    /// Whether a pairing is worth making at all.
    pub fn accepted(&self) -> bool;
}

/// `None` only when either side has no pages.
pub fn calibrate(a: &[PageHash], b: &[PageHash]) -> Option<Calibration>;
```

`accepted()` requires the separation guard **and** the margin guard. `solve`
calls `calibrate`, refuses when a pair is not accepted, and is otherwise
unchanged. The DP, `compress`, `verdict` and `confidence` do not move.

**Median, not mean, and over all of A.** Chapter dividers legitimately carry low
margins — 0.09 to 0.15 measured, against 0.20 to 0.35 for pages that resemble
nothing else (`notes.md:628`) — so a mean would let a minority of template pages
drag the estimate down. Taking the median over *all* of A rather than over paired
pages preserves the deliberate partial-edition refusal: when three quarters of A
matches nothing, the median margin lands in noise exactly as the median signal
already does, and the pinned test at `solve.rs:465` stays green.

**Cost is nil.** The runner-up is a second running maximum in the loop that
already computes the best. No extra pass over the matrix, which is 100 µs anyway
against seconds of decode.

### 2. `align --stats` prints the calibration

One line before solving and one after, so a shell loop over the corpus produces a
table. Shape only — **the values below are invented to show the format, not
measured**, and the whole point of the survey is that nobody knows the margin
figure yet:

```
stats: a=195 b=187 noise=0.5?? signal=0.8?? sep=0.3?? margin=?.??? floor=0.6?? accepted=yes
shape: runs=5 paired=193 avg_run=38.6 per_100=2.6 verdict=Sound
```

The flag is what makes the survey reproducible by anyone holding the corpus,
which matters because these numbers become the justification for two constants.
It also outlives the survey as a diagnostic: "why did this refuse" is a question
the correction UI will ask.

### 3. The survey

Four groups, run through `--stats` and collected into a table:

| Group | Members | Must |
| ----- | ------- | ---- |
| Confirmed good | Go-Toubun, Kaguya, Kobayashi, Oshi no Ko at v01/v03/v08; Tongari v01 and v09; Monster 2:1; Nausicaä 4:1; the four oplabel all-spread pairs; Chainsaw Man's four scans; the One Piece identity pairs | stay accepted |
| Known bad | `oshinoko-v01/raw-mono-BADCOPY`, Kobayashi Chromatic, One Piece v077 c0 against c1 | stay flagged |
| Needles | Color Walk pages against One Piece v105; one fan chapter against a whole volume | be refused |
| Unrelated | Go-Toubun raw against Kaguya English; the mislabelled Bocchi file against the main series | stay refused |

The exact member list is fixed when the survey script is written and recorded
with the results, since a measurement nobody can re-run is not evidence.

**Kaguya is the canary.** It is the weakest pairing in the corpus by every measure
taken so far — separation around 0.17, `MEDBIT` 57 to 69 — so its median margin
sets the ceiling on how high the guard can be placed. Whether a constant is
admissible at all is a fact about the distance between Kaguya's margin and the art
book's, and that distance is unmeasured today.

### 4. Choosing the threshold, with the rule fixed in advance

Stated before the numbers exist, so the survey cannot be read to taste:

1. If every confirmed-good pair's margin exceeds every needle and unrelated pair's
   margin, **take a constant at three quarters of the lowest good margin**,
   provided that value still exceeds every needle and unrelated margin. Anchored
   to the good cluster rather than placed at the midpoint between clusters,
   because the good side has around twenty samples and the needle side has one, so
   a midpoint would be fitted to whichever side is characterised worst.
2. If the good pairs do not separate absolutely but do separate once normalised by
   their own separation, **take the ratio form** — require the median margin to
   clear a fraction of `signal − noise`.
3. If neither separates, **record that and ship no guard.** This project has
   reverted a plausible fix on measurement before (`notes.md:1121`, the 90th
   percentile) and the honest outcome of a survey has to include failure.

Report the headroom either way: how far the lowest good pair sits above the bar,
and how far the needle sits below it. A guard whose margin of safety is unstated
is a guard nobody can revisit.

### 5. Average run length

Computable from `runs` alone, so `verdict`'s signature does not change. The survey
decides whether it replaces `FRAGMENTED_RUNS_PER_HUNDRED` (`solve.rs:264`, and
admittedly provisional, with the 8-to-22 band unpopulated) or merely joins it.

Replacing is justified only if average run length separates good from bad at least
as cleanly *and* survives the hole that coverage currently gates: a refused pair
is one long gap run, so it scores well on any runs-based measure. The notes'
claim — a genuine section ran 103 slots where noise ran 1.8 to 2.1
(`notes.md:1971`) — gets checked here rather than carried forward on one example.

### 6. Tests

Driven test-first, each watched failing.

- **The needle is refused.** Hashes modelling four A pages with no discrimination
  against a long B must produce no pairings. This is the new behaviour and the
  test that does not pass today.
- **A small A that genuinely matches is still aligned.** The guard must reject
  absence of signal, not shortness. Without this the first test is satisfiable by
  refusing every short A, which would break aligning one chapter's worth of
  material against its own pages.
- **The twenty-seven existing core tests stay green**, in particular
  `a_uniformly_weak_pairing_still_aligns` and
  `a_pair_where_only_part_of_a_matches_is_refused`. They pull in opposite
  directions and are the real constraint on any threshold.
- **The eleven corpus baselines in `corpus/README.md` reproduce**, verified before
  and after.

## Risk, stated plainly

**The failure mode of this change is a false refusal on a legitimate weak
pairing, which is worse than the bug it fixes.** Certifying an art book alignment
misleads whoever went looking for one; refusing Kaguya breaks reading a volume
somebody owns.

The mitigations are that the survey names Kaguya's margin before any constant is
chosen, that the threshold is placed below the good cluster rather than between
the clusters, and that twenty-odd confirmed-good pairs plus eleven documented
baselines form the regression net. Shipping no guard remains an allowed outcome.

**A second, smaller exposure:** with `|A|` very small the median of a few margins
is a fragile statistic. The needle case has four pages. The survey should report
`|A|` alongside the margin so it is visible whether the guard is resting on a
median of four or of two hundred, and any conclusion about tiny editions should be
labelled as resting on one example.

## What this deliberately does not do

- **Art book linking.** Still the wrong problem for this solver — sparse,
  unordered, cross-composition — and it wants nearest-neighbour search with a
  crop-tolerant hash. Refusing to align one is the correct answer, and all this
  change does is make the refusal happen.
- **Un-refuse the partial-edition case.** A single chapter against a volume stays
  refused, by design and by pinned test.
- **Tune `MIN_SEPARATION`.** The survey will produce separation figures for
  everything, which would make tuning it possible for the first time. That is a
  by-product worth recording in `notes.md` and not a change to make in the same
  pass as this one.
