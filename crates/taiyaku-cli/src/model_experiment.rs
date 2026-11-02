//! Phase 3: the model candidate, scored against the boxes a person drew — and
//! then asked whether it groups a page's balloons the same way on both editions.
//!
//! **Three questions, three `#[test]`s, one set of helpers.** Questions one and
//! two come first below; question three, the confidence sweep, is described
//! after question two's narrative because it reuses every piece of it.
//!
//! **Question one**, `the_model_candidate_scored`. The deliverable is a
//! head-to-head table — the promoted classical detector and
//! comic-text-detector's block head, on the same 45 pages, through the same
//! scorer, at the same coverage. Whether the model is promoted is a decision a
//! person makes reading that table against the rule; nothing here asserts it.
//!
//! **Question two**, `the_models_grouping_across_both_editions`. The model
//! sometimes draws **one box over two adjacent balloons** where the labeller
//! drew two. Whether that costs anything is not a labelling question, it is a
//! pairing question: L2 maps region to region, so a merge the model makes on
//! the raw **and** on the English page costs nothing — both sides agree and a
//! reader pointing at a balloon still lands on the right one. A merge on one
//! side only breaks the pairing, and **no amount of relabelling would fix it**,
//! because it is model behaviour rather than a labelling convention. So this
//! measurement puts the model against itself across the two editions of one
//! page and reads no truth boxes at all — the truth document is consulted only
//! for *which* pages somebody has looked at. It prints how often the grouping
//! agrees; like question one, it asserts no bar.
//!
//! **Question two's mechanics, and the three places they can go quietly
//! wrong.**
//!
//!   - **Page correspondence is in slots, not file pages.** `Edition::slots`
//!     expands `spreads`, so a joined double page counts as *two* slots and
//!     every slot index after it is one ahead of the file page number. On this
//!     corpus the `raw-mono` editions carry no spreads at all, so slot index
//!     equals page index on the raw side today, and the test **asserts** that
//!     rather than assuming it. A raw edition that ever gains a spread would
//!     otherwise shift the whole measurement by one page with nothing to show
//!     for it.
//!   - **A counterpart landing on a spread is skipped and counted**, not split.
//!     Which half of a joined page a raw page corresponds to is decided by
//!     reading direction, and getting it backwards would compare two unrelated
//!     pages and report the disagreement as the model's. For the handful of
//!     pages involved that risk is not worth taking; the skip count is printed
//!     beside the compared count so it can never be silent.
//!   - **Fractions correspond only approximately.** Regions are in page
//!     fractions, so no explicit transform is needed — but the two editions
//!     have different aspect ratios, so the same physical point sits at
//!     slightly different fractions on the two scans. The aspect delta is
//!     printed per pair so a reader can see how far the approximation is being
//!     stretched. **Fitting a transform is L2b's job and is deliberately not
//!     done here.**
//!
//! **Compared by centroid containment, not IoU**, because containment is what
//! pairing would actually do: a Japanese balloon holds vertical text and its
//! English counterpart horizontal text, so one balloon yields two boxes of very
//! different shape with similar centres. IoU would score that pair low and be
//! measuring the writing system rather than the grouping.
//!
//! **Twins.** Two near-identical boxes on one page (see `duplicated`) would
//! make containment read a grouping disagreement that is not there.
//! Suppression is class-agnostic — see `suppress` for the full reasoning,
//! which is `agnostic=True` on an existing YOLOv5 flag rather than a new
//! mechanism. The two headline figures — raw containment, and containment
//! after discounting twins — part only when suppression leaves twins, so
//! their agreeing is what says that artefact is gone rather than merely
//! smaller. Both are printed, and the twin line is printed, precisely so a
//! regression would show up as them parting again.
//!
//! **Question three**, `where_the_confidence_floor_goes`. `BLK_CONF` below is
//! upstream's default. It was chosen for upstream's *translation pipeline*,
//! not for this project's promotion rule. The rule's second term caps spurious
//! predictions at **10.7 per page** — the dev set's own truth-boxes-per-page —
//! and the model at the shipped floor sits far below that. Lowering the floor
//! spends precision to buy recall, and there is an enormous amount of precision
//! to spend. So this prints **one row per threshold**, losing rows included,
//! and like the other two it asserts no bar: which row wins is read off the
//! table against the rule, by a person. **`BLK_CONF` is not changed here**;
//! moving it is a separate commit.
//!
//! **`0.40` is the control arm** — the shipped value — and it is in the list on
//! purpose, exactly as `band_sweep.rs`'s `lo = 0` is: a sweep whose every option
//! is a change cannot report that the change did not help. **`0.50` is the
//! control in the other direction**, for the same reason upward. The `0.40` row
//! is marked in the output so a reader finds the incumbent without counting.
//!
//! **`BLK_IOU` is held on every row: one parameter at a time.**
//! `band_sweep::band`'s doc explains why the band sweep held `hi` fixed while
//! sweeping `lo`, and the reasoning transfers unchanged — sweeping both at once
//! would spend many times the forward passes to answer a question about a
//! constant nothing has said is wrong.
//!
//! **The Japanese `raw-mono` dialogue subtotal is printed per row**, beside the
//! pooled figures. That population is the whole reason this model is being
//! considered, and a floor that buys English recall while losing Japanese
//! would be invisible in a pooled row, which is question one's own reason for
//! its per-edition table restated.
//!
//! **This is a one-parameter sweep over a fixed dev set, which is a mild overfit
//! and is stated as one.** `band_sweep.rs` says exactly this about itself. The
//! 30 held-out pages are what would check it, and they are **not** opened here.
//!
//! **Every threshold is swept inside one forward pass per page**, which is the
//! whole reason `SWEEP` is an array inside `decode_all`'s closure rather than a
//! loop around it. The 1024×1024 inference is the expensive part and it does
//! **not** depend on the floor — `blk` is the same tensor whatever value is
//! applied to it afterwards, and filtering, suppressing and scoring are cheap
//! beside it. Re-running inference per threshold would cost eight times the
//! wall clock for identical numbers. `band_sweep.rs`'s memory reason applies on
//! top: `decode_all` bounds memory by decoding the large pages one at a time,
//! so the pixels are not available afterwards to run a second threshold over.
//!
//! **The shape is `band_sweep.rs`'s**, deliberately, rather than a fourth
//! convention: `#[cfg(test)]`, gated on `TAIYAKU_CORPUS`, skipping cleanly when
//! unset, reading its pages through `score::resolve_labelled` so it measures
//! **exactly the pages `taiyaku score` scores**, scoring through
//! `score::score_page` and `score::Report` at `score::COVERAGE`, and asserting
//! only that it measured something.
//!
//! **What the two rows are, stated precisely.** The classical row is
//! `detect::detect_page` as it ships today — the candidate promoted by L2b, not
//! the incumbent. The incumbent cannot be reproduced here: three
//! changes separate it from today's detector (the band, the two-axis union,
//! `MIN_INK` counting pixels), and `band_sweep::SWEEP`'s doc already records
//! why its `lo = 0` row is not byte-for-byte that detector either. The
//! incumbent's figure is the one `docs/status.md` records, not recomputed.
//!
//! **Pages arrive as `GrayImage` and the model wants three channels.**
//! `decode_all` decodes to luma, so the three input channels are filled with
//! the same value. Every one of the nine labelled editions is a mono scan, so
//! nothing is lost; on a colour edition this would be a real difference and it
//! is written down rather than left to be discovered.
//!
//! **The preprocessing is a port of the ONNX branch of upstream's pipeline, not
//! the torch branch**, and the two disagree. See `docs/notes.md`,
//! "comic-text-detector through `tract`", for both hazards: the letterbox is
//! **not centred** (the page goes top-left, padding on the bottom and right,
//! upstream's centring lines commented out), and the torch branch reverses the
//! channel axis a second time and feeds BGR while the ONNX branch feeds RGB.
//!
//! ```sh
//! ./scripts/fetch-model.sh
//! TAIYAKU_CORPUS="$PWD/corpus" TAIYAKU_MODEL="$PWD/models/comictextdetector.pt.onnx" \
//!   cargo test --release -p taiyaku-cli model_experiment -- --nocapture
//! ```
//!
//! **`--release` is not optional in practice**: each page pays a flood fill over
//! every pixel for the classical row and a 1024×1024 forward pass for the model
//! row.

use std::path::PathBuf;

use image::{GrayImage, imageops::FilterType};
use taiyaku_core::{Axis, Kind, Region};
use tract_onnx::prelude::*;

use crate::corpus::{corpus, members};
use crate::detect;
use crate::score::{COVERAGE, Report, resolve_labelled, score_page};

/// The model's input side. Fixed by the export, which carries no dynamic axes.
const SIDE: usize = 1024;

/// Upstream's `TextDetector.__init__` defaults for the block head:
/// `conf_thresh=0.4`, `nms_thresh=0.35`. Taken rather than tuned — tuning them
/// against this dev set would be the overfit `band_sweep.rs` already flags in
/// its own header, and a candidate that only wins at a tuned threshold is not
/// the finding phase 3 is after.
///
/// **That reasoning is about whether the model wins; it does not settle where
/// the floor goes.** `where_the_confidence_floor_goes` sweeps the floor over
/// [`SWEEP`] and prints a row each, with this value in the list as the control
/// arm. **The constant is deliberately unchanged by that test.** Moving it is a
/// person's decision read off the printed table against the rule, and a
/// separate commit; until then this is upstream's number and says so.
const BLK_CONF: f32 = 0.4;
const BLK_IOU: f64 = 0.35;

/// Confidence floors to sweep, applied to both of `candidates_at`'s gates.
///
/// **`0.40` is the control arm** — `BLK_CONF`, the shipped value — and **`0.50`
/// is the control in the other direction**. Both are in the list on purpose,
/// for `band_sweep.rs`'s `lo = 0` reason: a sweep whose every option is a change
/// cannot report that the change did not help, in either direction. Unlike that
/// file's control, `0.40` here **is** byte-for-byte the shipped path — nothing
/// else separates this row from `detect_with_model`, since `candidates` is a
/// call to `candidates_at(blk, BLK_CONF)` and `BLK_IOU` is held throughout — so
/// that row is expected to reproduce question one's `model/blk` figures exactly.
///
/// The low end goes to 0.02 because the budget is large enough to make it worth
/// asking, not because anything predicts it: 0.07 spurious per page against a
/// ceiling of 10.7 leaves room for two orders of magnitude more.
const SWEEP: [f32; 8] = [0.02, 0.05, 0.10, 0.15, 0.20, 0.30, 0.40, 0.50];

/// `blk` is `1 × 64512 × 7`: cx, cy, w, h, objectness, then two class scores.
const BLK_STRIDE: usize = 7;

/// The nine editions of four works that carry labels, named rather than
/// globbed — `truth_population.rs`'s and `band_sweep.rs`'s array, for their
/// reason: a glob that stops matching is the silent-nothing failure the
/// assertions below refuse to have.
const EDITIONS: [(&str, &str); 9] = [
    ("gotoubun-v01", "raw-mono"),
    ("gotoubun-v01", "en-mono"),
    ("gotoubun-v01", "en-fan"),
    ("kaguya-v01", "raw-mono"),
    ("kaguya-v01", "en-mono"),
    ("kobayashi-v01", "raw-mono"),
    ("kobayashi-v01", "en-mono"),
    ("oshinoko-v01", "raw-mono"),
    ("oshinoko-v01", "en-mono"),
];

/// Resize preserving aspect, pad to a square with black on the right and
/// bottom, and pack into the `1×3×SIDE×SIDE` f32 tensor the graph expects.
///
/// A port of upstream `imgproc_utils.py::letterbox(auto=False, scaleup=True)`
/// followed by `cv2.dnn.blobFromImage(scalefactor=1/255)`. Two details are
/// load-bearing and neither is the obvious choice:
///
///   - **Not centred.** `copyMakeBorder(im, 0, dh, 0, dw, ...)`, with the
///     centring lines commented out upstream. The page is at the top left.
///   - **`scaleup` is left on**, so a page smaller than `SIDE` is enlarged
///     rather than padded.
///
/// Returns the tensor and the padding, which the caller needs to turn boxes in
/// square-space back into page fractions.
fn letterbox(page: &GrayImage) -> (Tensor, usize, usize) {
    let side = u32::try_from(SIDE).expect("SIDE fits u32");
    let (w, h) = page.dimensions();

    // The smaller ratio is the one that fits both sides.
    let r = (f64::from(side) / f64::from(h)).min(f64::from(side) / f64::from(w));
    // `.clamp(1, side)` on both bounds: the upper bound stops `side - new_w`
    // (below) underflowing `usize` when f64 rounding nudges a dimension a
    // hair over `side`; the lower bound stops a zero-sized resize target on
    // an extreme aspect ratio. `dw`/`dh` are derived from these post-clamp
    // values, so the live region below is still exactly the resized page —
    // clamping cannot desync the two.
    let new_w = ((f64::from(w) * r).round() as u32).clamp(1, side);
    let new_h = ((f64::from(h) * r).round() as u32).clamp(1, side);

    let resized = image::imageops::resize(page, new_w, new_h, FilterType::Triangle);

    let mut square = GrayImage::new(side, side); // zero-filled: black, as upstream
    image::imageops::replace(&mut square, &resized, 0, 0);

    // Three identical channels: see the module header on luma.
    let tensor: Tensor =
        tract_ndarray::Array4::from_shape_fn((1, 3, SIDE, SIDE), |(_, _, y, x)| {
            let luma = square
                .get_pixel(
                    u32::try_from(x).expect("SIDE fits u32"),
                    u32::try_from(y).expect("SIDE fits u32"),
                )
                .0[0];
            f32::from(luma) / 255.0
        })
        .into();

    (tensor, (side - new_w) as usize, (side - new_h) as usize)
}

/// One decoded grid cell, in square-space pixels.
#[derive(Clone, Copy, Debug)]
struct Cand {
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
    score: f32,
}

/// Intersection over union, in square-space pixels.
fn iou(a: &Cand, b: &Cand) -> f64 {
    let w = (a.x1.min(b.x1) - a.x0.max(b.x0)).max(0.0);
    let h = (a.y1.min(b.y1) - a.y0.max(b.y0)).max(0.0);
    let overlap = w * h;
    let union = (a.x1 - a.x0) * (a.y1 - a.y0) + (b.x1 - b.x0) * (b.y1 - b.y0) - overlap;
    if union <= 0.0 { 0.0 } else { overlap / union }
}

/// Grid cells that clear `BLK_CONF`, as corner boxes.
///
/// A call to [`candidates_at`] at the shipped floor, and nothing else — the
/// two must not be able to disagree about what "the default" filters, which is
/// what a second copy of the loop would eventually allow.
fn candidates(blk: &[f32]) -> Vec<Cand> {
    candidates_at(blk, BLK_CONF)
}

/// Grid cells that clear a given confidence floor, as corner boxes.
///
/// The export decodes the grid in-graph — anchors applied, sigmoids applied,
/// coordinates already in square-space pixels — so this is a filter, not a
/// decode. Objectness gates first and then objectness × class, which is what
/// YOLOv5's own `non_max_suppression` does.
///
/// **`conf` is applied to both gates.** Upstream's `conf_thresh` is one number
/// used twice, and splitting it here would be sweeping a knob upstream does
/// not have.
fn candidates_at(blk: &[f32], conf: f32) -> Vec<Cand> {
    let mut out = Vec::new();
    for c in blk.as_chunks::<BLK_STRIDE>().0 {
        let objectness = c[4];
        if objectness < conf {
            continue;
        }
        // Which of the two channels wins sets the score; *which one it was* is
        // not carried, because nothing downstream may act on it —
        // see `suppress`.
        let class_score = c[5].max(c[6]);
        let score = objectness * class_score;
        if score < conf {
            continue;
        }
        let (cx, cy) = (f64::from(c[0]), f64::from(c[1]));
        let (w, h) = (f64::from(c[2]), f64::from(c[3]));
        out.push(Cand {
            x0: cx - w / 2.0,
            y0: cy - h / 2.0,
            x1: cx + w / 2.0,
            y1: cy + h / 2.0,
            score,
        });
    }
    out
}

/// Greedy non-maximum suppression, **class-agnostic**.
///
/// Written here because **the graph does not contain it**: there is no
/// `NonMaxSuppression` operator in the file, and upstream suppresses in Python
/// (`inference.py::postprocess_yolo`). That absence is also why `tract` runs
/// this model at all — the operators a YOLO export usually trips a pure-Rust
/// runtime on are the ones that come after the grid.
///
/// # Why agnostic, when upstream is not
///
/// YOLOv5's `non_max_suppression` takes an `agnostic` flag and upstream leaves
/// it `False`. **Following upstream here would be wrong**, and the reason is a
/// stage we never wrote:
///
/// Upstream does not stop at NMS. `inference.py` follows it with
/// `group_output(blks, lines, im_w, im_h, mask)`, which regroups the survivors
/// using the mask and line heads. Class-aware suppression is safe there because
/// something downstream tidies up after it. This port has no such stage, so
/// whatever NMS leaves is what gets scored.
///
/// A class-aware pass would leave duplicates. A text block whose grid cells
/// disagree about which of the two class channels wins would survive
/// **twice**, as two near-identical boxes.
///
/// A twin is not a second text block. Dropping class from the comparison is the
/// `agnostic=True` setting of an existing flag rather than a new mechanism or a
/// new constant, and it is what `group_output` would otherwise have had to
/// undo. **The class channel is not read anywhere after this**, which is why
/// `Cand` does not carry it: a field nothing may act on is one a later reader
/// would act on by mistake.
fn suppress(mut candidates: Vec<Cand>) -> Vec<Cand> {
    candidates.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut kept: Vec<Cand> = Vec::new();
    for candidate in candidates {
        if kept.iter().any(|k| iou(k, &candidate) > BLK_IOU) {
            continue;
        }
        kept.push(candidate);
    }
    kept
}

/// Square-space boxes as page fractions.
///
/// The live region — the square minus the right and bottom padding — *is* the
/// page, so a fraction of the live region is a fraction of the page and the
/// scan's own pixel size never enters. Boxes are clamped to the page: a grid
/// cell near the edge can decode to a box that runs slightly outside it.
///
/// **`axis` is derived from the box's own shape, not from the model's class
/// channel.** What the two classes mean is not established, and `score_page`
/// does not read `axis` — it scores geometry. Asserting a meaning for the class
/// channel here would be exactly the kind of unmeasured claim this sub-project
/// exists to stop.
fn to_regions(kept: &[Cand], dw: usize, dh: usize) -> Vec<Region> {
    let live_w = (SIDE - dw) as f64;
    let live_h = (SIDE - dh) as f64;
    kept.iter()
        .map(|c| {
            let x = (c.x0 / live_w).clamp(0.0, 1.0);
            let y = (c.y0 / live_h).clamp(0.0, 1.0);
            let w = (c.x1 / live_w).clamp(0.0, 1.0) - x;
            let h = (c.y1 / live_h).clamp(0.0, 1.0) - y;
            Region {
                x,
                y,
                w,
                h,
                axis: if h > w {
                    Axis::Vertical
                } else {
                    Axis::Horizontal
                },
            }
        })
        .filter(|r| r.w > 0.0 && r.h > 0.0)
        .collect()
}

/// The weights, from `TAIYAKU_MODEL`.
///
/// An env var rather than a path guessed from `CARGO_MANIFEST_DIR`, so that the
/// skip message can name exactly what is missing and how to get it.
/// The weights are not something to silently look for in three places.
fn model_path() -> Option<PathBuf> {
    let raw = std::env::var_os("TAIYAKU_MODEL")?;
    let path = PathBuf::from(raw);
    path.is_file().then_some(path)
}

/// Build the runnable graph once, for reuse across every page.
///
/// The input fact is fixed at `1×3×SIDE×SIDE`: the export carries no dynamic
/// axes and `tract` cannot resolve output facts without being told the shape.
fn load(path: &std::path::Path) -> TractResult<std::sync::Arc<TypedRunnableModel>> {
    tract_onnx::onnx()
        .model_for_path(path)?
        .with_input_fact(
            0,
            InferenceFact::dt_shape(f32::datum_type(), tvec!(1, 3, SIDE, SIDE)),
        )?
        .into_optimized()?
        .into_runnable()
}

/// Where to write overlays, from `TAIYAKU_OVERLAY_DIR`. `None` draws nothing.
///
/// A third variable rather than a flag on the other two, because drawing is not
/// part of the measurement: the numbers must be identical whether or not
/// anybody is looking. Unset in every gate run.
fn overlay_dir() -> Option<PathBuf> {
    let dir = PathBuf::from(std::env::var_os("TAIYAKU_OVERLAY_DIR")?);
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir)
}

/// One page with all three layers drawn over it, dimmed.
///
/// **Green is what a person drew, orange is `detect_page`, blue is the model.**
/// The point of putting all three on one page is the one thing the table cannot
/// show: *where* the splits and the one-to-one matches differ on the same art.
///
/// Dimming is not cosmetic — over full-contrast line art none of the three
/// colours is readable. Boxes flagged `ignore` are drawn hollow-thin, so a
/// reader can see that the scorer was told to skip them rather than wonder why
/// nothing matched.
#[allow(
    clippy::too_many_arguments,
    reason = "a drawing call, not an interface"
)]
fn draw_overlay(
    dir: &std::path::Path,
    work: &str,
    edition: &str,
    page_name: &str,
    luma: &GrayImage,
    truth: &[taiyaku_core::TruthRegion],
    classical: &[Region],
    model: &[Region],
) {
    let (w, h) = luma.dimensions();
    let mut canvas = image::RgbImage::new(w, h);
    for (x, y, px) in luma.enumerate_pixels() {
        let v = 128 + px.0[0] / 2; // halve the contrast, keep the structure
        canvas.put_pixel(x, y, image::Rgb([v, v, v]));
    }

    let scale = |r: (f64, f64, f64, f64)| {
        (
            r.0 * f64::from(w),
            r.1 * f64::from(h),
            (r.0 + r.2) * f64::from(w),
            (r.1 + r.3) * f64::from(h),
        )
    };

    for t in truth {
        let b = scale((t.x, t.y, t.w, t.h));
        let weight = if t.ignore { 1 } else { 3 };
        stroke(&mut canvas, b, image::Rgb([0, 150, 0]), weight);
    }
    for r in classical {
        stroke(
            &mut canvas,
            scale((r.x, r.y, r.w, r.h)),
            image::Rgb([235, 120, 0]),
            2,
        );
    }
    for r in model {
        stroke(
            &mut canvas,
            scale((r.x, r.y, r.w, r.h)),
            image::Rgb([0, 70, 225]),
            2,
        );
    }

    // Page names carry directories on the loose-image editions.
    let stem = page_name.replace(['/', '\\'], "_");
    let path = dir.join(format!("{work}-{edition}-{stem}.png"));
    canvas.save(&path).unwrap_or_else(|e| {
        panic!("{}: {e}", path.display());
    });
}

/// A hollow rectangle, `weight` pixels thick, clipped to the canvas.
fn stroke(img: &mut image::RgbImage, b: (f64, f64, f64, f64), colour: image::Rgb<u8>, weight: i64) {
    let (w, h) = (i64::from(img.width()), i64::from(img.height()));
    let (x0, y0) = (b.0.round() as i64, b.1.round() as i64);
    let (x1, y1) = (b.2.round() as i64, b.3.round() as i64);
    let mut put = |x: i64, y: i64| {
        if x >= 0 && y >= 0 && x < w && y < h {
            img.put_pixel(
                u32::try_from(x).expect("bounds checked"),
                u32::try_from(y).expect("bounds checked"),
                colour,
            );
        }
    };
    for t in 0..weight {
        for x in x0..=x1 {
            put(x, y0 + t);
            put(x, y1 - t);
        }
        for y in y0..=y1 {
            put(x0 + t, y);
            put(x1 - t, y);
        }
    }
}

/// One page through the model, as regions in page fractions.
///
/// Output order is the file's — `blk`, `seg`, `det`. Only `blk` is read. `seg`,
/// the full-resolution mask, recalls the same text at glyph granularity and
/// would score as `split` and `fragment` rather than `1:1`; `det` is the line
/// head, below the level L2 pairs on. Neither is decoded here — both are
/// described in `docs/notes.md` and neither is what the promotion rule counts.
fn detect_with_model(
    plan: &std::sync::Arc<TypedRunnableModel>,
    page: &GrayImage,
) -> TractResult<Vec<Region>> {
    let (blk, dw, dh) = block_head(plan, page)?;
    Ok(to_regions(&suppress(candidates(&blk)), dw, dh))
}

/// One page's raw `blk` tensor, beside the letterbox padding its boxes need.
///
/// Separate from `detect_with_model` for `where_the_confidence_floor_goes`,
/// which applies eight confidence floors to **one** forward pass: the inference
/// is the expensive part and it does not depend on the floor, so this is the
/// seam between what must run once per page and what may run once per threshold.
/// `detect_with_model` is this plus the default floor, so the shipped path and
/// the swept one cannot drift in the preprocessing.
fn block_head(
    plan: &std::sync::Arc<TypedRunnableModel>,
    page: &GrayImage,
) -> TractResult<(Vec<f32>, usize, usize)> {
    let (tensor, dw, dh) = letterbox(page);
    let outputs = plan.run(tvec!(tensor.into()))?;
    let blk: Vec<f32> = outputs[0]
        .to_plain_array_view::<f32>()?
        .iter()
        .copied()
        .collect();
    Ok((blk, dw, dh))
}

#[test]
fn the_model_candidate_scored() {
    let Some(root) = corpus() else {
        eprintln!("TAIYAKU_CORPUS unset — skipping");
        return;
    };
    let Some(weights) = model_path() else {
        eprintln!("TAIYAKU_MODEL unset or not a file — skipping; run ./scripts/fetch-model.sh");
        return;
    };

    let plan = load(&weights).unwrap_or_else(|e| panic!("{}: {e}", weights.display()));

    let mut classical = Report::default();
    let mut model = Report::default();
    let mut editions_seen = 0usize;

    // The pooled totals above cannot distinguish "the model found Japanese
    // dialogue" from "the model took every hit from the English editions and
    // none from the Japanese ones" — both produce the same pooled fraction.
    // A `Report` per candidate per edition is what tells those apart.
    let mut per_edition: Vec<(&str, &str, Report, Report)> = Vec::new();

    for (work, edition) in EDITIONS {
        let dir = root.join(work).join(edition);
        let sidecar = root.join(work).join("taiyaku.json");
        let paths: Vec<PathBuf> = members(&dir).into_iter().map(PathBuf::from).collect();
        let resolved = resolve_labelled(
            sidecar.to_str().expect("a corpus path is valid UTF-8"),
            &paths,
        )
        .unwrap_or_else(|e| panic!("{work}/{edition}: {e}"));

        // **Both detectors inside one decode**, which is `band_sweep.rs`'s
        // reason restated: `decode_all` bounds memory by decoding large pages
        // one at a time, so the pixels are not available afterwards, and a
        // second pass would pay for 45 more full-resolution decodes to score
        // the same pages twice.
        // Truth by page name, so the drawing closure can reach it. `truth` and
        // `pages` are parallel, but `decode_all`'s closure is handed a `&Page`
        // rather than an index. Names are unique within an edition — asserted
        // rather than assumed, because a collision would draw one page's boxes
        // over another's art and look plausible.
        let truth_by_name: std::collections::HashMap<&str, &Vec<taiyaku_core::TruthRegion>> =
            resolved
                .pages
                .iter()
                .map(|p| p.name.as_str())
                .zip(resolved.truth.iter())
                .collect();
        assert_eq!(
            truth_by_name.len(),
            resolved.pages.len(),
            "{work}/{edition}: page names are unique within an edition"
        );

        let detected = crate::fingerprint::decode_all(&resolved.pages, |page, luma| {
            let found = (
                detect::detect_page(luma),
                detect_with_model(&plan, luma).expect("forward pass"),
            );
            // Opt-in, and drawn from **this** run rather than reproduced
            // separately, so what a reader looks at is what was scored.
            if let Some(dir) = overlay_dir() {
                let truth = truth_by_name
                    .get(page.name.as_str())
                    .copied()
                    .expect("every decoded page is a labelled page");
                draw_overlay(
                    &dir, work, edition, &page.name, luma, truth, &found.0, &found.1,
                );
            }
            found
        })
        .unwrap_or_else(|e| panic!("{work}/{edition}: {e}"));

        let mut classical_ed = Report::default();
        let mut model_ed = Report::default();
        for (drawn, (found_classical, found_model)) in resolved.truth.iter().zip(&detected) {
            // One `score_page` call per detector per page: its `Scored` result
            // is folded into both the pooled report and this edition's report,
            // not recomputed.
            let scored_classical = score_page(drawn, found_classical, COVERAGE);
            let scored_model = score_page(drawn, found_model, COVERAGE);
            classical.add_page(drawn, &scored_classical);
            model.add_page(drawn, &scored_model);
            classical_ed.add_page(drawn, &scored_classical);
            model_ed.add_page(drawn, &scored_model);
        }

        println!("edition={work}/{edition} pages={}", detected.len());
        editions_seen += 1;
        assert_eq!(classical_ed.pages, 5, "{work}/{edition}: 5 labelled pages");
        assert_eq!(model_ed.pages, 5, "{work}/{edition}: 5 labelled pages");
        per_edition.push((work, edition, classical_ed, model_ed));
    }

    println!(
        "\n{:<12} {:>5} {:>7} {:>7} {:>7} {:>7} {:>9} {:>7}",
        "candidate", "1:1", "merged", "split", "frag", "missed", "spurious", "/page"
    );
    for (name, report) in [("detect_page", &classical), ("model/blk", &model)] {
        println!(
            "{name:<12} {:>5} {:>7} {:>7} {:>7} {:>7} {:>9} {:>7.2}",
            report.one_to_one,
            report.merged,
            report.split,
            report.fragment,
            report.missed,
            report.spurious,
            report.spurious_per_page(),
        );
    }

    // Per kind beside the total, because they are different results: the
    // per-kind split is the evidence spec §3.4 defers the balloon-interior
    // question to, and the model's whole claim is about one kind.
    println!("\nfound/drawn per kind, same runs:");
    for (name, report) in [("detect_page", &classical), ("model/blk", &model)] {
        let kinds: Vec<String> = report
            .by_kind
            .iter()
            .map(|(kind, tally)| format!("{kind:?}={}/{}", tally.found, tally.total))
            .collect();
        println!("{name:<12} {}", kinds.join(" "));
    }

    // Per edition, because the pooled figures above cannot tell "the model
    // found Japanese dialogue" from "every hit came from the English
    // editions" apart — see the module header. Grouped Japanese raw-mono
    // first, English second, so the four are readable at a glance against
    // the five rather than interleaved in `EDITIONS` order.
    println!("\nper edition (script, not page size, is what this checks):");
    println!(
        "{:<23} {:<12} {:>5} {:>7} {:>9} {:>7} {:>10}",
        "edition", "candidate", "1:1", "missed", "spurious", "/page", "dialogue"
    );
    let (japanese, english): (Vec<_>, Vec<_>) = per_edition
        .iter()
        .partition(|(_, edition, _, _)| *edition == "raw-mono");
    for (label, group) in [
        ("-- Japanese (raw-mono) --", &japanese),
        ("-- English --", &english),
    ] {
        println!("{label}");
        for (work, edition, classical_ed, model_ed) in group {
            for (name, report) in [("detect_page", classical_ed), ("model/blk", model_ed)] {
                let dialogue = report
                    .by_kind
                    .get(&Kind::Dialogue)
                    .copied()
                    .unwrap_or_default();
                println!(
                    "{:<23} {name:<12} {:>5} {:>7} {:>9} {:>7.2} {:>5}/{:<4}",
                    format!("{work}/{edition}"),
                    report.one_to_one,
                    report.missed,
                    report.spurious,
                    report.spurious_per_page(),
                    dialogue.found,
                    dialogue.total,
                );
            }
        }
    }

    // Assertions: that it measured something. Never a quality figure — those
    // move when a detector improves, and a check that fails on improvement is
    // not a measurement. Whether the model is promoted is read off the table
    // above against the rule, by a person.
    assert_eq!(editions_seen, EDITIONS.len(), "every named edition scored");
    assert_eq!(
        classical.pages, 45,
        "every labelled page scored classically"
    );
    assert_eq!(model.pages, 45, "every labelled page scored by the model");
    assert!(!model.by_kind.is_empty(), "some truth box was scored");
}

/// The two editions question two compares. Named rather than globbed, for
/// `EDITIONS`' reason: a name that stops matching is the silent-nothing failure
/// the assertions refuse to have.
const RAW: &str = "raw-mono";
const EN: &str = "en-mono";

/// A region's centre, in page fractions.
fn centroid(r: &Region) -> (f64, f64) {
    (r.x + r.w / 2.0, r.y + r.h / 2.0)
}

/// Whether a point in page fractions falls inside a region.
///
/// Closed on all four edges. A centroid landing exactly on a boundary is not a
/// case f64 arithmetic over decoded pixels produces in practice, and counting
/// it in is the answer that matches what a pairing lookup would do with it.
fn contains(r: &Region, (x, y): (f64, f64)) -> bool {
    x >= r.x && x <= r.x + r.w && y >= r.y && y <= r.y + r.h
}

/// For each of `of`'s centroids, how many of `boxes` contain it — bucketed
/// 0, 1, 2-or-more.
///
/// Buckets rather than a mean, because the three cases mean different things:
/// 0 is a region the other edition has nothing at, 1 is the pairing working,
/// and 2+ is one centroid inside two boxes at once.
fn containment(of: &[Region], boxes: &[Region]) -> [usize; 3] {
    let mut out = [0usize; 3];
    for region in of {
        let n = boxes
            .iter()
            .filter(|b| contains(b, centroid(region)))
            .count();
        out[n.min(2)] += 1;
    }
    out
}

/// How many of `boxes` hold two or more of `of`'s centroids.
///
/// **This is the headline, and it is not the 2+ bucket above read backwards.**
/// That bucket counts *centroids* sitting inside two boxes; this counts *boxes*
/// holding two centroids, which is the merge — one box drawn over what the
/// other edition got as two.
fn merging(boxes: &[Region], of: &[Region]) -> usize {
    boxes
        .iter()
        .filter(|b| of.iter().filter(|r| contains(b, centroid(r))).count() >= 2)
        .count()
}

/// Intersection over union of two regions, in page fractions.
///
/// A second IoU rather than a call to `iou` above, which is question one's and
/// takes `Cand` in square-space pixels. The two measure the same quantity in
/// different coordinate spaces, and reaching the first from here would mean
/// inventing a score for boxes that have none.
fn region_iou(a: &Region, b: &Region) -> f64 {
    let w = ((a.x + a.w).min(b.x + b.w) - a.x.max(b.x)).max(0.0);
    let h = ((a.y + a.h).min(b.y + b.h) - a.y.max(b.y)).max(0.0);
    let overlap = w * h;
    let union = a.w * a.h + b.w * b.h - overlap;
    if union <= 0.0 { 0.0 } else { overlap / union }
}

/// How many regions of one page have a near-identical twin on that same page.
///
/// **This is a standing check that twins do not decide how the headline
/// reads.** With class-aware suppression, a text block whose grid cells
/// disagreed about which of the two class channels won would survive
/// **twice**, as two near-identical boxes. Two centroids would then land
/// inside whatever the other edition drew over that one block, and
/// containment alone could not tell that from a grouping disagreement.
///
/// Suppression is class-agnostic (see `suppress`) and both counts are zero.
/// This function stays because a zero it prints is evidence, and because the
/// two headline figures agreeing is what says the artefact is gone rather than
/// merely smaller.
///
/// Counted at `BLK_IOU` — the model's own NMS threshold, so this is exactly the
/// set a class-blind suppression would have collapsed, not a number tuned here.
/// Both members of a pair are counted, because the question is how many of the
/// regions on the page are one of a twin.
fn duplicated(regions: &[Region]) -> usize {
    regions
        .iter()
        .enumerate()
        .filter(|(index, region)| {
            regions
                .iter()
                .enumerate()
                .any(|(other, o)| other != *index && region_iou(region, o) > BLK_IOU)
        })
        .count()
}

/// `merging`, discounting the twins `duplicated` counts.
///
/// A box counts only when two of the centroids it holds belong to regions that
/// are **not** near-duplicates of each other — that is, when the other edition
/// really did draw two things where this one drew one. This is the number the
/// question asks for; `merging` above is what containment alone can see.
fn merging_distinct(boxes: &[Region], of: &[Region]) -> usize {
    boxes
        .iter()
        .filter(|b| {
            let held: Vec<&Region> = of.iter().filter(|r| contains(b, centroid(r))).collect();
            held.iter().enumerate().any(|(index, a)| {
                held.iter()
                    .skip(index + 1)
                    .any(|c| region_iou(a, c) <= BLK_IOU)
            })
        })
        .count()
}

/// One work's worth of agreement, or every work's when they are folded together.
#[derive(Debug, Default, Clone, Copy)]
struct Agreement {
    compared: usize,
    unpaired: usize,
    spread: usize,
    raw_regions: usize,
    en_regions: usize,
    /// Raw centroids by how many English regions contain them: 0, 1, 2+.
    forward: [usize; 3],
    /// English centroids by how many raw regions contain them: 0, 1, 2+.
    reverse: [usize; 3],
    /// Raw regions holding two or more English centroids.
    raw_merges: usize,
    /// English regions holding two or more raw centroids.
    en_merges: usize,
    /// Of `raw_merges`, those whose held centroids are not all one region's
    /// twins — the grouping disagreements proper.
    raw_distinct: usize,
    /// Of `en_merges`, the same.
    en_distinct: usize,
    /// Raw regions that have a near-identical twin on their own page.
    raw_dupes: usize,
    /// English regions that have a near-identical twin on their own page.
    en_dupes: usize,
}

impl Agreement {
    fn fold(&mut self, other: &Self) {
        self.compared += other.compared;
        self.unpaired += other.unpaired;
        self.spread += other.spread;
        self.raw_regions += other.raw_regions;
        self.en_regions += other.en_regions;
        for i in 0..3 {
            self.forward[i] += other.forward[i];
            self.reverse[i] += other.reverse[i];
        }
        self.raw_merges += other.raw_merges;
        self.en_merges += other.en_merges;
        self.raw_distinct += other.raw_distinct;
        self.en_distinct += other.en_distinct;
        self.raw_dupes += other.raw_dupes;
        self.en_dupes += other.en_dupes;
    }
}

/// One edition's member paths, in the order the sidecar records them.
///
/// `docs/sidecar.md` rule 1 is normative — the edition's own `members` array
/// *is* the page order — and `read_members_in_order` does not sort, while
/// `corpus::members` returns whatever `read_dir` yielded. Matched by file name
/// here rather than trusted to arrive in agreement: every edition this test
/// reads happens to have exactly one member today, which is precisely the
/// condition under which an ordering bug would stay invisible until a
/// multi-member edition arrived.
fn ordered_members(edition: &taiyaku_core::Edition, dir: &std::path::Path) -> Vec<PathBuf> {
    let found: Vec<PathBuf> = members(dir).into_iter().map(PathBuf::from).collect();
    let ordered: Vec<PathBuf> = edition
        .members
        .iter()
        .map(|m| {
            let name = std::path::Path::new(&m.path)
                .file_name()
                .unwrap_or_else(|| panic!("sidecar member `{}` has no file name", m.path));
            found
                .iter()
                .find(|p| p.file_name() == Some(name))
                .unwrap_or_else(|| {
                    panic!("{}: nothing matching member `{}`", dir.display(), m.path)
                })
                .clone()
        })
        .collect();
    assert_eq!(
        ordered.len(),
        found.len(),
        "{}: the directory holds members the sidecar does not list",
        dir.display()
    );
    ordered
}

/// Run the model over the named pages of one edition.
///
/// `wanted` must be ascending and without repeats — the results come back in
/// page order, and the caller indexes into them by position. Returns each
/// page's regions beside its pixel dimensions, which the aspect delta needs and
/// which only the decode knows.
///
/// **Only the wanted pages are decoded**, for `resolve_labelled`'s reason:
/// `decode_all` decodes everything it is handed, and only five pages of an
/// edition are wanted here.
fn model_pages(
    plan: &std::sync::Arc<TypedRunnableModel>,
    doc: &taiyaku_core::Sidecar,
    root: &std::path::Path,
    work: &str,
    name: &str,
    wanted: &[u32],
) -> Vec<(Vec<Region>, (u32, u32))> {
    let dir = root.join(work).join(name);
    let edition = doc
        .editions
        .get(name)
        .unwrap_or_else(|| panic!("{work}: no edition `{name}`"));
    let all = taiyaku_read::read_members_in_order(&ordered_members(edition, &dir))
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()));
    assert_eq!(
        all.len(),
        edition.total_pages() as usize,
        "{}: the sidecar's page count and the files disagree, so every page \
         index below would address the wrong art",
        dir.display()
    );

    let subset: Vec<taiyaku_read::Page> = all
        .into_iter()
        .enumerate()
        .filter(|(index, _)| wanted.contains(&(*index as u32)))
        .map(|(_, page)| page)
        .collect();
    assert_eq!(
        subset.len(),
        wanted.len(),
        "{}: a wanted page is past the end of the edition",
        dir.display()
    );

    crate::fingerprint::decode_all(&subset, |_, luma| {
        (
            detect_with_model(plan, luma).expect("forward pass"),
            luma.dimensions(),
        )
    })
    .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
}

#[test]
fn the_models_grouping_across_both_editions() {
    let Some(root) = corpus() else {
        eprintln!("TAIYAKU_CORPUS unset — skipping");
        return;
    };
    let Some(weights) = model_path() else {
        eprintln!("TAIYAKU_MODEL unset or not a file — skipping; run ./scripts/fetch-model.sh");
        return;
    };

    let plan = load(&weights).unwrap_or_else(|e| panic!("{}: {e}", weights.display()));

    // Derived from `EDITIONS` rather than written out again, so the two lists
    // cannot drift into disagreeing about which works exist.
    let works: Vec<&str> = EDITIONS
        .iter()
        .filter(|(_, edition)| *edition == RAW)
        .map(|(work, _)| *work)
        .collect();
    assert_eq!(works.len(), 4, "four works carry a `{RAW}` edition");

    let mut total = Agreement::default();
    let mut per_work: Vec<(&str, Agreement)> = Vec::new();

    // `mRaw` and `mEn` read `total(distinct)`: the whole containment count, and
    // how many of those survive discounting the model's own twin detections.
    // `dup` is `raw/en` — how many regions on each page have such a twin.
    println!(
        "per page pair (fractions correspond only approximately — see the header):\n\
         {:<14} {:>4} {:>4} {:>6} {:>6} {:>7} {:>5} {:>4} {:>7} {:>10} {:>10} {:>7} {:>6}",
        "work",
        "raw",
        "en",
        "rawAR",
        "enAR",
        "delta",
        "rawR",
        "enR",
        "dup r/e",
        "fwd 0/1/2+",
        "rev 0/1/2+",
        "mRaw",
        "mEn"
    );

    for work in &works {
        let work = *work;
        let sidecar_path = root.join(work).join("taiyaku.json");
        let text = std::fs::read_to_string(&sidecar_path)
            .unwrap_or_else(|e| panic!("{}: {e}", sidecar_path.display()));
        let doc = taiyaku_core::Sidecar::from_json(&text)
            .unwrap_or_else(|e| panic!("{}: {e}", sidecar_path.display()));

        // The truth document decides **which** pages are compared and nothing
        // else: its boxes never enter this measurement. Read for its keys
        // rather than through `resolve_labelled`, which returns pages without
        // their page numbers — and a page number is exactly what
        // `counterpart` takes.
        let truth_path = taiyaku_core::TruthDoc::path_beside(&sidecar_path);
        let truth_text = std::fs::read_to_string(&truth_path)
            .unwrap_or_else(|e| panic!("{}: {e}", truth_path.display()));
        let truth = taiyaku_core::TruthDoc::from_json(&truth_text)
            .unwrap_or_else(|e| panic!("{}: {e}", truth_path.display()));
        let labelled: Vec<u32> = truth
            .editions
            .get(RAW)
            .unwrap_or_else(|| panic!("{}: no labels for `{RAW}`", truth_path.display()))
            .pages
            .keys()
            .copied()
            .collect();
        assert_eq!(labelled.len(), 5, "{work}/{RAW}: 5 labelled pages");

        let reading = doc.work.reading;
        let raw_slots = doc
            .editions
            .get(RAW)
            .unwrap_or_else(|| panic!("{work}: no edition `{RAW}`"))
            .slots(reading);
        let en_slots = doc
            .editions
            .get(EN)
            .unwrap_or_else(|| panic!("{work}: no edition `{EN}`"))
            .slots(reading);

        // The trap the header names, asserted rather than assumed. `counterpart`
        // is fed a labelled *page* number below, which is only a slot index
        // while this holds.
        for (index, slot) in raw_slots.iter().enumerate() {
            assert_eq!(
                (slot.page as usize, slot.half),
                (index, None),
                "{work}/{RAW}: slot {index} is not whole page {index} — a spread on the raw \
                 side puts every later page number one behind its slot index, and this \
                 measurement would compare the wrong pages while still printing a table"
            );
        }

        let edge = doc
            .edge_between(RAW, EN)
            .unwrap_or_else(|| panic!("{work}: no alignment relating `{RAW}` and `{EN}`"));
        let stored = doc.runs_of(edge).unwrap_or_else(|e| panic!("{work}: {e}"));
        // `edge_between` answers in whichever direction the document stores the
        // alignment, and `runs_of` indexes by that alignment's own `from` side.
        // Every sidecar in this corpus stores raw→en, so the second arm is
        // untaken today — leaving it out would make a document storing the edge
        // the other way resolve every counterpart backwards, in silence.
        let runs = if edge.from == RAW {
            stored
        } else {
            taiyaku_core::invert(&stored)
        };

        let mut here = Agreement::default();
        let mut pairs: Vec<(u32, u32)> = Vec::new();
        for &page in &labelled {
            let Some(slot) = taiyaku_core::counterpart(&runs, page) else {
                here.unpaired += 1;
                continue;
            };
            let slot = *en_slots.get(slot as usize).unwrap_or_else(|| {
                panic!("{work}: slot {slot} is past the end of `{EN}`");
            });
            if slot.half.is_some() {
                here.spread += 1;
                continue;
            }
            pairs.push((page, slot.page));
        }

        let raw_wanted: Vec<u32> = pairs.iter().map(|&(raw, _)| raw).collect();
        let mut en_wanted: Vec<u32> = pairs.iter().map(|&(_, en)| en).collect();
        en_wanted.sort_unstable();
        // The alignment is strictly 1:1 in slots, so two raw pages cannot claim
        // one whole English page. Asserted because `model_pages` indexes by
        // position and a repeat would silently shift every later lookup.
        let mut unique = en_wanted.clone();
        unique.dedup();
        assert_eq!(
            unique.len(),
            en_wanted.len(),
            "{work}: two raw pages resolved to one `{EN}` page"
        );

        let raw_found = model_pages(&plan, &doc, &root, work, RAW, &raw_wanted);
        let en_found = model_pages(&plan, &doc, &root, work, EN, &en_wanted);

        for &(raw_page, en_page) in &pairs {
            let raw_at = raw_wanted
                .iter()
                .position(|&p| p == raw_page)
                .expect("every paired raw page was asked for");
            let en_at = en_wanted
                .iter()
                .position(|&p| p == en_page)
                .expect("every paired English page was asked for");
            let (raw_regions, raw_dim) = &raw_found[raw_at];
            let (en_regions, en_dim) = &en_found[en_at];

            let aspect = |(w, h): (u32, u32)| f64::from(w) / f64::from(h);
            let forward = containment(raw_regions, en_regions);
            let reverse = containment(en_regions, raw_regions);
            let raw_merges = merging(raw_regions, en_regions);
            let en_merges = merging(en_regions, raw_regions);

            let raw_distinct = merging_distinct(raw_regions, en_regions);
            let en_distinct = merging_distinct(en_regions, raw_regions);
            let raw_dupes = duplicated(raw_regions);
            let en_dupes = duplicated(en_regions);

            println!(
                "{:<14} {:>4} {:>4} {:>6.3} {:>6.3} {:>+7.3} {:>5} {:>4} {:>7} {:>10} {:>10} \
                 {:>7} {:>6}",
                work,
                raw_page,
                en_page,
                aspect(*raw_dim),
                aspect(*en_dim),
                aspect(*en_dim) - aspect(*raw_dim),
                raw_regions.len(),
                en_regions.len(),
                format!("{raw_dupes}/{en_dupes}"),
                format!("{}/{}/{}", forward[0], forward[1], forward[2]),
                format!("{}/{}/{}", reverse[0], reverse[1], reverse[2]),
                format!("{raw_merges}({raw_distinct})"),
                format!("{en_merges}({en_distinct})"),
            );

            here.compared += 1;
            here.raw_regions += raw_regions.len();
            here.en_regions += en_regions.len();
            for i in 0..3 {
                here.forward[i] += forward[i];
                here.reverse[i] += reverse[i];
            }
            here.raw_merges += raw_merges;
            here.en_merges += en_merges;
            here.raw_distinct += raw_distinct;
            here.en_distinct += en_distinct;
            here.raw_dupes += raw_dupes;
            here.en_dupes += en_dupes;
        }

        total.fold(&here);
        per_work.push((work, here));
    }

    println!(
        "\nper work, and the total:\n{:<14} {:>8} {:>8} {:>6} {:>6} {:>5} {:>7} {:>12} {:>12} \
         {:>7} {:>6}",
        "work",
        "compared",
        "unpaired",
        "spread",
        "rawR",
        "enR",
        "dup r/e",
        "fwd 0/1/2+",
        "rev 0/1/2+",
        "mRaw",
        "mEn"
    );
    for (label, a) in per_work
        .iter()
        .map(|(work, a)| ((*work).to_string(), *a))
        .chain(std::iter::once(("TOTAL".to_string(), total)))
    {
        println!(
            "{:<14} {:>8} {:>8} {:>6} {:>6} {:>5} {:>7} {:>12} {:>12} {:>7} {:>6}",
            label,
            a.compared,
            a.unpaired,
            a.spread,
            a.raw_regions,
            a.en_regions,
            format!("{}/{}", a.raw_dupes, a.en_dupes),
            format!("{}/{}/{}", a.forward[0], a.forward[1], a.forward[2]),
            format!("{}/{}/{}", a.reverse[0], a.reverse[1], a.reverse[2]),
            format!("{}({})", a.raw_merges, a.raw_distinct),
            format!("{}({})", a.en_merges, a.en_distinct),
        );
    }

    println!(
        "\nforward: of {} raw regions, {} have their centre in no English region, \
         {} in exactly one, {} in two or more",
        total.raw_regions, total.forward[0], total.forward[1], total.forward[2]
    );
    println!(
        "reverse: of {} English regions, {} have their centre in no raw region, \
         {} in exactly one, {} in two or more",
        total.en_regions, total.reverse[0], total.reverse[1], total.reverse[2]
    );

    // The twin count comes before the headline, because it is what the headline
    // has to be read against — see `duplicated`.
    println!(
        "\ntwins: {} of {} raw regions and {} of {} English regions have a near-identical \
         twin on their own page, at the model's own NMS threshold of {BLK_IOU}. Both were \
         nonzero until suppression became class-agnostic — 0 of 147 and 40 of 161, measured \
         2026-08-12 — so a standing zero here is what says that fix still holds.",
        total.raw_dupes, total.raw_regions, total.en_dupes, total.en_regions
    );

    println!(
        "\nheadline: {} of {} raw regions hold two or more English centroids; {} of those \
         hold two that are not twins of each other — merged on the raw, split on the English",
        total.raw_merges, total.raw_regions, total.raw_distinct
    );
    println!(
        "          {} of {} English regions hold two or more raw centroids, {} of those \
         not twins — the mirror case",
        total.en_merges, total.en_regions, total.en_distinct
    );
    println!(
        "\n{} page pairs compared, {} skipped ({} unpaired, {} on a joined spread)",
        total.compared,
        total.unpaired + total.spread,
        total.unpaired,
        total.spread
    );

    // Assertions: that it measured something, and nothing about how well.
    // Whether this agreement rate is good enough is a person's call reading the
    // table above — a check that fails when the model improves is not a
    // measurement, it is a bar wearing one's clothes.
    assert_eq!(per_work.len(), works.len(), "every work was compared");
    assert!(total.compared > 0, "some page pair was compared");
    assert!(
        total.raw_regions > 0 && total.en_regions > 0,
        "the model found regions on both sides"
    );
}

#[test]
fn where_the_confidence_floor_goes() {
    let Some(root) = corpus() else {
        eprintln!("TAIYAKU_CORPUS unset — skipping");
        return;
    };
    let Some(weights) = model_path() else {
        eprintln!("TAIYAKU_MODEL unset or not a file — skipping; run ./scripts/fetch-model.sh");
        return;
    };

    let plan = load(&weights).unwrap_or_else(|e| panic!("{}: {e}", weights.display()));

    // One report per threshold, folded across all nine editions, because the
    // promotion rule is a sum over the whole dev set rather than a per-edition
    // verdict — `band_sweep.rs`'s array, for its reason.
    let mut reports: Vec<Report> = SWEEP.iter().map(|_| Report::default()).collect();
    // A second report per threshold over the four Japanese editions only. The
    // pooled row cannot tell "this floor bought recall" from "this floor bought
    // English recall and lost Japanese", and the Japanese pool is the whole
    // reason the model is a candidate — see the module header.
    let mut japanese: Vec<Report> = SWEEP.iter().map(|_| Report::default()).collect();
    let mut editions_seen = 0usize;

    for (work, edition) in EDITIONS {
        let dir = root.join(work).join(edition);
        let sidecar = root.join(work).join("taiyaku.json");
        let paths: Vec<PathBuf> = members(&dir).into_iter().map(PathBuf::from).collect();
        let resolved = resolve_labelled(
            sidecar.to_str().expect("a corpus path is valid UTF-8"),
            &paths,
        )
        .unwrap_or_else(|e| panic!("{work}/{edition}: {e}"));

        // **Every threshold is swept inside one forward pass**, which is the
        // whole point of the array here rather than a loop around `decode_all`:
        // the 1024×1024 inference is the costly part and `blk` does not depend
        // on the floor, so eight rows cost one pass plus eight cheap filters.
        // `band_sweep.rs`'s memory reason applies on top — `decode_all` decodes
        // the large pages one at a time and the pixels are gone afterwards.
        //
        // Nothing is shared between the rows below the tensor: each is its own
        // `candidates_at` + `suppress` + `to_regions`, so the rows are
        // independent measurements of the same inference rather than one run
        // edited.
        let detected = crate::fingerprint::decode_all(&resolved.pages, |_, luma| {
            let (blk, dw, dh) = block_head(&plan, luma).expect("forward pass");
            SWEEP.map(|conf| to_regions(&suppress(candidates_at(&blk, conf)), dw, dh))
        })
        .unwrap_or_else(|e| panic!("{work}/{edition}: {e}"));

        // `resolve_labelled` returns `truth` and `pages` parallel and ascending
        // and `decode_all` returns its results in page order, so zipping is
        // what `score::command` itself does — nothing is measured inside the
        // closure, so no lookup by name is needed.
        for (drawn, found) in resolved.truth.iter().zip(&detected) {
            for ((report, ja_report), regions) in
                reports.iter_mut().zip(japanese.iter_mut()).zip(found)
            {
                // One `score_page` call per threshold per page, folded into
                // both reports rather than computed twice.
                let scored = score_page(drawn, regions, COVERAGE);
                report.add_page(drawn, &scored);
                if edition == RAW {
                    ja_report.add_page(drawn, &scored);
                }
            }
        }

        println!("edition={work}/{edition} pages={}", detected.len());
        editions_seen += 1;
    }

    // `*` marks the incumbent so a reader finds it without counting rows, and
    // the losing rows are printed beside it — a sweep that showed only its
    // winner would be an assertion wearing a table's clothes.
    println!(
        "\n{:<7} {:>5} {:>7} {:>7} {:>7} {:>7} {:>9} {:>7} {:>12} {:>12}",
        "conf",
        "1:1",
        "merged",
        "split",
        "frag",
        "missed",
        "spurious",
        "/page",
        "dialogue",
        "ja-dialogue"
    );
    for ((conf, report), ja_report) in SWEEP.iter().zip(&reports).zip(&japanese) {
        let dialogue = report
            .by_kind
            .get(&Kind::Dialogue)
            .copied()
            .unwrap_or_default();
        let ja = ja_report
            .by_kind
            .get(&Kind::Dialogue)
            .copied()
            .unwrap_or_default();
        // Compared against the constant rather than a hard-coded 0.40, so the
        // marker follows `BLK_CONF` if it ever moves. Both are `f32` literals
        // from the same source text, so this is an identity check on a value
        // that was never computed, not a tolerance question.
        #[allow(
            clippy::float_cmp,
            reason = "marking the row whose literal is BLK_CONF, not comparing a computed float"
        )]
        let mark = if *conf == BLK_CONF { '*' } else { ' ' };
        println!(
            "{mark}{conf:<6.2} {:>5} {:>7} {:>7} {:>7} {:>7} {:>9} {:>7.2} {:>5}/{:<6} {:>5}/{:<6}",
            report.one_to_one,
            report.merged,
            report.split,
            report.fragment,
            report.missed,
            report.spurious,
            report.spurious_per_page(),
            dialogue.found,
            dialogue.total,
            ja.found,
            ja.total,
        );
    }
    println!(
        "\n* = BLK_CONF = {BLK_CONF}, the shipped incumbent, taken verbatim from upstream's \
         translation-pipeline defaults. `ja-dialogue` is the four Japanese `{RAW}` editions \
         only; `dialogue` is all nine. Every row holds NMS at BLK_IOU = {BLK_IOU}. The \
         promotion rule caps spurious at 10.7 per page — the dev set's own truth boxes per \
         page — and which row wins is read off this table against that, by a person."
    );

    // Assertions: that it measured something. Never a quality figure — those
    // move when the model or the floor improves, and a check that fails on
    // improvement is not a measurement. The floor this file exists to inform is
    // chosen by a person reading the table above, not here, and `BLK_CONF` is
    // deliberately not changed by this test.
    assert_eq!(editions_seen, EDITIONS.len(), "every named edition scored");
    assert!(
        SWEEP.contains(&BLK_CONF),
        "the shipped value is in the sweep as its control arm — without it the \
         table cannot report that lowering the floor did not help"
    );
    for ((conf, report), ja_report) in SWEEP.iter().zip(&reports).zip(&japanese) {
        assert_eq!(
            report.pages, 45,
            "every labelled page scored at conf={conf}"
        );
        assert_eq!(
            ja_report.pages, 20,
            "every Japanese labelled page scored at conf={conf}"
        );
        assert!(
            !report.by_kind.is_empty(),
            "some truth box was scored at conf={conf}"
        );
        assert!(
            !ja_report.by_kind.is_empty(),
            "some Japanese truth box was scored at conf={conf}"
        );
    }
}

#[cfg(test)]
mod letterbox_tests;

#[cfg(test)]
mod block_tests;
