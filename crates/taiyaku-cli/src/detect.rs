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
pub fn axis_of(strokes: &[Bounds], gap: u32) -> Axis {
    let vertical = merge_along(strokes, Axis::Vertical, gap).len();
    let horizontal = merge_along(strokes, Axis::Horizontal, gap).len();
    if vertical <= horizontal {
        Axis::Vertical
    } else {
        Axis::Horizontal
    }
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
/// `pub(crate)` so `region_experiment.rs` can compute the same gap from this
/// value rather than a restated copy of it.
pub(crate) const SMEAR_K: u32 = 2;

/// Fewest strokes a block may hold. **A starting value** (see `SMEAR_K`).
///
/// One stroke is a speck of dust, a screentone dot, or the tail of a balloon.
pub(crate) const MIN_STROKES: usize = 4;

/// Largest a block may be, as a fraction of page area. **A starting value.**
///
/// This is the disqualifier the L2 design names — a detector whose median
/// region exceeds 0.15 of the page is not finding text — set here at the
/// per-block level so such a block never reaches the median in the first place.
pub(crate) const MAX_BLOCK_AREA: f32 = 0.15;

/// Least ink a block may contain, as a fraction of its own area.
/// **A starting value.**
///
/// "Ink" here is the summed **bounding-box** area of the strokes inside the
/// block, not a count of true ink pixels — `components` returns boxes, so a
/// pixel-accurate tally is not available without giving it pixel access,
/// which is a design change out of scope here. That makes this floor looser
/// than true coverage: a hollow stroke (an outlined kanji, a screentone ring)
/// or a diagonal one counts its whole box as ink even though most of the box
/// is paper, so a block built from such strokes can clear this floor with
/// less real ink on the page than the ratio suggests.
///
/// It exists as a backstop against a filled box of type reading like an
/// empty box with a drawn border. Most such borders never reach this filter —
/// a drawn border is one connected ring, one stroke, and is already rejected
/// by `MIN_STROKES` — so this is what catches the ones that survive that far.
pub(crate) const MIN_INK: f32 = 0.05;

/// Fewest stroke-scales a block must span on its longer side.
/// **A starting value.**
///
/// Expressed in strokes rather than pixels for the reason `stroke_scale`
/// gives: the corpus spans an eightfold range of page widths.
pub(crate) const MIN_BLOCK_STROKE_SPAN: u32 = 2;

/// Every text block on one page, in page fractions.
///
/// Five stages: threshold, label, scale, smear, filter. The filters are what
/// keeps a panel border and a speck of dust out, and each of them is a
/// starting value the corpus survey settles.
pub fn detect_page(page: &GrayImage) -> Vec<Region> {
    let (width, height) = page.dimensions();
    if width == 0 || height == 0 {
        return Vec::new();
    }

    let strokes = components(page, otsu(page));
    let Some(scale) = stroke_scale(&strokes) else {
        return Vec::new();
    };
    let gap = (scale * SMEAR_K).max(1);

    // A block's ink is the summed bounding-box area of the strokes inside it
    // — see `MIN_INK` for what that floor does and does not establish.
    let ink_in = |block: &Bounds| -> u64 {
        strokes
            .iter()
            .filter(|s| {
                s.x0 >= block.x0 && s.x1 <= block.x1 && s.y0 >= block.y0 && s.y1 <= block.y1
            })
            .map(|s| u64::from(s.width()) * u64::from(s.height()))
            .sum()
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
    let axis = axis_of(&strokes, gap);

    merge_along(&strokes, axis, gap)
        .into_iter()
        .filter(|block| strokes_in(block) >= MIN_STROKES)
        .filter(|block| block.longer() >= scale * MIN_BLOCK_STROKE_SPAN)
        .filter(|block| {
            let area = f64::from(block.width()) * f64::from(block.height());
            area / page_area <= f64::from(MAX_BLOCK_AREA)
        })
        .filter(|block| {
            let area = f64::from(block.width()) * f64::from(block.height());
            #[expect(
                clippy::cast_precision_loss,
                reason = "ink_in sums each stroke's bounding-box area, filtered to \
                          strokes inside `block`; those boxes can overlap — two \
                          crossing diagonal strokes in one glyph is the easy case — \
                          so the sum can exceed `block`'s own pixel area rather than \
                          being bounded by it. Recorded as O34 in `docs/status.md`: \
                          61 of 76 surviving blocks score above 1.0 against MIN_INK's \
                          0.05 floor. The bound that does hold: at most \
                          `strokes.len()` boxes, each at most page_area, so the sum \
                          is bounded by strokes-on-page times page-area — still many \
                          orders of magnitude below f64's 2^53 exact-integer limit; \
                          see stroke_scale's doc comment above, real widths from 685 \
                          to 5343px. The lossless-From route is closed too: ink_in \
                          returns u64, and no From<u64> for f64 exists in std — only \
                          u32/i32/u16/i16/u8/i8 have one."
            )]
            {
                area > 0.0 && ink_in(block) as f64 / area >= f64::from(MIN_INK)
            }
        })
        .map(|block| {
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
    fn two_gapped_clusters(width: u32, height: u32) -> GrayImage {
        let marks: Vec<(u32, u32, u32, u32)> = (0..5)
            .map(|k| (100u32, 40 + 15 * k, 10, 10))
            .chain((0..5).map(|k| (100u32, 139 + 15 * k, 10, 10)))
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
    /// `stroke_scale` is 10 on both pages — it is computed from the strokes,
    /// which do not change — so the gap is `SMEAR_K * 10 = 20px` on both, and
    /// the clusters stay apart on both: two blocks, not one, at either size.
    /// A gap tied to page width would grow on the larger page and merge them
    /// there but not on the small one.
    ///
    /// **This is what `detection_is_scale_invariant` above cannot catch**, and
    /// the reason both tests exist. That fixture is a uniform upsample, so page
    /// size and stroke size double together and any gap formula linear in
    /// *either* passes it; it can only catch a gap frozen as a pixel constant.
    ///
    /// The clusters are separated by 29 rows of white — one ends at y=109, the
    /// next begins at y=139 — but the figure that matters is **30**, because
    /// `merge_along` bridges when `s0 <= g1 + gap`, so 30 is the smallest gap
    /// that would weld them. It is 30 that the 20px smear gap is under.
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

    #[test]
    fn a_lone_speck_is_not_a_region() {
        let speck = GrayImage::from_fn(400, 600, |x, y| {
            Luma([if (200..204).contains(&x) && (300..304).contains(&y) {
                0
            } else {
                255
            }])
        });
        assert!(detect_page(&speck).is_empty());
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
}
