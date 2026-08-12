//! Pixels to text blocks.
//!
//! Five stages, none of them a model and none of them a new dependency:
//! threshold (Otsu), label the connected components, take the page's own stroke
//! scale from them, smear the components into blocks at a gap that is a
//! multiple of that scale, then filter what comes out.
//!
//! The scale is a stage in its own right rather than a detail of the smear,
//! and this header called it four stages until the survey made the distinction
//! matter: **the measured failure is in the scale**, not in the smear it feeds.
//!
//! **Why this is tractable at all without a model:** manga is very nearly
//! bilevel already. That is the same property `notes.md` records from the other
//! direction — the dhash is invariant to text *substitution* and not to text
//! *presence*, because a text cell is dark marks on white at comparable density
//! in either language.
//!
//! **Every constant here is a starting value, not a measurement.** They were
//! meant to be settled by `scripts/region-survey.sh` over the corpus, the way
//! `MAX_CREDIT_RUN` was. The survey has since run, over four of the twelve
//! corpus works, and it did not settle them — not because a value is wrong,
//! but because the failure it found is upstream of all five: see
//! `docs/notes.md`, "The text-block detector, measured against four works, and
//! why it does not work yet", and the reproducible measurement behind that
//! section in `region_experiment.rs`. Each constant still says so where it is
//! defined, and a constant in this file asserted as measured — here or
//! anywhere else — remains a defect.
//!
//! *Corrected on 2026-08-12.* The paragraph above is no longer true of every
//! constant, and the rule it ends with is why it is corrected rather than
//! quietly left standing. Phase 1 of the L2b redesign measured the components
//! inside 478 hand-drawn truth boxes over all 45 labelled pages
//! (`truth_population.rs`; `docs/notes.md`, "What text is made of, measured
//! against 480 hand-drawn boxes"), which makes the rule *satisfiable* rather
//! than relaxing it. As of this commit:
//!
//! - **`MIN_STROKES` and `MIN_INK` cite that measurement.** Neither value
//!   moved — both already sat where phase 1 says they should — so what changed
//!   is the justification, and for `MIN_INK` the quantity it is a floor *on*.
//! - **`MAX_BLOCK_AREA` cites the labels too**, as a ceiling checked against
//!   them rather than derived from them; see its own comment.
//! - **`SMEAR_K` and `MIN_BLOCK_STROKE_SPAN` are still starting values**, and
//!   each says so and says why. Neither is stated in a unit phase 1 measured:
//!   both are multiples of the page's own `stroke_scale`, and phase 1's merge
//!   gaps are in pixels. `SMEAR_K` names those pixel figures anyway, as the
//!   nearest thing to evidence about it that exists.
//! - **`BAND`'s `lo` cites a sweep** — Task 7 scored the detector at eight
//!   values of it over all 45 labelled pages and took the row a rule fixed in
//!   advance selects. Its `hi` was held fixed throughout and is still a
//!   starting value; the constant's own comment separates the two.
//!
//! The rule is unchanged and now has teeth in both directions: a constant here
//! asserted as measured without a distribution named behind it is a defect,
//! and so is one that names a distribution it was not actually read off.

use image::GrayImage;
use taiyaku_core::{Axis, Region};

/// A bounding box in pixels, inclusive on both corners.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bounds {
    pub x0: u32,
    pub y0: u32,
    pub x1: u32,
    pub y1: u32,
}

impl Bounds {
    pub fn width(&self) -> u32 {
        self.x1 - self.x0 + 1
    }

    pub fn height(&self) -> u32 {
        self.y1 - self.y0 + 1
    }

    /// The longer side, without saying which axis it came from.
    ///
    /// Both text directions are ordinary here — Japanese runs vertical and
    /// English typeset runs horizontal — so a filter written in terms of width
    /// and height would have to be written twice and would drift. Only this
    /// side has ever been needed: `MIN_BLOCK_STROKE_SPAN` is the filter that
    /// asks for it, and nothing in this file has wanted the shorter one.
    pub fn longer(&self) -> u32 {
        self.width().max(self.height())
    }

    fn merge(&self, other: &Bounds) -> Bounds {
        Bounds {
            x0: self.x0.min(other.x0),
            y0: self.y0.min(other.y0),
            x1: self.x1.max(other.x1),
            y1: self.y1.max(other.y1),
        }
    }
}

/// A histogram bin index, as `otsu`'s `best_level` result.
///
/// # Panics
///
/// Panics if `level` exceeds `u8::MAX`. It cannot: every call site indexes
/// the fixed-size `histogram: [0u64; 256]` array below via `.enumerate()`,
/// which cannot yield an index past 255 — enforced by the array's declared
/// size, not merely a property of the material.
fn intensity_level(level: usize) -> u8 {
    u8::try_from(level).expect("histogram bin index exceeds u8::MAX")
}

/// Otsu's threshold: the level that best separates the page's two populations.
///
/// Chosen over a fixed level because the corpus spans scans, re-encodes and one
/// archival TIFF set, and a fixed level would read a grey scan as solid ink. It
/// is the standard method, it is a histogram and one pass over 256 bins, and it
/// needs nothing this crate does not already have.
pub fn otsu(page: &GrayImage) -> u8 {
    let mut histogram = [0u64; 256];
    for pixel in page.pixels() {
        histogram[pixel.0[0] as usize] += 1;
    }
    let total: u64 = histogram.iter().sum();
    if total == 0 {
        return 0;
    }
    let weighted: u64 = histogram
        .iter()
        .enumerate()
        .map(|(level, count)| level as u64 * count)
        .sum();

    let (mut below, mut below_weighted) = (0u64, 0u64);
    let (mut best_level, mut best_variance) = (0u8, -1.0f64);
    for (level, &count) in histogram.iter().enumerate() {
        below += count;
        if below == 0 {
            continue;
        }
        let above = total - below;
        if above == 0 {
            break;
        }
        below_weighted += level as u64 * count;
        #[expect(
            clippy::cast_precision_loss,
            reason = "below_weighted and below are u64 counts of pixels on one page \
                      (`total` above sums the 256-bin histogram over `page.pixels()`); \
                      f64 is exact up to 2^53 and no page in this corpus is remotely \
                      that large — stroke_scale's doc comment below records real page \
                      widths from 685 to 5343px. Property of the material, not \
                      something the code bounds. A third route was tried too: \
                      f64::from is lossless for u32/i32/u16/i16/u8/i8, but no such \
                      impl exists for u64 — its 64 bits exceed even f64's 52-bit \
                      mantissa — so this stays a cast, not a conversion."
        )]
        let mean_below = below_weighted as f64 / below as f64;
        #[expect(
            clippy::cast_precision_loss,
            reason = "weighted, below_weighted and above are u64 pixel counts, same \
                      bound as mean_below above: no page here comes near f64's 2^53 \
                      exact-integer limit. Same absence of a third route as mean_below \
                      above: no From<u64> for f64 exists in std."
        )]
        let mean_above = (weighted - below_weighted) as f64 / above as f64;
        #[expect(
            clippy::cast_precision_loss,
            reason = "below and above are u64 pixel counts, same bound as mean_below \
                      above: no page here comes near f64's 2^53 exact-integer limit. \
                      Same absence of a third route as mean_below above: no \
                      From<u64> for f64 exists in std."
        )]
        let variance = below as f64 * above as f64 * (mean_below - mean_above).powi(2);
        if variance > best_variance {
            best_variance = variance;
            best_level = intensity_level(level);
        }
    }
    best_level
}

/// A pixel coordinate, as `components`' flood-fill neighbour scan takes it.
///
/// # Panics
///
/// Panics if `v` is negative or exceeds `u32::MAX`. Neither happens at the
/// one call site here: the explicit `nx < 0 || ny < 0 || nx >= i64::from(width)
/// || ny >= i64::from(height)` guard immediately above it continues before
/// reaching this, so `v` is already known to be in `0..width` or `0..height`,
/// both `u32`.
fn pixel_coord(v: i64) -> u32 {
    u32::try_from(v).expect("pixel coordinate is negative or exceeds u32::MAX")
}

/// Bounding boxes of the connected regions of ink.
///
/// Ink is dark: a pixel is ink where its luma is at or below `threshold`. Eight
/// connectivity, because a diagonal stroke is one mark to a reader and four
/// connectivity would return it as a staircase of separate components.
///
/// An explicit stack rather than recursion. A run of ink down one side of a
/// full-page panel border is tens of thousands of pixels, and that is a stack
/// overflow rather than a slow function.
pub fn components(page: &GrayImage, threshold: u8) -> Vec<Bounds> {
    let (width, height) = page.dimensions();
    let mut seen = vec![false; (width as usize) * (height as usize)];
    let mut found = Vec::new();
    let mut stack: Vec<(u32, u32)> = Vec::new();

    let is_ink = |x: u32, y: u32| page.get_pixel(x, y).0[0] <= threshold;
    let at = |x: u32, y: u32| (y as usize) * (width as usize) + (x as usize);

    for y in 0..height {
        for x in 0..width {
            if seen[at(x, y)] || !is_ink(x, y) {
                continue;
            }
            seen[at(x, y)] = true;
            stack.push((x, y));
            let mut bounds = Bounds {
                x0: x,
                y0: y,
                x1: x,
                y1: y,
            };

            while let Some((cx, cy)) = stack.pop() {
                bounds = bounds.merge(&Bounds {
                    x0: cx,
                    y0: cy,
                    x1: cx,
                    y1: cy,
                });
                for dy in -1i64..=1 {
                    for dx in -1i64..=1 {
                        let nx = i64::from(cx) + dx;
                        let ny = i64::from(cy) + dy;
                        if nx < 0 || ny < 0 || nx >= i64::from(width) || ny >= i64::from(height) {
                            continue;
                        }
                        let (nx, ny) = (pixel_coord(nx), pixel_coord(ny));
                        if seen[at(nx, ny)] || !is_ink(nx, ny) {
                            continue;
                        }
                        seen[at(nx, ny)] = true;
                        stack.push((nx, ny));
                    }
                }
            }
            found.push(bounds);
        }
    }
    found
}

/// The page's own glyph scale, as the median of its components' longer sides.
///
/// Every gap below is a multiple of this rather than a fraction of the page,
/// and that is the one decision in this file that the corpus forces. Page
/// widths here run from 685 to 5343 pixels for editions of the same work, so a
/// gap expressed as a fraction of the page is eight times too large at one end
/// of that range and eight times too small at the other. Expressed against the
/// page's own strokes, it scales with the type.
///
/// The median rather than the mean, because a panel border is one enormous
/// component and a mean would follow it.
pub fn stroke_scale(strokes: &[Bounds]) -> Option<u32> {
    if strokes.is_empty() {
        return None;
    }
    let mut sides: Vec<u32> = strokes.iter().map(Bounds::longer).collect();
    sides.sort_unstable();
    Some(sides[sides.len() / 2])
}

/// A noise floor for component sizes, derived from the page's own
/// distribution of longer sides rather than a fixed pixel count.
///
/// `docs/notes.md` measured that a fixed floor would be wrong at one end of
/// this corpus or the other: page widths here run 685 to 5343 pixels for
/// editions of the same work, the same fact `stroke_scale`'s own doc comment
/// gives for why the smear gap is page-relative rather than absolute.
///
/// Otsu's variance-maximising split — the same method as `otsu` above,
/// restated rather than shared because `otsu` is specialised to a 256-bin
/// luma histogram — applied to the histogram of `log2(longer side)` instead.
/// Log-spaced because component sizes on one page span three orders of
/// magnitude (a screentone dot and a panel border can appear on the same
/// page), where a linear-pixel histogram would need thousands of bins to
/// resolve the noise cluster from everything above it.
///
/// Returns the top of the split's lower bucket, in real pixels — a component
/// clears the floor when its longer side exceeds this — or `None` only when
/// `strokes` is empty.
///
/// `#[cfg(test)]`, like `stroke_scale_above_floor` below and
/// `region_experiment.rs` itself: a Task 9 candidate, not yet called by
/// production code, so it would otherwise be dead code in an ordinary build.
#[cfg(test)]
fn size_floor(strokes: &[Bounds]) -> Option<u32> {
    // 25 buckets covers log2 of any real scan (2^24px is already absurd), so
    // no bucket count needs to be derived from the page in hand either.
    const BUCKETS: usize = 25;
    if strokes.is_empty() {
        return None;
    }
    let mut histogram = [0u64; BUCKETS];
    for s in strokes {
        let bucket = s.longer().ilog2() as usize;
        histogram[bucket.min(BUCKETS - 1)] += 1;
    }

    let total: u64 = histogram.iter().sum();
    let weighted: u64 = histogram
        .iter()
        .enumerate()
        .map(|(bucket, &count)| bucket as u64 * count)
        .sum();

    let (mut below, mut below_weighted) = (0u64, 0u64);
    let (mut best_bucket, mut best_variance) = (0usize, -1.0f64);
    for (bucket, &count) in histogram.iter().enumerate() {
        below += count;
        if below == 0 {
            continue;
        }
        let above = total - below;
        if above == 0 {
            break;
        }
        below_weighted += bucket as u64 * count;
        #[expect(
            clippy::cast_precision_loss,
            reason = "below_weighted and below are u64 counts of this page's \
                      connected components (`strokes.len()` at most), bucketed into \
                      25 log2 bins; f64 is exact up to 2^53 and no real page has \
                      anywhere near that many components. Property of the material, \
                      the same style of bound as otsu's mean_below above. Same absence \
                      of a third route, too: no From<u64> for f64 exists in std — \
                      f64::from is lossless only for u32/i32/u16/i16/u8/i8."
        )]
        let mean_below = below_weighted as f64 / below as f64;
        #[expect(
            clippy::cast_precision_loss,
            reason = "weighted, below_weighted and above are u64 component counts, \
                      same bound as mean_below above: nowhere near f64's 2^53 \
                      exact-integer limit. No From<u64> for f64 exists in std, so the \
                      third route is closed the same way as mean_below above."
        )]
        let mean_above = (weighted - below_weighted) as f64 / above as f64;
        #[expect(
            clippy::cast_precision_loss,
            reason = "below and above are u64 component counts, same bound as \
                      mean_below above: nowhere near f64's 2^53 exact-integer limit. \
                      No From<u64> for f64 exists in std, so the third route is \
                      closed the same way as mean_below above."
        )]
        let variance = below as f64 * above as f64 * (mean_below - mean_above).powi(2);
        if variance > best_variance {
            best_variance = variance;
            best_bucket = bucket;
        }
    }
    // Bucket b holds sizes in [2^b, 2^(b+1) - 1], so the top of the split's
    // lower bucket — the floor a component's longer side must exceed to
    // count as "above" — is 2^(best_bucket + 1) - 1.
    Some((1u32 << (best_bucket + 1)) - 1)
}

/// The page's own glyph scale, as the median longer side over components
/// above [`size_floor`] — a Task 9 candidate for `stroke_scale`'s "median
/// over everything," measured against it (and against [`stroke_scale_modal`])
/// by `region_experiment.rs` rather than assumed. **Not called by
/// `detect_page`** unless the L2A Task 9 report says this candidate was
/// promoted; see `docs/notes.md`.
///
/// Falls back to `stroke_scale` itself — the same components, unfiltered —
/// when nothing clears the floor: a page whose components are one
/// population all the way up, with nothing for the split to separate.
///
/// `#[cfg(test)]` — see `size_floor`'s doc comment for why.
#[cfg(test)]
pub(crate) fn stroke_scale_above_floor(strokes: &[Bounds]) -> Option<u32> {
    let floor = size_floor(strokes)?;
    let mut sides: Vec<u32> = strokes
        .iter()
        .map(Bounds::longer)
        .filter(|&side| side > floor)
        .collect();
    if sides.is_empty() {
        return stroke_scale(strokes);
    }
    sides.sort_unstable();
    Some(sides[sides.len() / 2])
}

/// The page's own glyph scale, as the mode of the component-size
/// distribution rather than `stroke_scale`'s median — the second Task 9
/// candidate, measured beside `stroke_scale_above_floor` and `stroke_scale`
/// itself by `region_experiment.rs`. **Not called by `detect_page`** unless
/// promoted; see `docs/notes.md`.
///
/// The mode of the exact pixel sizes, not a bucketed histogram: no bin width
/// to justify, and unlike `stroke_scale_above_floor` this candidate does not
/// try to separate a noise population from a signal one first — it is the
/// plain mode the design's three-way comparison asks for.
///
/// Ties go to the smaller size, for a deterministic answer that does not
/// depend on iteration order — the same reason `axis_of` breaks its own tie
/// toward `Axis::Vertical` rather than whichever direction merged first.
///
/// `#[cfg(test)]` — see `size_floor`'s doc comment for why.
#[cfg(test)]
pub(crate) fn stroke_scale_modal(strokes: &[Bounds]) -> Option<u32> {
    if strokes.is_empty() {
        return None;
    }
    let mut counts: std::collections::HashMap<u32, u32> = std::collections::HashMap::new();
    for s in strokes {
        *counts.entry(s.longer()).or_insert(0) += 1;
    }
    counts
        .into_iter()
        .max_by_key(|&(size, count)| (count, std::cmp::Reverse(size)))
        .map(|(size, _)| size)
}

/// Join boxes that are close along `axis` and overlap across it.
///
/// This is run-length smearing done on boxes rather than on pixels: the classic
/// document-layout method, at a fraction of the memory, because the components
/// are already in hand from `components` and the pixels are not needed again.
///
/// **Overlap across the axis is required, not optional.** Without it two
/// balloons at opposite edges of the page merge whenever their columns happen
/// to line up, which on a two-column page is most of them.
pub fn merge_along(strokes: &[Bounds], axis: Axis, gap: u32) -> Vec<Bounds> {
    // Along-axis extent, across-axis extent. One accessor pair, so the two
    // directions cannot drift apart — the same reason `longer` exists rather
    // than leaving every caller to pick `width()` or `height()` by hand.
    let along = |b: &Bounds| match axis {
        Axis::Vertical => (b.y0, b.y1),
        Axis::Horizontal => (b.x0, b.x1),
    };
    let across = |b: &Bounds| match axis {
        Axis::Vertical => (b.x0, b.x1),
        Axis::Horizontal => (b.y0, b.y1),
    };

    let mut open: Vec<Bounds> = Vec::new();
    let mut order: Vec<usize> = (0..strokes.len()).collect();
    order.sort_by_key(|&i| along(&strokes[i]).0);

    for &i in &order {
        let stroke = strokes[i];
        let (s0, s1) = along(&stroke);
        let (a0, a1) = across(&stroke);
        let joined = open.iter_mut().find(|group| {
            let (g0, g1) = along(group);
            let (b0, b1) = across(group);
            // Close along the axis: either overlapping, or within `gap` of it.
            let near = s0 <= g1.saturating_add(gap) && g0 <= s1.saturating_add(gap);
            let overlaps = a0 <= b1 && b0 <= a1;
            near && overlaps
        });
        match joined {
            Some(group) => *group = group.merge(&stroke),
            None => open.push(stroke),
        }
    }

    // One pass leaves groups that grew towards each other unmerged, because the
    // group a stroke joined may since have reached the group before it. Repeat
    // until nothing changes; the count strictly decreases, so this terminates.
    if open.len() < strokes.len() {
        let again = merge_along(&open, axis, gap);
        if again.len() < open.len() {
            return again;
        }
    }
    open
}

/// Which way the text runs, decided by merging both ways and taking the fewer.
///
/// Text smears into fewer pieces along its own reading direction. That is the
/// whole rule, it needs no knowledge of either language, and it is why nothing
/// here has to be told which edition is the raw. A tie reads as vertical, so a
/// single isolated glyph has a stable answer rather than one that follows
/// iteration order.
///
/// **`detect_page_with` no longer calls this.** It smears both ways and unions
/// the result (see there), so the page-wide answer this computes is not what
/// any region is stamped with. The rule it states is still the one that change
/// rests on — a block is kept over a block containing it precisely because
/// text merges furthest along its own direction — and the two tests below pin
/// it. `region_experiment.rs` still calls it, to report the direction the
/// whole page merges furthest along beside its per-block figures.
///
/// `#[cfg(test)]`, like `size_floor`, `stroke_scale_above_floor`,
/// `stroke_scale_modal` and `NO_BAND` above: every remaining caller is
/// test-gated, so leaving it compiled into the production binary is dead code
/// there and `cargo clippy --all-targets -- -D warnings` says so on the
/// non-test build. Gated rather than deleted because `region_experiment.rs`
/// calls it and its two tests below pin the rule; gated rather than
/// `#[allow(dead_code)]` because the gate is the file's existing answer to
/// exactly this question and an allow would hide the next one.
#[cfg(test)]
pub(crate) fn axis_of(strokes: &[Bounds], gap: u32) -> Axis {
    let vertical = merge_along(strokes, Axis::Vertical, gap).len();
    let horizontal = merge_along(strokes, Axis::Horizontal, gap).len();
    if vertical <= horizontal {
        Axis::Vertical
    } else {
        Axis::Horizontal
    }
}

/// A smeared block and the axis whose smear produced it.
///
/// The pair is the unit the two-axis union carries from `merge_along` all the
/// way to `Region`, which is the point of the union: the axis is a property of
/// the block, not of the page it came from. Named so that
/// `region_experiment.rs` — which runs the same stages one at a time and so
/// spells this type out five times over — can say it once.
pub(crate) type Block = (Bounds, Axis);

/// The blocks left after dropping every one wholly inside a block from the
/// other axis — the dedupe the two-axis union in `detect_page_with` needs.
///
/// A column of glyphs smeared *horizontally* comes back as its own separate
/// pieces, each sitting inside the vertical block that already covers them.
/// Dropping anything wholly contained in a different block leaves the survivor
/// whose axis actually joined the strokes.
///
/// **Two limits of that sentence, both measured rather than argued, because
/// the obvious reading over-claims.** Neither is a defect in this function —
/// it does what it says — but "the axis that joined the strokes" is not always
/// a thing the boxes distinguish:
///
/// 1. **An exact duplicate has no such axis.** When both smears land on the
///    same box, both joined the same strokes identically and the axis reported
///    is the tie-break's default, not a measurement. This is not a corner
///    case. Over the 90 band-free corpus pages `region_experiment.rs` samples,
///    the v/h split of surviving blocks moved from 55/21 to 65/8 across this
///    change. 69 blocks survive both before and after; 13 of them changed
///    axis, all horizontal to vertical, and all 13 on a block whose box,
///    stroke count, median stroke and ink are identical in the two runs. An
///    unchanged box that changes label is a duplicate being resolved by the
///    rule below, not a reading direction being discovered.
/// 2. **Containment does not separate blocks that merely overlap.** One
///    balloon and one sound effect crossing is the shape this is wanted for,
///    but a paragraph of several lines has the same shape: the wrong-axis
///    smear returns strips across the lines, which no single line's block
///    contains and which contain no line either, so both sets survive. Over
///    `kobayashi-v01/raw-mono`'s 146 pages the banded detector goes from 4,137
///    blocks to 6,116 across this change. Whether that is a text block found
///    twice or two text blocks found once is not something a count can say,
///    and nothing here has measured it against the truth boxes.
///
///    **Those two counts were measured at the provisional `lo = 16`, not at
///    the shipped `BAND`.** Task 5 ran them (`8531d05`) while `lo` was still
///    16; Task 7's sweep then moved it to 64, and between those two bands
///    dev-set spurious falls 2,473 → 303 (`docs/notes.md`, "Where the band
///    goes, measured at eight bands over 478 hand-drawn boxes"), so the
///    shipped detector's block counts on this edition are an order of
///    magnitude away from the pair above and nothing has re-run them. They are
///    kept, relabelled, rather than restated: what they are here to show is
///    that the union moves block *counts* and not merely labels, and a
///    before/after pair at one fixed band shows that whichever band it is.
///
/// **A third limit, and it is not a property of this function at all**, which
/// is why it sits outside the numbered pair above: **the dedupe runs before
/// the four filters, not after them.** `detect_page_with` applies this to the
/// raw union and then filters the survivors by `MIN_STROKES`,
/// `MIN_BLOCK_STROKE_SPAN`, `MAX_BLOCK_AREA` and `MIN_INK`. So a block dropped
/// here for sitting inside a larger block from the other axis is gone even
/// when that larger block is itself rejected a step later — most obviously by
/// `MAX_BLOCK_AREA`, which fires on 89 of 90 sampled pages (O33,
/// `region_experiment.rs`). A contained block can therefore be removed by a
/// container that never becomes a region.
///
/// **Unmeasured, and stated as unmeasured.** Nothing has counted how often a
/// dropped block's container is later rejected. It is named here because it is
/// new on this branch — the one-axis detector had no dedupe to order — and
/// because it is the most likely mechanism behind the one movement
/// `region_experiment.rs`'s header records without explaining: 76 → 73 final
/// blocks and 55 → 58 zero-block pages over its 90 band-free sampled pages.
/// Reordering the pipeline would be a behaviour change and is not made here.
///
/// **The tie-break is the part that is easy to get wrong**, and is why this is
/// a named function with its own tests rather than a closure. An exact
/// duplicate — the same box produced by both axes, which is what a single
/// isolated stroke gives — is contained in the other *and* contains it, so a
/// rule stated as "drop anything contained in another" drops both and the
/// region vanishes. Containment alone is not enough; the comparison is
/// **strictly larger, or equal-and-earlier**, so exactly one copy of a
/// duplicate survives, and it is the earlier index — `Axis::Vertical`, given
/// the order `detect_page_with` pushes them in.
///
/// Ties are broken on index rather than on axis so that this stays a statement
/// about boxes: nothing here needs to know that vertical is pushed first, and
/// a caller that pushed them the other way would still keep exactly one.
///
/// `pub(crate)` for the reason `merge_along` is: `region_experiment.rs` runs
/// the same stages one at a time and must call this rather than restate it.
/// This is algorithm, not the bookkeeping that module allows itself to copy.
pub(crate) fn uncontained(blocks: &[Block]) -> Vec<Block> {
    let contained = |inner: &Bounds, outer: &Bounds| {
        inner.x0 >= outer.x0 && inner.x1 <= outer.x1 && inner.y0 >= outer.y0 && inner.y1 <= outer.y1
    };
    // u64 so the product cannot wrap: every factor is a u32 pixel extent, and
    // `From<u32> for u64` is lossless, so this needs no cast and no bound.
    let area = |b: &Bounds| u64::from(b.width()) * u64::from(b.height());
    blocks
        .iter()
        .enumerate()
        .filter(|(i, (block, _))| {
            !blocks.iter().enumerate().any(|(j, (other, _))| {
                j != *i
                    && contained(block, other)
                    && (area(other) > area(block) || (area(other) == area(block) && j < *i))
            })
        })
        .map(|(_, entry)| *entry)
        .collect()
}

/// How far apart two strokes may be, as a multiple of the page's stroke scale.
///
/// **A starting value, not a measurement.** It has to exceed the gap between
/// glyphs in a line and between the lines of one balloon, and stay under the
/// whitespace between two balloons. `scripts/region-survey.sh` has since run
/// against four of the twelve corpus works and found this cannot be settled by
/// tuning at all: on seven of eight non-blank sampled pages, `stroke_scale`
/// itself is dominated by sub-5px noise — screentone dots, JPEG ringing,
/// antialiasing fragments — rather than glyph size, so no value of `SMEAR_K`
/// fixes a gap built on a scale that was never measuring type. See
/// `docs/notes.md`, "The text-block detector, measured against four works, and
/// why it does not work yet", and `region_experiment.rs`, which reproduces the
/// measurement in-tree.
///
/// **Phase 1 measured the gaps, but not in this unit**, so it does not settle
/// this either — recorded here because it is the nearest thing to evidence
/// about `SMEAR_K` that exists. Over 478 hand-drawn boxes the *within*-box
/// merge gap is p05 0, p25 0, median 1, p75 6, p95 14 pixels, and the
/// *between*-box gap median 70, p95 424 (`docs/notes.md`, "What text is made
/// of, measured against 480 hand-drawn boxes"). Those are pixels; this
/// constant is a multiple of the page's own `stroke_scale`, and the ratio
/// between them is exactly what the paragraph above says is not measuring type
/// yet. Converting one into the other would be reading a value off a
/// measurement of something else.
///
/// `pub(crate)` so `region_experiment.rs` can compute the same gap from this
/// value rather than a restated copy of it.
pub(crate) const SMEAR_K: u32 = 2;

/// Fewest strokes a block may hold.
///
/// One stroke is a speck of dust, a screentone dot, or the tail of a balloon.
///
/// **Measured, and already at the measured value — nothing moved here.** Phase
/// 1 counted the components inside each of 478 hand-drawn truth boxes and found
/// p05 = **4**, p25 11, median 22, p75 43, p95 271 (`docs/notes.md`, "What text
/// is made of, measured against 480 hand-drawn boxes"). The filter admits a
/// block at `>= 4`, so it excludes at most the bottom 5% of real text boxes.
///
/// **Two ways the two populations are not the same population**, stated because
/// the figure above is otherwise read as tighter than it is:
///
/// 1. Phase 1 counted *every* component whose centre lands in the box, before
///    any band. `strokes_in` counts only the strokes `glyph_band` passed, so
///    the count a block gets here is the smaller of the two on any page whose
///    material the band cuts.
/// 2. Phase 1 counts a component by its *centre*; `strokes_in` requires the
///    whole box inside, so a stroke straddling a block's edge counts there and
///    not here.
///
/// Both push the same way — this filter sees fewer strokes than phase 1
/// counted — so 4 is a floor read off a more generous population than the one
/// it is applied to, and that is the direction that costs recall rather than
/// precision. It has not been re-measured over the banded, wholly-inside
/// population.
///
/// Most screentone no longer reaches this filter at all: "a screentone dot"
/// above describes the band-free detector this constant predates, not the
/// population `detect_page` hands it today.
pub(crate) const MIN_STROKES: usize = 4;

/// Largest a block may be, as a fraction of page area.
///
/// The value comes from the disqualifier the L2 design names — a detector
/// whose median region exceeds 0.15 of the page is not finding text — applied
/// here at the per-block level so such a block never reaches the median in the
/// first place. It was **not** read off the labels; what the labels do is
/// check it.
///
/// **Checked against the labels, and kept.** Exactly 1 of 478 hand-drawn
/// truth boxes exceeds 0.15 — a full-page sound effect on
/// `kobayashi-v01/raw-mono` page 65, at 0.35 — so this ceiling excludes 0.2%
/// of the target. It is not what was costing recall.
///
/// **O32: it also makes `median_area` vacuous as an acceptance rule**, because
/// every region is clamped under 0.15 before the median is taken. That is why
/// the promotion rule is stated in 1:1 matches and spurious-per-page instead;
/// see `docs/superpowers/specs/2026-08-11-l2-detector-design.md` §1.
pub(crate) const MAX_BLOCK_AREA: f32 = 0.15;

/// Least ink a block may contain, as a fraction of its own area.
///
/// **Measured, and measured on the right quantity — the value did not move
/// but what it is a floor on did.** This floor used to sum the *bounding
/// boxes* of the strokes inside a block, which double-counts every overlap:
/// O34 in `docs/status.md` records 61 of 76 surviving blocks scoring above 1.0
/// against a floor of 0.05, and the filter rejecting 0 blocks on 0 of 90
/// sampled pages. It could not reject anything.
///
/// It now counts **ink pixels** — dark pixels at the page's own Otsu
/// threshold, over the block's own pixel area — which is exactly the quantity
/// `truth_population.rs::ink_fraction` measured inside 478 hand-drawn boxes,
/// at the same threshold from the same `otsu`. That distribution is p05
/// **0.054**, median 0.127, p95 0.303 (`docs/notes.md`, "What text is made of,
/// measured against 480 hand-drawn boxes"), so 0.05 admits about 95% of real
/// text boxes and the quantity is now bounded by 1.0 instead of running to
/// 1.89.
///
/// *Corrected on 2026-08-12, and the correction is the point rather than a
/// typo.* This doc comment cited a p05 of **0.050** — exactly the floor — and
/// the constant reads as having been set to the measured p05. It was not: the
/// p05 was printed by a line indexing `trunc(n · p)` while every other
/// percentile in the same table indexed `round((n − 1) · p)`, one element apart
/// at n = 478. Unified onto the second convention, this distribution's p05 is
/// 0.054 and its p95 0.303; the median is unchanged. **`MIN_INK` is unchanged
/// at 0.05**, and it is now a floor sitting just *below* the measured p05
/// rather than exactly on it — so it excludes slightly fewer than 5% of real
/// text boxes, and the "about 95%" claim survives with the coincidence gone.
/// The old figure is left visible here rather than quietly replaced, because a
/// constant whose justification was restated in silence is the defect this
/// file's header exists to prevent.
///
/// **What that changes about which blocks it catches**, because the old
/// wording described a different filter:
///
/// - It is no longer a floor on the band-passed strokes. `ink_in` reads the
///   page, so every dark pixel inside the block counts — screentone the band
///   dropped included. A block sitting on tone therefore scores *higher* here
///   than its glyphs alone would, and this filter is not the one that rejects
///   tone. `glyph_band` is.
/// - What it does reject is a block that is mostly paper: strokes strung far
///   enough apart that the box the smear drew around them is nearly empty.
///   `a_sparse_diagonal_of_strokes_is_not_a_region` below is that shape, at
///   0.044 against this floor.
///
/// It remains the backstop against a filled box of type reading like an empty
/// box with a drawn border, and a drawn border is still usually gone before it
/// — one connected ring is one stroke, rejected by `MIN_STROKES`, and at
/// `BAND` a full-page border is dropped by `hi` before `merge_along` forms a
/// block from it at all. The two filters describe an order of rejection, not a
/// conflict.
pub(crate) const MIN_INK: f32 = 0.05;

/// Fewest stroke-scales a block must span on its longer side.
/// **Still a starting value**, unlike the three constants above.
///
/// Expressed in strokes rather than pixels for the reason `stroke_scale`
/// gives: the corpus spans an eightfold range of page widths.
///
/// **Phase 1 measured nothing this can be read off.** It measured longer
/// sides, components per truth box, within- and between-box merge gaps and ink
/// fraction — not the ratio this is stated in, a block's longer side over the
/// page's own `stroke_scale`. Naming any of those distributions here would be
/// a constant asserted as measured against a measurement of something else,
/// which the header rules out. What has been measured about it is only how
/// often it fires: over the 90 sampled band-free pages of
/// `region_experiment.rs` it rejected 2 blocks on 1 page (O33).
pub(crate) const MIN_BLOCK_STROKE_SPAN: u32 = 2;

/// A component-size band, in ten-thousandths of page height.
///
/// Ten-thousandths because the quantities are small: phase 1 measured the
/// inside-a-truth-box population at a median of 8 and a p95 of 141, so
/// percent would round most of the distribution to zero.
///
/// Against page height rather than pixels because the corpus spans an
/// eightfold range of page widths for editions of the same work —
/// `stroke_scale`'s doc comment records 685 to 5343px — and screentone is
/// printed at a fixed ruling, so a tone dot is a near-constant fraction of the
/// printed page whatever the scan resolution. **Not the same claim as
/// `SMEAR_K`'s**, which stays tied to the page's own strokes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Band {
    pub lo: u32,
    pub hi: u32,
}

/// The band `detect_page` ships with. **`lo` is fixed by Task 7's sweep, over
/// the whole dev set** — it was provisional at 16 until 2026-08-12.
///
/// `band_sweep.rs` scored `detect_page_with` at eight values of `lo` over all
/// nine labelled editions — 45 pages, 478 hand-drawn boxes — and **64 is the
/// row the promotion rule selects**: 48 one-to-one matches at 6.73 spurious
/// per page, the highest 1:1 of the three rows that clear the rule's ceiling of
/// 10.7 spurious per page (the dev set's own truth-boxes-per-page). `lo = 32`
/// scores higher on 1:1 — 57 — and is refused at 12.89 spurious per page. The
/// whole table, losing rows included, is in `docs/notes.md`, "Where the band
/// goes, measured at eight bands over 478 hand-drawn boxes".
///
/// **What this value is and is not.** It is the value that scored best under a
/// rule fixed in advance, on a fixed 45-page dev set — a one-parameter fit,
/// which is a mild overfit and is stated as one. It is *not* a boundary between
/// two measured populations: phase 1 puts the outside population's p95 at 17‱
/// and its inside median at 8‱ (`truth_population.rs`), so a cut at 64 discards
/// most of the population a person's own boxes contain, and it wins anyway
/// because what it removes is what welds. That is a result about the smear, not
/// about glyph size — see `glyph_band`, which states the same limit from the
/// other side. The 30 held-out pages are what would check it, and were not
/// opened here.
///
/// `hi` is unchanged and was **not** swept — the sweep held it fixed so one
/// parameter moved at a time. It is generous and not load-bearing: it exists to
/// drop a panel border, which is one component spanning the whole page at
/// ~10,000. Nothing has measured where it should sit.
pub(crate) const BAND: Band = Band { lo: 64, hi: 2000 };

/// The band that filters nothing: `detect_page_with`'s behaviour before the
/// band existed.
///
/// Two callers, both of which need the *incumbent* rather than a nearly-off
/// band. `region_experiment.rs` reproduces `docs/notes.md`'s diagnosis of the
/// band-free detector and cross-checks its staged copy against this; the band
/// sweep uses it as the control arm, so "the band did not help" is an outcome
/// the sweep can actually report.
///
/// `#[cfg(test)]`, like `size_floor` above and `region_experiment.rs` itself:
/// both current callers are test-gated, so leaving this compiled into the
/// production binary would be dead code there — `cargo clippy --all-targets
/// -- -D warnings` catches exactly that on the non-test build, since this
/// constant has no reference outside `#[cfg(test)]` code.
#[cfg(test)]
pub(crate) const NO_BAND: Band = Band {
    lo: 0,
    hi: u32::MAX,
};

/// The components a band keeps.
///
/// **What this does, stated exactly, because the obvious reading is wrong.**
/// It does *not* isolate glyphs. Phase 1 measured the population inside a
/// hand-drawn truth box at a median longer side of 2px — **identical to the
/// population outside one** — because a glyph shatters into stroke fragments
/// and ringing at the Otsu threshold, and a whole glyph is rare even inside a
/// box. No threshold separates the two. See `docs/notes.md`, "What text is
/// made of, measured against 480 hand-drawn boxes".
///
/// What it does is **thin the field**. Phase 1 measured this for a fixed 4px
/// cut over the outside population — not the fraction-of-page-height cut this
/// function and `BAND` actually apply, which is a different cut on every page
/// of a different height — and found it keeps about 35% of the inside
/// population and at most 5% of the outside, taking the welding population
/// from 903,447 components to roughly 45,000 over the 45 labelled pages.
///
/// **Those two percentages do not describe the cut this function now ships
/// with, and are left here as the measurement they are rather than restated as
/// this one.** They are for a 4px cut, which is around the outside population's
/// directly-measured p95 of 17‱; Task 7's sweep fixed `BAND`'s `lo` at **64**,
/// nearly four times that, so the shipped band keeps far less of *both*
/// populations than the figures above — including most of what a person's own
/// boxes contain. What has been measured about the shipped cut is its outcome,
/// not its selectivity: `docs/notes.md`, "Where the band goes, measured at
/// eight bands over 478 hand-drawn boxes", and `band_sweep.rs`. Nothing has
/// re-counted the two populations at 64.
///
/// That a cut well above glyph-fragment size wins is consistent with what this
/// function is for rather than surprising: `merge_along`'s transitive passes
/// weld specks into a page-spanning blob because screentone is *dense*, not
/// because it is small, so a thinner field is a weaker weld whatever the
/// survivors are.
///
/// **Why this is not a third attempt at the two that were refused.** L2A Task
/// 9 measured `above-floor` and `modal` against a bar fixed before either ran,
/// and refused both. Both changed which components the *median scale* was
/// taken over; neither changed which components were *smeared*. Screentone
/// stayed in `merge_along`'s input both times, so the mechanism was never
/// touched. This changes the smear input.
pub(crate) fn glyph_band(strokes: &[Bounds], height: u32, band: Band) -> Vec<Bounds> {
    if height == 0 {
        return Vec::new();
    }
    strokes
        .iter()
        .filter(|s| {
            let per_ten_thousand = u64::from(s.longer()) * 10_000 / u64::from(height);
            per_ten_thousand >= u64::from(band.lo) && per_ten_thousand <= u64::from(band.hi)
        })
        .copied()
        .collect()
}

/// Every text block on one page, in page fractions.
///
/// A thin wrapper over `detect_page_with`'s stages, fixed at `BAND` — see that
/// function's doc comment for what actually runs, and `BAND`'s for the sweep
/// that chose its `lo`.
pub fn detect_page(page: &GrayImage) -> Vec<Region> {
    detect_page_with(page, BAND)
}

/// `detect_page`, with the band as a parameter.
///
/// Six stages, none of them a model and none of them a new dependency:
/// threshold (Otsu), label the connected components, band-pass them by size
/// (`glyph_band`), take the page's own stroke scale from what the band kept,
/// smear into blocks at a gap that is a multiple of that scale — **both ways,
/// unioned and deduped by `uncontained`**, so one page can hold a vertical
/// balloon and a horizontal sound effect at once — then filter what comes out.
/// The filters are what keeps a panel border and a speck of dust out. Which of
/// the constants along the way now cite a measurement and which are still
/// starting values is in the module header; `BAND`'s `lo` is the one Task 7's
/// sweep fixed, and its `hi` is not.
///
/// Exists so Task 7's sweep measures **this function** at several bands rather
/// than a second copy of it that could drift — the discipline
/// `region_experiment.rs`'s header states for `otsu`, `components`, `axis_of`
/// and `merge_along`.
pub(crate) fn detect_page_with(page: &GrayImage, band: Band) -> Vec<Region> {
    let (width, height) = page.dimensions();
    if width == 0 || height == 0 {
        return Vec::new();
    }

    // `threshold` is kept rather than inlined into `components`: `ink_in`
    // below counts ink pixels and must use the page's own Otsu level, the same
    // one the components were labelled at, not a second cut of its own.
    let threshold = otsu(page);
    let strokes = components(page, threshold);
    // The band comes first, before the scale and before the smear. The scale
    // is meant to describe type, and the smear must never see the field it
    // welds. See `glyph_band` for what this does and — importantly — what it
    // does not do.
    let strokes = glyph_band(&strokes, height, band);
    let Some(scale) = stroke_scale(&strokes) else {
        return Vec::new();
    };
    let gap = (scale * SMEAR_K).max(1);

    // Ink pixels, not summed stroke boxes. Summing boxes double-counts every
    // overlap and so could exceed the block's own area, which is O34 and why
    // this filter rejected nothing on any of 90 sampled pages. Counting
    // pixels reads the page rather than the band-passed strokes, so it is
    // bounded by the block's area by construction — see `MIN_INK` for what
    // that does and does not now catch.
    let ink_in = |block: &Bounds| -> u64 {
        let mut ink = 0u64;
        for y in block.y0..=block.y1 {
            for x in block.x0..=block.x1 {
                if page.get_pixel(x, y).0[0] <= threshold {
                    ink += 1;
                }
            }
        }
        ink
    };
    let strokes_in = |block: &Bounds| -> usize {
        strokes
            .iter()
            .filter(|s| {
                s.x0 >= block.x0 && s.x1 <= block.x1 && s.y0 >= block.y0 && s.y1 <= block.y1
            })
            .count()
    };

    let page_area = f64::from(width) * f64::from(height);

    // Both axes, unioned. `axis_of` picks one direction for the whole page and
    // stamps it on every region, which the L2 design records as a deviation it
    // shipped without noting: a page carrying vertical dialogue beside a
    // horizontal sound effect cannot report both. Smearing each way and
    // keeping what survives lets a page hold both: `taiyaku_core::Axis`
    // justifies storing the field on the grounds that "the clustering already
    // knows it", and a page-wide answer stamped on every block was not that.
    // A per-block answer is — with the one exception `uncontained` measures
    // and states, that a block both smears produce identically has no axis to
    // know and takes the tie-break's vertical.
    //
    // **This moves block counts, not only labels.** Measured over
    // `kobayashi-v01`'s 146 pages with `taiyaku regions`: raw-mono 4,137
    // blocks to 6,116, en-mono 4,112 to 3,997. Nothing here was tuned against
    // the truth boxes and no scorer figure was consulted, so that is a
    // recorded consequence rather than a claimed improvement.
    //
    // **Measured at the provisional `lo = 16`, before Task 7's sweep, not at
    // the `BAND` this function ships with.** Task 5 took them at `8531d05`;
    // `lo` moved to 64 afterwards, and dev-set spurious falls 2,473 → 303
    // between those two bands, so these are not the shipped detector's counts
    // and have not been re-run at 64. The claim they support is the
    // before/after difference at one fixed band, which is unaffected. See
    // `uncontained`'s doc comment, which carries the same pair and the same
    // caveat.
    //
    // Vertical first, because `uncontained`'s tie-break keeps the earlier of
    // two identical boxes and a single isolated stroke is exactly that case —
    // the same answer `axis_of` gives a tie, for the same reason.
    let mut blocks: Vec<Block> = Vec::new();
    for axis in [Axis::Vertical, Axis::Horizontal] {
        blocks.extend(
            merge_along(&strokes, axis, gap)
                .into_iter()
                .map(|b| (b, axis)),
        );
    }

    uncontained(&blocks)
        .into_iter()
        .filter(|(block, _)| strokes_in(block) >= MIN_STROKES)
        .filter(|(block, _)| block.longer() >= scale * MIN_BLOCK_STROKE_SPAN)
        .filter(|(block, _)| {
            let area = f64::from(block.width()) * f64::from(block.height());
            area / page_area <= f64::from(MAX_BLOCK_AREA)
        })
        .filter(|(block, _)| {
            let area = f64::from(block.width()) * f64::from(block.height());
            #[expect(
                clippy::cast_precision_loss,
                reason = "ink_in counts ink pixels inside `block`, incrementing at \
                          most once per pixel of the rectangle it walks, so the \
                          result is bounded by `block`'s own pixel area — which is \
                          `area` itself, the divisor, and in turn at most page_area. \
                          That is the bound the previous justification here said did \
                          *not* hold, and it did not: this filter summed overlapping \
                          stroke bounding boxes until O34 (`docs/status.md`) measured \
                          61 of 76 surviving blocks scoring above 1.0 against a floor \
                          of 0.05. Counting pixels makes the bound structural rather \
                          than a property of the material, and page_area is many \
                          orders of magnitude below f64's 2^53 exact-integer limit — \
                          see stroke_scale's doc comment above, real widths from 685 \
                          to 5343px. The lossless-From route is closed too: ink_in \
                          returns u64, and no From<u64> for f64 exists in std — only \
                          u32/i32/u16/i16/u8/i8 have one."
            )]
            {
                area > 0.0 && ink_in(block) as f64 / area >= f64::from(MIN_INK)
            }
        })
        .map(|(block, axis)| {
            let x = f64::from(block.x0) / f64::from(width);
            let y = f64::from(block.y0) / f64::from(height);
            let w = f64::from(block.width()) / f64::from(width);
            let h = f64::from(block.height()) / f64::from(height);
            Region { x, y, w, h, axis }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{GrayImage, Luma};

    /// A page that is half black and half white: the only threshold that
    /// separates them is somewhere between the two values.
    #[test]
    fn otsu_separates_two_levels() {
        let page = GrayImage::from_fn(20, 20, |x, _| Luma([if x < 10 { 30 } else { 220 }]));
        let t = otsu(&page);
        assert!(
            (30..220).contains(&t),
            "threshold {t} sits between the two levels"
        );
    }

    /// A uniform page has no threshold that separates anything, and the failure
    /// to say so is how a blank page becomes one page-sized component.
    #[test]
    fn otsu_on_a_uniform_page_finds_nothing_dark() {
        let blank = GrayImage::from_fn(20, 20, |_, _| Luma([255]));
        assert!(components(&blank, otsu(&blank)).is_empty());
    }

    fn with_marks(marks: &[(u32, u32, u32, u32)]) -> GrayImage {
        GrayImage::from_fn(40, 40, |x, y| {
            let inside = marks
                .iter()
                .any(|&(x0, y0, w, h)| x >= x0 && x < x0 + w && y >= y0 && y < y0 + h);
            Luma([if inside { 0 } else { 255 }])
        })
    }

    #[test]
    fn two_separated_marks_are_two_components() {
        let found = components(&with_marks(&[(2, 2, 4, 4), (20, 20, 4, 4)]), 128);
        assert_eq!(found.len(), 2);
    }

    #[test]
    fn a_component_bounds_its_mark_exactly() {
        let found = components(&with_marks(&[(2, 3, 4, 5)]), 128);
        assert_eq!(found.len(), 1);
        assert_eq!(
            found[0],
            Bounds {
                x0: 2,
                y0: 3,
                x1: 5,
                y1: 7
            }
        );
        assert_eq!(found[0].width(), 4);
        assert_eq!(found[0].height(), 5);
    }

    /// Eight-connectivity, not four. A diagonal stroke is one mark to a reader
    /// and would be a staircase of separate components under four.
    #[test]
    fn a_diagonal_stroke_is_one_component() {
        let diagonal = GrayImage::from_fn(20, 20, |x, y| {
            Luma([if x == y && x < 10 { 0 } else { 255 }])
        });
        assert_eq!(components(&diagonal, 128).len(), 1);
    }

    /// A page-sized component is what a flood fill produces if it walks the
    /// light pixels instead of the dark ones, so this pins the polarity.
    #[test]
    fn dark_is_ink_and_light_is_paper() {
        let found = components(&with_marks(&[(2, 2, 4, 4)]), 128);
        assert_eq!(found.len(), 1);
        assert!(found[0].width() < 10, "bounded the mark, not the paper");
    }

    #[test]
    fn longer_does_not_care_which_axis_it_came_from() {
        let wide = Bounds {
            x0: 0,
            y0: 0,
            x1: 9,
            y1: 4,
        };
        let tall = Bounds {
            x0: 0,
            y0: 0,
            x1: 4,
            y1: 9,
        };
        assert_eq!(wide.longer(), 10);
        assert_eq!(tall.longer(), 10);
    }

    fn boxes(spans: &[(u32, u32, u32, u32)]) -> Vec<Bounds> {
        spans
            .iter()
            .map(|&(x0, y0, x1, y1)| Bounds { x0, y0, x1, y1 })
            .collect()
    }

    /// `glyph_band`'s own contract, pinned independently of whatever `BAND`
    /// becomes: both `lo` and `hi` are inclusive boundaries (`>=`/`<=`, not
    /// `>`/`<`). The `lo` boundary in particular is decision-critical for
    /// Task 7's sweep's control arm — see `screentone_page`'s doc comment for
    /// a fixture that landed exactly on it and passed the band as a result.
    ///
    /// height=1000 makes the ‱ conversion exact: `longer * 10_000 / 1000 ==
    /// longer * 10`, so a longer side of 10 is exactly 100‱ and one of 20 is
    /// exactly 200‱ — the band's own `lo` and `hi` below, hit precisely
    /// rather than approximately.
    #[test]
    fn glyph_band_is_inclusive_at_both_ends() {
        let band = Band { lo: 100, hi: 200 };
        let strokes = boxes(&[
            (0, 0, 9, 0),  // longer=10 -> 100‱, at lo
            (0, 0, 8, 0),  // longer=9  ->  90‱, just below lo
            (0, 0, 19, 0), // longer=20 -> 200‱, at hi
            (0, 0, 20, 0), // longer=21 -> 210‱, just above hi
        ]);
        let kept = glyph_band(&strokes, 1000, band);
        assert_eq!(
            kept,
            vec![strokes[0], strokes[2]],
            "lo and hi are both inclusive boundaries"
        );
    }

    #[test]
    fn glyph_band_on_a_zero_height_page_is_empty() {
        let strokes = boxes(&[(0, 0, 9, 9)]);
        assert!(glyph_band(&strokes, 0, BAND).is_empty());
    }

    /// The median of the longer sides, which is a proxy for glyph size that
    /// does not care whether a glyph came back as one component or as five
    /// strokes — the median moves with the strokes, and the gap is a multiple
    /// of it, so both scale together.
    #[test]
    fn stroke_scale_is_the_median_longer_side() {
        let strokes = boxes(&[(0, 0, 3, 3), (0, 0, 9, 9), (0, 0, 5, 5)]);
        assert_eq!(stroke_scale(&strokes), Some(6));
        assert_eq!(stroke_scale(&[]), None);
    }

    /// A page of mostly noise (20 specks at 2px) and a handful of glyph-sized
    /// components (20px, 24px, 28px): `stroke_scale` stays with the noise
    /// population because it outnumbers everything else, exactly the failure
    /// `docs/notes.md` measured; `stroke_scale_above_floor` separates the two
    /// populations first and takes the median over the one above the floor.
    #[test]
    fn stroke_scale_above_floor_ignores_the_noise_population() {
        let mut spans: Vec<(u32, u32, u32, u32)> = (0..20).map(|_| (0, 0, 1, 1)).collect();
        spans.extend([(0, 0, 19, 19), (0, 0, 23, 23), (0, 0, 27, 27)]);
        let strokes = boxes(&spans);
        assert_eq!(
            stroke_scale(&strokes),
            Some(2),
            "the median over everything stays with the noise"
        );
        assert_eq!(
            stroke_scale_above_floor(&strokes),
            Some(24),
            "the median over the population above the floor"
        );
    }

    /// When every component is one population — nothing for the split to
    /// separate — `stroke_scale_above_floor` falls back to `stroke_scale`
    /// rather than returning `None` or an arbitrary floor.
    #[test]
    fn stroke_scale_above_floor_falls_back_when_nothing_clears_the_floor() {
        let strokes = boxes(&(0..8).map(|_| (0, 0, 0, 0)).collect::<Vec<_>>());
        assert_eq!(stroke_scale_above_floor(&strokes), stroke_scale(&strokes));
        assert_eq!(stroke_scale_above_floor(&[]), None);
    }

    /// Five components at 6px (the mode) against six components at six
    /// distinct larger sizes, none repeated: the median is pulled up by the
    /// unique larger sizes, but the mode stays at 6px because nothing else
    /// repeats at all.
    #[test]
    fn stroke_scale_modal_is_the_most_common_size_not_the_median() {
        let mut spans: Vec<(u32, u32, u32, u32)> = (0..5).map(|_| (0, 0, 5, 5)).collect();
        spans.extend([
            (0, 0, 19, 19),
            (0, 0, 21, 21),
            (0, 0, 23, 23),
            (0, 0, 25, 25),
            (0, 0, 27, 27),
            (0, 0, 29, 29),
        ]);
        let strokes = boxes(&spans);
        assert_eq!(
            stroke_scale_modal(&strokes),
            Some(6),
            "6px repeats five times, nothing else repeats at all"
        );
        assert_eq!(
            stroke_scale(&strokes),
            Some(20),
            "the median is pulled up by the unique larger sizes"
        );
        assert_eq!(stroke_scale_modal(&[]), None);
    }

    /// Two sizes tied at two components each: the tie goes to the smaller
    /// size, deterministically, rather than varying with iteration order.
    #[test]
    fn stroke_scale_modal_ties_go_to_the_smaller_size() {
        let strokes = boxes(&[(0, 0, 5, 5), (0, 0, 5, 5), (0, 0, 9, 9), (0, 0, 9, 9)]);
        assert_eq!(stroke_scale_modal(&strokes), Some(6));
    }

    /// Two glyphs a small gap apart, stacked vertically: merged along the
    /// vertical axis they are one column; along the horizontal they are two.
    #[test]
    fn merging_along_an_axis_joins_what_is_spaced_along_it() {
        let stacked = boxes(&[(10, 0, 19, 9), (10, 14, 19, 23)]);
        assert_eq!(merge_along(&stacked, Axis::Vertical, 8).len(), 1);
        assert_eq!(merge_along(&stacked, Axis::Horizontal, 8).len(), 2);
    }

    #[test]
    fn merging_respects_the_gap() {
        let stacked = boxes(&[(10, 0, 19, 9), (10, 40, 19, 49)]);
        assert_eq!(
            merge_along(&stacked, Axis::Vertical, 8).len(),
            2,
            "gap too wide to join"
        );
        assert_eq!(merge_along(&stacked, Axis::Vertical, 40).len(), 1);
    }

    /// Two columns side by side must not merge into one just because they
    /// overlap along the axis being merged; they have to overlap *across* it.
    #[test]
    fn merging_needs_overlap_across_the_axis() {
        let apart = boxes(&[(0, 0, 9, 9), (60, 14, 69, 23)]);
        assert_eq!(merge_along(&apart, Axis::Vertical, 8).len(), 2);
    }

    #[test]
    fn a_merged_box_covers_both() {
        let stacked = boxes(&[(10, 0, 19, 9), (10, 14, 19, 23)]);
        let merged = merge_along(&stacked, Axis::Vertical, 8);
        assert_eq!(
            merged[0],
            Bounds {
                x0: 10,
                y0: 0,
                x1: 19,
                y1: 23
            }
        );
    }

    /// Text smears into fewer pieces along its own reading direction, which is
    /// the whole of the axis rule and needs no language knowledge at all.
    #[test]
    fn the_axis_is_the_direction_that_merges_furthest() {
        let column = boxes(&[(10, 0, 19, 9), (10, 14, 19, 23), (10, 28, 19, 37)]);
        assert_eq!(axis_of(&column, 8), Axis::Vertical);

        let line = boxes(&[(0, 10, 9, 19), (14, 10, 23, 19), (28, 10, 37, 19)]);
        assert_eq!(axis_of(&line, 8), Axis::Horizontal);
    }

    /// A single glyph merges to one piece either way. Vertical is the answer
    /// because this project's hub editions are Japanese, and a tie has to go
    /// somewhere rather than varying with iteration order.
    #[test]
    fn a_tie_reads_as_vertical() {
        assert_eq!(axis_of(&boxes(&[(0, 0, 9, 9)]), 8), Axis::Vertical);
    }

    /// The case the whole dedupe exists for: a column smeared the wrong way
    /// comes back as its own pieces, each inside the block the right way
    /// produced, and it is the pieces that must go — not the block.
    #[test]
    fn a_block_swallowing_pieces_from_the_other_axis_keeps_the_block() {
        let column = Bounds {
            x0: 10,
            y0: 0,
            x1: 19,
            y1: 37,
        };
        let mut blocks = vec![(column, Axis::Vertical)];
        for (y0, y1) in [(0, 9), (14, 23), (28, 37)] {
            blocks.push((
                Bounds {
                    x0: 10,
                    y0,
                    x1: 19,
                    y1,
                },
                Axis::Horizontal,
            ));
        }
        assert_eq!(uncontained(&blocks), vec![(column, Axis::Vertical)]);
    }

    /// **The tie-break, checked rather than trusted.** The same box from both
    /// axes is contained in the other *and* contains it, so "drop anything
    /// contained in another block" drops both and the region disappears
    /// entirely. This is what a single isolated stroke produces — it merges to
    /// itself either way — so it is not a corner case the corpus never reaches.
    #[test]
    fn an_exact_duplicate_from_both_axes_keeps_one_copy_not_neither() {
        let same = Bounds {
            x0: 5,
            y0: 5,
            x1: 14,
            y1: 14,
        };
        let kept = uncontained(&[(same, Axis::Vertical), (same, Axis::Horizontal)]);
        assert_eq!(
            kept,
            vec![(same, Axis::Vertical)],
            "one copy survives, and it is the earlier index"
        );
    }

    /// Order decides a duplicate, not the axis label — `uncontained` is a
    /// statement about boxes. Pushed the other way round, the horizontal copy
    /// is the one that survives. `detect_page_with` pushes vertical first, so
    /// in production this resolves the same way `axis_of` resolves a tie; that
    /// is the caller's choice and not a rule hidden in here.
    #[test]
    fn a_duplicate_is_broken_on_order_not_on_axis() {
        let same = Bounds {
            x0: 5,
            y0: 5,
            x1: 14,
            y1: 14,
        };
        let kept = uncontained(&[(same, Axis::Horizontal), (same, Axis::Vertical)]);
        assert_eq!(kept, vec![(same, Axis::Horizontal)]);
    }

    /// Two blocks that overlap without either containing the other both
    /// survive: containment is the rule, not intersection. A vertical balloon
    /// crossing a horizontal sound effect is exactly this shape, and an
    /// overlap-based rule would delete one of them.
    ///
    /// Pinned as behaviour, **not asserted to be desirable** — see limit 2 in
    /// `uncontained`'s doc comment. A multi-line paragraph presents this same
    /// shape, so this rule also keeps the wrong axis's strips beside the right
    /// axis's lines, and the shipped block count rose by half on one raw
    /// edition because of it. This test says what the rule does so that a
    /// later change to it is a decision rather than an accident.
    #[test]
    fn blocks_that_merely_overlap_both_survive() {
        let tall = Bounds {
            x0: 10,
            y0: 0,
            x1: 19,
            y1: 39,
        };
        let wide = Bounds {
            x0: 0,
            y0: 15,
            x1: 39,
            y1: 24,
        };
        let kept = uncontained(&[(tall, Axis::Vertical), (wide, Axis::Horizontal)]);
        assert_eq!(kept, vec![(tall, Axis::Vertical), (wide, Axis::Horizontal)]);
    }

    /// A page with one balloon's worth of vertical text and nothing else.
    fn page_with_a_column() -> GrayImage {
        let marks: Vec<(u32, u32, u32, u32)> =
            (0..6).map(|i| (100u32, 40 + i * 30, 20, 20)).collect();
        GrayImage::from_fn(400, 600, |x, y| {
            let inked = marks
                .iter()
                .any(|&(x0, y0, w, h)| x >= x0 && x < x0 + w && y >= y0 && y < y0 + h);
            Luma([if inked { 0 } else { 255 }])
        })
    }

    #[test]
    fn a_column_of_glyphs_is_one_vertical_region() {
        let found = detect_page(&page_with_a_column());
        assert_eq!(found.len(), 1, "one block, got {found:?}");
        assert_eq!(found[0].axis, Axis::Vertical);
    }

    /// Fractions, not pixels. The corpus holds editions of one work whose page
    /// widths differ by a factor of eight, and a pixel box is meaningless
    /// without the page it came from.
    #[test]
    fn a_region_is_in_page_fractions() {
        let found = detect_page(&page_with_a_column());
        let region = found[0];
        assert!((region.x - 0.25).abs() < 0.01, "x was {}", region.x);
        assert!(region.w > 0.0 && region.w < 1.0);
        assert!(region.y + region.h <= 1.0, "stays on the page");
    }

    /// The same content, uniformly upsampled 2x, must produce the same
    /// fractions. Page width, page height and stroke size all double
    /// together here, so this only rules out a gap frozen as a fixed pixel
    /// constant — any formula linear in page width, page height or stroke
    /// size (a page-fraction gap included) passes this test unchanged. The
    /// gap staying tied to the page's own stroke size, rather than to the
    /// page, is `the_gap_does_not_grow_with_the_page`'s job below.
    #[test]
    fn detection_is_scale_invariant() {
        let small = page_with_a_column();
        let large = GrayImage::from_fn(800, 1200, |x, y| *small.get_pixel(x / 2, y / 2));
        let (a, b) = (detect_page(&small), detect_page(&large));
        assert_eq!(a.len(), b.len(), "same blocks at both scales");
        assert!((a[0].x - b[0].x).abs() < 0.02, "{} vs {}", a[0].x, b[0].x);
        assert!((a[0].h - b[0].h).abs() < 0.02, "{} vs {}", a[0].h, b[0].h);
    }

    /// Two 5-stroke clusters at a fixed size and spacing, on a canvas of any
    /// size — the extra area is pure empty margin.
    ///
    /// The marks are built from `k` alone, so `width` and `height` cannot reach
    /// stroke size or inter-cluster spacing. That independence is the whole
    /// point of the fixture; see `the_gap_does_not_grow_with_the_page` below
    /// for what it is for.
    ///
    /// **The marks are 24px, and were 10px until Task 7's sweep moved `BAND`'s
    /// `lo` from 16 to 64.** A fixed pixel mark on a canvas that grows is a
    /// *shrinking* fraction of page height, which is the one thing a band stated
    /// in ‱ of page height must cut: 10px on the 3,000px canvas below is 33‱,
    /// under 64, so `glyph_band` dropped every stroke and `detect_page` returned
    /// nothing on the large page while still returning two blocks on the small
    /// one. That is the band doing its job on a fixture whose marks had become
    /// tone-sized in page-relative terms — a glyph on a 3,000px page is nearer
    /// 60px, phase 1's inside p95 being 141‱ — rather than a defect in either.
    ///
    /// 24px is chosen against the band and not against the assertion: it is 80‱
    /// on the 3,000px canvas, clear of `lo` on the page where the margin is
    /// tightest. Everything else scales with it so the property below is
    /// untouched — pitch 36 (the strokes still merge within a cluster), 72 blank
    /// rows between the clusters (they still do not merge across).
    fn two_gapped_clusters(width: u32, height: u32) -> GrayImage {
        let marks: Vec<(u32, u32, u32, u32)> = (0..5)
            .map(|k| (100u32, 40 + 36 * k, 24, 24))
            .chain((0..5).map(|k| (100u32, 280 + 36 * k, 24, 24)))
            .collect();
        GrayImage::from_fn(width, height, |x, y| {
            let inked = marks
                .iter()
                .any(|&(x0, y0, w, h)| x >= x0 && x < x0 + w && y >= y0 && y < y0 + h);
            Luma([if inked { 0 } else { 255 }])
        })
    }

    /// The smear gap is tied to the page's own strokes, never to its width.
    ///
    /// `stroke_scale` is 24 on both pages — it is computed from the strokes,
    /// which do not change — so the gap is `SMEAR_K * 24 = 48px` on both, and
    /// the clusters stay apart on both: two blocks, not one, at either size.
    /// A gap tied to page width would grow on the larger page and merge them
    /// there but not on the small one.
    ///
    /// **This is what `detection_is_scale_invariant` above cannot catch**, and
    /// the reason both tests exist. That fixture is a uniform upsample, so page
    /// size and stroke size double together and any gap formula linear in
    /// *either* passes it; it can only catch a gap frozen as a pixel constant.
    ///
    /// The clusters are separated by 72 rows of white — one ends at y=207, the
    /// next begins at y=280 — but the figure that matters is **73**, because
    /// `merge_along` bridges when `s0 <= g1 + gap`, so 73 is the smallest gap
    /// that would weld them. It is 73 that the 48px smear gap is under.
    #[test]
    fn the_gap_does_not_grow_with_the_page() {
        let small = two_gapped_clusters(400, 600);
        let large = two_gapped_clusters(2000, 3000);
        let (a, b) = (detect_page(&small), detect_page(&large));
        assert_eq!(a.len(), 2, "small page: got {a:?}");
        assert_eq!(b.len(), 2, "large page: got {b:?}");
        assert_eq!(a[0].axis, Axis::Vertical);
        assert_eq!(b[0].axis, Axis::Vertical);
    }

    #[test]
    fn a_blank_page_has_no_regions() {
        let blank = GrayImage::from_fn(400, 600, |_, _| Luma([255]));
        assert!(detect_page(&blank).is_empty());
    }

    /// A full-page panel border is one enormous, nearly empty component, and it
    /// is the single most common thing on a manga page that is not text.
    #[test]
    fn a_panel_border_is_not_a_region() {
        let bordered = GrayImage::from_fn(400, 600, |x, y| {
            let edge = x < 4 || y < 4 || x >= 396 || y >= 596;
            Luma([if edge { 0 } else { 255 }])
        });
        assert!(
            detect_page(&bordered).is_empty(),
            "got {:?}",
            detect_page(&bordered)
        );
    }

    /// Ten strokes that pass `MIN_STROKES`, `MIN_BLOCK_STROKE_SPAN` and
    /// `MAX_BLOCK_AREA` — they smear into one block, that block is nowhere
    /// near a fixed pixel size, and it is a small sliver of the page — but
    /// are walked diagonally down the page just inside the smear gap, so the
    /// block's box is far larger than the strokes filling it. This is the
    /// fixture `MIN_INK` exists for: pins that a block can pass every other
    /// filter and still be rejected for being mostly empty.
    ///
    /// **It does not pin what `MIN_INK` counts**, and says so rather than
    /// leaving the next reader to assume it does. The marks here are solid
    /// 6x6 squares that nowhere overlap, so their summed bounding boxes and
    /// their true ink pixels are the same 360, and this block scores 0.044
    /// against the 0.05 floor under either arithmetic — the one this filter
    /// used before O34 and the one it uses now.
    /// `a_block_of_hollow_strokes_is_mostly_paper` below is the fixture that
    /// separates them.
    #[test]
    fn a_sparse_diagonal_of_strokes_is_not_a_region() {
        let marks: Vec<(u32, u32, u32, u32)> =
            (0..10).map(|i| (100 + 5 * i, 17 * i, 6, 6)).collect();
        let page = GrayImage::from_fn(400, 600, |x, y| {
            let inked = marks
                .iter()
                .any(|&(x0, y0, w, h)| x >= x0 && x < x0 + w && y >= y0 && y < y0 + h);
            Luma([if inked { 0 } else { 255 }])
        });
        assert!(
            detect_page(&page).is_empty(),
            "got {:?}",
            detect_page(&page)
        );
    }

    /// A column of six 100x100 marks on a 110px pitch, drawn either as solid
    /// squares or as 1px-thick outlines.
    ///
    /// The two pages are identical to every stage of the detector except the
    /// ink filter: the same six components, the same bounding boxes, the same
    /// `stroke_scale` of 100, the same single 100x650 block. Only the pixels
    /// inside the outlines differ.
    fn page_of_marks(hollow: bool) -> GrayImage {
        GrayImage::from_fn(800, 1200, |x, y| {
            let inked = (0..6).any(|k| {
                let (x0, y0) = (100u32, 100 + 110 * k);
                let inside = x >= x0 && x < x0 + 100 && y >= y0 && y < y0 + 100;
                let edge = x == x0 || x == x0 + 99 || y == y0 || y == y0 + 99;
                inside && (!hollow || edge)
            });
            Luma([if inked { 0 } else { 255 }])
        })
    }

    /// **The fixture that pins what `MIN_INK` counts**, which is the change
    /// O34 asked for and which no fixture in this file could previously fail.
    ///
    /// Both pages produce one 100x650 block holding six components: 65,000
    /// pixels of block, and six stroke boxes of 10,000 each. Summing those
    /// boxes — what this filter did before — gives 60,000/65,000 = 0.92 on
    /// *both* pages, so the old arithmetic admitted both. Counting ink pixels
    /// separates them: the solid marks are 60,000 pixels of ink and still
    /// score 0.92, while the outlines are 396 pixels each — 2,376 in total,
    /// 0.037 — and fall under the 0.05 floor.
    ///
    /// The paired assertion is the point. Asserting only that the hollow page
    /// comes back empty would pass if any *other* filter rejected it, and this
    /// file already carries fixtures that pass for the wrong reason (see O33).
    /// The solid page differs in nothing but ink and comes back as one region,
    /// so ink is the only thing that can be doing the rejecting.
    #[test]
    fn a_block_of_hollow_strokes_is_mostly_paper() {
        let solid = detect_page(&page_of_marks(false));
        assert_eq!(solid.len(), 1, "solid marks are one block, got {solid:?}");
        let hollow = detect_page(&page_of_marks(true));
        assert!(
            hollow.is_empty(),
            "outlines are mostly paper, got {hollow:?}"
        );
    }

    /// One 4px mark alone on a 400x600 page. It is one component, and
    /// `MIN_STROKES` wants four.
    ///
    /// **It clears the band by two ten-thousandths, and the empty-page
    /// assertion cannot tell the difference.** 4px on a 600px page is 66‱ of
    /// page height, against `BAND`'s `lo` of 64 — so the speck reaches
    /// `MIN_STROKES` today and is refused there, which is what this fixture is
    /// for. Any upward move of `lo` drops it at `glyph_band` instead, and
    /// `detect_page` still returns nothing, and this test still passes while
    /// pinning the band rather than the constant it is named for. That is the
    /// vacuous-fixture shape O33 records and `1173466` fixed twice already.
    ///
    /// The first assertion is the guard, in the shape
    /// `a_field_of_screentone_is_not_a_region` uses below: it fails loudly if
    /// the speck stops reaching the filters. Kept at 4px rather than grown
    /// clear of the band, because a mark with comfortable margin at any
    /// plausible `lo` is 8px — 133‱, against phase 1's inside p95 of 141 — and
    /// that is a glyph, not a speck. The fixture is named for what it is; the
    /// assertion is what keeps it that.
    #[test]
    fn a_lone_speck_is_not_a_region() {
        let speck = GrayImage::from_fn(400, 600, |x, y| {
            Luma([if (200..204).contains(&x) && (300..304).contains(&y) {
                0
            } else {
                255
            }])
        });
        assert_eq!(
            glyph_band(&components(&speck, otsu(&speck)), 600, BAND).len(),
            1,
            "fixture must bite: the speck must survive the band and reach \
             MIN_STROKES, or this test pins the band instead"
        );
        assert!(detect_page(&speck).is_empty());
    }

    /// A page-sized field of screentone: 2px dots on a 5px lattice, over a
    /// canvas the size of a real scan.
    ///
    /// Sized deliberately. The outside population's own ‱ percentiles,
    /// measured directly over the 45 labelled corpus pages, are p05=3 p25=4
    /// med=6 p75=6 p95=17 (`docs/notes.md`, "What text is made of, measured
    /// against 480 hand-drawn boxes"); 2px on 2400 is 8, inside that range
    /// rather than at its edge. The earlier fixture used 2px on 1200 — 16‱, at
    /// the outside population's own p95 rather than its middle — which is why
    /// it exercised nothing **against the `lo` of 16 that shipped at the
    /// time**: `glyph_band` keeps a component whose per-ten-thousand is `>=
    /// lo`, so a 16‱ dot landed exactly on the boundary and passed the band
    /// instead of being dropped by it. Task 7's sweep has since moved `lo` to
    /// 64, which would drop that dot too — so the fixture is sized off the
    /// measured population rather than off whatever `BAND` currently is, and
    /// the assertion below is what pins the behaviour either way.
    fn screentone_page(width: u32, height: u32, glyphs: &[(u32, u32, u32, u32)]) -> GrayImage {
        GrayImage::from_fn(width, height, |x, y| {
            let dot = x % 5 < 2 && y % 5 < 2;
            let glyph = glyphs
                .iter()
                .any(|&(x0, y0, w, h)| x >= x0 && x < x0 + w && y >= y0 && y < y0 + h);
            Luma([if dot || glyph { 0 } else { 255 }])
        })
    }

    /// A page of nothing but screentone must produce no region.
    ///
    /// **This fixture bites**, which the one it replaces did not. Without the
    /// band, `stroke_scale` is 2 and the gap 4, and a 5px lattice merges along
    /// a column — a dot ends at x=1 and the next begins at x=5, and
    /// `merge_along` bridges when `s0 <= g1 + gap` — so each column of tone
    /// smears into one full-height strip that clears every filter and comes
    /// back as a region. Hundreds of them. With the band, every dot is 8‱ and
    /// drops before the smear ever runs.
    ///
    /// Note what it does *not* claim: a square lattice does not weld
    /// page-wide, because `merge_along` also requires overlap *across* the
    /// axis and adjacent columns have none. Real screentone welds across
    /// because a real halftone is rotated and irregular. This fixture
    /// reproduces the regions, not that mechanism.
    #[test]
    fn a_field_of_screentone_is_not_a_region() {
        let page = screentone_page(1600, 2400, &[]);
        assert!(
            !detect_page_with(&page, NO_BAND).is_empty(),
            "fixture must bite: without the band this page must produce regions"
        );
        let found = detect_page(&page);
        assert!(found.is_empty(), "got {} regions", found.len());
    }

    /// Screentone under a column of glyphs must not take the glyphs with it.
    ///
    /// The band drops the tone and keeps the lettering, so the column comes
    /// back as one region. Without the band this returns many — the tone
    /// strips above, plus whatever the column merges into — so the assertion
    /// on the count is what bites, not the assertion on the height.
    #[test]
    fn glyphs_survive_a_screentone_background() {
        let marks: Vec<(u32, u32, u32, u32)> =
            (0..8).map(|k| (200u32, 200 + 80 * k, 40, 40)).collect();
        let page = screentone_page(1600, 2400, &marks);
        assert!(
            !detect_page_with(&page, NO_BAND).is_empty(),
            "fixture must bite: without the band this page must produce regions"
        );
        let found = detect_page(&page);
        assert_eq!(found.len(), 1, "one block, got {found:?}");
        assert!(found[0].h > 0.2, "spans the column, got {}", found[0].h);
    }

    /// Two balloons a long way apart stay two regions. A detector that merged
    /// them would score well on any metric counting how much text it covered
    /// and be useless for pairing, which is what the whole feature is for.
    #[test]
    fn two_distant_columns_stay_two_regions() {
        let marks: Vec<(u32, u32, u32, u32)> = (0..6)
            .flat_map(|i| [(60u32, 40 + i * 30, 20, 20), (300u32, 40 + i * 30, 20, 20)])
            .collect();
        let page = GrayImage::from_fn(400, 600, |x, y| {
            let inked = marks
                .iter()
                .any(|&(x0, y0, w, h)| x >= x0 && x < x0 + w && y >= y0 && y < y0 + h);
            Luma([if inked { 0 } else { 255 }])
        });
        assert_eq!(detect_page(&page).len(), 2);
    }

    /// A vertical column beside a horizontal line must come back as two
    /// regions carrying different axes. One axis per page cannot express
    /// this, and no count can observe the difference — which is why the L2
    /// design's own correction names it and why this test is geometric.
    #[test]
    fn a_column_beside_a_row_keeps_both_axes() {
        let column: Vec<(u32, u32, u32, u32)> =
            (0..6).map(|i| (80u32, 100 + i * 60, 40, 40)).collect();
        let row: Vec<(u32, u32, u32, u32)> =
            (0..6).map(|i| (500u32 + i * 60, 700, 40, 40)).collect();
        let marks: Vec<_> = column.into_iter().chain(row).collect();
        let page = GrayImage::from_fn(1000, 1400, |x, y| {
            let inked = marks
                .iter()
                .any(|&(x0, y0, w, h)| x >= x0 && x < x0 + w && y >= y0 && y < y0 + h);
            Luma([if inked { 0 } else { 255 }])
        });
        let found = detect_page(&page);
        assert_eq!(found.len(), 2, "got {found:?}");
        let mut axes: Vec<Axis> = found.iter().map(|r| r.axis).collect();
        axes.sort_by_key(|a| format!("{a:?}"));
        assert_eq!(
            axes,
            vec![Axis::Horizontal, Axis::Vertical],
            "got {found:?}"
        );
    }
}
