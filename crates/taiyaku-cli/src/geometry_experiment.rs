//! Which derived transform maps a rect between editions — a one-off.
//!
//! A measurement rather than a check: this is the shape the autocrop and
//! mirrored-pages hypotheses were settled in (`docs/notes.md`, "Two more
//! hypotheses tested and rejected"), and neither became a standing check. The
//! promotion rule was fixed before the number was known — see
//! `docs/superpowers/specs/2026-08-07-l1-geometry-design.md` §4 — so that the
//! result decides the cost rather than the cost deciding how the result reads.
//!
//! **It still asserts a floor**, and the floor is deliberately not a residual
//! bound: that the sample was non-empty, that every volume contributed at
//! least one slot whose search found a minimum, and that every residual it
//! reports is finite. A harness that silently measured nothing — a glob that
//! matched no volume, a solve that paired no slot, a volume every one of whose
//! searches ran out of room — would otherwise print an empty or NaN table and
//! pass, which is the one failure a measurement must not have. Pinning the
//! residual itself is what the promotion rule decides, after there is a number
//! to pin.
//!
//! ```sh
//! TAIYAKU_CORPUS="$PWD/corpus" cargo test --release -p taiyaku-cli geometry_experiment -- --nocapture
//! ```
//!
//! **`--release`, and it is not optional in practice.** The same command
//! without it measures the same thing and takes about forty minutes rather
//! than about one: the search fingerprints roughly 1.4 million crops, each a
//! Lanczos3 resize, and an unoptimised `image` is two orders of magnitude
//! slower at that than an optimised one. Both were run; the figures below and
//! in `docs/notes.md` are from the release build. This is a one-off, so the
//! usual reason to keep a check in the debug profile — that `check-all.sh`
//! runs it — does not apply, and `check-all.sh` does not run it.
//!
//! **What it found, so this header is not just a description of a method.**
//! Over 284 settled slots across all twelve corpus volumes, the best map's
//! vertical scale is 0.996 — the identity map to within one tick of the
//! search's own resolution — while its horizontal scale tracks `aFrom / aTo`
//! in all twelve, sign included. That is the letterbox assumption with its
//! axes swapped, so the two candidates the spec named came third and second:
//! `letterbox` as shipped is strictly worse than `identity`, and a third rule
//! this experiment named — the same arithmetic with `Math.max` over the two
//! aspects replaced by `Math.min` — beats both. `docs/notes.md`, "Which
//! derived transform maps a rect between editions", has the table and the
//! exposure.
//!
//! **And the falsification that decides whether to believe it.** The
//! horizontal finding was indistinguishable on its face from an artefact of a
//! score that renders every crop onto one 17x16 grid whatever its shape, so
//! the identical search is run on the identical slots the other way round. A
//! real geometric relation inverts when the pages swap roles; a preference
//! belonging to the metric or the search does not know which page is which and
//! comes back on the same side of 1. It inverts: the reverse horizontal scale
//! lands on the far side of 1 in twelve volumes of twelve, and
//! `sx forward * sx reverse` has a median of 0.994 against 1.084 for the
//! artefact. That is the second run's whole reason for existing, and it cost
//! 98 s rather than 68.

use std::path::{Path, PathBuf};

use image::GrayImage;
use rayon::prelude::*;
use taiyaku_core::{Direction, Half, PageHash, similarity, solve};

use crate::cached_edition;
use crate::corpus::{corpus, members};
use crate::fingerprint::{decode_luma, fingerprint};

/// Every volume of the aligned corpus, all four series at v01, v03 and v08.
///
/// Named rather than globbed. A glob that stops matching is the silent-nothing
/// failure this module's header refuses to have, and the assertion below that
/// *every* named volume contributed a measurement only means something if the
/// list is fixed.
const VOLUMES: [&str; 12] = [
    "gotoubun-v01",
    "gotoubun-v03",
    "gotoubun-v08",
    "kaguya-v01",
    "kaguya-v03",
    "kaguya-v08",
    "kobayashi-v01",
    "kobayashi-v03",
    "kobayashi-v08",
    "oshinoko-v01",
    "oshinoko-v03",
    "oshinoko-v08",
];

/// The one edition pair every volume has.
///
/// `raw-mono` against `en-mono` is the only pairing present in all twelve —
/// the colour and fan editions exist for four volumes between them. Measuring
/// the pair that exists everywhere is what makes the twelve rows comparable;
/// the alternative, measuring whatever each volume happened to carry, would
/// have confounded series with edition pair, which `notes.md`'s "the floor is
/// per edition-pair" section already says is the wrong way round.
const FROM: &str = "raw-mono";
const TO: &str = "en-mono";

/// All four series are manga.
const READING: Direction = Direction::RightToLeft;

/// Slots measured per volume.
///
/// Twelve volumes of full-resolution scans is thousands of pages and the search
/// below decodes both sides of every one it measures, so the sample is capped
/// rather than exhaustive. Every table below prints sampled-of-available,
/// because a silent cap reads as "measured everything" when it did not.
///
/// 24 rather than the 8 the first run used: at 8 the run took 26 s in release
/// and the interesting figure — the p90 of the residual, which is what the
/// promotion rule turns on — rested on 96 slots, so its tail was nine of them.
/// 24 costs about a minute and a half and puts 288 slots behind it.
const SAMPLE: usize = 24;

/// The window of the source page the search matches, in page fractions.
///
/// The central 60% on both axes. Two competing pressures set it: it has to be
/// large enough that a 16x16 difference hash of it is a real fingerprint rather
/// than noise, and small enough that the mapped window stays on the target page
/// across the whole search range. At 0.6 the widest search — scale 1.18 with a
/// 0.09 offset — reaches 0.944 of the page and still fits.
const WINDOW: (f64, f64) = (0.20, 0.80);

/// Height, in pixels, of the image the grid search actually runs on.
///
/// **A deviation, and it buys the run's whole time budget.** Each candidate
/// crop is fingerprinted, and `fingerprint` resizes to a 17x16 grid with
/// Lanczos3 — on a full 2147x3056 scan that is milliseconds per candidate and
/// there are ~4,900 candidates per slot. Pre-scaling each page once to 384px
/// tall makes it microseconds.
///
/// The cost is that a candidate is now resized twice rather than once, so a
/// hash here is not bit-identical to a hash of the same crop at full
/// resolution. That is the same property the hash is already relied on for:
/// `notes.md` records the corpus spanning 685x1024 to 2147x3056 across editions
/// of one volume and the dhash comparing across it, and `fingerprint`'s own doc
/// says the result is "independent of exposure and of the page's resolution".
/// 384 is comfortably above the 685-pixel-wide smallest edition in the corpus
/// divided by the window, so nothing is being asked to resolve detail it does
/// not have.
const WORK: u32 = 384;

/// Smallest crop side, in working pixels, that may be fingerprinted.
///
/// Below the 17x16 grid a "resize" is an upsample and the hash stops describing
/// the page. Nothing in the search range comes near this; it exists so that a
/// future widening of the range fails visibly rather than quietly hashing
/// interpolation.
const MIN_SIDE: u32 = 32;

/// Coarse grid: scale half-width, offset half-width, points per dimension.
///
/// Scale 0.82–1.18 covers every aspect ratio the corpus pairs — the widest
/// `raw`/`en` aspect ratio between any two editions of one volume is well
/// inside it — and offset ±0.09 covers a letterbox up to 9% of a page edge.
/// A winner sitting *on* either boundary is counted and printed, because that
/// is the search saying the truth may be outside the box it looked in.
const COARSE: (f64, f64, usize) = (0.18, 0.09, 7);

/// Refinement: points per dimension, and how many halvings follow the coarse
/// pass.
///
/// Each pass re-searches one coarse step either side of the winner, so the
/// resolution halves twice per pass. Four passes take scale from ±0.06 to
/// ±0.00375 and offset from ±0.03 to ±0.0019 — on a 1200px page that last
/// figure is two pixels, well under anything a reader could see.
///
/// Coarse-to-fine rather than one dense grid: a single grid at this resolution
/// is 96^2 x 48^2 candidates, which is four orders of magnitude more work for a
/// number that does not change. The exposure is a narrow similarity peak the
/// coarse pass steps over, which is why the coarse offset step (0.03 of the
/// page, against a hash cell of 0.6/16 = 0.0375) is deliberately finer than one
/// cell of the grid the hash is taken on.
const REFINE: (usize, usize) = (5, 4);

/// A per-axis map between two pages' fractional coordinates.
///
/// `u' = 0.5 + scale*(u - 0.5) + offset`, centred so that scale and offset move
/// as independently as an affine map allows: at `offset = 0` the page centre is
/// fixed whatever the scale. That is not cosmetic — both candidates under test
/// put the offset at exactly zero, so a parameterisation anchored anywhere else
/// would have entangled the one number they disagree about with one they agree
/// on.
#[derive(Debug, Clone, Copy)]
struct Axis {
    scale: f64,
    offset: f64,
}

impl Axis {
    /// The same fractions name the same content: candidate 1, and also what
    /// candidate 2 does to the horizontal axis.
    const IDENTITY: Self = Self {
        scale: 1.0,
        offset: 0.0,
    };

    fn at(self, u: f64) -> f64 {
        0.5 + self.scale * (u - 0.5) + self.offset
    }

    /// The furthest two maps put the same point, over `span`.
    ///
    /// Both maps are affine, so the extreme is at an end of the interval and
    /// two evaluations settle it. This is the residual: a fraction of the
    /// target page's width or height, and directly "how far off the reader
    /// lands".
    fn apart(self, other: Self, span: (f64, f64)) -> f64 {
        let d = |u: f64| (self.at(u) - other.at(u)).abs();
        d(span.0).max(d(span.1))
    }
}

/// The best map found for one slot pair, and what it cost each candidate.
#[derive(Debug, Clone, Copy)]
struct Measured {
    /// Which two pages this row measured.
    ///
    /// Carried so that a slot whose search ended on its own boundary can be
    /// *named* rather than only counted. The first run printed the count and
    /// nothing else, which left four rows that had not found a minimum sitting
    /// inside every distribution below with no way to look at them.
    a_page: usize,
    b_page: usize,
    /// Whole-page similarity of the two slots, in the solver's own currency.
    /// Printed so a row that measured two pages the solver barely paired is
    /// visible as such rather than averaged in unseen.
    paired: f32,
    /// Similarity at the best map the search found.
    best_score: f32,
    /// Similarity the two candidates reach, for the same reason.
    identity_score: f32,
    letterbox_score: f32,
    best: (Axis, Axis),
    letterbox: (Axis, Axis),
    /// `letterbox`'s mirror: the same rule with `Math.max` over the two aspects
    /// replaced by `Math.min`. Not in the spec's two, and added because the
    /// twelve rows of `scale x of best` asked for it by name — see its
    /// construction in `measure`.
    pillarbox: (Axis, Axis),
    pillarbox_score: f32,
    /// The same search run where the right answer is already known to be the
    /// identity map. See `reaspected`: this is the experiment's own floor.
    control: (Axis, Axis),
    control_score: f32,
    /// **The falsification.** The identical search on the identical two pages,
    /// run the other way round: `TO`'s window searched onto `FROM`.
    ///
    /// The forward run's horizontal finding — best scale tracking
    /// `aFrom / aTo` in twelve volumes of twelve — is indistinguishable on its
    /// face from an artefact of a search that likes a particular crop *shape*,
    /// because `fingerprint` renders every crop onto one 17x16 grid whatever
    /// its aspect. A real geometric relation between two pages inverts when the
    /// pages swap roles, so `reverse.0.scale` must come back as
    /// `1 / best.0.scale`; a bias belonging to the metric or the search does
    /// not know which page is which and comes back on the same side of 1.
    /// `scale x of best * scale x of reverse` is the whole test in one number:
    /// 1.0 if the geometry is real, `(aFrom/aTo)^2` if it is not.
    ///
    /// Measured on the same slots rather than a fresh sample, so the two
    /// directions differ in nothing but direction.
    reverse: (Axis, Axis),
    reverse_score: f32,
    /// The reverse direction's own floor, built the same way as `control`.
    reverse_control: (Axis, Axis),
    reverse_control_score: f32,
    /// Where the coarse pass ended, for each of the four searches, when it
    /// ended on the edge of its own range — `None` when it found a minimum.
    ///
    /// A search that stops on its boundary has not found a minimum; it has run
    /// out of room, and its answer is a floor rather than a result. Every
    /// distribution below drops the rows where *the search that figure came
    /// from* ran out of room — four separate filters, not one, because the four
    /// searches of a slot fail independently.
    forward_boundary: Option<(Axis, Axis)>,
    reverse_boundary: Option<(Axis, Axis)>,
    control_boundary: bool,
    reverse_control_boundary: bool,
}

impl Measured {
    /// Residual of one candidate, `(dx, dy)`, in fractions of the target page.
    fn residual(&self, candidate: (Axis, Axis), span: (f64, f64)) -> (f64, f64) {
        (
            candidate.0.apart(self.best.0, span),
            candidate.1.apart(self.best.1, span),
        )
    }

    /// Did the forward search find a minimum rather than run out of room?
    fn settled(&self) -> bool {
        self.forward_boundary.is_none()
    }

    /// Both directions did — what the reciprocal check needs, since it
    /// multiplies one direction's answer by the other's.
    fn settled_both(&self) -> bool {
        self.settled() && self.reverse_boundary.is_none()
    }
}

/// Scale a decoded page down to the working height, keeping its aspect.
///
/// Pages smaller than `WORK` are left alone rather than upsampled: growing a
/// small page adds no information and would make the search slower on exactly
/// the editions that are cheapest.
fn working(page: &GrayImage) -> GrayImage {
    let (w, h) = page.dimensions();
    if h <= WORK {
        return page.clone();
    }
    let width = ((w as f64 * WORK as f64 / h as f64).round() as u32).max(1);
    image::imageops::resize(page, width, WORK, image::imageops::FilterType::Lanczos3)
}

/// The source page re-sampled to the target's aspect, keeping its height.
///
/// **The control, and the reason there is one.** The score is
/// `fingerprint` — mandated, and rightly, so the number is in the same currency
/// as the rest of `notes.md` — but `fingerprint` resizes every crop to a fixed
/// 17x16 grid whatever its shape. Two crops holding the same artwork at
/// different aspects are therefore *rendered* differently before they are
/// compared, and the search can buy similarity by picking a per-axis scale
/// ratio that squares the two renderings up rather than by finding where the
/// artwork actually is. That is a property of the measure, not of the pages.
///
/// Running the identical search against a re-sampled copy of the source
/// separates the two. A resize moves nothing in fractional coordinates — the
/// page centre is still the page centre, and every point of the artwork is
/// still at the fraction it was — so the true answer here is *exactly* the
/// identity map, on both axes, with no framing difference of any kind present.
/// Whatever the search returns instead is the floor: what this experiment
/// produces from an aspect difference alone. A residual against a real edition
/// is only evidence of real framing once it is clear of this.
fn reaspected(page: &GrayImage, aspect: f64) -> GrayImage {
    let height = page.height();
    let width = ((f64::from(height) * aspect).round() as u32).max(1);
    image::imageops::resize(page, width, height, image::imageops::FilterType::Lanczos3)
}

/// Fingerprint the window of `page` that a pair of axis maps names.
///
/// `None` when the window falls off the page or shrinks below `MIN_SIDE`, which
/// the search treats as a candidate that cannot be scored rather than as a
/// score of zero — a clamped crop would be a *different* window scored as if it
/// were the requested one, which is how a grid search learns to like the edge
/// of its own range.
fn window_hash(page: &GrayImage, h: Axis, v: Axis) -> Option<PageHash> {
    let (w, height) = page.dimensions();
    let edge = |f: f64, n: u32| (f * f64::from(n)).round();
    let (x0, x1) = (edge(h.at(WINDOW.0), w), edge(h.at(WINDOW.1), w));
    let (y0, y1) = (edge(v.at(WINDOW.0), height), edge(v.at(WINDOW.1), height));
    if x0 < 0.0 || y0 < 0.0 || x1 > f64::from(w) || y1 > f64::from(height) {
        return None;
    }
    let (x0, y0) = (x0 as u32, y0 as u32);
    let (side_w, side_h) = ((x1 as u32) - x0, (y1 as u32) - y0);
    (side_w >= MIN_SIDE && side_h >= MIN_SIDE)
        .then(|| fingerprint(&image::imageops::crop_imm(page, x0, y0, side_w, side_h).to_image()))
}

/// The points of one grid dimension: `steps` values spanning `centre ± half`.
fn ticks(centre: f64, half: f64, steps: usize) -> Vec<f64> {
    match steps {
        0 | 1 => vec![centre],
        _ => (0..steps)
            .map(|i| centre - half + 2.0 * half * i as f64 / (steps - 1) as f64)
            .collect(),
    }
}

/// What one grid search returned.
#[derive(Debug, Clone, Copy)]
struct Search {
    map: (Axis, Axis),
    score: f32,
    /// The coarse pass's own winner, when that winner sat on the edge of the
    /// coarse range — `None` when it did not.
    ///
    /// The winner rather than a bare flag: the refinement passes move the
    /// answer off the edge again, so by the time the search returns there is
    /// nothing in the result to show which of the four parameters ran out of
    /// room, or on which side. Keeping the coarse winner is what lets such
    /// slots be printed and read rather than only counted — which is how the
    /// forward run's four turned out to be near-blank pages.
    boundary: Option<(Axis, Axis)>,
}

/// The best rect-to-rect map from `from`'s fixed window onto `onto`.
///
/// Scored with `fingerprint` on each crop and `taiyaku_core::similarity`
/// between the two hashes — the existing measure, so the number is in the same
/// currency as every other figure in `notes.md` and a bad score cannot be a
/// property of a metric invented for this experiment.
fn best_map(target: PageHash, onto: &GrayImage) -> Search {
    let mut centre = (Axis::IDENTITY, Axis::IDENTITY);
    let mut best = f32::NEG_INFINITY;
    let (mut scale_half, mut offset_half, mut steps) = COARSE;
    let mut boundary = None;

    for pass in 0..=REFINE.1 {
        // Re-centred on the running winner from the second pass onward. The
        // first pass is centred on the identity map because that is the only
        // prior worth having: it is one of the two candidates.
        let (sx_c, sy_c, ox_c, oy_c) = match pass {
            0 => (1.0, 1.0, 0.0, 0.0),
            _ => (
                centre.0.scale,
                centre.1.scale,
                centre.0.offset,
                centre.1.offset,
            ),
        };
        let mut grid: Vec<(Axis, Axis)> = Vec::new();
        for &sx in &ticks(sx_c, scale_half, steps) {
            for &ox in &ticks(ox_c, offset_half, steps) {
                for &sy in &ticks(sy_c, scale_half, steps) {
                    for &oy in &ticks(oy_c, offset_half, steps) {
                        grid.push((
                            Axis {
                                scale: sx,
                                offset: ox,
                            },
                            Axis {
                                scale: sy,
                                offset: oy,
                            },
                        ));
                    }
                }
            }
        }

        // Scores collected then scanned in order rather than reduced in
        // parallel: a parallel `max_by` breaks ties by whichever thread got
        // there first, and this has to give the same answer twice.
        let scores: Vec<f32> = grid
            .par_iter()
            .map(|&(h, v)| {
                window_hash(onto, h, v)
                    .map(|hash| similarity(target, hash))
                    .unwrap_or(f32::NEG_INFINITY)
            })
            .collect();
        let mut winner = 0usize;
        for (i, &s) in scores.iter().enumerate() {
            if s > scores[winner] {
                winner = i;
            }
        }
        best = scores[winner];
        centre = grid[winner];

        if pass == 0 {
            let edge = |v: f64, c: f64, half: f64| (v - c).abs() >= half - 1e-9;
            let ran_out = edge(centre.0.scale, 1.0, scale_half)
                || edge(centre.1.scale, 1.0, scale_half)
                || edge(centre.0.offset, 0.0, offset_half)
                || edge(centre.1.offset, 0.0, offset_half);
            boundary = ran_out.then_some(centre);
        }

        // One coarse step either side of the winner, which is where the truth
        // is if the winner is right.
        let step = |half: f64| 2.0 * half / (steps - 1) as f64;
        (scale_half, offset_half) = (step(scale_half), step(offset_half));
        steps = REFINE.0;
    }
    Search {
        map: centre,
        score: best,
        boundary,
    }
}

/// One eligible slot pair: the same drawn image on both sides.
#[derive(Debug, Clone, Copy)]
struct Eligible {
    a_page: usize,
    b_page: usize,
    paired: f32,
}

/// Which paired slots name a whole drawn image on both sides.
///
/// **The reader draws whole pages.** `taiyaku-serve` has one page route and it
/// returns the archive's bytes; `tiles.ts` collapses a joined spread to a
/// single wide tile spanning both its slots. So the two frames a swap maps
/// between are two whole page images, and a slot pair only names such a pair
/// when both sides sit the same way inside their page: both whole, or both the
/// first half of a joined spread — in which case the two wide pages correspond
/// as wholes and the second half is the same image again.
///
/// Skipping `Half::Left` is the convention `corpus.rs`'s star check already
/// uses for the same reason: it counts each page once. A slot paired against
/// the *other* kind — a whole page against a spread half — is counted and
/// excluded rather than dropped in silence, because it is a real thing the
/// corpus contains and no per-frame transform is defined for it.
fn eligible(
    runs: &[taiyaku_core::Run],
    a_slots: &[taiyaku_core::Slot],
    b_slots: &[taiyaku_core::Slot],
    a_hashes: &[PageHash],
    b_hashes: &[PageHash],
) -> (Vec<Eligible>, usize, usize, usize) {
    let (mut found, mut paired, mut mismatched, mut blank) = (Vec::new(), 0, 0, 0);
    for (i, a) in a_slots.iter().enumerate() {
        let Some(j) = taiyaku_core::counterpart(runs, i as u32) else {
            continue;
        };
        let Some(b) = b_slots.get(j as usize) else {
            continue;
        };
        paired += 1;
        if a.half == Some(Half::Left) {
            continue;
        }
        if a.half != b.half {
            mismatched += 1;
            continue;
        }
        // `notes.md`, "Blank pages match each other perfectly and lie about
        // it": a dhash of a blank page is exactly zero in every edition, so
        // every blank page matches every other one and a grid search over such
        // a pair is measuring nothing. Counted, not silently dropped.
        if a_hashes[i].0 == [0u64; 4] || b_hashes[j as usize].0 == [0u64; 4] {
            blank += 1;
            continue;
        }
        found.push(Eligible {
            a_page: a.page as usize,
            b_page: b.page as usize,
            paired: similarity(a_hashes[i], b_hashes[j as usize]),
        });
    }
    (found, paired, mismatched, blank)
}

/// `count` items spread evenly through `items`, or all of them when there are
/// fewer.
///
/// Evenly spaced rather than the first `count`: the front of a volume is colour
/// inserts, a title page and a table of contents, which is the least
/// representative stretch of any manga volume and the one most likely to differ
/// in framing between a raw and a licensed release.
fn spread_sample<T: Copy>(items: &[T], count: usize) -> Vec<T> {
    if items.len() <= count {
        return items.to_vec();
    }
    (0..count)
        .map(|i| items[i * items.len() / count + items.len() / (2 * count)])
        .collect()
}

/// min, median, 90th percentile, max of a sample.
///
/// The distribution rather than the mean. The tail is the interesting part, for
/// the same reason `margin` is stored as the weakest per-page value in a run
/// rather than the average: one slot that lands a reader on the wrong panel is
/// a defect a good average hides.
///
/// The percentile is nearest-rank, which is the only definition that returns a
/// value the sample actually contains. Interpolating between two neighbours
/// would report a residual no page had.
///
/// An empty sample reports NaN rather than panicking. That became reachable
/// once every distribution below is taken over the slots whose search settled:
/// a volume all of whose slots ended on the search's own boundary has nothing
/// to report, and losing twelve volumes' run to an index panic on the
/// thirteenth column of one of them is the wrong trade for a measurement.
fn distribution(sample: &[f64]) -> (f64, f64, f64, f64) {
    if sample.is_empty() {
        return (f64::NAN, f64::NAN, f64::NAN, f64::NAN);
    }
    let mut v = sample.to_vec();
    v.sort_by(|a, b| a.partial_cmp(b).expect("residuals are finite"));
    let at = |q: f64| v[(((v.len() as f64) * q).ceil() as usize).clamp(1, v.len()) - 1];
    (v[0], at(0.5), at(0.9), v[v.len() - 1])
}

/// Decode one page of an already-read edition down to the working size.
fn drawn(pages: &[taiyaku_read::Page], index: usize) -> Result<(GrayImage, (u32, u32)), String> {
    let page = pages
        .get(index)
        .ok_or_else(|| format!("page {index} is past the end of the edition"))?;
    let full = decode_luma(&page.bytes).map_err(|e| format!("{}: {e}", page.name))?;
    let frame = full.dimensions();
    Ok((working(&full), frame))
}

/// Everything one volume contributed.
struct Volume {
    name: &'static str,
    frames: ((u32, u32), (u32, u32)),
    /// The flat border each edition carries, as a fraction of that edition's
    /// page: `(horizontal, vertical)` per side.
    ///
    /// A diagnostic, not an input. `fingerprint::common_border` is what the
    /// aligner takes off a page *before* hashing it, and the server takes off
    /// nothing — it returns the archive's bytes — so any bar it finds is a
    /// framing difference the reader still sees and the residuals below still
    /// contain. Printed so that a horizontal residual can be checked against
    /// the one physical cause the corpus has already recorded for one: Tongari
    /// v09's letterbox, `notes.md` under `common_border`.
    borders: ((f64, f64), (f64, f64)),
    sampled: usize,
    available: usize,
    paired: usize,
    mismatched: usize,
    blank: usize,
    rows: Vec<Measured>,
}

impl Volume {
    /// The median of one figure over the rows a predicate keeps.
    ///
    /// The predicate is always "the search this figure comes from found a
    /// minimum", and it is a correction to the first run rather than a
    /// refinement of it: the first run printed the count of boundary slots and
    /// then included them in every figure it reported. A row whose search ran
    /// out of room contributes a bound where the table's header promises a
    /// measurement. Applied per search rather than once for all of them — the
    /// four searches of a slot fail independently, and excluding a control
    /// reading because the *forward* search failed would filter one measurement
    /// by another's result.
    fn median_where(&self, keep: impl Fn(&Measured) -> bool, f: impl Fn(&Measured) -> f64) -> f64 {
        let v: Vec<f64> = self.rows.iter().filter(|m| keep(m)).map(f).collect();
        distribution(&v).1
    }

    /// The forward search's own figures.
    fn median(&self, f: impl Fn(&Measured) -> f64) -> f64 {
        self.median_where(Measured::settled, f)
    }

    /// Figures combining the forward and reverse searches, which need both.
    fn median_both(&self, f: impl Fn(&Measured) -> f64) -> f64 {
        self.median_where(Measured::settled_both, f)
    }

    fn settled(&self) -> impl Iterator<Item = &Measured> {
        self.rows.iter().filter(|m| m.settled())
    }

    /// How much one figure moves *within* this volume, min to max.
    ///
    /// The question the per-volume table exists to answer. A framing difference
    /// between two editions is a property of the pair, so a real one is the same
    /// on every page and this spread is near zero; a number the search invented
    /// per slot scatters and this spread is as large as the figure itself. No
    /// derived transform computed from two frames can correct something that
    /// moves page to page, so which of the two it is decides whether the
    /// residual is a miss or a floor.
    fn spread(&self, f: impl Fn(&Measured) -> f64) -> f64 {
        let (lo, _, _, hi) = distribution(&self.settled().map(f).collect::<Vec<_>>());
        hi - lo
    }
}

fn measure(root: &Path, name: &'static str, cache: &Path) -> Result<Volume, String> {
    let volume = root.join(name);
    let a_members: Vec<PathBuf> = members(&volume.join(FROM))
        .iter()
        .map(PathBuf::from)
        .collect();
    let b_members: Vec<PathBuf> = members(&volume.join(TO))
        .iter()
        .map(PathBuf::from)
        .collect();

    let (a_hashes, a_slots, _) = cached_edition(cache, &a_members, READING)?;
    let (b_hashes, b_slots, _) = cached_edition(cache, &b_members, READING)?;
    let runs = solve(&a_hashes, &b_hashes);
    let (found, paired, mismatched, blank) =
        eligible(&runs, &a_slots, &b_slots, &a_hashes, &b_hashes);
    let chosen = spread_sample(&found, SAMPLE);

    let a_pages = taiyaku_read::read_edition(&a_members)?;
    let b_pages = taiyaku_read::read_edition(&b_members)?;

    let mut rows = Vec::new();
    let mut frames = ((0, 0), (0, 0));
    let mut borders = ((0.0, 0.0), (0.0, 0.0));
    for pick in &chosen {
        let (a_work, a_frame) = drawn(&a_pages, pick.a_page)?;
        let (b_work, b_frame) = drawn(&b_pages, pick.b_page)?;
        frames = (a_frame, b_frame);

        let Some(target) = window_hash(&a_work, Axis::IDENTITY, Axis::IDENTITY) else {
            return Err(format!(
                "{name}: {FROM} page {} is too small to window",
                pick.a_page
            ));
        };
        // The same window taken on the other page, which is what the reverse
        // search matches. Refused rather than defaulted for the same reason the
        // forward one is: a page too small to window is a page this experiment
        // cannot measure, and measuring it anyway would put a number in the
        // table that came from somewhere else.
        let Some(back) = window_hash(&b_work, Axis::IDENTITY, Axis::IDENTITY) else {
            return Err(format!(
                "{name}: {TO} page {} is too small to window",
                pick.b_page
            ));
        };
        let forward = best_map(target, &b_work);
        // The falsification, on the same two pages and in the same pass. Same
        // slots rather than a fresh sample of the reverse pairing: a second
        // sample would differ in which pages it drew as well as in direction,
        // and then a scale that failed to invert could be blamed on the draw.
        let reverse = best_map(back, &a_work);

        // Candidate 2, stated arithmetically. `web/src/viewport.ts`'s `mapRect`
        // takes the content aspect as the *max* of the two frames', so the
        // horizontal box is full width in both and `x`/`w` come out unchanged;
        // the vertical scale that survives is exactly `aTo / aFrom`. Derived
        // here from the same two numbers the browser has — the drawn images'
        // natural dimensions — and nothing else.
        let aspect = |(w, h): (u32, u32)| f64::from(w) / f64::from(h);
        let letterbox = (
            Axis::IDENTITY,
            Axis {
                scale: aspect(b_frame) / aspect(a_frame),
                offset: 0.0,
            },
        );
        let score = |m: (Axis, Axis)| {
            window_hash(&b_work, m.0, m.1)
                .map(|h| similarity(target, h))
                .unwrap_or(f32::NAN)
        };
        // The third candidate, and the reason it exists. The spec named two,
        // and the first run over the corpus put `scale y of best` at 0.996 —
        // identity to within the search's own tick — while `scale x of best`
        // tracked `aFrom / aTo` across all twelve volumes. That is the
        // letterbox assumption with its axes swapped: the shared artwork is as
        // *tall* as the taller page says, and the wider-aspect page carries its
        // extra as margin at left and right. In `mapRect`'s terms it is one
        // token — `Math.max` over the two aspects becomes `Math.min` — and the
        // arithmetic falls out the same way, with `cw` doing what `ch` did.
        //
        // Measured rather than argued in, and measured against the same best
        // map and the same control as the other two, so it wins or loses on the
        // same terms. It was not in the spec because nothing had looked yet.
        let pillarbox = (
            Axis {
                scale: aspect(a_frame) / aspect(b_frame),
                offset: 0.0,
            },
            Axis::IDENTITY,
        );
        let control = best_map(target, &reaspected(&a_work, aspect(b_frame)));
        // The reverse direction's own floor. Built from the *other* page, so
        // the reverse run stands on a control of its own rather than borrowing
        // the forward one's — the artefact it is meant to exclude is a property
        // of a crop's shape, and the two directions crop different pages.
        let reverse_control = best_map(back, &reaspected(&b_work, aspect(a_frame)));
        rows.push(Measured {
            a_page: pick.a_page,
            b_page: pick.b_page,
            paired: pick.paired,
            best_score: forward.score,
            identity_score: score((Axis::IDENTITY, Axis::IDENTITY)),
            letterbox_score: score(letterbox),
            pillarbox_score: score(pillarbox),
            best: forward.map,
            letterbox,
            pillarbox,
            control: control.map,
            control_score: control.score,
            reverse: reverse.map,
            reverse_score: reverse.score,
            reverse_control: reverse_control.map,
            reverse_control_score: reverse_control.score,
            forward_boundary: forward.boundary,
            reverse_boundary: reverse.boundary,
            control_boundary: control.boundary.is_some(),
            reverse_control_boundary: reverse_control.boundary.is_some(),
        });
    }

    // After the loop, so the frames the borders are divided by are the ones
    // actually measured. Both are per-edition constants — `common_border` is
    // sampled across the member and `notes.md` says why it must be — so one
    // reading each is the whole story.
    if !rows.is_empty() {
        let fraction = |b: crate::fingerprint::Border, (w, h): (u32, u32)| {
            (
                f64::from(b.left.max(b.right)) / f64::from(w),
                f64::from(b.top.max(b.bottom)) / f64::from(h),
            )
        };
        borders = (
            fraction(crate::fingerprint::common_border(&a_pages), frames.0),
            fraction(crate::fingerprint::common_border(&b_pages), frames.1),
        );
    }

    Ok(Volume {
        name,
        frames,
        borders,
        sampled: chosen.len(),
        available: found.len(),
        paired,
        mismatched,
        blank,
        rows,
    })
}

/// Which derived transform a swap should use, measured over the corpus.
///
/// See the module header for what is asserted and what deliberately is not.
#[test]
fn geometry_experiment_measures_the_derived_transform() {
    let Some(root) = corpus() else {
        eprintln!("TAIYAKU_CORPUS unset; skipping");
        return;
    };
    // Beside the corpus, not in the working directory — which for
    // `cargo test -p` is the crate directory. Same cache `align` and the star
    // check already share, and the same reasoning `corpus.rs` gives for it.
    let cache = root
        .parent()
        .unwrap_or(Path::new("."))
        .join(".taiyaku-cache");

    let started = std::time::Instant::now();
    // Progress as it happens, not only at the end. This reads and decodes both
    // editions of twelve volumes; a run that prints nothing for minutes is
    // indistinguishable from a run that has hung, and the table below cannot be
    // printed until every volume is in.
    let measured: Vec<Volume> = VOLUMES
        .iter()
        .map(|name| {
            eprintln!("  {name}: reading {FROM} and {TO}…");
            let volume =
                measure(&root, name, &cache).unwrap_or_else(|e| panic!("measuring {name}: {e}"));
            eprintln!(
                "  {name}: {} of {} eligible slots measured, {:.0}s elapsed",
                volume.sampled,
                volume.available,
                started.elapsed().as_secs_f64()
            );
            volume
        })
        .collect();

    // The window is what the search actually saw; the page is the same two
    // affine maps compared at the page edges. Both are printed because they
    // answer different questions — "how wrong was it where I looked" and "how
    // wrong could it be anywhere on the page" — and the second extrapolates the
    // fitted map past the region any pixel was compared in, which is fair for
    // two affine maps and worth saying out loud.
    const PAGE: (f64, f64) = (0.0, 1.0);

    println!("\n=== L1 geometry: which derived transform, over the corpus ===");
    println!(
        "{FROM} -> {TO} and {TO} -> {FROM} on the same slots, {} volumes, up to {SAMPLE} slots \
         each,\nscored with taiyaku_core::similarity over crate::fingerprint::fingerprint",
        VOLUMES.len()
    );
    println!(
        "window: central {:.0}% of the page | search: scale 1±{:.2}, offset ±{:.2}, \
         {} passes | working height {WORK}px",
        (WINDOW.1 - WINDOW.0) * 100.0,
        COARSE.0,
        COARSE.1,
        REFINE.1 + 1
    );

    let id = |m: &Measured| m.residual((Axis::IDENTITY, Axis::IDENTITY), WINDOW);
    let lb = |m: &Measured| m.residual(m.letterbox, WINDOW);
    let pb = |m: &Measured| m.residual(m.pillarbox, WINDOW);
    // The control's residual is measured the same way, against the answer that
    // is known in advance to be right — so it is directly comparable with the
    // two above, and it is the line below which none of them means anything.
    let ctl = |m: &Measured, span: (f64, f64)| {
        (
            Axis::IDENTITY.apart(m.control.0, span),
            Axis::IDENTITY.apart(m.control.1, span),
        )
    };

    println!("\n-- the pages, and the framing difference the aligner already knows about --");
    println!(
        "{:<14} {:>11} {:>11} {:>7} {:>7} {:>13} {:>13}",
        "volume", "raw frame", "en frame", "k(lb)", "sampled", "raw flat bar", "en flat bar"
    );
    for volume in &measured {
        println!(
            "{:<14} {:>5}x{:<5} {:>5}x{:<5} {:>7.3} {:>3}/{:<3} {:>6.3},{:<6.3} {:>6.3},{:<6.3}",
            volume.name,
            volume.frames.0.0,
            volume.frames.0.1,
            volume.frames.1.0,
            volume.frames.1.1,
            volume.median(|m| m.letterbox.1.scale),
            volume.sampled,
            volume.available,
            volume.borders.0.0,
            volume.borders.0.1,
            volume.borders.1.0,
            volume.borders.1.1,
        );
    }

    println!(
        "\n-- the best map found, per volume: median, and how far it moves within the volume --"
    );
    println!(
        "{:<14} {:>8} {:>8} {:>8} {:>8} {:>9} {:>9}",
        "volume", "scale x", "offset x", "scale y", "offset y", "spread ox", "spread oy"
    );
    for volume in &measured {
        println!(
            "{:<14} {:>8.4} {:>8.4} {:>8.4} {:>8.4} {:>9.4} {:>9.4}",
            volume.name,
            volume.median(|m| m.best.0.scale),
            volume.median(|m| m.best.0.offset),
            volume.median(|m| m.best.1.scale),
            volume.median(|m| m.best.1.offset),
            volume.spread(|m| m.best.0.offset),
            volume.spread(|m| m.best.1.offset),
        );
    }

    println!("\n-- the control: the same search where identity is known to be the answer --");
    println!(
        "{:<14} {:>9} {:>9} {:>9} {:>9} {:>9} {:>9}",
        "volume", "scale x", "scale y", "sx/sy", "1/k(lb)", "ctl dx", "ctl dy"
    );
    // The control's own rows, kept or dropped by whether the *control* search
    // settled — not by whether the forward one did.
    let ctl_found = |m: &Measured| !m.control_boundary;
    for volume in &measured {
        let sx = volume.median_where(ctl_found, |m| m.control.0.scale);
        let sy = volume.median_where(ctl_found, |m| m.control.1.scale);
        println!(
            "{:<14} {:>9.4} {:>9.4} {:>9.4} {:>9.4} {:>9.4} {:>9.4}",
            volume.name,
            sx,
            sy,
            sx / sy,
            1.0 / volume.median(|m| m.letterbox.1.scale),
            volume.median_where(ctl_found, |m| ctl(m, WINDOW).0),
            volume.median_where(ctl_found, |m| ctl(m, WINDOW).1),
        );
    }

    // The falsification, and it gets the widest table because it is the one
    // number the horizontal recommendation now rests on. `1/fwd sx` is the
    // prediction, printed beside `rev sx` rather than asserted about it, so
    // agreement or disagreement is visible per volume rather than summarised.
    println!("\n-- the falsification: the same search, the same slots, the other way round --");
    println!(
        "{:<14} {:>8} {:>8} {:>8} {:>9} {:>8} {:>8} {:>8} {:>8}",
        "volume", "fwd sx", "1/fwd sx", "rev sx", "aTo/aFrom", "fx*rx", "fwd sy", "rev sy", "fy*ry"
    );
    for volume in &measured {
        let fwd = volume.median_both(|m| m.best.0.scale);
        println!(
            "{:<14} {:>8.4} {:>8.4} {:>8.4} {:>9.4} {:>8.4} {:>8.4} {:>8.4} {:>8.4}",
            volume.name,
            fwd,
            1.0 / fwd,
            volume.median_both(|m| m.reverse.0.scale),
            volume.median_both(|m| m.letterbox.1.scale),
            volume.median_both(|m| m.best.0.scale * m.reverse.0.scale),
            volume.median_both(|m| m.best.1.scale),
            volume.median_both(|m| m.reverse.1.scale),
            volume.median_both(|m| m.best.1.scale * m.reverse.1.scale),
        );
    }

    println!("\n-- the reverse direction's own control, and what each candidate cost it --");
    println!(
        "{:<14} {:>9} {:>9} {:>9} {:>9} {:>9} {:>9}",
        "volume", "ctl sx", "ctl sy", "ctl sx/sy", "rev id dx", "rev pb dx", "rev id dy"
    );
    // The reverse direction's candidates are the forward ones inverted, which
    // is what "derived from the two frames" means when the frames swap roles:
    // `pillarbox` scales x by `aFrom/aTo` going one way and by `aTo/aFrom`
    // coming back. Derived from the stored maps rather than re-read from the
    // frames, so a sign error could not agree with itself twice.
    let rev_id = |m: &Measured, span: (f64, f64)| {
        (
            Axis::IDENTITY.apart(m.reverse.0, span),
            Axis::IDENTITY.apart(m.reverse.1, span),
        )
    };
    let rev_pb = |m: &Measured, span: (f64, f64)| {
        Axis {
            scale: 1.0 / m.pillarbox.0.scale,
            offset: 0.0,
        }
        .apart(m.reverse.0, span)
    };
    let rev_ctl = |m: &Measured, span: (f64, f64)| {
        (
            Axis::IDENTITY.apart(m.reverse_control.0, span),
            Axis::IDENTITY.apart(m.reverse_control.1, span),
        )
    };
    let rev_ctl_found = |m: &Measured| !m.reverse_control_boundary;
    for volume in &measured {
        let sx = volume.median_where(rev_ctl_found, |m| m.reverse_control.0.scale);
        let sy = volume.median_where(rev_ctl_found, |m| m.reverse_control.1.scale);
        println!(
            "{:<14} {:>9.4} {:>9.4} {:>9.4} {:>9.4} {:>9.4} {:>9.4}",
            volume.name,
            sx,
            sy,
            sx / sy,
            volume.median_both(|m| rev_id(m, WINDOW).0),
            volume.median_both(|m| rev_pb(m, WINDOW)),
            volume.median_both(|m| rev_id(m, WINDOW).1),
        );
    }

    println!("\n-- what each candidate cost, per volume: median residual within the window --");
    println!(
        "{:<14} {:>7} {:>7} {:>7} {:>7} {:>7} {:>7} {:>7} {:>7} {:>7}",
        "volume", "s(best)", "s(id)", "s(lb)", "s(pb)", "id dx", "lb dx", "pb dx", "id dy", "lb dy"
    );
    let (mut sampled, mut available, mut paired, mut mismatched, mut blank) = (0, 0, 0, 0, 0);
    for volume in &measured {
        println!(
            "{:<14} {:>7.3} {:>7.3} {:>7.3} {:>7.3} {:>7.4} {:>7.4} {:>7.4} {:>7.4} {:>7.4}",
            volume.name,
            volume.median(|m| f64::from(m.best_score)),
            volume.median(|m| f64::from(m.identity_score)),
            volume.median(|m| f64::from(m.letterbox_score)),
            volume.median(|m| f64::from(m.pillarbox_score)),
            volume.median(|m| id(m).0),
            volume.median(|m| lb(m).0),
            volume.median(|m| pb(m).0),
            volume.median(|m| id(m).1),
            volume.median(|m| lb(m).1),
        );

        sampled += volume.sampled;
        available += volume.available;
        paired += volume.paired;
        mismatched += volume.mismatched;
        blank += volume.blank;
    }

    println!(
        "\nsampled {sampled} of {available} eligible slots out of {paired} paired \
         across {} volumes",
        measured.len()
    );
    println!(
        "excluded: {mismatched} slots paired whole-page against spread-half, {blank} blank-page slots"
    );
    // Every distribution below is over the slots whose search *settled* — the
    // correction this run makes to the first one, which counted its boundary
    // slots and then reported them anyway. A search that stops on the edge of
    // its own range has not found a minimum; it has run out of room, and its
    // answer is a bound. `every_row` exists only to print the difference the
    // exclusion made, so that "restated" is a number rather than a claim.
    let every = |f: &dyn Fn(&Measured) -> f64| -> Vec<f64> {
        measured
            .iter()
            .flat_map(|v| v.rows.iter().filter(|m| m.settled()).map(f))
            .collect()
    };
    let every_both = |f: &dyn Fn(&Measured) -> f64| -> Vec<f64> {
        measured
            .iter()
            .flat_map(|v| v.rows.iter().filter(|m| m.settled_both()).map(f))
            .collect()
    };
    let every_ctl = |f: &dyn Fn(&Measured) -> f64| -> Vec<f64> {
        measured
            .iter()
            .flat_map(|v| v.rows.iter().filter(|m| ctl_found(m)).map(f))
            .collect()
    };
    let every_rev_ctl = |f: &dyn Fn(&Measured) -> f64| -> Vec<f64> {
        measured
            .iter()
            .flat_map(|v| v.rows.iter().filter(|m| rev_ctl_found(m)).map(f))
            .collect()
    };
    let every_row = |f: &dyn Fn(&Measured) -> f64| -> Vec<f64> {
        measured.iter().flat_map(|v| v.rows.iter().map(f)).collect()
    };
    let count = |keep: &dyn Fn(&Measured) -> bool| -> usize {
        measured
            .iter()
            .flat_map(|v| v.rows.iter())
            .filter(|m| keep(m))
            .count()
    };
    let (settled, settled_both) = (count(&|m| m.settled()), count(&|m| m.settled_both()));

    println!("\n-- slots where a search ran out of room rather than finding a minimum --");
    println!(
        "coarse boundary: forward {} of {sampled}, reverse {} of {sampled}, \
         control {} of {sampled}, reverse control {} of {sampled}",
        count(&|m| !m.settled()),
        count(&|m| m.reverse_boundary.is_some()),
        count(&|m| m.control_boundary),
        count(&|m| m.reverse_control_boundary),
    );
    println!("reported over {settled} settled forward slots, {settled_both} settled both ways");
    // Named, not just counted: the coarse winner says which of the four
    // parameters hit which edge, which is the difference between "the truth is
    // just outside the box" and "this page has no peak to find".
    for volume in &measured {
        for m in &volume.rows {
            for (which, at) in [
                ("forward", m.forward_boundary),
                ("reverse", m.reverse_boundary),
            ] {
                let Some((h, v)) = at else { continue };
                let edge = |x: f64, c: f64, half: f64| {
                    if (x - c).abs() >= half - 1e-9 {
                        if x > c { " HI" } else { " LO" }
                    } else {
                        "  ."
                    }
                };
                println!(
                    "  {:<14} {which:<7} {FROM} p{:<4} {TO} p{:<4} sx {:>7.4}{} ox {:>7.4}{} \
                     sy {:>7.4}{} oy {:>7.4}{} paired {:.3}",
                    volume.name,
                    m.a_page,
                    m.b_page,
                    h.scale,
                    edge(h.scale, 1.0, COARSE.0),
                    h.offset,
                    edge(h.offset, 0.0, COARSE.1),
                    v.scale,
                    edge(v.scale, 1.0, COARSE.0),
                    v.offset,
                    edge(v.offset, 0.0, COARSE.1),
                    m.paired,
                );
            }
        }
    }

    println!("\nresidual, fraction of the target page — WITHIN THE MEASURED WINDOW");
    println!(
        "{:<22} {:>8} {:>8} {:>8} {:>8}",
        "", "min", "median", "p90", "max"
    );
    let row = |label: &str, v: Vec<f64>| {
        let (lo, med, p90, hi) = distribution(&v);
        println!("{label:<22} {lo:>8.4} {med:>8.4} {p90:>8.4} {hi:>8.4}");
    };
    row("identity  dx", every(&|m| id(m).0));
    row("identity  dy", every(&|m| id(m).1));
    row("letterbox dx", every(&|m| lb(m).0));
    row("letterbox dy", every(&|m| lb(m).1));
    row("pillarbox dx", every(&|m| pb(m).0));
    row("pillarbox dy", every(&|m| pb(m).1));
    row("CONTROL   dx", every_ctl(&|m| ctl(m, WINDOW).0));
    row("CONTROL   dy", every_ctl(&|m| ctl(m, WINDOW).1));

    println!(
        "\nthe same, over all {sampled} slots — how the first run reported it, \
         boundary slots included"
    );
    row("identity  dx", every_row(&|m| id(m).0));
    row("identity  dy", every_row(&|m| id(m).1));
    row("letterbox dx", every_row(&|m| lb(m).0));
    row("letterbox dy", every_row(&|m| lb(m).1));
    row("pillarbox dx", every_row(&|m| pb(m).0));
    row("pillarbox dy", every_row(&|m| pb(m).1));
    row("CONTROL   dx", every_row(&|m| ctl(m, WINDOW).0));
    row("CONTROL   dy", every_row(&|m| ctl(m, WINDOW).1));

    println!("\nresidual, fraction of the target page — EXTRAPOLATED TO THE WHOLE PAGE");
    row(
        "identity  dx",
        every(&|m| m.residual((Axis::IDENTITY, Axis::IDENTITY), PAGE).0),
    );
    row(
        "identity  dy",
        every(&|m| m.residual((Axis::IDENTITY, Axis::IDENTITY), PAGE).1),
    );
    row("letterbox dx", every(&|m| m.residual(m.letterbox, PAGE).0));
    row("letterbox dy", every(&|m| m.residual(m.letterbox, PAGE).1));
    row("pillarbox dx", every(&|m| m.residual(m.pillarbox, PAGE).0));
    row("pillarbox dy", every(&|m| m.residual(m.pillarbox, PAGE).1));
    row("CONTROL   dx", every_ctl(&|m| ctl(m, PAGE).0));
    row("CONTROL   dy", every_ctl(&|m| ctl(m, PAGE).1));

    // Signed, and that is the point. An absolute value cannot tell a framing
    // difference that is the same on every page — which a derived transform
    // could correct — from one that changes sign page to page, which none of
    // these can and which therefore sets a floor rather than naming a miss.
    println!("\nthe best map itself, SIGNED — what the pixels asked for");
    row("scale x of best", every(&|m| m.best.0.scale));
    row("offset x of best", every(&|m| m.best.0.offset));
    row("scale y of best", every(&|m| m.best.1.scale));
    row("offset y of best", every(&|m| m.best.1.offset));
    row("scale x of letterbox", every(&|m| m.letterbox.0.scale));
    row("scale y of letterbox", every(&|m| m.letterbox.1.scale));
    row("scale x of pillarbox", every(&|m| m.pillarbox.0.scale));
    row("scale y of pillarbox", every(&|m| m.pillarbox.1.scale));
    row("scale x of CONTROL", every_ctl(&|m| m.control.0.scale));
    row("scale y of CONTROL", every_ctl(&|m| m.control.1.scale));

    // The falsification in one distribution. A real geometric relation between
    // two pages inverts when they swap roles, so each product is 1; a
    // preference belonging to the metric or the search does not know which page
    // is which, returns the same side of 1 both ways, and each product is the
    // square of the offending scale. The corpus's aspect ratios put that square
    // between 0.90 and 1.13, so the two hypotheses are far apart in this row —
    // which is the only reason one run of it settles anything.
    println!(
        "\nthe reverse map, and the reciprocal check — over {settled_both} slots settled both ways"
    );
    row("scale x of reverse", every_both(&|m| m.reverse.0.scale));
    row("scale y of reverse", every_both(&|m| m.reverse.1.scale));
    row(
        "sx fwd * sx reverse",
        every_both(&|m| m.best.0.scale * m.reverse.0.scale),
    );
    row(
        "sy fwd * sy reverse",
        every_both(&|m| m.best.1.scale * m.reverse.1.scale),
    );
    // What the reverse scale would be if the search were reading aspect rather
    // than artwork: the same side of 1 as the forward one, so the product is
    // its square. Printed beside the measured product so the alternative the
    // result beats is on the page rather than in the prose.
    row(
        "sx fwd squared (if bias)",
        every_both(&|m| m.best.0.scale * m.best.0.scale),
    );
    row(
        "scale x of rev CONTROL",
        every_rev_ctl(&|m| m.reverse_control.0.scale),
    );
    row(
        "scale y of rev CONTROL",
        every_rev_ctl(&|m| m.reverse_control.1.scale),
    );

    println!("\nresidual of the reverse direction's candidates, within the window");
    row("rev identity  dx", every_both(&|m| rev_id(m, WINDOW).0));
    row("rev identity  dy", every_both(&|m| rev_id(m, WINDOW).1));
    row("rev pillarbox dx", every_both(&|m| rev_pb(m, WINDOW)));
    row("rev CONTROL   dx", every_rev_ctl(&|m| rev_ctl(m, WINDOW).0));
    row("rev CONTROL   dy", every_rev_ctl(&|m| rev_ctl(m, WINDOW).1));

    println!("\nsimilarity reached, in the solver's own currency");
    row("whole page, as paired", every(&|m| f64::from(m.paired)));
    row("window, best map", every(&|m| f64::from(m.best_score)));
    row("window, identity", every(&|m| f64::from(m.identity_score)));
    row(
        "window, letterbox",
        every(&|m| f64::from(m.letterbox_score)),
    );
    row(
        "window, pillarbox",
        every(&|m| f64::from(m.pillarbox_score)),
    );
    row(
        "window, control best",
        every_ctl(&|m| f64::from(m.control_score)),
    );
    row(
        "window, reverse best",
        every_both(&|m| f64::from(m.reverse_score)),
    );
    row(
        "window, rev control",
        every_rev_ctl(&|m| f64::from(m.reverse_control_score)),
    );

    println!("\nmeasured in {:.1}s", started.elapsed().as_secs_f64());

    // The floor, and only the floor. Nothing here bounds a residual: that is
    // what the promotion rule decides, now that there is a number to bound.
    assert!(
        sampled > 0,
        "the sample is empty: no volume matched, or no slot paired"
    );
    for volume in &measured {
        assert!(
            volume.sampled > 0,
            "{} contributed nothing: {} eligible of {} paired",
            volume.name,
            volume.available,
            volume.paired
        );
        // Now that the tables are taken over settled slots, a volume with none
        // prints NaN across its row. Silent NaN in a table of measurements is
        // the same failure as an empty table, so it is asserted rather than
        // read: this is a floor on the sample, not on any residual.
        assert!(
            volume.rows.iter().any(Measured::settled_both),
            "{}: every one of {} slots ended a search on its own coarse boundary",
            volume.name,
            volume.sampled
        );
    }
    for volume in &measured {
        for (i, m) in volume.rows.iter().enumerate() {
            let finite = |(x, y): (f64, f64)| x.is_finite() && y.is_finite();
            assert!(
                finite(id(m))
                    && finite(lb(m))
                    && finite(pb(m))
                    && finite(ctl(m, WINDOW))
                    && finite(m.residual((Axis::IDENTITY, Axis::IDENTITY), PAGE))
                    && finite(m.residual(m.letterbox, PAGE))
                    && finite(m.residual(m.pillarbox, PAGE))
                    && finite(ctl(m, PAGE)),
                "{} slot {i} produced a non-finite residual: {m:?}",
                volume.name
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The parameterisation both candidates are expressed in. Identity has to
    /// be a genuine no-op, and the offset has to leave the page centre alone —
    /// which is the property that keeps the one number the candidates disagree
    /// about out of the one they agree on.
    #[test]
    fn an_axis_is_centred_so_scale_alone_fixes_the_middle() {
        assert!((Axis::IDENTITY.at(0.3) - 0.3).abs() < 1e-12);
        let scaled = Axis {
            scale: 1.2,
            offset: 0.0,
        };
        assert!((scaled.at(0.5) - 0.5).abs() < 1e-12, "the centre is fixed");
        assert!(
            (scaled.at(1.0) - 1.1).abs() < 1e-12,
            "a 20% stretch pushes the bottom edge a tenth of a page off the page"
        );
        assert!(
            (scaled.apart(Axis::IDENTITY, (0.0, 1.0)) - 0.1).abs() < 1e-12,
            "a 20% stretch moves a page edge by a tenth of a page"
        );
    }

    /// Nearest-rank, so every figure printed is a residual some slot actually
    /// had. An interpolating percentile would report one none did.
    #[test]
    fn the_distribution_reports_values_the_sample_contains() {
        let sample: Vec<f64> = (1..=10).map(f64::from).collect();
        assert_eq!(distribution(&sample), (1.0, 5.0, 9.0, 10.0));
        assert_eq!(distribution(&[3.0]), (3.0, 3.0, 3.0, 3.0));
    }

    /// Reachable since the tables are taken over the slots whose search
    /// settled: a volume with none has nothing to report. NaN in one cell,
    /// rather than an index panic that loses the other eleven volumes' hour.
    #[test]
    fn an_empty_sample_reports_nothing_rather_than_panicking() {
        let (lo, med, p90, hi) = distribution(&[]);
        assert!(lo.is_nan() && med.is_nan() && p90.is_nan() && hi.is_nan());
    }

    /// The sample must not be the front of the volume, which is colour inserts
    /// and front matter, and must never run off the end.
    #[test]
    fn the_sample_is_spread_through_the_volume_rather_than_taken_from_its_front() {
        let items: Vec<usize> = (0..100).collect();
        let picked = spread_sample(&items, 8);
        assert_eq!(picked.len(), 8);
        assert!(picked[0] > 0, "not the very first slot: {picked:?}");
        assert!(picked[7] < 100);
        assert!(picked.windows(2).all(|w| w[0] < w[1]), "{picked:?}");
        // Fewer than asked for is everything, unchanged.
        assert_eq!(spread_sample(&items[..5], 8), items[..5].to_vec());
    }

    /// A window that would run off the page is refused rather than clamped: a
    /// clamped crop is a different window scored as if it were the one asked
    /// for, which teaches a grid search to like the edge of its own range.
    #[test]
    fn a_window_that_leaves_the_page_is_refused_rather_than_clamped() {
        let page = GrayImage::from_fn(200, 300, |x, y| {
            image::Luma([((x * 7 + y * 13) % 251) as u8])
        });
        assert!(window_hash(&page, Axis::IDENTITY, Axis::IDENTITY).is_some());
        let far = Axis {
            scale: 1.0,
            offset: 0.5,
        };
        assert!(window_hash(&page, far, Axis::IDENTITY).is_none());
        assert!(window_hash(&page, Axis::IDENTITY, far).is_none());
    }
}
